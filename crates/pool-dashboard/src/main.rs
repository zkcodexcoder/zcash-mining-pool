use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;
use sqlx::sqlite::SqlitePoolOptions;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use node_rpc::ZcashRpcClient;
use pool_api::{ApiState, AppState};
use pool_db::PoolDb;

const ZATOSHIS_PER_ZEC: f64 = 100_000_000.0;

// -- Config structs (subset of pool-server, reads the same pool.toml) --

#[derive(Debug, Deserialize)]
struct Config {
    pool: PoolConfig,
    stratum: StratumConfig,
    node: NodeConfig,
    difficulty: DifficultyConfig,
    #[serde(default)]
    payout: PayoutConfig,
    api: ApiConfig,
    database: DatabaseConfig,
    #[serde(default)]
    admin: Option<AdminConfig>,
    // Ignored sections: pplns (mining only)
}

#[derive(Debug, Deserialize)]
struct AdminConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_admin_addr")]
    listen_addr: String,
    password: String,
    #[serde(default)]
    pool_log: Option<String>,
    #[serde(default)]
    dashboard_log: Option<String>,
    #[serde(default)]
    zallet_log: Option<String>,
    #[serde(default)]
    zallet_datadir: Option<String>,
    #[serde(default)]
    zallet_binary: Option<String>,
}

fn default_admin_addr() -> String {
    "127.0.0.1:9090".to_string()
}

#[derive(Debug, Deserialize)]
struct PoolConfig {
    name: String,
    fee_percent: f64,
    #[serde(default = "default_network")]
    network: String,
    #[serde(default)]
    hostname: Option<String>,
    #[serde(default)]
    coinbase_tag: Option<String>,
    #[serde(default)]
    banner: Option<String>,
}

fn default_network() -> String {
    "testnet".to_string()
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct StratumConfig {
    #[serde(default)]
    listen_addr: Option<String>,
    #[serde(default)]
    listen_addrs: Option<Vec<String>>,
    #[serde(default)]
    nonce1_size: usize,
    #[serde(default)]
    port_difficulty: HashMap<String, f64>,
    #[serde(default)]
    ports: Vec<StratumPortConfig>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct StratumPortConfig {
    addr: String,
    #[serde(default = "default_port_description")]
    description: String,
    #[serde(default)]
    initial_difficulty: Option<f64>,
}

fn default_port_description() -> String {
    "Default".to_string()
}

impl StratumConfig {
    fn addrs(&self) -> Vec<String> {
        if !self.ports.is_empty() {
            self.ports.iter().map(|p| p.addr.clone()).collect()
        } else if let Some(ref addrs) = self.listen_addrs {
            addrs.clone()
        } else if let Some(ref addr) = self.listen_addr {
            vec![addr.clone()]
        } else {
            vec!["0.0.0.0:3333".to_string()]
        }
    }

    fn port_info(&self) -> Vec<pool_api::StratumPortInfo> {
        if !self.ports.is_empty() {
            self.ports
                .iter()
                .map(|p| {
                    let port: u16 = p
                        .addr
                        .split(':')
                        .last()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    pool_api::StratumPortInfo {
                        port,
                        description: p.description.clone(),
                    }
                })
                .collect()
        } else {
            self.addrs()
                .iter()
                .map(|addr| {
                    let port: u16 = addr
                        .split(':')
                        .last()
                        .and_then(|p| p.parse().ok())
                        .unwrap_or(0);
                    pool_api::StratumPortInfo {
                        port,
                        description: "Default".to_string(),
                    }
                })
                .collect()
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
#[allow(dead_code)]
struct DifficultyConfig {
    initial_target: String,
    #[serde(default)]
    target_shares_per_minute: f64,
    #[serde(default)]
    retarget_interval_secs: u64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
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

fn default_minimum_payout() -> f64 {
    0.01
}
fn default_payout_interval() -> u64 {
    300
}
fn default_maturity() -> u64 {
    100
}

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

// -- Main --

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("Starting Zcash Pool Dashboard (standalone)");

    // Usage: zcash-dashboard [config/pool.toml] [--port 8081]
    let args: Vec<String> = std::env::args().collect();
    let config_path = args.get(1)
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "config/pool.toml".to_string());
    let port_override: Option<String> = args.windows(2)
        .find(|w| w[0] == "--port")
        .map(|w| w[1].clone());

    let config_str = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read config file: {config_path}"))?;
    let mut config: Config =
        toml::from_str(&config_str).with_context(|| "Failed to parse config file")?;

    if let Some(port) = port_override {
        config.api.listen_addr = format!("0.0.0.0:{port}");
    }

    let stratum_addrs = config.stratum.addrs();
    info!(
        name = %config.pool.name,
        api = %config.api.listen_addr,
        node = %config.node.rpc_url,
        "Configuration loaded"
    );

    // Connect to the same SQLite database (WAL mode for concurrent access)
    let db_pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&config.database.url)
        .await
        .with_context(|| "Failed to connect to database")?;
    let db = PoolDb::new(db_pool);
    db.run_migrations()
        .await
        .with_context(|| "Failed to run migrations")?;
    db.set_wal_mode()
        .await
        .with_context(|| "Failed to enable WAL mode")?;
    info!("Database connected (WAL mode, read-only dashboard)");

    // Initialize Zcash node RPC client
    let rpc = match (&config.node.rpc_user, &config.node.rpc_password) {
        (Some(user), Some(pass)) => {
            Arc::new(ZcashRpcClient::with_auth(&config.node.rpc_url, user, pass))
        }
        _ => Arc::new(ZcashRpcClient::new(&config.node.rpc_url)),
    };

    // Wallet RPC for Zallet monitoring (balance/health checks + trigger_payout)
    let wallet_rpc = config.payout.wallet_rpc_url.as_ref().map(|url| {
        let rpc = match (&config.payout.wallet_rpc_user, &config.payout.wallet_rpc_password) {
            (Some(u), Some(p)) => ZcashRpcClient::with_auth(url, u, p),
            _ => ZcashRpcClient::new(url),
        };
        Arc::new(rpc)
    });

    // Compute difficulty_multiplier from pool target
    let pool_target = pool_core_parse_target(&config.difficulty.initial_target)?;
    let difficulty_multiplier = {
        let target_f64 = pool_target
            .iter()
            .enumerate()
            .fold(0.0f64, |acc, (i, &b)| {
                acc + (b as f64) * 256.0f64.powi(31 - i as i32)
            });
        if target_f64 > 0.0 {
            2.0f64.powi(256) / target_f64
        } else {
            1.0
        }
    };

    let stratum_ports = config.stratum.port_info();

    // Build ApiState with no live atomics (triggers DB fallback in helpers)
    let api_state: AppState = Arc::new(ApiState {
        db: db.clone(),
        rpc: Arc::clone(&rpc),
        pool_name: config.pool.name.clone(),
        pool_fee: config.pool.fee_percent,
        network: config.pool.network.clone(),
        hostname: config
            .pool
            .hostname
            .clone()
            .unwrap_or_else(|| "127.0.0.1".to_string()),
        stratum_port: stratum_addrs
            .first()
            .and_then(|a| a.split(':').last())
            .and_then(|p| p.parse().ok())
            .unwrap_or(3333),
        stratum_ports,
        last_template_at_ms: None, // DB fallback
        wallet_rpc,
        pool_address: config.payout.pool_address.clone(),
        mining_address: config.payout.mining_address.clone(),
        min_payout_zatoshis: (config.payout.minimum_payout * ZATOSHIS_PER_ZEC) as i64,
        maturity_confirmations: config.payout.maturity_confirmations,
        network_blocks_cache: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        stats_history: pool_api::StatsHistory::new(),
        shares_accepted: Arc::new(std::sync::atomic::AtomicU64::new(0)), // DB fallback
        shares_rejected: Arc::new(std::sync::atomic::AtomicU64::new(0)), // DB fallback
        banner: config.pool.banner.clone(),
        difficulty_multiplier,
    });

    // Background stats history recorder (10s snapshots, 1hr ring buffer)
    let history_state = Arc::clone(&api_state);
    let history_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let snapshot = pool_api::compute_stats_snapshot(&history_state).await;
            history_state.stats_history.push(snapshot).await;
        }
    });

    // Background network cache warmer
    let net_cache_state = Arc::clone(&api_state);
    let net_cache_handle = tokio::spawn(async move {
        pool_api::warm_network_cache(&net_cache_state, &["1h"]).await;
        pool_api::warm_network_cache(&net_cache_state, &["24h", "1w"]).await;
        loop {
            tokio::time::sleep(Duration::from_secs(55)).await;
            pool_api::warm_network_cache(&net_cache_state, &["1h", "24h", "1w"]).await;
        }
    });

    // Build public dashboard router
    let router = pool_api::build_router(Arc::clone(&api_state));

    // Spawn API server
    let api_addr = config.api.listen_addr.clone();
    let api_handle = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(&api_addr)
            .await
            .expect("Failed to bind dashboard listener");
        info!(address = %api_addr, "Dashboard server listening");
        axum::serve(listener, router)
            .await
            .expect("Dashboard server failed");
    });

    // Spawn admin server if configured
    let admin_handle = if let Some(ref admin_cfg) = config.admin {
        if admin_cfg.enabled {
            let config_view = pool_api::admin::PoolConfigView {
                pool_name: config.pool.name.clone(),
                pool_fee: config.pool.fee_percent,
                network: config.pool.network.clone(),
                stratum_ports: stratum_addrs.clone(),
                difficulty_multiplier,
                min_payout_zec: config.payout.minimum_payout,
                maturity_confirmations: config.payout.maturity_confirmations,
                pool_address: config.payout.pool_address.clone(),
                mining_address: config.payout.mining_address.clone(),
                node_rpc_url: config.node.rpc_url.clone(),
                wallet_rpc_url: config.payout.wallet_rpc_url.clone(),
                coinbase_tag: config.pool.coinbase_tag.clone(),
                payout_interval_secs: config.payout.interval_secs,
            };
            let log_paths = pool_api::LogPaths {
                pool: admin_cfg.pool_log.clone(),
                dashboard: admin_cfg.dashboard_log.clone(),
                zallet: admin_cfg.zallet_log.clone(),
            };
            let zallet_paths = {
                let mut zp = pool_api::ZalletPaths::default();
                if let Some(ref b) = admin_cfg.zallet_binary {
                    zp.binary = b.clone();
                }
                if let Some(ref d) = admin_cfg.zallet_datadir {
                    zp.datadir = d.clone();
                }
                if let Some(ref l) = admin_cfg.zallet_log {
                    zp.log = l.clone();
                }
                zp
            };
            let admin_state = pool_api::AdminState::with_log_paths(
                Arc::clone(&api_state),
                &admin_cfg.password,
                config_view,
                config_path.clone(),
                log_paths,
                zallet_paths,
            );
            let admin_router = pool_api::admin::build_admin_router(admin_state);
            let admin_addr = admin_cfg.listen_addr.clone();
            Some(tokio::spawn(async move {
                match tokio::net::TcpListener::bind(&admin_addr).await {
                    Ok(listener) => {
                        info!(address = %admin_addr, "Admin server listening");
                        axum::serve(listener, admin_router)
                            .await
                            .expect("Admin server failed");
                    }
                    Err(e) => {
                        tracing::warn!(address = %admin_addr, error = %e,
                            "Admin server skipped (port in use — pool may already serve admin)");
                    }
                }
            }))
        } else {
            info!("Admin server disabled");
            None
        }
    } else {
        None
    };

    // Spawn payout loop if configured
    let payout_handle = if config.payout.enabled {
        let pool_address = config.payout.pool_address.clone()
            .expect("payout.pool_address is required when payouts are enabled");
        let mining_address = config.payout.mining_address.clone()
            .unwrap_or_else(|| pool_address.clone());
        let wallet_url = config.payout.wallet_rpc_url.clone()
            .expect("payout.wallet_rpc_url is required when payouts are enabled");
        let payout_wallet_rpc = match (&config.payout.wallet_rpc_user, &config.payout.wallet_rpc_password) {
            (Some(user), Some(pass)) => Arc::new(ZcashRpcClient::with_auth(&wallet_url, user, pass)),
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
            "Payout loop enabled"
        );
        Some(tokio::spawn(async move {
            run_payout_loop(
                payout_db, node_rpc, payout_wallet_rpc,
                &pool_address, &mining_address,
                min_payout_zatoshis, maturity, interval,
                &payout_network,
            ).await;
        }))
    } else {
        info!("Payouts disabled");
        None
    };

    let admin_info = config
        .admin
        .as_ref()
        .filter(|a| a.enabled)
        .map(|a| format!("\n         Admin: http://{}", a.listen_addr))
        .unwrap_or_default();

    info!(
        "Dashboard is running!\n\
         \n\
         Dashboard: http://{}\n\
         API: http://{}/api/pool/stats{}\n",
        config.api.listen_addr, config.api.listen_addr, admin_info,
    );

    // Wait for shutdown signal
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl+c");
    info!("Shutdown signal received, stopping dashboard...");

    api_handle.abort();
    if let Some(h) = admin_handle {
        h.abort();
    }
    history_handle.abort();
    net_cache_handle.abort();
    if let Some(h) = payout_handle {
        h.abort();
    }

    info!("Dashboard shut down gracefully");
    Ok(())
}

/// Parse a hex target string into a 32-byte array.
/// Duplicated from pool-core to avoid depending on the full pool-core crate.
fn pool_core_parse_target(hex_str: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(hex_str)
        .with_context(|| format!("Invalid hex in initial_target: {hex_str}"))?;
    if bytes.len() > 32 {
        anyhow::bail!("Target too long: {} bytes", bytes.len());
    }
    let mut target = [0u8; 32];
    // Right-pad with zeros (target is big-endian, short hex means leading zeros)
    let offset = 32 - bytes.len();
    target[offset..].copy_from_slice(&bytes);
    Ok(target)
}

// -- Payout loop --

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
    // Short initial delay to let dashboard fully start before doing RPC work.
    tokio::time::sleep(Duration::from_secs(30)).await;

    let mut consecutive_payout_failures: u32 = 0;
    let mut last_payout_error = String::new();

    loop {
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
        match process_payouts(&db, &wallet_rpc, pool_address, mining_address, min_payout_zatoshis, network).await {
            Ok(count) => {
                if count > 0 {
                    info!(payouts = count, "Payout round completed");
                    consecutive_payout_failures = 0;
                    last_payout_error.clear();
                }
            }
            Err(e) => {
                error!(error = %e, "Payout round failed");
                consecutive_payout_failures += 1;
                last_payout_error = format!("{e}");
            }
        }

        // Write payout health status for the admin health page to read.
        let _ = write_payout_health(
            &db, &wallet_rpc, mining_address,
            consecutive_payout_failures, &last_payout_error,
        ).await;

        tokio::time::sleep(interval).await;
    }
}

/// Collect and persist payout pipeline health metrics.
async fn write_payout_health(
    db: &PoolDb,
    wallet_rpc: &ZcashRpcClient,
    mining_address: &str,
    consecutive_failures: u32,
    last_error: &str,
) -> anyhow::Result<()> {
    // Check transparent balance (unshielded funds)
    let (transparent_zec, private_zec) = match wallet_rpc.call_raw::<serde_json::Value>(
        "z_gettotalbalance", serde_json::json!([1, true])
    ).await {
        Ok(bal) => {
            let t = bal.get("transparent").and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
            let p = bal.get("private").and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
            (t, p)
        }
        Err(_) => (0.0, 0.0),
    };

    // Check if there are confirmed (mature) blocks whose coinbase hasn't been shielded.
    // If transparent balance > 0 and we have mature blocks, shielding might be stuck.
    let shielding_stuck = transparent_zec > 0.01;

    // Check for Zallet sync issues by attempting a simple RPC
    let wallet_responsive = wallet_rpc.call_raw::<serde_json::Value>(
        "z_gettotalbalance", serde_json::json!([0, true])
    ).await.is_ok();

    let health = serde_json::json!({
        "consecutive_payout_failures": consecutive_failures,
        "last_payout_error": last_error,
        "transparent_balance_zec": transparent_zec,
        "private_balance_zec": private_zec,
        "shielding_stuck": shielding_stuck,
        "wallet_responsive": wallet_responsive,
        "checked_at": chrono::Utc::now().to_rfc3339(),
    });
    db.set_pool_status("payout_health", &health.to_string()).await?;
    Ok(())
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

    // Fire multiple z_shieldcoinbase calls concurrently. Each returns an opid
    // immediately while the proof runs in the background inside Zallet.
    // This lets us have multiple proofs running in parallel.
    const CONCURRENT_BATCHES: u32 = 5;

    let mut opids: Vec<(u32, String, u64, f64)> = Vec::new();

    for batch in 1..=MAX_SHIELD_BATCHES_PER_CYCLE {
        let result = match wallet_rpc.z_shield_coinbase(mining_address, pool_address, Some(50)).await {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("{e}");
                if msg.contains("No spendable transparent outputs")
                    || msg.contains("Insufficient")
                    || msg.contains("No funds")
                {
                    break;
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

        opids.push((batch, opid, shielding_utxos, shielding_value));

        // Once we have CONCURRENT_BATCHES queued, wait for them all before
        // queuing more. This prevents overwhelming Zallet with too many proofs.
        if opids.len() as u32 >= CONCURRENT_BATCHES || remaining_utxos == 0 {
            let mut total_ok = 0u64;
            let mut total_val = 0.0f64;
            for (b, op, utxos, val) in opids.drain(..) {
                match wait_for_operation(wallet_rpc, &op).await? {
                    OpResult::Success(txid) => {
                        total_ok += utxos;
                        total_val += val;
                        info!(batch = b, txid = %txid, "Shielding batch complete");
                    }
                    OpResult::Failed(msg) => {
                        warn!(batch = b, error = %msg, "Shielding batch failed");
                    }
                }
            }
            if total_ok > 0 {
                info!(utxos = total_ok, value_zec = total_val, "Concurrent batch group done");
            }
        }

        if remaining_utxos == 0 {
            break;
        }
    }

    // Wait for any remaining queued operations.
    for (b, op, _utxos, _val) in opids.drain(..) {
        match wait_for_operation(wallet_rpc, &op).await? {
            OpResult::Success(txid) => {
                info!(batch = b, txid = %txid, "Shielding batch complete");
            }
            OpResult::Failed(msg) => {
                warn!(batch = b, error = %msg, "Shielding batch failed");
            }
        }
    }

    Ok(())
}

enum OpResult {
    Success(String),
    Failed(String),
}

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
                _ => {}
            }
        }
    }
}

async fn process_payouts(
    db: &PoolDb,
    rpc: &ZcashRpcClient,
    pool_address: &str,
    mining_address: &str,
    min_payout_zatoshis: i64,
    network: &str,
) -> anyhow::Result<usize> {
    let pending = db.get_pending_payouts(min_payout_zatoshis).await?;
    if pending.is_empty() {
        return Ok(0);
    }

    let total_payout_zatoshis: i64 = pending.iter().map(|p| p.amount).sum();
    let total_payout_zec = total_payout_zatoshis as f64 / ZATOSHIS_PER_ZEC;

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

    let available_zec = (private_balance * 0.90).max(0.0);

    if available_zec < min_payout_zatoshis as f64 / ZATOSHIS_PER_ZEC {
        info!(private_balance, available_zec, "Shielded balance too low for any payouts");
        return Ok(0);
    }

    let scale = if available_zec >= total_payout_zec { 1.0 } else { available_zec / total_payout_zec };

    let mut payout_list: Vec<(usize, i64, String)> = Vec::new();
    let two_days_ago = Utc::now()
        .checked_sub_signed(chrono::Duration::days(2))
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();

    for (i, p) in pending.iter().enumerate() {
        let pay_to = if is_valid_zcash_address(&p.address, network) {
            p.address.clone()
        } else if p.created_at <= two_days_ago {
            warn!(
                miner_id = p.miner_id, address = %p.address, created_at = %p.created_at,
                amount_zec = p.amount as f64 / ZATOSHIS_PER_ZEC,
                "Redirecting payout to mining address (unpayable address, >2 days old)"
            );
            mining_address.to_string()
        } else {
            warn!(miner_id = p.miner_id, address = %p.address,
                "Skipping payout: invalid address format (account < 2 days old)");
            continue;
        };
        let scaled_zatoshis = (p.amount as f64 * scale).floor() as i64;
        if scaled_zatoshis >= min_payout_zatoshis {
            payout_list.push((i, scaled_zatoshis, pay_to));
        }
    }

    if payout_list.is_empty() {
        info!(private_balance, scale, "All scaled payouts below minimum, waiting for more shielding");
        return Ok(0);
    }

    let actual_total_zatoshis: i64 = payout_list.iter().map(|(_, amt, _)| *amt).sum();
    let actual_total_zec = actual_total_zatoshis as f64 / ZATOSHIS_PER_ZEC;
    info!(miners = payout_list.len(), total_zec = actual_total_zec, private_balance, scale, "Processing payouts");

    let (opid, payout_list) = {
        let mut current_list = payout_list;
        let mut retries = 0u32;
        loop {
            let mut merged: std::collections::BTreeMap<&str, f64> = std::collections::BTreeMap::new();
            for (_, zats, addr) in &current_list {
                *merged.entry(addr.as_str()).or_insert(0.0) += *zats as f64 / ZATOSHIS_PER_ZEC;
            }
            let amounts: Vec<(&str, f64)> = merged.into_iter().collect();

            match rpc.z_sendmany(pool_address, &amounts).await {
                Ok(opid) => break (opid, current_list),
                Err(e) => {
                    let msg = format!("{e}");
                    if retries < 2 && msg.contains("Insufficient balance") {
                        if let Some(have_zats) = parse_have_balance(&msg) {
                            let actual_available = have_zats as f64 * 0.95;
                            let current_total: f64 = current_list.iter().map(|(_, z, _)| *z as f64).sum();
                            if actual_available < min_payout_zatoshis as f64 || current_total <= 0.0 {
                                return Err(anyhow::anyhow!("Wallet balance too low: {msg}"));
                            }
                            let rescale = actual_available / current_total;
                            info!(have_zats, rescale, retry = retries + 1, "Rescaling payouts based on actual wallet balance");
                            current_list = current_list
                                .into_iter()
                                .filter_map(|(i, old_zats, addr)| {
                                    let new_zats = (old_zats as f64 * rescale).floor() as i64;
                                    if new_zats >= min_payout_zatoshis { Some((i, new_zats, addr)) } else { None }
                                })
                                .collect();
                            if current_list.is_empty() {
                                return Err(anyhow::anyhow!("All payouts below minimum after rescaling"));
                            }
                            retries += 1;
                            continue;
                        }
                    }
                    // If the error is about a bad address or amount, try removing
                    // the problematic entries and retry once more.
                    if retries < 2 && (msg.contains("unknown address") || msg.contains("Invalid amount") || msg.contains("Invalid parameter")) {
                        warn!(error = %msg, "z_sendmany failed, removing problematic entries and retrying");
                        // Can't easily identify which address is bad, so halve the batch
                        // and retry — eventually the bad one gets isolated.
                        let half = current_list.len() / 2;
                        if half == 0 {
                            return Err(anyhow::anyhow!("z_sendmany failed on single entry: {e}"));
                        }
                        current_list.truncate(half);
                        retries += 1;
                        continue;
                    }
                    return Err(anyhow::anyhow!("z_sendmany failed: {e}"));
                }
            }
        }
    };

    info!(opid = %opid, "z_sendmany submitted, waiting for completion");

    let txid = match wait_for_operation(rpc, &opid).await? {
        OpResult::Success(txid) => {
            info!(opid = %opid, txid = %txid, "Payout transaction broadcast");
            txid
        }
        OpResult::Failed(msg) => {
            return Err(anyhow::anyhow!("z_sendmany failed: {msg}"));
        }
    };

    let mut count = 0;
    for (i, amt_zatoshis, pay_to) in &payout_list {
        let p = &pending[*i];
        if let Err(e) = db.create_payout(p.miner_id, *amt_zatoshis, &txid).await {
            error!(miner_id = p.miner_id, error = %e, "Failed to record payout");
        } else {
            info!(
                miner_id = p.miner_id, address = %p.address, pay_to = %pay_to,
                amount_zec = *amt_zatoshis as f64 / ZATOSHIS_PER_ZEC, txid = %txid,
                "Payout recorded"
            );
            count += 1;
        }
    }

    Ok(count)
}

fn parse_have_balance(msg: &str) -> Option<i64> {
    let marker = "have ";
    let start = msg.find(marker)? + marker.len();
    let rest = &msg[start..];
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    rest[..end].parse::<i64>().ok()
}

/// Validates a Zcash address for the given network using proper encoding checks.
///
/// Transparent addresses (t1/t3/tm/t2) use base58check encoding with specific
/// version bytes. Sapling (zs/ztestsapling) and unified (u1/utest) addresses
/// use bech32/bech32m encoding with specific HRP and length constraints.
fn is_valid_zcash_address(addr: &str, network: &str) -> bool {
    let is_mainnet = network == "mainnet";

    // Transparent addresses: base58check with 2-byte version prefix.
    // Mainnet: t1 (P2PKH, version 0x1CB8), t3 (P2SH, version 0x1CBD)
    // Testnet: tm (P2PKH, version 0x1D25), t2 (P2SH, version 0x1CBA)
    if addr.starts_with('t') {
        // Quick prefix check for the right network
        let valid_prefix = if is_mainnet {
            addr.starts_with("t1") || addr.starts_with("t3")
        } else {
            addr.starts_with("tm") || addr.starts_with("t2")
        };
        if !valid_prefix {
            return false;
        }
        // Base58check decode: should produce exactly 22 bytes (2 version + 20 hash)
        return match bs58::decode(addr).with_check(None).into_vec() {
            Ok(bytes) => bytes.len() == 22,
            Err(_) => false,
        };
    }

    // Sapling addresses: bech32 encoding
    // Mainnet: "zs1" HRP, 78 chars total (43 byte payload)
    // Testnet: "ztestsapling1" HRP, 88 chars total
    if addr.starts_with('z') {
        let valid_prefix = if is_mainnet {
            addr.starts_with("zs1")
        } else {
            addr.starts_with("ztestsapling1")
        };
        if !valid_prefix {
            return false;
        }
        // bech32 charset: lowercase alphanumeric excluding 1, b, i, o
        let hrp_end = addr.rfind('1').unwrap_or(0);
        let data_part = &addr[hrp_end + 1..];
        let valid_charset = data_part
            .chars()
            .all(|c| "qpzry9x8gf2tvdw0s3jn54khce6mua7l".contains(c));
        let expected_len = if is_mainnet { 78 } else { 88 };
        return valid_charset && addr.len() == expected_len;
    }

    // Unified addresses: bech32m encoding
    // Mainnet: "u1" HRP
    // Testnet: "utest1" HRP
    if addr.starts_with('u') {
        let valid_prefix = if is_mainnet {
            addr.starts_with("u1") && !addr.starts_with("utest")
        } else {
            addr.starts_with("utest1")
        };
        if !valid_prefix {
            return false;
        }
        // Unified addresses vary in length depending on which receivers are
        // included (transparent, sapling, orchard). Minimum is ~62 chars for
        // a single-receiver UA, maximum ~320 for all three receivers.
        let hrp_end = addr.find('1').unwrap_or(0);
        let data_part = &addr[hrp_end + 1..];
        let valid_charset = data_part
            .chars()
            .all(|c| "qpzry9x8gf2tvdw0s3jn54khce6mua7l".contains(c));
        return valid_charset && data_part.len() >= 50 && addr.len() <= 320;
    }

    false
}
