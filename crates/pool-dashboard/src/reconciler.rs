//! Payout reconciliation sweep (audit P4).
//!
//! The payout pipeline's primary bookkeeping can lose records when the
//! process crashes or the wallet RPC fails between `z_sendmany` and the
//! status poll — the tx is on chain but the pool DB never learns. The
//! 2026-06-08 mainnet incident left 8.01 ZEC of completed payouts
//! unrecorded this way; the 2026-03-16 "phantom id=9" was the inverse (a
//! failed broadcast recorded as paid).
//!
//! This module runs an independent periodic sweep with three checks:
//!
//! 1. **Stale attempts** — `payout_attempts` rows stuck in `queued`/`sent`
//!    are resolved via `z_getoperationstatus` (opid) and
//!    `getrawtransaction` (txid). A stale attempt whose tx IS on chain is
//!    the incident signature: the attempt is marked confirmed and a loud
//!    alert is raised so the operator can verify the per-miner `payouts`
//!    rows exist (per-recipient splits aren't recoverable from the attempt
//!    row alone).
//! 2. **Phantom payouts** — every txid recorded in `payouts` over the last
//!    24h must exist on chain. Missing ones are alerted, never auto-deleted.
//! 3. **Accounting invariant** — `SUM(confirmed blocks.reward)` vs
//!    `SUM(balances.pending + paid)` drift beyond a threshold is alerted;
//!    sustained growth indicates a crediting bug.
//!
//! Results are written to `pool_status` under `reconciler_health` for the
//! admin Health page. The sweep never mutates `payouts` or `balances` —
//! it only resolves `payout_attempts` statuses and raises alerts.

use std::sync::Arc;
use std::time::Duration;

use node_rpc::ZcashRpcClient;
use pool_db::PoolDb;
use tracing::{error, info, warn};

const ZATOSHIS_PER_ZEC: f64 = 100_000_000.0;

/// Attempts younger than this are considered still in flight and skipped.
const STALE_ATTEMPT_MINUTES: i64 = 10;

/// Window for the phantom-payout chain check.
const PHANTOM_CHECK_HOURS: i64 = 24;

/// Invariant drift *movement* between sweeps beyond this many zatoshis
/// raises an alert. The absolute drift is reported as context but not
/// alerted on: long-lived pools accumulate benign residue (blocks found
/// with an empty share window distribute nothing; operator balance
/// adjustments; pre-fix orphan-reversal rounding) that would otherwise
/// alarm forever. A *change* in drift means money moved without matching
/// bookkeeping right now — that's the actionable signal.
const INVARIANT_DRIFT_DELTA_ALERT_ZATOSHIS: i64 = 10_000_000;

/// A reserved payout (round-3) whose tx is not on chain is only refunded
/// (paying -> pending) after this long — past the point a submitted Zcash tx
/// would have expired unmined — so a merely-slow broadcast is never refunded
/// and then double-paid when it later confirms.
const RESERVATION_REFUND_MINUTES: i64 = 60;

/// True if the SQLite datetime string `created_at` is older than `minutes` ago.
/// Parse failure -> false (never act destructively on a formatting accident).
fn older_than(created_at: &str, minutes: i64) -> bool {
    match chrono::NaiveDateTime::parse_from_str(created_at, "%Y-%m-%d %H:%M:%S") {
        Ok(t) => t.and_utc() <= chrono::Utc::now() - chrono::Duration::minutes(minutes),
        Err(_) => false,
    }
}

pub struct Reconciler {
    pub db: PoolDb,
    pub node_rpc: Arc<ZcashRpcClient>,
    pub wallet_rpc: Arc<ZcashRpcClient>,
    pub interval: Duration,
    /// Pool fee as a fraction (e.g. 0.01 for 1%). Distribution credits
    /// reward × (1 - fee), so the invariant must compare balances against
    /// the post-fee distributable total, not the raw reward sum.
    pub pool_fee: f64,
    /// The transparent collection address every block's coinbase must pay.
    /// Empty string disables the coinbase-output check (tests).
    pub mining_address: String,
    /// Audit #19: automatically void recorded payouts whose tx the node
    /// authoritatively reports absent (-5) past tx-expiry — the reorged-out
    /// class. Miners were never paid (the wallet kept the funds); voiding
    /// returns balances to pending and the loop re-pays. When false, the
    /// phantom check only alerts (legacy behavior).
    pub auto_void_reorged: bool,
}

impl Reconciler {
    /// Run the sweep loop forever. Spawn this alongside the payout loop.
    pub async fn run(self) {
        info!(interval_secs = self.interval.as_secs(), "Reconciler started");
        // Offset from the payout loop's startup burst.
        tokio::time::sleep(Duration::from_secs(90)).await;
        loop {
            match self.sweep_once().await {
                Ok(summary) => {
                    if !summary.alerts.is_empty() {
                        warn!(alerts = ?summary.alerts, "Reconciler found discrepancies");
                    }
                }
                Err(e) => error!(error = %e, "Reconciler sweep failed"),
            }
            tokio::time::sleep(self.interval).await;
        }
    }

    pub async fn sweep_once(&self) -> anyhow::Result<SweepSummary> {
        let mut summary = SweepSummary::default();

        // Round-3: resolve in-flight reservations first (this moves the `paying`
        // funds via confirm/refund), so the legacy stale-attempt handler below
        // only ever sees non-reserved rows.
        self.reconcile_reserved_payouts(&mut summary, Some(STALE_ATTEMPT_MINUTES)).await;
        self.resolve_stale_attempts(&mut summary).await;
        self.check_phantom_payouts(&mut summary).await;
        self.check_clawbacks(&mut summary).await;
        self.check_invariant(&mut summary).await;
        self.check_coinbase_outputs(&mut summary).await;

        let health = serde_json::json!({
            "invariant_version": self.invariant_version(),
            "last_run": chrono::Utc::now().to_rfc3339(),
            "attempts_resolved": summary.attempts_resolved,
            "attempts_failed": summary.attempts_failed,
            "late_confirmed_attempts": summary.late_confirmed,
            "phantom_txids": summary.phantom_txids,
            "invariant_drift_zec": summary.invariant_drift_zatoshis as f64 / ZATOSHIS_PER_ZEC,
            "alerts": summary.alerts,
        });
        self.db
            .set_pool_status("reconciler_health", &health.to_string())
            .await?;

        Ok(summary)
    }

    /// One-shot reserved-payout reconciliation (round-3), for the dashboard to
    /// call at startup with `older_than_minutes = None` — before the payout loop
    /// starts — so any reservation a crash orphaned is resolved promptly.
    pub async fn reconcile_reserved_payouts_once(
        &self,
        older_than_minutes: Option<i64>,
    ) -> SweepSummary {
        let mut summary = SweepSummary::default();
        self.reconcile_reserved_payouts(&mut summary, older_than_minutes).await;
        summary
    }

    /// Round-3: resolve in-flight payout reservations — attempts still holding
    /// `paying` funds in payout_items. For each, learn the txid (recorded or via
    /// the opid) and check the chain: on chain -> `confirm_payout` (paying ->
    /// paid); operation failed, or txid absent past the expiry window ->
    /// `refund_payout` (paying -> pending). Ambiguous "submitted but not yet
    /// visible" cases are left for a later sweep, so a late broadcast is never
    /// refunded and then double-paid. `older_than_minutes` keeps the periodic
    /// sweep off attempts the live payout loop is still processing.
    async fn reconcile_reserved_payouts(
        &self,
        summary: &mut SweepSummary,
        older_than_minutes: Option<i64>,
    ) {
        let reserved = match self.db.get_reserved_attempts(older_than_minutes).await {
            Ok(v) => v,
            Err(e) => {
                summary.alerts.push(format!("reserved-attempt query failed: {e}"));
                return;
            }
        };
        for (id, _status, opid, txid, total_zats, created_at) in reserved {
            let total_zec = total_zats as f64 / ZATOSHIS_PER_ZEC;

            // Learn the txid: recorded on the attempt, or resolved via the opid.
            let mut resolved_txid = txid.clone();
            let mut op_failed = false;
            if resolved_txid.is_none() {
                if let Some(ref op) = opid {
                    // Wallet unreachable = unknown, NOT "op never existed".
                    // Never let a transport failure fall through to the
                    // expiry-refund below — the tx may be broadcast and a
                    // refund would double-pay once it mines.
                    let statuses = match self.wallet_rpc.z_get_operation_status(&[op]).await {
                        Ok(s) => s,
                        Err(e) => {
                            summary.alerts.push(format!(
                                "reservation {id}: wallet unreachable resolving opid ({e}); holding"
                            ));
                            continue;
                        }
                    };
                    if let Some(st) = statuses.first() {
                        match st.get("status").and_then(|s| s.as_str()).unwrap_or("") {
                            "success" => {
                                resolved_txid = st
                                    .get("result")
                                    .and_then(|r| r.get("txid"))
                                    .and_then(|t| t.as_str())
                                    .map(String::from);
                            }
                            "failed" => op_failed = true,
                            // executing/queued — still in flight, leave it.
                            _ => continue,
                        }
                    }
                }
            }

            if let Some(t) = resolved_txid {
                match self.node_rpc.get_raw_transaction(&t, 1).await {
                    Ok(_) => {
                        // On chain -> finalize the reservation (idempotent).
                        match self.db.confirm_payout(id, &t).await {
                            Ok(n) => {
                                let _ = self
                                    .db
                                    .update_payout_attempt(
                                        id,
                                        "confirmed",
                                        None,
                                        Some(&t),
                                        Some("reconciled: confirmed via chain lookup"),
                                    )
                                    .await;
                                summary.attempts_resolved += 1;
                                summary.late_confirmed += 1;
                                summary.alerts.push(format!(
                                    "reservation {id} ({total_zec:.4} coins) confirmed via chain \
                                     lookup of {t} ({n} items finalized)"
                                ));
                            }
                            Err(e) => summary
                                .alerts
                                .push(format!("reservation {id}: confirm_payout failed: {e}")),
                        }
                    }
                    // Node unreachable / transport error: unknown, NOT absent.
                    // Hold the reservation regardless of age — funds parked in
                    // `paying` are safe; a refund here could double-pay.
                    Err(ref e) if !e.is_definitely_not_found() => {
                        summary.alerts.push(format!(
                            "reservation {id} ({total_zec:.4} coins): node unreachable checking \
                             txid {t} ({e}); holding"
                        ));
                    }
                    Err(_) => {
                        // Authoritative -5: the chain does not know this txid.
                        // Refund only once it is old enough that the tx would
                        // have expired unmined, so a merely-slow broadcast is
                        // never refunded-then-repaid.
                        if older_than(&created_at, RESERVATION_REFUND_MINUTES) {
                            self.refund_reservation(id, total_zec, "txid not on chain (-5) past expiry", summary)
                                .await;
                        } else {
                            summary.alerts.push(format!(
                                "reservation {id} ({total_zec:.4} coins) txid {t} not yet on chain \
                                 — leaving for a later sweep"
                            ));
                        }
                    }
                }
            } else if op_failed {
                // Operation definitively failed: nothing broadcast -> refund.
                self.refund_reservation(id, total_zec, "operation failed", summary)
                    .await;
            } else if opid.is_none() {
                // No opid was ever recorded: z_sendmany never returned, so
                // nothing was submitted. Safe to refund once past expiry.
                if older_than(&created_at, RESERVATION_REFUND_MINUTES) {
                    self.refund_reservation(id, total_zec, "never submitted (no opid) past expiry", summary)
                        .await;
                } else {
                    summary.alerts.push(format!(
                        "reservation {id} ({total_zec:.4} coins) unresolved (no opid yet) \
                         — leaving for a later sweep"
                    ));
                }
            } else {
                // An opid EXISTS but the wallet no longer knows it (restart
                // dropped in-memory op state) and no txid was recorded. The tx
                // may or may not have broadcast — refunding could double-pay,
                // so park the funds in `paying` and page the operator instead.
                summary.alerts.push(format!(
                    "reservation {id} ({total_zec:.4} coins) UNRESOLVABLE: opid known but wallet \
                     lost op state and no txid recorded — funds held in `paying`; operator must \
                     check wallet history and either confirm_payout or refund_payout manually"
                ));
            }
        }
    }

    /// Refund a reservation (paying -> pending) and mark its attempt failed.
    async fn refund_reservation(
        &self,
        id: i64,
        total_zec: f64,
        why: &str,
        summary: &mut SweepSummary,
    ) {
        match self.db.refund_payout(id).await {
            Ok(n) => {
                let _ = self
                    .db
                    .update_payout_attempt(
                        id,
                        "failed",
                        None,
                        None,
                        Some(&format!("reconciled: refunded ({why})")),
                    )
                    .await;
                summary.attempts_failed += 1;
                summary.alerts.push(format!(
                    "reservation {id} ({total_zec:.4} coins) refunded to pending: {why} ({n} items)"
                ));
            }
            Err(e) => summary
                .alerts
                .push(format!("reservation {id}: refund_payout failed: {e}")),
        }
    }

    /// Check 1: resolve payout_attempts stuck in queued/sent.
    async fn resolve_stale_attempts(&self, summary: &mut SweepSummary) {
        let stale = match self.db.get_stale_payout_attempts(STALE_ATTEMPT_MINUTES).await {
            Ok(v) => v,
            Err(e) => {
                summary.alerts.push(format!("stale-attempt query failed: {e}"));
                return;
            }
        };

        for (id, status, opid, txid, total_zatoshis, created_at) in stale {
            let total_zec = total_zatoshis as f64 / ZATOSHIS_PER_ZEC;

            // Try to learn the txid via the opid, if the wallet still knows it.
            let mut resolved_txid = txid.clone();
            if resolved_txid.is_none() {
                if let Some(ref op) = opid {
                    // Transport failure = "wallet unreachable", NOT "op unknown".
                    // Leave the attempt for the next sweep instead of letting it
                    // fall through to the 'unresolvable -> failed' branch (audit:
                    // that misclassification destroyed the double-pay detector
                    // exactly during correlated downtime).
                    let statuses = match self.wallet_rpc.z_get_operation_status(&[op]).await {
                        Ok(s) => s,
                        Err(e) => {
                            summary.alerts.push(format!(
                                "attempt {id}: wallet unreachable resolving opid ({e}); leaving for next sweep"
                            ));
                            continue;
                        }
                    };
                    {
                        if let Some(st) = statuses.first() {
                            let state = st.get("status").and_then(|s| s.as_str()).unwrap_or("");
                            match state {
                                "success" => {
                                    resolved_txid = st
                                        .get("result")
                                        .and_then(|r| r.get("txid"))
                                        .and_then(|t| t.as_str())
                                        .map(String::from);
                                }
                                "failed" => {
                                    let msg = st
                                        .get("error")
                                        .and_then(|e| e.get("message"))
                                        .and_then(|m| m.as_str())
                                        .unwrap_or("unknown");
                                    let _ = self
                                        .db
                                        .update_payout_attempt(
                                            id,
                                            "failed",
                                            None,
                                            None,
                                            Some(&format!("reconciler: op failed: {msg}")),
                                        )
                                        .await;
                                    summary.attempts_failed += 1;
                                    continue;
                                }
                                // executing/queued — genuinely still in flight
                                // (rare past the stale window, but possible
                                // under heavy proof load). Leave it alone.
                                _ => continue,
                            }
                        }
                    }
                }
            }

            // If we have a txid (recorded or just resolved), ask the node.
            if let Some(ref t) = resolved_txid {
                match self.node_rpc.get_raw_transaction(t, 1).await {
                    // Node unreachable / transport error: unknown, not absent.
                    Err(ref e) if !e.is_definitely_not_found() => {
                        summary.alerts.push(format!(
                            "attempt {id}: node unreachable checking txid {t} ({e}); leaving for next sweep"
                        ));
                    }
                    Ok(_) => {
                        // Tx IS on chain but the attempt never reached
                        // 'confirmed' — the incident signature. Resolve the
                        // attempt and alert: per-miner payout rows may be
                        // missing and need operator verification.
                        let _ = self
                            .db
                            .update_payout_attempt(
                                id,
                                "confirmed",
                                None,
                                Some(t),
                                Some("reconciler: late-confirmed via chain lookup"),
                            )
                            .await;
                        summary.attempts_resolved += 1;
                        summary.late_confirmed += 1;
                        summary.alerts.push(format!(
                            "attempt {id} ({total_zec:.4} coins, created {created_at}) \
                             confirmed late via chain lookup of {t} — verify per-miner \
                             payout rows exist for this tx"
                        ));
                    }
                    Err(_) => {
                        // The node AUTHORITATIVELY does not know this txid
                        // (JSON-RPC -5; transport errors were handled above).
                        // If the tx expired unmined this attempt is dead.
                        let _ = self
                            .db
                            .update_payout_attempt(
                                id,
                                "failed",
                                None,
                                None,
                                Some("reconciler: txid not found on chain (-5)"),
                            )
                            .await;
                        summary.attempts_failed += 1;
                        summary.alerts.push(format!(
                            "attempt {id} ({total_zec:.4} coins) had txid {t} that never \
                             reached the chain — marked failed; pending balances were \
                             never debited so miners will be paid next round"
                        ));
                    }
                }
            } else {
                // No txid and the opid resolved nothing (wallet restarted and
                // dropped its in-memory op state, or z_sendmany never returned).
                // Mark failed: if the tx somehow did broadcast, the phantom
                // check on payouts plus the operator alert path still cover it.
                let _ = self
                    .db
                    .update_payout_attempt(
                        id,
                        "failed",
                        None,
                        None,
                        Some(&format!(
                            "reconciler: unresolvable (status was '{status}', opid {} )",
                            opid.as_deref().unwrap_or("none")
                        )),
                    )
                    .await;
                summary.attempts_failed += 1;
                summary.alerts.push(format!(
                    "attempt {id} ({total_zec:.4} coins, created {created_at}) was \
                     unresolvable — wallet has no record of opid; verify wallet \
                     transaction history manually if balances look short"
                ));
            }
        }
    }

    /// Check: every newly-confirmed block's coinbase must pay the pool's
    /// collection address (audit: this was assumed, never verified — a
    /// template bug paying elsewhere would inflate book income silently).
    /// Walks forward from a persisted height watermark, ≤20 blocks per
    /// sweep; the watermark only advances past blocks actually verified,
    /// so a node outage just pauses the check.
    async fn check_coinbase_outputs(&self, summary: &mut SweepSummary) {
        if self.mining_address.is_empty() {
            return;
        }
        let last: i64 = match self.db.get_pool_status("coinbase_check_height").await {
            Ok(v) => v
                .and_then(|(val, _)| val.parse::<i64>().ok())
                .unwrap_or(0),
            Err(e) => {
                summary.alerts.push(format!("coinbase-check watermark read failed: {e}"));
                return;
            }
        };
        let blocks = match self.db.get_confirmed_blocks_above(last, 20).await {
            Ok(v) => v,
            Err(e) => {
                summary.alerts.push(format!("coinbase-check block query failed: {e}"));
                return;
            }
        };
        let mut watermark = last;
        for (height, _hash) in blocks {
            let paid_us = async {
                let bh = self.node_rpc.get_block_hash(height as u64).await?;
                let blk = self.node_rpc.get_block(&bh, 1).await?;
                let cb_txid = blk
                    .get("tx")
                    .and_then(|t| t.as_array())
                    .and_then(|a| a.first())
                    .and_then(|t| t.as_str())
                    .map(String::from);
                let Some(cb_txid) = cb_txid else {
                    return Ok::<Option<bool>, node_rpc::RpcError>(None);
                };
                let tx = self.node_rpc.get_raw_transaction(&cb_txid, 1).await?;
                let pays = tx
                    .get("vout")
                    .and_then(|v| v.as_array())
                    .map(|vouts| {
                        vouts.iter().any(|v| {
                            v.get("scriptPubKey")
                                .and_then(|s| s.get("addresses"))
                                .and_then(|a| a.as_array())
                                .map(|addrs| {
                                    addrs.iter().any(|a| a.as_str() == Some(self.mining_address.as_str()))
                                })
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false);
                Ok(Some(pays))
            }
            .await;
            match paid_us {
                Ok(Some(true)) => watermark = height,
                Ok(Some(false)) => {
                    summary.alerts.push(format!(
                        "COINBASE MISMATCH: confirmed block at height {height} does not pay \
                         the mining address — book income may be phantom; investigate before payouts"
                    ));
                    // Advance past it so the alert fires once, not every sweep;
                    // the alert text is the operator's handle.
                    watermark = height;
                }
                Ok(None) => {
                    summary.alerts.push(format!(
                        "coinbase-check: block {height} had no readable coinbase txid; skipping"
                    ));
                    watermark = height;
                }
                Err(_) => break, // node unreachable — retry from watermark next sweep
            }
        }
        if watermark > last {
            let _ = self
                .db
                .set_pool_status("coinbase_check_height", &watermark.to_string())
                .await;
        }
    }

    /// Check: surface fresh orphan clawbacks (credits for orphaned blocks
    /// that were already paid out — operator decides recovery).
    async fn check_clawbacks(&self, summary: &mut SweepSummary) {
        match self.db.get_recent_clawbacks(24).await {
            Ok(rows) => {
                for (block_id, miner_id, amount) in rows {
                    summary.alerts.push(format!(
                        "orphan clawback: miner {miner_id} kept {:.4} coins credited for \
                         orphaned block id {block_id} (already paid out before reversal) — \
                         operator decision: net against future earnings or absorb",
                        amount as f64 / ZATOSHIS_PER_ZEC,
                    ));
                }
            }
            Err(e) => summary.alerts.push(format!("clawback query failed: {e}")),
        }
    }

    /// Check 2: every recently recorded payout txid must exist on chain.
    async fn check_phantom_payouts(&self, summary: &mut SweepSummary) {
        let recent = match self.db.get_recent_payout_txids(PHANTOM_CHECK_HOURS).await {
            Ok(v) => v,
            Err(e) => {
                summary.alerts.push(format!("phantom-check query failed: {e}"));
                return;
            }
        };

        for (txid, total_zatoshis) in recent {
            let total_zec = total_zatoshis as f64 / ZATOSHIS_PER_ZEC;
            match self.node_rpc.get_raw_transaction(&txid, 1).await {
                Ok(_) => {}
                // #13 consistency: transport errors are "unknown", never
                // "absent" — skip this sweep rather than false-phantom.
                Err(ref e) if !e.is_definitely_not_found() => {
                    summary.alerts.push(format!(
                        "phantom-check: node unreachable for txid {txid} ({e}); skipping this sweep"
                    ));
                }
                Err(_) => {
                    // Authoritative -5: recorded as paid, tx not on chain.
                    if self.auto_void_reorged {
                        // Audit #19: correct the books without a human. The
                        // age guard (60 min) lives inside void_reorged_payout;
                        // a fresh tx returns (0,0) and falls through to the
                        // alert-only path for this sweep.
                        match self.db.void_reorged_payout(&txid).await {
                            Ok((rows, zats)) if rows > 0 => {
                                summary.alerts.push(format!(
                                    "AUTO-VOIDED payout txid {txid}: reorged out and expired (-5); \
                                     {rows} payout row(s) / {:.4} coins returned to pending — \
                                     miners re-pay next cycle",
                                    zats as f64 / ZATOSHIS_PER_ZEC
                                ));
                                continue;
                            }
                            Ok(_) => {} // too fresh / already voided — alert below
                            Err(e) => {
                                summary.alerts.push(format!(
                                    "auto-void FAILED for txid {txid}: {e} — manual reconcile required"
                                ));
                                continue;
                            }
                        }
                    }
                    summary.phantom_txids.push(txid.clone());
                    summary.alerts.push(format!(
                        "payout txid {txid} ({total_zec:.4} coins) is recorded as paid but \
                         not found on chain — phantom payout; balances.paid is overstated \
                         (manual reconcile required; reconciler never auto-deletes)"
                    ));
                }
            }
        }
    }

    /// Identifies the invariant formula + parameters. When this changes
    /// (formula edits across deploys, operator fee changes), the stored
    /// baseline is meaningless — re-baseline silently instead of alerting
    /// on the definition jump.
    fn invariant_version(&self) -> String {
        format!("v4-costrecovery-fee{:.4}", self.pool_fee)
    }

    /// Check 3: confirmed-rewards vs balances invariant. Alerts on drift
    /// *movement* since the previous sweep, not on the absolute value.
    async fn check_invariant(&self, summary: &mut SweepSummary) {
        match self.db.get_accounting_invariant().await {
            Ok((reward, balances, clawbacks)) => {
                let distributable = ((reward as f64) * (1.0 - self.pool_fee)) as i64;
                // Clawbacks are credits for orphaned blocks that had already
                // been paid out — explained, explicitly-ledgered drift.
                // Subtract them so the invariant only alarms on the
                // UNEXPLAINED kind.
                let drift = balances - clawbacks - distributable;
                summary.invariant_drift_zatoshis = drift;

                // Baseline = the drift recorded by the previous sweep, valid
                // only if it was computed with the same formula + parameters.
                let previous = self
                    .db
                    .get_pool_status("reconciler_health")
                    .await
                    .ok()
                    .flatten()
                    .and_then(|(v, _)| serde_json::from_str::<serde_json::Value>(&v).ok())
                    .filter(|j| {
                        j.get("invariant_version").and_then(|v| v.as_str())
                            == Some(self.invariant_version().as_str())
                    })
                    .and_then(|j| j.get("invariant_drift_zec").and_then(|d| d.as_f64()))
                    .map(|zec| (zec * ZATOSHIS_PER_ZEC) as i64);

                match previous {
                    Some(prev) => {
                        let delta = drift - prev;
                        if delta.abs() > INVARIANT_DRIFT_DELTA_ALERT_ZATOSHIS {
                            summary.alerts.push(format!(
                                "accounting invariant moved by {:+.4} coins since last \
                                 sweep (absolute drift now {:.4}) — money moved without \
                                 matching bookkeeping",
                                delta as f64 / ZATOSHIS_PER_ZEC,
                                drift as f64 / ZATOSHIS_PER_ZEC,
                            ));
                        }
                    }
                    None => {
                        // First sweep ever, or the formula/fee changed:
                        // establish a fresh baseline, inform only.
                        info!(
                            drift_zec = drift as f64 / ZATOSHIS_PER_ZEC,
                            version = %self.invariant_version(),
                            "Reconciler invariant baseline established"
                        );
                    }
                }
            }
            Err(e) => summary.alerts.push(format!("invariant query failed: {e}")),
        }
    }
}

#[derive(Debug, Default)]
pub struct SweepSummary {
    pub attempts_resolved: u32,
    pub attempts_failed: u32,
    pub late_confirmed: u32,
    pub phantom_txids: Vec<String>,
    pub invariant_drift_zatoshis: i64,
    pub alerts: Vec<String>,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use axum::{extract::State, routing::post, Json, Router};
    use sqlx::sqlite::SqlitePoolOptions;
    use std::collections::HashMap;

    /// Mock JSON-RPC server: answers each method from a canned response map.
    /// Unknown methods get a JSON-RPC error (matching a node that doesn't
    /// know the tx / op).
    pub(crate) async fn mock_rpc(responses: HashMap<&'static str, serde_json::Value>) -> String {
        let state = Arc::new(responses);
        async fn handler(
            State(state): State<Arc<HashMap<&'static str, serde_json::Value>>>,
            Json(req): Json<serde_json::Value>,
        ) -> Json<serde_json::Value> {
            let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
            let id = req.get("id").cloned().unwrap_or(serde_json::json!(1));
            match state.get(method) {
                Some(result) => Json(serde_json::json!({
                    "jsonrpc": "2.0", "id": id, "result": result, "error": null
                })),
                None => Json(serde_json::json!({
                    "jsonrpc": "2.0", "id": id, "result": null,
                    "error": {"code": -5, "message": "No such mempool or main chain transaction"}
                })),
            }
        }
        let app = Router::new().route("/", post(handler)).with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    /// In-memory pool DB + a raw handle for seeding rows directly.
    pub(crate) async fn setup_db() -> (PoolDb, sqlx::SqlitePool) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let db = PoolDb::new(pool.clone());
        db.run_migrations().await.unwrap();
        (db, pool)
    }

    pub(crate) fn reconciler(db: PoolDb, node_url: &str, wallet_url: &str) -> Reconciler {
        Reconciler {
            db,
            node_rpc: Arc::new(ZcashRpcClient::new(node_url)),
            wallet_rpc: Arc::new(ZcashRpcClient::new(wallet_url)),
            interval: Duration::from_secs(600),
            pool_fee: 0.0,
            // Empty disables the coinbase-output check in unit tests.
            mining_address: String::new(),
            // Legacy alert-only behavior for existing phantom tests; the
            // auto-void test flips this on explicitly.
            auto_void_reorged: false,
        }
    }

    #[tokio::test]
    async fn clean_state_no_alerts() {
        let (db, _pool) = setup_db().await;
        let node = mock_rpc(HashMap::new()).await;
        let wallet = mock_rpc(HashMap::new()).await;
        let r = reconciler(db, &node, &wallet);
        let s = r.sweep_once().await.unwrap();
        assert!(s.alerts.is_empty(), "alerts: {:?}", s.alerts);
        assert_eq!(s.attempts_resolved, 0);
        assert!(s.phantom_txids.is_empty());
    }

    #[tokio::test]
    async fn stale_sent_attempt_with_tx_on_chain_late_confirms() {
        let (db, pool) = setup_db().await;
        // Stale 'sent' attempt with an opid; the wallet remembers the op as
        // success with a txid, and the node has the tx — incident signature.
        sqlx::query(
            "INSERT INTO payout_attempts (status, opid, miner_count, total_zatoshis, source, created_at, updated_at)
             VALUES ('sent', 'opid-test1', 2, 800000000, 'loop', datetime('now','-30 minutes'), datetime('now','-30 minutes'))",
        )
        .execute(&pool)
        .await
        .unwrap();

        let wallet = mock_rpc(HashMap::from([(
            "z_getoperationstatus",
            serde_json::json!([{"id": "opid-test1", "status": "success",
                               "result": {"txid": "ab12cd34"}}]),
        )]))
        .await;
        let node = mock_rpc(HashMap::from([(
            "getrawtransaction",
            serde_json::json!({"txid": "ab12cd34", "height": 100}),
        )]))
        .await;

        let r = reconciler(db.clone(), &node, &wallet);
        let s = r.sweep_once().await.unwrap();
        assert_eq!(s.late_confirmed, 1);
        assert_eq!(s.attempts_resolved, 1);
        assert!(s.alerts.iter().any(|a| a.contains("confirmed late")), "alerts: {:?}", s.alerts);

        // Attempt row is now confirmed with the txid recorded.
        let stale_after = db.get_stale_payout_attempts(10).await.unwrap();
        assert!(stale_after.is_empty());
    }

    #[tokio::test]
    async fn stale_attempt_with_txid_not_on_chain_fails() {
        let (db, pool) = setup_db().await;
        sqlx::query(
            "INSERT INTO payout_attempts (status, opid, txid, miner_count, total_zatoshis, source, created_at, updated_at)
             VALUES ('sent', 'opid-test2', 'dead00beef', 1, 100000000, 'loop', datetime('now','-30 minutes'), datetime('now','-30 minutes'))",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Node knows nothing (default error response); wallet knows nothing.
        let node = mock_rpc(HashMap::new()).await;
        let wallet = mock_rpc(HashMap::new()).await;

        let r = reconciler(db.clone(), &node, &wallet);
        let s = r.sweep_once().await.unwrap();
        assert_eq!(s.attempts_failed, 1);
        assert!(
            s.alerts.iter().any(|a| a.contains("never") && a.contains("dead00beef")),
            "alerts: {:?}",
            s.alerts
        );
    }

    /// Audit #13: a TRANSPORT failure (node unreachable) must never be read as
    /// "tx doesn't exist" — the attempt is left for the next sweep, not failed.
    #[tokio::test]
    async fn stale_attempt_transport_error_left_for_next_sweep() {
        let (db, pool) = setup_db().await;
        sqlx::query(
            "INSERT INTO payout_attempts (status, opid, txid, miner_count, total_zatoshis, source, created_at, updated_at)
             VALUES ('sent', 'opid-t1', 'beef01', 1, 100000000, 'loop', datetime('now','-30 minutes'), datetime('now','-30 minutes'))",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Unreachable node = connection refused = RpcError::Http (transport).
        let wallet = mock_rpc(HashMap::new()).await;
        let r = reconciler(db.clone(), "http://127.0.0.1:9", &wallet);
        let s = r.sweep_once().await.unwrap();

        assert_eq!(s.attempts_failed, 0, "transport error must not fail the attempt");
        let still_stale = db.get_stale_payout_attempts(10).await.unwrap();
        assert_eq!(still_stale.len(), 1, "attempt must remain for the next sweep");
        assert!(
            s.alerts.iter().any(|a| a.contains("unreachable")),
            "alerts: {:?}",
            s.alerts
        );
    }

    /// Audit #13 (round-3): a reservation whose txid check hits a TRANSPORT
    /// error is HELD — never refunded — even far past the expiry window.
    #[tokio::test]
    async fn reserved_attempt_transport_error_never_refunds() {
        let (db, pool) = setup_db().await;
        sqlx::query("INSERT INTO miners (id, address) VALUES (7, 'utest1r3hold')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO balances (miner_id, pending, paid, paying) VALUES (7, 0, 0, 50000000)")
            .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO payout_attempts (id, status, opid, txid, miner_count, total_zatoshis, source, created_at, updated_at)
             VALUES (500, 'sent', 'opid-r3', 'beef02', 1, 50000000, 'loop', datetime('now','-180 minutes'), datetime('now','-180 minutes'))",
        )
        .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO payout_items (attempt_id, miner_id, amount) VALUES (500, 7, 50000000)")
            .execute(&pool).await.unwrap();

        let wallet = mock_rpc(HashMap::new()).await;
        let r = reconciler(db.clone(), "http://127.0.0.1:9", &wallet);
        let s = r.sweep_once().await.unwrap();

        let (pending, paying): (i64, i64) =
            sqlx::query_as("SELECT pending, paying FROM balances WHERE miner_id = 7")
                .fetch_one(&pool).await.unwrap();
        assert_eq!(paying, 50000000, "reservation must be HELD on transport error");
        assert_eq!(pending, 0, "no refund on transport error");
        assert_eq!(db.get_reserved_attempts(None).await.unwrap().len(), 1);
        assert!(
            s.alerts.iter().any(|a| a.contains("holding") || a.contains("unreachable")),
            "alerts: {:?}",
            s.alerts
        );
    }

    /// Audit #13 (round-3): an AUTHORITATIVE -5 past the expiry window DOES
    /// refund the reservation (paying -> pending, items cleared).
    #[tokio::test]
    async fn reserved_attempt_not_found_past_expiry_refunds() {
        let (db, pool) = setup_db().await;
        sqlx::query("INSERT INTO miners (id, address) VALUES (8, 'utest1r3ref')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO balances (miner_id, pending, paid, paying) VALUES (8, 0, 0, 60000000)")
            .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO payout_attempts (id, status, opid, txid, miner_count, total_zatoshis, source, created_at, updated_at)
             VALUES (501, 'sent', 'opid-r4', 'beef03', 1, 60000000, 'loop', datetime('now','-180 minutes'), datetime('now','-180 minutes'))",
        )
        .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO payout_items (attempt_id, miner_id, amount) VALUES (501, 8, 60000000)")
            .execute(&pool).await.unwrap();

        // Mock node: unknown methods -> -5 (authoritative not-found).
        let node = mock_rpc(HashMap::new()).await;
        let wallet = mock_rpc(HashMap::new()).await;
        let r = reconciler(db.clone(), &node, &wallet);
        let _ = r.sweep_once().await.unwrap();

        let (pending, paying): (i64, i64) =
            sqlx::query_as("SELECT pending, paying FROM balances WHERE miner_id = 8")
                .fetch_one(&pool).await.unwrap();
        assert_eq!(pending, 60000000, "authoritative -5 past expiry refunds to pending");
        assert_eq!(paying, 0);
        assert!(db.get_reserved_attempts(None).await.unwrap().is_empty());
    }

    /// Audit #13 (round-3): opid recorded but wallet lost op state and no txid
    /// — funds are PARKED with an operator alert, never auto-refunded.
    #[tokio::test]
    async fn reserved_attempt_opid_lost_parks_funds() {
        let (db, pool) = setup_db().await;
        sqlx::query("INSERT INTO miners (id, address) VALUES (9, 'utest1r3park')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO balances (miner_id, pending, paid, paying) VALUES (9, 0, 0, 70000000)")
            .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO payout_attempts (id, status, opid, txid, miner_count, total_zatoshis, source, created_at, updated_at)
             VALUES (502, 'sent', 'opid-lost', NULL, 1, 70000000, 'loop', datetime('now','-180 minutes'), datetime('now','-180 minutes'))",
        )
        .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO payout_items (attempt_id, miner_id, amount) VALUES (502, 9, 70000000)")
            .execute(&pool).await.unwrap();

        // Wallet answers z_getoperationstatus with an EMPTY array (restart
        // dropped op state); node would answer -5 but must never be consulted
        // without a txid.
        let node = mock_rpc(HashMap::new()).await;
        let wallet = mock_rpc(HashMap::from([("z_getoperationstatus", serde_json::json!([]))])).await;
        let r = reconciler(db.clone(), &node, &wallet);
        let s = r.sweep_once().await.unwrap();

        let (pending, paying): (i64, i64) =
            sqlx::query_as("SELECT pending, paying FROM balances WHERE miner_id = 9")
                .fetch_one(&pool).await.unwrap();
        assert_eq!(paying, 70000000, "lost-opid reservation must be parked, not refunded");
        assert_eq!(pending, 0);
        assert!(
            s.alerts.iter().any(|a| a.contains("UNRESOLVABLE")),
            "alerts: {:?}",
            s.alerts
        );
    }

    /// Audit #19: a recorded payout whose tx is authoritatively absent (-5)
    /// past expiry is AUTO-VOIDED — paid returns to pending, rows deleted,
    /// attempt failed — with no human in the loop.
    #[tokio::test]
    async fn phantom_payout_auto_voided_when_enabled() {
        let (db, pool) = setup_db().await;
        sqlx::query("INSERT INTO miners (id, address) VALUES (11, 'utest1av')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO balances (miner_id, pending, paid, paying) VALUES (11, 0, 90000000, 0)")
            .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO payouts (miner_id, txid, amount, created_at) \
             VALUES (11, 'reorgedbeef', 90000000, datetime('now','-120 minutes'))",
        )
        .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO payout_attempts (status, txid, miner_count, total_zatoshis, source, created_at, updated_at) \
             VALUES ('confirmed', 'reorgedbeef', 1, 90000000, 'loop', datetime('now','-120 minutes'), datetime('now','-120 minutes'))",
        )
        .execute(&pool).await.unwrap();

        // Node: default mock answers -5 for everything (authoritative absent).
        let node = mock_rpc(HashMap::new()).await;
        let wallet = mock_rpc(HashMap::new()).await;
        let mut r = reconciler(db.clone(), &node, &wallet);
        r.auto_void_reorged = true;
        let s = r.sweep_once().await.unwrap();

        let (pending, paid): (i64, i64) =
            sqlx::query_as("SELECT pending, paid FROM balances WHERE miner_id = 11")
                .fetch_one(&pool).await.unwrap();
        assert_eq!(pending, 90000000, "funds returned to pending");
        assert_eq!(paid, 0, "paid corrected");
        let rows: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM payouts WHERE txid='reorgedbeef'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(rows.0, 0, "payout rows deleted");
        let st: (String,) = sqlx::query_as("SELECT status FROM payout_attempts WHERE txid='reorgedbeef'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(st.0, "failed", "attempt marked failed");
        assert!(
            s.alerts.iter().any(|a| a.contains("AUTO-VOIDED")),
            "alerts: {:?}",
            s.alerts
        );
        assert!(s.phantom_txids.is_empty(), "voided tx is not left as a phantom");
    }

    /// Audit #19 age guard: a -5 tx younger than 60 min is NOT voided —
    /// alert-only this sweep (a merely-slow broadcast can never be voided).
    #[tokio::test]
    async fn phantom_payout_too_fresh_not_voided() {
        let (db, pool) = setup_db().await;
        sqlx::query("INSERT INTO miners (id, address) VALUES (12, 'utest1fresh')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO balances (miner_id, pending, paid, paying) VALUES (12, 0, 50000000, 0)")
            .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO payouts (miner_id, txid, amount) VALUES (12, 'freshbeef', 50000000)",
        )
        .execute(&pool).await.unwrap();

        let node = mock_rpc(HashMap::new()).await;
        let wallet = mock_rpc(HashMap::new()).await;
        let mut r = reconciler(db.clone(), &node, &wallet);
        r.auto_void_reorged = true;
        let s = r.sweep_once().await.unwrap();

        let (pending, paid): (i64, i64) =
            sqlx::query_as("SELECT pending, paid FROM balances WHERE miner_id = 12")
                .fetch_one(&pool).await.unwrap();
        assert_eq!(paid, 50000000, "fresh tx untouched");
        assert_eq!(pending, 0);
        assert!(
            s.alerts.iter().any(|a| a.contains("phantom payout")),
            "falls back to alert-only: {:?}",
            s.alerts
        );
    }

    #[tokio::test]
    async fn phantom_payout_alert() {
        let (db, pool) = setup_db().await;
        // A payouts row whose txid is not on chain (the id=9 incident shape).
        sqlx::query("INSERT INTO miners (address) VALUES ('utest1aaa')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO payouts (miner_id, txid, amount) VALUES (1, 'phant0m11', 1844264)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let node = mock_rpc(HashMap::new()).await; // tx not found
        let wallet = mock_rpc(HashMap::new()).await;

        let r = reconciler(db, &node, &wallet);
        let s = r.sweep_once().await.unwrap();
        assert_eq!(s.phantom_txids, vec!["phant0m11".to_string()]);
        assert!(s.alerts.iter().any(|a| a.contains("phantom")), "alerts: {:?}", s.alerts);
    }

    #[tokio::test]
    async fn invariant_drift_alert() {
        let (db, pool) = setup_db().await;
        // First sweep establishes a 0-drift baseline; then reward/balances
        // diverge by 0.5 → the second sweep alerts on the delta.
        sqlx::query(
            "INSERT INTO blocks (height, hash, reward, status, found_by)
             VALUES (100, 'aa', 1000000000, 'confirmed', NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO miners (address) VALUES ('utest1bbb')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO balances (miner_id, pending, paid) VALUES (1, 50000000, 1000000000)")
            .execute(&pool)
            .await
            .unwrap();

        let node = mock_rpc(HashMap::new()).await;
        let wallet = mock_rpc(HashMap::new()).await;

        let r = reconciler(db, &node, &wallet);
        // Sweep 1: no prior reconciler_health → baseline only, no alert.
        let s1 = r.sweep_once().await.unwrap();
        assert_eq!(s1.invariant_drift_zatoshis, 50_000_000);
        assert!(s1.alerts.is_empty(), "first sweep should baseline: {:?}", s1.alerts);
        // Sweep 2: drift unchanged → still quiet.
        let s2 = r.sweep_once().await.unwrap();
        assert!(s2.alerts.is_empty(), "unchanged drift must not alert: {:?}", s2.alerts);
        // Now balances move by 0.5 with no matching reward → delta alert.
        sqlx::query("UPDATE balances SET paid = paid + 50000000 WHERE miner_id = 1")
            .execute(&pool)
            .await
            .unwrap();
        let s3 = r.sweep_once().await.unwrap();
        assert!(
            s3.alerts.iter().any(|a| a.contains("invariant moved")),
            "delta must alert: {:?}",
            s3.alerts
        );
        // A freshly-found immature block credited at find time must NOT
        // register as drift: pending blocks count toward the reward side.
        sqlx::query(
            "INSERT INTO blocks (height, hash, reward, status, found_by)
             VALUES (101, 'cc', 100000000, 'pending', NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE balances SET pending = pending + 100000000 WHERE miner_id = 1")
            .execute(&pool)
            .await
            .unwrap();
        let s4 = r.sweep_once().await.unwrap();
        assert!(
            s4.alerts.is_empty(),
            "find-time credit on immature block must not alert: {:?}",
            s4.alerts
        );
    }

    #[tokio::test]
    async fn fresh_attempt_left_alone() {
        let (db, pool) = setup_db().await;
        // Recently-created 'sent' attempt must NOT be touched (still in flight).
        sqlx::query(
            "INSERT INTO payout_attempts (status, opid, miner_count, total_zatoshis, source)
             VALUES ('sent', 'opid-fresh', 1, 100000000, 'loop')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let node = mock_rpc(HashMap::new()).await;
        let wallet = mock_rpc(HashMap::new()).await;

        let r = reconciler(db.clone(), &node, &wallet);
        let s = r.sweep_once().await.unwrap();
        assert_eq!(s.attempts_resolved + s.attempts_failed, 0);
        assert!(s.alerts.is_empty(), "alerts: {:?}", s.alerts);
    }
}

#[cfg(test)]
mod fee_tests {
    use super::tests::*;
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn invariant_accounts_for_pool_fee() {
        let (db, pool) = setup_db().await;
        // 100 coins confirmed, 1% fee → distributable 99. Balances at 99
        // must NOT alert; the old fee-blind check would see -1.0 drift.
        sqlx::query(
            "INSERT INTO blocks (height, hash, reward, status, found_by)
             VALUES (100, 'bb', 10000000000, 'confirmed', NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO miners (address) VALUES ('utest1fee')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO balances (miner_id, pending, paid) VALUES (1, 0, 9900000000)")
            .execute(&pool)
            .await
            .unwrap();

        let node = mock_rpc(HashMap::new()).await;
        let wallet = mock_rpc(HashMap::new()).await;
        let mut r = reconciler(db, &node, &wallet);
        r.pool_fee = 0.01;
        let s = r.sweep_once().await.unwrap();
        assert_eq!(s.invariant_drift_zatoshis, 0, "fee-blind check would see -1.0");
        assert!(s.alerts.is_empty(), "alerts: {:?}", s.alerts);
    }
}
