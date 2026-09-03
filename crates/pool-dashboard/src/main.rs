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

mod reconciler;

const ZATOSHIS_PER_ZEC: f64 = 100_000_000.0;

// -- Config structs (subset of pool-server, reads the same pool.toml) --

#[derive(Debug, Deserialize)]
struct Config {
    pool: PoolConfig,
    stratum: StratumConfig,
    node: NodeConfig,
    difficulty: DifficultyConfig,
    #[serde(default)]
    pplns: PplnsModeOnlyConfig,
    #[serde(default)]
    payout: PayoutConfig,
    api: ApiConfig,
    database: DatabaseConfig,
    #[serde(default)]
    admin: Option<AdminConfig>,
    #[serde(default)]
    lwd_tip: LwdTipConfig,
}

/// Dashboard only needs the reward mode from [pplns] (for the Payout Scheme
/// label). All other [pplns] fields are pool-server concerns.
#[derive(Debug, Deserialize, Default)]
struct PplnsModeOnlyConfig {
    #[serde(default = "default_pplns_mode")]
    mode: String,
}

fn default_pplns_mode() -> String {
    "pplns".to_string()
}

#[derive(Debug, Deserialize)]
struct AdminConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_admin_addr")]
    listen_addr: String,
    password: String,
    /// Separate password for the internal ops dashboard (/ops). Independent of
    /// `password` above — neither session grants the other. Unset = /ops disabled.
    #[serde(default)]
    ops_password: Option<String>,
    /// Ops dashboard upstream. Required when `ops_password` is set; keeping it
    /// in deployment configuration avoids publishing infrastructure addresses.
    #[serde(default)]
    ops_upstream: Option<String>,
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
    /// Optional zebrad Prometheus metrics endpoint. If unset, derived
    /// from `rpc_url` by swapping port 8232 → 9999 and appending `/metrics`.
    #[serde(default)]
    metrics_url: Option<String>,
}

/// Public lightwalletd servers used to cross-check the chain tip. When the
/// list is empty (or the section is omitted) the authoritative-tip row on
/// the admin Mining Node card stays blank and no fan-out is performed.
#[derive(Debug, Default, Deserialize)]
struct LwdTipConfig {
    #[serde(default)]
    servers: Vec<String>,
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
    /// Audit #19: auto-void payouts whose tx the node reports absent (-5)
    /// past tx-expiry (reorged-out class). Default ON — the whole point is
    /// no human dependency; set false to fall back to alert-only.
    #[serde(default = "default_auto_void")]
    auto_void_reorged: bool,
    /// Minimum shielded balance (in ZEC/TAZ) to keep as a reserve so that
    /// miners can be paid from mature funds without waiting for newly-mined
    /// coinbase to reach 100-confirmation maturity and then be shielded.
    /// Payouts that would drop the balance below this threshold are deferred
    /// until shielding replenishes the reserve. Set to 0 to disable (default).
    #[serde(default)]
    reserve_min: f64,
    /// Fraction of the shielded balance considered spendable when sizing a
    /// payout round (audit Finding #8 — was a hardcoded 0.90). The margin
    /// absorbs ZIP-317 fees and notes whose Merkle witnesses aren't built
    /// yet ("have 0" cases) without aborting the round.
    #[serde(default = "default_balance_margin")]
    available_balance_margin: f64,
    /// Interval for the independent payout-reconciliation sweep (audit P4):
    /// resolves stale payout_attempts, verifies recorded payouts exist on
    /// chain, and checks the rewards-vs-balances invariant. 0 disables.
    #[serde(default = "default_reconcile_interval")]
    reconcile_interval_secs: u64,
    /// Faucet-style immediate payouts: skip the maturity gate and pay block
    /// credits at find time, with the reserve underwriting orphan losses
    /// (those clawbacks are auto-acknowledged as absorbed). Safeguarded:
    /// only takes effect in solo mode with reserve_min > 0, and each round
    /// only while total immature exposure stays within reserve_min.
    #[serde(default)]
    pay_immature: bool,
    /// Payout coalescing (#21): skip a miner whose last payout is younger
    /// than this, so steady earners get one batched payment instead of a
    /// same-address tx burst every cycle. 0 disables. HOT-RELOADED: the
    /// payout loop re-reads this from the config file every cycle.
    #[serde(default)]
    min_payout_interval_secs: u64,
    /// Coalescing override (#21): a pending balance at or above this many
    /// ZEC/TAZ is paid immediately regardless of the cooldown, so large
    /// balances never wait on a timer. Unset = no override. HOT-RELOADED.
    #[serde(default)]
    coalesce_override_zec: Option<f64>,
    /// The node mints the block reward straight into a shielded receiver
    /// (zakurad `mining.miner_address` = the wallet's UA), so there is no
    /// transparent coinbase output to look for. The reconciler's coinbase
    /// check then asks the wallet whether it received each coinbase txid
    /// instead of scanning vouts for `mining_address`. The sweep loop keeps
    /// running: `z_shieldcoinbase` still drains any legacy transparent
    /// coinbase and is a benign no-op once that is gone.
    #[serde(default)]
    shielded_coinbase: bool,
}

fn default_minimum_payout() -> f64 {
    0.01
}
fn default_payout_interval() -> u64 {
    300
}
fn default_auto_void() -> bool {
    true
}

fn default_maturity() -> u64 {
    100
}
fn default_reconcile_interval() -> u64 {
    600
}
fn default_balance_margin() -> f64 {
    0.90
}

impl Default for PayoutConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_void_reorged: default_auto_void(),
            pool_address: None,
            mining_address: None,
            wallet_rpc_url: None,
            wallet_rpc_user: None,
            wallet_rpc_password: None,
            minimum_payout: default_minimum_payout(),
            interval_secs: default_payout_interval(),
            maturity_confirmations: default_maturity(),
            reserve_min: 0.0,
            available_balance_margin: default_balance_margin(),
            reconcile_interval_secs: default_reconcile_interval(),
            pay_immature: false,
            min_payout_interval_secs: 0,
            coalesce_override_zec: None,
            shielded_coinbase: false,
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

    // Connect to the same SQLite database (WAL mode for concurrent access).
    // busy_timeout lets a writer WAIT for the lock instead of instantly failing
    // with SQLITE_BUSY (the failure that silently dropped block 90's payout
    // distribution); synchronous=NORMAL is the standard WAL setting. Per-conn.
    let db_opts: sqlx::sqlite::SqliteConnectOptions = config
        .database
        .url
        .parse()
        .with_context(|| "Invalid database url")?;
    let db_opts = db_opts
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(20));
    let db_pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(db_opts)
        .await
        .with_context(|| "Failed to connect to database")?;
    let db = PoolDb::new(db_pool);
    db.run_migrations()
        .await
        .with_context(|| "Failed to run migrations")?;
    // Audit #16: fail loud on schema holes rather than run the money path
    // on top of them.
    db.assert_critical_schema()
        .await
        .with_context(|| "Critical schema verification failed — refusing to start")?;
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

    // Resolve the zebra metrics URL once, share with the background scraper.
    let zebra_metrics_url = config
        .node
        .metrics_url
        .clone()
        .unwrap_or_else(|| pool_api::derive_metrics_url(&config.node.rpc_url));

    // Background-refreshed cache of zebra's /metrics output. See the doc on
    // ApiState::zebra_metrics_cache for why this exists.
    let zebra_metrics_cache: Arc<tokio::sync::RwLock<pool_api::ZebraMetrics>> =
        Arc::new(tokio::sync::RwLock::new(pool_api::ZebraMetrics::default()));

    // Background-refreshed cache of the authoritative chain tip from public
    // lwd servers. See ApiState::authoritative_tip_cache.
    let authoritative_tip_cache: Arc<tokio::sync::RwLock<pool_api::AuthoritativeTip>> =
        Arc::new(tokio::sync::RwLock::new(pool_api::AuthoritativeTip::default()));

    // Immediate-payout (faucet) mode, with its safeguard: the knob only
    // takes effect in solo mode with a reserve to underwrite orphan losses.
    let pay_immature = config.payout.pay_immature
        && config.pplns.mode.to_lowercase() == "solo"
        && config.payout.reserve_min > 0.0;
    if config.payout.pay_immature && !pay_immature {
        warn!(
            mode = %config.pplns.mode,
            reserve_min = config.payout.reserve_min,
            "pay_immature is set but disabled: it requires solo mode and reserve_min > 0"
        );
    }

    // Wake handle for the payout loop (audit #14): the admin trigger nudges
    // the one hardened pipeline instead of running its own copy.
    let payout_wake = Arc::new(tokio::sync::Notify::new());

    // Audit #16: shares retention. Hourly, archive+delete raw share rows
    // older than 30 days into shares_rollup (atomic per hour, ≤200 hours per
    // pass so the historical backlog drains over a few days without long
    // write locks).
    {
        let retention_db = db.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(120)).await;
            loop {
                match retention_db.rollup_and_prune_shares(30, 200).await {
                    Ok((hours, rows)) if hours > 0 => {
                        info!(hours, rows, "Shares retention: archived + pruned");
                    }
                    Ok(_) => {}
                    Err(e) => warn!(error = %e, "Shares retention pass failed"),
                }
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
        });
    }

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
        pay_immature,
        maturity_confirmations: config.payout.maturity_confirmations,
        payout_wake: if config.payout.enabled {
            Some(Arc::clone(&payout_wake))
        } else {
            None
        },
        network_blocks_cache: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        stats_history: pool_api::StatsHistory::new(),
        shares_accepted: Arc::new(std::sync::atomic::AtomicU64::new(0)), // DB fallback
        shares_rejected: Arc::new(std::sync::atomic::AtomicU64::new(0)), // DB fallback
        banner: config.pool.banner.clone(),
        difficulty_multiplier,
        payout_scheme: match config.pplns.mode.to_lowercase().as_str() {
            "solo" => "Solo".to_string(),
            _ => "PPLNS".to_string(),
        },
        zebra_metrics_cache: Arc::clone(&zebra_metrics_cache),
        authoritative_tip_cache: Arc::clone(&authoritative_tip_cache),
    });

    // Background zebra-metrics scraper. Refreshes the cache every 30 s; the
    // admin health endpoint reads from the cache, never blocking on the
    // scrape. The first iteration runs immediately so the cache has real
    // data within seconds of startup.
    let scrape_cache = Arc::clone(&zebra_metrics_cache);
    let scrape_url = zebra_metrics_url.clone();
    let zebra_scrape_handle = tokio::spawn(async move {
        loop {
            let snap = pool_api::fetch_zebra_metrics(&scrape_url).await;
            {
                let mut w = scrape_cache.write().await;
                *w = snap;
            }
            // Audit #20: zakura's /metrics is ~117 MB (cardinality leak) and
            // must be read in full — every 30s that's 2,880 pulls/day of node
            // CPU + LAN traffic. 5 min staleness on the health panel is fine.
            tokio::time::sleep(Duration::from_secs(300)).await;
        }
    });

    // Background authoritative-tip fan-out. Same pattern — refresh every
    // 30 s, dashboard reads instantly. Skipped entirely when the operator
    // didn't configure any servers in [lwd_tip].
    let lwd_servers = config.lwd_tip.servers.clone();
    let lwd_cache = Arc::clone(&authoritative_tip_cache);
    let lwd_tip_handle = if !lwd_servers.is_empty() {
        info!(server_count = lwd_servers.len(), "Starting authoritative-tip fan-out");
        Some(tokio::spawn(async move {
            loop {
                let snap = pool_api::fetch_authoritative_tip(&lwd_servers).await;
                {
                    let mut w = lwd_cache.write().await;
                    *w = snap;
                }
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        }))
    } else {
        info!("No [lwd_tip].servers configured — authoritative-tip card row disabled");
        None
    };

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
                zebra_metrics_url: zebra_metrics_url.clone(),
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
            let mut admin_state = pool_api::AdminState::with_log_paths(
                Arc::clone(&api_state),
                &admin_cfg.password,
                config_view,
                config_path.clone(),
                log_paths,
                zallet_paths,
            );
            // Ops dashboard gets its own password; without one the /ops routes
            // fail closed rather than serving the view unguarded.
            match (
                admin_cfg.ops_password.as_deref().map(str::trim),
                admin_cfg.ops_upstream.as_deref().map(str::trim),
            ) {
                (Some(pw), Some(upstream)) if !pw.is_empty() && !upstream.is_empty() => {
                    if pw == admin_cfg.password {
                        warn!("admin.ops_password matches admin.password — set a distinct one");
                    }
                    admin_state = admin_state.with_ops(pw, upstream);
                    info!("Ops dashboard enabled at /ops (separate password)");
                }
                (Some(pw), _) if !pw.is_empty() => {
                    warn!("Ops dashboard disabled (admin.ops_upstream is required)");
                }
                _ => info!("Ops dashboard disabled (no admin.ops_password set)"),
            }
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
        let reserve_min_zatoshis = (config.payout.reserve_min * ZATOSHIS_PER_ZEC) as i64;
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
            reserve_min_zec = config.payout.reserve_min,
            interval_secs = config.payout.interval_secs,
            maturity_confirmations = maturity,
            pay_immature,
            "Payout loop enabled"
        );
        // Independent reconciliation sweep (audit P4). Shares the same RPC
        // endpoints but runs on its own cadence so a wedged payout loop
        // can't silence it.
        let reconcile_secs = config.payout.reconcile_interval_secs;
        let reconciler_handle = if reconcile_secs > 0 {
            let r = reconciler::Reconciler {
                db: db.clone(),
                node_rpc: Arc::clone(&rpc),
                wallet_rpc: Arc::clone(&payout_wallet_rpc),
                interval: Duration::from_secs(reconcile_secs),
                pool_fee: (config.pool.fee_percent / 100.0).clamp(0.0, 1.0),
                mining_address: mining_address.clone(),
                auto_void_reorged: config.payout.auto_void_reorged,
                shielded_coinbase: config.payout.shielded_coinbase,
            };
            // Round-3: resolve any payout reservations orphaned by a crash BEFORE
            // the payout loop starts — confirm those whose tx reached the chain,
            // refund those that did not. Fast recovery of in-flight `paying` funds.
            // (No reservations -> no RPC calls, so this is a no-op on a clean boot.)
            let startup = r.reconcile_reserved_payouts_once(None).await;
            if !startup.alerts.is_empty() {
                warn!(alerts = ?startup.alerts, "Startup payout-reservation reconciliation");
            }
            Some(tokio::spawn(async move { r.run().await }))
        } else {
            info!("Reconciler disabled (reconcile_interval_secs = 0)");
            None
        };

        let balance_margin = config.payout.available_balance_margin.clamp(0.5, 1.0);
        let loop_wake = Arc::clone(&payout_wake);
        let coalesce_fallback = (
            config.payout.min_payout_interval_secs as i64,
            config
                .payout
                .coalesce_override_zec
                .map(|z| (z * ZATOSHIS_PER_ZEC) as i64)
                .unwrap_or(i64::MAX),
        );
        let payout_config_path = config_path.clone();
        let payout_task = tokio::spawn(async move {
            run_payout_loop(
                payout_db, node_rpc, payout_wallet_rpc,
                &pool_address, &mining_address,
                min_payout_zatoshis, reserve_min_zatoshis, balance_margin,
                maturity, interval,
                &payout_network, pay_immature,
                loop_wake,
                payout_config_path, coalesce_fallback,
            ).await;
        });
        Some((payout_task, reconciler_handle))
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
    if let Some((payout_task, reconciler_handle)) = payout_handle {
        payout_task.abort();
        if let Some(r) = reconciler_handle {
            r.abort();
        }
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

/// Re-read the coalescing knobs (#21) from the config file so they apply
/// without a restart. Any read/parse problem falls back to the startup
/// values — a bad config edit can never widen payouts mid-flight.
fn reload_coalesce_knobs(config_path: &str, fallback: (i64, i64)) -> (i64, i64) {
    let parsed = std::fs::read_to_string(config_path)
        .ok()
        .and_then(|s| toml::from_str::<Config>(&s).ok());
    match parsed {
        Some(c) => (
            c.payout.min_payout_interval_secs as i64,
            c.payout
                .coalesce_override_zec
                .map(|z| (z * ZATOSHIS_PER_ZEC) as i64)
                .unwrap_or(i64::MAX),
        ),
        None => {
            warn!(config_path, "coalesce knob reload failed; keeping previous values");
            fallback
        }
    }
}

async fn run_payout_loop(
    db: PoolDb,
    node_rpc: Arc<ZcashRpcClient>,
    wallet_rpc: Arc<ZcashRpcClient>,
    pool_address: &str,
    mining_address: &str,
    min_payout_zatoshis: i64,
    reserve_min_zatoshis: i64,
    balance_margin: f64,
    maturity_confirmations: u64,
    interval: Duration,
    network: &str,
    pay_immature: bool,
    wake: Arc<tokio::sync::Notify>,
    config_path: String,
    coalesce_fallback: (i64, i64),
) {
    info!("Payout loop started");
    // Short initial delay to let dashboard fully start before doing RPC work.
    tokio::time::sleep(Duration::from_secs(30)).await;

    let mut consecutive_payout_failures: u32 = 0;
    let mut last_payout_error = String::new();
    // Per-phase failure counters (audit P8): maturity-check and shielding
    // failures previously only hit the log, so payout_health undercounted
    // pipeline problems — a wedged shielding phase looked healthy as long
    // as process_payouts had nothing to do.
    let mut consecutive_maturity_failures: u32 = 0;
    let mut consecutive_shielding_failures: u32 = 0;
    let mut last_maturity_error = String::new();
    let mut last_shielding_error = String::new();

    loop {
        // Phase 1: Check block maturity
        match check_block_maturity(&db, &node_rpc, maturity_confirmations, pay_immature).await {
            Ok(()) => {
                consecutive_maturity_failures = 0;
                last_maturity_error.clear();
            }
            Err(e) => {
                error!(error = %e, "Block maturity check failed");
                consecutive_maturity_failures += 1;
                last_maturity_error = format!("{e}");
            }
        }

        // Phase 2: Shield mature coinbase UTXOs (transparent -> shielded)
        match shield_coinbase(&db, &wallet_rpc, mining_address, pool_address).await {
            Ok(()) => {
                consecutive_shielding_failures = 0;
                last_shielding_error.clear();
            }
            Err(e) => {
                error!(error = %e, "Coinbase shielding failed (will retry next cycle)");
                consecutive_shielding_failures += 1;
                last_shielding_error = format!("{e}");
            }
        }

        // Phase 3: Pay miners from shielded pool (respects reserve_min)
        let (coalesce_cooldown, coalesce_override) =
            reload_coalesce_knobs(&config_path, coalesce_fallback);
        match process_payouts(
            &db, &wallet_rpc, &node_rpc, pool_address, mining_address,
            min_payout_zatoshis, reserve_min_zatoshis, balance_margin, network,
            pay_immature, coalesce_cooldown, coalesce_override,
        ).await {
            Ok(count) => {
                consecutive_payout_failures = 0;
                last_payout_error.clear();
                if count > 0 {
                    info!(payouts = count, "Payout round completed");
                }
            }
            Err(e) => {
                error!(error = %e, "Payout round failed");
                consecutive_payout_failures += 1;
                last_payout_error = format!("{e}");
            }
        }

        let _ = write_payout_health(
            &db, &wallet_rpc, mining_address, reserve_min_zatoshis,
            PhaseFailures {
                payout: (consecutive_payout_failures, &last_payout_error),
                maturity: (consecutive_maturity_failures, &last_maturity_error),
                shielding: (consecutive_shielding_failures, &last_shielding_error),
            },
        ).await;

        // Sleep until the next scheduled cycle OR an admin nudge (audit #14:
        // the manual trigger wakes this loop instead of running its own
        // parallel pipeline).
        tokio::select! {
            _ = tokio::time::sleep(interval) => {}
            _ = wake.notified() => {
                info!("Payout loop woken early by manual trigger");
            }
        }
    }
}

/// Per-phase failure state for payout_health (audit P8).
struct PhaseFailures<'a> {
    payout: (u32, &'a str),
    maturity: (u32, &'a str),
    shielding: (u32, &'a str),
}

/// Collect and persist payout pipeline health metrics.
async fn write_payout_health(
    db: &PoolDb,
    wallet_rpc: &ZcashRpcClient,
    mining_address: &str,
    reserve_min_zatoshis: i64,
    failures: PhaseFailures<'_>,
) -> anyhow::Result<()> {
    // Check transparent balance (unshielded funds). Dialect-neutral: works
    // against zallet (z_gettotalbalance) and zecd (getbalances).
    let (transparent_zec, private_zec) = match wallet_rpc.wallet_balances(1).await {
        Ok(b) => (b.transparent, b.spendable),
        Err(_) => (0.0, 0.0),
    };

    // Check if there are confirmed (mature) blocks whose coinbase hasn't been shielded.
    // If transparent balance > 0 and we have mature blocks, shielding might be stuck.
    let shielding_stuck = transparent_zec > 0.01;

    // Check for wallet sync issues by attempting a simple RPC
    let wallet_responsive = wallet_rpc.wallet_balances(0).await.is_ok();

    let reserve_min_zec = reserve_min_zatoshis as f64 / ZATOSHIS_PER_ZEC;
    let spendable_zec = (private_zec - reserve_min_zec).max(0.0);

    let health = serde_json::json!({
        // Kept under the original key for dashboard/API compatibility.
        "consecutive_payout_failures": failures.payout.0,
        "last_payout_error": failures.payout.1,
        // Per-phase counters (audit P8).
        "consecutive_maturity_failures": failures.maturity.0,
        "last_maturity_error": failures.maturity.1,
        "consecutive_shielding_failures": failures.shielding.0,
        "last_shielding_error": failures.shielding.1,
        "transparent_balance_zec": transparent_zec,
        "private_balance_zec": private_zec,
        "reserve_min_zec": reserve_min_zec,
        "spendable_balance_zec": spendable_zec,
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
    pay_immature: bool,
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
            // Audit #14: reversal and status flip commit together in ONE
            // transaction — a crash can no longer leave an orphaned-status
            // block whose credits were never reversed (phantom credits).
            // Precise reversal debits exactly the credited miners; already-
            // paid credits land in orphan_clawbacks; pre-008 blocks fall
            // back to the legacy proportional reversal inside the same tx.
            match db.orphan_block(block.id, block.reward).await? {
                Some((reversed, clawback)) => {
                    info!(
                        height = block.height,
                        reversed_zatoshis = reversed,
                        clawback_zatoshis = clawback,
                        "Block orphaned (precise credit reversal)"
                    );
                    // Immediate-payout policy: already-paid credits of an
                    // orphan are the reserve's loss by design — record the
                    // decision so the reconciler doesn't page the operator.
                    if pay_immature && clawback > 0 {
                        let n = db
                            .acknowledge_clawbacks_for_block(
                                block.id,
                                "absorbed from reserve (immediate-payout policy)",
                            )
                            .await?;
                        info!(
                            height = block.height,
                            clawback_zatoshis = clawback,
                            rows = n,
                            "Orphan loss absorbed from reserve (auto-acknowledged)"
                        );
                    }
                }
                None => {
                    // orphan_block already ran the legacy proportional
                    // reversal inside the same transaction.
                    info!(
                        height = block.height,
                        "Block orphaned (legacy proportional reversal — pre-008 block)"
                    );
                }
            }
        }
    }

    Ok(())
}

/// Deterministic ZIP-317 fee estimate for a coinbase-shielding tx:
/// N transparent P2PKH inputs (~150 bytes each = 1 logical action apiece)
/// plus 2 padded Orchard actions. Zallet computes the real fee internally
/// and does not expose it, so we record this estimate as the pool's cost.
fn estimate_shield_fee(utxos: u64) -> i64 {
    5000 * (utxos as i64 + 2).max(2)
}

/// Deterministic ZIP-317 fee estimate for a payout tx: M transparent
/// outputs (1 logical action apiece) plus ~2 padded Orchard actions for
/// the spends/change.
fn estimate_payout_fee(recipients: usize) -> i64 {
    5000 * (recipients as i64 + 2).max(2)
}

async fn shield_coinbase(
    db: &PoolDb,
    wallet_rpc: &ZcashRpcClient,
    mining_address: &str,
    pool_address: &str,
) -> anyhow::Result<()> {
    if mining_address == pool_address {
        return Ok(());
    }

    // Queue one z_shieldcoinbase operation at a time. Each call returns an opid
    // immediately while the proof runs async in Zallet, so launching N in
    // parallel races them against the same transparent UTXO set: only the first
    // to broadcast wins, the rest are rejected from the mempool and their
    // half-built txs lock the inputs in wallet.db until the zallet sweeper purges
    // them on expiry. Serialized shielding also avoids saturating Zallet's RPC.
    const MAX_IN_FLIGHT_SHIELDING_BATCHES: u32 = 1;

    let mut opids: Vec<(u32, String, u64, f64)> = Vec::new();

    for batch in 1..=MAX_SHIELD_BATCHES_PER_CYCLE {
        let result = match wallet_rpc.z_shield_coinbase(mining_address, pool_address, Some(50)).await {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("{e}");
                if msg.contains("No spendable transparent outputs")
                    || msg.contains("Insufficient")
                    || msg.contains("No funds")
                    // zecd's "nothing mature to shield" (-6) benign case.
                    || msg.contains("Could not find any coinbase funds")
                {
                    break;
                }
                return Err(anyhow::anyhow!("z_shieldcoinbase failed: {e}"));
            }
        };

        // zecd dialect: the response carries only `opid` — one sweep of ALL
        // mature coinbase per call, no batching fields. Treat that as a single
        // full batch (counts unknown → 1/0) and stop after it.
        let zecd_dialect = result.get("shieldingUTXOs").is_none();
        let shielding_utxos = result
            .get("shieldingUTXOs")
            .and_then(|v| v.as_u64())
            .unwrap_or(if zecd_dialect { 1 } else { 0 });
        let shielding_value = result.get("shieldingValue").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let remaining_utxos = if zecd_dialect {
            0
        } else {
            result.get("remainingUTXOs").and_then(|v| v.as_u64()).unwrap_or(0)
        };
        let opid = result.get("opid").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();

        if shielding_utxos == 0 || opid == "unknown" {
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

        // Wait before queuing more work. With MAX_IN_FLIGHT_SHIELDING_BATCHES=1
        // this keeps shielding serialized for Zallet compatibility.
        if opids.len() as u32 >= MAX_IN_FLIGHT_SHIELDING_BATCHES || remaining_utxos == 0 {
            let mut total_ok = 0u64;
            let mut total_val = 0.0f64;
            for (b, op, utxos, val) in opids.drain(..) {
                match wait_for_operation(wallet_rpc, &op).await? {
                    OpResult::Success(txid) => {
                        total_ok += utxos;
                        total_val += val;
                        info!(batch = b, txid = %txid, "Shielding batch complete");
                        if let Err(e) = db.record_tx_cost("shield", &txid, estimate_shield_fee(utxos)).await {
                            warn!(error = %e, txid = %txid, "Failed to record shielding tx cost");
                        }
                    }
                    OpResult::Failed(msg) => {
                        warn!(batch = b, error = %msg, "Shielding batch failed");
                    }
                }
            }
            if total_ok > 0 {
                info!(utxos = total_ok, value_zec = total_val, "Shielding batch group done");
            }
        }

        if remaining_utxos == 0 {
            break;
        }
    }

    // Wait for any remaining queued operations.
    for (b, op, utxos, _val) in opids.drain(..) {
        match wait_for_operation(wallet_rpc, &op).await? {
            OpResult::Success(txid) => {
                info!(batch = b, txid = %txid, "Shielding batch complete");
                if let Err(e) = db.record_tx_cost("shield", &txid, estimate_shield_fee(utxos)).await {
                    warn!(error = %e, txid = %txid, "Failed to record shielding tx cost");
                }
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
    node_rpc: &ZcashRpcClient,
    pool_address: &str,
    mining_address: &str,
    min_payout_zatoshis: i64,
    reserve_min_zatoshis: i64,
    balance_margin: f64,
    network: &str,
    pay_immature: bool,
    coalesce_cooldown_secs: i64,
    coalesce_override_zatoshis: i64,
) -> anyhow::Result<usize> {
    // Immediate-payout safeguard: only skip the maturity gate while the
    // total credits on still-pending blocks (= worst-case orphan loss the
    // reserve could be asked to absorb) fit within the reserve. Past that,
    // fall back to mature-only for the round; exposure shrinks as blocks
    // confirm.
    let mut include_immature = false;
    if pay_immature {
        let exposure = db.get_immature_exposure().await?;
        if exposure <= reserve_min_zatoshis {
            include_immature = true;
        } else {
            info!(
                exposure_zatoshis = exposure,
                reserve_min_zatoshis,
                "Immature exposure exceeds reserve — mature-only payouts this round"
            );
        }
    }
    let pending = db
        .get_pending_payouts(
            min_payout_zatoshis, include_immature,
            coalesce_cooldown_secs, coalesce_override_zatoshis,
        )
        .await?;
    if pending.is_empty() {
        return Ok(0);
    }

    let total_payout_zatoshis: i64 = pending.iter().map(|p| p.amount).sum();
    let total_payout_zec = total_payout_zatoshis as f64 / ZATOSHIS_PER_ZEC;

    // Spendable-balance gate for the round. Dialect-neutral: zallet answers
    // z_gettotalbalance(minconf 3); zecd answers via getbalances, whose
    // `mine.trusted` applies its ZIP-315 confirmations policy (config
    // `trusted_confirmations`, which we set to 3 to match).
    let private_balance = match rpc.wallet_balances(3).await {
        Ok(b) => b.spendable,
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to check wallet balance: {e}"));
        }
    };

    // Enforce the reserve threshold: only spend the portion of the balance
    // above the reserve. Keeps a buffer so subsequent payouts can fire
    // without waiting for fresh shielding.
    let reserve_min_zec = reserve_min_zatoshis as f64 / ZATOSHIS_PER_ZEC;
    let spendable_zec = (private_balance - reserve_min_zec).max(0.0);
    let available_zec = (spendable_zec * balance_margin).max(0.0);

    if available_zec < min_payout_zatoshis as f64 / ZATOSHIS_PER_ZEC {
        info!(
            private_balance, reserve_min_zec, available_zec,
            "Shielded balance too low for any payouts (reserve protected)"
        );
        return Ok(0);
    }

    let scale = if available_zec >= total_payout_zec { 1.0 } else { available_zec / total_payout_zec };

    let mut payout_list: Vec<(usize, i64, String)> = Vec::new();
    // Audit NEW-G: parse the DB timestamp instead of comparing formatted
    // strings — lexicographic comparison silently misbehaves if SQLite ever
    // stores fractional seconds or a different separator. Parse failure is
    // treated as NOT old (safe direction: never redirect a miner's funds on
    // a formatting accident).
    let two_days_ago = Utc::now() - chrono::Duration::days(2);
    let is_older_than_two_days = |created_at: &str| -> bool {
        chrono::NaiveDateTime::parse_from_str(created_at, "%Y-%m-%d %H:%M:%S")
            .map(|t| t.and_utc() <= two_days_ago)
            .unwrap_or_else(|_| {
                warn!(created_at, "Unparseable miner created_at; treating as recent");
                false
            })
    };

    for (i, p) in pending.iter().enumerate() {
        let pay_to = if is_valid_zcash_address(&p.address, network) {
            p.address.clone()
        } else if is_older_than_two_days(&p.created_at) {
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

    let attempt_id = match db
        .create_payout_attempt(payout_list.len() as i64, actual_total_zatoshis, "loop")
        .await
    {
        Ok(id) => id,
        Err(e) => {
            // The reservation that makes a payout crash-safe is keyed to this
            // attempt row, so without it we must not send. Retry next cycle.
            warn!(error = %e, "Failed to create payout_attempt row; skipping payout round");
            return Ok(0);
        }
    };

    // Round-3 pre-debit saga: RESERVE funds (pending -> paying) BEFORE the send.
    // From here a crash leaves the funds in `paying`, out of `pending`, so the
    // payout loop cannot re-select and re-pay them; startup reconciliation
    // resolves the in-flight attempt by checking its txid on chain.
    let reserve_items: Vec<(i64, i64)> = payout_list
        .iter()
        .map(|(i, amt, _)| (pending[*i].miner_id, *amt))
        .collect();
    let reserved = match db.reserve_payout(attempt_id, &reserve_items).await {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "Payout reservation failed; nothing sent");
            let _ = db
                .update_payout_attempt(attempt_id, "failed", None, None, Some(&format!("reserve failed: {e}")))
                .await;
            return Err(anyhow::anyhow!("payout reservation failed: {e}"));
        }
    };
    if reserved.is_empty() {
        let _ = db
            .update_payout_attempt(attempt_id, "failed", None, None, Some("no funds reserved (balances changed since selection)"))
            .await;
        return Ok(0);
    }
    // Send to exactly the miners actually reserved.
    let reserved_ids: std::collections::HashSet<i64> = reserved.iter().map(|(m, _)| *m).collect();
    let payout_list: Vec<(usize, i64, String)> = payout_list
        .into_iter()
        .filter(|(i, _, _)| reserved_ids.contains(&pending[*i].miner_id))
        .collect();

    // Merge to unique destinations in exact integer zatoshis, converting each
    // destination total to ZEC exactly once (a single i64->ZEC conversion avoids
    // the f64 accumulation Zallet rejected as "Invalid amount"). One send.
    let mut merged: std::collections::BTreeMap<&str, i64> = std::collections::BTreeMap::new();
    for (_, zats, addr) in &payout_list {
        *merged.entry(addr.as_str()).or_insert(0) += *zats;
    }
    let amounts: Vec<(&str, f64)> = merged
        .into_iter()
        .map(|(addr, zats)| (addr, zats as f64 / ZATOSHIS_PER_ZEC))
        .collect();

    // Durably mark the attempt 'submitting' BEFORE the send. Without this, a
    // crash (or a failed status write) between Zallet accepting the request
    // and our 'sent' write leaves 'queued' + no opid — which the reconciler
    // reads as "never submitted" and expiry-refunds, while the tx may still
    // broadcast (external review 2026-08-21). 'submitting'/'sent' with no opid
    // are parked as fate-unknown instead. Fail CLOSED if the write fails:
    // nothing has been sent yet, so refunding is safe.
    if let Err(e) = db.update_payout_attempt(attempt_id, "submitting", None, None, None).await {
        error!(error = %e, "could not durably mark attempt submitting; refunding (nothing sent)");
        let _ = db.refund_payout(attempt_id).await;
        let _ = db
            .update_payout_attempt(attempt_id, "failed", None, None,
                Some(&format!("could not mark submitting (refunded, nothing sent): {e}")))
            .await;
        return Err(anyhow::anyhow!("could not mark attempt submitting: {e}"));
    }

    let opid = match rpc.z_sendmany(pool_address, &amounts).await {
        Ok(opid) => opid,
        Err(e) => {
            let msg = format!("{e}");
            // Only a JSON-RPC error RESPONSE proves the wallet queued nothing —
            // the wallet answered, so refunding is safe. A transport failure
            // (HTTP timeout, connection reset) is fate-unknown: the request may
            // have reached Zallet, which queues the operation before we ever see
            // a response, so the tx can still broadcast. Refunding there is the
            // July-22 double-pay class one stage earlier than the
            // wait_for_operation hole round-3 closed. Leave the reservation in
            // `paying` (status 'sent', no opid); the reconciler parks it as
            // UNRESOLVABLE for the operator instead of ever auto-refunding.
            if !matches!(e, node_rpc::RpcError::JsonRpc(_)) {
                warn!(error = %msg, "z_sendmany fate unknown (transport); leaving reservation for reconciliation");
                let _ = db
                    .update_payout_attempt(
                        attempt_id, "sent", None, None,
                        Some(&format!("z_sendmany fate unknown, NOT refunded: {msg}")),
                    )
                    .await;
                return Err(anyhow::anyhow!("z_sendmany fate unknown: {e}"));
            }
            // Not enough spendable balance — including the common "have 0 while
            // notes exist but the Orchard witness hasn't filled" case. Refund the
            // reservation and defer; the next cycle recomputes the scale against
            // the live balance (the rescale, one cycle later, without churn).
            let deferred = msg.contains("Insufficient balance");
            let _ = db.refund_payout(attempt_id).await;
            let note = if deferred {
                format!("deferred: insufficient spendable balance (refunded): {msg}")
            } else {
                format!("z_sendmany failed (refunded): {msg}")
            };
            let _ = db.update_payout_attempt(attempt_id, "failed", None, None, Some(&note)).await;
            if deferred {
                info!(error = %msg, "z_sendmany deferred; reservation refunded, retry next cycle");
                return Ok(0);
            }
            error!(error = %msg, "z_sendmany failed; reservation refunded");
            return Err(anyhow::anyhow!("z_sendmany failed: {e}"));
        }
    };

    info!(opid = %opid, "z_sendmany submitted, waiting for completion");
    if let Err(e) = db.update_payout_attempt(attempt_id, "sent", Some(&opid), None, None).await {
        // Keep going with the opid in hand; if we crash before confirm, the
        // attempt is still 'submitting' (no opid) and the reconciler parks it.
        warn!(error = %e, opid = %opid, "failed to record opid on attempt; continuing (reconciler parks on crash)");
    }

    let txid = match wait_for_operation(rpc, &opid).await {
        Ok(OpResult::Success(txid)) => {
            info!(opid = %opid, txid = %txid, "Payout operation reported success");
            txid
        }
        Ok(OpResult::Failed(msg)) => {
            // Operation definitively failed: no tx exists, so refunding is safe.
            let _ = db.refund_payout(attempt_id).await;
            let _ = db
                .update_payout_attempt(attempt_id, "failed", None, None, Some(&format!("z_sendmany operation failed: {msg}")))
                .await;
            return Err(anyhow::anyhow!("z_sendmany failed: {msg}"));
        }
        Err(e) => {
            // Fate unknown (the tx may have broadcast). Do NOT refund; leave the
            // reservation 'sent' for reconciliation to resolve against the chain.
            warn!(error = %e, opid = %opid, "wait_for_operation errored; leaving reservation for reconciliation");
            let _ = db
                .update_payout_attempt(attempt_id, "sent", Some(&opid), None, Some(&format!("wait_for_operation error: {e}")))
                .await;
            return Err(e);
        }
    };

    // Verify the tx is actually visible on the node before finalizing. Success
    // from z_getoperationstatus only means Zallet finished proof generation, not
    // that the tx broadcast. Retry briefly for mempool propagation.
    let mut verify_ok = false;
    let mut last_verify_err = String::new();
    for _ in 0..6 {
        match node_rpc.get_raw_transaction(&txid, 1).await {
            Ok(_) => {
                verify_ok = true;
                break;
            }
            Err(e) => {
                last_verify_err = format!("{e}");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
    if !verify_ok {
        // Ambiguous: the tx might broadcast late or might have expired. Do NOT
        // refund (a late broadcast would then double-pay) and do NOT confirm.
        // Leave the reservation in `paying` and the attempt 'sent' with its txid;
        // reconciliation checks the txid on chain later and confirms or refunds.
        warn!(
            opid = %opid, txid = %txid, error = %last_verify_err,
            "Payout tx not visible yet — leaving reservation for reconciliation to resolve"
        );
        let _ = db
            .update_payout_attempt(attempt_id, "sent", None, Some(&txid), Some(&format!("tx not yet visible: {last_verify_err}")))
            .await;
        return Ok(0);
    }
    info!(txid = %txid, "Payout transaction broadcast and visible on node");

    // Record the pool's ZIP-317 cost for this payout tx so the next block's
    // distribution recovers it. Recipients = unique destination addresses.
    let recipients = payout_list
        .iter()
        .map(|(_, _, addr)| addr.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    if let Err(e) = db.record_tx_cost("payout", &txid, estimate_payout_fee(recipients)).await {
        warn!(error = %e, txid = %txid, "Failed to record payout tx cost");
    }

    // CONFIRM: atomically move the reservation paying -> paid and write the
    // payouts rows (round-3). Idempotent, so a reconciliation re-run is harmless.
    let count = match db.confirm_payout(attempt_id, &txid).await {
        Ok(n) => n as usize,
        Err(e) => {
            // Tx is on chain but ledger finalization failed. Do NOT refund; leave
            // the reservation 'sent' + txid for reconciliation to re-run confirm.
            error!(error = %e, txid = %txid, "confirm_payout failed after broadcast; leaving for reconciliation");
            let _ = db
                .update_payout_attempt(attempt_id, "sent", None, Some(&txid), Some(&format!("confirm failed: {e}")))
                .await;
            return Ok(0);
        }
    };
    let _ = db.update_payout_attempt(attempt_id, "confirmed", None, Some(&txid), None).await;
    info!(miners = count, txid = %txid, "Payout round completed and confirmed");

    Ok(count)
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

/// Money-path lifecycle suite (audit #18): walk real funds through the full
/// found -> credited -> matured -> reserved -> sent -> confirmed pipeline and
/// its failure branches, asserting EXACT zatoshi conservation at every step
/// (operator policy: perfect accounting, no tolerance margins).
#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use crate::reconciler::tests::{mock_rpc, setup_db};
    use std::collections::HashMap;

    const MINER_ADDR: &str = "tmFU5Ak942B7SciQpZCh3xH76QV3UmJgnDd"; // valid testnet t-addr
    const TXID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const REWARD: i64 = 123_750_000;

    async fn sums(pool: &sqlx::SqlitePool) -> (i64, i64, i64, i64) {
        let b: (i64, i64, i64) = sqlx::query_as(
            "SELECT COALESCE(SUM(pending),0), COALESCE(SUM(paying),0), COALESCE(SUM(paid),0) FROM balances",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let p: (i64,) = sqlx::query_as("SELECT COALESCE(SUM(amount),0) FROM payouts")
            .fetch_one(pool)
            .await
            .unwrap();
        (b.0, b.1, b.2, p.0)
    }

    fn happy_wallet() -> HashMap<&'static str, serde_json::Value> {
        HashMap::from([
            ("z_gettotalbalance", serde_json::json!({"private": "100.0", "total": "100.0"})),
            ("z_sendmany", serde_json::json!("opid-lifecycle")),
            (
                "z_getoperationstatus",
                serde_json::json!([{"status": "success", "result": {"txid": TXID}}]),
            ),
        ])
    }

    fn node_with_tx() -> HashMap<&'static str, serde_json::Value> {
        HashMap::from([("getrawtransaction", serde_json::json!({"txid": TXID, "height": 10}))])
    }

    /// Credit a confirmed block to a fresh miner; returns (miner_id, block_id).
    async fn credited_block(db: &PoolDb, height: i64) -> (i64, i64) {
        let m = db.get_or_create_miner(MINER_ADDR).await.unwrap();
        let w = db.get_or_create_worker(m.id, "rig").await.unwrap();
        let block_id = db
            .record_block(height, &format!("hash{height}"), REWARD, Some(REWARD), w.id, None)
            .await
            .unwrap();
        db.distribute_block_credits(block_id, &[(m.id, REWARD)]).await.unwrap();
        db.update_block_status(block_id, "confirmed").await.unwrap();
        (m.id, block_id)
    }

    async fn run_payouts(db: &PoolDb, wallet_url: &str, node_url: &str) -> anyhow::Result<usize> {
        let wallet = ZcashRpcClient::new(wallet_url);
        let node = ZcashRpcClient::new(node_url);
        process_payouts(
            db, &wallet, &node, "upooladdr", "tmFU5Ak942B7SciQpZCh3xH76QV3UmJgnDd",
            1_000_000, 0, 1.0, "testnet", false, 0, i64::MAX,
        )
        .await
    }

    #[tokio::test]
    async fn happy_path_exact_conservation() {
        let (db, pool) = setup_db().await;
        let (_m, _b) = credited_block(&db, 100).await;
        assert_eq!(sums(&pool).await, (REWARD, 0, 0, 0), "credit lands in pending exactly");

        let wallet_url = mock_rpc(happy_wallet()).await;
        let node_url = mock_rpc(node_with_tx()).await;
        let n = run_payouts(&db, &wallet_url, &node_url).await.unwrap();
        assert_eq!(n, 1, "one miner paid");

        assert_eq!(
            sums(&pool).await,
            (0, 0, REWARD, REWARD),
            "payout must move the exact credit pending -> paid, payouts row equal"
        );
        let (st, leak): (String, i64) = (
            sqlx::query_scalar("SELECT status FROM payout_attempts ORDER BY id DESC LIMIT 1")
                .fetch_one(&pool).await.unwrap(),
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM payout_items pi JOIN payout_attempts pa ON pa.id=pi.attempt_id \
                 WHERE pa.status IN ('confirmed','failed')",
            )
            .fetch_one(&pool).await.unwrap(),
        );
        assert_eq!(st, "confirmed");
        assert_eq!(leak, 0, "no reservation items may survive a settled attempt");

        let (reward, balances, clawbacks) = db.get_accounting_invariant().await.unwrap();
        assert_eq!(reward, balances + clawbacks - 0, "invariant holds exactly");
    }

    /// Same happy path, but the wallet speaks the zecd dialect: no
    /// z_gettotalbalance (mock returns -5 -> fallback to getbalances with
    /// numeric fields). Proves the payout pipeline is wallet-agnostic.
    #[tokio::test]
    async fn happy_path_zecd_dialect_exact_conservation() {
        let (db, pool) = setup_db().await;
        credited_block(&db, 150).await;

        let wallet_url = mock_rpc(HashMap::from([
            (
                "getbalances",
                serde_json::json!({"mine": {"trusted": 100.0, "untrusted_pending": 0.0, "immature": 0.0, "coinbase": 0.0}}),
            ),
            ("z_sendmany", serde_json::json!("opid-zecd")),
            (
                "z_getoperationstatus",
                serde_json::json!([{"status": "success", "result": {"txid": TXID}}]),
            ),
        ]))
        .await;
        let node_url = mock_rpc(node_with_tx()).await;
        let n = run_payouts(&db, &wallet_url, &node_url).await.unwrap();
        assert_eq!(n, 1, "zecd-dialect payout must complete");
        assert_eq!(
            sums(&pool).await,
            (0, 0, REWARD, REWARD),
            "zecd dialect must conserve exactly like zallet"
        );
    }

    #[tokio::test]
    async fn orphan_after_credit_reverses_exactly() {
        let (db, pool) = setup_db().await;
        let (_m, block_id) = credited_block(&db, 200).await;
        assert_eq!(sums(&pool).await, (REWARD, 0, 0, 0));

        db.orphan_block(block_id, REWARD).await.unwrap();
        assert_eq!(
            sums(&pool).await,
            (0, 0, 0, 0),
            "orphan must claw back the exact credit, no residue in any bucket"
        );
        let st: String = sqlx::query_scalar("SELECT status FROM blocks WHERE id = ?1")
            .bind(block_id).fetch_one(&pool).await.unwrap();
        assert_eq!(st, "orphaned");
    }

    #[tokio::test]
    async fn crash_mid_saga_refunds_then_pays_exactly_once() {
        let (db, pool) = setup_db().await;
        let (m, _b) = credited_block(&db, 300).await;

        // Reserve as the payout loop would, then "crash" before z_sendmany.
        let attempt = db.create_payout_attempt(1, REWARD, "loop").await.unwrap();
        let reserved = db.reserve_payout(attempt, &[(m, REWARD)]).await.unwrap();
        assert_eq!(reserved.len(), 1);
        assert_eq!(sums(&pool).await, (0, REWARD, 0, 0), "reservation moves pending -> paying");

        // Recovery: reconciliation refunds the expired, never-sent reservation.
        sqlx::query("UPDATE payout_attempts SET created_at = datetime('now','-2 hours') WHERE id = ?1")
            .bind(attempt).execute(&pool).await.unwrap();
        let node_url = mock_rpc(HashMap::new()).await;
        let wallet_url = mock_rpc(HashMap::new()).await;
        let r = crate::reconciler::tests::reconciler(db.clone(), &node_url, &wallet_url);
        r.reconcile_reserved_payouts_once(None).await;
        assert_eq!(sums(&pool).await, (REWARD, 0, 0, 0), "refund restores pending exactly");

        // Next round pays for real — total paid must equal the single credit.
        let wallet_url = mock_rpc(happy_wallet()).await;
        let node_url = mock_rpc(node_with_tx()).await;
        let n = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            run_payouts(&db, &wallet_url, &node_url),
        )
        .await
        .expect("payout round timed out")
        .unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            sums(&pool).await,
            (0, 0, REWARD, REWARD),
            "crash + recovery must pay exactly once, never twice"
        );
    }

    #[tokio::test]
    async fn sendmany_definite_failure_refunds() {
        // Wallet answers the balance check; the harness answers the unknown
        // z_sendmany with a JSON-RPC -5 error response = definite failure.
        let (db, pool) = setup_db().await;
        credited_block(&db, 500).await;
        let wallet = HashMap::from([(
            "z_gettotalbalance",
            serde_json::json!({"private": "100.0", "total": "100.0"}),
        )]);
        let wallet_url = mock_rpc(wallet).await;
        let node_url = mock_rpc(node_with_tx()).await;
        assert!(run_payouts(&db, &wallet_url, &node_url).await.is_err());
        assert_eq!(sums(&pool).await, (REWARD, 0, 0, 0), "JSON-RPC error -> refunded exactly");
        let st: String = sqlx::query_scalar("SELECT status FROM payout_attempts ORDER BY id DESC LIMIT 1")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(st, "failed");
    }

    #[tokio::test]
    async fn sendmany_transport_failure_parks_never_refunds() {
        // GENUINE transport-class failure on z_sendmany: the mock returns a
        // non-JSON 502 for that method (the client surfaces it as a
        // non-JsonRpc RpcError, same class as a timeout/reset). The send may
        // or may not have been accepted — funds must stay parked in paying.
        let (db, pool) = setup_db().await;
        credited_block(&db, 510).await;
        let wallet = HashMap::from([
            ("z_gettotalbalance", serde_json::json!({"private": "100.0", "total": "100.0"})),
            ("z_sendmany", serde_json::json!(crate::reconciler::tests::TRANSPORT_FAIL)),
        ]);
        let wallet_url = mock_rpc(wallet).await;
        let node_url = mock_rpc(node_with_tx()).await;
        assert!(run_payouts(&db, &wallet_url, &node_url).await.is_err());
        assert_eq!(
            sums(&pool).await,
            (0, REWARD, 0, 0),
            "transport failure must NOT refund: funds stay in paying"
        );
        let (st, opid): (String, Option<String>) =
            sqlx::query_as("SELECT status, opid FROM payout_attempts ORDER BY id DESC LIMIT 1")
                .fetch_one(&pool).await.unwrap();
        assert_eq!(st, "sent");
        assert!(opid.is_none());

        // And the reconciler, even long after expiry, parks rather than refunds.
        sqlx::query("UPDATE payout_attempts SET created_at = datetime('now','-6 hours')")
            .execute(&pool).await.unwrap();
        let r = crate::reconciler::tests::reconciler(db.clone(), &node_url, &wallet_url);
        let summary = r.reconcile_reserved_payouts_once(None).await;
        assert_eq!(sums(&pool).await, (0, REWARD, 0, 0), "still parked after reconciliation");
        assert!(summary.alerts.iter().any(|a| a.contains("UNRESOLVABLE")), "{:?}", summary.alerts);
    }

    #[tokio::test]
    async fn submitting_no_opid_reservation_parks_never_refunds() {
        // The crash window: attempt durably 'submitting', z_sendmany may have
        // been accepted, process died before recording the opid. Park forever.
        let (db, pool) = setup_db().await;
        let (m, _b) = credited_block(&db, 600).await;
        let attempt = db.create_payout_attempt(1, REWARD, "loop").await.unwrap();
        db.reserve_payout(attempt, &[(m, REWARD)]).await.unwrap();
        db.update_payout_attempt(attempt, "submitting", None, None, None).await.unwrap();
        sqlx::query("UPDATE payout_attempts SET created_at = datetime('now','-6 hours') WHERE id = ?1")
            .bind(attempt).execute(&pool).await.unwrap();
        let node_url = mock_rpc(HashMap::new()).await;
        let wallet_url = mock_rpc(HashMap::new()).await;
        let r = crate::reconciler::tests::reconciler(db.clone(), &node_url, &wallet_url);
        let summary = r.reconcile_reserved_payouts_once(None).await;
        assert_eq!(sums(&pool).await, (0, REWARD, 0, 0), "submitting/no-opid must stay parked");
        assert!(summary.alerts.iter().any(|a| a.contains("UNRESOLVABLE")), "{:?}", summary.alerts);
    }

    #[tokio::test]
    async fn queued_no_opid_reservation_still_refunds_at_expiry() {
        // Crash BEFORE the 'submitting' write: nothing could have been sent.
        // The expiry refund must keep working (regression guard for fix 1).
        let (db, pool) = setup_db().await;
        let (m, _b) = credited_block(&db, 610).await;
        let attempt = db.create_payout_attempt(1, REWARD, "loop").await.unwrap();
        db.reserve_payout(attempt, &[(m, REWARD)]).await.unwrap();
        sqlx::query("UPDATE payout_attempts SET created_at = datetime('now','-6 hours') WHERE id = ?1")
            .bind(attempt).execute(&pool).await.unwrap();
        let node_url = mock_rpc(HashMap::new()).await;
        let wallet_url = mock_rpc(HashMap::new()).await;
        let r = crate::reconciler::tests::reconciler(db.clone(), &node_url, &wallet_url);
        r.reconcile_reserved_payouts_once(None).await;
        assert_eq!(sums(&pool).await, (REWARD, 0, 0, 0), "queued/no-opid past expiry -> refunded exactly");
    }

    #[tokio::test]
    async fn reorged_payout_voids_and_repays_exactly() {
        let (db, pool) = setup_db().await;
        credited_block(&db, 400).await;
        let wallet_url = mock_rpc(happy_wallet()).await;
        let node_url = mock_rpc(node_with_tx()).await;
        run_payouts(&db, &wallet_url, &node_url).await.unwrap();
        assert_eq!(sums(&pool).await, (0, 0, REWARD, REWARD));

        // The payout tx later reorgs out. Age the row past the 60-min guard,
        // then void: paid must return to pending, the payouts row must go.
        sqlx::query("UPDATE payouts SET created_at = datetime('now','-2 hours')")
            .execute(&pool).await.unwrap();
        db.void_reorged_payout(TXID).await.unwrap();
        assert_eq!(
            sums(&pool).await,
            (REWARD, 0, 0, 0),
            "void must restore the exact amount to pending and remove the payouts row"
        );

        // Re-pay: end state identical to a single clean payout.
        let wallet_url = mock_rpc(happy_wallet()).await;
        let node_url = mock_rpc(node_with_tx()).await;
        run_payouts(&db, &wallet_url, &node_url).await.unwrap();
        assert_eq!(sums(&pool).await, (0, 0, REWARD, REWARD), "reorg + repay conserves exactly");
    }
}
