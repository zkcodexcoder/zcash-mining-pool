//! Synthetic loopback-only admission and durable recovery tests. Old-wallet
//! responses deliberately lack PCZT support and must never authorize a send.
//! Directly seeded fee/proposal rows test DB recovery, not successful PCZT use.
use super::*;
use crate::reconciler::tests::{mock_rpc, setup_db};
use pool_db::pps_live::{PpsChainLease, PpsCredit, PpsFeeReservation, PpsFundingLease, PPS_SCALE};

const ADDRESS: &str = "tmFU5Ak942B7SciQpZCh3xH76QV3UmJgnDd";
const TXID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CREDIT: i64 = 50_000_000;

fn lease(now: i64) -> PpsChainLease {
    PpsChainLease {
        network: "testnet".into(),
        checked_at_unix: now,
        valid_until_unix: now + 90,
        agreeing_references: 2,
        disagreement: false,
    }
}

fn gate(node: &str, checked_at: i64) -> Arc<PpsGate> {
    Arc::new(PpsGate::synthetic(
        Arc::new(ZcashRpcClient::new(node)),
        "testnet",
        lease(checked_at),
    ))
}

async fn fixture(address: &str) -> (PoolDb, sqlx::SqlitePool, PpsPolicy, i64) {
    let (db, pool) = setup_db().await;
    let now = Utc::now().timestamp();
    let policy = PpsPolicy {
        network: "testnet".into(),
        epoch: "pps-test-1".into(),
        fee_bps: 100,
        max_liability_zatoshis: 100_000_000,
        total_exposure_zatoshis: 110_000_000,
        fee_allowance_zatoshis: 10_000_000,
        reserve_min_zatoshis: 10_000_000,
        max_payout_zatoshis: 20_000_000,
    };
    db.initialize_pps_epoch(&policy.epoch_config(), Some(&funding(&db, &policy).await))
        .await
        .unwrap();
    let miner = db.get_or_create_miner(address).await.unwrap();
    let worker = db
        .get_or_create_worker(miner.id, "synthetic-worker")
        .await
        .unwrap();
    db.credit_pps_share(
        &policy.epoch_config(),
        &PpsCredit {
            proof_id: "b".repeat(64),
            quote_id: "c".repeat(64),
            worker_id: worker.id,
            job_id: "job-1".into(),
            session_id: "session-1".into(),
            difficulty: 1.0,
            is_block: false,
            amount_subzatoshis: CREDIT as u128 * PPS_SCALE,
            accepted_at_unix: now,
            quote_height: 3_000_000,
            network_target_be: [1; 32],
            assigned_share_target_be: [2; 32],
            miner_subsidy_zats: 125_000_000,
        },
        Some(&lease(now)),
        Some(&funding(&db, &policy).await),
        now,
    )
    .await
    .unwrap();
    (db, pool, policy, miner.id)
}

async fn funding(db: &PoolDb, policy: &PpsPolicy) -> PpsFundingLease {
    let s = db
        .pps_funding_snapshot_for_epoch(&policy.epoch_config())
        .await
        .unwrap();
    let now = Utc::now().timestamp();
    PpsFundingLease {
        network: policy.network.clone(),
        checked_at_unix: now,
        valid_until_unix: now + 60,
        spendable_zatoshis: 10_000_000_000,
        reserve_floor_zatoshis: policy.reserve_min_zatoshis,
        reserved_fee_allowance_zatoshis: policy.fee_allowance_zatoshis,
        generation: s.generation,
    }
}

fn fee(attempt: i64) -> PpsFeeReservation {
    PpsFeeReservation {
        proposal_id: format!("{attempt:064x}"),
        fee_zatoshis: 10_000,
    }
}
async fn reserve_sealed(
    db: &PoolDb,
    policy: &PpsPolicy,
    attempt: i64,
    miner: i64,
    amount: i64,
    txid: Option<&str>,
) {
    db.reserve_pps_payout(
        attempt,
        &[(miner, amount)],
        &funding(db, policy).await,
        &fee(attempt),
    )
    .await
    .unwrap();
    db.seal_pps_payout(attempt, &fee(attempt).proposal_id)
        .await
        .unwrap();
    if let Some(txid) = txid {
        db.mark_pps_payout_signed(attempt, &fee(attempt).proposal_id, txid)
            .await
            .unwrap();
    }
}

struct UnsupportedWallet {
    url: String,
    calls: Arc<std::sync::Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}
impl std::ops::Deref for UnsupportedWallet {
    type Target = str;
    fn deref(&self) -> &str {
        &self.url
    }
}
impl Drop for UnsupportedWallet {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl UnsupportedWallet {
    fn assert_no_wallet_mutation(&self) {
        let calls = self.calls.lock().unwrap();
        for method in [
            "z_sendmany",
            "pczt_create",
            "pczt_prove",
            "pczt_sign",
            "pczt_extract",
            "sendrawtransaction",
        ] {
            assert!(
                !calls.iter().any(|v| v == method),
                "unsupported wallet received {method}"
            );
        }
    }
}
async fn wallet(result: serde_json::Value) -> UnsupportedWallet {
    use axum::{extract::State, routing::post, Json, Router};
    type StateData = (
        HashMap<&'static str, serde_json::Value>,
        Arc<std::sync::Mutex<Vec<String>>>,
    );
    async fn handler(
        State(state): State<Arc<StateData>>,
        Json(req): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        let method = req["method"].as_str().unwrap_or("");
        state.1.lock().unwrap().push(method.into());
        match state.0.get(method) {
            Some(v) => {
                Json(serde_json::json!({"jsonrpc":"2.0","id":req["id"],"result":v,"error":null}))
            }
            None => Json(
                serde_json::json!({"jsonrpc":"2.0","id":req["id"],"result":null,"error":{"code":-32601,"message":"synthetic unsupported method"}}),
            ),
        }
    }
    let responses = HashMap::from([
        (
            "z_gettotalbalance",
            serde_json::json!({"private": "100.0", "total": "100.0"}),
        ),
        ("z_sendmany", serde_json::json!("synthetic-operation")),
        (
            "z_getoperationstatus",
            serde_json::json!([{"status":"success", "result":result}]),
        ),
    ]);
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let state = Arc::new((responses, Arc::clone(&calls)));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new().route("/", post(handler)).with_state(state);
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    UnsupportedWallet { url, calls, task }
}

async fn pay(db: &PoolDb, wallet: &str, node: &str, policy: &PpsPolicy) -> anyhow::Result<usize> {
    pay_with_gate(
        db,
        wallet,
        node,
        policy,
        &gate(node, Utc::now().timestamp()),
    )
    .await
}

async fn pay_with_gate(
    db: &PoolDb,
    wallet: &str,
    node: &str,
    policy: &PpsPolicy,
    gate: &PpsGate,
) -> anyhow::Result<usize> {
    process_payouts_for_ledger(
        db,
        &ZcashRpcClient::new(wallet),
        &ZcashRpcClient::new(node),
        "synthetic-pool",
        ADDRESS,
        1_000_000,
        0,
        1.0,
        "testnet",
        false,
        0,
        i64::MAX,
        Some(policy),
        Some(gate),
    )
    .await
}

#[tokio::test]
async fn pps_old_sendmany_success_is_rejected_without_touching_either_ledger() {
    let (db, pool, policy, miner) = fixture(ADDRESS).await;
    sqlx::query("INSERT INTO balances(miner_id,pending,paying,paid) VALUES(?1,123,0,0)")
        .bind(miner)
        .execute(&pool)
        .await
        .unwrap();
    let wallet = wallet(serde_json::json!({"txids": [TXID]})).await;
    let node = mock_rpc(HashMap::from([(
        "getrawtransaction",
        serde_json::json!({"height": 1}),
    )]))
    .await;
    assert!(pay(&db, &wallet, &node, &policy).await.is_err());
    wallet.assert_no_wallet_mutation();
    let pps = db.pps_invariant().await.unwrap();
    assert_eq!(
        (pps.pending_zatoshis, pps.paying_zatoshis, pps.paid_zatoshis),
        (CREDIT, 0, 0)
    );
    let legacy: (i64, i64, i64) =
        sqlx::query_as("SELECT pending,paying,paid FROM balances WHERE miner_id=?1")
            .bind(miner)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(legacy, (123, 0, 0));
    let old_payouts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payouts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(old_payouts, 0);
    assert_eq!(db.get_reserved_pps_attempts(None).await.unwrap().len(), 0);
}

#[tokio::test]
async fn pps_old_wallet_variants_create_no_reservation_and_recovery_does_not_send() {
    for result in [
        serde_json::json!({"txids": [TXID, "d".repeat(64)]}),
        serde_json::json!({"txid": "unknown"}),
    ] {
        let (db, _pool, policy, _miner) = fixture(ADDRESS).await;
        let wallet = wallet(result).await;
        let node = mock_rpc(HashMap::new()).await;
        assert!(pay(&db, &wallet, &node, &policy).await.is_err());
        let before = db.pps_invariant().await.unwrap();
        assert_eq!(
            (
                before.pending_zatoshis,
                before.paying_zatoshis,
                before.paid_zatoshis
            ),
            (CREDIT, 0, 0)
        );
        let mut r = crate::reconciler::tests::reconciler(db.clone(), &node, &wallet);
        r.pps_policy = Some(policy);
        r.pps_gate = Some(gate(&node, Utc::now().timestamp()));
        r.reconcile_reserved_payouts_once(None).await;
        r.reconcile_reserved_payouts_once(None).await;
        assert_eq!(db.pps_invariant().await.unwrap(), before);
        assert_eq!(db.get_reserved_pps_attempts(None).await.unwrap().len(), 0);
        wallet.assert_no_wallet_mutation();
        assert!(db.get_stale_payout_attempts(0).await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn pps_not_found_after_expiry_never_uses_legacy_auto_refund() {
    let (db, pool, policy, miner) = fixture(ADDRESS).await;
    let attempt = db.create_payout_attempt(1, CREDIT, "loop").await.unwrap();
    reserve_sealed(&db, &policy, attempt, miner, CREDIT, Some(TXID)).await;
    db.update_payout_attempt(attempt, "sent", Some("synthetic-op"), Some(TXID), None)
        .await
        .unwrap();
    sqlx::query("UPDATE payout_attempts SET created_at=datetime('now','-180 minutes') WHERE id=?1")
        .bind(attempt)
        .execute(&pool)
        .await
        .unwrap();
    let wallet = mock_rpc(HashMap::new()).await;
    let node = mock_rpc(HashMap::new()).await;
    let mut r = crate::reconciler::tests::reconciler(db.clone(), &node, &wallet);
    r.pps_policy = Some(policy);
    r.pps_gate = Some(gate(&node, Utc::now().timestamp()));
    r.reconcile_reserved_payouts_once(None).await;
    let summary = db.pps_invariant().await.unwrap();
    assert_eq!(
        (
            summary.pending_zatoshis,
            summary.paying_zatoshis,
            summary.paid_zatoshis
        ),
        (0, CREDIT, 0)
    );
}

#[tokio::test]
async fn pps_seeded_sealed_exact_txid_recovery_confirms_once_without_wallet_send() {
    let (db, _pool, policy, miner) = fixture(ADDRESS).await;
    let attempt = db.create_payout_attempt(1, CREDIT, "loop").await.unwrap();
    reserve_sealed(&db, &policy, attempt, miner, CREDIT, Some(TXID)).await;
    db.update_payout_attempt(attempt, "sent", Some("synthetic-op"), None, None)
        .await
        .unwrap();
    let wallet = wallet(serde_json::json!({"txids": [TXID]})).await;
    let node = mock_rpc(HashMap::from([(
        "getrawtransaction",
        serde_json::json!({"height": 1}),
    )]))
    .await;
    let mut r = crate::reconciler::tests::reconciler(db.clone(), &node, &wallet);
    r.pps_policy = Some(policy);
    r.pps_gate = Some(gate(&node, Utc::now().timestamp()));
    r.reconcile_reserved_payouts_once(None).await;
    r.reconcile_reserved_payouts_once(None).await;
    wallet.assert_no_wallet_mutation();
    let summary = db.pps_invariant().await.unwrap();
    assert_eq!(
        (
            summary.pending_zatoshis,
            summary.paying_zatoshis,
            summary.paid_zatoshis
        ),
        (0, 0, CREDIT)
    );
}

#[tokio::test]
async fn pps_stale_lease_and_invariant_corruption_block_payouts() {
    let (db, pool, policy, miner) = fixture(ADDRESS).await;
    let wallet = wallet(serde_json::json!({"txid": TXID})).await;
    let node = mock_rpc(HashMap::new()).await;
    let stale = gate(&node, Utc::now().timestamp() - 91);
    assert!(pay_with_gate(&db, &wallet, &node, &policy, &stale)
        .await
        .is_err());
    let attempts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payout_attempts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(attempts, 0);
    sqlx::query("UPDATE pps_accounts SET pending=pending+1 WHERE miner_id=?1")
        .bind(miner)
        .execute(&pool)
        .await
        .unwrap();
    assert!(pay(&db, &wallet, &node, &policy).await.is_err());
    let mut r = crate::reconciler::tests::reconciler(db, &node, &wallet);
    r.pps_policy = Some(policy);
    r.pps_gate = Some(gate(&node, Utc::now().timestamp()));
    let sweep = r.sweep_once().await.unwrap();
    assert!(sweep
        .alerts
        .iter()
        .any(|a| a.contains("PPS exact accounting invariant FAILED")));
}

#[tokio::test]
async fn pps_unsupported_wallet_with_invalid_recipient_never_redirects_credit() {
    let (db, pool, policy, miner) = fixture("invalid-synthetic-address").await;
    sqlx::query("UPDATE miners SET created_at=datetime('now','-3 days') WHERE id=?1")
        .bind(miner)
        .execute(&pool)
        .await
        .unwrap();
    let wallet = wallet(serde_json::json!({"txid": TXID})).await;
    let node = mock_rpc(HashMap::new()).await;
    assert!(pay(&db, &wallet, &node, &policy).await.is_err());
    wallet.assert_no_wallet_mutation();
    assert_eq!(db.pps_invariant().await.unwrap().pending_zatoshis, CREDIT);
    let attempts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payout_attempts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(attempts, 0);
}

#[tokio::test]
async fn pps_low_balance_or_unsupported_wallet_preserves_unspent_credit() {
    let (db, pool, policy, _miner) = fixture(ADDRESS).await;
    let node = mock_rpc(HashMap::new()).await;
    let low_wallet = mock_rpc(HashMap::from([(
        "z_gettotalbalance",
        serde_json::json!({"private":"0.1", "total":"0.1"}),
    )]))
    .await;
    assert!(pay(&db, &low_wallet, &node, &policy).await.is_err());
    let attempts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payout_attempts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(attempts, 0);
    // Neither wallet advertises the mandatory PCZT contract. No send or
    // reservation is attempted, irrespective of its reported balance.
    let failing_wallet = mock_rpc(HashMap::from([(
        "z_gettotalbalance",
        serde_json::json!({"private":"100.0", "total":"100.0"}),
    )]))
    .await;
    assert!(pay(&db, &failing_wallet, &node, &policy).await.is_err());
    let summary = db.pps_invariant().await.unwrap();
    assert_eq!(
        (
            summary.pending_zatoshis,
            summary.paying_zatoshis,
            summary.paid_zatoshis
        ),
        (CREDIT, 0, 0)
    );
    assert!(db.get_reserved_pps_attempts(None).await.unwrap().is_empty());
}

#[tokio::test]
async fn pps_seeded_sealed_proposal_without_txid_is_parked_across_recovery() {
    let (db, _pool, policy, miner) = fixture(ADDRESS).await;
    let attempt = db
        .create_payout_attempt(1, CREDIT, "fixture-sealed")
        .await
        .unwrap();
    reserve_sealed(&db, &policy, attempt, miner, CREDIT, None).await;
    db.update_payout_attempt(attempt, "submitting", None, None, None)
        .await
        .unwrap();
    let wallet = wallet(serde_json::json!({"txids":[TXID,"d".repeat(64)]})).await;
    let node = mock_rpc(HashMap::new()).await;
    let before = db.pps_invariant().await.unwrap();
    let mut r = crate::reconciler::tests::reconciler(db.clone(), &node, &wallet);
    r.pps_policy = Some(policy);
    r.pps_gate = Some(gate(&node, Utc::now().timestamp()));
    r.reconcile_reserved_payouts_once(None).await;
    r.reconcile_reserved_payouts_once(None).await;
    assert_eq!(db.pps_invariant().await.unwrap(), before);
    assert_eq!(before.paying_zatoshis, CREDIT);
    assert!(db.refund_pps_payout(attempt).await.is_err());
    assert_eq!(
        db.pps_funding_snapshot()
            .await
            .unwrap()
            .reserved_fees_zatoshis,
        10_000
    );
    wallet.assert_no_wallet_mutation();
}

#[tokio::test]
async fn pps_seeded_proven_presign_failure_releases_only_its_fee_and_principal() {
    let (db, _pool, policy, miner) = fixture(ADDRESS).await;
    db.credit_balance(miner, 123).await.unwrap();
    let attempt = db
        .create_payout_attempt(1, CREDIT, "fixture-presign")
        .await
        .unwrap();
    db.reserve_pps_payout(
        attempt,
        &[(miner, CREDIT)],
        &funding(&db, &policy).await,
        &fee(attempt),
    )
    .await
    .unwrap();
    assert_eq!(db.refund_pps_payout(attempt).await.unwrap(), 1);
    assert_eq!(db.refund_pps_payout(attempt).await.unwrap(), 0);
    let state = db.pps_invariant().await.unwrap();
    assert_eq!(
        (
            state.pending_zatoshis,
            state.paying_zatoshis,
            state.paid_zatoshis
        ),
        (CREDIT, 0, 0)
    );
    let funding = db.pps_funding_snapshot().await.unwrap();
    assert_eq!(
        (funding.paid_fees_zatoshis, funding.reserved_fees_zatoshis),
        (0, 0)
    );
    assert_eq!(db.get_or_create_balance(miner).await.unwrap().pending, 123);
}
