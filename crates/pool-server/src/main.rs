use std::collections::HashMap;
use std::sync::atomic::AtomicI64;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;
use sqlx::sqlite::SqlitePoolOptions;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use node_rpc::ZcashRpcClient;
use pool_api::{ApiState, AppState};
use pool_core::{BlockAssembler, JobManager, ShareValidator, VardiffConfig};
use pool_db::PoolDb;
use rewards::PplnsCalculator;
use stratum::StratumServer;

const ZATOSHIS_PER_ZEC: f64 = 100_000_000.0;

#[derive(Debug, Deserialize)]
struct Config {
    pool: PoolConfig,
    stratum: StratumConfig,
    node: NodeConfig,
    difficulty: DifficultyConfig,
    pplns: PplnsConfig,
    #[serde(default)]
    payout: PayoutConfig,
    api: ApiConfig,
    database: DatabaseConfig,
}

#[derive(Debug, Deserialize)]
struct PoolConfig {
    name: String,
    fee_percent: f64,
    #[serde(default = "default_network")]
    network: String,
}

fn default_network() -> String { "testnet".to_string() }

#[derive(Debug, Deserialize)]
struct StratumConfig {
    #[serde(default)]
    listen_addr: Option<String>,
    #[serde(default)]
    listen_addrs: Option<Vec<String>>,
    nonce1_size: usize,
    /// Per-port initial difficulty overrides (port number as string -> difficulty).
    #[serde(default)]
    port_difficulty: HashMap<String, f64>,
}

impl StratumConfig {
    fn addrs(&self) -> Vec<String> {
        if let Some(ref addrs) = self.listen_addrs {
            addrs.clone()
        } else if let Some(ref addr) = self.listen_addr {
            vec![addr.clone()]
        } else {
            vec!["0.0.0.0:3333".to_string()]
        }
    }
}

#[derive(Debug, Deserialize)]
struct NodeConfig {
    rpc_url: String,
    #[serde(default)]
    rpc_user: Option<String>,
    #[serde(default)]
    rpc_password: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DifficultyConfig {
    initial_target: String,
    target_shares_per_minute: f64,
    retarget_interval_secs: u64,
}

#[derive(Debug, Deserialize)]
struct PplnsConfig {
    window_multiplier: f64,
}

#[derive(Debug, Deserialize)]
struct PayoutConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    pool_address: Option<String>,
    #[serde(default)]
    mining_address: Option<String>,
    #[serde(default)]
    wallet_rpc_url: Option<String>,
    #[serde(default)]
    wallet_rpc_user: Option<String>,
    #[serde(default)]
    wallet_rpc_password: Option<String>,
    #[serde(default = "default_minimum_payout")]
    minimum_payout: f64,
    #[serde(default = "default_payout_interval")]
    interval_secs: u64,
    #[serde(default = "default_maturity")]
    maturity_confirmations: u64,
}

fn default_minimum_payout() -> f64 { 0.01 }
fn default_payout_interval() -> u64 { 300 }
fn default_maturity() -> u64 { 100 }

impl Default for PayoutConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pool_address: None,
            mining_address: None,
            wallet_rpc_url: None,
            wallet_rpc_user: None,
            wallet_rpc_password: None,
            minimum_payout: default_minimum_payout(),
            interval_secs: default_payout_interval(),
            maturity_confirmations: default_maturity(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ApiConfig {
    listen_addr: String,
}

#[derive(Debug, Deserialize)]
struct DatabaseConfig {
    url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("Starting Zcash Mining Pool");

    // Load configuration
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config/pool.toml".to_string());
    let config_str = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read config file: {config_path}"))?;
    let config: Config = toml::from_str(&config_str)
        .with_context(|| "Failed to parse config file")?;

    let stratum_addrs = config.stratum.addrs();
    info!(
        name = %config.pool.name,
        fee = %config.pool.fee_percent,
        stratum = ?stratum_addrs,
        api = %config.api.listen_addr,
        node = %config.node.rpc_url,
        "Configuration loaded"
    );

    // Initialize database
    let db_pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect(&config.database.url)
        .await
        .with_context(|| "Failed to connect to database")?;
    let db = PoolDb::new(db_pool);
    db.run_migrations()
        .await
        .with_context(|| "Failed to run migrations")?;
    info!("Database initialized");

    // Backfill luck_percent for any blocks that don't have it yet.
    // This runs once at startup using the current network hashrate as an approximation.
    {
        let all_blocks = db.get_all_blocks_by_height().await.unwrap_or_default();
        let needs_backfill: Vec<_> = all_blocks.iter().filter(|b| b.luck_percent.is_none()).collect();
        if !needs_backfill.is_empty() {
            info!(count = needs_backfill.len(), "Backfilling block luck values");
            // We need RPC for network hashrate — initialize a temporary client
            let temp_rpc = match (&config.node.rpc_user, &config.node.rpc_password) {
                (Some(user), Some(pass)) => ZcashRpcClient::with_auth(&config.node.rpc_url, user, pass),
                _ => ZcashRpcClient::new(&config.node.rpc_url),
            };
            let network_hashrate = temp_rpc.get_network_sol_ps(Some(120)).await.unwrap_or(0.0);
            if network_hashrate > 0.0 {
                let expected_work = network_hashrate * 75.0; // BLOCK_TIME_SECS
                // Compute difficulty_multiplier inline (pool_target not yet available here,
                // but we can parse it from config).
                let temp_pool_target = pool_core::parse_target(&config.difficulty.initial_target).ok();
                let temp_diff_mult = temp_pool_target.map(|t| {
                    let target_f64 = t.iter().enumerate().fold(0.0f64, |acc, (i, &b)| {
                        acc + (b as f64) * 256.0f64.powi(31 - i as i32)
                    });
                    if target_f64 > 0.0 { 2.0f64.powi(256) / target_f64 } else { 1.0 }
                }).unwrap_or(1.0);

                for (idx, block) in all_blocks.iter().enumerate() {
                    if block.luck_percent.is_some() {
                        continue;
                    }
                    // Sum share difficulties between previous block and this block
                    let since = if idx > 0 {
                        all_blocks[idx - 1].created_at.clone()
                    } else {
                        "1970-01-01 00:00:00".to_string()
                    };
                    let diff_sum = db.get_difficulty_sum_between(&since, &block.created_at).await.unwrap_or(0.0);
                    let actual_work = diff_sum * temp_diff_mult;
                    if actual_work > 0.0 {
                        let luck = (actual_work / expected_work) * 100.0;
                        if let Err(e) = db.update_block_luck(block.id, luck).await {
                            warn!(block_id = block.id, error = %e, "Failed to backfill block luck");
                        }
                    }
                }
                info!("Block luck backfill complete");
            } else {
                warn!("Skipping luck backfill: network hashrate unavailable");
            }
        }
    }

    // Initialize Zcash RPC client
    let rpc = match (&config.node.rpc_user, &config.node.rpc_password) {
        (Some(user), Some(pass)) => {
            info!("Using RPC authentication");
            Arc::new(ZcashRpcClient::with_auth(&config.node.rpc_url, user, pass))
        }
        _ => Arc::new(ZcashRpcClient::new(&config.node.rpc_url)),
    };

    // Shared latest notify message — used by both stratum (send on subscribe)
    // and job manager (updates on each new template).
    let latest_notify: Arc<tokio::sync::RwLock<Option<stratum::ServerMessage>>> =
        Arc::new(tokio::sync::RwLock::new(None));

    // Initialize Stratum server
    let (event_tx, event_rx) = mpsc::channel(1024);
    let (stratum, _notify_rx) = StratumServer::new_with_latest_notify(
        config.stratum.nonce1_size,
        event_tx,
        Arc::clone(&latest_notify),
    );
    let stratum = Arc::new(stratum);

    // Stall detection: last time we got a block template (unix ms)
    let last_template_at_ms = Arc::new(AtomicI64::new(0));

    // Initialize Job Manager (updates last_template_at_ms on each successful poll)
    let job_manager = JobManager::new_with_stall_tracking_and_notify(
        Arc::clone(&rpc),
        Arc::clone(&stratum),
        Some(Arc::clone(&last_template_at_ms)),
        Arc::clone(&latest_notify),
    );
    let jobs = job_manager.jobs();

    // Initialize Block Assembler
    let block_assembler = Arc::new(BlockAssembler::new(Arc::clone(&rpc)));

    // Parse initial pool target
    let pool_target = pool_core::parse_target(&config.difficulty.initial_target)
        .map_err(|e| anyhow::anyhow!(e))?;

    // Convert initial_target to a difficulty value for vardiff
    let initial_difficulty = {
        let target_f64 = pool_target.iter().enumerate().fold(0.0f64, |acc, (i, &b)| {
            acc + (b as f64) * 256.0f64.powi(31 - i as i32)
        });
        let pow_limit: f64 = 2.0f64.powi(251) - 1.0;
        if target_f64 > 0.0 { pow_limit / target_f64 } else { 1.0 }
    };

    let vardiff_config = VardiffConfig {
        initial_difficulty,
        target_shares_per_minute: config.difficulty.target_shares_per_minute,
        retarget_interval_secs: config.difficulty.retarget_interval_secs as f64,
    };

    // Initialize PPLNS Calculator
    let pplns_window = (config.pplns.window_multiplier * 1000.0) as i64;
    let pplns = Arc::new(PplnsCalculator::new(
        db.clone(),
        pplns_window,
        config.pool.fee_percent / 100.0,
    ));

    // Initialize Share Validator
    let port_difficulty: HashMap<u16, f64> = config.stratum.port_difficulty.iter()
        .filter_map(|(k, &v)| k.parse::<u16>().ok().map(|port| (port, v)))
        .collect();
    if !port_difficulty.is_empty() {
        info!(?port_difficulty, "Per-port difficulty overrides loaded");
    }
    // Compute difficulty_multiplier for share→Sol/s conversion
    let difficulty_multiplier = {
        let target_f64 = pool_target.iter().enumerate().fold(0.0f64, |acc, (i, &b)| {
            acc + (b as f64) * 256.0f64.powi(31 - i as i32)
        });
        if target_f64 > 0.0 { 2.0f64.powi(256) / target_f64 } else { 1.0 }
    };

    let share_validator = ShareValidator::new(
        db.clone(),
        Arc::clone(&stratum),
        jobs,
        block_assembler,
        Arc::clone(&pplns),
        pool_target,
        vardiff_config,
        port_difficulty.clone(),
        latest_notify,
        Arc::clone(&rpc),
        difficulty_multiplier,
    );

    // Wallet RPC for Zallet monitoring (and payouts)
    let wallet_rpc = config
        .payout
        .wallet_rpc_url
        .as_ref()
        .map(|url| {
            let rpc = match (&config.payout.wallet_rpc_user, &config.payout.wallet_rpc_password) {
                (Some(u), Some(p)) => ZcashRpcClient::with_auth(url, u, p),
                _ => ZcashRpcClient::new(url),
            };
            Arc::new(rpc)
        });

    // Build stratum port info for the dashboard.
    let stratum_ports: Vec<pool_api::StratumPortInfo> = stratum_addrs.iter().map(|addr| {
        let port: u16 = addr.split(':').last().and_then(|p| p.parse().ok()).unwrap_or(0);
        let description = match port {
            3334 => "300 KSol/s - 3 MSol/s".to_string(),
            3335 => "3-50 MSol/s".to_string(),
            3336 => "50+ MSol/s".to_string(),
            _ => "Default".to_string(),
        };
        pool_api::StratumPortInfo { port, description }
    }).collect();

    // Initialize API (shares last_template_at_ms for /health and pool stats)
    let api_state: AppState = Arc::new(ApiState {
        db: db.clone(),
        rpc: Arc::clone(&rpc),
        pool_name: config.pool.name.clone(),
        pool_fee: config.pool.fee_percent,
        network: config.pool.network.clone(),
        stratum_port: stratum_addrs.first()
            .and_then(|a| a.split(':').last())
            .and_then(|p| p.parse().ok())
            .unwrap_or(3333),
        stratum_ports,
        last_template_at_ms: Some(last_template_at_ms),
        wallet_rpc,
        pool_address: config.payout.pool_address.clone(),
        mining_address: config.payout.mining_address.clone(),
        min_payout_zatoshis: (config.payout.minimum_payout * ZATOSHIS_PER_ZEC) as i64,
        maturity_confirmations: config.payout.maturity_confirmations,
        network_blocks_cache: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        stats_history: pool_api::StatsHistory::new(),
        difficulty_multiplier: {
            // Convert shares/sec → Sol/s.  Each share means a hash below pool_target,
            // so on average each share takes 2^256 / target hashes to find.
            let target_f64 = pool_target.iter().enumerate().fold(0.0f64, |acc, (i, &b)| {
                acc + (b as f64) * 256.0f64.powi(31 - i as i32)
            });
            if target_f64 > 0.0 { 2.0f64.powi(256) / target_f64 } else { 1.0 }
        },
    });
    // Spawn background stats history recorder (10s snapshots, 1hr ring buffer).
    let history_state = Arc::clone(&api_state);
    let history_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let snapshot = pool_api::compute_stats_snapshot(&history_state).await;
            history_state.stats_history.push(snapshot).await;
        }
    });

    // Spawn background network cache warmer so /network page loads instantly.
    // Warms all three ranges (1h, 24h, 1w) every 55s; warm_cache skips ranges
    // whose TTL hasn't expired yet, so heavier ranges refresh less often.
    let net_cache_state = Arc::clone(&api_state);
    let net_cache_handle = tokio::spawn(async move {
        loop {
            pool_api::warm_network_cache(&net_cache_state, &["1h", "24h", "1w"]).await;
            tokio::time::sleep(Duration::from_secs(55)).await;
        }
    });

    let router = pool_api::build_router(api_state);

    // Spawn all services — one stratum listener per address
    let mut stratum_handles = Vec::new();
    for addr in &stratum_addrs {
        let stratum = Arc::clone(&stratum);
        let addr = addr.clone();
        stratum_handles.push(tokio::spawn(async move {
            if let Err(e) = stratum.listen(&addr).await {
                error!(error = %e, addr = %addr, "Stratum server failed");
            }
        }));
    }

    let job_handle = tokio::spawn(async move {
        job_manager.run(Duration::from_millis(500)).await;
    });

    let share_handle = tokio::spawn(async move {
        share_validator.run(event_rx).await;
    });

    let api_addr = config.api.listen_addr.clone();
    let api_handle = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(&api_addr)
            .await
            .expect("Failed to bind API listener");
        info!(address = %api_addr, "API server listening");
        axum::serve(listener, router)
            .await
            .expect("API server failed");
    });

    let payout_handle = if config.payout.enabled {
        let pool_address = config.payout.pool_address.clone()
            .expect("payout.pool_address is required when payouts are enabled");
        let mining_address = config.payout.mining_address.clone()
            .unwrap_or_else(|| pool_address.clone());
        let wallet_url = config.payout.wallet_rpc_url.clone()
            .expect("payout.wallet_rpc_url is required when payouts are enabled");
        let wallet_rpc = match (&config.payout.wallet_rpc_user, &config.payout.wallet_rpc_password) {
            (Some(user), Some(pass)) => {
                Arc::new(ZcashRpcClient::with_auth(&wallet_url, user, pass))
            }
            _ => Arc::new(ZcashRpcClient::new(&wallet_url)),
        };
        let min_payout_zatoshis = (config.payout.minimum_payout * ZATOSHIS_PER_ZEC) as i64;
        let interval = Duration::from_secs(config.payout.interval_secs);
        let maturity = config.payout.maturity_confirmations;
        let payout_db = db.clone();
        let node_rpc = Arc::clone(&rpc);
        let payout_network = config.pool.network.clone();
        info!(
            pool_address = %pool_address,
            mining_address = %mining_address,
            wallet_rpc = %wallet_url,
            min_payout_zec = config.payout.minimum_payout,
            interval_secs = config.payout.interval_secs,
            maturity_confirmations = maturity,
            "Payout loop enabled (using wallet RPC)"
        );
        Some(tokio::spawn(async move {
            run_payout_loop(
                payout_db, node_rpc, wallet_rpc,
                &pool_address, &mining_address,
                min_payout_zatoshis, maturity, interval,
                &payout_network,
            ).await;
        }))
    } else {
        info!("Payouts disabled");
        None
    };

    info!(
        "Pool is running!\n\
         \n\
         Stratum: {:?}\n\
         Dashboard: http://{}\n\
         API: http://{}/api/pool/stats\n",
        stratum_addrs,
        config.api.listen_addr,
        config.api.listen_addr,
    );

    // Wait for shutdown signal
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl+c");
    info!("Shutdown signal received, stopping services...");

    for h in &stratum_handles { h.abort(); }
    job_handle.abort();
    share_handle.abort();
    api_handle.abort();
    history_handle.abort();
    net_cache_handle.abort();
    if let Some(h) = payout_handle { h.abort(); }

    info!("Pool shut down gracefully");
    Ok(())
}

/// Maximum number of shielding batches per payout cycle.
/// Each batch shields up to 50 UTXOs (~5s proof time each).
const MAX_SHIELD_BATCHES_PER_CYCLE: u32 = 10;

/// Timeout for waiting on a single async operation (shield or sendmany).
const OP_POLL_TIMEOUT: Duration = Duration::from_secs(120);

async fn run_payout_loop(
    db: PoolDb,
    node_rpc: Arc<ZcashRpcClient>,
    wallet_rpc: Arc<ZcashRpcClient>,
    pool_address: &str,
    mining_address: &str,
    min_payout_zatoshis: i64,
    maturity_confirmations: u64,
    interval: Duration,
    network: &str,
) {
    info!("Payout loop started");
    loop {
        tokio::time::sleep(interval).await;

        // Phase 1: Check block maturity
        if let Err(e) = check_block_maturity(&db, &node_rpc, maturity_confirmations).await {
            error!(error = %e, "Block maturity check failed");
        }

        // Phase 2: Shield mature coinbase UTXOs (transparent -> shielded).
        // Shields in batches of 50 UTXOs, up to MAX_SHIELD_BATCHES_PER_CYCLE per cycle.
        if let Err(e) = shield_coinbase(&wallet_rpc, mining_address, pool_address).await {
            error!(error = %e, "Coinbase shielding failed (will retry next cycle)");
        }

        // Phase 3: Pay miners from shielded pool (only if balance is sufficient)
        match process_payouts(&db, &wallet_rpc, pool_address, min_payout_zatoshis, network).await {
            Ok(count) => {
                if count > 0 {
                    info!(payouts = count, "Payout round completed");
                }
            }
            Err(e) => {
                error!(error = %e, "Payout round failed");
            }
        }
    }
}

fn reverse_hex_bytes(hex_str: &str) -> String {
    let bytes = hex::decode(hex_str).unwrap_or_default();
    let reversed: Vec<u8> = bytes.into_iter().rev().collect();
    hex::encode(reversed)
}

async fn check_block_maturity(
    db: &PoolDb,
    node_rpc: &ZcashRpcClient,
    maturity_confirmations: u64,
) -> anyhow::Result<()> {
    let current_height = node_rpc.get_block_count().await
        .map_err(|e| anyhow::anyhow!("getblockcount failed: {e}"))?;

    let pending_blocks = db.get_pending_blocks().await?;
    if pending_blocks.is_empty() {
        return Ok(());
    }

    for block in &pending_blocks {
        let confs = current_height as i64 - block.height;
        if confs < maturity_confirmations as i64 {
            continue;
        }

        let chain_hash = node_rpc.get_block_hash(block.height as u64).await
            .map_err(|e| anyhow::anyhow!("getblockhash failed: {e}"))?;

        let pool_hash_reversed = reverse_hex_bytes(&block.hash);
        if chain_hash == pool_hash_reversed || chain_hash == block.hash {
            db.update_block_status(block.id, "confirmed").await?;
            info!(
                height = block.height,
                confirmations = confs,
                "Block confirmed (mature)"
            );
        } else {
            db.update_block_status(block.id, "orphaned").await?;
            db.reverse_block_credits(block.reward).await?;
            info!(
                height = block.height,
                "Block orphaned (hash mismatch, credits reversed)"
            );
        }
    }

    Ok(())
}

async fn shield_coinbase(
    wallet_rpc: &ZcashRpcClient,
    mining_address: &str,
    pool_address: &str,
) -> anyhow::Result<()> {
    if mining_address == pool_address {
        return Ok(());
    }

    let mut total_shielded_utxos: u64 = 0;
    let mut total_shielded_value: f64 = 0.0;

    for batch in 1..=MAX_SHIELD_BATCHES_PER_CYCLE {
        // Call z_shieldcoinbase with limit=50 (batch 50 UTXOs per transaction)
        let result = match wallet_rpc.z_shield_coinbase(mining_address, pool_address, Some(50)).await {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("{e}");
                // These are expected "nothing to do" conditions, not errors
                if msg.contains("No spendable transparent outputs")
                    || msg.contains("Insufficient")
                    || msg.contains("No funds")
                {
                    if total_shielded_utxos > 0 {
                        info!(
                            total_utxos = total_shielded_utxos,
                            total_value = total_shielded_value,
                            batches = batch - 1,
                            "Shielding complete, no more UTXOs"
                        );
                    }
                    return Ok(());
                }
                return Err(anyhow::anyhow!("z_shieldcoinbase failed: {e}"));
            }
        };

        let shielding_utxos = result.get("shieldingUTXOs").and_then(|v| v.as_u64()).unwrap_or(0);
        let shielding_value = result.get("shieldingValue").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let remaining_utxos = result.get("remainingUTXOs").and_then(|v| v.as_u64()).unwrap_or(0);
        let opid = result.get("opid").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();

        if shielding_utxos == 0 {
            break;
        }

        info!(
            batch,
            opid = %opid,
            utxos = shielding_utxos,
            value_zec = shielding_value,
            remaining = remaining_utxos,
            "Shielding coinbase batch"
        );

        // Wait for this batch to complete before starting next
        match wait_for_operation(wallet_rpc, &opid).await? {
            OpResult::Success(txid) => {
                total_shielded_utxos += shielding_utxos;
                total_shielded_value += shielding_value;
                info!(
                    batch,
                    txid = %txid,
                    "Shielding batch complete"
                );
            }
            OpResult::Failed(msg) => {
                // Insufficient funds during building is non-fatal (fees exceeded inputs)
                if msg.contains("Insufficient") || msg.contains("No funds") {
                    warn!(batch, error = %msg, "Shielding batch skipped (insufficient for fee)");
                    break;
                }
                return Err(anyhow::anyhow!("Shielding batch {batch} failed: {msg}"));
            }
        }

        // If no more UTXOs remain, stop batching
        if remaining_utxos == 0 {
            break;
        }
    }

    if total_shielded_utxos > 0 {
        info!(
            total_utxos = total_shielded_utxos,
            total_value_zec = total_shielded_value,
            "Shielding round complete, waiting for confirmations"
        );

        // Wait for shielded funds to get at least 1 confirmation so they're spendable
        // by process_payouts. Poll for up to 5 minutes.
        for _ in 0..60 {
            tokio::time::sleep(Duration::from_secs(5)).await;
            if let Ok(bal) = wallet_rpc.call_raw::<serde_json::Value>(
                "z_gettotalbalance", serde_json::json!([1, true])
            ).await {
                let private = bal.get("private")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);
                if private > 0.0 {
                    info!(private_balance = private, "Shielded funds confirmed (1+ conf)");
                    return Ok(());
                }
            }
        }
        warn!("Timed out waiting for shielded fund confirmations");
    }

    Ok(())
}

/// Result of waiting for an async wallet operation.
enum OpResult {
    Success(String), // txid
    Failed(String),  // error message
}

/// Poll z_getoperationstatus until the operation succeeds or fails,
/// with a timeout of OP_POLL_TIMEOUT.
async fn wait_for_operation(
    wallet_rpc: &ZcashRpcClient,
    opid: &str,
) -> anyhow::Result<OpResult> {
    let deadline = tokio::time::Instant::now() + OP_POLL_TIMEOUT;

    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;

        if tokio::time::Instant::now() > deadline {
            return Err(anyhow::anyhow!(
                "Timed out waiting for operation {opid} after {}s",
                OP_POLL_TIMEOUT.as_secs()
            ));
        }

        let statuses = wallet_rpc.z_get_operation_status(&[opid]).await
            .map_err(|e| anyhow::anyhow!("z_getoperationstatus failed: {e}"))?;

        if let Some(status) = statuses.first() {
            let state = status.get("status").and_then(|s| s.as_str()).unwrap_or("");
            match state {
                "success" => {
                    let txid = status.get("result")
                        .and_then(|r| r.get("txid"))
                        .or_else(|| {
                            // z_shieldcoinbase returns txids array
                            status.get("result")
                                .and_then(|r| r.get("txids"))
                                .and_then(|a| a.as_array())
                                .and_then(|a| a.first())
                        })
                        .and_then(|t| t.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    return Ok(OpResult::Success(txid));
                }
                "failed" => {
                    let msg = status.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown error")
                        .to_string();
                    return Ok(OpResult::Failed(msg));
                }
                _ => {
                    // "executing" or "queued" — keep waiting
                }
            }
        }
    }
}

async fn process_payouts(
    db: &PoolDb,
    rpc: &ZcashRpcClient,
    pool_address: &str,
    min_payout_zatoshis: i64,
    network: &str,
) -> anyhow::Result<usize> {
    let pending = db.get_pending_payouts(min_payout_zatoshis).await?;
    if pending.is_empty() {
        return Ok(0);
    }

    let total_payout_zatoshis: i64 = pending.iter().map(|p| p.amount).sum();
    let total_payout_zec = total_payout_zatoshis as f64 / ZATOSHIS_PER_ZEC;

    // Check shielded balance (minconf=3 to exclude recently-shielded notes that
    // may not yet be spendable because the wallet hasn't scanned the block yet).
    let private_balance = match rpc.call_raw::<serde_json::Value>(
        "z_gettotalbalance", serde_json::json!([3, true])
    ).await {
        Ok(bal) => {
            bal.get("private")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0)
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to check wallet balance: {e}"));
        }
    };

    // Use 90% of private balance as available to account for:
    // - ZIP 317 fees (scale with number of outputs)
    // - Recently-shielded funds that may not yet be spendable
    let available_zec = (private_balance * 0.90).max(0.0);

    if available_zec < min_payout_zatoshis as f64 / ZATOSHIS_PER_ZEC {
        info!(
            private_balance,
            available_zec,
            "Shielded balance too low for any payouts"
        );
        return Ok(0);
    }

    // Scale payouts proportionally if we can't cover everyone.
    // Each miner gets: (their_pending / total_pending) * available_balance
    let scale = if available_zec >= total_payout_zec {
        1.0
    } else {
        available_zec / total_payout_zec
    };

    // Build scaled payout list in zatoshis, skipping miners below minimum
    // or with invalid addresses.
    let mut payout_list: Vec<(usize, i64)> = Vec::new(); // (index into pending, scaled_zatoshis)
    for (i, p) in pending.iter().enumerate() {
        // Skip invalid addresses: must start with a known Zcash prefix
        if !is_valid_zcash_address(&p.address, network) {
            warn!(
                miner_id = p.miner_id,
                address = %p.address,
                "Skipping payout: invalid address format"
            );
            continue;
        }
        let scaled_zatoshis = (p.amount as f64 * scale).floor() as i64;
        if scaled_zatoshis >= min_payout_zatoshis {
            payout_list.push((i, scaled_zatoshis));
        }
    }

    if payout_list.is_empty() {
        info!(
            private_balance,
            scale,
            "All scaled payouts below minimum, waiting for more shielding"
        );
        return Ok(0);
    }

    let actual_total_zatoshis: i64 = payout_list.iter().map(|(_, amt)| *amt).sum();
    let actual_total_zec = actual_total_zatoshis as f64 / ZATOSHIS_PER_ZEC;
    info!(
        miners = payout_list.len(),
        total_zec = actual_total_zec,
        private_balance,
        scale,
        "Processing payouts"
    );

    // Try z_sendmany, handling "Insufficient balance" by rescaling from the
    // wallet's actual spendable balance (reported in the error message).
    let (opid, payout_list) = {
        let mut current_list = payout_list;
        let mut retries = 0u32;
        loop {
            let amounts: Vec<(&str, f64)> = current_list
                .iter()
                .map(|(i, zats)| (pending[*i].address.as_str(), *zats as f64 / ZATOSHIS_PER_ZEC))
                .collect();

            match rpc.z_sendmany(pool_address, &amounts).await {
                Ok(opid) => break (opid, current_list),
                Err(e) => {
                    let msg = format!("{e}");
                    // Parse "Insufficient balance (have XXXX, need YYYY including fee)"
                    if retries < 2 && msg.contains("Insufficient balance") {
                        if let Some(have_zats) = parse_have_balance(&msg) {
                            // Use 95% of actual spendable balance to leave room for fees
                            let actual_available = have_zats as f64 * 0.95;
                            let current_total: f64 = current_list.iter().map(|(_, z)| *z as f64).sum();
                            if actual_available < min_payout_zatoshis as f64 || current_total <= 0.0 {
                                return Err(anyhow::anyhow!("Wallet balance too low: {msg}"));
                            }
                            let rescale = actual_available / current_total;
                            info!(
                                have_zats,
                                rescale,
                                retry = retries + 1,
                                "Rescaling payouts based on actual wallet balance"
                            );
                            current_list = current_list
                                .into_iter()
                                .filter_map(|(i, old_zats)| {
                                    let new_zats = (old_zats as f64 * rescale).floor() as i64;
                                    if new_zats >= min_payout_zatoshis {
                                        Some((i, new_zats))
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            if current_list.is_empty() {
                                return Err(anyhow::anyhow!("All payouts below minimum after rescaling"));
                            }
                            retries += 1;
                            continue;
                        }
                    }
                    return Err(anyhow::anyhow!("z_sendmany failed: {e}"));
                }
            }
        }
    };

    info!(opid = %opid, "z_sendmany submitted, waiting for completion");

    // Wait for the operation with timeout
    let txid = match wait_for_operation(rpc, &opid).await? {
        OpResult::Success(txid) => {
            info!(opid = %opid, txid = %txid, "Payout transaction broadcast");
            txid
        }
        OpResult::Failed(msg) => {
            return Err(anyhow::anyhow!("z_sendmany failed: {msg}"));
        }
    };

    // Record the actual (scaled) payout amounts in the database.
    let mut count = 0;
    for (i, amt_zatoshis) in &payout_list {
        let p = &pending[*i];
        if let Err(e) = db.create_payout(p.miner_id, *amt_zatoshis, &txid).await {
            error!(miner_id = p.miner_id, error = %e, "Failed to record payout");
        } else {
            info!(
                miner_id = p.miner_id,
                address = %p.address,
                amount_zec = *amt_zatoshis as f64 / ZATOSHIS_PER_ZEC,
                txid = %txid,
                "Payout recorded"
            );
            count += 1;
        }
    }

    Ok(count)
}

/// Parse "have XXXX" from an "Insufficient balance (have XXXX, need YYYY ...)" error.
fn parse_have_balance(msg: &str) -> Option<i64> {
    let marker = "have ";
    let start = msg.find(marker)? + marker.len();
    let rest = &msg[start..];
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    rest[..end].parse::<i64>().ok()
}

/// Check if an address looks like a valid Zcash address for the given network.
/// Rejects wrong-network addresses, garbage strings, and anything else that would
/// cause z_sendmany to fail and block the entire payout batch.
fn is_valid_zcash_address(addr: &str, network: &str) -> bool {
    if network == "mainnet" {
        // Mainnet transparent: t1/t3
        // Mainnet sapling: zs
        // Mainnet unified: u (but not utest)
        addr.starts_with("t1")
            || addr.starts_with("t3")
            || addr.starts_with("zs")
            || (addr.starts_with('u') && !addr.starts_with("utest"))
    } else {
        // Testnet transparent: tm/t2
        // Testnet sapling: ztestsapling
        // Testnet unified: utest
        addr.starts_with("tm")
            || addr.starts_with("t2")
            || addr.starts_with("ztestsapling")
            || addr.starts_with("utest")
    }
}
