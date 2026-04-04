use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;
use sqlx::sqlite::SqlitePoolOptions;
use tracing::info;
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
            let admin_state = pool_api::AdminState::new(
                Arc::clone(&api_state),
                &admin_cfg.password,
                config_view,
                config_path.clone(),
            );
            let admin_router = pool_api::admin::build_admin_router(admin_state);
            let admin_addr = admin_cfg.listen_addr.clone();
            Some(tokio::spawn(async move {
                let listener = tokio::net::TcpListener::bind(&admin_addr)
                    .await
                    .expect("Failed to bind admin listener");
                info!(address = %admin_addr, "Admin server listening");
                axum::serve(listener, admin_router)
                    .await
                    .expect("Admin server failed");
            }))
        } else {
            info!("Admin server disabled");
            None
        }
    } else {
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
