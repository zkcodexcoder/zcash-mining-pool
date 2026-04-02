use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, Json};
use chrono::TimeZone;
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
    /// Minimum payout in zatoshis.
    pub min_payout_zatoshis: i64,
    /// Maturity confirmations required.
    pub maturity_confirmations: u64,
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
}

#[derive(Debug, Clone, Serialize)]
pub struct StratumPortInfo {
    pub port: u16,
    pub description: String,
}

#[derive(Serialize)]
pub struct PoolStats {
    pub name: String,
    pub fee_percent: f64,
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
    /// Pool luck over the last 24h as a percentage (100 = exactly as expected).
    pub luck_percent: Option<f64>,
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
pub struct MinerStats {
    pub address: String,
    pub balance: MinerBalance,
    pub workers: Vec<WorkerInfo>,
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
    let (network_hashrate, wallet_ok) = tokio::join!(
        async { state.rpc.get_network_sol_ps(Some(120)).await.unwrap_or(0.0) },
        check_wallet_rpc(state),
    );

    StatsSnapshot {
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
        pool_hashrate: hashrate,
        pool_hashrate_1m: hashrate_current,
        network_hashrate,
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
    let pending_payout = state.db.get_pending_payout_blocks_count().await.unwrap_or(0);

    let (node_ok, last_template_at) = match &state.last_template_at_ms {
        None => (true, None),
        Some(at) => {
            let ms = at.load(Ordering::Relaxed);
            let now_ms = chrono::Utc::now().timestamp_millis();
            let age_ms = now_ms - ms;
            let ok = ms > 0 && age_ms < STALL_THRESHOLD_MS;
            let ts = if ms > 0 {
                chrono::Utc.timestamp_millis_opt(ms)
                    .single()
                    .map(|dt| dt.to_rfc3339())
            } else {
                None
            };
            (ok, ts)
        }
    };

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

    Ok(Json(PoolStats {
        name: state.pool_name.clone(),
        fee_percent: state.pool_fee,
        stratum_url: format!("stratum+tcp://{}:{}", state.hostname, state.stratum_port),
        stratum_port: state.stratum_port,
        stratum_ports: state.stratum_ports.clone(),
        connected_miners: snap.connected_miners,
        total_blocks: snap.total_blocks,
        immature_blocks: immature,
        pending_payout_blocks: pending_payout,
        total_shares: snap.total_shares,
        hashrate_estimate: snap.pool_hashrate,
        hashrate_current: snap.pool_hashrate_1m,
        network_hashrate: snap.network_hashrate,
        luck_percent,
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

/// NOMP-compatible `/api/stats` endpoint for miningpoolstats.stream scraping.
pub async fn get_pool_stats_nomp(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let snap = compute_stats_snapshot(&state).await;

    // Network difficulty: try getmininginfo, fall back to estimation from hashrate.
    let network_difficulty = match state.rpc.get_mining_info().await {
        Ok(info) => info
            .get("difficulty")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        Err(_) => {
            // Equihash approximation: difficulty ≈ hashrate * block_time / 2^13
            snap.network_hashrate * BLOCK_TIME_SECS / 8192.0
        }
    };

    // Network height
    let network_height = state.rpc.get_block_count().await.unwrap_or(0) as i64;

    // Workers (individual worker processes, not unique miners)
    let workers = state.db.get_connected_workers_count().await.unwrap_or(0);

    // Last block found timestamp (epoch ms as string, NOMP convention)
    let last_block_found = match state.db.get_recent_blocks(1).await {
        Ok(blocks) => blocks.first().and_then(|b| {
            chrono::NaiveDateTime::parse_from_str(&b.created_at, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|dt| {
                    dt.and_utc().timestamp_millis().to_string()
                })
        }),
        Err(_) => None,
    };
    let last_block_str = last_block_found.unwrap_or_default();

    Json(serde_json::json!({
        "config": {
            "ports": state.stratum_ports.iter().map(|p| serde_json::json!({
                "port": p.port,
                "description": p.description,
                "tls": false,
            })).collect::<Vec<_>>(),
            "fee": state.pool_fee,
            "minPaymentThreshold": state.min_payout_zatoshis,
            "paymentScheme": "PPLNS"
        },
        "network": {
            "height": network_height,
            "difficulty": network_difficulty,
            "hashrate": snap.network_hashrate
        },
        "pool": {
            "hashrate": snap.pool_hashrate,
            "miners": snap.connected_miners,
            "workers": workers,
            "totalBlocks": snap.total_blocks,
            "lastBlockFound": last_block_str,
            "stats": {
                "lastBlockFound": last_block_str
            }
        }
    }))
}

async fn check_wallet_rpc(state: &ApiState) -> bool {
    match &state.wallet_rpc {
        Some(rpc) => {
            // Use z_gettotalbalance as a lightweight health check -- it's wallet-specific
            // and confirms Zallet is running and responsive. Timeout after 30s —
            // z_gettotalbalance can take 15s+ when Zallet is scanning.
            let fut = rpc.call_raw::<serde_json::Value>(
                "z_gettotalbalance", serde_json::json!([0, true])
            );
            tokio::time::timeout(std::time::Duration::from_secs(30), fut)
                .await
                .map(|r| r.is_ok())
                .unwrap_or(false)
        }
        None => false,
    }
}

/// Health check: 200 if node is returning templates recently, 503 if stalled.
pub async fn get_health(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    let (ok, reason) = match &state.last_template_at_ms {
        None => (true, "stall tracking not configured"),
        Some(at) => {
            let ms = at.load(Ordering::Relaxed);
            let now_ms = chrono::Utc::now().timestamp_millis();
            let age_ms = now_ms - ms;
            if ms == 0 {
                (false, "no template received yet")
            } else if age_ms >= STALL_THRESHOLD_MS {
                (false, "node has not returned a block template recently; pool may be stalled")
            } else {
                (true, "ok")
            }
        }
    };
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
    let wallet_rpc = match &state.wallet_rpc {
        Some(rpc) => rpc,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "status": "error", "message": "Wallet RPC not configured"
        }))),
    };
    let pool_address = match &state.pool_address {
        Some(a) => a.clone(),
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "status": "error", "message": "Pool address not configured"
        }))),
    };

    // Phase 1: Check block maturity
    let current_height = match state.rpc.get_block_count().await {
        Ok(h) => h,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "status": "error", "message": format!("getblockcount failed: {e}")
        }))),
    };

    let mut confirmed = 0i64;
    let mut orphaned = 0i64;
    if let Ok(pending_blocks) = state.db.get_pending_blocks().await {
        for block in &pending_blocks {
            let confs = current_height as i64 - block.height;
            if confs < state.maturity_confirmations as i64 { continue; }
            match state.rpc.get_block_hash(block.height as u64).await {
                Ok(chain_hash) => {
                    let pool_hash_reversed = reverse_hex(&block.hash);
                    if chain_hash == pool_hash_reversed || chain_hash == block.hash {
                        let _ = state.db.update_block_status(block.id, "confirmed").await;
                        confirmed += 1;
                    } else {
                        let _ = state.db.update_block_status(block.id, "orphaned").await;
                        let _ = state.db.reverse_block_credits(block.reward).await;
                        orphaned += 1;
                    }
                }
                Err(_) => {}
            }
        }
    }

    // Phase 2: Shield coinbase (if mining_address differs from pool_address)
    let mut shielded = false;
    if let Some(ref mining_addr) = state.mining_address {
        if *mining_addr != pool_address {
            match wallet_rpc.z_shield_coinbase(mining_addr, &pool_address, None).await {
                Ok(result) => {
                    let utxos = result.get("shieldingUTXOs").and_then(|v| v.as_u64()).unwrap_or(0);
                    if utxos > 0 {
                        let opid = result.get("opid").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                        tracing::info!(opid = %opid, utxos = utxos, "Shielding triggered, waiting for 3 confirmations");
                        shielded = true;

                        // Wait for the shielding operation to complete
                        let shield_ok = loop {
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            match wallet_rpc.z_get_operation_status(&[&opid]).await {
                                Ok(statuses) => {
                                    if let Some(s) = statuses.first() {
                                        match s.get("status").and_then(|v| v.as_str()).unwrap_or("") {
                                            "success" => break true,
                                            "failed" => {
                                                tracing::error!("Shielding operation failed");
                                                break false;
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                Err(_) => break false,
                            }
                        };

                        // Wait for 3 confirmations (~30-45s on testnet)
                        if shield_ok {
                            tracing::info!("Shielding complete, waiting for 3 confirmations");
                            for _ in 0..60 {
                                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                                match wallet_rpc.call_raw::<serde_json::Value>(
                                    "z_gettotalbalance", serde_json::json!([3, true])
                                ).await {
                                    Ok(bal) => {
                                        let private = bal.get("private")
                                            .and_then(|v| v.as_str())
                                            .and_then(|s| s.parse::<f64>().ok())
                                            .unwrap_or(0.0);
                                        if private > 0.0 {
                                            tracing::info!(balance = private, "Shielded funds confirmed (3+ confs)");
                                            break;
                                        }
                                    }
                                    Err(_) => break,
                                }
                            }
                        }
                    }
                }
                Err(_) => {}
            }
        }
    }

    // Phase 3: Process payouts
    let pending = match state.db.get_pending_payouts(state.min_payout_zatoshis).await {
        Ok(p) => p,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "status": "error", "message": format!("DB error: {e}")
        }))),
    };

    if pending.is_empty() {
        return (StatusCode::OK, Json(serde_json::json!({
            "status": "ok",
            "message": "No miners eligible for payout",
            "blocks_confirmed": confirmed,
            "blocks_orphaned": orphaned,
            "shielding_triggered": shielded,
            "payouts": 0
        })));
    }

    let amounts: Vec<(&str, f64)> = pending
        .iter()
        .map(|p| (p.address.as_str(), p.amount as f64 / ZATOSHIS_PER_ZEC_F64))
        .collect();

    let opid = match wallet_rpc.z_sendmany(&pool_address, &amounts).await {
        Ok(id) => id,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "status": "error",
            "message": format!("z_sendmany failed: {e}"),
            "blocks_confirmed": confirmed,
            "blocks_orphaned": orphaned,
        }))),
    };

    (StatusCode::OK, Json(serde_json::json!({
        "status": "ok",
        "message": "Payout submitted",
        "opid": opid,
        "miners": pending.len(),
        "total_zec": pending.iter().map(|p| p.amount).sum::<i64>() as f64 / ZATOSHIS_PER_ZEC_F64,
        "blocks_confirmed": confirmed,
        "blocks_orphaned": orphaned,
        "shielding_triggered": shielded,
    })))
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

    Ok(Json(MinerStats {
        address: miner.address,
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
        match wallet.z_get_total_balance().await {
            Ok(v) => {
                status.rpc_ok = true;
                if let Some(obj) = v.as_object() {
                    status.balance = Some(ZalletBalance {
                        transparent: obj.get("transparent").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
                        private: obj.get("private").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
                        total: obj.get("total").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
                    });
                }
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
