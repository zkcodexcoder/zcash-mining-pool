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

/// Invariant drift beyond this many zatoshis raises an alert.
/// 0.1 ZEC: comfortably above the known ~0.06 ZEC historical orphan-reversal
/// rounding residue, low enough to catch a single misrecorded payout.
const INVARIANT_DRIFT_ALERT_ZATOSHIS: i64 = 10_000_000;

pub struct Reconciler {
    pub db: PoolDb,
    pub node_rpc: Arc<ZcashRpcClient>,
    pub wallet_rpc: Arc<ZcashRpcClient>,
    pub interval: Duration,
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

        self.resolve_stale_attempts(&mut summary).await;
        self.check_phantom_payouts(&mut summary).await;
        self.check_invariant(&mut summary).await;

        let health = serde_json::json!({
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
                    if let Ok(statuses) = self.wallet_rpc.z_get_operation_status(&[op]).await {
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
                        // Has a txid but the chain doesn't know it. If the tx
                        // expired unmined this attempt is dead.
                        let _ = self
                            .db
                            .update_payout_attempt(
                                id,
                                "failed",
                                None,
                                None,
                                Some("reconciler: txid not found on chain"),
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
            if self.node_rpc.get_raw_transaction(&txid, 1).await.is_err() {
                let total_zec = total_zatoshis as f64 / ZATOSHIS_PER_ZEC;
                summary.phantom_txids.push(txid.clone());
                summary.alerts.push(format!(
                    "payout txid {txid} ({total_zec:.4} coins) is recorded as paid but \
                     not found on chain — phantom payout; balances.paid is overstated \
                     (manual reconcile required; reconciler never auto-deletes)"
                ));
            }
        }
    }

    /// Check 3: confirmed-rewards vs balances invariant.
    async fn check_invariant(&self, summary: &mut SweepSummary) {
        match self.db.get_accounting_invariant().await {
            Ok((reward, balances)) => {
                let drift = balances - reward;
                summary.invariant_drift_zatoshis = drift;
                if drift.abs() > INVARIANT_DRIFT_ALERT_ZATOSHIS {
                    summary.alerts.push(format!(
                        "accounting invariant drift: balances exceed confirmed rewards \
                         by {:.4} coins (threshold {:.4})",
                        drift as f64 / ZATOSHIS_PER_ZEC,
                        INVARIANT_DRIFT_ALERT_ZATOSHIS as f64 / ZATOSHIS_PER_ZEC,
                    ));
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
mod tests {
    use super::*;
    use axum::{extract::State, routing::post, Json, Router};
    use sqlx::sqlite::SqlitePoolOptions;
    use std::collections::HashMap;

    /// Mock JSON-RPC server: answers each method from a canned response map.
    /// Unknown methods get a JSON-RPC error (matching a node that doesn't
    /// know the tx / op).
    async fn mock_rpc(responses: HashMap<&'static str, serde_json::Value>) -> String {
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
    async fn setup_db() -> (PoolDb, sqlx::SqlitePool) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let db = PoolDb::new(pool.clone());
        db.run_migrations().await.unwrap();
        (db, pool)
    }

    fn reconciler(db: PoolDb, node_url: &str, wallet_url: &str) -> Reconciler {
        Reconciler {
            db,
            node_rpc: Arc::new(ZcashRpcClient::new(node_url)),
            wallet_rpc: Arc::new(ZcashRpcClient::new(wallet_url)),
            interval: Duration::from_secs(600),
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
        // Confirmed reward 10.0 but balances total 10.5 → 0.5 drift > 0.1 threshold.
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
        let s = r.sweep_once().await.unwrap();
        assert_eq!(s.invariant_drift_zatoshis, 50_000_000);
        assert!(s.alerts.iter().any(|a| a.contains("invariant")), "alerts: {:?}", s.alerts);
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
