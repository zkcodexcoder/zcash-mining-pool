// Source-only real-PoW PPS credit/restart and unsupported-wallet hold rehearsal.
// Public real PoW fixture: ZcashFoundation/zebra, zebra-test/src/vectors/
// block-test-0-000-001.txt (testnet block 1). The embedded fixture is fixed;
// this test never downloads it or contacts a real node, miner, or wallet.
//
// We include the unchanged production validator and dashboard implementation
// solely to call their private entry points. No validation code is copied or
// bypassed. Only external RPC responses/chain attestations are synthetic.
// This is NOT a successful PCZT settlement, Stratum wire/ACK, independent-chain
// verifier, actual wallet, or live deployment test. Direct DB fee/sealed recovery
// below is a synthetic ledger proof, not evidence of wallet compatibility.
include!("../src/main.rs");

pub use pool_core::{block, difficulty, job, pps_chain, pps_economics, pps_funding};
// The included production validator uses crate-private health tracking. Include
// the same implementation here as well; never replace it with a bypassing stub.
#[path = "../../pool-core/src/pps_credit_health.rs"]
mod pps_credit_health;

#[allow(dead_code)] // Unchanged source also contains runtime entry points not called by this test.
mod actual_validator {
    include!("../../pool-core/src/share.rs");

    pub(super) struct Harness {
        validator: ShareValidator,
        job: MiningJob,
        block: Vec<u8>,
    }

    impl Harness {
        pub(super) async fn new(
            db: PoolDb,
            rpc: Arc<ZcashRpcClient>,
            wallet_rpc: Arc<ZcashRpcClient>,
            epoch: PpsEpoch,
            lease: PpsChainLease,
            block: Vec<u8>,
        ) -> Self {
            Self::new_for_route(db,rpc,wallet_rpc,epoch,lease,block,
                pool_core::pps_funding::PpsFundingRoute::ZalletPczt).await
        }
        pub(super) async fn new_for_route(
            db:PoolDb,rpc:Arc<ZcashRpcClient>,wallet_rpc:Arc<ZcashRpcClient>,epoch:PpsEpoch,
            lease:PpsChainLease,block:Vec<u8>,funding_route:pool_core::pps_funding::PpsFundingRoute,
        )->Self {
            let funding_lease = super::synthetic_funding(&db, &epoch).await;
            let mut target = [0_u8; 32];
            target[..3].copy_from_slice(&[0x07, 0xff, 0xff]);
            let reverse_hex = |b: &[u8]| hex::encode(b.iter().rev().copied().collect::<Vec<_>>());
            let time = u32::from_le_bytes(block[100..104].try_into().unwrap());
            let template = serde_json::from_value(serde_json::json!({
                "version": 4,
                "previousblockhash": reverse_hex(&block[4..36]),
                "defaultroots": {"merkleroot": reverse_hex(&block[36..68])},
                "transactions": [],
                "coinbasetxn": {
                    "data": hex::encode(&block[1488..]),
                    "hash": reverse_hex(&block[36..68]), "fee": 0
                },
                "target": hex::encode(target), "mintime": time, "curtime": time,
                "bits": "2007ffff", "height": 1
            }))
            .unwrap();
            let job = MiningJob::from_template(template, "public-testnet-block-1".into());
            assert_eq!(
                build_header_input(&job, &job.time_hex).unwrap(),
                block[..108]
            );
            assert_eq!(&block[140..143], &[0xfd, 0x40, 0x05]);
            assert_eq!(block[1487], 1);
            assert!(meets_target(&sha256d(&block[..1487]), &target));
            equihash::is_valid_solution(200, 9, &block[..108], &block[108..140], &block[143..1487])
                .expect("official public block must pass real Equihash");
            let latest = Arc::new(RwLock::new(Some(job.to_notify(true))));
            let (events, _events_rx) = mpsc::channel(16);
            let (mut server, _notify_rx) =
                StratumServer::new_with_latest_notify(4, events, Arc::clone(&latest));
            server
                .set_fixed_share_target(stratum::FixedShareTarget::new(target).unwrap())
                .unwrap();
            let jobs = Arc::new(RwLock::new(HashMap::from([(
                job.job_id.clone(),
                Arc::new(job.clone()),
            )])));
            let counter = || Arc::new(std::sync::atomic::AtomicU64::new(0));
            let mut validator = ShareValidator::new(
                db.clone(),
                Arc::new(server),
                jobs,
                Arc::new(BlockAssembler::new(Arc::clone(&rpc))),
                Arc::new(PplnsCalculator::new(db, 100, 0.0, rewards::RewardMode::Pps)),
                target,
                VardiffConfig {
                    initial_difficulty: 1.0,
                    target_shares_per_minute: 10.0,
                    retarget_interval_secs: 30.0,
                },
                HashMap::new(),
                latest,
                rpc,
                1.0,
                counter(),
                counter(),
                counter(),
                counter(),
                counter(),
                counter(),
                counter(),
                counter(),
            )
            .with_pps(PpsRuntime {
                epoch,
                chain_lease: Some(lease),
                funding_lease: Some(funding_lease),
                wallet_rpc,
                payout_source: "synthetic-source".into(),
                funding_route,
            })
            .unwrap();
            // Test-only dependency isolation: real automatic verification is
            // separately tested. Never let this harness access public RPCs.
            validator.pps_refresh_task.take().unwrap().abort();
            Self {
                validator,
                job,
                block,
            }
        }

        pub(super) async fn submit(
            &self,
            session: &str,
            solution: &[u8],
        ) -> Result<ShareResult, StratumError> {
            self.validator
                .validate_share(
                    session,
                    &format!("{}.fixture", super::CANARY_ADDRESS),
                    &self.job.job_id,
                    &self.job.time_hex,
                    &hex::encode(&self.block[108..112]),
                    &hex::encode(&self.block[112..140]),
                    &hex::encode(solution),
                )
                .await
        }

        pub(super) async fn invalidate_lease(&self) {
            *self.validator.pps.as_ref().unwrap().lease.write().await = None;
        }
        pub(super) async fn invalidate_funding(&self) {
            *self.validator.pps.as_ref().unwrap().funding.write().await = None;
        }
        pub(super) async fn credit_health(&self)->super::pps_credit_health::PpsCreditHealth {
            let p=self.validator.pps.as_ref().unwrap();
            super::pps_credit_health::sample_health(&self.validator.db,&p.epoch,&p.funding_route,
                &p.lease,&p.funding,&p.health,&self.validator.latest_notify).await
        }
        pub(super) async fn replace_notify(&self) {
            let mut notify=self.job.to_notify(false);
            if let ServerMessage::Notify {job_id,..}=&mut notify {*job_id="replacement-job".into();}
            *self.validator.latest_notify.write().await=Some(notify);
        }
        pub(super) fn replace_quote(&self,amount:u128) {
            let p=self.validator.pps.as_ref().unwrap();
            super::pps_credit_health::validated_quote(&p.health,&self.job.job_id,&self.job.prev_hash_hex,
                amount,chrono::Utc::now().timestamp(),Instant::now());
        }

        pub(super) fn invalid_equihash_meeting_target(&self) -> Vec<u8> {
            // Deliberately pass the cheap SHA target gate while failing the
            // genuine Equihash verifier: no credit can hide behind that gate.
            let mut header = self.block[..1487].to_vec();
            let mut target = [0_u8; 32];
            target[..3].copy_from_slice(&[0x07, 0xff, 0xff]);
            for mutation in 1_u16..4096 {
                header[143..145].copy_from_slice(&mutation.to_le_bytes());
                if meets_target(&sha256d(&header), &target)
                    && equihash::is_valid_solution(
                        200,
                        9,
                        &header[..108],
                        &header[108..140],
                        &header[143..],
                    )
                    .is_err()
                {
                    return header[143..].to_vec();
                }
            }
            panic!("bounded search did not find invalid Equihash with an adequate header hash");
        }
    }
}

const CANARY_ADDRESS: &str = "tmFU5Ak942B7SciQpZCh3xH76QV3UmJgnDd";
const CANARY_TXID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const PUBLIC_BLOCK_HEX: &str = "04000000382c4a332661c7ed0671f32a34d724619f086c61873bce7c99859dd9920aa605755f7c7d27a811596e9fae6dd30ca45be86e901d499909de35b6ff1f699f7ef30000000000000000000000000000000000000000000000000000000000000000e9851358ffff0720000056c2264c31261d597c6fcea7c5e00160cf6be1cd89ca96a0389473e50000fd40050053f4438864bc5d6dfc009d4bba545ac5e5feaaf46f9455b975b02115f842a966e26517ce678f1c074d09cc8d0049a190859eb505af5f3e760312fbbe54da115db2bc03c96408f39b679891790b539d2d9d17a801dc6af9af14ca3f6ba060edce2a1dd45aa45f11fe37dbaf1eb2647ae7c393f6680c3d5d7e53687e34530f48edf58924a04d3e0231c150b1c8218998f674bc171edd222bcb4ac4ba4ea52d7baa86399f371d5284043e1e166f9069dd0f2904ff94c7922a70fa7c660e0553cc40a20d9ee08eb3f47278485801ddae9c270411360773f0b74e03db2d92c50952c9bd4924bbca2a260e1235e99df51fe71e75744232f2d641ef94f394110a5ad05f51a057e4cb515b92c16cb1404a8cdcc43d4a4bb2caa54ca35dccf41aa7d832da65123b7029223c46ed2a13387d598d445435d3cb32fdad9e27672903864c90d86353b162033078327b5b7aaffc89b40096ae004f2d5c6bd2c99188574348518db66e9b6020f93f12ee1c06f7b00fe346fefceaffb1da9e3cdf08285057f549733eb10825737fcd1431bfdfb155f323f24e95a869212baacf445b30f2670206645779110e6547d5da90a5f2fe5151da911d5ecd5a833023661d1356b6c395d85968947678d53efd4db7b06f23b21125e74492644277ea0c1131b80d6a4e3e8093b82332556fbb3255a55ac3f0b7e4844c0e12bf577c37fd02323ae5ef4781772ed501d63b568032a3d31576c5104a48c01ac54f715286932351a8adc8cf2467a84a0572e99f366ee00f82c3735545fd4bb941d591ce70070425a81304272db89887949bc7dd8236bb7e82190f9815da938cd6e8fec7660e91354326a7a9bfe38120e97997fca3c289d54513ed00286c2b825fbe84f91a39528f335674b5e957425a6edfdd00f2feb2c2df575616197998c1e964e069875d4d934f419a9b02b100848d023b76d47bd4e284c3895ef9227a40d8ea8826e86c7155d6aa95b8f9175812523a32cd611efc700688e03f7c245c5bff01718281b5d75cefe8318b2c08962236b14a0bf79534c203df735fd9cced97cbae07c2b4ee9cda8c9993f3f6277ff3fec261fb94d3961c4befe4b0893dcf67b312c7d8d6ff7adc8539cb2b1d3534fccf109efddd07a9f1e77b94ab1e505b164221dca1c34621b1e9d234c31a032a401267d95f65b800d579a2482638dfeade804149c81e95d7ef5510ac0b6212231506b1c635a2e1d2f0c9712989f9f246762fadb4c55c20f707dcc0e510a33e9465fc5d5bdbfa524dab0d7a1c6a1baaa36869cf542aa2257c5c44ef07547a570343442c6091e13bc04d559dc0e6db5b001861914bf956816edce2a86b274bd97f27e2dbb08608c16a3e5d8595952faa91fb162d7fa6a7a47e849a1ad8fab3ba620ee3295a04fe13e5fb655ac92ae60d01020b8999526af8d56b28733e69c9ffb285de27c61edc0bf62261ac0787eff347d0fcd62257301ede9603106ea41650a3e3119bd5c4e86a7f6a3f00934f3a545f7f21d41699f3e35d38cf925a8bdaf2bf7eedea11c31c3d8bf6c527c77c6378281cdf02211a58fa5e46d28d7e7c5fb79d69b31703fd752395da115845952cf99aaeb2155c2ab951a69f67d938f223185567e52cfa3e57b62c790bf78674c4b02c12b7d3225fe8f705b408ba11c24245b3924482e2f3480994461b550641a88cd941d371139f3498afacdcba1249631402b20695760eaada5376e68df0e45139c410700effc9420dc3726515e7fcb3f349320f30511451964bd9b6530682efec65910ceb548aa2ab05ac3309e803161697213631ae8e13cc7d223ac28446c1bf94a19a8782ac16ff57df7ee4f10fb6e488c02c68d6b6dee6987f6d2c39227da366c59f54ff67e312ca530e7c467c3dc80101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff03510101ffffffff0250c30000000000002321025229e1240a21004cf8338db05679fa34753706e84f6aebba086ba04317fd8f99acd43000000000000017a914ef775f1f997f122a062fff1a2d7443abd1f9c6428700000000";
const MINER_SUBSIDY: i64 = 50_000;

fn synthetic_lease() -> pool_db::pps_live::PpsChainLease {
    let now = Utc::now().timestamp();
    pool_db::pps_live::PpsChainLease {
        network: "testnet".into(),
        checked_at_unix: now,
        valid_until_unix: now + 90,
        agreeing_references: 2,
        disagreement: false,
    }
}

fn synthetic_policy() -> PpsPolicy {
    PpsPolicy {
        network: "testnet".into(),
        epoch: "full-path-canary".into(),
        fee_bps: 0,
        max_liability_zatoshis: 990_000_000,
        total_exposure_zatoshis: 1_000_000_000, // synthetic 10 ZEC total, fees included
        fee_allowance_zatoshis: 10_000_000,
        reserve_min_zatoshis: 10_000_000,
        max_payout_zatoshis: 100_000_000,
    }
}

async fn synthetic_funding(
    db: &PoolDb,
    epoch: &pool_db::pps_live::PpsEpoch,
) -> pool_db::pps_funding::PpsFundingLease {
    let s = db.pps_funding_snapshot_for_epoch(epoch).await.unwrap();
    let now = Utc::now().timestamp();
    pool_db::pps_funding::PpsFundingLease {
        network: epoch.network.clone(),
        checked_at_unix: now,
        valid_until_unix: now + 60,
        spendable_zatoshis: 10_000_000_000.max(s.required_spendable_zatoshis),
        reserve_floor_zatoshis: epoch.reserve_floor_zatoshis,
        reserved_fee_allowance_zatoshis: epoch.fee_allowance_zatoshis,
        generation: s.generation,
    }
}

// Capture only synthetic requests to assert exact block assembly and no wallet send.
struct FakeRpc {
    url: String,
    requests: Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    observed_db: Arc<std::sync::Mutex<Option<PoolDb>>>,
    at_send: Arc<std::sync::Mutex<Vec<(i64, i64, i64)>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for FakeRpc {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl FakeRpc {
    async fn new(responses: HashMap<&'static str, serde_json::Value>) -> Self {
        use axum::{extract::State, routing::post, Json, Router};
        type StateData = (
            HashMap<&'static str, serde_json::Value>,
            Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
            Arc<std::sync::Mutex<Option<PoolDb>>>,
            Arc<std::sync::Mutex<Vec<(i64, i64, i64)>>>,
        );
        async fn handler(
            State(state): State<Arc<StateData>>,
            Json(request): Json<serde_json::Value>,
        ) -> Json<serde_json::Value> {
            let method = request["method"].as_str().unwrap_or("");
            if method == "z_sendmany" {
                let observed = state.2.lock().unwrap().clone();
                if let Some(db) = observed {
                    let pps = db.pps_invariant().await.unwrap();
                    state.3.lock().unwrap().push((
                        pps.pending_zatoshis,
                        pps.paying_zatoshis,
                        pps.paid_zatoshis,
                    ));
                }
            }
            state.1.lock().unwrap().push(request.clone());
            let id = request
                .get("id")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            match state.0.get(method) {
                Some(result) => {
                    Json(serde_json::json!({"jsonrpc":"2.0","id":id,"result":result,"error":null}))
                }
                None => Json(serde_json::json!({"jsonrpc":"2.0","id":id,"result":null,
                    "error":{"code":-5,"message":"synthetic unavailable"}})),
            }
        }
        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
        let observed_db = Arc::new(std::sync::Mutex::new(None));
        let at_send = Arc::new(std::sync::Mutex::new(Vec::new()));
        let state = Arc::new((
            responses,
            Arc::clone(&requests),
            Arc::clone(&observed_db),
            Arc::clone(&at_send),
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new().route("/", post(handler)).with_state(state);
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            url,
            requests,
            observed_db,
            at_send,
            task,
        }
    }

    fn calls(&self, method: &str) -> Vec<serde_json::Value> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request["method"] == method)
            .cloned()
            .collect()
    }
}

// The real block path writes last_block.hex. Contain that public diagnostic in
// a uniquely owned temporary directory, and run this single test serially.
// No config, DB, state, or diagnostic from any existing runtime is read.
struct IsolatedCwd {
    original: std::path::PathBuf,
    directory: std::path::PathBuf,
}
impl IsolatedCwd {
    fn new() -> Self {
        use std::os::unix::fs::DirBuilderExt;
        let original = std::env::current_dir().unwrap();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("pps-public-fixture-{}-{stamp}", std::process::id()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        std::env::set_current_dir(&directory).unwrap();
        Self {
            original,
            directory,
        }
    }
}
impl Drop for IsolatedCwd {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.original);
        let _ = std::fs::remove_file(self.directory.join("last_block.hex"));
        let _ = std::fs::remove_file(self.directory.join("canary.sqlite-wal"));
        let _ = std::fs::remove_file(self.directory.join("canary.sqlite-shm"));
        let _ = std::fs::remove_file(self.directory.join("canary.sqlite"));
        let _ = std::fs::remove_dir(&self.directory);
    }
}

async fn persistent_fixture(directory: &std::path::Path) -> (PoolDb, sqlx::SqlitePool) {
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
    let options = SqliteConnectOptions::new()
        .filename(directory.join("canary.sqlite"))
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let db = PoolDb::new(pool.clone());
    db.run_migrations().await.unwrap();
    let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        synchronous, 2,
        "actual file-backed connection must use FULL durability"
    );
    (db, pool)
}

async fn assert_uncredited(db: &PoolDb) {
    let state = db.pps_invariant().await.unwrap();
    assert_eq!(state.accepted_events, 0);
    assert_eq!(state.gross_subzatoshis, 0);
    assert_eq!(
        (
            state.pending_zatoshis,
            state.paying_zatoshis,
            state.paid_zatoshis
        ),
        (0, 0, 0)
    );
    let shares: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM shares")
        .fetch_one(db.inner())
        .await
        .unwrap();
    assert_eq!(shares, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn real_equihash_credit_restart_replay_and_unsupported_wallet_hold() {
    tokio::time::timeout(
        Duration::from_secs(10),
        credit_restart_replay_and_unsupported_wallet_case(),
    )
    .await
    .expect("source-only PPS rehearsal exceeded its 10-second bound");
}

async fn credit_restart_replay_and_unsupported_wallet_case() {
    use actual_validator::Harness;
    use pool_db::pps_live::PPS_SCALE;
    use sha2::{Digest, Sha256};

    let isolated = IsolatedCwd::new();
    let block = hex::decode(PUBLIC_BLOCK_HEX).unwrap();
    assert_eq!(block.len(), 1618);
    let header_hash = Sha256::digest(Sha256::digest(&block[..1487]));
    let canonical_hash = hex::encode(header_hash.iter().rev().copied().collect::<Vec<_>>());
    let node = FakeRpc::new(HashMap::from([
        (
            "getblockchaininfo",
            serde_json::json!({"chain":"test","blocks":0,"headers":0,"initialblockdownload":false}),
        ),
        (
            "getblocksubsidy",
            serde_json::from_str(r#"{"miner":0.0005}"#).unwrap(),
        ),
        ("submitblock", serde_json::Value::Null),
        ("getblockhash", serde_json::json!(canonical_hash)),
        ("getnetworksolps", serde_json::json!(0)),
        ("getrawtransaction", serde_json::json!({"height":1})),
    ]))
    .await;
    let wallet = FakeRpc::new(HashMap::from([
        (
            "z_gettotalbalance",
            serde_json::json!({"private":"100.0","total":"100.0"}),
        ),
        ("z_sendmany", serde_json::json!("synthetic-operation")),
        (
            "z_getoperationstatus",
            serde_json::json!([{"status":"success","result":{"txids":[CANARY_TXID]}}]),
        ),
    ]))
    .await;
    let policy = synthetic_policy();
    policy.validate("testnet").unwrap();
    let (db, pool) = persistent_fixture(&isolated.directory).await;
    db.initialize_pps_epoch(
        &policy.epoch_config(),
        Some(&synthetic_funding(&db, &policy.epoch_config()).await),
    )
    .await
    .unwrap();
    let harness = Harness::new(
        db.clone(),
        Arc::new(ZcashRpcClient::new(&node.url)),
        Arc::new(ZcashRpcClient::new(&wallet.url)),
        policy.epoch_config(),
        synthetic_lease(),
        block.clone(),
    )
    .await;

    // Correct header-hash difficulty cannot make invalid Equihash billable.
    let bad = harness.invalid_equihash_meeting_target();
    let rejected = harness.submit("bad-proof", &bad).await;
    assert!(rejected.is_err());
    assert!(rejected.err().unwrap().message.contains("Equihash"));
    assert_uncredited(&db).await;
    assert!(node.calls("submitblock").is_empty());

    // A real validated proof returns success only after durable priced credit.
    let result = harness
        .submit("first-session", &block[143..1487])
        .await
        .unwrap();
    assert!(result.is_block);
    assert_eq!(result.block_height, Some(1));
    let accepted = db.pps_invariant().await.unwrap();
    assert_eq!(accepted.accepted_events, 1);
    assert_eq!(
        accepted.gross_subzatoshis,
        MINER_SUBSIDY as u128 * PPS_SCALE
    );
    assert_eq!(
        accepted.max_liability_subzatoshis,
        990_000_000_u128 * PPS_SCALE
    );
    assert_eq!(
        (
            accepted.pending_zatoshis,
            accepted.paying_zatoshis,
            accepted.paid_zatoshis
        ),
        (MINER_SUBSIDY, 0, 0)
    );
    assert_eq!(
        node.calls("submitblock")[0]["params"][0].as_str(),
        Some(PUBLIC_BLOCK_HEX)
    );
    let miner = db.get_or_create_miner(CANARY_ADDRESS).await.unwrap();
    // Distinct legacy liability must survive PPS payout without changes.
    sqlx::query("INSERT INTO balances(miner_id,pending,paying,paid) VALUES(?1,123,0,0)")
        .bind(miner.id)
        .execute(&pool)
        .await
        .unwrap();
    let quote: (i64, String, String, i64) = sqlx::query_as(
        "SELECT height,lower(hex(network_target)),lower(hex(assigned_target)),miner_subsidy FROM pps_quotes")
        .fetch_one(&pool).await.unwrap();
    let target = format!("07ffff{}", "00".repeat(29));
    assert_eq!(quote, (1, target.clone(), target, MINER_SUBSIDY));

    // Close every original SQLite connection and reopen the WAL/FULL database.
    // This is persistence across connection/validator restart, NOT a simulated
    // power loss or a claim about the production filesystem's fsync behavior.
    drop(harness);
    pool.close().await;
    drop(db);
    let (db, pool) = persistent_fixture(&isolated.directory).await;
    db.verify_pps_epoch(&policy.epoch_config()).await.unwrap();
    assert_eq!(
        db.pps_invariant().await.unwrap().gross_subzatoshis,
        accepted.gross_subzatoshis
    );
    // Replay after a new validator/session (including canonical CompactSize)
    // is the same durable proof, not another payable share.
    let restarted = Harness::new(
        db.clone(),
        Arc::new(ZcashRpcClient::new(&node.url)),
        Arc::new(ZcashRpcClient::new(&wallet.url)),
        policy.epoch_config(),
        synthetic_lease(),
        block.clone(),
    )
    .await;
    restarted
        .submit("restarted-session", &block[140..1487])
        .await
        .unwrap();
    let replayed = db.pps_invariant().await.unwrap();
    assert_eq!(replayed.accepted_events, 1);
    assert_eq!(replayed.gross_subzatoshis, accepted.gross_subzatoshis);

    let gate = PpsGate::synthetic(
        Arc::new(ZcashRpcClient::new(&node.url)),
        "testnet",
        synthetic_lease(),
    );
    let wallet_client = ZcashRpcClient::new(&wallet.url);
    let node_client = ZcashRpcClient::new(&node.url);
    *wallet.observed_db.lock().unwrap() = Some(db.clone());
    let pay = || {
        process_payouts_for_ledger(
            &db,
            &wallet_client,
            &node_client,
            "synthetic-pool",
            CANARY_ADDRESS,
            1,
            0,
            1.0,
            "testnet",
            false,
            0,
            i64::MAX,
            Some(&policy),
            Some(&gate),
        )
    };
    // A synthetically successful z_sendmany response is NOT PCZT capability.
    // The actual new payout path must stop before any signing or sending, while
    // preserving the real Equihash-derived credit and every legacy liability.
    assert!(pay().await.is_err());
    assert!(pay().await.is_err());
    assert_eq!(db.pps_invariant().await.unwrap(), replayed);
    assert!(db.get_reserved_pps_attempts(None).await.unwrap().is_empty());
    let attempts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payout_attempts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(attempts, 0);
    for method in [
        "z_sendmany",
        "pczt_create",
        "pczt_prove",
        "pczt_sign",
        "pczt_extract",
        "sendrawtransaction",
    ] {
        assert!(
            wallet.calls(method).is_empty(),
            "unsupported wallet received {method}"
        );
    }
    assert!(node.calls("sendrawtransaction").is_empty());
    assert!(wallet.at_send.lock().unwrap().is_empty());

    // Separate DB-only recovery proof: explicitly seed the already-inspected
    // proposal and signature fence. This does not call a signer/broadcaster or
    // claim a successful PCZT round trip; fake chain evidence settles once.
    let attempt = db
        .create_payout_attempt(1, MINER_SUBSIDY, "fixture-db-recovery")
        .await
        .unwrap();
    let fee = pool_db::pps_funding::PpsFeeReservation {
        proposal_id: "e".repeat(64),
        fee_zatoshis: 10_000,
    };
    db.reserve_pps_payout(
        attempt,
        &[(miner.id, MINER_SUBSIDY)],
        &synthetic_funding(&db, &policy.epoch_config()).await,
        &fee,
    )
    .await
    .unwrap();
    db.seal_pps_payout(attempt, &fee.proposal_id).await.unwrap();
    assert!(db.refund_pps_payout(attempt).await.is_err());
    db.mark_pps_payout_signed(attempt, &fee.proposal_id, CANARY_TXID)
        .await
        .unwrap();
    assert!(db
        .confirm_pps_payout(attempt, &"d".repeat(64))
        .await
        .is_err());
    assert_eq!(
        db.confirm_pps_payout(attempt, CANARY_TXID).await.unwrap(),
        1
    );
    assert_eq!(
        db.confirm_pps_payout(attempt, CANARY_TXID).await.unwrap(),
        0
    );
    let paid = db.pps_invariant().await.unwrap();
    assert_eq!(
        (
            paid.pending_zatoshis,
            paid.paying_zatoshis,
            paid.paid_zatoshis
        ),
        (0, 0, MINER_SUBSIDY)
    );
    assert_eq!(paid.gross_subzatoshis, accepted.gross_subzatoshis);
    assert_eq!(
        db.pps_funding_snapshot().await.unwrap().paid_fees_zatoshis,
        10_000
    );
    let legacy: (i64, i64, i64) =
        sqlx::query_as("SELECT pending,paying,paid FROM balances WHERE miner_id=?1")
            .bind(miner.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(legacy, (123, 0, 0));
    let legacy_payouts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payouts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(legacy_payouts, 0);
    let legacy_credits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM block_credits")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(legacy_credits, 0);
    let pps_payouts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pps_payouts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pps_payouts, 1);

    // Failure injection into the genuine acceptance path: no partial share or
    // partial liability may survive a refused credit, and (audit B1) the found
    // block is submitted whatever PPS admission decides.
    for failure in ["cap", "lease", "funding", "database"] {
        let (fdb, fpool) = reconciler::tests::setup_db().await;
        let mut fpolicy = policy.clone();
        if failure == "cap" {
            fpolicy.max_liability_zatoshis = MINER_SUBSIDY - 1;
            fpolicy.max_payout_zatoshis = MINER_SUBSIDY - 1;
        }
        fdb.initialize_pps_epoch(
            &fpolicy.epoch_config(),
            Some(&synthetic_funding(&fdb, &fpolicy.epoch_config()).await),
        )
        .await
        .unwrap();
        let failing = Harness::new(
            fdb.clone(),
            Arc::new(ZcashRpcClient::new(&node.url)),
            Arc::new(ZcashRpcClient::new(&wallet.url)),
            fpolicy.epoch_config(),
            synthetic_lease(),
            block.clone(),
        )
        .await;
        if failure == "lease" {
            failing.invalidate_lease().await;
        }
        if failure == "funding" {
            failing.invalidate_funding().await;
        }
        if failure == "database" {
            sqlx::query("CREATE TRIGGER reject_test_credit BEFORE INSERT ON pps_events BEGIN SELECT RAISE(ABORT, 'synthetic commit failure'); END")
                .execute(&fpool).await.unwrap();
        }
        let submits_before = node.calls("submitblock").len();
        let outcome = failing.submit("failure-session", &block[143..1487]).await;
        if failure == "funding" {
            // Never-reject: a stale funding lease is advisory at credit time.
            assert!(outcome.is_ok(), "{failure}: advisory funding must still credit");
        } else {
            // Credit refused (cap, chain lease, database), but the block-solving
            // share is still accepted and nothing partial is credited.
            assert!(outcome.as_ref().is_ok_and(|r| r.is_block), "{failure}: block must be accepted");
            assert_uncredited(&fdb).await;
        }
        assert_eq!(node.calls("submitblock").len(), submits_before + 1,
            "{failure}: a found block must be submitted");
    }
    quote_health_actual_validator_case(&node,&wallet,&block).await;
    drop(restarted);
    pool.close().await;
}

// Exact production validator + telemetry sampler, not a copied pricing path.
// All credits below are isolated synthetic ledger fixtures, never wallet sends.
async fn quote_health_actual_validator_case(node:&FakeRpc,wallet:&FakeRpc,block:&[u8]) {
    use pool_db::pps_live::{PpsCredit,PPS_SCALE};
    use pps_credit_health::CreditAdmissionState;
    for cap_rejected in [false,true] {
        let (db,pool)=reconciler::tests::setup_db().await;
        let mut policy=synthetic_policy();
        policy.max_liability_zatoshis=95_000_000_000;
        policy.total_exposure_zatoshis=100_000_000_000;
        policy.fee_allowance_zatoshis=5_000_000_000;
        let epoch=policy.epoch_config();
        db.initialize_pps_epoch(&epoch,Some(&synthetic_funding(&db,&epoch).await)).await.unwrap();
        if cap_rejected {
            let miner=db.get_or_create_miner(CANARY_ADDRESS).await.unwrap();
            let worker=db.get_or_create_worker(miner.id,"synthetic-seed").await.unwrap();
            let now=Utc::now().timestamp();
            // Seed a valid ledger through its normal transactional API, leaving
            // a POSITIVE remainder one zatoshi below the real fixture's price.
            db.credit_pps_share(&epoch,&PpsCredit {proof_id:"1".repeat(64),quote_id:"2".repeat(64),
                worker_id:worker.id,job_id:"synthetic-seed".into(),session_id:"synthetic-seed".into(),
                difficulty:1.0,is_block:false,quote_height:1,network_target_be:[1;32],
                assigned_share_target_be:[2;32],miner_subsidy_zats:MINER_SUBSIDY as u64,
                amount_subzatoshis:(policy.max_liability_zatoshis-(MINER_SUBSIDY-1)) as u128*PPS_SCALE,
                accepted_at_unix:now},Some(&synthetic_lease()),Some(&synthetic_funding(&db,&epoch).await),now).await.unwrap();
        }
        let harness=actual_validator::Harness::new_for_route(db.clone(),Arc::new(ZcashRpcClient::new(&node.url)),
            Arc::new(ZcashRpcClient::new(&wallet.url)),epoch,synthetic_lease(),block.to_vec(),
            pool_core::pps_funding::PpsFundingRoute::ZecdConventionalTestnet{hold_new_legacy_sends:true}).await;
        let before=harness.credit_health().await;
        // No share priced yet is not "unknown": the gates themselves are healthy.
        assert_eq!(before.state,CreditAdmissionState::Ready); assert_eq!(before.category,"ok");
        let result=harness.submit("quote-observation",&block[143..1487]).await;
        // Audit B1: this share solves a block, so it is accepted and its block
        // submitted even when the cap refuses its credit.
        assert!(result.as_ref().is_ok_and(|r| r.is_block), "cap_rejected={cap_rejected}");
        let health=harness.credit_health().await;
        assert_eq!(health.state,if cap_rejected {CreditAdmissionState::Paused}else{CreditAdmissionState::Ready});
        assert_eq!(health.current_quote_fits,Some(!cap_rejected));
        assert_eq!(health.category,if cap_rejected {"current_quote_insufficient"}else{"ok"});
        assert_eq!(health.budget_low,cap_rejected);
        if cap_rejected {assert_eq!(db.pps_invariant().await.unwrap().accepted_events,1);}
        // Block the actual SQLite observation after its opening quote/job read.
        // Poll once to Pending deterministically; no timing sleeps or injected
        // production hooks. A same-price observation must not renew the old one.
        for change in ["same_price","different_price","new_job"] {
            let held=pool.acquire().await.unwrap();
            // Exclude a cooperative-budget yield before the DB acquire: the
            // only pending boundary is the intentionally held connection.
            let mut sample=Box::pin(tokio::task::unconstrained(harness.credit_health()));
            std::future::poll_fn(|cx| {
                assert!(std::future::Future::poll(sample.as_mut(),cx).is_pending());
                std::task::Poll::Ready(())
            }).await;
            match change {
                "same_price"=>harness.replace_quote(MINER_SUBSIDY as u128*PPS_SCALE),
                "different_price"=>harness.replace_quote(MINER_SUBSIDY as u128*PPS_SCALE+1),
                _=>harness.replace_notify().await,
            }
            drop(held);
            let sampled=sample.await;
            if change=="same_price" {
                assert_eq!(sampled.state,health.state);
                assert_eq!(sampled.quote_checked_at_unix,health.quote_checked_at_unix);
                assert_eq!(sampled.quote_expires_at_unix,health.quote_expires_at_unix);
            } else {
                // A changed price or tip is informational: the state stays what the
                // gates say, and headroom below the last observed price stays a pause.
                assert_eq!(sampled.state,if cap_rejected {CreditAdmissionState::Paused}else{CreditAdmissionState::Ready});
                assert_eq!(sampled.category,if cap_rejected {"cap_exhausted"}else{"ok"});
                assert_eq!(sampled.current_quote_fits,None);
            }
        }
        assert_eq!(harness.credit_health().await.category,if cap_rejected {"cap_exhausted"}else{"ok"});
        assert!(wallet.calls("z_sendmany").is_empty());
        drop(harness); pool.close().await;
    }
}
