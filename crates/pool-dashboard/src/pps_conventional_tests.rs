//! Isolated SQLite + ephemeral loopback RPC state-flow tests. The raw transaction
//! has synthetic effect data and dummy proofs/signatures: the mock canonical
//! node, NOT a real network, supplies inclusion. These tests establish runtime
//! accounting/recovery behavior, not successful wallet proving or broadcast.
use crate::{pps_conventional, pps_gate::PpsGate};
use chrono::Utc;
use node_rpc::ZcashRpcClient;
use pool_core::pps_funding::PpsFundingRoute;
use pool_db::{
    pps_funding::PpsFundingLease,
    pps_live::{PpsChainLease, PpsCredit, PPS_SCALE},
    pps_policy::PpsPolicy,
    PoolDb,
};
use serde_json::{json, Value};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteSynchronous},
    Row,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

const SOURCE: &str =
    "ztestsapling1qyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszy8qa3d";
const RECIPIENT: &str = "tm9ty64b2UE2PWqVH1NN7hBmZr27U771NKY";
const SAPLING_RECIPIENT: &str = "ztestsapling1ywvgdtat0cemx5y6ejpu5wapc5x2j0c08f9lee3fd9s6wvv6n079w5nplr48qne73w6swec4vzv";
const UNIFIED_RECIPIENT: &str = "utest1rak2faln6pat6jx7rmfulvm80c0mjcnj5z2zvsynqn0zhu9gtk97ll5cyvu7maglwgazje4t00958n2yyadc8ee2vskkmg0e7wscqxaaahke023r8pejc097tf0e5zu6ltq9g6f99xxtpfprujl4uhaph3yj7mu52w3da6x0lgj3j0qy";
const SHIELDED_TXID: &str = "07ed436dda2cc05b9170b2db927992f1c9f984295a60533aaef787126fc0b145";
const SHIELDED_BAD_FEE_TXID: &str = "8256f1fec4de46144b96085cb5d8043003b1315795f158a5ceb01e50e6301845";
const TXID: &str = "afd4a3eb9c392a8c80290b7ea50decd075b8529ae9856faa470f909aeb4c64e6";
const BAD_FEE_TXID: &str = "e98bfe62bc0ca2f7b999b7ebe106d4cf965742e5fa378111923c68354c078449";
const CREDIT: i64 = 1_000_000;
const HEIGHT: u32 = 2_000_000;

fn route() -> PpsFundingRoute {
    PpsFundingRoute::ZecdConventionalTestnet {
        hold_new_legacy_sends: true,
    }
}
fn chain_lease() -> PpsChainLease {
    chain_lease_at(Utc::now().timestamp())
}
fn chain_lease_at(now: i64) -> PpsChainLease {
    PpsChainLease {
        network: "testnet".into(),
        checked_at_unix: now,
        valid_until_unix: now + 90,
        agreeing_references: 2,
        disagreement: false,
    }
}
fn gate(rpc: &Arc<ZcashRpcClient>) -> PpsGate {
    PpsGate::synthetic(rpc.clone(), "testnet", chain_lease())
}
async fn funding(db: &PoolDb, p: &PpsPolicy) -> PpsFundingLease {
    let s = db
        .pps_funding_snapshot_for_epoch(&p.epoch_config())
        .await
        .unwrap();
    let now = Utc::now().timestamp();
    PpsFundingLease {
        network: "testnet".into(),
        checked_at_unix: now,
        valid_until_unix: now + 60,
        spendable_zatoshis: 200_000_000_000,
        reserve_floor_zatoshis: p.reserve_min_zatoshis,
        reserved_fee_allowance_zatoshis: p.fee_allowance_zatoshis,
        generation: s.generation,
    }
}

// The runtime has one process-wide conventional WORKFLOW. Serialize complete
// synthetic fixtures before starting their unchanged 10-second operation
// deadlines, so those deadlines do not include unrelated test-fixture queues.
static FIXTURE_LIFETIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct Fixture {
    db: PoolDb,
    pool: sqlx::SqlitePool,
    policy: PpsPolicy,
    miner: i64,
    dir: PathBuf,
    // Last field: release only after fixture cleanup and the other fields drop.
    _serial: tokio::sync::MutexGuard<'static, ()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
async fn open(path: &std::path::Path) -> (PoolDb, sqlx::SqlitePool) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .synchronous(SqliteSynchronous::Full),
        )
        .await
        .unwrap();
    (PoolDb::new(pool.clone()), pool)
}
impl Fixture {
    async fn new() -> Self {
        Self::new_for_recipient(RECIPIENT).await
    }
    async fn new_for_recipient(recipient: &str) -> Self {
        use std::os::unix::fs::DirBuilderExt;
        static ID: AtomicU64 = AtomicU64::new(0);
        let serial = FIXTURE_LIFETIME.lock().await;
        let dir = std::env::temp_dir().join(format!(
            "pps-conventional-test-{}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let (db, pool) = open(&dir.join("synthetic.sqlite")).await;
        db.run_migrations().await.unwrap();
        let policy = PpsPolicy {
            network: "testnet".into(),
            epoch: "conventional-test-1".into(),
            fee_bps: 0,
            max_liability_zatoshis: 95_000_000_000,
            total_exposure_zatoshis: 100_000_000_000,
            fee_allowance_zatoshis: 5_000_000_000,
            reserve_min_zatoshis: 1,
            max_payout_zatoshis: CREDIT,
        };
        db.initialize_pps_epoch(&policy.epoch_config(), Some(&funding(&db, &policy).await))
            .await
            .unwrap();
        let miner = db.get_or_create_miner(recipient).await.unwrap();
        let worker = db
            .get_or_create_worker(miner.id, "synthetic-worker")
            .await
            .unwrap();
        let now = Utc::now().timestamp();
        db.credit_pps_share(
            &policy.epoch_config(),
            &PpsCredit {
                proof_id: "b".repeat(64),
                quote_id: "c".repeat(64),
                worker_id: worker.id,
                job_id: "synthetic-job".into(),
                session_id: "synthetic-session".into(),
                difficulty: 1.0,
                is_block: false,
                amount_subzatoshis: CREDIT as u128 * PPS_SCALE,
                accepted_at_unix: now,
                quote_height: HEIGHT as u64,
                network_target_be: [1; 32],
                assigned_share_target_be: [2; 32],
                miner_subsidy_zats: 125_000_000,
            },
            // One sampled clock binds the synthetic accepted share and its
            // chain proof even if fixture setup crosses a wall-clock second.
            Some(&chain_lease_at(now)),
            Some(&funding(&db, &policy).await),
            now,
        )
        .await
        .unwrap();
        db.credit_balance(miner.id, 12_345).await.unwrap();
        Self {
            db,
            pool,
            policy,
            miner: miner.id,
            dir,
            _serial: serial,
        }
    }
    async fn reopen(&mut self) {
        self.pool.close().await;
        let (db, pool) = open(&self.dir.join("synthetic.sqlite")).await;
        self.db = db;
        self.pool = pool;
    }
    async fn legacy(&self) -> (i64, i64, i64) {
        let row = sqlx::query("SELECT pending,paying,paid FROM balances WHERE miner_id=?1")
            .bind(self.miner)
            .fetch_one(&self.pool)
            .await
            .unwrap();
        (row.get(0), row.get(1), row.get(2))
    }
    async fn attempt(&self) -> pool_db::pps_funding::PpsConventionalAttempt {
        let id: i64 = sqlx::query_scalar("SELECT MAX(attempt_id) FROM pps_conventional_attempts")
            .fetch_one(&self.pool)
            .await
            .unwrap();
        self.db
            .get_pps_conventional_attempt(id)
            .await
            .unwrap()
            .unwrap()
    }
}

#[tokio::test]
async fn conventional_fixture_share_and_chain_proof_use_one_sampled_clock() {
    use pool_db::pps_live::PpsDbError;
    let f = Fixture::new().await;
    let worker = f.db.get_or_create_worker(f.miner,"clock-boundary-worker").await.unwrap();
    let observed = Utc::now().timestamp();
    // Deterministically model accepted_at just before a second boundary and
    // a separately sampled chain proof just after it, without sleeping.
    let accepted = observed - 1;
    let credit = PpsCredit { proof_id:"d".repeat(64),quote_id:"e".repeat(64),
        worker_id:worker.id,job_id:"clock-boundary-job".into(),session_id:"clock-boundary-session".into(),
        difficulty:1.0,is_block:false,amount_subzatoshis:CREDIT as u128 * PPS_SCALE,
        accepted_at_unix:accepted,quote_height:HEIGHT as u64,network_target_be:[1;32],
        assigned_share_target_be:[2;32],miner_subsidy_zats:125_000_000 };
    let proof = funding(&f.db,&f.policy).await;
    let incoherent = f.db.credit_pps_share(&f.policy.epoch_config(),&credit,
        Some(&chain_lease_at(observed)),Some(&proof),observed).await;
    assert!(matches!(incoherent,Err(PpsDbError::ChainLeaseRequired)));
    assert_eq!(f.db.pps_invariant().await.unwrap().pending_zatoshis,CREDIT);
    let coherent = chain_lease_at(accepted);
    assert_eq!(coherent.checked_at_unix,credit.accepted_at_unix);
    assert_eq!(coherent.valid_until_unix - coherent.checked_at_unix,90);
    f.db.credit_pps_share(&f.policy.epoch_config(),&credit,Some(&coherent),Some(&proof),observed)
        .await.unwrap();
    assert_eq!(f.db.pps_invariant().await.unwrap().pending_zatoshis,2 * CREDIT);
}

// Reproduced by tests/support/zecd_raw_fixture.rs using the upstream parser.
fn synthetic_raw(fee: i64) -> String {
    let point =
        hex::decode("a9cb0d137232ff8448d0f078b6814c66cb331b0f2d3d8a085bedba815f00a8db").unwrap();
    let mut raw = Vec::new();
    for value in [0x80000005u32, 0x26a7270a, 0xc2d6d0b4, 0, HEIGHT + 40] {
        raw.extend(value.to_le_bytes());
    }
    raw.extend([0, 1]);
    raw.extend((CREDIT as u64).to_le_bytes());
    raw.extend([25, 0x76, 0xa9, 0x14]);
    raw.extend([2; 20]);
    raw.extend([0x88, 0xac]);
    raw.push(1);
    raw.extend(&point);
    raw.extend([0; 32]);
    raw.extend(&point);
    raw.push(0);
    raw.extend((CREDIT + fee).to_le_bytes());
    raw.extend([0; 32]);
    raw.extend([0; 192]);
    raw.extend([0; 64]);
    raw.extend([0; 64]);
    raw.push(0);
    hex::encode(raw)
}

// Same V5 Sapling effect-data fixture as node-rpc's shielded_receipt_fixture:
// one spend, one output, no transparent outputs, and a 10,000-zatoshi fee.
// Ciphertexts/proofs/signatures are intentionally dummy bytes. The synthetic
// sender-wallet view supplies an asserted amount; this is not a decrypt proof.
fn synthetic_shielded_raw() -> String {
    synthetic_shielded_raw_with_fee(10_000)
}
fn synthetic_shielded_raw_with_fee(fee: i64) -> String {
    let point =
        hex::decode("a9cb0d137232ff8448d0f078b6814c66cb331b0f2d3d8a085bedba815f00a8db").unwrap();
    let mut raw = Vec::new();
    for value in [0x80000005u32, 0x26a7270a, 0xc2d6d0b4, 0, HEIGHT + 40] {
        raw.extend(value.to_le_bytes());
    }
    raw.extend([0, 0, 1]);
    raw.extend(&point);
    raw.extend([0; 32]);
    raw.extend(&point);
    raw.push(1);
    raw.extend(&point);
    raw.extend([0; 32]);
    raw.extend(&point);
    raw.extend([0; 580]);
    raw.extend([0; 80]);
    raw.extend(fee.to_le_bytes());
    raw.extend([0; 32]);
    raw.extend([0; 192]);
    raw.extend([0; 64]);
    raw.extend([0; 192]);
    raw.extend([0; 64]);
    raw.push(0);
    hex::encode(raw)
}

struct MockState {
    calls: Vec<String>,
    send_error: bool,
    invalid_opid: bool,
    operation: Value,
    missing_inclusion: bool,
    wrong_chain: bool,
    bad_raw: bool,
    confirmations: u64,
    fail_second_funding: bool,
    funding_reads: usize,
    saw_durable_seal: bool,
    expected_amount: i64,
    actual_fee: i64,
    recipient: String,
    raw_override: Option<(String, String)>,
    wallet_history: Option<Value>,
    history_readiness: Option<Value>,
}
struct MockRpc {
    client: Arc<ZcashRpcClient>,
    endpoint: String,
    state: Arc<Mutex<MockState>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for MockRpc {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl MockRpc {
    async fn new(db: &PoolDb) -> Self {
        use axum::{extract::State, routing::post, Json, Router};
        type StateData = (Arc<Mutex<MockState>>, PoolDb);
        async fn handler(
            State((state, db)): State<StateData>,
            Json(req): Json<Value>,
        ) -> Json<Value> {
            let method = req["method"].as_str().unwrap_or("");
            let params = &req["params"];
            state.lock().unwrap().calls.push(method.into());
            if method == "z_sendmany" {
                let rows = db
                    .get_reserved_pps_conventional_attempts(100)
                    .await
                    .unwrap();
                let sum = db.pps_invariant().await.unwrap();
                let mut st = state.lock().unwrap();
                st.saw_durable_seal = rows.len() == 1
                    && rows[0].sealed
                    && sum.paying_zatoshis == CREDIT
                    && rows[0].fee_upper_bound_zatoshis == 28_410_000;
            }
            let mut st = state.lock().unwrap();
            let hash = "a".repeat(64);
            let (txid, raw_hex) = if let Some((txid,raw)) = &st.raw_override {
                (txid.clone(),raw.clone())
            } else {
                (if st.actual_fee == 10_000 {TXID}else{BAD_FEE_TXID}.to_string(),
                    synthetic_raw(st.actual_fee))
            };
            let result = match method {
                "getnetworkinfo" => json!({"version":700,"subversion":"/zecd:0.7.0/"}),
                "getblockchaininfo" => json!({"chain":"test","blocks":HEIGHT-1,"headers":HEIGHT-1,
                    "initialblockdownload":false,"bestblockhash":hash}),
                "getwalletinfo" => {
                    let submitted=st.calls.iter().any(|m|m == "z_sendmany");
                    if submitted && st.history_readiness.is_some() {
                        st.history_readiness.clone().unwrap()
                    } else {
                        json!({"walletname":"synthetic-wallet","walletversion":169900,
                            "format":"sqlite","private_keys_enabled":true,"scanning":false,
                            "enhanced_through":if submitted {HEIGHT}else{HEIGHT-1},
                            "unlocked_until":Utc::now().timestamp()+3600})
                    }
                }
                "getaddressinfo" => json!({"address":SOURCE,"ismine":true,"solvable":true,
                    "iswatchonly":false,"isscript":false,"scriptPubKey":"","receiver_types":["sapling"]}),
                "getbalance" => {
                    st.funding_reads += 1;
                    if st.fail_second_funding && st.funding_reads >= 2 {
                        json!(0)
                    } else {
                        json!(2000)
                    }
                }
                "listunspent" => {
                    json!([{"pool":"orchard","txid":"d".repeat(64),"vout":0,"address":SOURCE,
                    "amount":2000,"confirmations":10,"safe":true,"spendable":true,"solvable":true}])
                }
                "getblockcount" => json!(if st.calls.iter().any(|m| m == "z_sendmany") {
                    HEIGHT
                } else {
                    HEIGHT - 1
                }),
                "getblockhash" => {
                    if st.wrong_chain && params[0].as_u64() == Some(HEIGHT as u64) {
                        json!("b".repeat(64))
                    } else {
                        json!(hash)
                    }
                }
                "z_getoperationstatus" => {
                    if params.as_array().is_some_and(|p| p.is_empty()) {
                        json!([])
                    } else {
                        st.operation.clone()
                    }
                }
                "z_sendmany" => {
                    assert_eq!(params[0], SOURCE);
                    assert_eq!(params[1].as_array().unwrap().len(), 1);
                    assert_eq!(params[1][0]["address"], st.recipient);
                    assert_eq!(
                        params[1][0]["amount"].to_string(),
                        format!(
                            "{}.{:08}",
                            st.expected_amount / 100_000_000,
                            st.expected_amount % 100_000_000
                        )
                    );
                    assert_eq!(params[2], 10);
                    assert!(params[3].is_null());
                    assert_eq!(params[4], "AllowRevealedRecipients");
                    if st.send_error {
                        return Json(json!({"jsonrpc":"2.0","id":req["id"],"result":null,
                        "error":{"code":-1,"message":"synthetic ambiguous failure after send"}}));
                    }
                    if st.invalid_opid {
                        json!({"bad":"operation"})
                    } else {
                        json!("opid-synthetic-1")
                    }
                }
                "getrawtransaction" => {
                    json!({"txid":txid,"hex":if st.bad_raw {format!("{raw_hex}00")}else{raw_hex},
                    "confirmations":st.confirmations,"blockhash":hash})
                }
                "gettransaction" => {
                    assert_eq!(params,&json!([txid]));
                    st.wallet_history.clone().unwrap_or(Value::Null)
                }
                "getblock" => json!({"hash":hash,"height":HEIGHT,"confirmations":st.confirmations,
                    "tx":if st.missing_inclusion {json!([])}else{json!([txid])}}),
                _ => {
                    return Json(json!({"jsonrpc":"2.0","id":req["id"],"result":null,
                    "error":{"code":-32601,"message":"unsupported synthetic method"}}))
                }
            };
            Json(json!({"jsonrpc":"2.0","id":req["id"],"result":result,"error":null}))
        }
        let state = Arc::new(Mutex::new(MockState {
            calls: vec![],
            send_error: false,
            invalid_opid: false,
            operation: json!([{"id":"opid-synthetic-1","status":"executing"}]),
            missing_inclusion: false,
            wrong_chain: false,
            bad_raw: false,
            confirmations: crate::pps_conventional::PPS_SETTLE_MATURITY,
            fail_second_funding: false,
            funding_reads: 0,
            saw_durable_seal: false,
            expected_amount: CREDIT,
            actual_fee: 10_000,
            recipient: RECIPIENT.into(),
            raw_override: None,
            wallet_history: None,
            history_readiness: None,
        }));
        let listener =
            tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
                .await
                .unwrap();
        let endpoint = format!(
            "http://{}",
            listener.local_addr().unwrap()
        );
        let client = Arc::new(ZcashRpcClient::new(&endpoint));
        let app = Router::new()
            .route("/", post(handler))
            .with_state((state.clone(), db.clone()));
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            client,
            endpoint,
            state,
            task,
        }
    }
    fn success(&self) {
        let mut state = self.state.lock().unwrap();
        let txid = if let Some((txid,_)) = &state.raw_override {
            txid.as_str()
        } else if state.actual_fee == 10_000 {
            TXID
        } else {
            BAD_FEE_TXID
        };
        state.operation = json!([
            {"id":"opid-synthetic-1","status":"success","result":{"txids":[txid]}}]);
    }
    fn shielded(&self, recipient: &str) {
        let mut state = self.state.lock().unwrap();
        let raw = synthetic_shielded_raw();
        let fee: Value = serde_json::from_str("-0.00010000").unwrap();
        let amount: Value = serde_json::from_str("-0.01000000").unwrap();
        state.recipient = recipient.into();
        state.raw_override = Some((SHIELDED_TXID.into(),raw.clone()));
        state.wallet_history = Some(json!({"txid":SHIELDED_TXID,"hex":raw,
            "confirmations":1,"blockhash":"a".repeat(64),"fee":fee,
            "details":[{"category":"send","address":SAPLING_RECIPIENT,
                "amount":amount,"vout":0,"pool":"sapling","abandoned":false,"fee":fee}]}));
    }
    fn sends(&self) -> usize {
        self.state
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|m| *m == "z_sendmany")
            .count()
    }
    async fn failure_diagnostics(&self) -> String {
        let methods:Vec<_> = self.state.lock().unwrap().calls.iter().rev().take(8).cloned().collect();
        // The bounded production RPC deliberately suppresses transport details.
        // A separate read-only synthetic probe can supply secondary evidence
        // without printing endpoints, request bodies or raw error strings.
        let probe = ZcashRpcClient::new(&self.endpoint);
        let transport = match tokio::time::timeout(Duration::from_secs(2),probe.get_block_count()).await {
            Ok(Ok(_)) => "fresh_probe_ok".to_string(),
            Ok(Err(node_rpc::RpcError::Http(error))) => {
                let mut source:Option<&(dyn std::error::Error + 'static)> = Some(&error);
                let mut os_code = None;
                while let Some(cause) = source {
                    if let Some(io) = cause.downcast_ref::<std::io::Error>() { os_code=io.raw_os_error(); }
                    source = cause.source();
                }
                format!("fresh_probe_http(connect={},timeout={},body={},decode={},os={os_code:?})",
                    error.is_connect(),error.is_timeout(),error.is_body(),error.is_decode())
            }
            Ok(Err(_)) => "fresh_probe_rpc_error".to_string(),
            Err(_) => "fresh_probe_deadline".to_string(),
        };
        format!("recent_rpc_methods={methods:?}; {transport}")
    }
    fn forbidden_calls(&self) -> usize {
        self.state
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|m| {
                matches!(
                    m.as_str(),
                    "pczt_create"
                        | "pczt_prove"
                        | "pczt_sign"
                        | "pczt_extract"
                        | "sendrawtransaction"
                        | "z_getoperationresult"
                )
            })
            .count()
    }
}
async fn process(f: &Fixture, rpc: &MockRpc) -> anyhow::Result<usize> {
    tokio::time::timeout(
        Duration::from_secs(10),
        pps_conventional::process(
            &f.db,
            &rpc.client,
            &rpc.client,
            SOURCE,
            1,
            &f.policy,
            &gate(&rpc.client),
            &route(),
        ),
    )
    .await
    .unwrap()
}
async fn reconcile(f: &Fixture, rpc: &MockRpc, attempt: i64) -> anyhow::Result<usize> {
    tokio::time::timeout(
        Duration::from_secs(10),
        pps_conventional::reconcile_one(
            &f.db,
            &rpc.client,
            &rpc.client,
            &f.policy,
            &gate(&rpc.client),
            attempt,
        ),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn below_minimum_resets_payout_health_without_hiding_blocked_credit_admission() {
    use pool_core::pps_credit_health::{decode_credit_health, CreditAdmissionState};
    let mut f = Fixture::new().await;
    f.policy.max_payout_zatoshis = CREDIT + 1;
    let rpc = MockRpc::new(&f.db).await;
    let legacy = f.legacy().await;
    let now = Utc::now().timestamp();
    let mut blocked = decode_credit_health(None, now);
    blocked.state = CreditAdmissionState::Paused;
    blocked.category = "funding_missing".into();
    blocked.last_refresh_result = "wallet_not_ready".into();
    blocked.last_refresh_stage = "opening_metadata_check".into();
    let raw = serde_json::to_string(&blocked).unwrap();
    let (result, report) = crate::payout_health::observe(pps_conventional::process(
        &f.db, &rpc.client, &rpc.client, SOURCE, CREDIT + 1, &f.policy,
        &gate(&rpc.client), &route(),
    )).await;
    assert_eq!(result.unwrap(), 0);
    assert_eq!(report.outcome, "no_payout_due");
    assert_eq!(report.funding_check, "not_checked_no_payout_due");
    let (mut failures, mut error) = (3, "synthetic previous hold".to_owned());
    crate::payout_health::clear_successful_cycle(&mut failures, &mut error);
    assert_eq!((failures, error.as_str()), (0, ""));
    let presented = pool_api::credit_health::present(true, Some(&raw), now).unwrap();
    assert_eq!(presented.state, CreditAdmissionState::Paused);
    assert_eq!(presented.last_refresh_result, "wallet_not_ready");
    assert!(rpc.state.lock().unwrap().calls.is_empty(), "idle path must not query the wallet");
    let attempts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payout_attempts")
        .fetch_one(&f.pool).await.unwrap();
    assert_eq!(attempts, 0);
    let sum = f.db.pps_invariant().await.unwrap();
    assert_eq!((sum.pending_zatoshis, sum.paying_zatoshis, sum.paid_zatoshis), (CREDIT, 0, 0));
    assert_eq!(f.legacy().await, legacy);
}

#[tokio::test]
async fn conventional_reserve_seal_once_send_and_exact_confirmed_settlement() {
    let f = Fixture::new().await;
    let rpc = MockRpc::new(&f.db).await;
    rpc.success();
    let legacy = f.legacy().await;
    assert_eq!(process(&f, &rpc).await.unwrap(), 1);
    assert!(rpc.state.lock().unwrap().saw_durable_seal);
    assert_eq!(rpc.sends(), 1);
    assert_eq!(rpc.forbidden_calls(), 0);
    assert!(!rpc.state.lock().unwrap().calls.iter().any(|m|m == "gettransaction"));
    let sum = f.db.pps_invariant().await.unwrap();
    assert_eq!(
        (sum.pending_zatoshis, sum.paying_zatoshis, sum.paid_zatoshis),
        (0, 0, CREDIT)
    );
    let money = f.db.pps_funding_snapshot().await.unwrap();
    assert_eq!(
        (money.paid_fees_zatoshis, money.reserved_fees_zatoshis),
        (10_000, 0)
    );
    assert_eq!(f.legacy().await, legacy);
    assert_eq!(process(&f, &rpc).await.unwrap(), 0);
    assert_eq!(rpc.sends(), 1);
}

#[tokio::test]
async fn conventional_settlement_waits_for_maturity_depth() {
    use crate::pps_conventional::PPS_SETTLE_MATURITY;
    let f = Fixture::new().await;
    let rpc = MockRpc::new(&f.db).await;
    rpc.success();
    // The send lands but is only one block deep: reserved, not settled.
    rpc.state.lock().unwrap().confirmations = 1;
    assert_eq!(process(&f, &rpc).await.unwrap(), 0);
    assert_eq!(rpc.sends(), 1);
    let sum = f.db.pps_invariant().await.unwrap();
    assert_eq!(
        (sum.pending_zatoshis, sum.paying_zatoshis, sum.paid_zatoshis),
        (0, CREDIT, 0)
    );
    let attempt = f.attempt().await;
    // One short of the threshold: still held in `paying`.
    rpc.state.lock().unwrap().confirmations = PPS_SETTLE_MATURITY - 1;
    assert_eq!(reconcile(&f, &rpc, attempt.attempt_id).await.unwrap(), 0);
    assert_eq!(f.db.pps_invariant().await.unwrap().paying_zatoshis, CREDIT);
    // At the threshold: settles exactly once, and no resend ever happens.
    rpc.state.lock().unwrap().confirmations = PPS_SETTLE_MATURITY;
    assert_eq!(reconcile(&f, &rpc, attempt.attempt_id).await.unwrap(), 1);
    let sum = f.db.pps_invariant().await.unwrap();
    assert_eq!(
        (sum.pending_zatoshis, sum.paying_zatoshis, sum.paid_zatoshis),
        (0, 0, CREDIT)
    );
    assert_eq!(reconcile(&f, &rpc, attempt.attempt_id).await.unwrap(), 0);
    assert_eq!(rpc.sends(), 1);
}

#[tokio::test]
async fn conventional_unified_and_bare_shielded_wallet_view_settles_exactly_once() {
    for recipient in [UNIFIED_RECIPIENT,SAPLING_RECIPIENT] {
        let f = Fixture::new_for_recipient(recipient).await;
        let rpc = MockRpc::new(&f.db).await;
        rpc.shielded(recipient);
        rpc.success();
        let legacy = f.legacy().await;
        assert_eq!(process(&f,&rpc).await.unwrap(),1);
        let attempt = f.attempt().await;
        assert_eq!(attempt.intent().unwrap().unwrap().items[0].address,recipient);
        assert_eq!(f.db.pps_invariant().await.unwrap().paid_zatoshis,CREDIT);
        assert_eq!(f.db.pps_funding_snapshot().await.unwrap().paid_fees_zatoshis,10_000);
        assert_eq!(reconcile(&f,&rpc,attempt.attempt_id).await.unwrap(),0);
        assert_eq!(process(&f,&rpc).await.unwrap(),0);
        assert_eq!(rpc.sends(),1);
        assert_eq!(rpc.forbidden_calls(),0);
        assert_eq!(f.legacy().await,legacy);
        assert_eq!(rpc.state.lock().unwrap().calls.iter().filter(|m|*m == "gettransaction").count(),1);
    }
}

#[tokio::test]
async fn conventional_shielded_incomplete_history_holds_then_recovers_persisted_unified_intent() {
    // Every failure occurs after the one-shot seal. Completing enhancement or
    // recovering the matching pool-specific receiver view may settle that same
    // intent, but never creates another send or releases its reservation.
    for failure in 0..10 {
        let mut f = Fixture::new_for_recipient(UNIFIED_RECIPIENT).await;
        let rpc = MockRpc::new(&f.db).await;
        rpc.shielded(UNIFIED_RECIPIENT);
        rpc.success();
        let valid = rpc.state.lock().unwrap().wallet_history.clone().unwrap();
        {
            let mut state = rpc.state.lock().unwrap();
            match failure {
                0 => state.history_readiness = Some(json!({"scanning":false,"enhanced_through":HEIGHT-1})),
                1 => state.history_readiness = Some(json!({"scanning":true,"enhanced_through":HEIGHT})),
                2 => state.wallet_history = None,
                3 => state.wallet_history.as_mut().unwrap()["blockhash"] = json!("b".repeat(64)),
                4 => state.wallet_history.as_mut().unwrap()["hex"] = json!(format!("{}00",synthetic_shielded_raw())),
                5 => state.wallet_history.as_mut().unwrap()["txid"] = json!("c".repeat(64)),
                6 => state.wallet_history.as_mut().unwrap()["details"] = json!([]),
                7 => state.wallet_history.as_mut().unwrap()["details"][0]["address"] = json!(UNIFIED_RECIPIENT),
                8 => state.wallet_history.as_mut().unwrap()["fee"] = serde_json::from_str("-0.00010001").unwrap(),
                _ => state.wallet_history.as_mut().unwrap()["confirmations"] = json!(0),
            }
        }
        assert!(process(&f,&rpc).await.is_err());
        let before = f.attempt().await;
        assert!(before.sealed && before.halt_category.is_none(),
            "synthetic recovery setup case {failure}: sealed={}, halted={}; {}",
            before.sealed,before.halt_category.is_some(),rpc.failure_diagnostics().await);
        assert_eq!(before.expected_txid.as_deref(),Some(SHIELDED_TXID));
        assert_eq!(f.db.pps_invariant().await.unwrap().paying_zatoshis,CREDIT);
        assert_eq!(f.db.pps_funding_snapshot().await.unwrap().reserved_fees_zatoshis,28_410_000);
        assert!(f.db.refund_pps_payout(before.attempt_id).await.is_err());
        assert!(process(&f,&rpc).await.is_err());
        if failure < 2 {
            assert!(!rpc.state.lock().unwrap().calls.iter().any(|m|m == "gettransaction"));
        }
        f.reopen().await;
        assert_eq!(f.attempt().await.canonical_intent,before.canonical_intent);
        {
            let mut state = rpc.state.lock().unwrap();
            // No signer readiness is needed to observe a completed payment.
            state.history_readiness = Some(json!({"scanning":false,"enhanced_through":HEIGHT}));
            state.wallet_history = Some(valid);
            state.operation = json!([]);
        }
        let recovered = reconcile(&f,&rpc,before.attempt_id).await;
        assert!(recovered.is_ok(),"synthetic recovery case {failure} failed: {}",
            rpc.failure_diagnostics().await);
        assert_eq!(recovered.unwrap(),1);
        assert_eq!(f.db.pps_invariant().await.unwrap().paid_zatoshis,CREDIT);
        assert_eq!(rpc.sends(),1);
        assert_eq!(rpc.forbidden_calls(),0);
    }
}

#[tokio::test]
async fn conventional_missing_wallet_history_cannot_mask_raw_financial_breach() {
    use pool_db::pps_funding::PpsConventionalHalt;
    for fee_violation in [true,false] {
        let f = Fixture::new_for_recipient(UNIFIED_RECIPIENT).await;
        let rpc = MockRpc::new(&f.db).await;
        rpc.shielded(UNIFIED_RECIPIENT);
        {
            let mut state = rpc.state.lock().unwrap();
            state.raw_override = Some(if fee_violation {
                (SHIELDED_BAD_FEE_TXID.into(),synthetic_shielded_raw_with_fee(10_001))
            } else {
                (TXID.into(),synthetic_raw(10_000))
            });
            state.wallet_history = None;
            state.history_readiness = Some(json!({}));
        }
        rpc.success();
        assert!(process(&f,&rpc).await.is_err());
        let attempt = f.attempt().await;
        assert_eq!(attempt.halt_category,Some(if fee_violation {
            PpsConventionalHalt::FeeMismatch
        } else { PpsConventionalHalt::RecipientMismatch }));
        assert!(attempt.sealed);
        // Halt fences sending, not accounting: snapshot works, send stays blocked.
        assert!(f.db.pps_funding_snapshot().await.is_ok());
        assert!(f.db.refund_pps_payout(attempt.attempt_id).await.is_err());
        assert!(process(&f,&rpc).await.is_err());
        assert_eq!(rpc.sends(),1);
        let state = rpc.state.lock().unwrap();
        let send_index = state.calls.iter().position(|m|m == "z_sendmany").unwrap();
        assert!(!state.calls[send_index+1..].iter()
            .any(|m|matches!(m.as_str(),"getwalletinfo"|"gettransaction")));
    }
}

#[tokio::test]
async fn conventional_restart_recovers_same_intent_without_sending_again() {
    let mut f = Fixture::new().await;
    let rpc = MockRpc::new(&f.db).await;
    assert_eq!(process(&f, &rpc).await.unwrap(), 0);
    let before = f.attempt().await;
    assert!(before.sealed);
    assert_eq!(rpc.sends(), 1);
    let legacy = f.legacy().await;
    f.reopen().await;
    rpc.success();
    let reopened = f.attempt().await;
    assert_eq!(reopened.canonical_intent, before.canonical_intent);
    assert_eq!(reconcile(&f, &rpc, before.attempt_id).await.unwrap(), 1);
    assert_eq!(rpc.sends(), 1);
    assert_eq!(rpc.forbidden_calls(), 0);
    assert_eq!(f.legacy().await, legacy);
    assert_eq!(f.db.pps_invariant().await.unwrap().paid_zatoshis, CREDIT);
    assert_eq!(reconcile(&f, &rpc, before.attempt_id).await.unwrap(), 0);
}

#[tokio::test]
async fn conventional_saved_txid_recovers_after_wallet_forgets_operation() {
    let mut f = Fixture::new().await;
    let rpc = MockRpc::new(&f.db).await;
    rpc.success();
    rpc.state.lock().unwrap().confirmations = 0;
    assert_eq!(process(&f, &rpc).await.unwrap(), 0);
    let attempt = f.attempt().await;
    assert_eq!(attempt.expected_txid.as_deref(), Some(TXID));
    f.reopen().await;
    let operation_reads = {
        let mut s = rpc.state.lock().unwrap();
        s.operation = json!([]);
        s.confirmations = crate::pps_conventional::PPS_SETTLE_MATURITY;
        s.calls
            .iter()
            .filter(|m| *m == "z_getoperationstatus")
            .count()
    };
    assert_eq!(reconcile(&f, &rpc, attempt.attempt_id).await.unwrap(), 1);
    assert_eq!(
        rpc.state
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|m| *m == "z_getoperationstatus")
            .count(),
        operation_reads
    );
    assert_eq!(rpc.sends(), 1);
    assert_eq!(f.db.pps_invariant().await.unwrap().paid_zatoshis, CREDIT);
}

#[tokio::test]
async fn conventional_ambiguous_send_or_lost_opid_is_never_refunded_or_retried() {
    for invalid_opid in [false, true] {
        let mut f = Fixture::new().await;
        let rpc = MockRpc::new(&f.db).await;
        {
            let mut s = rpc.state.lock().unwrap();
            s.invalid_opid = invalid_opid;
            s.send_error = !invalid_opid;
        }
        assert!(process(&f, &rpc).await.is_err());
        let attempt = f.attempt().await;
        assert!(attempt.sealed);
        assert!(attempt.operation_id.is_none());
        f.reopen().await;
        assert!(reconcile(&f, &rpc, attempt.attempt_id).await.is_err());
        assert!(process(&f, &rpc).await.is_err());
        assert_eq!(rpc.sends(), 1);
        let sum = f.db.pps_invariant().await.unwrap();
        assert_eq!(
            (sum.pending_zatoshis, sum.paying_zatoshis, sum.paid_zatoshis),
            (0, CREDIT, 0)
        );
        assert_eq!(
            f.db.pps_funding_snapshot()
                .await
                .unwrap()
                .reserved_fees_zatoshis,
            28_410_000
        );
        assert!(f.db.refund_pps_payout(attempt.attempt_id).await.is_err());
    }
}

#[tokio::test]
async fn conventional_unknown_failed_or_conflicting_results_remain_held() {
    for response in [
        json!([]),
        json!([{"id":"opid-synthetic-1","status":"failed"}]),
        json!([{"id":"different-operation","status":"success","result":{"txid":TXID}}]),
        json!([{"id":"opid-synthetic-1","status":"success","result":{"txids":[TXID,"c".repeat(64)]}}]),
    ] {
        let f = Fixture::new().await;
        let rpc = MockRpc::new(&f.db).await;
        assert_eq!(process(&f, &rpc).await.unwrap(), 0);
        let attempt = f.attempt().await;
        rpc.state.lock().unwrap().operation = response;
        let _ = reconcile(&f, &rpc, attempt.attempt_id).await;
        assert_eq!(rpc.sends(), 1);
        assert_eq!(f.db.pps_invariant().await.unwrap().paying_zatoshis, CREDIT);
        assert_eq!(
            f.db.pps_funding_snapshot()
                .await
                .unwrap()
                .reserved_fees_zatoshis,
            28_410_000
        );
        assert!(f.db.refund_pps_payout(attempt.attempt_id).await.is_err());
    }
}

#[tokio::test]
async fn conventional_canonical_inclusion_and_raw_contract_required() {
    for failure in 0..4 {
        let f = Fixture::new().await;
        let rpc = MockRpc::new(&f.db).await;
        rpc.success();
        {
            let mut s = rpc.state.lock().unwrap();
            match failure {
                0 => s.confirmations = 0,
                1 => s.wrong_chain = true,
                2 => s.missing_inclusion = true,
                _ => s.bad_raw = true,
            }
        }
        let _ = process(&f, &rpc).await;
        assert_eq!(rpc.sends(), 1);
        let sum = f.db.pps_invariant().await.unwrap();
        assert_eq!(
            (sum.pending_zatoshis, sum.paying_zatoshis, sum.paid_zatoshis),
            (0, CREDIT, 0)
        );
        assert!(f.attempt().await.halt_category.is_none());
        assert!(f
            .db
            .refund_pps_payout(f.attempt().await.attempt_id)
            .await
            .is_err());
    }
}

#[tokio::test]
async fn conventional_proven_recipient_or_fee_violation_is_a_durable_global_hold() {
    use pool_db::pps_funding::PpsConventionalHalt;
    for fee_violation in [false, true] {
        let mut f = Fixture::new().await;
        let rpc = MockRpc::new(&f.db).await;
        if fee_violation {
            rpc.state.lock().unwrap().actual_fee = 10_001;
        } else {
            f.policy.max_payout_zatoshis = CREDIT - 1;
            rpc.state.lock().unwrap().expected_amount = CREDIT - 1;
        }
        rpc.success();
        let legacy = f.legacy().await;
        assert!(process(&f, &rpc).await.is_err());
        let attempt = f.attempt().await;
        assert_eq!(
            attempt.halt_category,
            Some(if fee_violation {
                PpsConventionalHalt::FeeMismatch
            } else {
                PpsConventionalHalt::RecipientMismatch
            }),
            "synthetic financial-halt case fee_violation={fee_violation}: {}",
            rpc.failure_diagnostics().await
        );
        // A halt fences SENDING only; accounting/admission/startup must survive.
        assert!(f.db.pps_funding_snapshot().await.is_ok());
        f.reopen().await;
        let reopened = f.attempt().await;
        assert_eq!(reopened.halt_category, attempt.halt_category);
        assert!(f.db.pps_invariant().await.is_ok());
        let row = sqlx::query("SELECT pending,paying,paid FROM pps_accounts WHERE miner_id=?1")
            .bind(f.miner)
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(
            (
                row.get::<i64, _>(0),
                row.get::<i64, _>(1),
                row.get::<i64, _>(2)
            ),
            (
                CREDIT - f.policy.max_payout_zatoshis,
                f.policy.max_payout_zatoshis,
                0
            )
        );
        assert!(reconcile(&f, &rpc, attempt.attempt_id).await.is_err());
        assert!(process(&f, &rpc).await.is_err());
        assert!(f.db.refund_pps_payout(attempt.attempt_id).await.is_err());
        assert_eq!(rpc.sends(), 1);
        assert_eq!(f.legacy().await, legacy);
        assert_eq!(rpc.forbidden_calls(), 0);
    }
}

#[tokio::test]
async fn conventional_preseal_funding_failure_releases_only_unsent_reservation() {
    let f = Fixture::new().await;
    let rpc = MockRpc::new(&f.db).await;
    rpc.state.lock().unwrap().fail_second_funding = true;
    assert!(process(&f, &rpc).await.is_err());
    assert_eq!(rpc.sends(), 0);
    let sum = f.db.pps_invariant().await.unwrap();
    assert_eq!(
        (sum.pending_zatoshis, sum.paying_zatoshis, sum.paid_zatoshis),
        (CREDIT, 0, 0)
    );
    assert_eq!(
        f.db.pps_funding_snapshot()
            .await
            .unwrap()
            .reserved_fees_zatoshis,
        0
    );
    assert!(!f.attempt().await.sealed);
}

#[tokio::test]
async fn conventional_real_seal_rejection_refunds_unsealed_claims_without_sending() {
    let f=Fixture::new().await;
    let rpc=MockRpc::new(&f.db).await;
    let legacy=f.legacy().await;
    // This rejects the actual SQL seal in the normal process path, before
    // commit. No production fault hook or altered payout operation is needed.
    sqlx::query("CREATE TRIGGER synthetic_reject_seal BEFORE UPDATE OF sealed ON pps_fee_reservations WHEN NEW.sealed=1 BEGIN SELECT RAISE(ABORT,'synthetic seal rejection'); END")
        .execute(&f.pool).await.unwrap();
    assert!(process(&f,&rpc).await.is_err());
    assert_eq!(rpc.sends(),0);
    assert_eq!(rpc.forbidden_calls(),0);
    assert_eq!(f.legacy().await,legacy);
    let sum=f.db.pps_invariant().await.unwrap();
    assert_eq!((sum.pending_zatoshis,sum.paying_zatoshis,sum.paid_zatoshis),(CREDIT,0,0));
    assert_eq!(f.db.pps_funding_snapshot().await.unwrap().reserved_fees_zatoshis,0);
    assert!(!f.attempt().await.sealed);
}

#[tokio::test]
async fn conventional_final_preparation_failures_release_only_definitely_unsealed_reservations() {
    use pool_db::pps_funding::{PpsConventionalIntent,PpsConventionalRecipient,PpsConventionalReservation};
    // Malformed encoding is not reachable from normal bounded selection. Test
    // the real encoder and exact production cleanup boundary with that input,
    // rather than adding a runtime injection path or corrupting the ledger.
    for failure in ["encoding","missing_cache","expired_at_seal","ambiguous_committed_seal"] {
        let f=Fixture::new().await;
        let rpc=MockRpc::new(&f.db).await;
        let legacy=f.legacy().await;
        let intent=PpsConventionalIntent { version:1,network:"testnet".into(),epoch:f.policy.epoch.clone(),
            target_height:HEIGHT as u64,source:SOURCE.into(),profile:"consensus-size-v1".into(),
            max_recipients:100,items:vec![PpsConventionalRecipient {
                miner_id:f.miner,address:RECIPIENT.into(),amount_zatoshis:CREDIT }] };
        let bound=PpsConventionalReservation::from_intent(intent.clone()).unwrap();
        let attempt=f.db.create_payout_attempt(1,CREDIT,"pps-conventional-testnet").await.unwrap();
        f.db.reserve_pps_conventional_payout(attempt,&[(f.miner,CREDIT)],
            &funding(&f.db,&f.policy).await,&bound).await.unwrap();
        let outcome: anyhow::Result<()> = pps_conventional::pre_send_or_release(&f.db,attempt,async {
            match failure {
                "encoding" => {
                    let mut malformed=intent.clone();
                    malformed.items[0].amount_zatoshis=i64::MAX;
                    pps_conventional::encode_recipients(&malformed)?;
                },
                "missing_cache" => {
                    // New production gate, no supplied-evidence test shortcut.
                    let empty=PpsGate::new(rpc.client.clone(),"testnet");
                    let _held=empty.valid_cached_lease().await?;
                },
                _ => {
                    let gate=gate(&rpc.client);
                    let held=gate.valid_cached_lease().await?;
                    let mut lease=funding(&f.db,&f.policy).await;
                    lease.valid_until_unix=lease.valid_until_unix.min(held.valid_until_unix());
                    if failure=="expired_at_seal" {
                        // Models expiry while waiting for the database. Its
                        // actual funded entry check must reject the proof.
                        lease.valid_until_unix=Utc::now().timestamp();
                    }
                    f.db.seal_pps_conventional_payout_funded(attempt,&bound.intent_id,&lease).await?;
                    drop(held);
                    // Real durable commit, followed by a caller-side error:
                    // the same production cleanup must NOT release these funds.
                    anyhow::bail!("synthetic post-commit response failure");
                },
            }
            Ok(())
        }).await;
        assert!(outcome.is_err());
        assert_eq!(rpc.sends(),0);
        assert_eq!(rpc.forbidden_calls(),0);
        assert_eq!(f.legacy().await,legacy);
        let sum=f.db.pps_invariant().await.unwrap();
        let row=f.attempt().await;
        let fees=f.db.pps_funding_snapshot().await.unwrap().reserved_fees_zatoshis;
        if failure=="ambiguous_committed_seal" {
            assert!(row.sealed);
            assert_eq!((sum.pending_zatoshis,sum.paying_zatoshis,sum.paid_zatoshis),(0,CREDIT,0));
            assert_eq!(fees,bound.fee_upper_bound_zatoshis);
            assert!(f.db.refund_pps_payout(attempt).await.is_err());
            assert!(process(&f,&rpc).await.is_err());
            assert_eq!(rpc.sends(),0);
        } else {
            assert!(!row.sealed);
            assert_eq!((sum.pending_zatoshis,sum.paying_zatoshis,sum.paid_zatoshis),(CREDIT,0,0));
            assert_eq!(fees,0);
        }
    }
}

#[tokio::test]
async fn conventional_legacy_inflight_and_wrong_route_prevent_new_sends() {
    let f = Fixture::new().await;
    let rpc = MockRpc::new(&f.db).await;
    let attempt =
        f.db.create_payout_attempt(1, 100, "legacy-test")
            .await
            .unwrap();
    f.db.reserve_payout(attempt, &[(f.miner, 100)])
        .await
        .unwrap();
    let before = f.legacy().await;
    assert!(process(&f, &rpc).await.is_err());
    assert_eq!(rpc.sends(), 0);
    assert_eq!(f.legacy().await, before);
    assert_eq!(f.db.pps_invariant().await.unwrap().pending_zatoshis, CREDIT);
    assert!(pps_conventional::process(
        &f.db,
        &rpc.client,
        &rpc.client,
        SOURCE,
        1,
        &f.policy,
        &gate(&rpc.client),
        &PpsFundingRoute::ZalletPczt
    )
    .await
    .is_err());
    let mut policy = f.policy.clone();
    policy.network = "mainnet".into();
    assert!(pps_conventional::process(
        &f.db,
        &rpc.client,
        &rpc.client,
        SOURCE,
        1,
        &policy,
        &gate(&rpc.client),
        &route()
    )
    .await
    .is_err());
    assert_eq!(rpc.sends(), 0);
    assert_eq!(rpc.forbidden_calls(), 0);
}
