//! Fixed-transaction PPS payout path, isolated from legacy wallet operations.
use super::pps_gate::PpsGate;
use anyhow::{Context, Result};
use node_rpc::{pczt, ZcashRpcClient};
use pool_db::{pps_funding::PpsFeeReservation, pps_policy::PpsPolicy, PoolDb};
use std::collections::BTreeMap;

/// There is one automatic payout loop. It awaits every wallet operation before
/// entering this path; a sealed unresolved PPS attempt blocks NEW PPS proposals.
/// Reconciliation may only settle already-known transactions, never broadcast.
pub(crate) async fn process(
    db: &PoolDb,
    wallet: &ZcashRpcClient,
    node: &ZcashRpcClient,
    from: &str,
    minimum: i64,
    network: &str,
    policy: &PpsPolicy,
    chain: &PpsGate,
) -> Result<usize> {
    policy.validate(network).map_err(anyhow::Error::msg)?;
    anyhow::ensure!(minimum > 0, "invalid PPS payout minimum");
    chain.fresh_lease().await?;
    db.verify_pps_epoch(&policy.epoch_config()).await?;
    db.pps_invariant().await?;
    // Unknown signer/extraction/broadcast outcomes retain both liability and
    // fees. Never select new notes while such an attempt exists.
    // Audit B17: one wallet, one in-flight send of ANY kind (PCZT, conventional or
    // legacy) — the same serialization gate as the conventional route.
    anyhow::ensure!(
        db.get_reserved_pps_attempts(None).await?.is_empty()
            && db.get_reserved_pps_conventional_attempts(100).await?.is_empty()
            && db.get_reserved_attempts(None).await?.is_empty(),
        "PPS or legacy payout unresolved; new proposals held"
    );
    let pending = db.get_pending_pps_payouts(minimum).await?;
    if pending.is_empty() {
        super::payout_health::note_no_payout_due();
        return Ok(0);
    }
    let epoch = policy.epoch_config();
    super::payout_health::note_funding_check();
    let funding =
        pool_core::pps_funding::collect_pps_funding(db, wallet, &epoch, from, node).await?;
    db.check_pps_funding(&funding).await?;
    let snapshot = db.pps_funding_snapshot().await?;
    let fee_available = snapshot
        .fee_allowance_zatoshis
        .checked_sub(snapshot.paid_fees_zatoshis)
        .and_then(|v| v.checked_sub(snapshot.reserved_fees_zatoshis))
        .context("PPS fee accounting invalid")?;
    anyhow::ensure!(fee_available > 0, "PPS fee allowance exhausted");
    let mut remaining = policy.max_payout_zatoshis;
    let mut items = Vec::new();
    let mut destinations: BTreeMap<String, i64> = BTreeMap::new();
    for payout in pending {
        // Audit B6: skip (and keep the balance of) a recipient this route cannot pay,
        // instead of failing the whole round for every other miner.
        if pczt::validate_pps_recipient(network, &payout.address).is_err() {
            tracing::warn!(miner_id = payout.miner_id,
                "PPS payout recipient unsupported by the PCZT route; skipped, balance retained");
            continue;
        }
        let amount = payout.amount.min(remaining);
        if amount < minimum {
            continue;
        }
        let destination = destinations.entry(payout.address).or_default();
        *destination = destination
            .checked_add(amount)
            .context("PPS amount overflow")?;
        items.push((payout.miner_id, amount));
        remaining -= amount;
        if remaining < minimum || destinations.len() >= 100 {
            break;
        }
    }
    if items.is_empty() {
        return Ok(0);
    }
    let amounts: Vec<_> = destinations.into_iter().collect();
    let tip = u32::try_from(
        node.get_block_count()
            .await
            .map_err(|_| anyhow::anyhow!("PPS chain tip unavailable"))?,
    )?;
    let proposal =
        pczt::prepare_pps_proposal(wallet, network, from, &amounts, fee_available, tip).await?;
    // Proving is performed before reserving funds or marking an irreversible
    // signer fence. Failure here queued no wallet spend and creates no attempt.
    let proposal = pczt::prove_pps_proposal(wallet, proposal).await?;
    let proposal_id = proposal.proposal_id();
    let fee = PpsFeeReservation {
        proposal_id: proposal_id.clone(),
        fee_zatoshis: proposal.fee_zatoshis(),
    };
    chain.fresh_lease().await?;
    let funding =
        pool_core::pps_funding::collect_pps_funding(db, wallet, &epoch, from, node).await?;
    let total = items
        .iter()
        .try_fold(0i64, |a, (_, v)| a.checked_add(*v))
        .context("PPS amount overflow")?;
    let attempt = db
        .create_payout_attempt(items.len() as i64, total, "pps-pczt")
        .await?;
    let reserved = match db.reserve_pps_payout(attempt, &items, &funding, &fee).await {
        Ok(v) => v,
        Err(_) => {
            let _ = db
                .update_payout_attempt(
                    attempt,
                    "failed",
                    None,
                    None,
                    Some("PPS exact reservation rejected; nothing signed"),
                )
                .await;
            anyhow::bail!("PPS exact reservation rejected; nothing signed");
        }
    };
    anyhow::ensure!(reserved == items, "PPS exact reservation mismatch; held");
    // Reserve changed the funding generation. Refresh once more before sealing;
    // an error here is definitively pre-sign and can release exact reservations.
    let pre_sign = async {
        chain.fresh_lease().await?;
        let funding =
            pool_core::pps_funding::collect_pps_funding(db, wallet, &epoch, from, node).await?;
        db.check_pps_funding(&funding).await?;
        Ok::<_, anyhow::Error>(funding)
    }
    .await;
    let funding = match pre_sign {
        Ok(v) => v,
        Err(_) => {
            if db.refund_pps_payout(attempt).await.is_ok() {
                let _ = db
                    .update_payout_attempt(
                        attempt,
                        "failed",
                        None,
                        None,
                        Some("PPS pre-sign gate rejected; nothing signed"),
                    )
                    .await;
            }
            anyhow::bail!("PPS pre-sign gate rejected");
        }
    };
    db.seal_pps_payout(attempt, &proposal_id).await?;
    // From this point ALL failures hold the fee and principal. No automatic
    // refunds, regenerated proposals, or sendmany fallback are possible.
    db.update_payout_attempt(attempt, "submitting", None, None, None)
        .await?;
    let transaction = pczt::sign_pps_proposal(wallet, proposal).await?;
    db.mark_pps_payout_signed(attempt, &proposal_id, transaction.txid())
        .await?;
    // Extraction marks inputs pending, so a NEW wallet balance read now would
    // double-count this reserved spend. Recheck the still-fresh pre-sign lease,
    // current DB generation and obligations immediately before broadcasting.
    chain.fresh_lease().await?;
    db.check_pps_funding(&funding).await?;
    db.update_payout_attempt(attempt, "sent", None, Some(transaction.txid()), None)
        .await?;
    transaction.broadcast(node).await?;
    // Audit B2: mempool visibility is not payment. The attempt stays reserved and
    // the reconciler settles it only once the transaction is PPS_SETTLE_MATURITY
    // confirmations deep, the same bar as the conventional route. Until then the
    // serialization gate above holds new proposals.
    Ok(0)
}
