use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use node_rpc::types::BlockTemplate;
use node_rpc::ZcashRpcClient;
use sha2::{Digest, Sha256};
use stratum::server::StratumServer;
use stratum::ServerMessage;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};


/// A mining job derived from a block template.
#[derive(Debug, Clone)]
pub struct MiningJob {
    pub job_id: String,
    pub template: BlockTemplate,
    /// Version as little-endian hex (e.g. "04000000")
    pub version_hex: String,
    /// Previous block hash hex (as in block header)
    pub prev_hash_hex: String,
    /// Merkle root hex (as in block header)
    pub merkle_root_hex: String,
    /// Reserved field hex (32-byte zero by convention)
    pub reserved_hex: String,
    /// Block time hex (little-endian)
    pub time_hex: String,
    /// Compact difficulty bits hex
    pub bits_hex: String,
}

impl MiningJob {
    pub fn from_template(template: BlockTemplate, job_id: String) -> Self {
        let version_hex = format!("{:08x}", template.version.swap_bytes());

        let prev_hash_hex = reverse_hex(&template.previousblockhash);

        let merkle_root_rpc = template
            .defaultroots
            .as_ref()
            .and_then(|dr| dr.merkleroot.clone())
            .unwrap_or_else(|| compute_merkle_root(&template));
        let merkle_root_hex = reverse_hex(&merkle_root_rpc);

        // RESERVED = hashBlockCommitments
        let reserved_rpc = template
            .defaultroots
            .as_ref()
            .and_then(|dr| dr.blockcommitmentshash.clone())
            .or_else(|| template.blockcommitmentshash.clone())
            .or_else(|| template.lightclientroothash.clone())
            .or_else(|| template.finalsaplingroothash.clone())
            .unwrap_or_else(|| "00".repeat(32));
        let reserved_hex = reverse_hex(&reserved_rpc);

        // Time: LE 4 bytes
        let time_hex = format!("{:08x}", (template.curtime as u32).swap_bytes());

        // Bits: 4-byte value from RPC is big-endian hex, block header needs LE
        let bits_hex = reverse_hex(&template.bits);

        Self {
            job_id,
            template,
            version_hex,
            prev_hash_hex,
            merkle_root_hex,
            reserved_hex,
            time_hex,
            bits_hex,
        }
    }

    /// Convert this job into a stratum Notify message.
    pub fn to_notify(&self, clean_jobs: bool) -> ServerMessage {
        ServerMessage::Notify {
            job_id: self.job_id.clone(),
            version: self.version_hex.clone(),
            prev_hash: self.prev_hash_hex.clone(),
            merkle_root: self.merkle_root_hex.clone(),
            reserved: self.reserved_hex.clone(),
            time: self.time_hex.clone(),
            bits: self.bits_hex.clone(),
            clean_jobs,
        }
    }
}

/// How often to send non-clean job updates to miners (seconds).
const NON_CLEAN_JOB_INTERVAL_SECS: u64 = 5;

/// Configuration for BIP22 long-polling of block templates.
#[derive(Clone, Debug)]
pub struct LongpollConfig {
    pub enabled: bool,
    pub timeout: Duration,
}

impl Default for LongpollConfig {
    fn default() -> Self {
        Self { enabled: false, timeout: Duration::from_secs(60) }
    }
}

/// Manages mining jobs by polling the node for new block templates.
pub struct JobManager {
    rpc: Arc<ZcashRpcClient>,
    stratum: Arc<StratumServer>,
    jobs: Arc<RwLock<HashMap<String, MiningJob>>>,
    job_counter: Arc<std::sync::atomic::AtomicU64>,
    last_prev_hash: Arc<RwLock<String>>,
    last_non_clean_broadcast: Arc<RwLock<std::time::Instant>>,
    /// When set, updated with unix timestamp (ms) on each successful template fetch. Used for stall detection.
    last_template_at_ms: Option<Arc<AtomicI64>>,
    /// Most recent notify message, sent to newly connecting miners.
    latest_notify: Arc<RwLock<Option<ServerMessage>>>,
    /// Optional tag to inject into coinbase scriptSig (e.g. "Legends").
    coinbase_tag: Option<Vec<u8>>,
    /// Long-poll configuration.
    longpoll: LongpollConfig,
    /// Last `longpollid` returned by the node. Passed back on the next
    /// longpoll request so the node knows which template we already have.
    last_longpollid: Arc<RwLock<Option<String>>>,
    /// Counter of successful longpoll wake-ups (new template returned).
    longpoll_wake_count: Arc<std::sync::atomic::AtomicU64>,
}

impl JobManager {
    pub fn new(rpc: Arc<ZcashRpcClient>, stratum: Arc<StratumServer>) -> Self {
        Self::new_with_stall_tracking(rpc, stratum, None)
    }

    /// Same as `new` but with an optional `Arc<AtomicI64>` to store last successful template time (unix ms).
    pub fn new_with_stall_tracking(
        rpc: Arc<ZcashRpcClient>,
        stratum: Arc<StratumServer>,
        last_template_at_ms: Option<Arc<AtomicI64>>,
    ) -> Self {
        Self::new_with_stall_tracking_and_notify(rpc, stratum, last_template_at_ms, Arc::new(RwLock::new(None)))
    }

    /// Full constructor with external latest_notify Arc (shared with stratum server).
    pub fn new_with_stall_tracking_and_notify(
        rpc: Arc<ZcashRpcClient>,
        stratum: Arc<StratumServer>,
        last_template_at_ms: Option<Arc<AtomicI64>>,
        latest_notify: Arc<RwLock<Option<ServerMessage>>>,
    ) -> Self {
        Self {
            rpc,
            stratum,
            jobs: Arc::new(RwLock::new(HashMap::new())),
            job_counter: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            last_prev_hash: Arc::new(RwLock::new(String::new())),
            last_non_clean_broadcast: Arc::new(RwLock::new(std::time::Instant::now())),
            last_template_at_ms,
            latest_notify,
            coinbase_tag: None,
            longpoll: LongpollConfig::default(),
            last_longpollid: Arc::new(RwLock::new(None)),
            longpoll_wake_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Set the coinbase tag to inject into each coinbase scriptSig.
    pub fn set_coinbase_tag(&mut self, tag: Vec<u8>) {
        self.coinbase_tag = Some(tag);
    }

    /// Enable or disable BIP22 long-polling.
    pub fn set_longpoll_config(&mut self, cfg: LongpollConfig) {
        self.longpoll = cfg;
    }

    /// Accessor for the longpoll wake counter (for dashboards).
    pub fn longpoll_wake_count(&self) -> Arc<std::sync::atomic::AtomicU64> {
        Arc::clone(&self.longpoll_wake_count)
    }

    pub fn jobs(&self) -> Arc<RwLock<HashMap<String, MiningJob>>> {
        Arc::clone(&self.jobs)
    }

    pub fn latest_notify(&self) -> Arc<RwLock<Option<ServerMessage>>> {
        Arc::clone(&self.latest_notify)
    }

    /// Start polling for new block templates. Runs indefinitely.
    /// If longpoll is enabled, uses BIP22 long-polling to detect template
    /// changes instantly, falling back to `poll_interval` regular polling
    /// on errors or before we have a `longpollid`.
    pub async fn run(&self, poll_interval: Duration) {
        if self.longpoll.enabled {
            info!(timeout_secs = self.longpoll.timeout.as_secs(),
                  fallback_interval = ?poll_interval,
                  "Job manager started with longpoll enabled");
        } else {
            info!("Job manager started, polling every {:?}", poll_interval);
        }

        loop {
            // Decide: longpoll if enabled and we have a longpollid, else regular poll.
            let lpid = if self.longpoll.enabled {
                self.last_longpollid.read().await.clone()
            } else {
                None
            };

            let result = match lpid {
                Some(id) => self.poll_template_with_longpoll(&id).await,
                None => self.poll_template().await,
            };

            match result {
                Ok(new_block) => {
                    if new_block {
                        debug!("New block detected, jobs cleaned");
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Failed to poll block template");
                    // Sleep briefly after errors to avoid a tight retry loop.
                    tokio::time::sleep(poll_interval).await;
                    continue;
                }
            }

            // When longpoll is active and succeeded, zebrad already blocked
            // until the template changed, so we can loop immediately. When
            // using regular polling, sleep between cycles.
            if !self.longpoll.enabled || self.last_longpollid.read().await.is_none() {
                tokio::time::sleep(poll_interval).await;
            }
        }
    }

    /// Long-poll variant: passes `longpollid` back to the node. Returns
    /// when the template changes, or falls back to a regular poll on
    /// error so the loop never stalls.
    async fn poll_template_with_longpoll(&self, longpollid: &str) -> Result<bool, node_rpc::RpcError> {
        match self.rpc.get_block_template_longpoll(longpollid, self.longpoll.timeout).await {
            Ok(template) => {
                self.longpoll_wake_count.fetch_add(1, Ordering::Relaxed);
                self.process_template(template).await
            }
            Err(e) => {
                // Longpoll failed (timeout, HTTP error, etc.) — fall back
                // to a normal poll so we always have a fresh template and
                // longpollid for the next iteration.
                warn!(error = %e, "Longpoll failed, falling back to regular poll");
                self.poll_template().await
            }
        }
    }

    /// Poll the node for a new block template. Broadcasts clean jobs
    /// immediately on new blocks. Non-clean job updates are throttled
    /// to avoid spamming miners that ignore them.
    async fn poll_template(&self) -> Result<bool, node_rpc::RpcError> {
        let template = self.rpc.get_block_template().await?;
        self.process_template(template).await
    }

    /// Build and broadcast an empty-block notify: a mining job with no
    /// mempool transactions, just our (tagged) coinbase. Used as the
    /// race-to-tip fast path — lets miners start hashing on the new
    /// prev_hash while we finish building the full template in parallel.
    async fn broadcast_empty_block(&self, template: &BlockTemplate) {
        // Clone into a stripped-down template with no mempool txs and no
        // pre-cached merkle root (so compute_merkle_root runs over just
        // the coinbase).
        let mut empty = template.clone();
        empty.transactions.clear();
        if let Some(ref mut dr) = empty.defaultroots {
            dr.merkleroot = None;
        }

        // Re-apply our coinbase tag with zero auth_digests (since the
        // block will contain only the coinbase transaction).
        if let Some(ref tag) = self.coinbase_tag {
            if let Some(ref mut cb) = empty.coinbasetxn {
                let chain_history_root = empty
                    .defaultroots
                    .as_ref()
                    .and_then(|dr| dr.chainhistoryroot.as_deref())
                    .unwrap_or("");
                match crate::coinbase::inject_coinbase_tag(
                    &cb.data,
                    tag,
                    chain_history_root,
                    &[], // no other tx auth digests
                ) {
                    Ok(result) => {
                        cb.data = result.new_coinbase_hex;
                        if let Some(new_txid) = result.new_txid {
                            cb.hash = new_txid;
                        }
                        if let Some(new_bc) = result.new_block_commitments {
                            if let Some(ref mut dr) = empty.defaultroots {
                                dr.blockcommitmentshash = Some(new_bc.clone());
                            }
                            empty.blockcommitmentshash = Some(new_bc);
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "empty-block: failed to inject coinbase tag; skipping race-to-tip for this round");
                        return;
                    }
                }
            }
        }

        let job_id = self
            .job_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .to_string();
        let job = MiningJob::from_template(empty, job_id.clone());
        let notify = job.to_notify(true); // clean_jobs=true: switch to new tip

        // Cache the empty job so shares submitted against it can be validated.
        {
            let mut jobs = self.jobs.write().await;
            jobs.insert(job_id.clone(), job);
        }
        {
            let mut latest = self.latest_notify.write().await;
            *latest = Some(notify.clone());
        }
        self.stratum.broadcast_notify(notify);
        debug!(job_id = %job_id, "Race-to-tip: emitted empty-block notify");
    }

    /// Process a fetched template: detect new-block, inject coinbase tag,
    /// create job, broadcast notify. Shared by regular and longpoll paths.
    async fn process_template(&self, mut template: BlockTemplate) -> Result<bool, node_rpc::RpcError> {
        // Capture the longpollid for the next request (if present).
        {
            let mut lpid = self.last_longpollid.write().await;
            *lpid = template.longpollid.clone();
        }
        let new_prev_hash = template.previousblockhash.clone();

        let is_new_block = {
            let last = self.last_prev_hash.read().await;
            *last != new_prev_hash
        };

        // Race-to-tip: on a new block, emit a lightweight empty-block notify
        // (no mempool txs) so miners switch to the new prev_hash ~5-15ms
        // earlier than if we waited to finish the full template build. If a
        // miner finds a share against the empty template it's still a valid
        // block — we just forgo the mempool tx fees on that one block. The
        // full-template notify follows immediately with clean_jobs=false.
        if is_new_block {
            self.broadcast_empty_block(&template).await;
        }

        if let Some(ref at) = self.last_template_at_ms {
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            at.store(ms, Ordering::Relaxed);
        }

        let should_broadcast = if is_new_block {
            true
        } else {
            let last = self.last_non_clean_broadcast.read().await;
            last.elapsed().as_secs() >= NON_CLEAN_JOB_INTERVAL_SECS
        };

        if !should_broadcast {
            return Ok(false);
        }

        // Inject coinbase tag if configured
        if let Some(ref tag) = self.coinbase_tag {
            if let Some(ref mut cb) = template.coinbasetxn {
                let chain_history_root = template
                    .defaultroots
                    .as_ref()
                    .and_then(|dr| dr.chainhistoryroot.as_deref())
                    .unwrap_or("");

                // Collect auth digests from non-coinbase transactions (provided by zebrad)
                let tx_auth_digests: Vec<String> = template
                    .transactions
                    .iter()
                    .filter_map(|tx| tx.authdigest.clone())
                    .collect();

                match crate::coinbase::inject_coinbase_tag(
                    &cb.data,
                    tag,
                    chain_history_root,
                    &tx_auth_digests,
                ) {
                    Ok(result) => {
                        cb.data = result.new_coinbase_hex;
                        if let Some(new_txid) = result.new_txid {
                            // v4: update txid and clear cached merkleroot
                            cb.hash = new_txid;
                            if let Some(ref mut dr) = template.defaultroots {
                                dr.merkleroot = None;
                            }
                        }
                        if let Some(new_bc) = result.new_block_commitments {
                            // v5: update blockcommitmentshash
                            if let Some(ref mut dr) = template.defaultroots {
                                dr.blockcommitmentshash = Some(new_bc.clone());
                            }
                            template.blockcommitmentshash = Some(new_bc);
                        }
                        debug!(
                            tx_count = template.transactions.len(),
                            "Injected coinbase tag"
                        );
                    }
                    Err(e) => {
                        warn!("Failed to inject coinbase tag: {e}");
                    }
                }
            }
        }

        let job_id = self
            .job_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .to_string();

        let job = MiningJob::from_template(template, job_id.clone());
        // Race-to-tip already sent the clean_jobs=true empty notify when
        // is_new_block. This follow-up full-template notify uses
        // clean_jobs=false so miners keep any in-flight shares they
        // already found against the empty job as still-valid.
        let notify = job.to_notify(false);

        {
            let mut jobs = self.jobs.write().await;
            if is_new_block {
                // Keep the last few jobs so shares found just before a block
                // change aren't rejected as "Job not found".
                if jobs.len() > 10 {
                    // Sort by numeric job_id, not string, so e.g. "1000" sorts
                    // after "999" instead of after "1" (lexicographic). Without
                    // this fix, every counter digit-boundary crossing evicts
                    // the newest jobs and keeps old ones — causing spurious
                    // Job-not-found rejections on the next block transition.
                    let mut ids: Vec<String> = jobs.keys().cloned().collect();
                    ids.sort_by_key(|s| s.parse::<u64>().unwrap_or(0));
                    let remove_count = ids.len().saturating_sub(3);
                    for id in ids.into_iter().take(remove_count) {
                        jobs.remove(&id);
                    }
                }
            }
            jobs.insert(job_id.clone(), job);
        }

        if is_new_block {
            let mut last = self.last_prev_hash.write().await;
            *last = new_prev_hash;
        } else {
            let mut last = self.last_non_clean_broadcast.write().await;
            *last = std::time::Instant::now();
        }

        {
            let mut latest = self.latest_notify.write().await;
            *latest = Some(notify.clone());
        }
        self.stratum.broadcast_notify(notify);

        Ok(is_new_block)
    }
}

/// Reverse the byte order of a hex string (e.g., "aabbccdd" -> "ddccbbaa").
/// Used to convert between RPC display order and block header internal order.
fn reverse_hex(hex_str: &str) -> String {
    let bytes = hex::decode(hex_str).unwrap_or_default();
    let reversed: Vec<u8> = bytes.into_iter().rev().collect();
    hex::encode(reversed)
}

fn sha256d(data: &[u8]) -> [u8; 32] {
    let first = Sha256::digest(data);
    let second = Sha256::digest(first);
    let mut out = [0u8; 32];
    out.copy_from_slice(&second);
    out
}

/// Compute the merkle root from coinbase + transaction hashes in the template.
fn compute_merkle_root(template: &BlockTemplate) -> String {
    let mut hashes: Vec<[u8; 32]> = Vec::new();

    // Coinbase tx hash
    if let Some(ref cb) = template.coinbasetxn {
        if let Ok(bytes) = hex::decode(&cb.hash) {
            if bytes.len() == 32 {
                let mut h = [0u8; 32];
                h.copy_from_slice(&bytes);
                hashes.push(h);
            }
        }
    }

    // Other transaction hashes
    for tx in &template.transactions {
        if let Ok(bytes) = hex::decode(&tx.hash) {
            if bytes.len() == 32 {
                let mut h = [0u8; 32];
                h.copy_from_slice(&bytes);
                hashes.push(h);
            }
        }
    }

    if hashes.is_empty() {
        return "00".repeat(32);
    }

    // Build merkle tree
    while hashes.len() > 1 {
        if hashes.len() % 2 != 0 {
            let last = *hashes.last().unwrap();
            hashes.push(last);
        }
        let mut next = Vec::new();
        for pair in hashes.chunks(2) {
            let mut combined = Vec::with_capacity(64);
            combined.extend_from_slice(&pair[0]);
            combined.extend_from_slice(&pair[1]);
            next.push(sha256d(&combined));
        }
        hashes = next;
    }

    hex::encode(hashes[0])
}
