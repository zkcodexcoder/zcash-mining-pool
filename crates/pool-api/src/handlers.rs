use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, Json};
use node_rpc::ZcashRpcClient;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use pool_db::PoolDb;

pub type AppState = Arc<ApiState>;

/// If no template received in this many ms, the pool is considered stalled.
const STALL_THRESHOLD_MS: i64 = 90_000;

/// Zcash target block interval.
const BLOCK_TIME_SECS: f64 = 75.0;

pub struct ApiState {
    pub db: PoolDb,
    pub rpc: Arc<ZcashRpcClient>,
    pub pool_name: String,
    pub pool_fee: f64,
    pub network: String,
    pub hostname: String,
    pub stratum_port: u16,
    /// All stratum ports with their descriptions.
    pub stratum_ports: Vec<StratumPortInfo>,
    /// Unix timestamp (ms) of last successful getblocktemplate. Used for stall detection.
    pub last_template_at_ms: Option<Arc<std::sync::atomic::AtomicI64>>,
    /// Wallet RPC client (Zallet) for balance/health check. Pool communicates with Zallet only via RPC.
    pub wallet_rpc: Option<Arc<ZcashRpcClient>>,
    /// Pool payout address (unified address for z_sendmany).
    pub pool_address: Option<String>,
    /// Mining address (transparent, for shielding).
    pub mining_address: Option<String>,
    /// Text the pool injects into its coinbase scriptSig. Lets the network
    /// page recognise our blocks when the reward is minted into a shielded
    /// note and no transparent vout pays `mining_address`.
    pub coinbase_tag: Option<String>,
    /// Minimum payout in zatoshis.
    pub min_payout_zatoshis: i64,
    /// Effective immediate-payout flag (faucet mode): when true, displayed
    /// pending payouts skip the maturity gate, matching the payout loop.
    pub pay_immature: bool,
    /// Maturity confirmations required.
    pub maturity_confirmations: u64,
    /// Wake handle for the dashboard payout loop (audit #14): the manual
    /// trigger nudges the one hardened pipeline instead of running its own.
    /// None when payouts are disabled in config.
    pub payout_wake: Option<Arc<tokio::sync::Notify>>,
    /// Difficulty multiplier: converts shares/sec to Sol/s.
    /// Equal to 2^256 / share_target. For target 0800...0000 this is 32.
    pub difficulty_multiplier: f64,
    /// Per-range cache for network block mining stats. Key = range ("1h","24h","1w").
    pub network_blocks_cache: tokio::sync::RwLock<std::collections::HashMap<String, (crate::network::NetworkMiningStats, i64)>>,
    /// In-memory ring buffer of stats snapshots (1 hour @ 10s = 360 entries).
    pub stats_history: StatsHistory,
    /// Accepted share count since startup (atomic, no DB).
    pub shares_accepted: Arc<std::sync::atomic::AtomicU64>,
    /// Rejected share count since startup (atomic, no DB).
    pub shares_rejected: Arc<std::sync::atomic::AtomicU64>,
    /// Optional banner message shown at the top of the public dashboard.
    pub banner: Option<String>,
    /// Reward distribution scheme label for the dashboard UI ("PPLNS" or "Solo").
    pub payout_scheme: String,
    /// Explicit configuration, not inferred from a presentation label or DB row.
    pub pps_enabled: bool,
    /// Background-refreshed snapshot of zebra's /metrics endpoint. The
    /// admin health handler reads this directly instead of triggering a
    /// live scrape — zebra 4.4.x's metrics body grew to ~9 MB with
    /// per-peer cardinality, and the endpoint serves the full body (no
    /// gzip, no Range, no chunked encoding), so a live scrape costs
    /// ~1.5s per call. The background task in `zcash-dashboard` refreshes
    /// this every ~30s, keeping the admin endpoint constant-time.
    pub zebra_metrics_cache: Arc<tokio::sync::RwLock<crate::zebra_metrics::ZebraMetrics>>,
    /// Background-refreshed authoritative chain tip from a fan-out across
    /// several public lwd servers. Lets the admin page surface a
    /// second-opinion tip when zebra's local `sync_estimated_network_tip`
    /// looks wrong (we've seen it report below verified_height in 4.4.1).
    pub authoritative_tip_cache: Arc<tokio::sync::RwLock<crate::lwd_tip::AuthoritativeTip>>,
}

impl ApiState {
    /// Get last_template_at_ms: from live atomic if connected, else from pool_status DB.
    pub async fn get_last_template_ms(&self) -> (bool, Option<String>) {
        // Try live atomic first
        if let Some(ref at) = self.last_template_at_ms {
            let ms = at.load(Ordering::Relaxed);
            if ms > 0 {
                let now_ms = chrono::Utc::now().timestamp_millis();
                let age_ms = now_ms - ms;
                let ok = age_ms < STALL_THRESHOLD_MS;
                let ts = chrono::TimeZone::timestamp_millis_opt(&chrono::Utc, ms)
                    .single()
                    .map(|dt| dt.to_rfc3339());
                return (ok, ts);
            }
        }
        // Fallback: read from pool_status DB table
        if let Ok(Some((val, _updated))) = self.db.get_pool_status("last_template_at_ms").await {
            if let Ok(ms) = val.parse::<i64>() {
                if ms > 0 {
                    let now_ms = chrono::Utc::now().timestamp_millis();
                    let age_ms = now_ms - ms;
                    let ok = age_ms < STALL_THRESHOLD_MS;
                    let ts = chrono::TimeZone::timestamp_millis_opt(&chrono::Utc, ms)
                        .single()
                        .map(|dt| dt.to_rfc3339());
                    return (ok, ts);
                }
            }
        }
        // No data at all — stall tracking not configured
        (true, None)
    }

    /// Get shares accepted/rejected: from live atomics if non-zero, else from pool_status DB.
    pub async fn get_shares_counters(&self) -> (u64, u64) {
        let accepted = self.shares_accepted.load(Ordering::Relaxed);
        let rejected = self.shares_rejected.load(Ordering::Relaxed);
        if accepted > 0 || rejected > 0 {
            return (accepted, rejected);
        }
        // Fallback: read from DB
        let db_accepted = self.db.get_pool_status("shares_accepted").await
            .ok().flatten()
            .and_then(|(v, _)| v.parse::<u64>().ok())
            .unwrap_or(0);
        let db_rejected = self.db.get_pool_status("shares_rejected").await
            .ok().flatten()
            .and_then(|(v, _)| v.parse::<u64>().ok())
            .unwrap_or(0);
        (db_accepted, db_rejected)
    }

    /// Get rate limit counters from pool_status DB.
    pub async fn get_rate_counters(&self) -> (u64, u64) {
        let warn = self.db.get_pool_status("rate_warn_count").await
            .ok().flatten()
            .and_then(|(v, _)| v.parse::<u64>().ok())
            .unwrap_or(0);
        let reject = self.db.get_pool_status("rate_reject_count").await
            .ok().flatten()
            .and_then(|(v, _)| v.parse::<u64>().ok())
            .unwrap_or(0);
        (warn, reject)
    }

    /// Get rejection breakdown from pool_status DB: (low_diff, job_not_found, duplicate, other).
    pub async fn get_rejection_breakdown(&self) -> (u64, u64, u64, u64) {
        let read = |key: &'static str| {
            let db = self.db.clone();
            async move {
                db.get_pool_status(key).await
                    .ok().flatten()
                    .and_then(|(v, _)| v.parse::<u64>().ok())
                    .unwrap_or(0)
            }
        };
        let (ld, jnf, dup, oth) = tokio::join!(
            read("rejects_low_diff"),
            read("rejects_job_not_found"),
            read("rejects_duplicate"),
            read("rejects_other"),
        );
        (ld, jnf, dup, oth)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StratumPortInfo {
    pub port: u16,
    pub description: String,
}

#[derive(Serialize)]
pub struct PoolStats {
    /// Separate sampled credit readiness; missing PPS telemetry is explicitly unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pps_credit_health: Option<pool_core::pps_credit_health::PpsCreditHealth>,
    /// Bounded PPS health only; no other miner's balances are disclosed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pps_status: Option<PpsPublicStatus>,
    pub name: String,
    pub fee_percent: f64,
    /// Minimum payout threshold in ZEC.
    #[serde(rename = "minpay")]
    pub min_payout_zec: f64,
    pub payout_scheme: String,
    pub stratum_url: String,
    pub stratum_port: u16,
    pub stratum_ports: Vec<StratumPortInfo>,
    pub connected_miners: i64,
    pub total_blocks: i64,
    pub immature_blocks: i64,
    pub pending_payout_blocks: i64,
    pub total_shares: i64,
    pub hashrate_estimate: f64,
    /// Short-term (1 min) pool hashrate for responsive display.
    pub hashrate_current: f64,
    pub network_hashrate: f64,
    pub network_height: u64,
    /// Pool luck over the last 24h as a percentage (100 = exactly as expected).
    pub luck_percent: Option<f64>,
    /// Average luck across the last 10 blocks found (100 = expected, lower = luckier).
    pub luck_last_10: Option<f64>,
    /// Average luck across every block the pool has ever found.
    pub luck_lifetime: Option<f64>,
    /// Pool's share of network blocks in the last 24h (percentage).
    pub pool_percent_24h: Option<f64>,
    pub node_ok: bool,
    pub last_template_at: Option<String>,
    pub wallet_ok: bool,
    pub network: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
}

#[derive(Serialize)]
pub struct PpsPublicStatus {
    pub invariant_ok: bool,
    pub chain_lease_current: bool,
    pub last_checked_at: String,
}

#[derive(Serialize)]
pub struct MinerStats {
    pub address: String,
    pub balance: MinerBalance,
    pub workers: Vec<WorkerInfo>,
    /// Isolated PPS earnings for this queried miner only. Legacy balance
    /// fields retain their existing meaning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pps_balance: Option<PpsMinerBalance>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PpsMinerBalance {
    pub pending_zatoshis: i64,
    pub paying_zatoshis: i64,
    pub paid_zatoshis: i64,
    pub fractional_subzatoshis: String,
    pub subzatoshis_per_zatoshi: String,
}

async fn read_pps_miner_balance(db: &PoolDb, miner_id: i64)
    -> Result<Option<PpsMinerBalance>, sqlx::Error>
{
    let row: Option<(i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT pending,paying,paid,fraction FROM pps_accounts WHERE miner_id=?1",
    ).bind(miner_id).fetch_optional(db.inner()).await?;
    Ok(row.map(|(pending, paying, paid, fraction)| PpsMinerBalance {
        pending_zatoshis: pending, paying_zatoshis: paying, paid_zatoshis: paid,
        fractional_subzatoshis: fraction.to_string(),
        subzatoshis_per_zatoshi: pool_db::pps_live::PPS_SCALE.to_string(),
    }))
}

#[cfg(test)]
mod pps_balance_tests {
    use super::*;

    #[tokio::test]
    async fn pps_fields_are_isolated_to_the_queried_miner() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1)
            .connect("sqlite::memory:").await.unwrap();
        let db = PoolDb::new(pool.clone());
        db.run_migrations().await.unwrap();
        for (miner, pending) in [(1_i64, 123_i64), (2, 987)] {
            sqlx::query("INSERT INTO miners(id,address) VALUES(?1,?2)")
                .bind(miner).bind(format!("synthetic-miner-{miner}")).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO pps_accounts(miner_id,pending,paying,paid,fraction) VALUES(?1,?2,4,5,6)")
                .bind(miner).bind(pending).execute(&pool).await.unwrap();
        }
        assert_eq!(read_pps_miner_balance(&db, 1).await.unwrap().unwrap().pending_zatoshis, 123);
        assert_eq!(read_pps_miner_balance(&db, 2).await.unwrap().unwrap().pending_zatoshis, 987);
        assert!(read_pps_miner_balance(&db, 3).await.unwrap().is_none());
        let response = MinerStats {
            address: "synthetic-miner-1".into(), workers: vec![],
            balance: MinerBalance { pending_zatoshis: 10, paid_zatoshis: 20, pending_zec: 0.0000001, paid_zec: 0.0000002 },
            pps_balance: read_pps_miner_balance(&db, 1).await.unwrap(),
        };
        let json = serde_json::to_value(response).unwrap();
        assert_eq!(json["balance"]["pending_zatoshis"], 10);
        assert_eq!(json["pps_balance"]["pending_zatoshis"], 123);
        assert_eq!(json["pps_balance"]["paying_zatoshis"], 4);
        assert_eq!(json["pps_balance"]["fractional_subzatoshis"], "6");
        assert!(!json.to_string().contains("synthetic-miner-2"));
    }
}

#[derive(Serialize)]
pub struct MinerBalance {
    pub pending_zatoshis: i64,
    pub paid_zatoshis: i64,
    pub pending_zec: f64,
    pub paid_zec: f64,
}

#[derive(Serialize)]
pub struct WorkerInfo {
    pub name: String,
    pub last_seen: String,
}

#[derive(Serialize)]
pub struct BlockInfo {
    pub height: i64,
    pub hash: String,
    pub reward_zatoshis: i64,
    pub reward_zec: f64,
    pub status: String,
    pub found_at: String,
    pub luck_percent: Option<f64>,
}

#[derive(Serialize)]
pub struct PayoutInfo {
    pub miner_id: i64,
    pub miner_address: String,
    pub txid: Option<String>,
    pub amount_zatoshis: i64,
    pub amount_zec: f64,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct ImmatureBlock {
    pub height: i64,
    pub reward_zec: f64,
    pub confirmations: i64,
    pub required: u64,
    pub progress_percent: f64,
    pub found_at: String,
}

#[derive(Serialize)]
pub struct MinerListInfo {
    pub address: String,
    pub pending_zec: f64,
    pub share_count: i64,
    pub worker_count: i64,
    pub hashrate: f64,
    pub hashrate_1m: f64,
    pub joined: String,
}

#[derive(Serialize)]
pub struct ApiError {
    pub error: String,
}

/// A point-in-time snapshot of pool stats for the history ring buffer.
#[derive(Clone, Serialize)]
pub struct StatsSnapshot {
    pub timestamp_ms: i64,
    pub pool_hashrate: f64,
    pub pool_hashrate_1m: f64,
    pub network_hashrate: f64,
    #[serde(default)]
    pub network_height: u64,
    pub connected_miners: i64,
    pub total_blocks: i64,
    pub total_shares: i64,
    /// Cached wallet reachability (checked in background, not per-request).
    #[serde(skip)]
    pub wallet_ok: bool,
}

/// Fixed-capacity ring buffer that holds up to 360 snapshots (1 hour @ 10s).
pub struct StatsHistory {
    buffer: tokio::sync::RwLock<VecDeque<StatsSnapshot>>,
}

const STATS_HISTORY_CAPACITY: usize = 360;

impl StatsHistory {
    pub fn new() -> Self {
        Self {
            buffer: tokio::sync::RwLock::new(VecDeque::with_capacity(STATS_HISTORY_CAPACITY)),
        }
    }

    pub async fn push(&self, snapshot: StatsSnapshot) {
        let mut buf = self.buffer.write().await;
        if buf.len() >= STATS_HISTORY_CAPACITY {
            buf.pop_front();
        }
        buf.push_back(snapshot);
    }

    pub async fn get_all(&self) -> Vec<StatsSnapshot> {
        self.buffer.read().await.iter().cloned().collect()
    }

    pub async fn latest(&self) -> Option<StatsSnapshot> {
        self.buffer.read().await.back().cloned()
    }
}

/// Compute a stats snapshot from the DB + RPC. Used by both the polling handler
/// and the background history-recording task.
pub async fn compute_stats_snapshot(state: &ApiState) -> StatsSnapshot {
    let connected = state.db.get_connected_miners_count().await.unwrap_or(0);
    let blocks = state.db.get_blocks_count().await.unwrap_or(0);
    let shares = state.db.get_total_shares_count().await.unwrap_or(0);

    // 10-minute average hashrate
    let since_10m = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::minutes(10))
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();
    let diff_sum_10m = state
        .db
        .get_difficulty_sum_since(&since_10m)
        .await
        .unwrap_or(0.0);
    let hashrate = (diff_sum_10m / 600.0) * state.difficulty_multiplier;

    // 1-minute current hashrate
    let since_1m = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::minutes(1))
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();
    let diff_sum_1m = state
        .db
        .get_difficulty_sum_since(&since_1m)
        .await
        .unwrap_or(0.0);
    let hashrate_current = (diff_sum_1m / 60.0) * state.difficulty_multiplier;

    // Run slow RPC calls concurrently to avoid blocking the snapshot.
    let (network_hashrate, network_height, wallet_ok) = tokio::join!(
        async { state.rpc.get_network_sol_ps(Some(120)).await.unwrap_or(0.0) },
        async { state.rpc.get_block_count().await.unwrap_or(0) },
        check_wallet_rpc(state),
    );

    StatsSnapshot {
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
        pool_hashrate: hashrate,
        pool_hashrate_1m: hashrate_current,
        network_hashrate,
        network_height,
        connected_miners: connected,
        total_blocks: blocks,
        total_shares: shares,
        wallet_ok,
    }
}

const ZATOSHIS_PER_ZEC: f64 = 100_000_000.0;

pub async fn get_pool_stats(
    State(state): State<AppState>,
) -> Result<Json<PoolStats>, StatusCode> {
    // Use cached snapshot (updated every 10s in background) to avoid blocking
    // on slow RPC calls. Fall back to computing fresh only on first request.
    let snap = match state.stats_history.latest().await {
        Some(s) => s,
        None => compute_stats_snapshot(&state).await,
    };

    let immature = state.db.get_immature_blocks_count().await.unwrap_or(0);
    let pending_payout = state.db.get_pending_payouts(state.min_payout_zatoshis, state.pay_immature, 0, i64::MAX).await
        .map(|v| v.len() as i64).unwrap_or(0);

    let (node_ok, last_template_at) = state.get_last_template_ms().await;

    // Query 24h block count once for both luck and network share.
    let since_24h = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::hours(24))
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();
    let actual_blocks_24h = state
        .db
        .get_blocks_count_since(&since_24h)
        .await
        .unwrap_or(0) as f64;

    // Current luck: work done since last block / expected work * 100.
    // 0% = just found a block, 100% = block is "due", >100% = overdue.
    // Lower is better: green ≤100%, yellow ≤150%, red >150%.
    let luck_percent = if snap.network_hashrate > 0.0 {
        let expected_work = snap.network_hashrate * BLOCK_TIME_SECS;
        let recent_blocks = state.db.get_recent_blocks(1).await.unwrap_or_default();
        let since = if let Some(latest) = recent_blocks.first() {
            latest.created_at.clone()
        } else {
            // No blocks found yet — sum all shares from epoch.
            "1970-01-01 00:00:00".to_string()
        };
        let diff_sum = state.db.get_difficulty_sum_since(&since).await.unwrap_or(0.0);
        let actual_work = diff_sum * state.difficulty_multiplier;
        if expected_work > 0.0 {
            Some((actual_work / expected_work) * 100.0)
        } else {
            None
        }
    } else {
        None
    };

    // Network share: pool_blocks_24h / expected_network_blocks_24h * 100.
    let pool_percent_24h = if actual_blocks_24h > 0.0 {
        Some((actual_blocks_24h / 1152.0) * 100.0)
    } else {
        None
    };

    // Historical luck: average of last-10 and lifetime per-block luck_percent values.
    let luck_last_10 = match state.db.get_recent_blocks(10).await {
        Ok(blocks) => {
            let lucks: Vec<f64> = blocks.iter().filter_map(|b| b.luck_percent).collect();
            if lucks.is_empty() {
                None
            } else {
                Some(lucks.iter().sum::<f64>() / lucks.len() as f64)
            }
        }
        Err(_) => None,
    };
    let luck_lifetime = state.db.get_lifetime_luck().await.unwrap_or(None);

    let pps_status = state.db.get_pool_status("pps_health").await.ok().flatten()
        .and_then(|(raw, _)| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| {
            let last = v.get("last_run")?.as_str()?;
            let checked = chrono::DateTime::parse_from_rfc3339(last).ok()?;
            let age = chrono::Utc::now().timestamp() - checked.timestamp();
            let fresh = (0..=90).contains(&age);
            Some(PpsPublicStatus {
                invariant_ok: fresh && v.get("invariant_ok")?.as_bool()?,
                chain_lease_current: fresh && v.get("chain_lease_current")?.as_bool()?
                    && v.get("chain_valid_until_unix")?.as_i64()? > chrono::Utc::now().timestamp(),
                last_checked_at: last.to_string(),
            })
        });
    let pps_credit_health = crate::credit_health::read(&state.db, state.pps_enabled).await;
    Ok(Json(PoolStats {
        pps_credit_health,
        pps_status,
        name: state.pool_name.clone(),
        fee_percent: state.pool_fee,
        payout_scheme: state.payout_scheme.clone(),
        stratum_url: format!("stratum+tcp://{}:{}", state.hostname, state.stratum_port),
        stratum_port: state.stratum_port,
        stratum_ports: state.stratum_ports.clone(),
        min_payout_zec: state.min_payout_zatoshis as f64 / 100_000_000.0,
        connected_miners: snap.connected_miners,
        total_blocks: snap.total_blocks,
        immature_blocks: immature,
        pending_payout_blocks: pending_payout,
        total_shares: snap.total_shares,
        hashrate_estimate: snap.pool_hashrate,
        hashrate_current: snap.pool_hashrate_1m,
        network_hashrate: snap.network_hashrate,
        network_height: snap.network_height,
        luck_percent,
        luck_last_10,
        luck_lifetime,
        pool_percent_24h,
        node_ok,
        last_template_at,
        wallet_ok: snap.wallet_ok,
        network: state.network.clone(),
        banner: state.banner.clone(),
    }))
}

pub async fn get_stats_history(
    State(state): State<AppState>,
) -> Json<Vec<StatsSnapshot>> {
    Json(state.stats_history.get_all().await)
}

/// `/api/stats` endpoint compatible with open-ethereum-pool / 2miners format
/// for miningpoolstats.stream scraping.
pub async fn get_pool_stats_nomp(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let snap = compute_stats_snapshot(&state).await;

    // Network difficulty: try getmininginfo, fall back to estimation from hashrate.
    let network_difficulty = match state.rpc.get_mining_info().await {
        Ok(info) => info
            .get("difficulty")
            .and_then(|v| v.as_f64())
            .filter(|&d| d > 0.0)
            .unwrap_or_else(|| snap.network_hashrate * BLOCK_TIME_SECS / 8192.0),
        Err(_) => {
            snap.network_hashrate * BLOCK_TIME_SECS / 8192.0
        }
    };

    // Network height
    let network_height = state.rpc.get_block_count().await.unwrap_or(0);

    // Workers (individual worker processes, not unique miners)
    let workers = state.db.get_connected_workers_count().await.unwrap_or(0);

    // Block maturity counts
    let immature = state.db.get_immature_blocks_count().await.unwrap_or(0);
    let matured = state.db.get_pending_payout_blocks_count().await.unwrap_or(0);

    // Last block found timestamp (epoch seconds as integer, 2miners convention)
    let last_block_found: i64 = match state.db.get_recent_blocks(1).await {
        Ok(blocks) => blocks.first().and_then(|b| {
            chrono::NaiveDateTime::parse_from_str(&b.created_at, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|dt| dt.and_utc().timestamp())
        }).unwrap_or(0),
        Err(_) => 0,
    };

    // Current round shares (shares since last block found)
    let round_shares: i64 = if last_block_found > 0 {
        let since = chrono::DateTime::from_timestamp(last_block_found, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "1970-01-01 00:00:00".to_string());
        state.db.get_shares_count_since(&since).await.unwrap_or(0.0) as i64
    } else {
        snap.total_shares
    };

    // Luck: average luck across recent blocks (lower = luckier).
    let luck = match state.db.get_recent_blocks(10).await {
        Ok(blocks) => {
            let lucks: Vec<f64> = blocks.iter().filter_map(|b| b.luck_percent).collect();
            if lucks.is_empty() { 0.0 } else { lucks.iter().sum::<f64>() / lucks.len() as f64 }
        }
        Err(_) => 0.0,
    };

    // Min payout in ZEC (not zatoshis)
    let min_payout_zec = state.min_payout_zatoshis as f64 / 100_000_000.0;

    let now = chrono::Utc::now().timestamp_millis();

    Json(serde_json::json!({
        "apiVersion": 200,
        "now": now,
        "hashrate": snap.pool_hashrate,
        "minersTotal": snap.connected_miners,
        "workersTotal": workers,
        "candidatesTotal": 0,
        "immatureTotal": immature,
        "maturedTotal": matured,
        "luck": luck,
        "minPayout": min_payout_zec,
        "minpay": min_payout_zec,
        "fee": state.pool_fee,
        "payoutScheme": state.payout_scheme,
        "paymentMethod": state.payout_scheme,
        "payout_scheme": state.payout_scheme,
        "height": network_height,
        "last_block_found": last_block_found,
        "nodes": [{
            "name": "zcash_pplns",
            "difficulty": format!("{:.4}", network_difficulty),
            "height": format!("{}", network_height),
            "networkhashps": format!("{:.0}", snap.network_hashrate),
            "lastBeat": format!("{}", chrono::Utc::now().timestamp()),
            "avgBlockTime": format!("{:.2}", BLOCK_TIME_SECS)
        }],
        "stats": {
            "lastBlockFound": last_block_found,
            "nShares": snap.total_shares,
            "roundShares": round_shares
        }
    }))
}

async fn check_wallet_rpc(state: &ApiState) -> bool {
    // Audit #16: this used to issue a LIVE z_gettotalbalance (up to 30s) per
    // anonymous request — a free wallet-exhaustion lever pointed at the same
    // RPC the payout pipeline needs. Serve from the payout_health snapshot
    // the dashboard's loop already writes every cycle instead; consider the
    // wallet healthy only if the snapshot is fresh (< 15 min) AND it reported
    // responsive.
    if state.wallet_rpc.is_none() {
        return false;
    }
    match state.db.get_pool_status("payout_health").await {
        Ok(Some((value, _updated_at))) => {
            match serde_json::from_str::<serde_json::Value>(&value) {
                Ok(h) => {
                    let responsive = h
                        .get("wallet_responsive")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let fresh = h
                        .get("checked_at")
                        .and_then(|v| v.as_str())
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|t| chrono::Utc::now().signed_duration_since(t).num_minutes() < 15)
                        .unwrap_or(false);
                    responsive && fresh
                }
                Err(_) => false,
            }
        }
        _ => false,
    }
}

/// Health check: 200 if node is returning templates recently, 503 if stalled.
pub async fn get_health(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    let (ok, _last_template_at) = state.get_last_template_ms().await;
    let reason = if ok { "ok" } else { "node has not returned a block template recently; pool may be stalled" };
    let status = if ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let body = serde_json::json!({
        "status": if ok { "ok" } else { "stalled" },
        "node_ok": ok,
        "reason": reason
    });
    (status, Json(body))
}

const ZATOSHIS_PER_ZEC_F64: f64 = 100_000_000.0;

fn reverse_hex(hex_str: &str) -> String {
    let bytes = hex::decode(hex_str).unwrap_or_default();
    let reversed: Vec<u8> = bytes.into_iter().rev().collect();
    hex::encode(reversed)
}

pub async fn trigger_payout(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    // Audit #14: this endpoint was a full parallel re-implementation of the
    // payout pipeline (own maturity check with the LEGACY proportional orphan
    // reversal, own shielding wait, own z_sendmany with debit-AFTER-send) that
    // bypassed the round-3 reservation saga entirely — a crash mid-loop here
    // re-opened the double-pay class via the admin path. It is now a nudge:
    // wake the real dashboard payout loop, which runs the single hardened
    // pipeline (reserve -> send -> confirm, with reconciliation).
    if state.wallet_rpc.is_none() || state.pool_address.is_none() {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "status": "error", "message": "Payouts not configured on this dashboard"
        })));
    }
    let inflight = state.db.get_inflight_payout_attempt().await.ok().flatten();
    match &state.payout_wake {
        Some(wake) => {
            wake.notify_one();
            (StatusCode::OK, Json(serde_json::json!({
                "status": "ok",
                "message": "payout loop nudged; the hardened pipeline will run within seconds",
                "inflight_attempt": inflight.map(|(id, status, source)| serde_json::json!({
                    "id": id, "status": status, "source": source
                })),
            })))
        }
        None => (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "status": "error",
            "message": "payout loop is not running (payouts disabled in config)"
        }))),
    }
}

pub async fn get_miner_stats(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<MinerStats>, (StatusCode, Json<ApiError>)> {
    let miner = state
        .db
        .get_miner_by_address(&address)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Database error".to_string(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: "Miner not found".to_string(),
                }),
            )
        })?;

    let balance = state
        .db
        .get_or_create_balance(miner.id)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Database error".to_string(),
                }),
            )
        })?;

    let workers = state
        .db
        .get_workers_for_miner(miner.id)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Database error".to_string(),
                }),
            )
        })?;

    let pps_balance = read_pps_miner_balance(&state.db, miner.id).await.map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError { error: "Database error".into() }))
    })?;
    Ok(Json(MinerStats {
        address: miner.address,
        pps_balance,
        balance: MinerBalance {
            pending_zatoshis: balance.pending,
            paid_zatoshis: balance.paid,
            pending_zec: balance.pending as f64 / ZATOSHIS_PER_ZEC,
            paid_zec: balance.paid as f64 / ZATOSHIS_PER_ZEC,
        },
        workers: workers
            .into_iter()
            .map(|w| WorkerInfo {
                name: w.name,
                last_seen: w.last_seen,
            })
            .collect(),
    }))
}

pub async fn get_blocks(
    State(state): State<AppState>,
) -> Result<Json<Vec<BlockInfo>>, StatusCode> {
    let blocks = state
        .db
        .get_recent_blocks(150)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result: Vec<BlockInfo> = blocks
        .iter()
        .map(|b| BlockInfo {
            height: b.height,
            hash: b.hash.clone(),
            reward_zatoshis: b.reward,
            reward_zec: b.reward as f64 / ZATOSHIS_PER_ZEC,
            status: b.status.clone(),
            found_at: b.created_at.clone(),
            luck_percent: b.luck_percent,
        })
        .collect();

    Ok(Json(result))
}

pub async fn get_miners(
    State(state): State<AppState>,
) -> Result<Json<Vec<MinerListInfo>>, StatusCode> {
    let miners = state
        .db
        .get_all_miners_with_stats()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(
        miners
            .into_iter()
            .map(|m| MinerListInfo {
                address: m.address,
                pending_zec: m.pending_balance as f64 / ZATOSHIS_PER_ZEC,
                share_count: m.share_count,
                worker_count: m.worker_count,
                hashrate: (m.recent_diff / 600.0) * state.difficulty_multiplier,
                hashrate_1m: (m.recent_diff_1m / 60.0) * state.difficulty_multiplier,
                joined: m.created_at,
            })
            .collect(),
    ))
}

pub async fn get_payouts(
    State(state): State<AppState>,
) -> Result<Json<Vec<PayoutInfo>>, StatusCode> {
    let payouts = state
        .db
        .get_recent_payouts(50)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut result = Vec::with_capacity(payouts.len());
    for p in payouts {
        let addr = state
            .db
            .get_miner_address(p.miner_id)
            .await
            .unwrap_or_else(|_| format!("miner#{}", p.miner_id));
        result.push(PayoutInfo {
            miner_id: p.miner_id,
            miner_address: addr,
            txid: p.txid,
            amount_zatoshis: p.amount,
            amount_zec: p.amount as f64 / ZATOSHIS_PER_ZEC,
            created_at: p.created_at,
        });
    }
    Ok(Json(result))
}

#[derive(Serialize)]
pub struct PendingPayoutEntry {
    pub miner_address: String,
    pub amount_zatoshis: i64,
    pub amount_zec: f64,
    pub joined_at: String,
}

pub async fn get_pending_payouts_list(
    State(state): State<AppState>,
) -> Result<Json<Vec<PendingPayoutEntry>>, StatusCode> {
    let entries = state
        .db
        .get_pending_payouts(state.min_payout_zatoshis, state.pay_immature, 0, i64::MAX)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result: Vec<PendingPayoutEntry> = entries
        .into_iter()
        .map(|p| PendingPayoutEntry {
            miner_address: p.address,
            amount_zatoshis: p.amount,
            amount_zec: p.amount as f64 / ZATOSHIS_PER_ZEC,
            joined_at: p.created_at,
        })
        .collect();

    Ok(Json(result))
}

// --- Wallet status (RPC only; pool does not manage Zallet process) ---

#[derive(Serialize)]
pub struct ZalletStatus {
    pub rpc_ok: bool,
    pub balance: Option<ZalletBalance>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct ZalletBalance {
    pub transparent: String,
    pub private: String,
    pub total: String,
}

pub async fn get_zallet_status(
    State(state): State<AppState>,
) -> Result<Json<ZalletStatus>, StatusCode> {
    let mut status = ZalletStatus {
        rpc_ok: false,
        balance: None,
        error: None,
    };

    if let Some(ref wallet) = state.wallet_rpc {
        match wallet.wallet_balances(0).await {
            Ok(b) => {
                status.rpc_ok = true;
                status.balance = Some(ZalletBalance {
                    transparent: format!("{:.8}", b.transparent),
                    private: format!("{:.8}", b.spendable),
                    total: format!("{:.8}", b.transparent + b.spendable + b.pending + b.immature),
                });
            }
            Err(e) => status.error = Some(format!("RPC error: {e}")),
        }
    } else {
        status.error = Some("Wallet RPC not configured (payout.wallet_rpc_url)".to_string());
    }

    Ok(Json(status))
}

pub async fn get_immature_blocks(
    State(state): State<AppState>,
) -> Result<Json<Vec<ImmatureBlock>>, StatusCode> {
    let current_height = state
        .rpc
        .get_block_count()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? as i64;

    let pending = state
        .db
        .get_pending_blocks()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let required = state.maturity_confirmations;
    let mut blocks: Vec<ImmatureBlock> = pending
        .into_iter()
        .map(|b| {
            let confirmations = (current_height - b.height).max(0);
            let progress = (confirmations as f64 / required as f64 * 100.0).min(100.0);
            ImmatureBlock {
                height: b.height,
                reward_zec: b.reward as f64 / ZATOSHIS_PER_ZEC,
                confirmations,
                required,
                progress_percent: progress,
                found_at: b.created_at,
            }
        })
        .collect();

    blocks.sort_by(|a, b| b.height.cmp(&a.height));
    Ok(Json(blocks))
}

pub async fn zallet_dashboard() -> Html<String> {
    Html(ZALLET_DASHBOARD_HTML.to_string())
}

const ZALLET_DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Wallet Status - Zcash Mining Pool</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #0a0e17; color: #e0e0e0; min-height: 100vh; }
        .header { background: linear-gradient(135deg, #1a1f2e 0%, #0d1117 100%); border-bottom: 1px solid #f4b728; padding: 1.5rem 2rem; display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 1rem; }
        .header h1 { color: #f4b728; font-size: 1.5rem; }
        .header a { color: #718096; text-decoration: none; font-size: 0.9rem; }
        .header a:hover { color: #f4b728; }
        .container { max-width: 700px; margin: 0 auto; padding: 2rem; }
        .card { background: #1a1f2e; border: 1px solid #2d3748; border-radius: 8px; padding: 1.25rem; margin-bottom: 1.5rem; }
        .card h2 { font-size: 1rem; color: #a0aec0; margin-bottom: 1rem; padding-bottom: 0.5rem; border-bottom: 1px solid #2d3748; }
        .badge { padding: 0.25rem 0.75rem; border-radius: 4px; font-size: 0.8rem; font-weight: 600; }
        .badge-ok { background: #22543d; color: #68d391; }
        .badge-fail { background: #742a2a; color: #fc8181; }
        .balance-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 1rem; }
        .balance-item .label { font-size: 0.7rem; text-transform: uppercase; color: #718096; margin-bottom: 0.25rem; }
        .balance-item .value { font-size: 1.25rem; color: #f4b728; }
        .error { color: #fc8181; margin-top: 0.5rem; }
        .note { font-size: 0.75rem; color: #718096; margin-top: 1rem; }
        .maturity-table { width: 100%; border-collapse: collapse; margin-top: 0.75rem; }
        .maturity-table th { font-size: 0.65rem; text-transform: uppercase; letter-spacing: 0.08em; color: #718096; padding: 0.4rem 0.5rem; text-align: left; border-bottom: 1px solid #2d3748; }
        .maturity-table td { font-family: 'JetBrains Mono', 'Fira Code', 'Courier New', monospace; font-size: 0.8rem; padding: 0.4rem 0.5rem; border-bottom: 1px solid #1a2332; color: #a0aec0; }
        .progress-bar { background: #2d3748; border-radius: 4px; height: 16px; overflow: hidden; min-width: 80px; }
        .progress-fill { height: 100%; background: linear-gradient(90deg, #d69e2e, #f4b728); border-radius: 4px; transition: width 0.5s ease; }
        .empty-msg { color: #4a5568; font-size: 0.8rem; padding: 1rem 0; text-align: center; }
    </style>
</head>
<body style="opacity:0;transition:opacity 0.15s">
    <div class="header">
        <h1>Wallet Status</h1>
        <a href="/">← Pool Dashboard</a>
    </div>
    <div class="container">
        <div class="card">
            <h2>RPC Status</h2>
            <span id="badge-rpc" class="badge badge-fail">–</span>
            <div id="error" class="error"></div>
        </div>
        <div class="card">
            <h2>Balance</h2>
            <div class="balance-grid">
                <div class="balance-item"><div class="label">Transparent</div><div class="value" id="bal-t">–</div></div>
                <div class="balance-item"><div class="label">Private</div><div class="value" id="bal-p">–</div></div>
                <div class="balance-item"><div class="label">Total</div><div class="value" id="bal-total">–</div></div>
            </div>
            <div class="note">Zallet must be run externally. Pool communicates with it via RPC only.</div>
        </div>
        <div class="card">
            <h2>Block Maturity</h2>
            <div id="maturity-content">
                <div class="empty-msg">Loading...</div>
            </div>
        </div>
    </div>
    <script>
        let COIN = 'TAZ';
        async function fetchCoin() {
            try {
                const r = await fetch('/api/pool/info');
                const d = await r.json();
                COIN = d.coin || 'TAZ';
            } catch(e) {}
        }
        async function fetchStatus() {
            try {
                const r = await fetch('/api/zallet/status');
                const d = await r.json();
                const rb = document.getElementById('badge-rpc');
                rb.textContent = d.rpc_ok ? 'Online' : 'Offline';
                rb.className = 'badge ' + (d.rpc_ok ? 'badge-ok' : 'badge-fail');
                if (d.balance) {
                    document.getElementById('bal-t').textContent = d.balance.transparent + ' ' + COIN;
                    document.getElementById('bal-p').textContent = d.balance.private + ' ' + COIN;
                    document.getElementById('bal-total').textContent = d.balance.total + ' ' + COIN;
                } else {
                    document.getElementById('bal-t').textContent = '–';
                    document.getElementById('bal-p').textContent = '–';
                    document.getElementById('bal-total').textContent = '–';
                }
                document.getElementById('error').textContent = d.error || '';
            } catch (e) {
                document.getElementById('badge-rpc').textContent = 'Error';
                document.getElementById('badge-rpc').className = 'badge badge-fail';
                document.getElementById('error').textContent = 'Failed to fetch: ' + e;
            }
        }
        async function fetchImmature() {
            try {
                const r = await fetch('/api/blocks/immature');
                const blocks = await r.json();
                const el = document.getElementById('maturity-content');
                if (!blocks.length) {
                    el.innerHTML = '<div class="empty-msg">No immature blocks</div>';
                    return;
                }
                let html = '<table class="maturity-table"><thead><tr><th>Height</th><th>Reward</th><th>Confirmations</th><th>Progress</th></tr></thead><tbody>';
                for (const b of blocks) {
                    const pct = Math.min(b.progress_percent, 100).toFixed(0);
                    html += '<tr>' +
                        '<td style="color:#e2e8f0">' + b.height + '</td>' +
                        '<td>' + b.reward_zec.toFixed(4) + ' ' + COIN + '</td>' +
                        '<td>' + b.confirmations + ' / ' + b.required + '</td>' +
                        '<td><div class="progress-bar"><div class="progress-fill" style="width:' + pct + '%"></div></div></td>' +
                        '</tr>';
                }
                html += '</tbody></table>';
                el.innerHTML = html;
            } catch (e) {
                document.getElementById('maturity-content').innerHTML = '<div class="empty-msg">Failed to load</div>';
            }
        }
        (() => {
            fetchCoin();
            document.body.style.opacity = '1';
            fetchStatus();
            fetchImmature();
            setInterval(fetchStatus, 5000);
            setInterval(fetchImmature, 5000);
        })();
    </script>
</body>
</html>
"##;
