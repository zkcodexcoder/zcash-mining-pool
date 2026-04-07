use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use pool_db::PoolDb;
use serde::Serialize;
use sha2::{Digest, Sha256};
use stratum::messages::StratumError;
use stratum::server::{StratumEvent, StratumServer};
use stratum::ServerMessage;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use node_rpc::ZcashRpcClient;

use crate::block::BlockAssembler;
use crate::difficulty::{difficulty_to_target_hex, VardiffTracker};
use crate::job::MiningJob;
use rewards::PplnsCalculator;

/// Zcash target block interval.
const BLOCK_TIME_SECS: f64 = 75.0;

const RAW_SOLUTION_SIZE: usize = 1344; // Equihash(200,9)

/// Configuration for variable difficulty.
#[derive(Debug, Clone)]
pub struct VardiffConfig {
    pub initial_difficulty: f64,
    pub target_shares_per_minute: f64,
    pub retarget_interval_secs: f64,
}

/// Shares/sec threshold that triggers an immediate vardiff retarget.
const VARDIFF_TRIGGER_RATE: f64 = 100.0;

/// Shares/sec threshold above which shares are rejected outright.
const MAX_SHARES_PER_SEC: f64 = 500.0;

/// Result of per-share rate check.
enum RateStatus {
    /// Below VARDIFF_TRIGGER_RATE — normal processing.
    Ok,
    /// Between VARDIFF_TRIGGER_RATE and MAX_SHARES_PER_SEC — accept but force retarget.
    Warn,
    /// Above MAX_SHARES_PER_SEC — reject share.
    Reject,
}

/// Max difficulty adjustments to keep per session.
const MAX_DIFF_HISTORY: usize = 50;

/// Per-session state: vardiff tracker + current target + rate limiter.
struct SessionDifficulty {
    vardiff: VardiffTracker,
    target: [u8; 32],
    /// Tracks share submissions for rate limiting.
    rate_window_start: Instant,
    rate_window_shares: u32,
    /// Metadata for live debugging.
    worker_name: String,
    peer_addr: String,
    connected_at: Instant,
    local_port: u16,
    /// Recent difficulty adjustments (newest last).
    diff_history: Vec<DiffAdjustment>,
}

/// A single difficulty adjustment event.
#[derive(Debug, Clone, Serialize)]
pub struct DiffAdjustment {
    pub secs_since_connect: u64,
    pub difficulty: f64,
}

/// Snapshot of a live session for the debugging page.
#[derive(Debug, Clone, Serialize)]
pub struct SessionSnapshot {
    pub session_id: String,
    pub worker_name: String,
    pub peer_addr: String,
    pub local_port: u16,
    pub difficulty: f64,
    pub hashrate: f64,
    pub connected_secs: u64,
    pub shares_per_min: f64,
    pub smoothed_ratio: f64,
    pub retargets: usize,
    pub last_retarget_secs: Option<u64>,
    pub diff_history: Vec<DiffAdjustment>,
}

pub struct ShareValidator {
    db: PoolDb,
    stratum: Arc<StratumServer>,
    jobs: Arc<RwLock<HashMap<String, MiningJob>>>,
    block_assembler: Arc<BlockAssembler>,
    pplns: Arc<PplnsCalculator>,
    rpc: Arc<ZcashRpcClient>,
    difficulty_multiplier: f64,
    /// Fallback target for sessions without a vardiff entry yet.
    default_target: [u8; 32],
    /// Per-session difficulty tracking, keyed by session_id.
    session_difficulty: RwLock<HashMap<String, SessionDifficulty>>,
    vardiff_config: VardiffConfig,
    /// Per-port initial difficulty overrides (port -> difficulty).
    port_difficulty: HashMap<u16, f64>,
    /// Most recent job notify, sent to miners on connect so they have work immediately.
    latest_notify: Arc<RwLock<Option<ServerMessage>>>,
    /// Accepted share counter (shared with API for stats).
    shares_accepted: Arc<std::sync::atomic::AtomicU64>,
    /// Rejected share counter (shared with API for stats).
    shares_rejected: Arc<std::sync::atomic::AtomicU64>,
    /// Rate warn counter (100-500 shares/sec events).
    rate_warn_count: Arc<std::sync::atomic::AtomicU64>,
    /// Rate reject counter (>500 shares/sec events).
    rate_reject_count: Arc<std::sync::atomic::AtomicU64>,
    /// Maps session_id -> worker_id for persisting difficulty on retarget.
    session_worker_id: RwLock<HashMap<String, i64>>,
    /// Tracks recent disconnects for rapid-reconnect difficulty escalation.
    /// Key: worker_name, Value: (disconnect_time, last_difficulty).
    recent_disconnects: RwLock<HashMap<String, (Instant, f64)>>,
}

impl ShareValidator {
    pub fn new(
        db: PoolDb,
        stratum: Arc<StratumServer>,
        jobs: Arc<RwLock<HashMap<String, MiningJob>>>,
        block_assembler: Arc<BlockAssembler>,
        pplns: Arc<PplnsCalculator>,
        pool_target: [u8; 32],
        vardiff_config: VardiffConfig,
        port_difficulty: HashMap<u16, f64>,
        latest_notify: Arc<RwLock<Option<ServerMessage>>>,
        rpc: Arc<ZcashRpcClient>,
        difficulty_multiplier: f64,
        shares_accepted: Arc<std::sync::atomic::AtomicU64>,
        shares_rejected: Arc<std::sync::atomic::AtomicU64>,
        rate_warn_count: Arc<std::sync::atomic::AtomicU64>,
        rate_reject_count: Arc<std::sync::atomic::AtomicU64>,
    ) -> Self {
        Self {
            db,
            stratum,
            jobs,
            block_assembler,
            pplns,
            rpc,
            difficulty_multiplier,
            default_target: pool_target,
            session_difficulty: RwLock::new(HashMap::new()),
            vardiff_config,
            port_difficulty,
            latest_notify,
            shares_accepted,
            shares_rejected,
            rate_warn_count,
            rate_reject_count,
            session_worker_id: RwLock::new(HashMap::new()),
            recent_disconnects: RwLock::new(HashMap::new()),
        }
    }

    fn make_initial_target(&self) -> ([u8; 32], VardiffTracker) {
        self.make_target_for_difficulty(self.vardiff_config.initial_difficulty)
    }

    fn make_target_for_difficulty(&self, difficulty: f64) -> ([u8; 32], VardiffTracker) {
        let tracker = VardiffTracker::new(
            self.vardiff_config.target_shares_per_minute,
            self.vardiff_config.retarget_interval_secs,
            difficulty,
        );
        let target_hex = difficulty_to_target_hex(difficulty);
        let target = parse_target(&target_hex).unwrap_or(self.default_target);
        (target, tracker)
    }

    pub async fn run(&self, mut event_rx: mpsc::Receiver<StratumEvent>) {
        info!("Share validator started (vardiff: initial_diff={}, target_spm={}, retarget={}s)",
            self.vardiff_config.initial_difficulty,
            self.vardiff_config.target_shares_per_minute,
            self.vardiff_config.retarget_interval_secs,
        );

        let mut snapshot_interval = tokio::time::interval(std::time::Duration::from_secs(5));
        snapshot_interval.tick().await; // consume the immediate first tick

        loop {
            let event = tokio::select! {
                ev = event_rx.recv() => match ev {
                    Some(e) => e,
                    None => break,
                },
                _ = snapshot_interval.tick() => {
                    self.write_session_snapshots().await;
                    continue;
                }
            };
            match event {
                StratumEvent::ShareSubmitted {
                    session_id,
                    request_id,
                    worker_name,
                    job_id,
                    time,
                    nonce_1,
                    nonce_2,
                    equihash_solution,
                } => {
                    // 3-tier rate check before expensive validation
                    let rate_status = self.check_rate(&session_id).await;
                    if matches!(rate_status, RateStatus::Reject) {
                        self.shares_rejected.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        self.rate_reject_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        warn!(worker = %worker_name, "Share rate >500/s, rejecting");
                        self.stratum
                            .send_to_session(&session_id, ServerMessage::SubmitResult {
                                id: request_id, accepted: false,
                                error: Some(StratumError::other("Share rate limit exceeded")),
                            })
                            .await;
                        self.force_retarget_session(&session_id).await;
                        continue;
                    }
                    let force_retarget = matches!(rate_status, RateStatus::Warn);
                    if force_retarget {
                        self.rate_warn_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }

                    let result = self
                        .validate_share(
                            &session_id, &worker_name, &job_id, &time,
                            &nonce_1, &nonce_2, &equihash_solution,
                        )
                        .await;

                    match result {
                        Ok(share_result) => {
                            if share_result.is_block {
                                info!(
                                    worker = %worker_name,
                                    job = %job_id,
                                    height = share_result.block_height.unwrap_or(0),
                                    "BLOCK FOUND!"
                                );
                            } else {
                                debug!(worker = %worker_name, job = %job_id, "Share accepted");
                            }
                            self.shares_accepted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            self.stratum
                                .send_to_session(&session_id, ServerMessage::SubmitResult {
                                    id: request_id, accepted: true, error: None,
                                })
                                .await;

                            // Check vardiff retarget after accepted share
                            if force_retarget {
                                self.force_retarget_session(&session_id).await;
                            } else {
                                self.maybe_retarget(&session_id).await;
                            }
                        }
                        Err(e) => {
                            self.shares_rejected.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            warn!(worker = %worker_name, error = %e, "Share rejected");
                            self.stratum
                                .send_to_session(&session_id, ServerMessage::SubmitResult {
                                    id: request_id, accepted: false, error: Some(e),
                                })
                                .await;
                            // Retarget on rejected shares too — if difficulty is too
                            // high, all shares get rejected and without this, vardiff
                            // never gets called to bring it back down.
                            self.maybe_retarget(&session_id).await;
                        }
                    }
                }
                StratumEvent::WorkerConnected { session_id, worker_name, password, addr, local_port } => {
                    info!(%worker_name, %addr, port = local_port, "Worker connected");
                    let miner_address = worker_name.split('.').next().unwrap_or(&worker_name);
                    let wname = worker_name.split('.').nth(1).unwrap_or("default");

                    // Register worker and look up last known difficulty
                    let db_difficulty = match self.register_worker_and_get_difficulty(miner_address, wname).await {
                        Ok((worker_id, last_diff)) => {
                            // Track session -> worker_id for persisting difficulty on retarget
                            self.session_worker_id.write().await.insert(session_id.clone(), worker_id);
                            last_diff
                        }
                        Err(e) => {
                            error!(error = %e, "Failed to register worker");
                            None
                        }
                    };

                    // Check for rapid reconnect — if this worker disconnected recently
                    // with a short session, escalate difficulty to prevent connect/disconnect loops.
                    let reconnect_diff = {
                        let mut disconnects = self.recent_disconnects.write().await;
                        if let Some((dc_time, last_diff)) = disconnects.remove(&worker_name) {
                            if dc_time.elapsed().as_secs() < 120 {
                                // Double the last difficulty to discourage connect/disconnect loops
                                let escalated = last_diff * 2.0;
                                info!(%worker_name, last_diff, escalated, "Rapid reconnect detected, escalating difficulty");
                                Some(escalated)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };

                    // Priority: password-requested > rapid-reconnect > DB last_difficulty > per-port > default
                    let requested_diff = parse_difficulty_from_password(&password);
                    let initial_diff = requested_diff
                        .or(reconnect_diff)
                        .or(db_difficulty)
                        .or_else(|| self.port_difficulty.get(&local_port).copied());
                    let (target, tracker) = if let Some(diff) = initial_diff {
                        let source = if requested_diff.is_some() { "password" }
                            else if reconnect_diff.is_some() { "reconnect-escalation" }
                            else if db_difficulty.is_some() { "restored" }
                            else { "port" };
                        info!(%session_id, difficulty = diff, port = local_port, source, "Using initial difficulty");
                        self.make_target_for_difficulty(diff)
                    } else {
                        self.make_initial_target()
                    };
                    let target_hex = hex::encode(target);
                    {
                        let mut sessions = self.session_difficulty.write().await;
                        sessions.insert(session_id.clone(), SessionDifficulty {
                            vardiff: tracker,
                            target,
                            rate_window_start: Instant::now(),
                            rate_window_shares: 0,
                            worker_name: worker_name.clone(),
                            peer_addr: addr.to_string(),
                            connected_at: Instant::now(),
                            local_port,
                            diff_history: Vec::new(),
                        });
                    }
                    let difficulty = initial_diff.unwrap_or(self.vardiff_config.initial_difficulty);
                    // Seed diff_history with initial difficulty
                    {
                        let mut sessions = self.session_difficulty.write().await;
                        if let Some(sd) = sessions.get_mut(&session_id) {
                            sd.diff_history.push(DiffAdjustment {
                                secs_since_connect: 0,
                                difficulty,
                            });
                        }
                    }
                    info!(%session_id, target = %target_hex, difficulty, "Sending initial target");
                    self.stratum
                        .send_to_session(&session_id, ServerMessage::SetDifficulty {
                            difficulty,
                        })
                        .await;
                    self.stratum
                        .send_to_session(&session_id, ServerMessage::SetTarget {
                            target: target_hex,
                        })
                        .await;

                    // Send the current job so the miner can start working immediately
                    let latest = self.latest_notify.read().await;
                    if let Some(ref notify) = *latest {
                        self.stratum.send_to_session(&session_id, notify.clone()).await;
                    }
                }
                StratumEvent::SessionDisconnected { session_id } => {
                    debug!(%session_id, "Session disconnected");
                    // Record disconnect for rapid-reconnect detection
                    if let Some(sd) = self.session_difficulty.read().await.get(&session_id) {
                        let diff = sd.vardiff.current_difficulty();
                        let connected_secs = sd.connected_at.elapsed().as_secs();
                        let worker = sd.worker_name.clone();
                        // Only track if session was short (< 30s) — likely a failed start
                        if connected_secs < 30 {
                            self.recent_disconnects.write().await.insert(worker, (Instant::now(), diff));
                        }
                    }
                    self.session_difficulty.write().await.remove(&session_id);
                    self.session_worker_id.write().await.remove(&session_id);
                }
                StratumEvent::TargetSuggested { session_id, target } => {
                    debug!(%session_id, %target, "Target suggestion received (ignored, using vardiff)");
                }
            }
        }
    }

    /// Check the session's share submission rate.
    /// Resets the counter every second.
    async fn check_rate(&self, session_id: &str) -> RateStatus {
        let mut sessions = self.session_difficulty.write().await;
        if let Some(sd) = sessions.get_mut(session_id) {
            let elapsed = sd.rate_window_start.elapsed().as_secs_f64();
            if elapsed >= 1.0 {
                // Reset window
                sd.rate_window_start = Instant::now();
                sd.rate_window_shares = 1;
                RateStatus::Ok
            } else {
                sd.rate_window_shares += 1;
                let rate = sd.rate_window_shares as f64 / elapsed.max(0.01);
                if rate > MAX_SHARES_PER_SEC {
                    RateStatus::Reject
                } else if rate > VARDIFF_TRIGGER_RATE {
                    RateStatus::Warn
                } else {
                    RateStatus::Ok
                }
            }
        } else {
            RateStatus::Ok
        }
    }

    /// Check if a session needs retargeting after an accepted share.
    async fn maybe_retarget(&self, session_id: &str) {
        let new_diff = {
            let mut sessions = self.session_difficulty.write().await;
            if let Some(sd) = sessions.get_mut(session_id) {
                sd.vardiff.record_share()
            } else {
                None
            }
        };
        self.apply_retarget(session_id, new_diff).await;
    }

    /// Force an immediate retarget, bypassing the vardiff interval gate.
    async fn force_retarget_session(&self, session_id: &str) {
        let new_diff = {
            let mut sessions = self.session_difficulty.write().await;
            if let Some(sd) = sessions.get_mut(session_id) {
                sd.vardiff.force_retarget()
            } else {
                None
            }
        };
        self.apply_retarget(session_id, new_diff).await;
    }

    /// Apply a retarget if a new difficulty was computed.
    async fn apply_retarget(&self, session_id: &str, new_diff: Option<f64>) {
        if let Some(diff) = new_diff {
            let target_hex = difficulty_to_target_hex(diff);
            let new_target = parse_target(&target_hex).unwrap_or(self.default_target);
            {
                let mut sessions = self.session_difficulty.write().await;
                if let Some(sd) = sessions.get_mut(session_id) {
                    sd.target = new_target;
                    let entry = DiffAdjustment {
                        secs_since_connect: sd.connected_at.elapsed().as_secs(),
                        difficulty: diff,
                    };
                    sd.diff_history.push(entry);
                    if sd.diff_history.len() > MAX_DIFF_HISTORY {
                        sd.diff_history.remove(0);
                    }
                }
            }
            info!(%session_id, difficulty = diff, target = %target_hex, "Vardiff retarget");
            // Persist difficulty to DB for restoration on reconnect
            if let Some(&worker_id) = self.session_worker_id.read().await.get(session_id) {
                if let Err(e) = self.db.update_worker_difficulty(worker_id, diff).await {
                    warn!(error = %e, "Failed to persist worker difficulty");
                }
            }
            self.stratum
                .send_to_session(session_id, ServerMessage::SetDifficulty {
                    difficulty: diff,
                })
                .await;
            self.stratum
                .send_to_session(session_id, ServerMessage::SetTarget {
                    target: target_hex,
                })
                .await;
        }
    }

    /// Build snapshots of all active sessions for the live debugging page.
    async fn build_session_snapshots(&self) -> Vec<SessionSnapshot> {
        let sessions = self.session_difficulty.read().await;
        sessions.iter().map(|(sid, sd)| {
            let elapsed = sd.vardiff.window_elapsed_secs().max(0.01);
            // Estimate hashrate from difficulty and target share rate.
            // At the correct difficulty, miner produces target_spm shares/min.
            let hashrate = sd.vardiff.current_difficulty()
                * (self.vardiff_config.target_shares_per_minute / 60.0)
                * self.difficulty_multiplier;
            SessionSnapshot {
                session_id: sid.clone(),
                worker_name: sd.worker_name.clone(),
                peer_addr: sd.peer_addr.clone(),
                local_port: sd.local_port,
                difficulty: sd.vardiff.current_difficulty(),
                hashrate,
                connected_secs: sd.connected_at.elapsed().as_secs(),
                shares_per_min: (sd.vardiff.shares_in_window() as f64 / elapsed) * 60.0,
                smoothed_ratio: sd.vardiff.smoothed_ratio(),
                retargets: sd.diff_history.len().saturating_sub(1), // exclude initial
                last_retarget_secs: if sd.diff_history.len() > 1 {
                    let last = sd.diff_history.last().unwrap();
                    Some(sd.connected_at.elapsed().as_secs() - last.secs_since_connect)
                } else {
                    None
                },
                diff_history: sd.diff_history.clone(),
            }
        }).collect()
    }

    /// Write session snapshots to pool_status for the dashboard to read.
    async fn write_session_snapshots(&self) {
        let snapshots = self.build_session_snapshots().await;
        match serde_json::to_string(&snapshots) {
            Ok(json) => {
                let _ = self.db.set_pool_status("sessions_snapshot", &json).await;
            }
            Err(e) => {
                warn!(error = %e, "Failed to serialize session snapshots");
            }
        }
    }

    /// Compute luck for the block being found right now.
    /// Returns `Some(luck_percent)` or `None` if data is unavailable.
    async fn compute_block_luck(&self) -> Result<Option<f64>, String> {
        let network_hashrate = self.rpc.get_network_sol_ps(Some(120)).await
            .map_err(|e| format!("RPC error: {e}"))?;
        if network_hashrate <= 0.0 {
            return Ok(None);
        }
        let expected_work = network_hashrate * BLOCK_TIME_SECS;

        // Get the most recent block's created_at as the start of the window
        let recent_blocks = self.db.get_recent_blocks(1).await
            .map_err(|e| format!("DB error: {e}"))?;
        let since = if let Some(latest) = recent_blocks.first() {
            latest.created_at.clone()
        } else {
            "1970-01-01 00:00:00".to_string()
        };

        let diff_sum = self.db.get_difficulty_sum_since(&since).await
            .map_err(|e| format!("DB error: {e}"))?;
        let actual_work = diff_sum * self.difficulty_multiplier;

        if actual_work > 0.0 {
            Ok(Some((actual_work / expected_work) * 100.0))
        } else {
            Ok(None)
        }
    }

    /// Register worker in DB and return (worker_id, last_difficulty).
    async fn register_worker_and_get_difficulty(
        &self,
        address: &str,
        worker: &str,
    ) -> Result<(i64, Option<f64>), pool_db::DbError> {
        let miner = self.db.get_or_create_miner(address).await?;
        let w = self.db.get_or_create_worker(miner.id, worker).await?;
        Ok((w.id, w.last_difficulty))
    }

    async fn validate_share(
        &self,
        session_id: &str,
        worker_name: &str,
        job_id: &str,
        time: &str,
        nonce_1: &str,
        nonce_2: &str,
        equihash_solution: &str,
    ) -> Result<ShareResult, StratumError> {
        let job = {
            let jobs = self.jobs.read().await;
            jobs.get(job_id).cloned()
        };
        let job = job.ok_or(StratumError::job_not_found())?;

        // Reconstruct the full nonce (32 bytes = nonce_1 + nonce_2)
        let nonce_hex = format!("{}{}", nonce_1, nonce_2);
        let nonce = hex::decode(&nonce_hex)
            .map_err(|_| StratumError::other("Invalid nonce hex"))?;
        if nonce.len() != 32 {
            return Err(StratumError::other("Nonce must be 32 bytes"));
        }

        // Decode the equihash solution bytes from the miner.
        let solution_bytes = hex::decode(equihash_solution)
            .map_err(|_| StratumError::other("Invalid solution hex"))?;

        // Equihash(200,9) raw solution is 1344 bytes.
        // ZIP 301 says miners send WITH compactSize prefix (1347 bytes),
        // but some miners send WITHOUT (1344 bytes). Handle both.
        let (raw_solution, solution_for_header) = if solution_bytes.len() == RAW_SOLUTION_SIZE {
            let mut with_prefix = compact_size(RAW_SOLUTION_SIZE);
            with_prefix.extend_from_slice(&solution_bytes);
            (solution_bytes.clone(), with_prefix)
        } else if solution_bytes.len() > RAW_SOLUTION_SIZE {
            let raw = strip_compact_size(&solution_bytes)?;
            if raw.len() != RAW_SOLUTION_SIZE {
                return Err(StratumError::other(&format!(
                    "Bad solution size: {} (expected {})", raw.len(), RAW_SOLUTION_SIZE
                )));
            }
            (raw.to_vec(), solution_bytes.clone())
        } else {
            return Err(StratumError::other(&format!(
                "Solution too short: {} bytes", solution_bytes.len()
            )));
        };

        // Build the block header input (version + prevhash + merkleroot + reserved + time + bits = 108 bytes)
        let header_input = build_header_input(&job, time)?;

        debug!(
            header_len = header_input.len(),
            raw_soln_len = raw_solution.len(),
            soln_with_prefix_len = solution_for_header.len(),
            job_id = %job_id,
            "Verifying Equihash"
        );

        // Verify the Equihash solution (n=200, k=9 for Zcash)
        equihash::is_valid_solution(200, 9, &header_input, &nonce, &raw_solution)
            .map_err(|e| StratumError::other(&format!("Invalid Equihash solution: {e}")))?;

        // Build the full serialized header for hashing AND block submission.
        // Block header = header_input(108) + nonce(32) + solution_with_compactSize(1347)
        let mut full_header = header_input.clone();
        full_header.extend_from_slice(&nonce);
        full_header.extend_from_slice(&solution_for_header);

        // Compute SHA-256d of the header (Zcash/Bitcoin block hash)
        let hash_bytes = sha256d(&full_header);

        // Read the session's pool difficulty target and current difficulty.
        let (pool_target, difficulty) = {
            let sessions = self.session_difficulty.read().await;
            match sessions.get(session_id) {
                Some(sd) => (sd.target, sd.vardiff.current_difficulty()),
                None => (self.default_target, 1.0),
            }
        };

        // Check against the session's POOL target — reject shares that don't
        // meet the assigned difficulty. Without this check, high-hashrate
        // miners flood the pool with low-difficulty shares.
        if !meets_target(&hash_bytes, &pool_target) {
            return Err(StratumError::low_difficulty());
        }

        // Check against the NETWORK target from the block template
        let network_target = parse_target(&job.template.target)
            .map_err(|e| StratumError::other(&format!("Bad network target: {e}")))?;
        let is_block = meets_target(&hash_bytes, &network_target);

        // Parse worker name (format: "address.worker")
        let miner_address = worker_name.split('.').next().unwrap_or(worker_name);
        let wname = worker_name.split('.').nth(1).unwrap_or("default");

        let miner = self.db.get_or_create_miner(miner_address).await
            .map_err(|e| StratumError::other(&format!("DB error: {e}")))?;
        let worker = self.db.get_or_create_worker(miner.id, wname).await
            .map_err(|e| StratumError::other(&format!("DB error: {e}")))?;

        self.db.record_share(worker.id, job_id, difficulty, is_block, session_id).await
            .map_err(|e| StratumError::other(&format!("DB error: {e}")))?;

        let mut block_height = None;

        if is_block {
            info!(
                network_target = %job.template.target,
                hash = %hex::encode(hash_bytes),
                height = job.template.height,
                "Share meets network difficulty!"
            );

            // Assemble full block: serialized header + transactions
            match assemble_full_block(&full_header, &job.template) {
                Ok(full_block) => {
                    let block_hex = hex::encode(&full_block);
                    // Write block hex to file for debugging
                    let _ = std::fs::write("last_block.hex", &block_hex);
                    match self.block_assembler.submit_block(&block_hex).await {
                        Ok(_) => {
                            let height = job.template.height as i64;
                            let reward = compute_block_reward(height);
                            let hash_hex = hex::encode(hash_bytes);

                            // Compute luck at discovery time
                            let luck_percent = match self.compute_block_luck().await {
                                Ok(luck) => luck,
                                Err(e) => {
                                    warn!(error = %e, "Failed to compute block luck");
                                    None
                                }
                            };

                            match self.db.record_block(height, &hash_hex, reward, worker.id, luck_percent).await {
                                Ok(block_id) => {
                                    info!(height, reward, block_id, "Distributing PPLNS rewards");
                                    if let Err(e) = self.pplns.distribute(reward, block_id).await {
                                        error!(error = %e, "PPLNS distribution failed");
                                    }
                                }
                                Err(e) => {
                                    error!(error = %e, "Failed to record block");
                                }
                            }
                            block_height = Some(height);
                        }
                        Err(e) => {
                            error!(error = %e, "Block submission failed");
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "Block assembly failed");
                }
            }
        }

        Ok(ShareResult { is_block, block_height })
    }
}

pub struct ShareResult {
    pub is_block: bool,
    pub block_height: Option<i64>,
}

fn sha256d(data: &[u8]) -> [u8; 32] {
    let first = Sha256::digest(data);
    let second = Sha256::digest(first);
    let mut out = [0u8; 32];
    out.copy_from_slice(&second);
    out
}

/// Strip the compactSize prefix from a byte slice, returning the payload.
fn strip_compact_size(data: &[u8]) -> Result<&[u8], StratumError> {
    if data.is_empty() {
        return Err(StratumError::other("Empty solution"));
    }
    let (prefix_len, size) = match data[0] {
        0..=252 => (1, data[0] as usize),
        0xFD => {
            if data.len() < 3 { return Err(StratumError::other("Truncated compactSize")); }
            (3, u16::from_le_bytes([data[1], data[2]]) as usize)
        }
        0xFE => {
            if data.len() < 5 { return Err(StratumError::other("Truncated compactSize")); }
            (5, u32::from_le_bytes([data[1], data[2], data[3], data[4]]) as usize)
        }
        0xFF => {
            if data.len() < 9 { return Err(StratumError::other("Truncated compactSize")); }
            (9, u64::from_le_bytes(data[1..9].try_into().unwrap()) as usize)
        }
    };
    if data.len() < prefix_len + size {
        return Err(StratumError::other("Solution shorter than declared size"));
    }
    Ok(&data[prefix_len..prefix_len + size])
}

fn build_header_input(job: &MiningJob, time_hex: &str) -> Result<Vec<u8>, StratumError> {
    let mut input = Vec::with_capacity(108);

    let version = hex::decode(&job.version_hex)
        .map_err(|_| StratumError::other("Invalid version hex"))?;
    input.extend_from_slice(&version);

    let prev_hash = hex::decode(&job.prev_hash_hex)
        .map_err(|_| StratumError::other("Invalid prev_hash hex"))?;
    input.extend_from_slice(&prev_hash);

    let merkle_root = hex::decode(&job.merkle_root_hex)
        .map_err(|_| StratumError::other("Invalid merkle_root hex"))?;
    input.extend_from_slice(&merkle_root);

    let reserved = hex::decode(&job.reserved_hex)
        .map_err(|_| StratumError::other("Invalid reserved hex"))?;
    input.extend_from_slice(&reserved);

    let time = hex::decode(time_hex)
        .map_err(|_| StratumError::other("Invalid time hex"))?;
    input.extend_from_slice(&time);

    let bits = hex::decode(&job.bits_hex)
        .map_err(|_| StratumError::other("Invalid bits hex"))?;
    input.extend_from_slice(&bits);

    Ok(input)
}

/// Check if hash <= target for PoW validity.
/// SHA-256d output is interpreted as a little-endian 256-bit integer
/// (byte[31] is MSB, byte[0] is LSB). The target from getblocktemplate
/// is a big-endian hex string. We compare the reversed hash against
/// the target, both as big-endian.
fn meets_target(hash: &[u8; 32], target: &[u8; 32]) -> bool {
    for i in 0..32 {
        let h = hash[31 - i];
        if h < target[i] {
            return true;
        } else if h > target[i] {
            return false;
        }
    }
    true
}

fn target_to_difficulty(target: &[u8; 32]) -> f64 {
    let pow_limit: f64 = 2.0f64.powi(251) - 1.0;
    let mut val: f64 = 0.0;
    for (i, &byte) in target.iter().enumerate() {
        val += (byte as f64) * 256.0f64.powi((31 - i) as i32);
    }
    if val == 0.0 { return f64::MAX; }
    pow_limit / val
}

/// Compute the miner portion of the block reward in zatoshis.
/// Post-Blossom halving interval is 1,046,400 blocks. Total subsidy starts at 12.5 ZEC
/// and halves each interval. The miner receives 80% after NU6.
fn compute_block_reward(height: i64) -> i64 {
    let halving_interval: i64 = 1_046_400;
    let initial_subsidy: i64 = 1_250_000_000; // 12.5 ZEC total subsidy
    if height < 0 { return 0; }
    let halvings = height / halving_interval;
    if halvings >= 64 { return 0; }
    let total_subsidy = initial_subsidy >> halvings;
    // Miner receives 80% of the subsidy (post-NU6)
    total_subsidy * 80 / 100
}

fn compact_size(n: usize) -> Vec<u8> {
    let n = n as u64;
    if n < 253 {
        vec![n as u8]
    } else if n <= 0xFFFF {
        let mut v = vec![0xFD];
        v.extend_from_slice(&(n as u16).to_le_bytes());
        v
    } else if n <= 0xFFFFFFFF {
        let mut v = vec![0xFE];
        v.extend_from_slice(&(n as u32).to_le_bytes());
        v
    } else {
        let mut v = vec![0xFF];
        v.extend_from_slice(&n.to_le_bytes());
        v
    }
}

/// Assemble a full block for submitblock: serialized_header + tx_count + transactions.
fn assemble_full_block(
    serialized_header: &[u8],
    template: &node_rpc::types::BlockTemplate,
) -> Result<Vec<u8>, StratumError> {
    let coinbase = template.coinbasetxn.as_ref()
        .ok_or_else(|| StratumError::other("Block template missing coinbase transaction"))?;

    let tx_count = 1 + template.transactions.len();
    let mut block = serialized_header.to_vec();
    block.extend_from_slice(&compact_size(tx_count));

    let coinbase_bytes = hex::decode(&coinbase.data)
        .map_err(|_| StratumError::other("Invalid coinbase hex"))?;
    block.extend_from_slice(&coinbase_bytes);

    for tx in &template.transactions {
        let tx_bytes = hex::decode(&tx.data)
            .map_err(|_| StratumError::other("Invalid transaction hex"))?;
        block.extend_from_slice(&tx_bytes);
    }

    Ok(block)
}

/// Parse difficulty from the miner's password field.
/// Supports formats: "d=128", "sd=128", "d=128,other_option"
fn parse_difficulty_from_password(password: &str) -> Option<f64> {
    for part in password.split(',') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix("d=").or_else(|| part.strip_prefix("sd=")) {
            if let Ok(d) = val.parse::<f64>() {
                if d > 0.0 {
                    return Some(d);
                }
            }
        }
    }
    None
}

pub fn parse_target(target_hex: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(target_hex).map_err(|e| format!("Invalid target hex: {e}"))?;
    if bytes.len() > 32 {
        return Err(format!("Target too long: {} bytes", bytes.len()));
    }
    let mut padded = [0u8; 32];
    let offset = 32 - bytes.len();
    padded[offset..].copy_from_slice(&bytes);
    Ok(padded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256d() {
        let hash = sha256d(b"hello");
        assert_eq!(hash.len(), 32);
        let expected = "9595c9df90075148eb06860365df33584b75bff782a510c6cd4883a419833d50";
        assert_eq!(hex::encode(hash), expected);
    }

    #[test]
    fn test_meets_target_easy() {
        let hash = [0u8; 32];
        let target = [0xff; 32];
        assert!(meets_target(&hash, &target));
    }

    #[test]
    fn test_strip_compact_size_1344() {
        let mut data = vec![0xFD, 0x40, 0x05]; // compactSize(1344)
        data.extend_from_slice(&[0xCC; 1344]);
        let raw = strip_compact_size(&data).unwrap();
        assert_eq!(raw.len(), 1344);
    }

    #[test]
    fn test_block_reward() {
        // At height 1: total 12.5 ZEC, miner gets 80% = 10 ZEC
        assert_eq!(compute_block_reward(1), 1_000_000_000);
        // After 1st halving: total 6.25 ZEC, miner gets 5 ZEC
        assert_eq!(compute_block_reward(1_046_400), 500_000_000);
        // After 3rd halving (testnet current): total 1.5625, miner gets 1.25 ZEC
        assert_eq!(compute_block_reward(3_853_089), 125_000_000);
    }

    #[test]
    fn test_parse_target() {
        let t = parse_target("0007ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap();
        assert_eq!(t[0], 0x00);
        assert_eq!(t[1], 0x07);
        assert_eq!(t[31], 0xff);
    }
}
