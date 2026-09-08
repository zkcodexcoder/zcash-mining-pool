//! Explicit testnet-only conventional payouts. An async send is attempted once
//! after a durable seal. Reconciliation only observes and settles; it cannot
//! resend, replace, or refund a possibly submitted transaction.
use crate::pps_gate::PpsGate;
use anyhow::{Context, Result};
use node_rpc::{zecd_conventional::{ConventionalPayoutExpectation, TestnetConventionalProfile,
    verify_conventional_payout, verify_conventional_payout_with_wallet}, ZcashRpcClient};
use pool_core::pps_funding::{PpsFundingError, PpsFundingRoute, collect_pps_funding_for_route};
use pool_db::{PoolDb, pps_policy::PpsPolicy, pps_funding::{PpsConventionalIntent,
    PpsConventionalRecipient, PpsConventionalReservation, PpsFundingLease}};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, str::FromStr, time::Instant};

// The one dashboard's payout loop and independent reconciler must not cancel
// each other's pre-seal funding work. DB fences remain the cross-process guard.
static WORKFLOW:tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Canonical depth a payout tx must reach before it is settled paying->paid.
/// Settling at 1 confirmation (the prior behaviour) let a reorg un-mine an
/// already-"paid" payout with no reversal path, permanently shorting the miner
/// (audit finding #3). There is no paid->pending reversal; instead we simply
/// never mark paid until the tx is buried this deep in the canonical chain.
/// OPERATOR KNOB: 10 matches the wallet's 10-confirmation note-eligibility
/// policy and gives ~12.5 min finality; raise toward 24 (the chain-lease tip
/// spread) for more reorg margin. No depth covers an operator-induced
/// multi-thousand-block rollback -- that stays a manual reconciliation event.
pub(crate) const PPS_SETTLE_MATURITY: u64 = 10;

/// Classify typed failures only. Never format an error, its context, or any
/// supplied value: even an otherwise harmless database/RPC error can carry
/// addresses, identifiers, configuration or wallet material.
fn pre_send_error_category(error: &anyhow::Error) -> &'static str {
    use node_rpc::zecd_conventional::ConventionalError;
    use pool_core::pps_funding::PpsFundingError;
    use pool_db::pps_live::PpsDbError;
    if let Some(error) = error.downcast_ref::<PpsFundingError>() {
        return match error {
            PpsFundingError::RoutePolicy => "route_policy",
            PpsFundingError::UnsupportedRecipient => "unsupported_recipient",
            PpsFundingError::Timeout => "funding_timeout",
            PpsFundingError::WalletUnavailable => "wallet_unavailable",
            PpsFundingError::NetworkMismatch => "network_mismatch",
            PpsFundingError::FeeContractUnavailable => "fee_contract_unavailable",
            PpsFundingError::AccountingUnavailable => "accounting_unavailable",
            PpsFundingError::ConcurrentChange => "funding_changed",
            PpsFundingError::InsufficientFunding => "insufficient_funding",
            PpsFundingError::InvalidEvidence => "invalid_funding_evidence",
            PpsFundingError::IdentitySignerNotProven => "signer_unproven",
            PpsFundingError::FeeCapacityExhausted => "fee_capacity_exhausted",
        };
    }
    if let Some(error) = error.downcast_ref::<ConventionalError>() {
        return match error {
            ConventionalError::Profile => "unsupported_profile",
            ConventionalError::Payout => "unsupported_payout",
            ConventionalError::Transaction => "invalid_transaction",
            ConventionalError::Fee => "invalid_fee",
            ConventionalError::Recipient => "recipient_mismatch",
            ConventionalError::WalletHistory => "wallet_history_unproven",
        };
    }
    if let Some(error) = error.downcast_ref::<PpsDbError>() {
        return match error {
            PpsDbError::Invalid => "invalid_database_input",
            PpsDbError::EpochMismatch => "epoch_mismatch",
            PpsDbError::LegacyRecoveryRequired => "legacy_recovery_required",
            PpsDbError::DuplicateMismatch => "duplicate_mismatch",
            PpsDbError::ChainLeaseRequired => "chain_lease_required",
            PpsDbError::CapExceeded => "liability_cap_exceeded",
            PpsDbError::FundingLeaseRequired => "funding_lease_required",
            PpsDbError::FundingInsufficient => "insufficient_funding",
            PpsDbError::FeeBudgetExceeded => "fee_budget_exceeded",
            PpsDbError::PayoutHalted => "payout_halted",
            PpsDbError::Invariant => "accounting_invariant",
            PpsDbError::Database(_) => "database_unavailable",
        };
    }
    "unclassified"
}

/// This observes one existing operation, without retries, spawning, new reads,
/// changed deadlines or altered results. Stage arguments below are literals.
/// Success means this phase passed, never that a payout was sent or settled.
async fn pre_send_phase<T, E>(stage: &'static str,
    operation: impl Future<Output = std::result::Result<T, E>>) -> Result<T>
where E: Into<anyhow::Error>
{
    let started = Instant::now();
    let result = operation.await.map_err(Into::into);
    let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    match &result {
        Ok(_) => tracing::info!(stage, category = "passed", duration_ms,
            "PPS conventional pre-send phase"),
        Err(error) => tracing::warn!(stage, category = pre_send_error_category(error), duration_ms,
            "PPS conventional pre-send phase"),
    }
    result
}

/// Amounts never pass through binary floating point. node-rpc enables serde's
/// arbitrary_precision feature for this workspace's RPC JSON numbers.
fn exact_amount(zats: i64) -> Result<Value> {
    anyhow::ensure!((1..=950_000_000).contains(&zats), "invalid testnet PPS amount");
    Ok(Value::Number(serde_json::Number::from_str(
        &format!("{}.{:08}", zats / 100_000_000, zats % 100_000_000))?))
}

fn amounts(intent: &PpsConventionalIntent) -> Result<Vec<(String, i64)>> {
    let mut values = BTreeMap::<String, i64>::new();
    for item in &intent.items {
        let value = values.entry(item.address.clone()).or_default();
        *value = value.checked_add(item.amount_zatoshis).context("PPS amount overflow")?;
    }
    Ok(values.into_iter().collect())
}

pub(super) fn encode_recipients(intent: &PpsConventionalIntent) -> Result<Vec<Value>> {
    amounts(intent)?.into_iter().map(|(address,amount)|
        Ok(json!({"address":address,"amount":exact_amount(amount)?})))
        .collect()
}

/// Only shorten the original proof's lifetime. The funded seal's unchanged
/// entry/precommit clock checks then enforce BOTH wallet and chain expiry.
fn intersect_seal_deadline(funding: &mut PpsFundingLease, chain_expiry: i64) {
    funding.valid_until_unix = funding.valid_until_unix.min(chain_expiry);
}

/// Every failure before a successful seal attempts the existing atomic refund.
/// A committed or ambiguously committed seal makes that refund fail closed.
/// The one-shot wallet send must stay outside this cleanup boundary.
pub(super) async fn pre_send_or_release<T>(db: &PoolDb, attempt: i64,
    preparation: impl Future<Output = Result<T>>) -> Result<T>
{
    match preparation.await {
        Ok(prepared) => Ok(prepared),
        Err(_) => {
            pre_send_phase("unsealed_refund", db.refund_pps_payout(attempt)).await?;
            anyhow::bail!("PPS pre-send gate rejected");
        }
    }
}

fn expectation(intent: &PpsConventionalIntent) -> Result<ConventionalPayoutExpectation> {
    let profile = TestnetConventionalProfile::consensus_size_bound(&intent.network,
        usize::from(intent.max_recipients))?;
    anyhow::ensure!(intent.profile == profile.identifier(), "PPS intent profile mismatch");
    Ok(ConventionalPayoutExpectation::new(&profile, &intent.source, &amounts(intent)?,
        u32::try_from(intent.target_height)?)?)
}

async fn rpc(client: &ZcashRpcClient, method: &'static str, params: Value) -> Result<Value> {
    client.zecd_conventional_rpc(method, params).await
        .map_err(|_| anyhow::anyhow!("testnet PPS RPC unavailable or invalid"))
}

fn valid_opid(opid: &str) -> bool {
    (1..=128).contains(&opid.len())
        && opid.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// The pinned sender wallet supplies the shielded output view, but never its
/// own inclusion authority. Bind that view to the exact bytes and confirmed
/// canonical block independently fetched from the node. Missing or mismatched
/// wallet evidence is uncertainty, not a reason to resend or refund.
fn bind_wallet_history(wallet: &Value, txid: &str, raw_hex: &str, blockhash: &str) -> Result<()> {
    let wallet_txid = wallet.get("txid").and_then(Value::as_str)
        .context("PPS wallet transaction identity missing; held")?;
    let wallet_hex = wallet.get("hex").and_then(Value::as_str)
        .context("PPS wallet transaction bytes missing; held")?;
    let wallet_block = wallet.get("blockhash").and_then(Value::as_str)
        .context("PPS wallet transaction block missing; held")?;
    anyhow::ensure!(wallet_txid == txid
        && crate::wallet_operation::normalize_txid(wallet_txid)? == wallet_txid
        && !raw_hex.is_empty() && wallet_hex == raw_hex
        && wallet.get("confirmations").and_then(Value::as_u64).is_some_and(|n|n >= 1)
        && wallet_block == blockhash
        && crate::wallet_operation::normalize_txid(wallet_block)? == wallet_block,
        "PPS wallet transaction canonical binding unproven; held");
    Ok(())
}

fn wallet_history_ready(info: &Value, payout_height: u64) -> Result<()> {
    anyhow::ensure!(info.get("scanning").and_then(Value::as_bool) == Some(false)
        && info.get("enhanced_through").and_then(Value::as_u64)
            .is_some_and(|height|height >= payout_height),
        "PPS wallet enhancement incomplete for confirmed payout; held");
    Ok(())
}

async fn wallet_idle(wallet: &ZcashRpcClient) -> Result<()> {
    let state = rpc(wallet, "z_getoperationstatus", json!([])).await?;
    let operations = state.as_array().context("PPS wallet operation schema")?;
    anyhow::ensure!(operations.len() <= 4096 && operations.iter().all(|op|
        matches!(op.get("status").and_then(Value::as_str), Some("success" | "failed" | "cancelled"))),
        "PPS wallet has an outstanding or unknown operation");
    Ok(())
}

pub(crate) async fn process(db: &PoolDb, wallet: &ZcashRpcClient, node: &ZcashRpcClient,
    from: &str, minimum: i64, policy: &PpsPolicy, chain: &PpsGate,
    route: &PpsFundingRoute) -> Result<usize>
{
    let _workflow=WORKFLOW.lock().await;
    // Never return raw RPC data, addresses, or SQL errors to payout-health logs.
    process_inner(db,wallet,node,from,minimum,policy,chain,route).await
        .map_err(|_| anyhow::anyhow!("testnet PPS payout held; funding, wallet or transaction gate failed"))
}

/// zecd flips not-ready after almost every block, and a block arriving during
/// the ~40s multi-RPC funding read bumps the funding generation, so a single
/// fresh collection frequently fails transiently on this reorgy testnet -- the
/// reason payouts almost never completed (they reserved, then the pre-send
/// funding recheck failed and refunded). Retry the READ-ONLY collection a
/// bounded number of times on transient categories before giving up the round.
/// Real rejections (insolvency, wrong network, unproven signer, bad evidence,
/// route/recipient) fail immediately. This moves no money: the resulting lease
/// is still validated at reserve and at the seal under the writer lock.
async fn collect_funding_resilient(db: &PoolDb, wallet: &ZcashRpcClient, policy: &PpsPolicy,
    from: &str, node: &ZcashRpcClient, route: &PpsFundingRoute)
    -> std::result::Result<PpsFundingLease, PpsFundingError>
{
    let mut attempt = 0u32;
    loop {
        match collect_pps_funding_for_route(db, wallet, &policy.epoch_config(), from, node, route).await {
            Ok(lease) => return Ok(lease),
            Err(error) => {
                let transient = matches!(error,
                    PpsFundingError::Timeout | PpsFundingError::WalletUnavailable
                    | PpsFundingError::ConcurrentChange | PpsFundingError::AccountingUnavailable);
                if !transient || attempt >= 4 { return Err(error); }
                attempt += 1;
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    }
}

async fn process_inner(db: &PoolDb, wallet: &ZcashRpcClient, node: &ZcashRpcClient,
    from: &str, minimum: i64, policy: &PpsPolicy, chain: &PpsGate,
    route: &PpsFundingRoute) -> Result<usize>
{
    pre_send_phase("policy", async { policy.validate("testnet").map_err(anyhow::Error::msg) }).await?;
    pre_send_phase("route", async {
        route.validate(&policy.epoch_config())?;
        anyhow::ensure!(route.holds_new_legacy_sends() && minimum > 0, "PPS route mismatch");
        Ok::<_,anyhow::Error>(())
    }).await?;
    pre_send_phase("initial_chain", chain.fresh_lease()).await?;
    pre_send_phase("epoch", db.verify_pps_epoch(&policy.epoch_config())).await?;
    pre_send_phase("accounting_invariant", db.pps_invariant()).await?;
    pre_send_phase("unresolved_attempts", async {
        anyhow::ensure!(db.get_reserved_pps_attempts(None).await?.is_empty()
            && db.get_reserved_pps_conventional_attempts(100).await?.is_empty()
            && db.get_reserved_attempts(None).await?.is_empty(),
            "PPS or legacy payout unresolved; new sends held");
        Ok::<_,anyhow::Error>(())
    }).await?;
    let pending = pre_send_phase("pending_claims", db.get_pending_pps_payouts(minimum)).await?;
    if pending.is_empty() {
        super::payout_health::note_no_payout_due();
        return Ok(0);
    }
    pre_send_phase("initial_wallet_idle", wallet_idle(wallet)).await?;
    let mut selected = pre_send_phase("recipient_selection", async {
        let mut remaining = policy.max_payout_zatoshis;
        let mut selected = Vec::new();
        for p in pending {
            route.validate_recipient("testnet", &p.address)?;
            let amount = p.amount.min(remaining);
            if amount < minimum { continue; }
            selected.push(PpsConventionalRecipient { miner_id:p.miner_id,
                address:p.address, amount_zatoshis:amount });
            remaining -= amount;
            if selected.len() == 100 || remaining < minimum { break; }
        }
        Ok::<_,anyhow::Error>(selected)
    }).await?;
    if selected.is_empty() { return Ok(0); }
    selected.sort_by_key(|p|p.miner_id);
    let tip = pre_send_phase("node_tip", async {
        node.get_block_count().await.map_err(|_|anyhow::anyhow!("PPS node tip unavailable"))
    }).await?;
    let intent = pre_send_phase("intent_construction", async {
        Ok::<_,anyhow::Error>(PpsConventionalIntent { version:1,network:"testnet".into(),epoch:policy.epoch.clone(),
            target_height:tip.checked_add(1).context("PPS target overflow")?,source:from.into(),
            profile:"consensus-size-v1".into(),max_recipients:100,items:selected })
    }).await?;
    let _verified = pre_send_phase("intent_expectation", async { expectation(&intent) }).await?;
    let bound = pre_send_phase("reservation_intent", async {
        PpsConventionalReservation::from_intent(intent.clone())
    }).await?;
    super::payout_health::note_funding_check();
    let funding = pre_send_phase("funding_before_reserve",
        collect_funding_resilient(db,wallet,policy,from,node,route)).await?;
    let items:Vec<_> = intent.items.iter().map(|p|(p.miner_id,p.amount_zatoshis)).collect();
    let total = pre_send_phase("claim_total", async {
        items.iter().try_fold(0_i64,|sum,(_,v)|sum.checked_add(*v)).context("PPS total overflow")
    }).await?;
    let attempt = pre_send_phase("attempt_create",
        db.create_payout_attempt(items.len() as i64,total,"pps-conventional-testnet")).await?;
    if pre_send_phase("reserve", db.reserve_pps_conventional_payout(attempt,&items,&funding,&bound))
        .await.is_err()
    {
        let _=db.update_payout_attempt(attempt,"failed",None,None,Some("PPS reservation rejected before send")).await;
        anyhow::bail!("PPS exact reservation rejected");
    }
    let recipients = pre_send_or_release(db,attempt,async {
        pre_send_phase("chain_before_send", chain.fresh_lease()).await?;
        pre_send_phase("wallet_idle_before_send", wallet_idle(wallet)).await?;
        let mut funding=pre_send_phase("funding_before_send",
            collect_funding_resilient(db,wallet,policy,from,node,route)).await?;
        pre_send_phase("funding_recheck", db.check_pps_funding(&funding)).await?;
        let recipients=pre_send_phase("recipient_encoding", async { encode_recipients(&intent) }).await?;
        let chain_guard=pre_send_phase("chain_before_seal", chain.valid_cached_lease()).await?;
        intersect_seal_deadline(&mut funding,chain_guard.valid_until_unix());
        pre_send_phase("seal", db.seal_pps_conventional_payout_funded(attempt,&bound.intent_id,&funding)).await?;
        // Keep the same cache mutex held until the funded seal has returned.
        drop(chain_guard);
        Ok(recipients)
    }).await?;
    // No fallback and no transport retry. Even a reported RPC failure can
    // follow submission. All errors after this one-shot fence retain funds.
    let sent=rpc(wallet,"z_sendmany",json!([from,recipients,10,null,"AllowRevealedRecipients"])).await?;
    let opid=sent.as_str().filter(|v|valid_opid(v)).context("PPS operation ID invalid")?;
    db.record_pps_conventional_operation(attempt,&bound.intent_id,opid).await?;
    reconcile_inner(db,wallet,node,policy,chain,attempt).await
}

/// Returns recipient count only after canonical block inclusion and exact raw
/// payout verification. Unknown, failed, expired or missing operations remain
/// held; this function has no send or proposal-generation path.
pub(crate) async fn reconcile_one(db:&PoolDb,wallet:&ZcashRpcClient,node:&ZcashRpcClient,
    policy:&PpsPolicy,chain:&PpsGate,attempt:i64) -> Result<usize>
{
    let _workflow=WORKFLOW.lock().await;
    reconcile_inner(db,wallet,node,policy,chain,attempt).await
        .map_err(|_|anyhow::anyhow!("testnet PPS reconciliation held; exact outcome unproven"))
}

async fn reconcile_inner(db:&PoolDb,wallet:&ZcashRpcClient,node:&ZcashRpcClient,
    policy:&PpsPolicy,chain:&PpsGate,attempt:i64) -> Result<usize>
{
    policy.validate("testnet").map_err(anyhow::Error::msg)?;
    chain.fresh_lease().await?;
    db.verify_pps_epoch(&policy.epoch_config()).await?;
    let row=db.get_pps_conventional_attempt(attempt).await?.context("PPS attempt missing")?;
    let intent=row.intent()?.context("PPS historical intent missing; held")?;
    anyhow::ensure!(intent.epoch==policy.epoch && intent.network==policy.network,"PPS epoch binding mismatch");
    let expected=expectation(&intent)?;
    if matches!(row.status.as_str(),"paid"|"released") { return Ok(0); }
    if !row.sealed {
        db.refund_pps_payout(attempt).await?;
        return Ok(0);
    }
    let opid=row.operation_id.as_deref().filter(|v|valid_opid(v)).context("PPS lost operation; held")?;
    let txid=match row.expected_txid {
        Some(ref t)=>crate::wallet_operation::normalize_txid(t)?,
        None=>{
            let response=rpc(wallet,"z_getoperationstatus",json!([[opid]])).await?;
            let statuses=response.as_array().context("PPS operation schema")?;
            anyhow::ensure!(statuses.len()==1 && statuses[0].get("id").and_then(Value::as_str)==Some(opid),
                "PPS operation identity mismatch");
            match statuses[0].get("status").and_then(Value::as_str) {
                Some("success")=>{},
                Some("queued"|"executing")=>return Ok(0),
                _=>anyhow::bail!("PPS operation failed or unknown; submitted funds remain held"),
            }
            let txid=crate::wallet_operation::successful_txid(statuses[0].get("result"))?;
            db.record_pps_conventional_transaction(attempt,&row.intent_id,opid,&txid).await?;
            txid
        }
    };
    let raw=rpc(node,"getrawtransaction",json!([txid,1])).await?;
    // Stay reserved (soft return, retried next cycle) until the tx is buried
    // PPS_SETTLE_MATURITY-deep in our node's active chain; only then settle.
    if raw.get("confirmations").and_then(Value::as_u64).unwrap_or(0)<PPS_SETTLE_MATURITY { return Ok(0); }
    let raw_txid=raw.get("txid").and_then(Value::as_str).context("PPS raw txid missing")?;
    anyhow::ensure!(crate::wallet_operation::normalize_txid(raw_txid)?==txid,"PPS raw txid mismatch");
    let blockhash=raw.get("blockhash").and_then(Value::as_str).context("PPS block missing")?;
    crate::wallet_operation::normalize_txid(blockhash)?;
    let block=rpc(node,"getblock",json!([blockhash,1])).await?;
    let height=block.get("height").and_then(Value::as_u64).context("PPS height missing")?;
    anyhow::ensure!(block.get("hash").and_then(Value::as_str)==Some(blockhash)
        && block.get("confirmations").and_then(Value::as_u64).unwrap_or(0)>=1
        && block.get("tx").and_then(Value::as_array).is_some_and(|txs|
            txs.iter().filter(|t|t.as_str()==Some(txid.as_str())).count()==1),
        "PPS transaction inclusion unproven");
    let canonical=rpc(node,"getblockhash",json!([height])).await?;
    anyhow::ensure!(canonical.as_str()==Some(blockhash),"PPS block no longer canonical");
    let hex=raw.get("hex").and_then(Value::as_str).context("PPS raw bytes missing")?;
    // Canonical raw policy breaches must reach the durable HALT even when the
    // wallet is unavailable. For shielded intents this raw-only API returns
    // WalletHistory only after all independent raw checks have passed.
    let outcome=match verify_conventional_payout(hex,&txid,&expected) {
        Err(node_rpc::zecd_conventional::ConventionalError::WalletHistory)
            if expected.requires_wallet_history() => {
            // Completion through this payout's block is sufficient; reconciliation
            // needs no unlocked signer and does not require catching a moving tip.
            let info=rpc(wallet,"getwalletinfo",json!([])).await?;
            wallet_history_ready(&info,height)?;
            let history=rpc(wallet,"gettransaction",json!([txid])).await?;
            bind_wallet_history(&history,&txid,hex,blockhash)?;
            // This checks the pinned wallet's complete non-change output view;
            // it does not independently decrypt shielded transaction outputs.
            verify_conventional_payout_with_wallet(hex,&txid,&expected,&history)
        }
        raw_outcome => raw_outcome,
    };
    let verified=match outcome {
        Ok(v)=>v,
        Err(e)=>{
            use node_rpc::zecd_conventional::ConventionalError;
            use pool_db::pps_funding::PpsConventionalHalt;
            let category=match e {
                ConventionalError::Fee=>PpsConventionalHalt::FeeMismatch,
                ConventionalError::Recipient=>PpsConventionalHalt::RecipientMismatch,
                // Invalid raw data and any incomplete/mismatching wallet view
                // are not proven spending-policy violations. Future error
                // variants also default to HOLD, never refund or resend.
                _=>anyhow::bail!("PPS canonical raw transaction unproven; held"),
            };
            db.halt_pps_conventional_payout(attempt,category).await?;
            anyhow::bail!("PPS canonical transaction violated wallet contract; financial halt");
        }
    };
    chain.fresh_lease().await?;
    let canonical=rpc(node,"getblockhash",json!([height])).await?;
    anyhow::ensure!(canonical.as_str()==Some(blockhash),"PPS block changed during verification");
    let settled=db.confirm_pps_conventional_payout(attempt,verified.txid(),verified.actual_fee_zatoshis()).await?;
    Ok(usize::try_from(settled)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_deadline_intersection_never_extends_or_changes_other_funding_fields() {
        let original=PpsFundingLease { network:"testnet".into(),checked_at_unix:100,
            valid_until_unix:160,spendable_zatoshis:1_000_000_000,
            reserve_floor_zatoshis:1,reserved_fee_allowance_zatoshis:50_000_000,generation:17 };
        for (chain_expiry,expected) in [(190,160),(160,160),(140,140),(100,100),(99,99)] {
            let mut bounded=original.clone();
            intersect_seal_deadline(&mut bounded,chain_expiry);
            assert_eq!(bounded.valid_until_unix,expected);
            bounded.valid_until_unix=original.valid_until_unix;
            assert!(bounded==original);
        }
    }

    #[test]
    fn pre_send_categories_use_types_not_error_text_or_context() {
        use node_rpc::zecd_conventional::ConventionalError as C;
        use pool_core::pps_funding::PpsFundingError as F;
        use pool_db::pps_live::PpsDbError as D;
        for (error, expected) in [
            (F::RoutePolicy,"route_policy"), (F::UnsupportedRecipient,"unsupported_recipient"),
            (F::Timeout,"funding_timeout"), (F::WalletUnavailable,"wallet_unavailable"),
            (F::NetworkMismatch,"network_mismatch"), (F::FeeContractUnavailable,"fee_contract_unavailable"),
            (F::AccountingUnavailable,"accounting_unavailable"), (F::ConcurrentChange,"funding_changed"),
            (F::InsufficientFunding,"insufficient_funding"), (F::InvalidEvidence,"invalid_funding_evidence"),
            (F::IdentitySignerNotProven,"signer_unproven"), (F::FeeCapacityExhausted,"fee_capacity_exhausted"),
        ] {
            let wrapped=anyhow::Error::new(error).context("SYNTHETIC_PRIVATE_CONTEXT");
            assert_eq!(pre_send_error_category(&wrapped),expected);
        }
        for (error, expected) in [(C::Profile,"unsupported_profile"), (C::Payout,"unsupported_payout"),
            (C::Transaction,"invalid_transaction"), (C::Fee,"invalid_fee"),
            (C::Recipient,"recipient_mismatch"), (C::WalletHistory,"wallet_history_unproven")]
        { assert_eq!(pre_send_error_category(&error.into()),expected); }
        for (error, expected) in [(D::Invalid,"invalid_database_input"), (D::EpochMismatch,"epoch_mismatch"),
            (D::LegacyRecoveryRequired,"legacy_recovery_required"), (D::DuplicateMismatch,"duplicate_mismatch"),
            (D::ChainLeaseRequired,"chain_lease_required"), (D::CapExceeded,"liability_cap_exceeded"),
            (D::FundingLeaseRequired,"funding_lease_required"), (D::FundingInsufficient,"insufficient_funding"),
            (D::FeeBudgetExceeded,"fee_budget_exceeded"), (D::Invariant,"accounting_invariant"),
            (D::Database(sqlx::Error::Protocol("SYNTHETIC_PRIVATE_SQL".into())),"database_unavailable")]
        { assert_eq!(pre_send_error_category(&error.into()),expected); }
        // Similar text is not authority to classify an untyped error.
        assert_eq!(pre_send_error_category(&anyhow::anyhow!("funding_timeout SYNTHETIC_PRIVATE_VALUE")),
            "unclassified");
    }

    #[test]
    fn pre_send_phase_logs_only_fixed_metadata_and_preserves_results_and_order() {
        use std::{fmt, io::Write, sync::{Arc,Mutex}};
        #[derive(Clone)]
        struct Buffer(Arc<Mutex<Vec<u8>>>);
        impl Write for Buffer {
            fn write(&mut self, bytes:&[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
        }
        struct UnprintableError;
        impl fmt::Debug for UnprintableError {
            fn fmt(&self,_:&mut fmt::Formatter<'_>) -> fmt::Result { panic!("error debug forbidden") }
        }
        impl fmt::Display for UnprintableError {
            fn fmt(&self,_:&mut fmt::Formatter<'_>) -> fmt::Result { panic!("error display forbidden") }
        }
        impl std::error::Error for UnprintableError {}

        let output=Arc::new(Mutex::new(Vec::new()));
        let writer=Buffer(output.clone());
        let subscriber=tracing_subscriber::fmt().without_time().with_ansi(false).with_target(false)
            .with_writer(move || writer.clone()).finish();
        tracing::subscriber::with_default(subscriber,|| {
            tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(async {
                let mut calls=Vec::new();
                let passed=pre_send_phase("intent_expectation",async {
                    calls.push("first");
                    Ok::<_,anyhow::Error>("SYNTHETIC_PRIVATE_RESULT")
                }).await.unwrap();
                assert_eq!(passed,"SYNTHETIC_PRIVATE_RESULT");
                let held:Result<()> = pre_send_phase("funding_before_reserve",async {
                    calls.push("second");
                    Err(anyhow::Error::new(pool_core::pps_funding::PpsFundingError::ConcurrentChange)
                        .context("SYNTHETIC_PRIVATE_CONTEXT"))
                }).await;
                assert_eq!(held.unwrap_err().downcast_ref::<pool_core::pps_funding::PpsFundingError>(),
                    Some(&pool_core::pps_funding::PpsFundingError::ConcurrentChange));
                let unprintable:Result<()> = pre_send_phase("reserve",async {
                    calls.push("third");
                    Err(UnprintableError)
                }).await;
                assert!(unprintable.unwrap_err().is::<UnprintableError>());
                assert_eq!(calls,["first","second","third"]);
            });
        });
        let text=String::from_utf8(output.lock().unwrap().clone()).unwrap();
        let lines:Vec<_>=text.lines().collect();
        assert_eq!(lines.len(),3);
        for (line,(stage,category)) in lines.iter().zip([
            ("intent_expectation","passed"), ("funding_before_reserve","funding_changed"),
            ("reserve","unclassified")])
        {
            assert!(line.contains("PPS conventional pre-send phase"));
            assert!(line.contains(stage) && line.contains(category));
            assert_eq!(line.matches('=').count(),3);
            for field in ["stage=","category=","duration_ms="] { assert!(line.contains(field)); }
        }
        assert!(!text.contains("SYNTHETIC_PRIVATE"));
    }

    #[tokio::test]
    async fn pre_send_phase_failure_keeps_existing_short_circuit() {
        let mut reached_next=false;
        let outcome:Result<()> = async {
            pre_send_phase("intent_expectation",async {
                Err::<(),_>(node_rpc::zecd_conventional::ConventionalError::Recipient)
            }).await?;
            reached_next=true;
            Ok(())
        }.await;
        assert!(outcome.unwrap_err().is::<node_rpc::zecd_conventional::ConventionalError>());
        assert!(!reached_next);
    }

    #[test]
    fn exact_amounts_and_operation_identifiers() {
        for (v,s) in [(1,"0.00000001"),(99_999_999,"0.99999999"),(950_000_000,"9.50000000")] {
            assert_eq!(exact_amount(v).unwrap().to_string(),s);
        }
        for v in [0,-1,950_000_001,i64::MAX] { assert!(exact_amount(v).is_err()); }
        assert!(valid_opid("opid-1234-abcd"));
        for s in ["","with space","../path","line\n"] { assert!(!valid_opid(s)); }
    }

    #[test]
    fn wallet_history_must_bind_to_confirmed_node_bytes_and_block() {
        let txid = "a".repeat(64);
        let block = "b".repeat(64);
        let valid = json!({"txid":txid,"hex":"00abcd","confirmations":1,"blockhash":block});
        assert!(bind_wallet_history(&valid,&txid,"00abcd",&block).is_ok());
        let uppercase = json!({"txid":txid.to_uppercase(),"hex":"00ABCD",
            "confirmations":2,"blockhash":block.to_uppercase()});
        assert!(bind_wallet_history(&uppercase,&txid,"00abcd",&block).is_err());
        for field in ["txid","hex","confirmations","blockhash"] {
            let mut missing = valid.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(bind_wallet_history(&missing,&txid,"00abcd",&block).is_err());
            let mut wrong_type = valid.clone();
            wrong_type[field] = json!([]);
            assert!(bind_wallet_history(&wrong_type,&txid,"00abcd",&block).is_err());
        }
        for (field, value) in [("txid",json!("c".repeat(64))), ("hex",json!("00abce")),
            ("hex",json!("00abcd00")), ("hex",json!("")), ("confirmations",json!(0)),
            ("confirmations",json!(-1)), ("confirmations",json!(1.5)),
            ("blockhash",json!("c".repeat(64)))]
        {
            let mut bad = valid.clone();
            bad[field] = value;
            assert!(bind_wallet_history(&bad,&txid,"00abcd",&block).is_err());
        }
    }

    #[test]
    fn wallet_history_requires_completed_enhancement_at_the_payout_height() {
        for height in [100,101] {
            assert!(wallet_history_ready(&json!({"scanning":false,"enhanced_through":height}),100).is_ok());
        }
        for bad in [json!({}), json!({"scanning":false}), json!({"enhanced_through":100}),
            json!({"scanning":true,"enhanced_through":100}),
            json!({"scanning":{},"enhanced_through":100}),
            json!({"scanning":false,"enhanced_through":99}),
            json!({"scanning":false,"enhanced_through":-1}),
            json!({"scanning":false,"enhanced_through":"100"})]
        { assert!(wallet_history_ready(&bad,100).is_err()); }
    }
}
