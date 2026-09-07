use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
use pool_db::pps_live::{PpsChainLease, PpsCredit, PpsEpoch};
use pool_db::pps_funding::PpsFundingLease;
use rewards::pps::{quote_standard_pps, PpsNetwork, PpsQuoteInput};

/// Bounded PPS startup policy plus proof obtained by the automatic verifier.
#[derive(Clone)]
pub struct PpsRuntime {
    pub epoch: PpsEpoch,
    pub chain_lease: PpsChainLease,
    pub funding_lease: PpsFundingLease,
    pub wallet_rpc: Arc<ZcashRpcClient>,
    pub payout_source: String,
    pub funding_route: crate::pps_funding::PpsFundingRoute,
}

struct ActivePps {
    epoch: PpsEpoch,
    funding_route: crate::pps_funding::PpsFundingRoute,
    lease: Arc<RwLock<Option<PpsChainLease>>>,
    funding: Arc<RwLock<Option<PpsFundingLease>>>,
    health: crate::pps_credit_health::SharedCreditHealth,
}

const PPS_FUNDING_REFRESH_INTERVAL: Duration = Duration::from_secs(120);
/// Cadence after a failed collection: fast enough that a lapsed lease is
/// re-armed within a minute, slow enough not to saturate the wallet RPC
/// (a collection itself can take ~40 s against zecd).
const PPS_FUNDING_RETRY_INTERVAL: Duration = Duration::from_secs(30);
/// Evidence-read failures that say nothing about solvency or integrity.
/// The categories match pps_credit_health's fixed vocabulary.
fn pps_transient_refresh_category(category: &str) -> bool {
    matches!(category, "rpc_unavailable" | "wallet_not_ready" | "concurrent_change" | "deadline_exceeded")
}

fn begin_pps_funding_refresh(
    route: &crate::pps_funding::PpsFundingRoute,
    cached: &mut Option<PpsFundingLease>,
) {
    // Only the explicit testnet route retains its previous proof while a
    // single replacement is pending. Its original expiry and generation still
    // gate every DB credit; a pending read creates no new authorization.
    if !route.holds_new_legacy_sends() {
        *cached = None;
    }
}

fn finish_pps_funding_refresh(
    route: &crate::pps_funding::PpsFundingRoute,
    cached: &mut Option<PpsFundingLease>,
    result: Result<PpsFundingLease, crate::pps_funding::PpsFundingError>,
    elapsed: Duration,
    category: &str,
) -> Duration {
    match result {
        Ok(lease) => {
            *cached = Some(lease);
            if route.holds_new_legacy_sends() {
                PPS_FUNDING_REFRESH_INTERVAL.saturating_sub(elapsed)
            } else {
                // The existing PCZT route keeps its original
                // completion-to-start cadence on all results.
                PPS_FUNDING_REFRESH_INTERVAL
            }
        }
        Err(_) => {
            // On the testnet route, a transient read failure keeps the
            // previous proof: its own valid_until, generation and spendable
            // still gate every DB credit, so retention authorizes nothing
            // new. Any substantive rejection (insolvency, accounting, signer,
            // chain) still revokes immediately — and the PCZT route already
            // cleared its cache in begin_pps_funding_refresh.
            if !(route.holds_new_legacy_sends() && pps_transient_refresh_category(category)) {
                *cached = None;
            }
            PPS_FUNDING_RETRY_INTERVAL
        }
    }
}

/// At most two short retries for explicitly transient testnet evidence failures.
/// The cache is already revoked by finish_pps_funding_refresh on every failure.
fn pps_refresh_retry_delay(route:&crate::pps_funding::PpsFundingRoute,
    category:&str, consecutive_failures:&mut u8, ordinary:Duration) -> Duration {
    if category=="ok" { *consecutive_failures=0; return ordinary; }
    *consecutive_failures=consecutive_failures.saturating_add(1);
    if route.holds_new_legacy_sends() && *consecutive_failures<=2
        && pps_transient_refresh_category(category)
    { Duration::from_secs(5) } else { ordinary }
}

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

/// A previous target we still accept shares for (grace window). When the
/// pool raises difficulty, in-flight shares from the miner were generated
/// against the older, easier target; rejecting them as low_diff wastes the
/// miner's work. Instead we credit them at the older difficulty.
#[derive(Clone)]
struct GraceTarget {
    target: [u8; 32],
    difficulty: f64,
    set_at: Instant,
}

/// Max number of previous targets to remember. Sized to cover a full
/// vardiff up-ramp (~4 early-trigger steps over ~48s with the cooldown)
/// so in-flight shares straddling any step still get credited.
const GRACE_TARGET_HISTORY: usize = 8;

/// Per-session bounded history of share fingerprints, for duplicate/replay
/// rejection. A replayed valid share would otherwise be credited again.
const SHARE_DEDUP_HISTORY: usize = 1024;
/// Max age of a grace target before we stop accepting shares for it.
/// Any miner still submitting at a target older than this is malfunctioning.
const GRACE_TARGET_MAX_AGE: Duration = Duration::from_secs(60);

/// Audit #15: how often a cached worker's last_seen is bumped. Between
/// touches, accepted shares cost ONE insert instead of the old five-statement
/// resolve+touch round-trip.
const WORKER_TOUCH_INTERVAL: Duration = Duration::from_secs(30);

/// Per-session state: vardiff tracker + current target + rate limiter.
struct SessionDifficulty {
    vardiff: VardiffTracker,
    target: [u8; 32],
    /// Recently-replaced targets that we still accept shares for. Newest
    /// last. Capped at GRACE_TARGET_HISTORY entries.
    recent_targets: Vec<GraceTarget>,
    /// Tracks share submissions for rate limiting.
    rate_window_start: Instant,
    rate_window_shares: u32,
    /// Consecutive low-difficulty rejection count for this session.
    low_diff_streak: u32,
    /// Metadata for live debugging.
    worker_name: String,
    peer_addr: String,
    connected_at: Instant,
    local_port: u16,
    /// Recent difficulty adjustments (newest last).
    diff_history: Vec<DiffAdjustment>,
    /// Per-session share counters for live visibility.
    shares_accepted: u64,
    shares_rejected_low_diff: u64,
    shares_rejected_job_not_found: u64,
    shares_rejected_other: u64,
    /// Bounded fingerprints of recently-seen shares (job+nonce+solution) for
    /// duplicate/replay rejection. Oldest evicted first.
    recent_share_fps: std::collections::VecDeque<u64>,
}

/// A single difficulty adjustment event.
#[derive(Debug, Clone, Serialize)]
pub struct DiffAdjustment {
    pub secs_since_connect: u64,
    pub difficulty: f64,
    #[serde(default)]
    pub reason: String,
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
    /// Per-session share counts since this session connected.
    #[serde(default)]
    pub shares_accepted: u64,
    #[serde(default)]
    pub shares_rejected_low_diff: u64,
    #[serde(default)]
    pub shares_rejected_job_not_found: u64,
    #[serde(default)]
    pub shares_rejected_other: u64,
}

pub struct ShareValidator {
    pps: Option<ActivePps>,
    pps_refresh_task: Option<tokio::task::JoinHandle<()>>,
    pps_subsidies: RwLock<HashMap<u64, (u64, Instant)>>,
    db: PoolDb,
    stratum: Arc<StratumServer>,
    jobs: Arc<RwLock<HashMap<String, std::sync::Arc<MiningJob>>>>,
    block_assembler: Arc<BlockAssembler>,
    pplns: Arc<PplnsCalculator>,
    rpc: Arc<ZcashRpcClient>,
    difficulty_multiplier: f64,
    /// Fallback target for sessions without a vardiff entry yet.
    default_target: [u8; 32],
    /// Per-session difficulty tracking, keyed by session_id.
    session_difficulty: RwLock<HashMap<String, SessionDifficulty>>,
    /// Audit #15: (session_id, worker_name) -> (miner_id, worker_id, last
    /// last_seen touch). Avoids 5 DB statements per accepted share.
    worker_cache: RwLock<HashMap<(String, String), (i64, i64, std::time::Instant)>>,
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
    /// Rejections broken down by reason (code 23).
    rejects_low_diff: Arc<std::sync::atomic::AtomicU64>,
    /// Rejections broken down by reason (code 21).
    rejects_job_not_found: Arc<std::sync::atomic::AtomicU64>,
    /// Rejections broken down by reason (code 22).
    rejects_duplicate: Arc<std::sync::atomic::AtomicU64>,
    /// Rejections broken down by reason (code 20 / other — bad solution, nonce, etc).
    rejects_other: Arc<std::sync::atomic::AtomicU64>,
    /// Maps session_id -> worker_id for persisting difficulty on retarget.
    session_worker_id: RwLock<HashMap<String, i64>>,
    /// Tracks recent disconnects for rapid-reconnect difficulty escalation.
    /// Key: worker_name, Value: (disconnect_time, last_difficulty).
    recent_disconnects: RwLock<HashMap<String, (Instant, f64)>>,
    /// Liveness heartbeat: unix-ms timestamp stamped at the top of every
    /// `run()` loop iteration. The loop cycles at least every 5s (the snapshot
    /// tick) even with zero miners, so a healthy validator keeps this fresh.
    /// If an event handler deadlocks, the loop never comes back around and this
    /// freezes — the external watchdog detects the stall and terminates the
    /// process. Catches the *stalled* wedge that the return-only watchdog in
    /// pool-server misses.
    heartbeat: Arc<std::sync::atomic::AtomicI64>,
}

impl ShareValidator {
    pub fn new(
        db: PoolDb,
        stratum: Arc<StratumServer>,
        jobs: Arc<RwLock<HashMap<String, std::sync::Arc<MiningJob>>>>,
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
        rejects_low_diff: Arc<std::sync::atomic::AtomicU64>,
        rejects_job_not_found: Arc<std::sync::atomic::AtomicU64>,
        rejects_duplicate: Arc<std::sync::atomic::AtomicU64>,
        rejects_other: Arc<std::sync::atomic::AtomicU64>,
    ) -> Self {
        Self {
            pps: None,
            pps_refresh_task: None,
            pps_subsidies: RwLock::new(HashMap::new()),
            db,
            stratum,
            jobs,
            block_assembler,
            pplns,
            rpc,
            difficulty_multiplier,
            default_target: pool_target,
            session_difficulty: RwLock::new(HashMap::new()),
            worker_cache: RwLock::new(HashMap::new()),
            vardiff_config,
            port_difficulty,
            latest_notify,
            shares_accepted,
            shares_rejected,
            rate_warn_count,
            rate_reject_count,
            rejects_low_diff,
            rejects_job_not_found,
            rejects_duplicate,
            rejects_other,
            session_worker_id: RwLock::new(HashMap::new()),
            recent_disconnects: RwLock::new(HashMap::new()),
            heartbeat: Arc::new(std::sync::atomic::AtomicI64::new(
                chrono::Utc::now().timestamp_millis(),
            )),
        }
    }

    /// Returns a handle to the validator's liveness heartbeat (unix-ms,
    /// updated each `run()` loop iteration). The pool-server watchdog polls
    /// this to detect a stalled validator and terminate the process.
    pub fn heartbeat(&self) -> Arc<std::sync::atomic::AtomicI64> {
        Arc::clone(&self.heartbeat)
    }

    pub fn with_pps(mut self, runtime: PpsRuntime) -> Result<Self, String> {
        if self.stratum.fixed_share_target().is_none() {
            return Err("PPS requires one immutable announced target".into());
        }
        if runtime.epoch.network.parse::<PpsNetwork>().is_err()
            || runtime.epoch.network != runtime.chain_lease.network
            || runtime.epoch.fee_bps >= 10_000
            || runtime.payout_source.is_empty() || runtime.payout_source.len() > 2048
        {
            return Err("invalid PPS runtime policy".into());
        }
        let network = runtime.epoch.network.parse::<PpsNetwork>()
            .map_err(|_| "invalid PPS network".to_string())?;
        runtime.funding_route.validate(&runtime.epoch)
            .map_err(|_| "invalid PPS funding route policy".to_string())?;
        crate::pps_funding::validate_funding_lease(&runtime.funding_lease,
            &runtime.epoch, chrono::Utc::now().timestamp())
            .map_err(|_| "invalid PPS startup funding evidence".to_string())?;
        let lease = Arc::new(RwLock::new(Some(runtime.chain_lease)));
        let funding = Arc::new(RwLock::new(Some(runtime.funding_lease)));
        let health=Arc::new(std::sync::Mutex::new(crate::pps_credit_health::CreditHealthTracker::default()));
        let refresh_health=Arc::clone(&health);
        let refresh_latest=Arc::clone(&self.latest_notify);
        let refresh_lease = Arc::clone(&lease);
        let refresh_funding = Arc::clone(&funding);
        let rpc = Arc::clone(&self.rpc);
        let wallet = runtime.wallet_rpc;
        let db = self.db.clone();
        let epoch = runtime.epoch.clone();
        let payout_source = runtime.payout_source;
        let funding_route = runtime.funding_route;
        self.pps_refresh_task = Some(tokio::spawn(async move {
            // Independent loops: a slow public chain reference must not delay
            // refreshing the shorter wallet lease. Aborting this one parent
            // task drops all futures; no orphan verifier/publisher tasks are spawned.
            tokio::join!(
                async {
                    loop {
                        tokio::time::sleep(Duration::from_secs(30)).await;
                        let started=Instant::now();
                        match crate::pps_chain::verify_pps_chain(&rpc, network).await {
                            Ok(proof) => { *refresh_lease.write().await = Some(proof); }
                            Err(error) => {
                                use crate::pps_chain::PpsChainError as E;
                                // Affirmative disagreement (a fork, a wrong
                                // network) revokes the previous proof at once.
                                // A reference that is merely slow, down or
                                // lagging keeps it: the proof's own expiry
                                // still bounds it at credit time, and a
                                // flaky explorer must not halt all shares.
                                let revoke = matches!(error,
                                    E::InvalidEvidence|E::NetworkMismatch|E::BranchMismatch
                                    |E::TipMismatch|E::HashMismatch);
                                let category=match error {
                                    E::Timeout=>"deadline_exceeded",E::Unavailable=>"rpc_unavailable",
                                    E::InvalidEvidence=>"invalid_evidence",E::NetworkMismatch=>"network_mismatch",
                                    E::NotSynchronized=>"not_synchronized",E::BranchMismatch=>"branch_mismatch",
                                    E::TipMismatch=>"tip_mismatch",E::HashMismatch=>"hash_mismatch",
                                    E::StaleEvidence=>"stale_evidence",
                                };
                                let duration_ms=u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                                if revoke {
                                    warn!(stage="chain_refresh",category,duration_ms,"PPS chain evidence rejected; new credits paused");
                                    *refresh_lease.write().await = None;
                                } else {
                                    warn!(stage="chain_refresh",category,duration_ms,"PPS chain evidence unavailable; retaining prior proof until expiry");
                                }
                            }
                        };
                    }
                },
                async {
                    // A slow testnet collection can consume most of the original
                    // 60-second lease. Start its replacement immediately, then
                    // use a single-flight start-to-start cadence. PCZT retains
                    // the original initial delay and clear-during-read behavior.
                    let mut delay = if funding_route.holds_new_legacy_sends() {
                        Duration::ZERO
                    } else { PPS_FUNDING_REFRESH_INTERVAL };
                    let mut consecutive_failures=0;
                    loop {
                        // Sleep out the cadence, but wake early when a payout
                        // bumps the funding generation past the cached lease:
                        // admission then recovers after one collection instead
                        // of one full cadence period.
                        let wait_started = Instant::now();
                        loop {
                            let remaining = delay.saturating_sub(wait_started.elapsed());
                            if remaining.is_zero() { break; }
                            tokio::time::sleep(remaining.min(Duration::from_secs(5))).await;
                            let cached_generation =
                                refresh_funding.read().await.as_ref().map(|l| l.generation);
                            if let Some(generation) = cached_generation {
                                if matches!(db.pps_funding_generation().await,
                                    Ok(current) if u64::try_from(current).ok() != Some(generation))
                                { break; }
                            }
                        }
                        begin_pps_funding_refresh(&funding_route, &mut *refresh_funding.write().await);
                        if let Ok(mut h)=refresh_health.lock() { h.begin(chrono::Utc::now().timestamp()); }
                        let started = Instant::now();
                        let (result,stage,category) = crate::pps_credit_health::observe_refresh(
                            crate::pps_funding::collect_pps_credit_funding_for_route(
                            &db, &wallet, &epoch, &payout_source, &rpc, &funding_route,
                        )).await;
                        let elapsed = started.elapsed();
                        let duration_ms=u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
                        if result.is_err() {
                            warn!(stage,category,duration_ms,"PPS credit funding refresh failed");
                        } else {
                            info!(stage,category,duration_ms,"PPS credit funding refresh completed");
                        }
                        delay = finish_pps_funding_refresh(&funding_route,
                            &mut *refresh_funding.write().await, result, elapsed, category);
                        if let Ok(mut h)=refresh_health.lock() { h.finish(chrono::Utc::now().timestamp(),stage,category); }
                        delay=pps_refresh_retry_delay(&funding_route,category,&mut consecutive_failures,delay);
                    }
                },
                crate::pps_credit_health::publish_loop(&db,&epoch,&funding_route,
                    &refresh_lease,&refresh_funding,&refresh_health,&refresh_latest),
            );
        }));
        self.pps = Some(ActivePps { epoch: runtime.epoch, funding_route: runtime.funding_route, lease, funding, health });
        Ok(self)
    }

    fn make_initial_target(&self) -> ([u8; 32], VardiffTracker) {
        self.make_target_for_difficulty(self.vardiff_config.initial_difficulty)
    }

    fn make_target_for_difficulty(&self, difficulty: f64) -> ([u8; 32], VardiffTracker) {
        if let Some(fixed) = self.stratum.fixed_share_target() {
            return (fixed.target_be(), VardiffTracker::new(
                self.vardiff_config.target_shares_per_minute,
                self.vardiff_config.retarget_interval_secs,
                fixed.display_difficulty(),
            ));
        }
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
            // Liveness heartbeat (audit: stalled-validator watchdog). Stamped
            // once per iteration; the loop cycles at least every 5s via the
            // snapshot tick even with no share traffic, so this stays fresh
            // while healthy and freezes if a handler below deadlocks.
            self.heartbeat.store(
                chrono::Utc::now().timestamp_millis(),
                std::sync::atomic::Ordering::Relaxed,
            );
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
                        self.rejects_other.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        {
                            let mut sessions = self.session_difficulty.write().await;
                            if let Some(sd) = sessions.get_mut(&session_id) {
                                sd.shares_rejected_other = sd.shares_rejected_other.saturating_add(1);
                            }
                        }
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
                            // Reset low-diff streak and bump per-session accepted count
                            {
                                let mut sessions = self.session_difficulty.write().await;
                                if let Some(sd) = sessions.get_mut(&session_id) {
                                    sd.low_diff_streak = 0;
                                    sd.shares_accepted = sd.shares_accepted.saturating_add(1);
                                }
                            }
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
                            let code = e.code;
                            match code {
                                21 => { self.rejects_job_not_found.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
                                22 => { self.rejects_duplicate.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
                                23 => { self.rejects_low_diff.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
                                _ => { self.rejects_other.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
                            }
                            // Bump per-session counters for live visibility,
                            // and decide whether this session is a bad-share
                            // flooder we should kick. A miner with zero accepts
                            // and 50+ rejects after 5 s connected is almost
                            // certainly running the wrong algorithm or broken
                            // hardware — vardiff alone can't defend (it just
                            // ramps), and at high rates this storm can wedge
                            // the single validator task.
                            // A PPS admission pause is the pool refusing valid
                            // work, not the miner misbehaving: those rejects
                            // must not feed the flooder kick, or every pause
                            // turns into a pool-wide disconnect/reconnect storm.
                            let pool_side_pause = e.message.starts_with("PPS admission paused");
                            let should_disconnect = !pool_side_pause && {
                                let mut sessions = self.session_difficulty.write().await;
                                if let Some(sd) = sessions.get_mut(&session_id) {
                                    match code {
                                        21 => sd.shares_rejected_job_not_found = sd.shares_rejected_job_not_found.saturating_add(1),
                                        23 => sd.shares_rejected_low_diff = sd.shares_rejected_low_diff.saturating_add(1),
                                        _ => sd.shares_rejected_other = sd.shares_rejected_other.saturating_add(1),
                                    }
                                    let total_rejects = sd.shares_rejected_low_diff
                                        + sd.shares_rejected_job_not_found
                                        + sd.shares_rejected_other;
                                    sd.shares_accepted == 0
                                        && total_rejects >= 50
                                        && sd.connected_at.elapsed() >= std::time::Duration::from_secs(5)
                                } else {
                                    false
                                }
                            };
                            if should_disconnect {
                                warn!(
                                    %session_id,
                                    worker = %worker_name,
                                    "Disconnecting bad-share flooder (no accepts, 50+ rejects in 5s+)"
                                );
                                self.stratum.disconnect_session(&session_id).await;
                                continue;
                            }
                            let is_low_diff = e.is_low_difficulty();
                            warn!(worker = %worker_name, error = %e, "Share rejected");
                            self.stratum
                                .send_to_session(&session_id, ServerMessage::SubmitResult {
                                    id: request_id, accepted: false, error: Some(e),
                                })
                                .await;
                            // If difficulty is too high, miner can only submit
                            // low-difficulty shares. Reset to minimum so vardiff
                            // can ramp up from scratch.
                            //
                            // Safety: only reset if the miner has NEVER submitted
                            // a valid share. A responsive miner that has any
                            // accepts doesn't need this reset — vardiff will
                            // self-correct. Without this guard, very fast GPUs
                            // (5090-class) thrash: rate-limit ramps diff up, a
                            // burst of in-flight shares at old diff fails as
                            // low_diff, hits 10 streak, resets to diff 1, repeat.
                            if is_low_diff && self.stratum.fixed_share_target().is_none() {
                                let mut sessions = self.session_difficulty.write().await;
                                if let Some(sd) = sessions.get_mut(&session_id) {
                                    sd.low_diff_streak += 1;
                                    let never_accepted = sd.shares_accepted == 0;
                                    if sd.low_diff_streak >= 10 && never_accepted {
                                        let new_diff = self.vardiff_config.initial_difficulty;
                                        let streak = sd.low_diff_streak;
                                        info!(%session_id, old_diff = sd.vardiff.current_difficulty(),
                                            new_diff, streak,
                                            "Resetting difficulty after low-diff rejection streak");
                                        sd.low_diff_streak = 0;
                                        drop(sessions);
                                        let (target, tracker) = self.make_target_for_difficulty(new_diff);
                                        let target_hex = hex::encode(target);
                                        {
                                            let mut sessions = self.session_difficulty.write().await;
                                            if let Some(sd) = sessions.get_mut(&session_id) {
                                                push_grace_target(sd);
                                                sd.vardiff = tracker;
                                                sd.target = target;
                                                sd.diff_history.push(DiffAdjustment {
                                                    secs_since_connect: sd.connected_at.elapsed().as_secs(),
                                                    difficulty: new_diff,
                                                    reason: format!("reset: {streak} consecutive low-diff rejects"),
                                                });
                                                if sd.diff_history.len() > MAX_DIFF_HISTORY {
                                                    sd.diff_history.remove(0);
                                                }
                                            }
                                        }
                                        self.stratum
                                            .send_to_session(&session_id, ServerMessage::SetDifficulty {
                                                difficulty: new_diff,
                                            }).await;
                                        self.stratum
                                            .send_to_session(&session_id, ServerMessage::SetTarget {
                                                target: target_hex,
                                            }).await;
                                    }
                                }
                            }
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

                    // Don't restore difficulty from DB — always start at the port's
                    // base difficulty and let vardiff ramp up. Restored values caused
                    // loops where broken sessions saved high difficulty, and miners
                    // that don't honor set_difficulty get permanently stuck.
                    let port_base = self.port_difficulty.get(&local_port).copied()
                        .unwrap_or(self.vardiff_config.initial_difficulty);

                    // Priority: password-requested > per-port > default
                    let requested_diff = parse_difficulty_from_password(&password);
                    let initial_diff = self.stratum.fixed_share_target()
                        .map(|fixed| fixed.display_difficulty())
                        .or(requested_diff).or(Some(port_base));
                    let (target, tracker, init_source) = if let Some(diff) = initial_diff {
                        let source = if requested_diff.is_some() { "password" }
                            else { "port" };
                        info!(%session_id, difficulty = diff, port = local_port, source, "Using initial difficulty");
                        let (t, tr) = self.make_target_for_difficulty(diff);
                        (t, tr, format!("initial: {source}"))
                    } else {
                        let (t, tr) = self.make_initial_target();
                        (t, tr, "initial: default".to_string())
                    };
                    let target_hex = hex::encode(target);
                    {
                        let mut sessions = self.session_difficulty.write().await;
                        sessions.insert(session_id.clone(), SessionDifficulty {
                            vardiff: tracker,
                            target,
                            recent_targets: Vec::new(),
                            rate_window_start: Instant::now(),
                            rate_window_shares: 0,
                            low_diff_streak: 0,
                            worker_name: worker_name.clone(),
                            peer_addr: addr.to_string(),
                            connected_at: Instant::now(),
                            local_port,
                            diff_history: Vec::new(),
                            shares_accepted: 0,
                            shares_rejected_low_diff: 0,
                            shares_rejected_job_not_found: 0,
                            shares_rejected_other: 0,
                            recent_share_fps: std::collections::VecDeque::new(),
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
                                reason: init_source,
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
        if self.stratum.fixed_share_target().is_some() { return; }
        let result = {
            let mut sessions = self.session_difficulty.write().await;
            if let Some(sd) = sessions.get_mut(session_id) {
                sd.vardiff.record_share()
            } else {
                None
            }
        };
        if let Some((diff, reason)) = result {
            self.apply_retarget(session_id, diff, &reason).await;
        }
    }

    /// Force an immediate retarget, bypassing the vardiff interval gate.
    async fn force_retarget_session(&self, session_id: &str) {
        if self.stratum.fixed_share_target().is_some() { return; }
        let result = {
            let mut sessions = self.session_difficulty.write().await;
            if let Some(sd) = sessions.get_mut(session_id) {
                sd.vardiff.force_retarget()
            } else {
                None
            }
        };
        if let Some((diff, reason)) = result {
            self.apply_retarget(session_id, diff, &reason).await;
        }
    }

    /// Apply a retarget with a reason string for logging/display.
    async fn apply_retarget(&self, session_id: &str, diff: f64, reason: &str) {
        if self.stratum.fixed_share_target().is_some() { return; }
        let target_hex = difficulty_to_target_hex(diff);
        let new_target = parse_target(&target_hex).unwrap_or(self.default_target);
        {
            let mut sessions = self.session_difficulty.write().await;
            if let Some(sd) = sessions.get_mut(session_id) {
                // Save the previous target to the grace window so in-flight
                // shares the miner already found at the old (easier) diff
                // can still be credited at that diff.
                push_grace_target(sd);
                sd.target = new_target;
                let entry = DiffAdjustment {
                    secs_since_connect: sd.connected_at.elapsed().as_secs(),
                    difficulty: diff,
                    reason: reason.to_string(),
                };
                sd.diff_history.push(entry);
                if sd.diff_history.len() > MAX_DIFF_HISTORY {
                    sd.diff_history.remove(0);
                }
            }
        }
        info!(%session_id, difficulty = diff, target = %target_hex, reason, "Vardiff retarget");
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
                shares_accepted: sd.shares_accepted,
                shares_rejected_low_diff: sd.shares_rejected_low_diff,
                shares_rejected_job_not_found: sd.shares_rejected_job_not_found,
                shares_rejected_other: sd.shares_rejected_other,
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
    /// Audit #15 startup sweep — run once at pool start, before serving.
    /// 1) Settles open block_submissions breadcrumbs (crash between submit
    ///    and record): asks the chain whose block it is; records ours.
    /// 2) Redistributes recent blocks with ZERO credit rows (the block-90
    ///    class: recorded but distribution swallowed) — which also covers
    ///    blocks recovered in step 1.
    pub async fn startup_block_sweep(&self) {
        match self.db.get_open_block_submissions().await {
            Ok(rows) => {
                for (id, height, hash, worker_id, reward, actual) in rows {
                    match self
                        .block_assembler
                        .verify_block_inclusion(height as u64, &hash)
                        .await
                    {
                        crate::block::InclusionCheck::Verified => {
                            let recorded = match self.db.get_block_id_by_hash(&hash).await {
                                Ok(Some(_)) => true,
                                Ok(None) => match self
                                    .db
                                    .record_block(height, &hash, reward, actual, worker_id, None)
                                    .await
                                {
                                    Ok(b) => {
                                        warn!(
                                            height,
                                            block_id = b,
                                            "SWEEP: recovered a won block lost before recording"
                                        );
                                        true
                                    }
                                    Err(e) => {
                                        error!(error = %e, height, "SWEEP: failed to record recovered block");
                                        false
                                    }
                                },
                                Err(e) => {
                                    error!(error = %e, "SWEEP: block lookup failed");
                                    false
                                }
                            };
                            if recorded {
                                let _ = self.db.resolve_block_submission(id, "swept-recorded").await;
                            }
                        }
                        crate::block::InclusionCheck::Mismatch => {
                            let _ = self.db.resolve_block_submission(id, "swept-rejected").await;
                            info!(height, "SWEEP: open submission not on best chain — settled as rejected");
                        }
                        crate::block::InclusionCheck::Unknown => {
                            warn!(height, "SWEEP: node unreachable — leaving submission open for next start");
                        }
                    }
                }
            }
            Err(e) => error!(error = %e, "SWEEP: open-submissions query failed"),
        }

        match self.db.get_recent_blocks_missing_credits(7).await {
            Ok(rows) => {
                for (block_id, height, basis, worker_id) in rows {
                    warn!(block_id, height, "SWEEP: block has no credits — redistributing (block-90 class)");
                    if let Err(e) = self.pplns.distribute(basis, block_id, worker_id).await {
                        error!(error = %e, block_id, "SWEEP: redistribution failed");
                    }
                }
            }
            Err(e) => error!(error = %e, "SWEEP: missing-credits query failed"),
        }
    }

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
    ) -> Result<(i64, Option<f64>), String> {
        if let Some(pps) = &self.pps {
            pps.funding_route.validate_recipient(&pps.epoch.network, address)
                .map_err(|_| "unsupported PPS payout recipient".to_string())?;
        }
        let miner = self.db.get_or_create_miner(address).await
            .map_err(|_| "worker registration database failure".to_string())?;
        let w = self.db.get_or_create_worker(miner.id, worker).await
            .map_err(|_| "worker registration database failure".to_string())?;
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
        if let Some(pps) = &self.pps {
            let address = worker_name.split('.').next().unwrap_or(worker_name);
            pps.funding_route.validate_recipient(&pps.epoch.network, address)
                .map_err(|_| StratumError::other("PPS admission paused: unsupported payout recipient"))?;
        }
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
            // SECURITY: rebuild the header solution canonically from the exact
            // bytes Equihash validated. Reusing the raw miner input would let an
            // attacker append trailing garbage (which Equihash never checks) and
            // grind sha256d over it to forge full-difficulty pool-target shares
            // at ~1/32D the honest cost.
            let raw_vec = raw.to_vec();
            let mut canonical = compact_size(RAW_SOLUTION_SIZE);
            canonical.extend_from_slice(&raw_vec);
            (raw_vec, canonical)
        } else {
            return Err(StratumError::other(&format!(
                "Solution too short: {} bytes", solution_bytes.len()
            )));
        };

        // Reject replays of an identical (job, nonce, solution) before the
        // expensive Equihash check. Without this a miner can resubmit one valid
        // share repeatedly, each credited to PPLNS, for k× reward at no extra
        // hashrate. Fingerprint over the CANONICAL raw solution so padding
        // variants collapse to the same key.
        let share_fp = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            job_id.hash(&mut h);
            nonce.hash(&mut h);
            raw_solution.hash(&mut h);
            h.finish()
        };
        if self.pps.is_none() {
            let mut sessions = self.session_difficulty.write().await;
            if let Some(sd) = sessions.get_mut(session_id) {
                if sd.recent_share_fps.contains(&share_fp) {
                    return Err(StratumError::duplicate_share());
                }
                sd.recent_share_fps.push_back(share_fp);
                if sd.recent_share_fps.len() > SHARE_DEDUP_HISTORY {
                    sd.recent_share_fps.pop_front();
                }
            }
        }

        // Audit #16: clamp miner-supplied ntime BEFORE it enters the header.
        // A miner with a fast clock could otherwise find a "valid" block the
        // node rejects as time-too-new — forfeiting a real block (~1.25 ZEC).
        // Window: [template curtime, now + 90s]. Miners echo the job time or
        // roll it slightly forward; both stay inside.
        {
            validate_ntime_encoding(time)?;
            let miner_time = u32::from_str_radix(time, 16)
                .map_err(|_| StratumError::other("Invalid ntime hex"))?
                .swap_bytes();
            let curtime = job.template.curtime as u32;
            if self.pps.as_ref().is_some_and(|pps| pps.epoch.network == "testnet")
                && miner_time != curtime
            {
                // Testnet minimum-difficulty rules depend on candidate time.
                // A rolled timestamp cannot reuse the template's priced nBits.
                return Err(StratumError::other("PPS testnet requires the exact job timestamp"));
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as u32)
                .unwrap_or(curtime);
            let max_ok = now.saturating_add(90);
            if miner_time < curtime || miner_time > max_ok {
                return Err(StratumError::other(&format!(
                    "ntime out of range: {miner_time} not in [{curtime}, {max_ok}]"
                )));
            }
        }

        // Build the block header input (version + prevhash + merkleroot + reserved + time + bits = 108 bytes)
        let header_input = build_header_input(&job, time)?;
        if header_input.len() != 108 {
            return Err(StratumError::other("Invalid consensus header length"));
        }

        debug!(
            header_len = header_input.len(),
            raw_soln_len = raw_solution.len(),
            soln_with_prefix_len = solution_for_header.len(),
            job_id = %job_id,
            "Verifying Equihash"
        );

        // Build the full serialized header for hashing AND block submission.
        // Block header = header_input(108) + nonce(32) + solution_with_compactSize(1347)
        let mut full_header = header_input.clone();
        full_header.extend_from_slice(&nonce);
        full_header.extend_from_slice(&solution_for_header);

        // Compute SHA-256d of the header (Zcash/Bitcoin block hash)
        let hash_bytes = sha256d(&full_header);

        // Read the session's pool difficulty target plus any recent (grace)
        // targets we still accept shares for.
        let (pool_target, current_difficulty, grace_targets) = {
            let sessions = self.session_difficulty.read().await;
            match sessions.get(session_id) {
                Some(sd) => (
                    sd.target,
                    sd.vardiff.current_difficulty(),
                    sd.recent_targets.clone(),
                ),
                None => (self.default_target, 1.0, Vec::new()),
            }
        };

        // Check against the session's POOL target. If the share doesn't meet
        // the current target, fall back to the grace window: in-flight shares
        // generated against an older (easier) target should still be credited
        // at that older difficulty rather than being rejected as low_diff.
        let difficulty = if let Some(fixed) = self.stratum.fixed_share_target() {
            if !fixed.accepts_hash_le(&hash_bytes) {
                return Err(StratumError::low_difficulty());
            }
            fixed.display_difficulty()
        } else if meets_target(&hash_bytes, &pool_target) {
            current_difficulty
        } else {
            // Walk grace targets newest-first, accept the first that matches
            // and is recent enough.
            let mut matched: Option<f64> = None;
            for gt in grace_targets.iter().rev() {
                if gt.set_at.elapsed() > GRACE_TARGET_MAX_AGE {
                    continue;
                }
                if meets_target(&hash_bytes, &gt.target) {
                    matched = Some(gt.difficulty);
                    break;
                }
            }
            match matched {
                Some(d) => d,
                None => return Err(StratumError::low_difficulty()),
            }
        };

        // Audit #16: Equihash runs AFTER the cheap sha256d target gate. A
        // low-difficulty share (the bulk of storm traffic) is rejected above
        // without paying ~2ms of Equihash verification; every share that can
        // be credited or become a block is still fully verified here.
        equihash::is_valid_solution(200, 9, &header_input, &nonce, &raw_solution)
            .map_err(|e| StratumError::other(&format!("Invalid Equihash solution: {e}")))?;

        // Check against the NETWORK target from the block template
        let network_target = parse_target(&job.template.target)
            .map_err(|e| StratumError::other(&format!("Bad network target: {e}")))?;
        let is_block = meets_target(&hash_bytes, &network_target);

        // DIAG (temporary): log achieved hash difficulty vs assigned to watch
        // the current round. hash_bytes is little-endian (sha256d); reverse to
        // match meets_target/target_to_difficulty big-endian convention.
        {
            let mut rev_hash = hash_bytes;
            rev_hash.reverse();
            let achieved_diff = target_to_difficulty(&rev_hash);
            info!(achieved_diff, assigned_diff = difficulty, is_block, worker = %worker_name, "DIAG_ACH");
        }

        // Resolve (miner_id, worker_id) via the per-session cache (audit #15):
        // the old path ran get_or_create_miner + get_or_create_worker (5 DB
        // statements incl. a last_seen UPDATE) on EVERY accepted share.
        let cache_key = (session_id.to_string(), worker_name.to_string());
        let cached = {
            let c = self.worker_cache.read().await;
            c.get(&cache_key).copied()
        };
        let worker_id: i64 = match cached {
            Some((_m, w, touched)) if touched.elapsed() < WORKER_TOUCH_INTERVAL => w,
            Some((m, w, _)) => {
                let _ = self.db.touch_worker(w).await;
                self.worker_cache
                    .write()
                    .await
                    .insert(cache_key, (m, w, std::time::Instant::now()));
                w
            }
            None => {
                // Parse worker name (format: "address.worker")
                let miner_address = worker_name.split('.').next().unwrap_or(worker_name);
                let wname = worker_name.split('.').nth(1).unwrap_or("default");
                let miner = self.db.get_or_create_miner(miner_address).await
                    .map_err(|e| StratumError::other(&format!("DB error: {e}")))?;
                let worker = self.db.get_or_create_worker(miner.id, wname).await
                    .map_err(|e| StratumError::other(&format!("DB error: {e}")))?;
                let mut c = self.worker_cache.write().await;
                if c.len() > 4096 {
                    // Sessions churn slowly; a rare full clear beats unbounded growth.
                    c.clear();
                }
                c.insert(cache_key, (miner.id, worker.id, std::time::Instant::now()));
                worker.id
            }
        };

        let mut pps_reward = None;
        if let Some(pps) = &self.pps {
            // The fixed target is identical for every issued job. Reject work
            // superseded by a new tip rather than buying stale-chain shares.
            let latest = self.latest_notify.read().await;
            match latest.as_ref() {
                Some(ServerMessage::Notify { prev_hash, .. }) if prev_hash == &job.prev_hash_hex => {},
                _ => { crate::pps_credit_health::denial(&pps.health,"job","stale_job");
                    return Err(StratumError::other("PPS admission paused: stale job")); },
            }
            drop(latest);
            let network = pps.epoch.network.parse::<PpsNetwork>()
                .map_err(|_| { crate::pps_credit_health::denial(&pps.health,"network","network_mismatch");
                    StratumError::other("PPS network invalid") })?;
            let cached = self.pps_subsidies.read().await.get(&job.template.height).copied();
            let subsidy = match cached {
                Some((subsidy, checked)) if checked.elapsed() < Duration::from_secs(15) => subsidy,
                _ => {
                    let subsidy = tokio::time::timeout(Duration::from_secs(5),
                        crate::pps_economics::validated_miner_subsidy(
                            &self.rpc, network, job.template.height,
                        ),
                    ).await.map_err(|_| { crate::pps_credit_health::denial(&pps.health,"subsidy","subsidy_timeout");
                        StratumError::other("PPS admission paused: subsidy evidence timeout") })?
                        .map_err(|_| { crate::pps_credit_health::denial(&pps.health,"subsidy","subsidy_unavailable");
                            StratumError::other("PPS admission paused: subsidy evidence unavailable") })?;
                    let mut cache = self.pps_subsidies.write().await;
                    if cache.len() > 16 { cache.clear(); }
                    cache.insert(job.template.height, (subsidy, Instant::now()));
                    subsidy
                }
            };
            let fixed = self.stratum.fixed_share_target()
                .ok_or_else(|| { crate::pps_credit_health::denial(&pps.health,"target","fixed_target_unavailable");
                    StratumError::other("PPS fixed target unavailable") })?;
            crate::pps_economics::validate_template_target(&job.template.bits, &network_target)
                .map_err(|_| { crate::pps_credit_health::denial(&pps.health,"target","target_invalid");
                    StratumError::other("PPS admission paused: inconsistent consensus target") })?;
            let quote = quote_standard_pps(&PpsQuoteInput {
                network, height: job.template.height, network_target_be: network_target,
                assigned_share_target_be: fixed.target_be(), miner_subsidy_zats: subsidy,
                fee_bps: pps.epoch.fee_bps,
            }).map_err(|_| { crate::pps_credit_health::denial(&pps.health,"quote","quote_invalid");
                StratumError::other("PPS admission paused: invalid price") })?;
            // Preserve the actual price's original observation time, including
            // time spent waiting for the final job guard and ledger commit.
            let quote_checked_at=chrono::Utc::now().timestamp();
            let quote_checked=Instant::now();
            let (proof_id, quote_id) = pps_credit_identity(
                &pps.epoch, job.template.height, subsidy, &network_target,
                &fixed.target_be(), &full_header,
            );
            // RPC may have taken long enough to cross a tip or lease expiry.
            // Recheck after it, and keep the current-job guard until commit.
            let latest = self.latest_notify.read().await;
            match latest.as_ref() {
                Some(ServerMessage::Notify { prev_hash, .. }) if prev_hash == &job.prev_hash_hex => {},
                _ => { crate::pps_credit_health::denial(&pps.health,"job","stale_job");
                    return Err(StratumError::other("PPS admission paused: superseded job")); },
            }
            let now = chrono::Utc::now().timestamp();
            if crate::pps_credit_health::quote_required(&pps.epoch,&pps.funding_route) {
                // Metadata only, under the existing current-tip guard. Keep
                // the actual validated job identity, never relabel an older
                // accepted job as the newest notify. Record before admission
                // so a valid price rejected by the cap also informs health.
                crate::pps_credit_health::validated_quote(&pps.health,&job.job_id,&job.prev_hash_hex,
                    quote.amount_subzatoshis,quote_checked_at,quote_checked);
            }
            let lease = pps.lease.read().await.clone();
            let funding = pps.funding.read().await.clone();
            let receipt = self.db.credit_pps_share(&pps.epoch, &PpsCredit {
                proof_id, quote_id, worker_id, job_id: job_id.to_string(),
                session_id: session_id.to_string(), difficulty, is_block,
                quote_height: job.template.height, network_target_be: network_target,
                assigned_share_target_be: fixed.target_be(), miner_subsidy_zats: subsidy,
                amount_subzatoshis: quote.amount_subzatoshis, accepted_at_unix: now,
            }, lease.as_ref(), funding.as_ref(), now).await
                .map_err(|error| { crate::pps_credit_health::denial(&pps.health,"ledger_credit",
                    crate::pps_credit_health::db_category(&error));
                    StratumError::other("PPS admission paused: ledger or authorization gate") })?;
            drop(latest);
            if receipt.duplicate && !is_block {
                return Ok(ShareResult { is_block: false, block_height: None });
            }
            pps_reward = Some(subsidy as i64);
        } else {
            self.db.record_share(worker_id, job_id, difficulty, is_block, session_id).await
                .map_err(|e| StratumError::other(&format!("DB error: {e}")))?;
        }

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
                    let height = job.template.height as i64;
                    let reward = pps_reward.unwrap_or_else(|| compute_block_reward(height));
                    // Audit P2: actual coinbase value = subsidy + the tx
                    // fees this job's template collected. BIP22 reports
                    // the coinbase "fee" as MINUS the collected fees.
                    // Falls back to tx-fee summation, then to subsidy
                    // only (recorded as NULL so consumers know).
                    let collected_fees = job
                        .template
                        .coinbasetxn
                        .as_ref()
                        .and_then(|cb| cb.fee)
                        .map(|f| -f)
                        .unwrap_or_else(|| {
                            job.template.transactions.iter().map(|t| t.fee).sum()
                        })
                        .max(0);
                    let actual_reward = Some(reward + collected_fees);
                    let hash_hex = hex::encode(hash_bytes);

                    // Audit #15: durable breadcrumb BEFORE submit. If we die
                    // anywhere past this point, the startup sweep settles the
                    // block's fate from the chain — a won block can no longer
                    // be lost between submit and record.
                    let crumb = match self
                        .db
                        .record_block_submission(height, &hash_hex, worker_id, reward, actual_reward)
                        .await
                    {
                        Ok(id) => Some(id),
                        Err(e) => {
                            // Submitting matters more than the breadcrumb.
                            error!(error = %e, "Failed to write block_submissions breadcrumb");
                            None
                        }
                    };

                    // Submit FIRST — every millisecond before submitblock
                    // widens the orphan window (audit #15: the debug file
                    // write moved after; the redundant proposal precheck is
                    // gone from this path).
                    let submit_res = self.block_assembler.submit_block(&block_hex).await;
                    let _ = std::fs::write("last_block.hex", &block_hex);

                    // Timeout ≠ rejection: on a submit RPC error the node may
                    // still have accepted the block — ask the chain before
                    // treating it as rejected.
                    let fate = match submit_res {
                        Ok(_) => crate::block::InclusionCheck::Verified,
                        Err(e) => {
                            warn!(error = %e, height, "submitblock errored — verifying inclusion before treating as rejected");
                            self.block_assembler
                                .verify_block_inclusion(height as u64, &hash_hex)
                                .await
                        }
                    };

                    match fate {
                        crate::block::InclusionCheck::Mismatch => {
                            if let Some(cid) = crumb {
                                let _ = self.db.resolve_block_submission(cid, "rejected").await;
                            }
                            error!(height, "Block submission rejected (not on best chain)");
                        }
                        crate::block::InclusionCheck::Unknown => {
                            // Node unreachable: fate unknown. Leave the
                            // breadcrumb OPEN — the startup sweep (or the
                            // next one) settles it against the chain.
                            warn!(height, "Block fate unknown (node unreachable) — breadcrumb left open for sweep");
                        }
                        crate::block::InclusionCheck::Verified => {

                            // Luck window start must predate OUR row — grab
                            // it now (one fast local query); the luck math
                            // itself (node RPC) runs off-loop and updates the
                            // row afterwards.
                            let luck_since = self
                                .db
                                .get_recent_blocks(1)
                                .await
                                .ok()
                                .and_then(|b| b.first().map(|x| x.created_at.clone()))
                                .unwrap_or_else(|| "1970-01-01 00:00:00".to_string());

                            // Record IMMEDIATELY after chain-accept. Luck,
                            // the P11 height-race check, and distribution all
                            // run off the validator loop so a slow node can
                            // never watchdog-kill us between submitblock
                            // success and the DB row (block-90's sibling).
                            match self.db.record_block(height, &hash_hex, reward, actual_reward, worker_id, None).await {
                                Ok(block_id) => {
                                    if let Some(cid) = crumb {
                                        let _ = self.db.resolve_block_submission(cid, "recorded").await;
                                    }
                                    info!(height, block_id, "Block recorded — post-processing off-loop");
                                    block_height = Some(height);

                                    let db = self.db.clone();
                                    let assembler = Arc::clone(&self.block_assembler);
                                    let pplns = Arc::clone(&self.pplns);
                                    let rpc = Arc::clone(&self.rpc);
                                    let diff_mult = self.difficulty_multiplier;
                                    let hash_task = hash_hex.clone();
                                    let basis = actual_reward.unwrap_or(reward);
                                    tokio::spawn(async move {
                                        // Audit P11 (off-loop): submit-Ok only
                                        // proves validation; confirm best-chain
                                        // before crediting. Our row exists but
                                        // has NO credits yet, so orphaning here
                                        // is a plain status flip.
                                        let inclusion = assembler
                                            .verify_block_inclusion(height as u64, &hash_task)
                                            .await;
                                        if inclusion == crate::block::InclusionCheck::Mismatch {
                                            warn!(height, hash = %hash_task, "Block lost the height race — orphaning fresh row (no credits yet)");
                                            let _ = db.update_block_status(block_id, "orphaned").await;
                                            return;
                                        }
                                        // Luck (RPC + share sum since previous block).
                                        if let Ok(nh) = rpc.get_network_sol_ps(Some(120)).await {
                                            if nh > 0.0 {
                                                if let Ok(ds) = db.get_difficulty_sum_since(&luck_since).await {
                                                    let actual_work = ds * diff_mult;
                                                    if actual_work > 0.0 {
                                                        let luck = (actual_work / (nh * BLOCK_TIME_SECS)) * 100.0;
                                                        let _ = db.update_block_luck(block_id, luck).await;
                                                    }
                                                }
                                            }
                                        }
                                        info!(height, basis, block_id, "Distributing block rewards (off-loop)");
                                        if let Err(e) = pplns.distribute(basis, block_id, worker_id).await {
                                            error!(error = %e, block_id, "Reward distribution failed — startup sweep retries via missing-credits pass");
                                        }
                                    });
                                }
                                Err(e) => {
                                    // Breadcrumb stays OPEN — the startup
                                    // sweep records + distributes from it.
                                    error!(error = %e, "Failed to record block — breadcrumb left open for sweep");
                                }
                            }
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

impl Drop for ShareValidator {
    fn drop(&mut self) {
        if let Some(task) = self.pps_refresh_task.take() {
            task.abort();
        }
    }
}

pub struct ShareResult {
    pub is_block: bool,
    pub block_height: Option<i64>,
}

/// A canonical proof is globally unique across sessions, job counter reuse,
/// and policy epochs. Pricing provenance is separate and binds exact inputs.
fn pps_credit_identity(
    epoch: &PpsEpoch, height: u64, subsidy: u64,
    network_target: &[u8; 32], assigned_target: &[u8; 32], header: &[u8],
) -> (String, String) {
    let mut proof = Sha256::new();
    proof.update(b"zcash-pps-proof-v1\0");
    proof.update(epoch.network.as_bytes());
    proof.update([0]);
    proof.update(header);
    let mut quote = Sha256::new();
    quote.update(b"zcash-pps-standard-fixed-v1\0");
    quote.update(epoch.network.as_bytes());
    quote.update([0]);
    quote.update(epoch.id.as_bytes());
    quote.update([0]);
    quote.update(epoch.quote_provenance.as_bytes());
    quote.update([0]);
    quote.update(height.to_be_bytes());
    quote.update(subsidy.to_be_bytes());
    quote.update(epoch.fee_bps.to_be_bytes());
    quote.update(network_target);
    quote.update(assigned_target);
    (hex::encode(proof.finalize()), hex::encode(quote.finalize()))
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
    let (prefix_len, size): (usize, usize) = match data[0] {
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
    // SECURITY: checked_add prevents usize overflow — a 0xFF-prefixed length near
    // u64::MAX would otherwise wrap prefix_len+size to a tiny value, pass a naive
    // bounds check, and panic on the out-of-range slice. That panic unwinds the
    // single validator task; the watchdog then exit(3)s the whole pool — a
    // one-packet, replayable denial of service.
    let end = prefix_len
        .checked_add(size)
        .filter(|&e| e <= data.len())
        .ok_or_else(|| StratumError::other("Solution shorter than declared size"))?;
    Ok(&data[prefix_len..end])
}

fn validate_ntime_encoding(time_hex: &str) -> Result<(), StratumError> {
    if time_hex.len() != 8 || !time_hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(StratumError::other("ntime must encode exactly four bytes"));
    }
    Ok(())
}

fn build_header_input(job: &MiningJob, time_hex: &str) -> Result<Vec<u8>, StratumError> {
    validate_ntime_encoding(time_hex)?;
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
/// Save the session's current target to the grace window. Called right
/// before replacing `sd.target` with a new value. The old difficulty is
/// derived from the old target bytes (since `vardiff.current_difficulty()`
/// has already been updated to the new value by the time this runs).
/// Caps the buffer at `GRACE_TARGET_HISTORY` entries (oldest evicted first).
fn push_grace_target(sd: &mut SessionDifficulty) {
    let entry = GraceTarget {
        target: sd.target,
        difficulty: target_to_difficulty(&sd.target),
        set_at: Instant::now(),
    };
    sd.recent_targets.push(entry);
    if sd.recent_targets.len() > GRACE_TARGET_HISTORY {
        sd.recent_targets.remove(0);
    }
}

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
    // Audit #16: the old formula used the FIRST halving's height (1,046,400)
    // as a recurring interval. That coincidentally matches the real schedule
    // in the current era but halves ~220k blocks (~6 months) EARLY at height
    // 4,185,600 (~mid-2028), silently under-crediting every block by 2x.
    //
    // Real Zcash schedule (post-Blossom 75s spacing):
    //   [..1,046,400)          total  6.25 ZEC  (shift 1 from 12.5)
    //   [1,046,400..2,726,400) total  3.125     (shift 2)
    //   [2,726,400..4,406,400) total  1.5625    (shift 3)
    //   then every 1,680,000 blocks: one more halving.
    let initial_subsidy: i64 = 1_250_000_000; // 12.5 ZEC in zatoshis
    if height < 0 {
        return 0;
    }
    let halvings: i64 = if height < 1_046_400 {
        1
    } else if height < 2_726_400 {
        2
    } else {
        3 + (height - 2_726_400) / 1_680_000
    };
    if halvings >= 64 {
        return 0;
    }
    let total_subsidy = initial_subsidy >> halvings;
    // Miner receives 80% of the subsidy (post-NU6)
    total_subsidy * 80 / 100
}

#[cfg(test)]
mod reward_schedule_tests {
    use super::compute_block_reward;

    #[test]
    fn current_era_unchanged() {
        // Today's blocks (3.2M-3.5M era) must pay exactly what the old
        // formula paid: 1.25 ZEC miner share.
        for h in [3_273_381, 3_432_288, 3_446_714] {
            assert_eq!(compute_block_reward(h), 125_000_000, "height {h}");
        }
    }

    #[test]
    fn old_formula_break_point_now_correct() {
        // The old formula halved at 4,185,600 (4 x 1,046,400) — six months
        // before the real halving at 4,406,400. Must stay 1.25 until then.
        assert_eq!(compute_block_reward(4_185_600), 125_000_000);
        assert_eq!(compute_block_reward(4_406_399), 125_000_000);
        // Real third halving: miner share drops to 0.625.
        assert_eq!(compute_block_reward(4_406_400), 62_500_000);
        // And the one after (interval 1,680,000).
        assert_eq!(compute_block_reward(6_086_400), 31_250_000);
    }
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

    fn refresh_test_proof() -> PpsFundingLease {
        PpsFundingLease { network:"testnet".into(),checked_at_unix:100,valid_until_unix:160,
            spendable_zatoshis:1_000_000_001,reserve_floor_zatoshis:1,
            reserved_fee_allowance_zatoshis:50_000_000,generation:7 }
    }

    #[test]
    fn pps_testnet_refresh_retains_original_proof_without_renewing_expiry() {
        use crate::pps_funding::{PpsFundingRoute, validate_funding_lease};
        let route = PpsFundingRoute::ZecdConventionalTestnet { hold_new_legacy_sends:true };
        let original = refresh_test_proof();
        let mut cached = Some(original.clone());
        begin_pps_funding_refresh(&route,&mut cached);
        assert_eq!(cached,Some(original.clone()));
        let epoch = PpsEpoch { id:"refresh-test".into(),network:"testnet".into(),fee_bps:0,
            max_liability_zatoshis:950_000_000,total_exposure_zatoshis:1_000_000_000,
            fee_allowance_zatoshis:50_000_000,reserve_floor_zatoshis:1,
            quote_provenance:"synthetic".into() };
        assert!(validate_funding_lease(cached.as_ref().unwrap(),&epoch,159).is_ok());
        assert!(validate_funding_lease(cached.as_ref().unwrap(),&epoch,160).is_err());
        // Even if a replacement is still pending after expiry, no helper
        // extends its timestamp or changes the generation used by the DB.
        begin_pps_funding_refresh(&route,&mut cached);
        assert_eq!(cached,Some(original));
        assert!(validate_funding_lease(cached.as_ref().unwrap(),&epoch,168).is_err());
    }

    #[test]
    fn pps_testnet_refresh_uses_single_flight_start_to_start_success_cadence() {
        use crate::pps_funding::PpsFundingRoute;
        let route = PpsFundingRoute::ZecdConventionalTestnet { hold_new_legacy_sends:true };
        for (collection_seconds,delay_seconds) in [(0,120),(10,110),(30,90),(34,86),(150,0)] {
            let mut cached = Some(refresh_test_proof());
            let mut replacement = refresh_test_proof();
            replacement.checked_at_unix = 134;
            replacement.valid_until_unix = 194;
            replacement.generation = 8;
            let delay = finish_pps_funding_refresh(&route,&mut cached,Ok(replacement.clone()),
                Duration::from_secs(collection_seconds),"ok");
            assert_eq!(delay,Duration::from_secs(delay_seconds));
            assert_eq!(cached,Some(replacement));
        }
    }

    #[test]
    fn pps_testnet_refresh_failure_retains_only_on_transient_categories() {
        use crate::pps_funding::{PpsFundingError, PpsFundingRoute};
        let route = PpsFundingRoute::ZecdConventionalTestnet { hold_new_legacy_sends:true };
        let original = refresh_test_proof();
        // A transient read failure keeps the previous proof (its own expiry
        // and generation still gate every credit); any substantive rejection
        // revokes it.
        for category in ["rpc_unavailable","wallet_not_ready","concurrent_change","deadline_exceeded"] {
            for elapsed in [Duration::ZERO,Duration::from_secs(34),Duration::from_secs(45)] {
                let mut cached = Some(original.clone());
                begin_pps_funding_refresh(&route,&mut cached);
                assert!(cached.is_some());
                assert_eq!(finish_pps_funding_refresh(&route,&mut cached,
                    Err(PpsFundingError::Timeout),elapsed,category),
                    Duration::from_secs(30));
                assert_eq!(cached,Some(original.clone()));
            }
        }
        for category in ["invalid_evidence","funding_insufficient","chain_mismatch",
            "accounting_unavailable","identity_signer_not_proven"] {
            let mut cached = Some(original.clone());
            begin_pps_funding_refresh(&route,&mut cached);
            assert_eq!(finish_pps_funding_refresh(&route,&mut cached,
                Err(PpsFundingError::InvalidEvidence),Duration::ZERO,category),
                Duration::from_secs(30));
            assert!(cached.is_none());
        }
    }

    #[test]
    fn pps_pczt_refresh_preserves_clear_during_read_and_completion_delay() {
        use crate::pps_funding::{PpsFundingError, PpsFundingRoute};
        for network in ["mainnet","testnet"] {
            let route = PpsFundingRoute::ZalletPczt;
            let mut proof = refresh_test_proof();
            proof.network = network.into();
            let mut cached = Some(proof.clone());
            begin_pps_funding_refresh(&route,&mut cached);
            assert!(cached.is_none());
            assert_eq!(finish_pps_funding_refresh(&route,&mut cached,Ok(proof.clone()),
                Duration::from_secs(34),"ok"),Duration::from_secs(120));
            assert_eq!(cached,Some(proof));
            begin_pps_funding_refresh(&route,&mut cached);
            // PCZT already cleared during the read, so a transient category
            // has nothing to retain: failures always leave it revoked.
            assert_eq!(finish_pps_funding_refresh(&route,&mut cached,
                Err(PpsFundingError::Timeout),Duration::ZERO,"deadline_exceeded"),Duration::from_secs(30));
            assert!(cached.is_none());
        }
    }

    #[test]
    fn pps_credit_refresh_fast_retries_are_bounded_and_retain_transient_proofs() {
        use crate::pps_funding::{PpsFundingError,PpsFundingRoute};
        let route=PpsFundingRoute::ZecdConventionalTestnet {hold_new_legacy_sends:true};
        for category in ["rpc_unavailable","wallet_not_ready","concurrent_change","deadline_exceeded"] {
            let mut failures=0;
            for expected in [5,5,30,30] {
                let mut cached=Some(refresh_test_proof());
                let ordinary=finish_pps_funding_refresh(&route,&mut cached,Err(PpsFundingError::WalletUnavailable),Duration::ZERO,category);
                assert_eq!(pps_refresh_retry_delay(&route,category,&mut failures,ordinary),Duration::from_secs(expected));
                assert!(cached.is_some());
            }
            assert_eq!(pps_refresh_retry_delay(&route,"ok",&mut failures,Duration::ZERO),Duration::ZERO);
            assert_eq!(failures,0);
            assert_eq!(pps_refresh_retry_delay(&route,category,&mut failures,Duration::from_secs(30)),Duration::from_secs(5));
        }
        for category in ["invalid_evidence","identity_mismatch","chain_mismatch","accounting_unavailable","fee_capacity_exhausted","identity_signer_not_proven"] {
            assert_eq!(pps_refresh_retry_delay(&route,category,&mut 0,Duration::from_secs(30)),Duration::from_secs(30));
        }
        assert_eq!(pps_refresh_retry_delay(&PpsFundingRoute::ZalletPczt,"deadline_exceeded",&mut 0,Duration::from_secs(30)),Duration::from_secs(30));
    }

    #[test]
    fn consensus_ntime_requires_exact_four_byte_hex() {
        for invalid in ["", "000000", "0000000", "000000000", "0000000000", "0000000g", "é000000"] {
            assert!(validate_ntime_encoding(invalid).is_err());
        }
        assert!(validate_ntime_encoding("001122Af").is_ok());
    }

    #[test]
    fn pps_proof_identity_is_restart_stable_and_quote_binds_economics() {
        let mut epoch = PpsEpoch {
            id: "test-epoch".into(), network: "testnet".into(), fee_bps: 100,
            max_liability_zatoshis: 1_000_000, quote_provenance: "fixed-v1".into(),
            total_exposure_zatoshis: 1_010_000, fee_allowance_zatoshis: 10_000,
            reserve_floor_zatoshis: 1,
        };
        let n = [1u8; 32];
        let s = [2u8; 32];
        let original = pps_credit_identity(&epoch, 100, 25, &n, &s, b"canonical-header");
        assert_eq!(original, pps_credit_identity(&epoch, 100, 25, &n, &s, b"canonical-header"));
        epoch.fee_bps += 1;
        let changed_fee = pps_credit_identity(&epoch, 100, 25, &n, &s, b"canonical-header");
        assert_eq!(original.0, changed_fee.0);
        assert_ne!(original.1, changed_fee.1);
        epoch.fee_bps -= 1;
        for changed in [
            pps_credit_identity(&epoch, 101, 25, &n, &s, b"canonical-header"),
            pps_credit_identity(&epoch, 100, 26, &n, &s, b"canonical-header"),
            pps_credit_identity(&epoch, 100, 25, &s, &s, b"canonical-header"),
            pps_credit_identity(&epoch, 100, 25, &n, &n, b"canonical-header"),
        ] {
            assert_eq!(original.0, changed.0);
            assert_ne!(original.1, changed.1);
        }
        assert_ne!(original.0, pps_credit_identity(&epoch, 100, 25, &n, &s, b"other-header").0);
        epoch.id = "new-epoch".into();
        let next_epoch = pps_credit_identity(&epoch, 100, 25, &n, &s, b"canonical-header");
        assert_eq!(original.0, next_epoch.0);
        assert_ne!(original.1, next_epoch.1);
        epoch.network = "mainnet".into();
        assert_ne!(original.0, pps_credit_identity(&epoch, 100, 25, &n, &s, b"canonical-header").0);
    }

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
    fn test_strip_compact_size_overflow_is_error_not_panic() {
        // SECURITY regression: a 0xFF-prefixed length near u64::MAX must return
        // Err (not panic via a wrapped prefix_len+size slice). This was a
        // one-packet validator crash-loop DoS.
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        assert!(strip_compact_size(&data).is_err());
        // Declared-but-absent bytes also error rather than slice OOB.
        let short = vec![0xFD, 0x40, 0x05, 0x11, 0x22];
        assert!(strip_compact_size(&short).is_err());
    }

    #[test]
    fn test_block_reward() {
        // NOTE (audit #16): the old assertions here validated the buggy
        // interval formula against itself — e.g. claiming 6.25 total AT the
        // first halving height, where the real chain pays 3.125. Corrected to
        // the actual post-Blossom schedule.
        // Pre-first-halving era (pool never mined here): total 6.25, miner 5.
        assert_eq!(compute_block_reward(1), 500_000_000);
        // At the first halving (1,046,400): total 3.125, miner 2.5.
        assert_eq!(compute_block_reward(1_046_400), 250_000_000);
        // Current era: total 1.5625, miner 1.25.
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
