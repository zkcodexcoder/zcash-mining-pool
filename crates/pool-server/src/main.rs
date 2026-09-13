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
use pool_core::{BlockAssembler, JobManager, ShareValidator, VardiffConfig};
use pool_db::PoolDb;
use pool_db::pps_policy::PpsPolicy;
use pool_core::pps_funding::PpsFundingRoute;
use rewards::{PplnsCalculator, RewardMode};
use stratum::StratumServer;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Config {
    pool: PoolConfig,
    stratum: StratumConfig,
    node: NodeConfig,
    difficulty: DifficultyConfig,
    pplns: PplnsConfig,
    pps: Option<PpsPolicy>,
    pps_funding: Option<PpsFundingRoute>,
    #[serde(default)]
    payout: PayoutConfig,
    api: ApiConfig,
    database: DatabaseConfig,
    #[serde(default)]
    admin: Option<AdminConfig>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
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
#[allow(dead_code)]
struct PoolConfig {
    name: String,
    fee_percent: f64,
    #[serde(default = "default_network")]
    network: String,
    /// Public hostname or IP for stratum connection URLs shown on the dashboard.
    #[serde(default)]
    hostname: Option<String>,
    /// Text to inject into coinbase scriptSig for pool identification (e.g. "Legends").
    #[serde(default)]
    coinbase_tag: Option<String>,
    /// Optional warning/info banner shown at the top of the public dashboard.
    #[serde(default)]
    banner: Option<String>,
}

fn default_network() -> String { "testnet".to_string() }

#[derive(Debug, Deserialize)]
struct StratumConfig {
    /// Legacy: single listen address.
    #[serde(default)]
    listen_addr: Option<String>,
    /// Legacy: list of listen addresses.
    #[serde(default)]
    listen_addrs: Option<Vec<String>>,
    nonce1_size: usize,
    /// Legacy: per-port difficulty overrides (port number as string -> difficulty).
    #[serde(default)]
    port_difficulty: HashMap<String, f64>,
    /// Structured port definitions (preferred over legacy fields).
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

fn default_port_description() -> String { "Default".to_string() }

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

    /// Build port_difficulty map, merging structured ports with legacy overrides.
    fn resolved_port_difficulty(&self) -> HashMap<u16, f64> {
        let mut map: HashMap<u16, f64> = self.port_difficulty.iter()
            .filter_map(|(k, &v)| k.parse::<u16>().ok().map(|port| (port, v)))
            .collect();
        // Structured ports override legacy entries
        for p in &self.ports {
            if let Some(diff) = p.initial_difficulty {
                if let Some(port) = p.addr.split(':').last().and_then(|s| s.parse::<u16>().ok()) {
                    map.insert(port, diff);
                }
            }
        }
        map
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
    #[serde(default)]
    use_longpoll: bool,
    #[serde(default = "default_longpoll_timeout_secs")]
    longpoll_timeout_secs: u64,
}

fn default_longpoll_timeout_secs() -> u64 {
    60
}

#[derive(Debug, Deserialize)]
struct PplnsConfig {
    window_multiplier: f64,
    /// Unknown modes are rejected; PPS additionally requires bounded policy.
    #[serde(default = "default_reward_mode", deserialize_with = "deserialize_reward_mode")]
    mode: RewardMode,
}

fn default_reward_mode() -> RewardMode {
    RewardMode::Pplns
}

fn deserialize_reward_mode<'de, D>(deserializer: D) -> std::result::Result<RewardMode, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mode = String::deserialize(deserializer)?;
    if mode.eq_ignore_ascii_case("pplns") {
        Ok(RewardMode::Pplns)
    } else if mode.eq_ignore_ascii_case("solo") {
        Ok(RewardMode::Solo)
    } else if mode.eq_ignore_ascii_case("pps") {
        Ok(RewardMode::Pps)
    } else {
        // Fail while parsing configuration, before opening the database or
        // starting services. Never silently turn a PPS request into PPLNS.
        Err(serde::de::Error::custom(
            "unsupported [pplns].mode: expected pplns, solo or pps",
        ))
    }
}

fn validate_pps_config(mode: RewardMode, policy: Option<&PpsPolicy>, network: &str) -> Result<()> {
    match (mode == RewardMode::Pps, policy) {
        (true, Some(p)) => {
            p.validate(network).map_err(anyhow::Error::msg)?;
        }
        (true, None) => anyhow::bail!("PPS mode requires explicit [pps] bounded policy"),
        (false, Some(_)) => anyhow::bail!("[pps] policy requires explicit [pplns].mode=pps"),
        (false, None) => {},
    }
    Ok(())
}

fn validate_pps_legacy_reserve(policy: &PpsPolicy, reserve: Option<f64>) -> Result<()> {
    // The legacy dashboard's established missing-field default is zero. Ceil
    // rather than truncate makes this comparison conservative at its f64
    // boundary; the new authoritative PPS floor remains exact integer zats.
    let legacy = reserve.unwrap_or(0.0) * 100_000_000.0;
    anyhow::ensure!(legacy.is_finite() && legacy >= 0.0
        && legacy.ceil() <= policy.reserve_min_zatoshis as f64,
        "PPS reserve floor must preserve the existing legacy payout reserve");
    Ok(())
}

fn validate_pps_funding_route(policy: Option<&PpsPolicy>, configured: Option<PpsFundingRoute>)
    -> Result<PpsFundingRoute>
{
    anyhow::ensure!(policy.is_some() || configured.is_none(),
        "PPS funding route requires an explicit PPS policy");
    let route = configured.unwrap_or_default();
    if let Some(policy) = policy { route.validate(&policy.epoch_config())?; }
    Ok(route)
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
    // Existing legacy dashboard reserve, in ZEC. Read here solely to prevent
    // a new immutable PPS reserve floor from reducing the prior protection.
    #[serde(default)]
    reserve_min: Option<f64>,
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
            reserve_min: None,
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
    validate_pps_config(config.pplns.mode, config.pps.as_ref(), &config.pool.network)?;
    let funding_route = validate_pps_funding_route(config.pps.as_ref(), config.pps_funding)?;

    let stratum_addrs = config.stratum.addrs();
    info!(
        name = %config.pool.name,
        fee = %config.pool.fee_percent,
        stratum = ?stratum_addrs,
        api = %config.api.listen_addr,
        node = %config.node.rpc_url,
        "Configuration loaded"
    );

    // Initialize database. busy_timeout lets a writer WAIT for the WAL lock
    // instead of instantly failing with SQLITE_BUSY — the exact failure that
    // silently dropped block 90's reward distribution. synchronous=NORMAL is the
    // standard safe+fast setting for a WAL pool. Set per-connection via options
    // so every pooled connection inherits them.
    let db_opts: sqlx::sqlite::SqliteConnectOptions = config
        .database
        .url
        .parse()
        .with_context(|| "Invalid database url")?;
    let db_opts = db_opts
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(if config.pps.is_some() {
            sqlx::sqlite::SqliteSynchronous::Full
        } else { sqlx::sqlite::SqliteSynchronous::Normal })
        .busy_timeout(std::time::Duration::from_secs(20));
    let db_pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect_with(db_opts)
        .await
        .with_context(|| "Failed to connect to database")?;
    let db = PoolDb::new(db_pool);
    db.run_migrations()
        .await
        .with_context(|| "Failed to run migrations")?;
    // Audit #16: refuse to start on a schema hole — migrations run
    // best-effort, so a partially-applied one must fail LOUD here, not
    // corrupt the money path later.
    db.assert_critical_schema()
        .await
        .with_context(|| "Critical schema verification failed — refusing to start")?;
    db.set_wal_mode()
        .await
        .with_context(|| "Failed to enable WAL mode")?;
    info!("Database initialized (WAL mode)");
    db.assert_reward_mode(config.pps.is_some()).await?;

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

    let port_difficulty = config.stratum.resolved_port_difficulty();
    if !port_difficulty.is_empty() {
        info!(?port_difficulty, "Per-port difficulty overrides loaded");
    }

    // Initialize Stratum server
    let (event_tx, event_rx) = mpsc::channel(1024);
    let (mut stratum, _notify_rx) = StratumServer::new_with_latest_notify(
        config.stratum.nonce1_size,
        event_tx,
        Arc::clone(&latest_notify),
    );
    // Announce each port's real starting difficulty at subscribe time
    // (pre-authorize). Pool checkers like MiningRigRentals subscribe without
    // authorizing and report whatever the first set_target says — so it must
    // be the port's configured initial, not a placeholder. Uses the same
    // difficulty→target conversion as the authorize-time path.
    let subscribe_initial: HashMap<u16, (f64, String)> = port_difficulty
        .iter()
        .map(|(port, diff)| {
            (*port, (*diff, pool_core::difficulty::difficulty_to_target_hex(*diff)))
        })
        .collect();
    stratum.set_initial_difficulty(
        subscribe_initial,
        (
            initial_difficulty,
            pool_core::difficulty::difficulty_to_target_hex(initial_difficulty),
        ),
    );
    if config.pps.is_some() {
        stratum.set_fixed_share_target(stratum::FixedShareTarget::new(pool_target)?)?;
    }
    let stratum = Arc::new(stratum);

    // Stall detection: last time we got a block template (unix ms)
    let last_template_at_ms = Arc::new(AtomicI64::new(0));

    // Initialize Job Manager (updates last_template_at_ms on each successful poll)
    let mut job_manager = JobManager::new_with_stall_tracking_and_notify(
        Arc::clone(&rpc),
        Arc::clone(&stratum),
        Some(Arc::clone(&last_template_at_ms)),
        Arc::clone(&latest_notify),
    );
    if let Some(ref tag) = config.pool.coinbase_tag {
        info!(coinbase_tag = %tag, "Coinbase tag injection enabled");
        job_manager.set_coinbase_tag(tag.as_bytes().to_vec());
    }
    job_manager.set_longpoll_config(pool_core::job::LongpollConfig {
        enabled: config.difficulty.use_longpoll,
        timeout: Duration::from_secs(config.difficulty.longpoll_timeout_secs),
    });
    let jobs = job_manager.jobs();
    // Grab the lag tracker before the JobManager is moved into its task —
    // the status loop snapshots it every cycle into pool_status so the
    // dashboard (separate process) can render template-lag stats.
    let lag_tracker = job_manager.lag_tracker();
    let longpoll_wakes = job_manager.longpoll_wake_count();
    let longpoll_fails = job_manager.longpoll_fail_count();
    let last_longpoll_fail_ms = job_manager.last_longpoll_fail_at_ms();

    // Initialize Block Assembler
    let block_assembler = Arc::new(BlockAssembler::new(Arc::clone(&rpc)));

    let vardiff_config = VardiffConfig {
        initial_difficulty,
        target_shares_per_minute: config.difficulty.target_shares_per_minute,
        retarget_interval_secs: config.difficulty.retarget_interval_secs as f64,
    };

    // Initialize reward calculator (PPLNS or Solo).
    let pplns_window = (config.pplns.window_multiplier * 1000.0) as i64;
    let reward_mode = config.pplns.mode;
    tracing::info!(?reward_mode, pplns_window, fee_percent = config.pool.fee_percent, "Reward calculator initialized");
    let pplns = Arc::new(PplnsCalculator::new(
        db.clone(),
        pplns_window,
        config.pool.fee_percent / 100.0,
        reward_mode,
    ));

    // Initialize Share Validator
    // Compute difficulty_multiplier for share→Sol/s conversion
    let difficulty_multiplier = {
        let target_f64 = pool_target.iter().enumerate().fold(0.0f64, |acc, (i, &b)| {
            acc + (b as f64) * 256.0f64.powi(31 - i as i32)
        });
        if target_f64 > 0.0 { 2.0f64.powi(256) / target_f64 } else { 1.0 }
    };

    // Shared counters for accepted/rejected shares (used by both validator and API).
    let shares_accepted = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let shares_rejected = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let rate_warn_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let rate_reject_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
    // Rejection breakdown counters (by error code).
    let rejects_low_diff = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let rejects_job_not_found = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let rejects_duplicate = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let rejects_other = Arc::new(std::sync::atomic::AtomicU64::new(0));

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
        Arc::clone(&shares_accepted),
        Arc::clone(&shares_rejected),
        Arc::clone(&rate_warn_count),
        Arc::clone(&rate_reject_count),
        Arc::clone(&rejects_low_diff),
        Arc::clone(&rejects_job_not_found),
        Arc::clone(&rejects_duplicate),
        Arc::clone(&rejects_other),
    );
    let share_validator = if let Some(policy) = &config.pps {
        validate_pps_legacy_reserve(policy, config.payout.reserve_min)?;
        let network = policy.network.parse().map_err(|_| anyhow::anyhow!("invalid PPS network"))?;
        // Audit B12: a wallet or chain-reference outage must never stop mining.
        // Try each startup collection briefly; if it is still unavailable, start
        // anyway without that lease. The background refresh loops acquire it;
        // until then crediting follows its normal gates (chain lease required,
        // funding advisory) and found blocks are still submitted.
        let chain_lease = {
            let mut attempt = 0u32;
            loop {
                match pool_core::pps_chain::verify_pps_chain(&rpc, network).await {
                    Ok(lease) => break Some(lease),
                    Err(error) if attempt < 4 => {
                        attempt += 1;
                        tracing::warn!(%error, attempt, "PPS chain evidence unavailable at startup; retrying in 15s");
                        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                    }
                    Err(error) => {
                        tracing::error!(%error, "PPS chain evidence still unavailable; starting without it (background refresh continues)");
                        break None;
                    }
                }
            }
        };
        // Never infer that the mining node also owns spendable wallet funds.
        // PPS needs its explicit existing wallet RPC endpoint; no wallet write
        // or automatic funding transfer is performed by this read-only gate.
        let wallet_url = config.payout.wallet_rpc_url.as_deref()
            .context("PPS requires explicit payout wallet RPC")?;
        let wallet_rpc = Arc::new(match (&config.payout.wallet_rpc_user, &config.payout.wallet_rpc_password) {
            (Some(user), Some(password)) => ZcashRpcClient::with_auth(wallet_url, user, password),
            (None, None) => ZcashRpcClient::new(wallet_url),
            _ => anyhow::bail!("PPS wallet authentication configuration incomplete"),
        });
        let epoch = policy.epoch_config();
        let payout_source = config.payout.pool_address.clone()
            .context("PPS requires explicit existing shielded payout source")?;
        let funding_lease = {
            let mut attempt = 0u32;
            loop {
                match pool_core::pps_funding::collect_pps_credit_funding_for_route(
                    &db, &wallet_rpc, &epoch, &payout_source, &rpc, &funding_route,
                ).await {
                    Ok(lease) => break Some(lease),
                    Err(error) if attempt < 4 => {
                        attempt += 1;
                        tracing::warn!(%error, attempt, "PPS funding evidence unavailable at startup; retrying in 15s");
                        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                    }
                    Err(error) => {
                        tracing::error!(%error, "PPS funding evidence still unavailable; starting without it (background refresh continues)");
                        break None;
                    }
                }
            }
        };
        let validator = share_validator.with_pps(pool_core::share::PpsRuntime {
            epoch: epoch.clone(), chain_lease, funding_lease: funding_lease.clone(), wallet_rpc, payout_source, funding_route,
        }).map_err(anyhow::Error::msg)?;
        // Activate the durable mode boundary. Activating a NEW epoch still
        // requires fresh funding evidence (and fails without it); restarting an
        // existing epoch treats funding as advisory, like crediting does.
        db.initialize_pps_epoch(&epoch, funding_lease.as_ref()).await?;
        db.pps_invariant().await?;
        validator
    } else { share_validator };

    // Audit #15: settle any block whose fate a previous crash left unknown,
    // and redistribute recent blocks whose distribution was swallowed —
    // BEFORE serving miners, so recovery isn't racing live traffic.
    share_validator.startup_block_sweep().await;

    // Write pool_started_at once, then update live stats every 5s into pool_status
    // so the standalone dashboard binary can read them.
    let status_db = db.clone();
    let status_template = Arc::clone(&last_template_at_ms);
    let status_accepted = Arc::clone(&shares_accepted);
    let status_rejected = Arc::clone(&shares_rejected);
    let status_rate_warn = Arc::clone(&rate_warn_count);
    let status_rate_reject = Arc::clone(&rate_reject_count);
    let status_rejects_low_diff = Arc::clone(&rejects_low_diff);
    let status_rejects_job_not_found = Arc::clone(&rejects_job_not_found);
    let status_rejects_duplicate = Arc::clone(&rejects_duplicate);
    let status_rejects_other = Arc::clone(&rejects_other);
    let status_lag_tracker = Arc::clone(&lag_tracker);
    let status_longpoll_wakes = Arc::clone(&longpoll_wakes);
    let status_longpoll_fails = Arc::clone(&longpoll_fails);
    let status_last_longpoll_fail_ms = Arc::clone(&last_longpoll_fail_ms);
    let status_handle = tokio::spawn(async move {
        let _ = status_db.set_pool_status(
            "pool_started_at",
            &chrono::Utc::now().timestamp().to_string(),
        ).await;
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let ms = status_template.load(std::sync::atomic::Ordering::Relaxed);
            let _ = status_db.set_pool_status("last_template_at_ms", &ms.to_string()).await;
            let acc = status_accepted.load(std::sync::atomic::Ordering::Relaxed);
            let _ = status_db.set_pool_status("shares_accepted", &acc.to_string()).await;
            let rej = status_rejected.load(std::sync::atomic::Ordering::Relaxed);
            let _ = status_db.set_pool_status("shares_rejected", &rej.to_string()).await;
            let rw = status_rate_warn.load(std::sync::atomic::Ordering::Relaxed);
            let _ = status_db.set_pool_status("rate_warn_count", &rw.to_string()).await;
            let rr = status_rate_reject.load(std::sync::atomic::Ordering::Relaxed);
            let _ = status_db.set_pool_status("rate_reject_count", &rr.to_string()).await;
            let rld = status_rejects_low_diff.load(std::sync::atomic::Ordering::Relaxed);
            let _ = status_db.set_pool_status("rejects_low_diff", &rld.to_string()).await;
            let rjn = status_rejects_job_not_found.load(std::sync::atomic::Ordering::Relaxed);
            let _ = status_db.set_pool_status("rejects_job_not_found", &rjn.to_string()).await;
            let rdu = status_rejects_duplicate.load(std::sync::atomic::Ordering::Relaxed);
            let _ = status_db.set_pool_status("rejects_duplicate", &rdu.to_string()).await;
            let rot = status_rejects_other.load(std::sync::atomic::Ordering::Relaxed);
            let _ = status_db.set_pool_status("rejects_other", &rot.to_string()).await;
            // Snapshot template-lag tracker as JSON. Read by the dashboard
            // (separate process) via pool_status; key is the contract.
            let snap = status_lag_tracker.snapshot();
            if let Ok(snap_json) = serde_json::to_string(&snap) {
                let _ = status_db.set_pool_status("template_lag_stats", &snap_json).await;
            }
            // Longpoll counters — wake/fail totals and the unix-ms timestamp of
            // the most recent failure. Dashboard reads these to render the
            // Longpoll row in the Mining Node card.
            let lw = status_longpoll_wakes.load(std::sync::atomic::Ordering::Relaxed);
            let _ = status_db.set_pool_status("longpoll_wakes_total", &lw.to_string()).await;
            let lf = status_longpoll_fails.load(std::sync::atomic::Ordering::Relaxed);
            let _ = status_db.set_pool_status("longpoll_fails_total", &lf.to_string()).await;
            let lfm = status_last_longpoll_fail_ms.load(std::sync::atomic::Ordering::Relaxed);
            let _ = status_db.set_pool_status("last_longpoll_fail_at_ms", &lfm.to_string()).await;
        }
    });

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

    // Validator runs in a single task and is the only consumer of share
    // events. If it ever exits or panics, the pool silently stops processing
    // shares while still accepting connections — exactly the wedge we hit in
    // production. Wrap the spawn so any unexpected termination crashes the
    // process loudly; an operator (or process supervisor) can then restart.
    // Aborts via .abort() during normal shutdown cancel the future before
    // these post-await lines run, so this only fires on real failures.
    // Stalled-validator watchdog. The return-catcher below only fires if the
    // validator task *returns*; the 2026-07-01 wedge was a deadlock inside an
    // event handler — the task never returned, so it went undetected for ~12h.
    // The validator stamps a heartbeat every loop iteration (≤5s when healthy,
    // driven by its snapshot tick even with zero miners). If the heartbeat
    // goes stale, the loop is wedged: terminate loudly so the supervisor
    // restarts the pool (the same remedy an operator applies by hand).
    let validator_heartbeat = share_validator.heartbeat();
    const VALIDATOR_STALL_SECS: i64 = 60;
    let watchdog_handle = tokio::spawn(async move {
        // Grace period so startup / first heartbeat isn't flagged.
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            tokio::time::sleep(Duration::from_secs(15)).await;
            let last = validator_heartbeat.load(std::sync::atomic::Ordering::Relaxed);
            let age_ms = chrono::Utc::now().timestamp_millis() - last;
            if age_ms > VALIDATOR_STALL_SECS * 1000 {
                error!(
                    stall_secs = age_ms / 1000,
                    threshold_secs = VALIDATOR_STALL_SECS,
                    "Share validator heartbeat is stale — validator is wedged (deadlocked), terminating so the supervisor can restart"
                );
                std::process::exit(3);
            }
        }
    });

    let share_handle = tokio::spawn(async move {
        share_validator.run(event_rx).await;
        error!("Share validator exited unexpectedly — pool is useless without it, terminating");
        std::process::exit(2);
    });

    info!(
        "Pool is running! (mining only — dashboard served by zcash-dashboard)\n\
         \n\
         Stratum: {:?}\n",
        stratum_addrs,
    );

    // Wait for shutdown signal
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl+c");
    info!("Shutdown signal received, stopping services...");

    // Abort the watchdog first, before the validator, so a graceful shutdown
    // can't be misread as a stall and trigger exit(3).
    watchdog_handle.abort();
    for h in &stratum_handles { h.abort(); }
    job_handle.abort();
    share_handle.abort();
    status_handle.abort();

    info!("Pool shut down gracefully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PplnsConfig, RewardMode, PpsPolicy, PpsFundingRoute, validate_pps_config,
        validate_pps_legacy_reserve, validate_pps_funding_route};

    #[test]
    fn testnet_funding_route_is_opt_in_and_cannot_cross_into_mainnet_or_legacy() {
        let route = PpsFundingRoute::ZecdConventionalTestnet { hold_new_legacy_sends: true };
        assert!(validate_pps_funding_route(None, Some(route)).is_err());
        assert_eq!(validate_pps_funding_route(None,None).unwrap(), PpsFundingRoute::ZalletPczt);
        let mut policy = PpsPolicy { network:"testnet".into(), epoch:"testnet-canary".into(),
            fee_bps:100, max_liability_zatoshis:95_000_000_000, total_exposure_zatoshis:100_000_000_000,
            fee_allowance_zatoshis:5_000_000_000, reserve_min_zatoshis:1, max_payout_zatoshis:1_000_000 };
        assert_eq!(validate_pps_funding_route(Some(&policy), Some(route)).unwrap(), route);
        assert_eq!(policy.fee_bps,100); // The route never rewrites the existing pool fee.
        policy.network="mainnet".into();
        assert!(validate_pps_funding_route(Some(&policy),Some(route)).is_err());
        assert_eq!(validate_pps_funding_route(Some(&policy),None).unwrap(),PpsFundingRoute::ZalletPczt);
        policy.network="testnet".into(); policy.max_liability_zatoshis+=1;
        assert!(validate_pps_funding_route(Some(&policy),Some(route)).is_err());
    }

    fn parse_mode(mode: &str) -> Result<PplnsConfig, toml::de::Error> {
        toml::from_str(&format!("window_multiplier = 2.0\nmode = {mode:?}\n"))
    }

    #[test]
    fn omitted_reward_mode_keeps_the_pplns_default() {
        let config: PplnsConfig = toml::from_str("window_multiplier = 2.0\n").unwrap();
        assert_eq!(config.mode, RewardMode::Pplns);
    }

    #[test]
    fn implemented_reward_modes_remain_case_insensitive() {
        for mode in ["pplns", "PPLNS", "PpLnS"] {
            assert_eq!(parse_mode(mode).unwrap().mode, RewardMode::Pplns);
        }
        for mode in ["solo", "SOLO", "SoLo"] {
            assert_eq!(parse_mode(mode).unwrap().mode, RewardMode::Solo);
        }
        for mode in ["pps", "PPS"] {
            assert_eq!(parse_mode(mode).unwrap().mode, RewardMode::Pps);
        }
    }

    #[test]
    fn unsupported_pps_modes_and_typos_fail_during_config_parsing() {
        for mode in ["pps-shadow", "fpps", "ppln", "sol", "", " pplns", "solo "] {
            let error = parse_mode(mode).unwrap_err();
            assert!(error.message().contains("unsupported [pplns].mode"));
        }
    }

    #[test]
    fn non_string_reward_modes_are_rejected() {
        for value in ["1", "true", "[]", "{}"] {
            assert!(toml::from_str::<PplnsConfig>(&format!(
                "window_multiplier = 2.0\nmode = {value}\n",
            )).is_err());
        }
    }

    #[test]
    fn pps_mode_without_explicit_policy_fails_before_startup() {
        assert!(validate_pps_config(RewardMode::Pps, None, "testnet").is_err());
        assert!(validate_pps_config(RewardMode::Pplns, None, "testnet").is_ok());
        let policy = PpsPolicy { network: "testnet".into(), epoch: "canary-1".into(),
            fee_bps: 100, max_liability_zatoshis: 100_000_000, reserve_min_zatoshis: 10_000_000,
            total_exposure_zatoshis: 110_000_000, fee_allowance_zatoshis: 10_000_000,
            max_payout_zatoshis: 20_000_000 };
        assert!(validate_pps_config(RewardMode::Pplns, Some(&policy), "testnet").is_err());
        assert!(validate_pps_config(RewardMode::Pps, Some(&policy), "mainnet").is_err());
        assert!(validate_pps_config(RewardMode::Pps, Some(&policy), "testnet").is_ok());
        assert!(validate_pps_legacy_reserve(&policy,None).is_ok());
        assert!(validate_pps_legacy_reserve(&policy,Some(0.1)).is_ok());
        for bad in [0.10000001, -1.0, f64::NAN, f64::INFINITY] {
            assert!(validate_pps_legacy_reserve(&policy,Some(bad)).is_err());
        }
    }
}
