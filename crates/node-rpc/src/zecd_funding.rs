//! Standalone testnet-only read-side evidence for exact zecd 0.7.0.
//!
//! This neither verifies a fee contract nor creates a pool DB funding lease.
//! There is no wallet mutation or automatic dialect fallback.
//! The caller must independently pin the deployed binary/config and bracket
//! these reads with the ledger's payout generation before admitting liability.
use crate::{funding::exact_zatoshis, ZcashRpcClient};
use serde_json::{json, Value};
use std::{collections::HashSet, time::{Duration, SystemTime, UNIX_EPOCH}};

pub const MAX_RPC_BODY_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_UNSPENT_OUTPUTS: usize = 8192;
/// Must equal pool-db's FUNDING_LEASE_SECONDS (asserted at compile time in
/// pool-core). Collections take ~40 s against a large zecd wallet; a lifetime
/// of the same order made it impossible for admission to stay armed.
pub const EVIDENCE_LIFETIME_SECONDS: i64 = 600;
/// Collection remains bounded below the evidence's original-start lifetime.
/// Slow successful reads consume validity; completing collection never renews it.
pub const COLLECTION_TIMEOUT_SECONDS: u64 = 45;
/// The wallet tip may advance FORWARD by up to this many blocks during the
/// funding read. A testnet fast-block burst moves the tip every few seconds, so
/// requiring it to hold perfectly still across the multi-RPC read pauses all
/// crediting during such bursts. Forward-only progress is a chain extension:
/// the closing anchor is re-proven canonical against the node, the mature
/// (>=10-conf) eligible balance is unaffected by tip-level movement, and any
/// deep reorg makes zecd rewind (not ready), which the readiness gate rejects.
/// A backward move or a jump larger than this is rejected as suspect.
pub const FUNDING_ANCHOR_MAX_DRIFT: u64 = 24;
const MAX_MONEY: i64 = 21_000_000 * 100_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ZecdFundingError {
    #[error("testnet wallet RPC unavailable")]
    Unavailable,
    #[error("testnet wallet RPC unsupported")]
    UnsupportedRpc,
    #[error("testnet wallet evidence invalid")]
    InvalidEvidence,
    #[error("testnet wallet response exceeds collection limit")]
    ResponseTooLarge,
    #[error("testnet wallet enumeration exceeds collection limit")]
    EnumerationTooLarge,
    #[error("testnet wallet identity or network mismatch")]
    IdentityMismatch,
    #[error("testnet wallet is not ready")]
    NotReady,
    #[error("testnet identity-encrypted signer readiness is not proven")]
    IdentitySignerNotProven,
    #[error("testnet wallet source is not supported")]
    UnsupportedSource,
    #[error("testnet wallet changed during collection")]
    ConcurrentChange,
    #[error("testnet wallet canonical chain proof failed")]
    ChainMismatch,
    #[error("testnet wallet funding collection timed out")]
    Timeout,
    #[error("testnet actor probe returned the required absence")]
    ActorProbeMissing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignerReadiness { PassphraseUnlocked, IdentityOperationVerified }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSignerReadiness { PassphraseUnlocked, IdentityNotProven }

/// Owner-side read diagnostics are deliberately a different, opaque type from
/// credit-authorizing evidence. An identity wallet can expose confirmed funding
/// diagnostics without being mistaken for a proven working signer. No caller
/// can turn this into a funding proof by supplying a boolean or deserializing it.
pub struct ZecdFundingDiagnostics {
    confirmed_eligible_zatoshis: i64,
    checked_at_unix: i64,
    valid_until_unix: i64,
    signer_readiness: DiagnosticSignerReadiness,
}
impl ZecdFundingDiagnostics {
    pub fn confirmed_eligible_zatoshis(&self) -> i64 { self.confirmed_eligible_zatoshis }
    pub fn checked_at_unix(&self) -> i64 { self.checked_at_unix }
    pub fn valid_until_unix(&self) -> i64 { self.valid_until_unix }
    pub fn signer_readiness(&self) -> DiagnosticSignerReadiness { self.signer_readiness }
}
impl std::fmt::Debug for ZecdFundingDiagnostics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ZecdFundingDiagnostics { redacted }")
    }
}

/// Read-side evidence only, not a spend/fee authorization. Debug redacts all
/// values. Construction is private: identity-unproven responses cannot create
/// successful evidence. Zero is a valid verified amount, not funded approval.
#[derive(Clone, PartialEq, Eq)]
pub struct ZecdFundingEvidence {
    pub confirmed_eligible_zatoshis: i64,
    pub checked_at_unix: i64,
    pub valid_until_unix: i64,
    pub signer_readiness: SignerReadiness,
    _private: (),
}
impl std::fmt::Debug for ZecdFundingEvidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ZecdFundingEvidence { redacted }")
    }
}

#[derive(PartialEq, Eq)]
struct Anchor { height: u64, hash: String }

// Nonmonetary identity/readiness only. No balance can be carried across the
// slow historical signer proof through this type.
struct WalletEnvelope {
    name: String,
    readiness: DiagnosticSignerReadiness,
    anchor: Anchor,
}

// Invocation-local selected proof; never cached, serialized or logged.
struct IdentityOperationProof {
    candidate: Value,
    opid: String,
    height: u64,
    blockhash: String,
}

// Diagnostic context is opt-in and local to one collector future. No RPC
// response, amount, address, identity, or exception text can enter it.
tokio::task_local! {
    static PROBE_STAGE: std::cell::Cell<&'static str>;
    static SHARED_PROBE_STAGE: FundingStageObserver;
}
fn probe_stage(stage: &'static str) {
    let _ = PROBE_STAGE.try_with(|current| current.set(stage));
    let _ = SHARED_PROBE_STAGE.try_with(|observer| observer.set(stage));
}

/// One collector invocation's fixed-label progress, readable after cancellation
/// by an outer timeout. This contains no RPC data and is never authorization.
/// Use a fresh observer for each concurrent collector; setters remain private.
#[derive(Clone)]
pub struct FundingStageObserver(std::sync::Arc<std::sync::Mutex<&'static str>>);
impl Default for FundingStageObserver {
    fn default() -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new("collection_start")))
    }
}
impl FundingStageObserver {
    pub fn stage(&self) -> &'static str {
        self.0.lock().map(|stage| *stage).unwrap_or("collection_start")
    }
    fn set(&self, stage: &'static str) {
        if let Ok(mut current)=self.0.lock() { *current=stage; }
    }
}

/// Read-only diagnostic result, not a funding lease or signer capability.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct FundingProbeReport {
    pub passed: bool,
    pub stage: &'static str,
    pub category: &'static str,
}

/// Fixed failure metadata from the same collector invocation. No response or
/// exception text is retained, and no additional diagnostic RPC is performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FundingObservedError {
    pub error: ZecdFundingError,
    pub stage: &'static str,
    pub category: &'static str,
}

pub async fn collect_testnet_funding_observed(wallet: &ZcashRpcClient, source: &str,
    node: &ZcashRpcClient) -> Result<ZecdFundingEvidence, FundingObservedError> {
    collect_testnet_funding_observed_with_stage(wallet,source,node,&FundingStageObserver::default()).await
}

/// Same single collector invocation, with an owner-held progress observer for
/// outer cancellation. No retry, background task, or extra RPC is introduced.
pub async fn collect_testnet_funding_observed_with_stage(wallet: &ZcashRpcClient, source: &str,
    node: &ZcashRpcClient, observer: &FundingStageObserver)
    -> Result<ZecdFundingEvidence, FundingObservedError> {
    observer.set("collection_start");
    SHARED_PROBE_STAGE.scope(observer.clone(),PROBE_STAGE.scope(std::cell::Cell::new("collection_start"), async {
        collect_testnet_funding(wallet, source, node).await.map_err(|error| FundingObservedError {
            error, stage: PROBE_STAGE.with(|current| current.get()), category: probe_category(&error),
        })
    })).await
}

#[cfg(test)]
tokio::task_local! {
    static TEST_NOW: std::cell::Cell<i64>;
    static TEST_COLLECTION_TIMEOUT: Duration;
}

fn collection_timeout() -> Duration {
    #[cfg(test)]
    if let Ok(duration) = TEST_COLLECTION_TIMEOUT.try_with(|duration| *duration) { return duration; }
    Duration::from_secs(COLLECTION_TIMEOUT_SECONDS)
}

fn probe_category(error: &ZecdFundingError) -> &'static str {
    match error {
        ZecdFundingError::Unavailable => "rpc_unavailable",
        ZecdFundingError::UnsupportedRpc => "rpc_unsupported",
        ZecdFundingError::InvalidEvidence => "invalid_evidence",
        ZecdFundingError::ResponseTooLarge => "response_too_large",
        ZecdFundingError::EnumerationTooLarge => "enumeration_too_large",
        ZecdFundingError::IdentityMismatch => "identity_mismatch",
        ZecdFundingError::NotReady => "wallet_not_ready",
        ZecdFundingError::IdentitySignerNotProven => "identity_signer_not_proven",
        ZecdFundingError::UnsupportedSource => "unsupported_source",
        ZecdFundingError::ConcurrentChange => "concurrent_change",
        ZecdFundingError::ChainMismatch => "chain_mismatch",
        ZecdFundingError::Timeout => "deadline_exceeded",
        ZecdFundingError::ActorProbeMissing => "actor_probe_missing",
    }
}

async fn run_funding_probe<F>(future: F) -> FundingProbeReport
where F: std::future::Future<Output = Result<ZecdFundingEvidence, ZecdFundingError>> {
    PROBE_STAGE.scope(std::cell::Cell::new("collection_start"), async {
        match future.await {
            Ok(_) => FundingProbeReport { passed: true, stage: "complete", category: "passed" },
            Err(error) => FundingProbeReport { passed: false,
                stage: PROBE_STAGE.with(|current| current.get()),
                category: probe_category(&error) },
        }
    }).await
}

/// Executes exactly the normal bounded collection once and discards its private
/// evidence. The only additional behavior is fixed stage metadata. In
/// particular, this does not retry, relax proof checks, or renew timestamps.
pub async fn probe_testnet_funding(wallet: &ZcashRpcClient, source: &str,
    node: &ZcashRpcClient) -> FundingProbeReport {
    run_funding_probe(collect_testnet_funding(wallet, source, node)).await
}

fn now() -> Result<i64, ZecdFundingError> {
    #[cfg(test)]
    if let Ok(at) = TEST_NOW.try_with(|at| at.get()) { return Ok(at); }
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|_| ZecdFundingError::InvalidEvidence)?.as_secs();
    i64::try_from(secs).map_err(|_| ZecdFundingError::InvalidEvidence)
}
fn hash(value: &Value) -> Result<String, ZecdFundingError> {
    let s = value.as_str().ok_or(ZecdFundingError::InvalidEvidence)?;
    if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ZecdFundingError::InvalidEvidence);
    }
    Ok(s.to_ascii_lowercase())
}
fn amount(value: &Value) -> Result<i64, ZecdFundingError> {
    let n = value.as_number().ok_or(ZecdFundingError::InvalidEvidence)?;
    exact_zatoshis(&n.to_string()).map_err(|_| ZecdFundingError::InvalidEvidence)
}
fn checked_sum(a: i64, b: i64) -> Result<i64, ZecdFundingError> {
    a.checked_add(b).filter(|n| (0..=MAX_MONEY).contains(n))
        .ok_or(ZecdFundingError::InvalidEvidence)
}
fn identity(value: &Value) -> Result<(), ZecdFundingError> {
    // Accept zecd 0.7.0 (for rollback) and any 0.8.x (0.8.0-rc2 and later rc/final).
    // The daemon is ours; a version/subversion outside this supported set is treated
    // as a foreign or unexpected wallet and rejected.
    let version = value.get("version").and_then(Value::as_u64);
    let subversion = value.get("subversion").and_then(Value::as_str);
    let supported = matches!(version, Some(700) | Some(800))
        && subversion.is_some_and(|s| s == "/zecd:0.7.0/" || s.starts_with("/zecd:0.8."));
    if !supported { return Err(ZecdFundingError::IdentityMismatch); }
    Ok(())
}
fn anchor(value: &Value) -> Result<Anchor, ZecdFundingError> {
    if value.get("chain").and_then(Value::as_str) != Some("test") {
        return Err(ZecdFundingError::IdentityMismatch);
    }
    let height = value.get("blocks").and_then(Value::as_u64)
        .filter(|h| (1..=u32::MAX as u64).contains(h)).ok_or(ZecdFundingError::NotReady)?;
    if value.get("headers").and_then(Value::as_u64) != Some(height)
        || value.get("initialblockdownload").and_then(Value::as_bool) != Some(false)
    { return Err(ZecdFundingError::NotReady); }
    Ok(Anchor { height, hash: hash(value.get("bestblockhash").ok_or(ZecdFundingError::InvalidEvidence)?)? })
}
fn wallet_metadata(value: &Value, height: u64, until: i64)
    -> Result<(String, DiagnosticSignerReadiness), ZecdFundingError>
{
    // Under `[sync] fetch_memos = false` (zecd 0.8.x) the wallet records no memo
    // enhancement watermark, so `enhanced_through` is null by design. Readiness is
    // then proven by `scanning == false` (scanned to tip on a synced node). When
    // memos are fetched — or the field is absent (zecd 0.7.0) — keep the strict
    // `enhanced_through == height` currency proof.
    let memoless = value.get("fetch_memos").and_then(Value::as_bool) == Some(false);
    let enhanced_current = if memoless {
        matches!(value.get("enhanced_through"), None | Some(Value::Null))
            || value.get("enhanced_through").and_then(Value::as_u64) == Some(height)
    } else {
        value.get("enhanced_through").and_then(Value::as_u64) == Some(height)
    };
    if value.get("private_keys_enabled").and_then(Value::as_bool) != Some(true)
        || value.get("scanning").and_then(Value::as_bool) != Some(false)
        || !enhanced_current
        || value.get("walletversion").and_then(Value::as_u64) != Some(169900)
        || value.get("format").and_then(Value::as_str) != Some("sqlite")
    { return Err(ZecdFundingError::NotReady); }
    let name = value.get("walletname").and_then(Value::as_str)
        .filter(|n| !n.is_empty() && n.len() <= 128 && !n.chars().any(char::is_control))
        .ok_or(ZecdFundingError::InvalidEvidence)?;
    // Unlike Zallet's unencrypted case, zecd's identity-decrypt failure leaves
    // a readable daemon with this field absent. Never infer unlocked from it.
    let Some(unlock) = value.get("unlocked_until") else {
        return Ok((name.to_owned(), DiagnosticSignerReadiness::IdentityNotProven));
    };
    if until <= 0 || unlock.as_u64().filter(|u| *u >= until as u64).is_none() {
        return Err(ZecdFundingError::NotReady);
    }
    Ok((name.to_owned(), DiagnosticSignerReadiness::PassphraseUnlocked))
}

#[cfg(test)]
fn wallet_ready(value: &Value, height: u64, until: i64) -> Result<String, ZecdFundingError> {
    let (name, readiness) = wallet_metadata(value, height, until)?;
    if readiness == DiagnosticSignerReadiness::IdentityNotProven {
        return Err(ZecdFundingError::IdentitySignerNotProven);
    }
    Ok(name)
}
fn source_owned(value: &Value, source: &str) -> Result<(), ZecdFundingError> {
    if source.is_empty() || source.len() > 2048
        || value.get("address").and_then(Value::as_str) != Some(source)
        || value.get("ismine").and_then(Value::as_bool) != Some(true)
        || value.get("solvable").and_then(Value::as_bool) != Some(true)
        || value.get("iswatchonly").and_then(Value::as_bool) != Some(false)
        || value.get("isscript").and_then(Value::as_bool) != Some(false)
        || value.get("scriptPubKey").and_then(Value::as_str) != Some("")
    { return Err(ZecdFundingError::UnsupportedSource); }
    if value.get("receivers_consistent").is_some_and(|v| v.as_bool() != Some(true)) {
        return Err(ZecdFundingError::UnsupportedSource);
    }
    let kinds = value.get("receiver_types").and_then(Value::as_array)
        .filter(|v| !v.is_empty() && v.len() <= 2).ok_or(ZecdFundingError::UnsupportedSource)?;
    let mut seen = HashSet::new();
    for kind in kinds {
        let kind = kind.as_str().ok_or(ZecdFundingError::UnsupportedSource)?;
        if !matches!(kind, "sapling" | "orchard") || !seen.insert(kind) {
            return Err(ZecdFundingError::UnsupportedSource);
        }
    }
    Ok(())
}

/// listunspent's spendable=true is unconditional (including watch-only) and
/// its selector uses LockFilter::Unfiltered. Thus it is not a standalone
/// spendability oracle. The exact aggregate is the upper bound; subtracting
/// every disallowed unspent output is conservative, then cap by eligible
/// listed notes. This excludes ALL transparent funds, including coinbase,
/// plus Sapling which is outside the separately reviewed cached payout path.
fn eligible_balance(total: &Value, outputs: &Value) -> Result<i64, ZecdFundingError> {
    let total = amount(total)?;
    let rows = outputs.as_array().ok_or(ZecdFundingError::InvalidEvidence)?;
    if rows.len() > MAX_UNSPENT_OUTPUTS { return Err(ZecdFundingError::EnumerationTooLarge); }
    let (mut excluded, mut eligible, mut listed) = (0, 0, 0);
    let mut unique = HashSet::new();
    for row in rows {
        let pool = row.get("pool").and_then(Value::as_str).ok_or(ZecdFundingError::InvalidEvidence)?;
        if !matches!(pool, "transparent" | "sapling" | "orchard" | "ironwood") {
            return Err(ZecdFundingError::InvalidEvidence);
        }
        let txid = hash(row.get("txid").ok_or(ZecdFundingError::InvalidEvidence)?)?;
        let vout = row.get("vout").and_then(Value::as_u64)
            .filter(|v| *v <= u32::MAX as u64).ok_or(ZecdFundingError::InvalidEvidence)?;
        if !unique.insert((pool.to_owned(), txid, vout)) { return Err(ZecdFundingError::InvalidEvidence); }
        let value = amount(row.get("amount").ok_or(ZecdFundingError::InvalidEvidence)?)?;
        listed = checked_sum(listed, value)?;
        // Transparent and Sapling are never eligible (they are subtracted from
        // the total below). Count and move on.
        if matches!(pool, "transparent" | "sapling") {
            excluded = checked_sum(excluded, value)?;
            continue;
        }
        // An Orchard/Ironwood note counts toward eligible ONLY if it is mature
        // (>=10 confirmations) AND fully spendable. A note failing either -- e.g.
        // a fresh payout change note still under 10 confirmations -- is simply
        // not counted yet; it must NOT invalidate the whole funding evidence.
        // The old hard error paused ALL share admission for ~10 blocks after
        // every payout. Under-counting eligible is conservative: the lease still
        // requires eligible >= required_spendable, so this can never
        // over-authorize, only (harmlessly) undercount while a note matures.
        let mature = row.get("confirmations").and_then(Value::as_i64)
            .is_some_and(|d| (10..=u32::MAX as i64).contains(&d));
        let spendable_now = row.get("safe").and_then(Value::as_bool) == Some(true)
            && row.get("spendable").and_then(Value::as_bool) == Some(true)
            && row.get("solvable").and_then(Value::as_bool) == Some(true)
            && row.get("address").and_then(Value::as_str).filter(|a| a.len() <= 2048).is_some();
        if mature && spendable_now {
            eligible = checked_sum(eligible, value)?;
        }
    }
    let remainder = total.checked_sub(excluded).filter(|n| *n >= 0).ok_or(ZecdFundingError::InvalidEvidence)?;
    Ok(remainder.min(eligible))
}

/// Fixed read-only RPC sequence. The wallet and node are existing configured
/// clients; no new endpoint discovery, credentials, key use or writes occur.
/// Runtime pin/config and bounded-fee capability remain separate prerequisites.
pub async fn collect_testnet_funding(wallet: &ZcashRpcClient, source: &str, node: &ZcashRpcClient)
    -> Result<ZecdFundingEvidence, ZecdFundingError>
{
    tokio::time::timeout(collection_timeout(), async {
        let (checked_at, until, initial) = begin_funding(wallet, source).await?;
        // Signer readiness. A passphrase-unlocked wallet needs no further proof.
        // zecd 0.8.x exposes no `unlocked_until`, so an unencrypted/auto-unlock
        // wallet (resident spend authority) reports IdentityNotProven even though it
        // can sign. begin_funding has ALREADY proven spend capability for the source
        // via source_owned (getaddressinfo: ismine && solvable && !iswatchonly): a
        // wallet that could not derive the source's spending key reports
        // solvable=false and fails there, so IdentityNotProven reaching this point
        // means the source is genuinely spendable. The definitive spend gate remains
        // the payout send (z_sendmany at the seal), which fails closed if the wallet
        // truly cannot sign. We therefore do not require the slow ephemeral-operation
        // proof to issue credit — it depends on a recent confirmed payout being
        // present in zecd's per-session operation list, which empties on every
        // daemon restart and deadlocks funding. verify_identity_operation is retained
        // for a future passphrase-encrypted configuration.
        let (before, signer): (WalletEnvelope, Option<IdentityOperationProof>) =
            match initial.readiness {
                DiagnosticSignerReadiness::PassphraseUnlocked
                | DiagnosticSignerReadiness::IdentityNotProven => (initial, None),
            };
        let proof = collect_money(wallet,source,node,checked_at,until,before,signer.as_ref()).await?;
        let readiness = if signer.is_some() { SignerReadiness::IdentityOperationVerified }
            else { SignerReadiness::PassphraseUnlocked };
        probe_stage("evidence_final_freshness");
        let finished = now()?;
        if finished < proof.checked_at_unix || finished >= proof.valid_until_unix {
            return Err(ZecdFundingError::InvalidEvidence);
        }
        Ok(ZecdFundingEvidence { confirmed_eligible_zatoshis: proof.confirmed_eligible_zatoshis,
            checked_at_unix: proof.checked_at_unix, valid_until_unix: proof.valid_until_unix,
            signer_readiness: readiness, _private: () })
    }).await.map_err(|_| ZecdFundingError::Timeout)?
}

/// Read-only diagnostics for an operator preflight. Identity-mode funding can be
/// inspected, but this returns no funding lease and no signing authorization.
pub async fn collect_testnet_funding_diagnostics(wallet: &ZcashRpcClient, source: &str, node: &ZcashRpcClient)
    -> Result<ZecdFundingDiagnostics, ZecdFundingError>
{
    tokio::time::timeout(collection_timeout(), async {
        let (checked_at,until,before)=begin_funding(wallet,source).await?;
        collect_money(wallet,source,node,checked_at,until,before,None).await
    }).await.map_err(|_| ZecdFundingError::Timeout)?
}

async fn begin_funding(wallet: &ZcashRpcClient, source: &str)
    -> Result<(i64,i64,WalletEnvelope),ZecdFundingError>
{
    probe_stage("source_input");
    if source.is_empty() || source.len() > 2048 { return Err(ZecdFundingError::UnsupportedSource); }
        probe_stage("diagnostic_clock");
        let checked_at = now()?;
        let until = checked_at.checked_add(EVIDENCE_LIFETIME_SECONDS).ok_or(ZecdFundingError::InvalidEvidence)?;
        probe_stage("wallet_identity_read");
        let value=wallet.zecd_funding_read("getnetworkinfo", json!([])).await?;
        probe_stage("wallet_identity_check");
        identity(&value)?;
        // zecd samples this status after its expensive metadata balance read.
        // Capture it BEFORE the cheap anchor, so a block completed during that
        // read is not compared with an anchor sampled several seconds earlier.
        // Exact height matching still rejects any later or incomplete advance.
        probe_stage("opening_metadata_read");
        let opening_info=wallet.zecd_funding_read("getwalletinfo", json!([])).await?;
        probe_stage("opening_anchor_read");
        let value=wallet.zecd_funding_read("getblockchaininfo", json!([])).await?;
        probe_stage("opening_anchor_check");
        let before = anchor(&value)?;
        probe_stage("opening_metadata_check");
        let (before_name, before_signer) = wallet_metadata(&opening_info, before.height, until)?;
        probe_stage("opening_source_read");
        let value=wallet.zecd_funding_read("getaddressinfo", json!([source])).await?;
        probe_stage("opening_source_check");
        source_owned(&value, source)?;
        Ok((checked_at,until,WalletEnvelope {name:before_name,readiness:before_signer,anchor:before}))
}

async fn collect_money(wallet: &ZcashRpcClient, source: &str, node: &ZcashRpcClient,
    checked_at: i64, until: i64, before: WalletEnvelope, signer: Option<&IdentityOperationProof>)
    -> Result<ZecdFundingDiagnostics,ZecdFundingError>
{
        // Fixed concurrency two, entirely inside the unchanged wallet-anchor
        // bracket. Neither read consumes the other's result. Await BOTH even
        // on failure; no closing proof starts while either read is pending.
        // This does not claim that separate wallet RPCs are an atomic snapshot.
        probe_stage("balance_inventory_read");
        let (total,outputs) = tokio::join!(
            wallet.zecd_funding_read("getbalance", json!(["*", 10])),
            wallet.zecd_funding_read("listunspent", json!([10, u32::MAX, [], false])));
        // Preserve the previous balance-first error priority. Values are still
        // parsed only by the exact integer/conservative routine below.
        let total = total.map_err(|error| { probe_stage("balance_read"); error })?;
        let outputs = outputs.map_err(|error| { probe_stage("inventory_read"); error })?;
        probe_stage("inventory_check");
        let available = eligible_balance(&total, &outputs)?;
        if let Some(signer)=signer {
            // Historical receipt/status reads can survive an actor failure.
            // The private historical proof remains provisional until THIS
            // same liveness sentinel succeeds after the monetary reads.
            probe_stage("signer_actor_read");
            wallet.zecd_actor_live().await?;
            // Close the same selected invocation-scoped operation and source
            // inside the fresh monetary bracket, never choose a replacement.
            probe_stage("signer_final_operation_read");
            let final_ops=wallet.zecd_signer_read("z_getoperationstatus",json!([[signer.opid]])).await?;
            probe_stage("signer_final_operation_check");
            if final_ops.as_array().filter(|v|v.len()==1).and_then(|v|v.first()) != Some(&signer.candidate) {
                return Err(ZecdFundingError::ConcurrentChange);
            }
            probe_stage("signer_final_source_read");
            let value=wallet.zecd_funding_read("getaddressinfo",json!([source])).await?;
            probe_stage("signer_final_source_check");
            source_owned(&value,source)?;
        }
        // Also close AFTER the slow readiness read. Both metadata and the final
        // anchor must still match the opening anchor exactly; no old monetary
        // snapshot is accepted merely because the wallet advanced normally.
        probe_stage("closing_metadata_read");
        let closing_info=wallet.zecd_funding_read("getwalletinfo", json!([])).await?;
        probe_stage("closing_anchor_read");
        let value=wallet.zecd_funding_read("getblockchaininfo", json!([])).await?;
        probe_stage("closing_anchor_check");
        let after = anchor(&value)?;
        probe_stage("closing_metadata_check");
        let (after_name, after_signer) = wallet_metadata(&closing_info, after.height, until)?;
        probe_stage("funding_bracket_check");
        // Allow the tip to advance FORWARD during the read (fast-block bursts),
        // but reject any backward move or absurd jump, and require wallet
        // identity and signer readiness to hold. Forward progress is a chain
        // extension: `after` is re-proven canonical against the node below, the
        // mature eligible balance is unaffected by tip-level movement, and a
        // deep reorg would leave zecd rewinding (not ready) and fail readiness.
        let forward = after.height.checked_sub(before.anchor.height);
        if forward.map_or(true, |d| d > FUNDING_ANCHOR_MAX_DRIFT)
            || before.name != after_name
            || before.readiness != after_signer
        {
            return Err(ZecdFundingError::ConcurrentChange);
        }
        probe_stage("node_tip_read");
        let value=node.zecd_funding_read("getblockcount", json!([])).await?;
        probe_stage("node_tip_check");
        let node_height = value.as_u64()
            .filter(|h| *h <= u32::MAX as u64).ok_or(ZecdFundingError::InvalidEvidence)?;
        if node_height < after.height || node_height - after.height > 24 { return Err(ZecdFundingError::ChainMismatch); }
        probe_stage("funding_anchor_canonical_read");
        let canonical = node.zecd_funding_read("getblockhash", json!([after.height])).await?;
        probe_stage("funding_anchor_canonical_check");
        if hash(&canonical)? != after.hash { return Err(ZecdFundingError::ChainMismatch); }
        if let Some(signer)=signer {
            // The earlier signer proof is not enough if that historical block
            // became noncanonical while fresh money was read. Applies to both
            // transparent and wallet-history receipt paths.
            probe_stage("signer_closing_canonical_read");
            let canonical=node.zecd_signer_read("getblockhash",json!([signer.height])).await?;
            probe_stage("signer_closing_canonical_check");
            if hash(&canonical)? != signer.blockhash { return Err(ZecdFundingError::ChainMismatch); }
        }
        probe_stage("diagnostic_final_freshness");
        let finished = now()?;
        if finished < checked_at || finished >= until { return Err(ZecdFundingError::InvalidEvidence); }
        Ok(ZecdFundingDiagnostics { confirmed_eligible_zatoshis: available, checked_at_unix: checked_at,
            valid_until_unix: until, signer_readiness: after_signer })
}

fn operation_id(value: &Value) -> Result<&str, ZecdFundingError> {
    let id = value.as_str().ok_or(ZecdFundingError::InvalidEvidence)?;
    let bytes = id.as_bytes();
    if bytes.len() != 41 || !id.starts_with("opid-")
        || bytes[5..].iter().enumerate().any(|(i, b)| {
            if [8,13,18,23].contains(&i) { *b != b'-' }
            else { !b.is_ascii_digit() && !(b'a'..=b'f').contains(b) }
        })
    { return Err(ZecdFundingError::InvalidEvidence); }
    Ok(id)
}

fn operation_amounts(value: &Value, source: &str, at: i64)
    -> Result<Vec<(String, i64)>, ZecdFundingError>
{
    operation_id(value.get("id").ok_or(ZecdFundingError::InvalidEvidence)?)?;
    if value.get("status").and_then(Value::as_str) != Some("success")
        || value.get("method").and_then(Value::as_str) != Some("z_sendmany")
        || value.get("creation_time").and_then(Value::as_i64)
            .filter(|t| *t > 0 && *t <= at).is_none()
        || value.get("error").is_some_and(|e| !e.is_null())
    { return Err(ZecdFundingError::InvalidEvidence); }
    let params=value.get("params").ok_or(ZecdFundingError::InvalidEvidence)?;
    if params.get("fromaddress").and_then(Value::as_str) != Some(source)
        || params.get("minconf").and_then(Value::as_u64)
            .filter(|n| (1..=u32::MAX as u64).contains(n)).is_none()
    { return Err(ZecdFundingError::InvalidEvidence); }
    let outputs=params.get("amounts").and_then(Value::as_array)
        .filter(|r| !r.is_empty() && r.len() <= 100).ok_or(ZecdFundingError::InvalidEvidence)?;
    let mut amounts=std::collections::BTreeMap::<String,i64>::new();
    let mut total=0;
    for row in outputs {
        let address=row.get("address").and_then(Value::as_str)
            .filter(|s| !s.is_empty() && s.len() <= 512).ok_or(ZecdFundingError::InvalidEvidence)?;
        crate::zecd_conventional::validate_testnet_recipient(address)
            .map_err(|_| ZecdFundingError::InvalidEvidence)?;
        if row.get("memo").is_some_and(|m| m.as_str() != Some("")) {
            return Err(ZecdFundingError::InvalidEvidence);
        }
        let n=amount(row.get("amount").ok_or(ZecdFundingError::InvalidEvidence)?)?;
        if n <= 0 { return Err(ZecdFundingError::InvalidEvidence); }
        total=checked_sum(total,n)?;
        let item=amounts.entry(address.to_owned()).or_default();
        *item=checked_sum(*item,n)?;
    }
    Ok(amounts.into_iter().collect())
}

/// An identity seed remains resident for this wallet actor's lifetime in the
/// exact supported zecd version: neither walletlock nor timeout can remove it.
/// A successful operation retained in its transient, wallet-scoped registry,
/// plus a live actor and a node-confirmed matching transaction, therefore proves
/// this invocation has a working signer. This is NOT a payout settlement proof:
/// the historical inclusion-height surrogate may only authorize key readiness.
/// No operation is issued, consumed, retried or persistently trusted here.
/// This helper returns only the private provisional receipt portion; collection
/// requires the live actor sentinel AFTER the subsequent fresh money reads.
// Retained for a future passphrase-encrypted wallet configuration. Unused while the
// testnet wallet is unencrypted/auto-unlock (see the readiness match in
// collect_testnet_funding for why the ephemeral-operation proof is not required there).
#[allow(dead_code)]
async fn verify_identity_operation(wallet: &ZcashRpcClient, source: &str,
    node: &ZcashRpcClient, initial: &WalletEnvelope)
    -> Result<IdentityOperationProof, ZecdFundingError>
{
    probe_stage("signer_operations_read");
    let ops=wallet.zecd_signer_read("z_getoperationstatus",json!([])).await?;
    probe_stage("signer_operation_selection");
    let ops=ops.as_array().filter(|v| v.len() <= 1024).ok_or(ZecdFundingError::InvalidEvidence)?;
    // Screen locally, then discover at most three newest qualifying receipts.
    // Only an explicit node -5 absence may move to the next candidate. A
    // transport error, unconfirmed/malformed response or any failed positive
    // receipt proof stops collection: no fallback after selection, no cache.
    // The operation id resolves equal timestamps stably.
    let at=now()?;
    let mut candidates=ops.iter().filter_map(|o| {
        let amounts=operation_amounts(o,source,at).ok()?;
        let opid=operation_id(o.get("id")?).ok()?;
        let result=o.get("result")?.as_object()?;
        if result.len()!=1 { return None; }
        let txid=hash(result.get("txid")?).ok()?;
        let created=o.get("creation_time")?.as_i64()?;
        Some((o,amounts,opid,txid,created))
    }).collect::<Vec<_>>();
    candidates.sort_by(|a,b|b.4.cmp(&a.4).then_with(||a.2.cmp(b.2)));
    let mut selected=None;
    for candidate in candidates.into_iter().take(3) {
        probe_stage("signer_raw_read");
        let Some(raw)=node.zecd_signer_raw_lookup(&candidate.3).await? else {continue};
        probe_stage("signer_raw_check");
        if raw.get("confirmations").and_then(Value::as_i64).filter(|n| *n > 0).is_none()
            || hash(raw.get("txid").ok_or(ZecdFundingError::InvalidEvidence)?)? != candidate.3
        { return Err(ZecdFundingError::ChainMismatch); }
        selected=Some((candidate,raw)); break;
    }
    let ((candidate,amounts,opid,txid,_),raw)=selected
        .ok_or(ZecdFundingError::IdentitySignerNotProven)?;
    let blockhash=hash(raw.get("blockhash").ok_or(ZecdFundingError::InvalidEvidence)?)?;
    probe_stage("signer_block_read");
    let block=node.zecd_signer_read("getblock",json!([blockhash,1])).await?;
    probe_stage("signer_block_check");
    let height=block.get("height").and_then(Value::as_u64)
        .filter(|h| *h > 0 && *h <= initial.anchor.height)
        .ok_or(ZecdFundingError::ChainMismatch)?;
    let transactions=block.get("tx").and_then(Value::as_array)
        .filter(|v| !v.is_empty() && v.len() <= 100_000).ok_or(ZecdFundingError::InvalidEvidence)?;
    if hash(block.get("hash").ok_or(ZecdFundingError::InvalidEvidence)?)? != blockhash
        || block.get("confirmations").and_then(Value::as_i64).filter(|n| *n > 0).is_none()
        || transactions.iter().filter(|t| t.as_str()==Some(txid.as_str())).count() != 1
    { return Err(ZecdFundingError::ChainMismatch); }
    probe_stage("signer_canonical_read");
    let canonical=node.zecd_signer_read("getblockhash",json!([height])).await?;
    probe_stage("signer_canonical_check");
    if hash(&canonical)? != blockhash { return Err(ZecdFundingError::ChainMismatch); }
    let profile=crate::zecd_conventional::TestnetConventionalProfile::consensus_size_bound("testnet",100)
        .map_err(|_|ZecdFundingError::InvalidEvidence)?;
    let expected=crate::zecd_conventional::ConventionalPayoutExpectation::new(&profile,source,&amounts,height as u32)
        .map_err(|_|ZecdFundingError::InvalidEvidence)?;
    let raw_hex=raw.get("hex").and_then(Value::as_str).ok_or(ZecdFundingError::InvalidEvidence)?;
    if expected.requires_wallet_history() {
        // A shielded destination cannot be proven from public raw bytes alone.
        // Read only this already-selected receipt; missing/incomplete history
        // never falls back to another operation or becomes signing evidence.
        // This historical result is provisional. The actor sentinel is read
        // once, AFTER fresh monetary reads, because read-side wallet metadata
        // and the retained operation registry do not themselves prove liveness.
        probe_stage("signer_history_read");
        let history=wallet.zecd_signer_read("gettransaction",json!([txid])).await?;
        probe_stage("signer_history_check");
        validate_history_identity(&history,raw_hex,&txid,&blockhash)?;
        crate::zecd_conventional::verify_conventional_payout_with_wallet(
            raw_hex,&txid,&expected,&history)
            .map_err(|_|ZecdFundingError::IdentitySignerNotProven)?;
        // Close the new wallet-history read window against the independent
        // node's canonical chain; a mid-read reorg cannot prove this signer.
        probe_stage("signer_history_canonical_read");
        let canonical=node.zecd_signer_read("getblockhash",json!([height])).await?;
        probe_stage("signer_history_canonical_check");
        if hash(&canonical)? != blockhash { return Err(ZecdFundingError::ChainMismatch); }
    } else {
        probe_stage("signer_transparent_receipt_check");
        crate::zecd_conventional::verify_conventional_payout(raw_hex,&txid,&expected)
            .map_err(|_|ZecdFundingError::InvalidEvidence)?;
    }
    Ok(IdentityOperationProof { candidate:candidate.clone(),opid:opid.to_owned(),height,blockhash })
}

fn validate_history_identity(history: &Value, raw_hex: &str, txid: &str, blockhash: &str)
    -> Result<(), ZecdFundingError>
{
    let matched = history.is_object()
        && history.get("confirmations").and_then(Value::as_i64).is_some_and(|n|n>0)
        && history.get("txid").and_then(|v|hash(v).ok()).as_deref()==Some(txid)
        && history.get("blockhash").and_then(|v|hash(v).ok()).as_deref()==Some(blockhash)
        && history.get("hex").and_then(Value::as_str).is_some_and(|hex|
            !hex.is_empty() && hex.len()<=4_000_000 && hex.len()%2==0
                && hex.bytes().all(|b|b.is_ascii_hexdigit()) && hex.eq_ignore_ascii_case(raw_hex));
    if matched { Ok(()) } else { Err(ZecdFundingError::IdentitySignerNotProven) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    fn tip(h: u64, c: &str) -> Value { json!({"chain":"test","blocks":h,"headers":h,
        "initialblockdownload":false,"bestblockhash":c.repeat(64)}) }
    fn info(h: u64) -> Value { json!({"walletname":"synthetic-wallet","walletversion":169900,
        "format":"sqlite","private_keys_enabled":true,"scanning":false,"enhanced_through":h,
        "unlocked_until":now().unwrap()+3600}) }
    fn source() -> Value { json!({"address":"synthetic-source","ismine":true,"solvable":true,
        "iswatchonly":false,"isscript":false,"scriptPubKey":"","receiver_types":["orchard"]}) }
    fn note(pool: &str, n: u64, amount: &str) -> Value {
        let mut row=json!({"pool":pool,"txid":format!("{n:064x}"),"vout":0,"address":"",
            "amount":serde_json::from_str::<Value>(amount).unwrap(),"confirmations":10,
            "safe":true,"spendable":true,"solvable":true});
        if pool=="transparent" { row["generated"]=json!(true); }
        row
    }
    fn responses() -> Vec<Value> { vec![json!({"version":700,"subversion":"/zecd:0.7.0/"}),
        tip(100,"a"),info(100),source(),json!(4),
        json!([note("transparent",1,"1"),note("sapling",2,"1"),note("orchard",3,"1"),note("ironwood",4,"1")]),
        tip(100,"a"),info(100)] }

    async fn mock_request(stream: &mut tokio::net::TcpStream) -> Value {
        let mut bytes=Vec::new();
        let mut buffer=[0_u8;4096];
        loop {
            let n=stream.read(&mut buffer).await.unwrap(); assert!(n>0);
            bytes.extend_from_slice(&buffer[..n]); assert!(bytes.len()<65536);
            if let Some(end)=bytes.windows(4).position(|w|w==b"\r\n\r\n") {
                let headers=std::str::from_utf8(&bytes[..end]).unwrap();
                let length:usize=headers.lines().find_map(|line| {
                    let (name,value)=line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length").then(||value.trim().parse().unwrap())
                }).unwrap();
                if bytes.len()>=end+4+length {
                    return serde_json::from_slice::<Value>(&bytes[end+4..end+4+length]).unwrap();
                }
            }
        }
    }

    async fn mock_reply(stream: &mut tokio::net::TcpStream, request:&Value,result:&Value) {
        if let Some(method)=result.get("mock_expected_method") {
            assert_eq!(&request["method"],method,"fixed RPC order changed");
        }
        if let Some(delay)=result.get("mock_delay_ms").and_then(Value::as_u64) {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        let result=result.get("mock_result").unwrap_or(result);
        let body=if result.get("mock_error").is_some() {
            json!({"id":request["id"],"result":null,"error":result["mock_error"]})
        } else { json!({"id":request["id"],"result":result,"error":null}) }.to_string();
        let status=if result.get("mock_error").is_some() {"500 Internal Server Error"} else {"200 OK"};
        stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
    }

    // Shielded history is now sequential and provisional; the SAME actor
    // sentinel is deferred until after the fresh monetary pair.
    async fn shielded_mock(mut results:Vec<Value>,reverse:bool)
        -> (ZcashRpcClient,tokio::task::JoinHandle<Vec<Value>>)
    {
        if results.len()>=8 {
            results[6]=json!({"mock_reverse_money":reverse,"mock_money_result":results[6]});
        }
        mock(results,None).await
    }

    /// Harness for the RETAINED identity-operation proof. Since 95862e8
    /// `collect_testnet_funding` no longer calls `verify_identity_operation`
    /// (an IdentityNotProven wallet proceeds straight to the money reads), but
    /// the function and `collect_money`'s signer branch are kept for a future
    /// passphrase-encrypted configuration. This composes exactly those retained
    /// production pieces — begin_funding, verify_identity_operation, then
    /// collect_money with the proof — so their own logic stays covered. It adds
    /// no re-envelope read (that was removed from production with the wiring).
    async fn collect_with_identity_proof(wallet: &ZcashRpcClient, source: &str, node: &ZcashRpcClient)
        -> Result<ZecdFundingEvidence, ZecdFundingError>
    {
        tokio::time::timeout(collection_timeout(), async {
            let (checked_at,until,initial)=begin_funding(wallet,source).await?;
            let signer=match initial.readiness {
                DiagnosticSignerReadiness::PassphraseUnlocked => None,
                DiagnosticSignerReadiness::IdentityNotProven =>
                    Some(verify_identity_operation(wallet,source,node,&initial).await?),
            };
            let proof=collect_money(wallet,source,node,checked_at,until,initial,signer.as_ref()).await?;
            let readiness=if signer.is_some() {SignerReadiness::IdentityOperationVerified}
                else {SignerReadiness::PassphraseUnlocked};
            probe_stage("evidence_final_freshness");
            let finished=now()?;
            if finished < proof.checked_at_unix || finished >= proof.valid_until_unix {
                return Err(ZecdFundingError::InvalidEvidence);
            }
            Ok(ZecdFundingEvidence { confirmed_eligible_zatoshis: proof.confirmed_eligible_zatoshis,
                checked_at_unix: proof.checked_at_unix, valid_until_unix: proof.valid_until_unix,
                signer_readiness: readiness, _private: () })
        }).await.map_err(|_| ZecdFundingError::Timeout)?
    }

    async fn identity_proof_observed_with_stage(wallet: &ZcashRpcClient, source: &str,
        node: &ZcashRpcClient, observer: &FundingStageObserver)
        -> Result<ZecdFundingEvidence, FundingObservedError>
    {
        observer.set("collection_start");
        SHARED_PROBE_STAGE.scope(observer.clone(),PROBE_STAGE.scope(std::cell::Cell::new("collection_start"), async {
            collect_with_identity_proof(wallet,source,node).await.map_err(|error| FundingObservedError {
                error, stage: PROBE_STAGE.with(|current| current.get()), category: probe_category(&error),
            })
        })).await
    }

    async fn identity_proof_observed(wallet: &ZcashRpcClient, source: &str, node: &ZcashRpcClient)
        -> Result<ZecdFundingEvidence, FundingObservedError>
    {
        identity_proof_observed_with_stage(wallet,source,node,&FundingStageObserver::default()).await
    }

    fn methods(reads: &[Value]) -> Vec<&str> {
        reads.iter().map(|r| r["method"].as_str().unwrap()).collect()
    }

    // Private ephemeral loopback mocks only; never a real node/wallet.
    async fn mock(results: Vec<Value>, raw_http: Option<Vec<u8>>) -> (ZcashRpcClient, tokio::task::JoinHandle<Vec<Value>>) {
        mock_with_money_ready(results,raw_http,None).await
    }

    async fn mock_with_money_ready(mut results: Vec<Value>, raw_http: Option<Vec<u8>>,
        mut money_ready: Option<tokio::sync::oneshot::Sender<()>>)
        -> (ZcashRpcClient,tokio::task::JoinHandle<Vec<Value>>) {
        // Fixtures retain their named logical anchor/metadata slots. Map only
        // the two explicitly changed read orders before serving any request.
        // Identity fixtures are explicitly in actual signer-first order; they
        // are not positional legacy diagnostic fixtures.
        let signer_first=results.first().is_some_and(|r|r["mock_signer_first"]==true);
        // Funding fixtures are recognised by a supported zecd identity reply.
        let funding_fixture=results.first().and_then(|r|r.get("version"))
            .is_some_and(|v| *v==json!(700) || *v==json!(800));
        if !signer_first && results.len() >= 3 && funding_fixture {
            results.swap(1,2);
            if results.len() >= 8 { results.swap(6,7); }
        }
        // Explicitly migrate the existing funding fixture's two monetary
        // response positions to method-routed concurrent responses. Inputs and
        // expected values stay unchanged; the server insists BOTH exact RPCs
        // arrive before replying to either, and forbids an early closing read.
        let money_index=if signer_first {if results[0]["mock_shielded"]==true {6} else {5}} else {4};
        if results.len() >= money_index+2 && funding_fixture {
            let reverse=results[money_index].get("mock_reverse_money").and_then(Value::as_bool).unwrap_or(false);
            let total=results[money_index].get("mock_money_result").unwrap_or(&results[money_index]).clone();
            let pair=json!({"mock_parallel":[
                {"method":"getbalance","params":["*",10],"result":total},
                {"method":"listunspent","params":[10,u32::MAX,[],false],"result":results[money_index+1]}
            ],"reverse":reverse,"hold_last":results[money_index].get("mock_hold_money")==Some(&json!(true))});
            results.splice(money_index..money_index+2,[pair]);
        }
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr=listener.local_addr().unwrap();
        let task=tokio::spawn(async move {
            let mut requests=Vec::new();
            for result in results {
                if let Some(routes)=result.get("mock_parallel").and_then(Value::as_array) {
                    assert_eq!(routes.len(),2);
                    let mut pending=Vec::new();
                    for _ in 0..2 {
                        let (mut stream,_)=tokio::time::timeout(Duration::from_secs(2),listener.accept())
                            .await.expect("both bounded reads must be issued concurrently").unwrap();
                        let request=mock_request(&mut stream).await;
                        let index=routes.iter().position(|r|r["method"]==request["method"]
                            && r["params"]==request["params"]).expect("unexpected concurrent RPC");
                        assert!(pending.iter().all(|(_,_,i)|*i!=index));
                        requests.push(request.clone()); pending.push((stream,request,index));
                    }
                    pending.sort_by_key(|(_,_,i)|*i);
                    if result["reverse"]==true { pending.reverse(); }
                    let (mut first,request,index)=pending.remove(0);
                    mock_reply(&mut first,&request,&routes[index]["result"]).await;
                    // No final operation/metadata read may start while the
                    // other member of the pair still lacks its response.
                    assert!(tokio::time::timeout(Duration::from_millis(15),listener.accept()).await.is_err());
                    let (mut last,request,index)=pending.remove(0);
                    if result["hold_last"]==true {
                        if let Some(ready)=money_ready.take() { ready.send(()).unwrap(); }
                        // Actual collector timeout must cancel the outstanding
                        // HTTP request, not detach a task that can finish later.
                        let mut byte=[0_u8;1];
                        let read=tokio::time::timeout(Duration::from_secs(60),last.read(&mut byte))
                            .await.expect("collector cancellation must close pending request").unwrap();
                        assert_eq!(read,0);
                        assert!(tokio::time::timeout(Duration::from_millis(15),listener.accept()).await.is_err());
                    } else {
                        mock_reply(&mut last,&request,&routes[index]["result"]).await;
                    }
                    continue;
                }
                // A scripted reply the collector never requests fails loudly
                // instead of hanging the test run.
                let (mut stream,_)=tokio::time::timeout(Duration::from_secs(10),listener.accept()).await
                    .unwrap_or_else(|_| panic!("scripted RPC never requested; issued so far: {:?}",
                        requests.iter().map(|r:&Value|r["method"].clone()).collect::<Vec<_>>()))
                    .unwrap();
                let request=mock_request(&mut stream).await;
                if let Some(raw)=&raw_http {
                    let _=stream.write_all(raw).await;
                } else {
                    mock_reply(&mut stream,&request,&result).await;
                }
                requests.push(request);
            }
            requests
        });
        let http=reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(2)).build().unwrap();
        (ZcashRpcClient::with_transport(&format!("http://{addr}"),None,http),task)
    }

    #[test]
    fn collection_timeout_preserves_original_evidence_lifetime() {
        const { assert!(COLLECTION_TIMEOUT_SECONDS < EVIDENCE_LIFETIME_SECONDS as u64); }
        assert_eq!(COLLECTION_TIMEOUT_SECONDS,45);
        assert_eq!(EVIDENCE_LIFETIME_SECONDS,600);
    }

    #[tokio::test]
    async fn opening_metadata_can_finish_one_block_later_before_exact_money_bracket() {
        // The former order samples H before this slow metadata read returns
        // H+1. That comparison fails, although the wallet is fully ready at H+1.
        assert_eq!(wallet_metadata(&info(101),100,now().unwrap()+60),Err(ZecdFundingError::NotReady));
        let mut r=responses();
        r[1]=tip(101,"b");
        r[2]=json!({"mock_expected_method":"getwalletinfo","mock_delay_ms":30,"mock_result":info(101)});
        r[6]=tip(101,"b"); r[7]=info(101);
        let (wallet,w)=mock(r,None).await;
        let (node,n)=mock(vec![json!(101),json!("b".repeat(64))],None).await;
        let started=std::time::Instant::now();
        let proof=collect_testnet_funding_observed(&wallet,"synthetic-source",&node).await.unwrap();
        assert!(started.elapsed()>=Duration::from_millis(30));
        assert_eq!(proof.confirmed_eligible_zatoshis,200_000_000);
        let requests=w.await.unwrap();
        assert_eq!(requests.len(),8);
        assert_eq!(requests[1]["method"],"getwalletinfo");
        assert_eq!(requests[2]["method"],"getblockchaininfo");
        assert_eq!(requests[6]["method"],"getwalletinfo");
        assert_eq!(requests[7]["method"],"getblockchaininfo");
        assert_eq!(n.await.unwrap()[1]["params"],json!([101]));
    }

    #[tokio::test]
    async fn reordered_reads_do_not_accept_later_anchor_reorg_backlog_locked_or_watchonly() {
        for case in 0..7 {
            let mut r=responses();
            let (expected,stage)=match case {
                0 => { r[1]=tip(101,"b"); r.truncate(3);
                    (ZecdFundingError::NotReady,"opening_metadata_check") },
                1 => { r[6]=tip(130,"b"); r[7]=info(130);
                    (ZecdFundingError::ConcurrentChange,"funding_bracket_check") },
                2 => { r[6]=tip(99,"b"); r[7]=info(99);
                    (ZecdFundingError::ConcurrentChange,"funding_bracket_check") },
                3 => { r[7]["scanning"]=json!({"pending_enhancements":1});
                    (ZecdFundingError::NotReady,"closing_metadata_check") },
                4 => { r[2]["private_keys_enabled"]=json!(false); r.truncate(3);
                    (ZecdFundingError::NotReady,"opening_metadata_check") },
                5 => { r[2]["unlocked_until"]=json!(0); r.truncate(3);
                    (ZecdFundingError::NotReady,"opening_metadata_check") },
                _ => { r[7]["unlocked_until"]=json!(0);
                    (ZecdFundingError::NotReady,"closing_metadata_check") },
            };
            let expected_reads=r.len();
            let (wallet,w)=mock(r,None).await; let (node,n)=mock(vec![],None).await;
            let error=collect_testnet_funding_observed(&wallet,"synthetic-source",&node).await.unwrap_err();
            assert_eq!(error.error,expected); assert_eq!(error.stage,stage);
            assert_eq!(error.category,probe_category(&expected));
            assert_eq!(w.await.unwrap().len(),expected_reads);
            assert!(n.await.unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn actual_collector_uses_original_clock_and_rejects_expiry_or_clock_rollback() {
        for finished in [159,100+EVIDENCE_LIFETIME_SECONDS,99] {
            let mut r=responses();
            r[2]=json!({"mock_expected_method":"getwalletinfo","mock_delay_ms":30,"mock_result":info(100)});
            let (wallet,w)=mock(r,None).await;
            let (node,n)=mock(vec![json!(100),json!("a".repeat(64))],None).await;
            let outcome=TEST_NOW.scope(std::cell::Cell::new(100),async {
                let (outcome,())=tokio::join!(
                    collect_testnet_funding_observed(&wallet,"synthetic-source",&node),
                    async {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                        TEST_NOW.with(|clock|clock.set(finished));
                    });
                outcome
            }).await;
            if finished==159 {
                let proof=outcome.unwrap();
                assert_eq!(proof.checked_at_unix,100);
                assert_eq!(proof.valid_until_unix,100+EVIDENCE_LIFETIME_SECONDS);
            } else {
                let error=outcome.unwrap_err();
                assert_eq!(error.error,ZecdFundingError::InvalidEvidence);
                assert_eq!(error.stage,"diagnostic_final_freshness");
            }
            assert_eq!(w.await.unwrap().len(),8); assert_eq!(n.await.unwrap().len(),2);
        }
    }

    #[tokio::test]
    async fn actual_collector_timeout_cancels_pending_http_and_does_not_close_bracket() {
        let mut r=responses(); r.truncate(6);
        r[4]=json!({"mock_hold_money":true,"mock_money_result":r[4]});
        // Each successful read stays below the unchanged 15s RPC timeout;
        // together they consume 36s of the REAL 45s collector budget. Holding
        // the next RPC then tests the collector deadline before its own 15s
        // deadline. There is no 100ms startup/CPU-speed assumption.
        for index in [0,1,2,3] {
            r[index]=json!({"mock_delay_ms":9_000,"mock_result":r[index]});
        }
        // Preserve the fixture discriminator consumed by the mock's explicit
        // anchor/metadata read-order mapping; only mock_result is sent by RPC.
        r[0]["version"]=json!(700);
        let (ready,ready_rx)=tokio::sync::oneshot::channel();
        let (wallet,w)=mock_with_money_ready(r,None,Some(ready)).await;
        let (node,n)=mock(vec![],None).await;
        let outcome={
            let collector=collect_testnet_funding_observed(&wallet,"synthetic-source",&node);
            tokio::pin!(collector);
            tokio::select! {
                ready=ready_rx=>ready.expect("both money RPCs must be pending before deadline"),
                result=&mut collector=>panic!("collector ended before ready barrier: {result:?}"),
            }
            collector.await
        };
        assert_eq!(outcome.unwrap_err(),FundingObservedError {error:ZecdFundingError::Timeout,
            stage:"balance_inventory_read",category:"deadline_exceeded"});
        let requests=w.await.unwrap();
        assert_eq!(requests.len(),6);
        assert_eq!(requests.iter().filter(|r|r["method"]=="getbalance").count(),1);
        assert_eq!(requests.iter().filter(|r|r["method"]=="listunspent").count(),1);
        assert!(n.await.unwrap().is_empty());
        assert!(PROBE_STAGE.try_with(|_|()).is_err());
        assert_eq!(collection_timeout(),Duration::from_secs(45));
    }

    #[tokio::test]
    async fn actual_outer_timeout_keeps_pending_node_stage_and_cancels_the_same_http_read() {
        let mut r=responses(); r.truncate(6);
        r[4]=json!({"mock_hold_money":true,"mock_money_result":r[4]});
        let (ready,ready_rx)=tokio::sync::oneshot::channel();
        let (wallet,w)=mock_with_money_ready(r,None,Some(ready)).await;
        let (node,n)=mock(vec![],None).await;
        let observer=FundingStageObserver::default();
        assert_eq!(observer.stage(),"collection_start");
        // Only expire the caller deadline AFTER the server has both exact
        // money requests and is holding its response. The production inner
        // timeout stays 45s, independent of parallel test-runner startup load.
        let result={
            let collector=collect_testnet_funding_observed_with_stage(
                &wallet,"synthetic-source",&node,&observer);
            tokio::pin!(collector);
            tokio::select! {
                ready=ready_rx=>ready.expect("money read ready barrier"),
                result=&mut collector=>panic!("collector ended before ready barrier: {result:?}"),
            }
            assert_eq!(observer.stage(),"balance_inventory_read");
            // Dropping this block also drops the pinned collector itself;
            // dropping merely a Pin<&mut Future> would leave the request alive.
            tokio::time::timeout(Duration::ZERO,collector).await
        };
        assert!(result.is_err());
        assert_eq!(observer.stage(),"balance_inventory_read");
        assert_eq!(w.await.unwrap().len(),6);
        assert!(n.await.unwrap().is_empty());
        assert!(PROBE_STAGE.try_with(|_|()).is_err());
        assert!(SHARED_PROBE_STAGE.try_with(|_|()).is_err());
        assert_eq!(collection_timeout(),Duration::from_secs(45));
        assert_eq!(EVIDENCE_LIFETIME_SECONDS,600);
    }

    #[tokio::test]
    async fn money_pair_requires_both_reads_and_closes_only_after_both_response_orders() {
        for reverse in [false,true] {
            let mut responses=responses();
            responses[4]=json!({"mock_reverse_money":reverse,"mock_money_result":responses[4]});
            let (wallet,w)=mock(responses,None).await;
            let (node,n)=mock(vec![json!(100),json!("a".repeat(64))],None).await;
            let at=now().unwrap();
            let evidence=collect_testnet_funding(&wallet,"synthetic-source",&node).await.unwrap();
            assert_eq!(evidence.confirmed_eligible_zatoshis,200_000_000);
            assert!(evidence.checked_at_unix>=at && evidence.checked_at_unix<=now().unwrap());
            assert_eq!(evidence.valid_until_unix,evidence.checked_at_unix+EVIDENCE_LIFETIME_SECONDS);
            let reads=w.await.unwrap();
            assert_eq!(reads.len(),8);
            assert_eq!(reads[6]["method"],"getwalletinfo");
            assert_eq!(reads[7]["method"],"getblockchaininfo");
            assert_eq!(reads.iter().filter(|v|v["method"]=="getbalance").count(),1);
            assert_eq!(reads.iter().filter(|v|v["method"]=="listunspent").count(),1);
            assert_eq!(n.await.unwrap().len(),2);
        }
    }

    #[tokio::test]
    async fn money_pair_keeps_original_balance_rpc_error_priority_and_requires_inventory() {
        for case in 0..4 {
            for reverse in [false,true] {
                let mut r=responses(); r.truncate(6);
                let (category,stage)=match case {
                    0 => { r[4]=json!({"mock_error":{"code":-32601,"message":"synthetic balance detail"}});
                        ("rpc_unsupported","balance_read") },
                    1 => { r[5]=json!({"mock_error":{"code":-1,"message":"synthetic inventory detail"}});
                        ("rpc_unavailable","inventory_read") },
                    2 => {
                        r[4]=json!({"mock_error":{"code":-32601,"message":"synthetic balance detail"}});
                        r[5]=json!({"mock_error":{"code":-1,"message":"synthetic inventory detail"}});
                        ("rpc_unsupported","balance_read")
                    },
                    _ => { r[4]=json!("synthetic malformed amount");
                        r[5]=json!({"mock_error":{"code":-1,"message":"synthetic inventory detail"}});
                        // As before, an inventory RPC failure precedes parsing
                        // a successfully received but malformed balance value.
                        ("rpc_unavailable","inventory_read") },
                };
                r[4]=json!({"mock_reverse_money":reverse,"mock_money_result":r[4]});
                let (wallet,w)=mock(r,None).await; let (node,n)=mock(vec![],None).await;
                let report=probe_testnet_funding(&wallet,"synthetic-source",&node).await;
                assert_eq!(report,FundingProbeReport {passed:false,stage,category});
                assert_eq!(w.await.unwrap().len(),6);
                assert!(n.await.unwrap().is_empty());
            }
        }
    }

    #[tokio::test]
    async fn money_pair_preserves_exact_anchor_scanning_canonical_and_amount_gates() {
        for case in 0..5 {
            let mut r=responses(); let mut node_responses=vec![];
            let expected=match case {
                // A forward jump beyond the drift tolerance, and a backward
                // move (a reorg shortening), are both rejected at the bracket.
                0 => { r[6]=tip(125,"b"); r[7]=info(125); ZecdFundingError::ConcurrentChange },
                1 => { r[6]=tip(99,"b"); r[7]=info(99); ZecdFundingError::ConcurrentChange },
                2 => { r[7]["scanning"]=json!({"progress":1.0,"pending_enhancements":1}); ZecdFundingError::NotReady },
                3 => { node_responses=vec![json!(100),json!("b".repeat(64))]; ZecdFundingError::ChainMismatch },
                _ => { r[4]=Value::Null; r.truncate(6); ZecdFundingError::InvalidEvidence },
            };
            let (wallet,w)=mock(r,None).await; let (node,n)=mock(node_responses,None).await;
            assert_eq!(collect_testnet_funding(&wallet,"synthetic-source",&node).await,Err(expected));
            w.await.unwrap(); n.await.unwrap();
        }
    }

    #[tokio::test]
    async fn money_pair_deadline_drops_both_futures_without_new_lifetime_or_spawn() {
        use std::sync::{Arc,atomic::{AtomicUsize,Ordering}};
        struct Guard(Arc<AtomicUsize>);
        impl Drop for Guard { fn drop(&mut self) { self.0.fetch_add(1,Ordering::SeqCst); } }
        async fn pending_read(counter:Arc<AtomicUsize>) -> Result<Value,ZecdFundingError> {
            let _guard=Guard(counter);
            std::future::pending().await
        }
        let counter=Arc::new(AtomicUsize::new(0));
        let report=run_funding_probe(async {
            probe_stage("balance_inventory_read");
            let _ = tokio::time::timeout(Duration::from_millis(1),async {
                tokio::join!(pending_read(counter.clone()),pending_read(counter.clone()))
            }).await.map_err(|_|ZecdFundingError::Timeout)?;
            unreachable!()
        }).await;
        assert_eq!(report,FundingProbeReport {passed:false,stage:"balance_inventory_read",category:"deadline_exceeded"});
        assert_eq!(counter.load(Ordering::SeqCst),2);
        assert_eq!(COLLECTION_TIMEOUT_SECONDS,45);
        assert_eq!(EVIDENCE_LIFETIME_SECONDS,600);
    }

    #[tokio::test]
    async fn stage_probe_is_task_local_bounded_and_does_not_return_evidence() {
        probe_stage("outside_probe");
        assert!(PROBE_STAGE.try_with(|_| ()).is_err());
        let (first,second)=tokio::join!(
            run_funding_probe(async {
                probe_stage("opening_anchor_check");
                tokio::task::yield_now().await;
                Err(ZecdFundingError::NotReady)
            }),
            run_funding_probe(async {
                probe_stage("inventory_read");
                tokio::task::yield_now().await;
                Err(ZecdFundingError::Unavailable)
            }));
        assert_eq!(first,FundingProbeReport {passed:false,stage:"opening_anchor_check",category:"wallet_not_ready"});
        assert_eq!(second,FundingProbeReport {passed:false,stage:"inventory_read",category:"rpc_unavailable"});
        let timeout=run_funding_probe(async {
            probe_stage("signer_history_actor_read");
            tokio::time::timeout(Duration::from_millis(1), std::future::pending::<()>()).await
                .map_err(|_|ZecdFundingError::Timeout)?;
            unreachable!()
        }).await;
        assert_eq!(timeout.stage,"signer_history_actor_read");
        assert_eq!(timeout.category,"deadline_exceeded");
        let success=run_funding_probe(async { Ok(ZecdFundingEvidence {
            confirmed_eligible_zatoshis:123456789,checked_at_unix:111,
            valid_until_unix:171,signer_readiness:SignerReadiness::PassphraseUnlocked,_private:()
        }) }).await;
        assert_eq!(serde_json::to_value(success).unwrap(),json!({"passed":true,"stage":"complete","category":"passed"}));
        assert!(PROBE_STAGE.try_with(|_| ()).is_err());
    }

    #[tokio::test]
    async fn stage_probe_identifies_exact_normal_scanning_boundaries_without_extra_reads() {
        for (index,change,stage) in [
            (1,"anchor","opening_anchor_check"),
            (2,"metadata","opening_metadata_check"),
            (6,"anchor","closing_anchor_check"),
            (7,"metadata","closing_metadata_check"),
        ] {
            let mut r=responses();
            if change=="anchor" { r[index]["initialblockdownload"]=json!(true); }
            else { r[index]["scanning"]=json!({"progress":1.0,"pending_enhancements":1}); }
            let reads=if index<3 {3} else {8};
            r.truncate(reads);
            let (wallet,w)=mock(r,None).await;
            let (node,n)=mock(vec![],None).await;
            let report=probe_testnet_funding(&wallet,"synthetic-source",&node).await;
            assert_eq!(report,FundingProbeReport {passed:false,stage,category:"wallet_not_ready"});
            assert_eq!(w.await.unwrap().len(),reads);
            assert!(n.await.unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn stage_probe_exposes_moving_final_height_and_actor_without_acceptance_change() {
        for actor_failure in [false,true] {
            let (source,mut wallet_responses,mut node_responses)=identity_receipt_responses();
            if actor_failure {
                wallet_responses[7]=json!({"mock_error":{"code":-1,"message":"synthetic private detail"}});
                wallet_responses.truncate(8);
            } else {
                // The closing metadata finished one block past the closing anchor.
                let h=wallet_responses[10]["enhanced_through"].as_u64().unwrap();
                wallet_responses[10]["enhanced_through"]=json!(h+1);
            }
            node_responses.truncate(3);
            let (wallet,w)=mock(wallet_responses,None).await;
            let (node,n)=mock(node_responses,None).await;
            let report=run_funding_probe(collect_with_identity_proof(&wallet,&source,&node)).await;
            assert_eq!(report,FundingProbeReport {passed:false,
                stage:if actor_failure {"signer_actor_read"} else {"closing_metadata_check"},
                category:"wallet_not_ready"});
            assert!(!format!("{report:?}").contains("synthetic private detail"));
            w.await.unwrap(); n.await.unwrap();
        }
    }

    #[test]
    fn conservative_balance_excludes_pending_watchonly_and_nonroute_pools() {
        let rows=json!([note("transparent",1,"1"),note("sapling",2,"2"),note("orchard",3,"3"),note("ironwood",4,"4")]);
        assert_eq!(eligible_balance(&json!(10),&rows),Ok(700_000_000));
        assert_eq!(eligible_balance(&json!(8),&rows),Ok(500_000_000)); // Aggregate caps nonspendable notes.
        assert_eq!(eligible_balance(&json!(20),&rows),Ok(700_000_000)); // Listing caps aggregate too.
        assert!(eligible_balance(&json!(2),&rows).is_err());
        // An immature note is now excluded (not counted), never an error, so a
        // fresh payout change note cannot pause admission.
        let mut pending=note("orchard",3,"1"); pending["confirmations"]=json!(0);
        assert_eq!(eligible_balance(&json!(1),&json!([pending])),Ok(0));
        let tiny=serde_json::from_str::<Value>("1.000000000000001").unwrap();
        assert!(eligible_balance(&tiny,&json!([])).is_err());
        assert!(eligible_balance(&Value::Null,&json!([])).is_err());
        assert_eq!(eligible_balance(&json!(0),&json!([])),Ok(0));
    }

    #[test]
    fn enumeration_is_typed_bounded_unique_and_complete() {
        let n=note("orchard",1,"1");
        assert!(eligible_balance(&json!(2),&json!([n.clone(),n.clone()])).is_err());
        // Structural corruption still invalidates the whole evidence:
        for field in ["pool","txid","vout","amount"] {
            let mut bad=n.clone(); bad.as_object_mut().unwrap().remove(field);
            assert!(eligible_balance(&json!(1),&json!([bad])).is_err(),"{field}");
        }
        // A missing eligibility field now EXCLUDES that note (it just is not
        // counted yet) rather than pausing all admission:
        for field in ["address","confirmations","safe","spendable","solvable"] {
            let mut bad=n.clone(); bad.as_object_mut().unwrap().remove(field);
            assert_eq!(eligible_balance(&json!(1),&json!([bad])),Ok(0),"{field}");
        }
        let rows=Value::Array(vec![n;MAX_UNSPENT_OUTPUTS+1]);
        assert_eq!(eligible_balance(&json!(1),&rows),Err(ZecdFundingError::EnumerationTooLarge));
        assert!(eligible_balance(&json!(1),&json!({"partial":[]})).is_err());
    }

    #[test]
    fn signer_readiness_never_assumes_identity_decryption_succeeded() {
        let until=now().unwrap()+60;
        let good=info(100); assert!(wallet_ready(&good,100,until).is_ok());
        let mut absent=good.clone(); absent.as_object_mut().unwrap().remove("unlocked_until");
        assert_eq!(wallet_ready(&absent,100,until),Err(ZecdFundingError::IdentitySignerNotProven));
        for field in ["private_keys_enabled","scanning","enhanced_through","walletname","walletversion","format"] {
            let mut bad=good.clone(); bad.as_object_mut().unwrap().remove(field);
            assert!(wallet_ready(&bad,100,until).is_err());
        }
        let mut watch=good.clone(); watch["private_keys_enabled"]=json!(false);
        assert!(wallet_ready(&watch,100,until).is_err());
        for end in [0,until-1] { let mut bad=good.clone(); bad["unlocked_until"]=json!(end);
            assert!(wallet_ready(&bad,100,until).is_err()); }
    }

    #[test]
    fn source_and_network_must_match_exact_supported_wallet() {
        assert!(source_owned(&source(),"synthetic-source").is_ok());
        for field in ["ismine","solvable"] { let mut s=source(); s[field]=json!(false);
            assert!(source_owned(&s,"synthetic-source").is_err()); }
        let mut s=source(); s["receiver_types"]=json!(["transparent"]);
        assert!(source_owned(&s,"synthetic-source").is_err());
        let mut s=source(); s["receivers_consistent"]=json!(false);
        assert!(source_owned(&s,"synthetic-source").is_err());
        assert!(source_owned(&source(),"other").is_err());
        let mut wrong=tip(100,"a"); wrong["chain"]=json!("main");
        assert!(anchor(&wrong).is_err());
        wrong=tip(100,"a"); wrong["initialblockdownload"]=json!(true);
        assert!(anchor(&wrong).is_err());
        assert!(identity(&json!({"version":700,"subversion":"/zecd:0.7.1/"})).is_err());
    }

    #[test]
    fn identity_accepts_zecd_0_7_0_and_0_8_x_only() {
        for (version,subversion) in [(700,"/zecd:0.7.0/"),(800,"/zecd:0.8.0-rc2/"),
            (800,"/zecd:0.8.0/"),(800,"/zecd:0.8.1/")] {
            assert_eq!(identity(&json!({"version":version,"subversion":subversion})),Ok(()),
                "{version} {subversion}");
        }
        for reply in [
            json!({"version":900,"subversion":"/zecd:0.9.0/"}),
            json!({"version":800,"subversion":"/zecd:0.9.0/"}),
            json!({"version":700,"subversion":"/zecd:0.7.1/"}),
            json!({"version":701,"subversion":"/zecd:0.7.0/"}),
            json!({"version":801,"subversion":"/zecd:0.8.1/"}),
            json!({"version":810,"subversion":"/zecd:0.8.1/"}),
            json!({"version":0,"subversion":"/zecd:0.8.1/"}),
            json!({"version":"800","subversion":"/zecd:0.8.1/"}),
            json!({"subversion":"/zecd:0.8.1/"}),
            json!({"version":800}),
            json!({"version":800,"subversion":"/zecd:0.80/"}),
            json!({"version":800,"subversion":"/zecd:0.8/"}),
            json!({"version":800,"subversion":"/zcashd:0.8.1/"}),
        ] {
            assert_eq!(identity(&reply),Err(ZecdFundingError::IdentityMismatch),"{reply}");
        }
    }

    #[test]
    fn memoless_wallet_readiness_accepts_null_watermark_only_when_fetch_memos_is_false() {
        let until=now().unwrap()+60;
        let with=|fetch:Option<Value>,enhanced:Option<Value>| {
            let mut v=info(100);
            if let Some(fetch)=fetch { v["fetch_memos"]=fetch; }
            match enhanced {
                Some(e) => v["enhanced_through"]=e,
                None => { v.as_object_mut().unwrap().remove("enhanced_through"); },
            }
            v
        };
        let off=||Some(json!(false));
        for accepted in [with(off(),Some(Value::Null)),with(off(),None),with(off(),Some(json!(100)))] {
            assert_eq!(wallet_metadata(&accepted,100,until).map(|(_,r)|r),
                Ok(DiagnosticSignerReadiness::PassphraseUnlocked),"{accepted}");
        }
        // zecd 0.8 memoless wallet without unlocked_until: readable, identity not proven.
        let mut no_unlock=with(off(),Some(Value::Null));
        no_unlock.as_object_mut().unwrap().remove("unlocked_until");
        assert_eq!(wallet_metadata(&no_unlock,100,until),
            Ok(("synthetic-wallet".to_owned(),DiagnosticSignerReadiness::IdentityNotProven)));
        // A stale numeric watermark, or an unfinished scan, is still not ready with memos off.
        assert_eq!(wallet_metadata(&with(off(),Some(json!(99))),100,until),Err(ZecdFundingError::NotReady));
        let mut scanning=with(off(),Some(Value::Null)); scanning["scanning"]=json!(true);
        assert_eq!(wallet_metadata(&scanning,100,until),Err(ZecdFundingError::NotReady));
        // Memo-fetching, unspecified or malformed fetch_memos keeps the strict proof.
        for fetch in [None,Some(json!(true)),Some(json!("false")),Some(Value::Null)] {
            for enhanced in [Some(Value::Null),None] {
                let v=with(fetch.clone(),enhanced.clone());
                assert_eq!(wallet_metadata(&v,100,until),Err(ZecdFundingError::NotReady),
                    "fetch_memos={fetch:?} enhanced_through={enhanced:?}");
            }
        }
    }

    #[test]
    fn source_owned_rejects_watchonly_unsolvable_and_unstated_spend_capability() {
        for (field,value) in [("iswatchonly",json!(true)),("solvable",json!(false)),
            ("ismine",json!(false)),("isscript",json!(true)),
            ("iswatchonly",Value::Null),("solvable",Value::Null)] {
            let mut s=source(); s[field]=value.clone();
            assert_eq!(source_owned(&s,"synthetic-source"),Err(ZecdFundingError::UnsupportedSource),
                "{field}={value}");
            let mut s=source(); s.as_object_mut().unwrap().remove(field);
            assert_eq!(source_owned(&s,"synthetic-source"),Err(ZecdFundingError::UnsupportedSource),
                "{field} absent");
        }
    }

    /// Unencrypted wallet as deployed after 95862e8: no `unlocked_until`
    /// (IdentityNotProven readiness). "0.8" is zecd 0.8.0-rc2 with
    /// `fetch_memos=false` and a null watermark. Legacy positional layout.
    fn unencrypted_responses(release: &str) -> Vec<Value> {
        let mut r=responses();
        if release=="0.8" {
            r[0]=json!({"version":800,"subversion":"/zecd:0.8.0-rc2/"});
            for i in [2,7] { r[i]["fetch_memos"]=json!(false); r[i]["enhanced_through"]=Value::Null; }
        }
        for i in [2,7] { r[i].as_object_mut().unwrap().remove("unlocked_until"); }
        r
    }

    #[tokio::test]
    async fn unencrypted_wallet_funding_succeeds_without_any_operation_proof_reads() {
        for release in ["0.7","0.8"] {
            let (wallet,w)=mock(unencrypted_responses(release),None).await;
            let (node,n)=mock(vec![json!(100),json!("a".repeat(64))],None).await;
            let evidence=collect_testnet_funding_observed(&wallet,"synthetic-source",&node).await
                .unwrap_or_else(|e| panic!("release {release}: {e:?}"));
            assert_eq!(evidence.signer_readiness,SignerReadiness::PassphraseUnlocked);
            assert_eq!(evidence.confirmed_eligible_zatoshis,200_000_000);
            assert_eq!(evidence.valid_until_unix-evidence.checked_at_unix,EVIDENCE_LIFETIME_SECONDS);
            let reads=w.await.unwrap();
            assert_eq!(reads.len(),8,"release {release}");
            assert_eq!(methods(&reads[..4]),
                vec!["getnetworkinfo","getwalletinfo","getblockchaininfo","getaddressinfo"]);
            let mut money=methods(&reads[4..6]); money.sort();
            assert_eq!(money,vec!["getbalance","listunspent"]);
            assert_eq!(methods(&reads[6..]),vec!["getwalletinfo","getblockchaininfo"]);
            assert!(reads.iter().all(|r| !matches!(r["method"].as_str().unwrap(),
                "z_getoperationstatus"|"getrawtransaction"|"getblock"|"gettransaction")));
            let node_reads=n.await.unwrap();
            assert_eq!(methods(&node_reads),vec!["getblockcount","getblockhash"]);
            assert_eq!(node_reads[1]["params"],json!([100]));
        }
    }

    #[tokio::test]
    async fn watchonly_or_unsolvable_source_cannot_pass_unencrypted_funding() {
        for (field,value) in [("iswatchonly",json!(true)),("solvable",json!(false))] {
            for release in ["0.7","0.8"] {
                let mut r=unencrypted_responses(release); r[3][field]=value.clone(); r.truncate(4);
                let (wallet,w)=mock(r,None).await; let (node,n)=mock(vec![],None).await;
                let failure=collect_testnet_funding_observed(&wallet,"synthetic-source",&node).await.unwrap_err();
                assert_eq!(failure,FundingObservedError {error:ZecdFundingError::UnsupportedSource,
                    stage:"opening_source_check",category:"unsupported_source"},"{field} {release}");
                let reads=w.await.unwrap();
                assert_eq!(methods(&reads),
                    vec!["getnetworkinfo","getwalletinfo","getblockchaininfo","getaddressinfo"]);
                assert!(n.await.unwrap().is_empty());
            }
        }
    }

    #[tokio::test]
    async fn runtime_success_uses_fixed_reads_and_exact_anchor() {
        let (wallet,w)=mock(responses(),None).await;
        let (node,n)=mock(vec![json!(102),json!("a".repeat(64))],None).await;
        let evidence=collect_testnet_funding(&wallet,"synthetic-source",&node).await.unwrap();
        assert_eq!(evidence.valid_until_unix-evidence.checked_at_unix,EVIDENCE_LIFETIME_SECONDS);
        assert_eq!(evidence.confirmed_eligible_zatoshis,200_000_000);
        assert_eq!(evidence.signer_readiness,SignerReadiness::PassphraseUnlocked);
        assert_eq!(format!("{evidence:?}"),"ZecdFundingEvidence { redacted }");
        let r=w.await.unwrap();
        assert_eq!(r[..4].iter().map(|v|v["method"].as_str().unwrap()).collect::<Vec<_>>(),vec![
            "getnetworkinfo","getwalletinfo","getblockchaininfo","getaddressinfo"]);
        assert_eq!(r[6..].iter().map(|v|v["method"].as_str().unwrap()).collect::<Vec<_>>(),vec![
            "getwalletinfo","getblockchaininfo"]);
        assert_eq!(r.iter().find(|v|v["method"]=="getbalance").unwrap()["params"],json!(["*",10]));
        assert_eq!(r.iter().find(|v|v["method"]=="listunspent").unwrap()["params"],json!([10,u32::MAX,[],false]));
        assert_eq!(n.await.unwrap()[1]["params"],json!([100]));
    }

    #[tokio::test]
    async fn a_forward_tip_advance_within_tolerance_still_produces_funding() {
        // The wallet tip advances +5 during the read (a fast-block burst).
        // Forward movement within FUNDING_ANCHOR_MAX_DRIFT is a chain
        // extension, not a reorg, so it must NOT pause crediting -- the closing
        // anchor is still canonical and the read yields evidence.
        let mut r=responses();
        r[6]=tip(105,"a"); r[7]=info(105);
        let (wallet,w)=mock(r,None).await;
        let (node,n)=mock(vec![json!(107),json!("a".repeat(64))],None).await;
        let evidence=collect_testnet_funding(&wallet,"synthetic-source",&node).await.unwrap();
        assert_eq!(evidence.confirmed_eligible_zatoshis,200_000_000);
        // The node's canonical check is against the CLOSING (advanced) height.
        assert_eq!(n.await.unwrap()[1]["params"],json!([105]));
        w.await.unwrap();
    }

    #[tokio::test]
    async fn wrongfork_or_midread_change_never_produces_funding() {
        for changed in [false,true] {
            let mut r=responses();
            if changed { r[6]=tip(125,"b"); r[7]=info(125); }
            let (wallet,w)=mock(r,None).await;
            let (node,n)=mock(if changed {vec![]} else {vec![json!(100),json!("b".repeat(64))]},None).await;
            let error=collect_testnet_funding(&wallet,"synthetic-source",&node).await.unwrap_err();
            assert_eq!(error,if changed {ZecdFundingError::ConcurrentChange} else {ZecdFundingError::ChainMismatch});
            w.await.unwrap(); n.await.unwrap();
        }
    }

    #[tokio::test]
    async fn unavailable_enumeration_is_not_zero_and_has_no_fallback() {
        for code in [-32601,-32602,-13,-28] {
            let mut r=responses(); r.truncate(6);
            r[5]=json!({"mock_error":{"code":code,"message":"private remote detail"}});
            let (wallet,w)=mock(r,None).await;
            let (node,n)=mock(vec![],None).await;
            let error=collect_testnet_funding(&wallet,"synthetic-source",&node).await.unwrap_err();
            assert_eq!(error,if code == -32601 {ZecdFundingError::UnsupportedRpc} else {ZecdFundingError::Unavailable});
            assert!(!error.to_string().contains("private remote detail"));
            assert_eq!(w.await.unwrap().len(),6); assert!(n.await.unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn identity_diagnostics_stay_opaque_and_signer_mode_changes_are_rejected() {
        let identity_responses = || {
            let mut r=responses();
            for i in [2,7] { r[i].as_object_mut().unwrap().remove("unlocked_until"); }
            r
        };
        let (wallet,w)=mock(identity_responses(),None).await;
        let (node,n)=mock(vec![json!(100),json!("a".repeat(64))],None).await;
        let diagnostics=collect_testnet_funding_diagnostics(&wallet,"synthetic-source",&node).await.unwrap();
        assert_eq!(diagnostics.signer_readiness(),DiagnosticSignerReadiness::IdentityNotProven);
        assert_eq!(diagnostics.confirmed_eligible_zatoshis(),200_000_000);
        assert_eq!(format!("{diagnostics:?}"),"ZecdFundingDiagnostics { redacted }");
        w.await.unwrap(); n.await.unwrap();
        // Since 95862e8 collect_testnet_funding no longer requires the operation
        // proof (see unencrypted_wallet_funding_* below). The RETAINED proof
        // itself still refuses an empty operation registry.
        let mut unknown=identity_responses(); unknown.truncate(4); unknown.swap(1,2);
        unknown[0]["mock_signer_first"]=json!(true); unknown.push(json!([]));
        let (wallet,w)=mock(unknown,None).await;
        let (node,n)=mock(vec![],None).await;
        assert_eq!(collect_with_identity_proof(&wallet,"synthetic-source",&node).await,Err(ZecdFundingError::IdentitySignerNotProven));
        w.await.unwrap(); n.await.unwrap();
        // A signer-mode change inside the bracket is rejected by BOTH the
        // diagnostics and the credit-authorizing collector.
        for credit in [false,true] {
            let mut r=identity_responses(); r[7]=info(100);
            let (wallet,w)=mock(r,None).await; let (node,n)=mock(vec![],None).await;
            let outcome=if credit {
                collect_testnet_funding(&wallet,"synthetic-source",&node).await.map(|_|())
            } else {
                collect_testnet_funding_diagnostics(&wallet,"synthetic-source",&node).await.map(|_|())
            };
            assert_eq!(outcome,Err(ZecdFundingError::ConcurrentChange),"credit={credit}");
            assert_eq!(w.await.unwrap().len(),8); assert!(n.await.unwrap().is_empty());
        }
    }

    fn identity_receipt_responses() -> (String,Vec<Value>,Vec<Value>) {
        let (raw,txid,height,src,recipient)=crate::zecd_conventional::tests::signer_receipt_fixture();
        let mut wallet_info=info(height as u64); wallet_info.as_object_mut().unwrap().remove("unlocked_until");
        let mut owned=source(); owned["address"]=json!(src);
        let op=json!({"id":"opid-00000000-0000-4000-8000-000000000001","status":"success",
            "method":"z_sendmany","creation_time":now().unwrap()-1,
            "params":{"fromaddress":src,"minconf":1,"amounts":[{"address":recipient,"amount":0.001}]},
            "result":{"txid":txid}});
        // Order through collect_with_identity_proof: opening identity envelope
        // (0-3), selected signer proof (4), money pair (5-6), actor sentinel
        // (7), immutable operation/source (8-9), closing envelope (10-11).
        // No re-envelope exists after the signer proof since 95862e8.
        let wallet=vec![json!({"version":700,"subversion":"/zecd:0.7.0/","mock_signer_first":true}),
            wallet_info.clone(),tip(height as u64,"a"),owned.clone(),json!([op.clone()]),
            json!(12),json!([note("orchard",1,"12")]),
            json!({"mock_error":{"code":-5,"message":"fixed absence"}}),
            json!([op]),owned,wallet_info,tip(height as u64,"a")];
        let node=vec![json!({"txid":txid,"hex":raw,"confirmations":1,"blockhash":"a".repeat(64)}),
            json!({"height":height,"hash":"a".repeat(64),"confirmations":1,"tx":[txid]}),
            json!("a".repeat(64)),json!(height),json!("a".repeat(64)),json!("a".repeat(64))];
        (src,wallet,node)
    }

    // These exercise parser/history integration only: the transaction proof
    // bytes and authenticated wallet responses are synthetic, not a real signer.
    fn shielded_identity_responses(pool: &str) -> (String,Vec<Value>,Vec<Value>) {
        let (raw,txid,height,src,recipient,history)=
            crate::zecd_conventional::tests::shielded_receipt_fixture(pool);
        let mut wallet_info=info(height as u64);
        wallet_info.as_object_mut().unwrap().remove("unlocked_until");
        let mut owned=source(); owned["address"]=json!(src);
        let op=json!({"id":"opid-00000000-0000-4000-8000-000000000002","status":"success",
            "method":"z_sendmany","creation_time":now().unwrap(),
            "params":{"fromaddress":src,"minconf":1,"amounts":[{"address":recipient,"amount":0.001}]},
            "result":{"txid":txid}});
        let wallet=vec![json!({"version":700,"subversion":"/zecd:0.7.0/","mock_signer_first":true,"mock_shielded":true}),
            wallet_info.clone(),tip(height as u64,"a"),owned.clone(),json!([op.clone()]),history,
            json!(12),json!([note("orchard",1,"12")]),
            json!({"mock_error":{"code":-5,"message":"fixed absence"}}),
            json!([op]),owned,wallet_info,tip(height as u64,"a")];
        let node=vec![json!({"txid":txid,"hex":raw,"confirmations":1,"blockhash":"a".repeat(64)}),
            json!({"height":height,"hash":"a".repeat(64),"confirmations":1,"tx":[txid]}),
            json!("a".repeat(64)),json!("a".repeat(64)),json!(height),
            json!("a".repeat(64)),json!("a".repeat(64))];
        (src,wallet,node)
    }

    #[tokio::test]
    async fn selected_unified_receipt_uses_one_bound_wallet_view_and_keeps_read_only_fences() {
        for (pool,bare_sapling) in [("orchard",false),("ironwood",false),("sapling",false),("sapling",true)] {
            let (src,mut wr,nr)=shielded_identity_responses(pool);
            if bare_sapling {
                let address=wr[5]["details"][0]["address"].clone();
                wr[4][0]["params"]["amounts"][0]["address"]=address.clone();
                wr[9][0]["params"]["amounts"][0]["address"]=address;
            }
            let selected=wr[4][0].clone();
            let (_,other_wr,_)=identity_receipt_responses();
            let mut newer=other_wr[4][0].clone(); newer["params"]["fromaddress"]=json!(src);
            newer["creation_time"]=selected["creation_time"].clone();
            newer["id"]=json!("opid-00000000-0000-4000-8000-000000000003");
            wr[4]=json!([newer,selected]);
            let (wallet,w)=shielded_mock(wr,pool=="orchard").await; let (node,n)=mock(nr,None).await;
            let evidence=collect_with_identity_proof(&wallet,&src,&node).await.unwrap();
            assert_eq!(evidence.signer_readiness,SignerReadiness::IdentityOperationVerified);
            let reads=w.await.unwrap(); let node_reads=n.await.unwrap();
            assert_eq!(reads.len(),13); assert_eq!(node_reads.len(),7);
            assert_eq!(reads.iter().filter(|r|r["method"]=="gettransaction").count(),1);
            let history_read=reads.iter().find(|r|r["method"]=="gettransaction").unwrap();
            assert_eq!(history_read["params"][0],node_reads[0]["params"][0]);
            assert_eq!(reads.iter().filter(|r|r["method"]=="getrawtransaction").count(),1);
            assert_eq!(reads[9]["params"],json!([["opid-00000000-0000-4000-8000-000000000002"]]));
            assert_eq!(node_reads[2]["params"],node_reads[3]["params"]);
            assert_eq!(node_reads[2]["params"],node_reads[6]["params"]);
            assert!(reads.iter().all(|r|matches!(r["method"].as_str().unwrap(),
                "getnetworkinfo"|"getblockchaininfo"|"getwalletinfo"|"getaddressinfo"|"getbalance"
                |"listunspent"|"z_getoperationstatus"|"gettransaction"|"getrawtransaction")));
        }
    }

    #[tokio::test]
    async fn selected_unified_receipt_missing_or_conflicting_history_never_scans_newer() {
        for case in 0..11 {
            let (src,mut wr,mut nr)=shielded_identity_responses("orchard");
            let selected=wr[4][0].clone();
            let (_,other_wr,_)=identity_receipt_responses();
            let mut newer=other_wr[4][0].clone(); newer["params"]["fromaddress"]=json!(src);
            newer["creation_time"]=selected["creation_time"].clone();
            newer["id"]=json!("opid-00000000-0000-4000-8000-000000000003");
            wr[4]=json!([newer,selected]);
            match case {
                0 => wr[5]=Value::Null,
                1 => { wr[5].as_object_mut().unwrap().remove("details"); },
                2 => wr[5]["txid"]=json!("b".repeat(64)),
                3 => wr[5]["hex"]=json!("00"),
                4 => wr[5]["blockhash"]=json!("b".repeat(64)),
                5 => wr[5]["confirmations"]=json!(0),
                6 => { wr[5].as_object_mut().unwrap().remove("fee"); },
                7 => wr[5]["details"][0]["amount"]=json!(-0.002),
                8 => wr[5]["details"][0]["address"]=json!("unsupported synthetic receiver"),
                9 => wr[5]["details"][0]["pool"]=json!("sapling"),
                _ => wr[5]=json!({"mock_error":{"code":-5,"message":"private history absent"}}),
            }
            wr.truncate(6); nr.truncate(3);
            let (wallet,w)=shielded_mock(wr,case%2==0).await; let (node,n)=mock(nr,None).await;
            let error=collect_with_identity_proof(&wallet,&src,&node).await.unwrap_err();
            assert!(!error.to_string().contains("private history absent"));
            let reads=w.await.unwrap(); let node_reads=n.await.unwrap();
            assert_eq!(reads.len(),6,"case {case}"); assert_eq!(node_reads.len(),3,"case {case}");
            assert_eq!(reads.iter().filter(|r|r["method"]=="gettransaction").count(),1);
            assert_eq!(node_reads.iter().filter(|r|r["method"]=="getrawtransaction").count(),1);
            assert_eq!(reads.iter().filter(|r|r["method"]=="getrawtransaction").count(),0);
            assert!(reads.iter().all(|r| !matches!(r["method"].as_str().unwrap(),
                "z_sendmany"|"z_getoperationresult"|"walletpassphrase"|"walletlock"|"signmessage")));
        }
    }

    #[tokio::test]
    async fn unified_history_keeps_final_chain_actor_operation_source_and_anchor_fences() {
        for case in 0..6 {
            let (src,mut wr,mut nr)=shielded_identity_responses("orchard");
            let (expected,stage)=match case {
                0 => { nr[3]=json!("b".repeat(64)); nr.truncate(4); wr.truncate(6);
                    (ZecdFundingError::ChainMismatch,"signer_history_canonical_check") },
                1 => { wr[8]=json!({"mock_error":{"code":-1,"message":"actor unavailable"}}); wr.truncate(9); nr.truncate(4);
                    (ZecdFundingError::NotReady,"signer_actor_read") },
                2 => { wr[9][0]["params"]["minconf"]=json!(2); wr.truncate(10); nr.truncate(4);
                    (ZecdFundingError::ConcurrentChange,"signer_final_operation_check") },
                // Formerly the removed post-signer re-envelope; the wallet
                // identity fence now lives in the closing envelope only.
                3 => { wr[11]["walletname"]=json!("other synthetic wallet"); nr.truncate(4);
                    (ZecdFundingError::ConcurrentChange,"funding_bracket_check") },
                4 => { wr[12]["bestblockhash"]=json!("b".repeat(64)); nr.truncate(6);
                    (ZecdFundingError::ChainMismatch,"funding_anchor_canonical_check") },
                _ => { wr[10]["ismine"]=json!(false); wr.truncate(11); nr.truncate(4);
                    (ZecdFundingError::UnsupportedSource,"signer_final_source_check") },
            };
            let (wallet,w)=shielded_mock(wr,case%2==0).await; let (node,n)=mock(nr,None).await;
            let failure=identity_proof_observed(&wallet,&src,&node).await.unwrap_err();
            assert_eq!((failure.error,failure.stage),(expected,stage),"case {case}");
            w.await.unwrap(); n.await.unwrap();
        }
    }

    #[tokio::test]
    async fn identity_receipt_proves_current_signer_without_any_key_use_or_send() {
        let (src,wallet,node)=identity_receipt_responses();
        let (wallet,w)=mock(wallet,None).await; let (node,n)=mock(node,None).await;
        let evidence=collect_with_identity_proof(&wallet,&src,&node).await.unwrap();
        assert_eq!(evidence.signer_readiness,SignerReadiness::IdentityOperationVerified);
        assert_eq!(evidence.confirmed_eligible_zatoshis,1_200_000_000);
        let reads=w.await.unwrap(); assert_eq!(reads.len(),12);
        assert_eq!(reads[7]["method"],"getrawtransaction");
        assert_eq!(reads[7]["params"],json!(["0".repeat(64),0]));
        assert_eq!(reads[8]["params"],json!([["opid-00000000-0000-4000-8000-000000000001"]]));
        assert!(reads.iter().all(|r| !matches!(r["method"].as_str().unwrap(),
            "z_sendmany"|"z_getoperationresult"|"walletpassphrase"|"walletlock"|"signmessage")));
        assert_eq!(n.await.unwrap().len(),6);
    }

    #[tokio::test]
    async fn signer_first_normal_advance_uses_only_fresh_money_and_closes_both_canonical_proofs() {
        for shielded in [false,true] {
            let (src,mut wr,mut nr)=if shielded {shielded_identity_responses("orchard")}
                else {identity_receipt_responses()};
            let shift=usize::from(shielded);
            let old_height=wr[2]["blocks"].as_u64().unwrap();
            // The wallet advances H -> H+1 between the opening and closing envelopes.
            wr[10+shift]["enhanced_through"]=json!(old_height+1);
            wr[11+shift]=tip(old_height+1,"b");
            // The money pair is sampled only after the historical signer work.
            wr[5+shift]=json!(1); wr[6+shift]=json!([note("orchard",9,"1")]);
            nr[3+shift]=json!(old_height+1); nr[4+shift]=json!("b".repeat(64));
            let (wallet,w)=if shielded {shielded_mock(wr,true).await} else {mock(wr,None).await};
            let (node,n)=mock(nr,None).await;
            let proof=collect_with_identity_proof(&wallet,&src,&node).await.unwrap();
            assert_eq!(proof.confirmed_eligible_zatoshis,100_000_000);
            let reads=w.await.unwrap(); let node_reads=n.await.unwrap();
            assert_eq!(reads.len(),12+shift); assert_eq!(node_reads.len(),6+shift);
            assert_eq!(methods(&reads[..5]),
                vec!["getnetworkinfo","getwalletinfo","getblockchaininfo","getaddressinfo","z_getoperationstatus"]);
            let mut money=methods(&reads[5+shift..7+shift]); money.sort();
            assert_eq!(money,vec!["getbalance","listunspent"]);
            assert_eq!(methods(&reads[7+shift..]),vec!["getrawtransaction","z_getoperationstatus",
                "getaddressinfo","getwalletinfo","getblockchaininfo"]);
            assert_eq!(node_reads[4+shift]["params"],json!([old_height+1]));
            assert_eq!(node_reads[5+shift]["params"],json!([old_height]));
            assert_eq!(reads.iter().filter(|r|r["method"]=="getbalance").count(),1);
            assert_eq!(reads.iter().filter(|r|r["method"]=="listunspent").count(),1);
        }
    }

    #[tokio::test]
    async fn signer_first_rejects_money_reorg_end_readiness_identity_and_final_receipt_changes() {
        // The former re-envelope cases (walletname / backward height at the
        // removed signer_final_metadata_check) are dropped with that read; the
        // same fences are exercised at the closing envelope by cases 1 and 4.
        for case in 0..9 {
            let (src,mut wr,mut nr)=identity_receipt_responses();
            let (expected,stage)=match case {
                0 => {let h=wr[11]["blocks"].as_u64().unwrap()+30; wr[11]=tip(h,"b"); wr[10]["enhanced_through"]=json!(h); nr.truncate(3);
                    (ZecdFundingError::ConcurrentChange,"funding_bracket_check")},
                1 => {let h=wr[11]["blocks"].as_u64().unwrap()-1; wr[11]=tip(h,"b"); wr[10]["enhanced_through"]=json!(h); nr.truncate(3);
                    (ZecdFundingError::ConcurrentChange,"funding_bracket_check")},
                2 => {wr[10]["scanning"]=json!(true); nr.truncate(3);
                    (ZecdFundingError::NotReady,"closing_metadata_check")},
                3 => {wr[10]["private_keys_enabled"]=json!(false); nr.truncate(3);
                    (ZecdFundingError::NotReady,"closing_metadata_check")},
                4 => {wr[10]["walletname"]=json!("replacement-wallet"); nr.truncate(3);
                    (ZecdFundingError::ConcurrentChange,"funding_bracket_check")},
                5 => {wr[10]["unlocked_until"]=json!(now().unwrap()+3600); nr.truncate(3);
                    (ZecdFundingError::ConcurrentChange,"funding_bracket_check")},
                6 => {nr[5]=json!("b".repeat(64));
                    (ZecdFundingError::ChainMismatch,"signer_closing_canonical_check")},
                7 => {nr[4]=json!("b".repeat(64)); nr.truncate(5);
                    (ZecdFundingError::ChainMismatch,"funding_anchor_canonical_check")},
                _ => {wr[8][0]["creation_time"]=json!(1); wr.truncate(9); nr.truncate(3);
                    (ZecdFundingError::ConcurrentChange,"signer_final_operation_check")},
            };
            let (wallet,w)=mock(wr,None).await; let (node,n)=mock(nr,None).await;
            let failure=identity_proof_observed(&wallet,&src,&node).await.unwrap_err();
            assert_eq!(failure.error,expected,"case {case}"); assert_eq!(failure.stage,stage,"case {case}");
            w.await.unwrap(); n.await.unwrap();
        }
        // The EXTRA closing receipt check is mandatory for shielded too, not
        // substituted by its earlier post-history canonical check.
        let (src,wr,mut nr)=shielded_identity_responses("orchard"); nr[6]=json!("b".repeat(64));
        let (wallet,w)=shielded_mock(wr,false).await; let (node,n)=mock(nr,None).await;
        let failure=identity_proof_observed(&wallet,&src,&node).await.unwrap_err();
        assert_eq!(failure.stage,"signer_closing_canonical_check");
        w.await.unwrap(); n.await.unwrap();
    }

    #[tokio::test]
    async fn signer_first_original_timestamp_is_never_restamped_after_historical_work() {
        for finished in [159,100+EVIDENCE_LIFETIME_SECONDS,99] {
            let (src,mut wr,mut nr)=identity_receipt_responses();
            wr[4][0]["creation_time"]=json!(99); wr[8][0]["creation_time"]=json!(99);
            nr[0]=json!({"mock_expected_method":"getrawtransaction","mock_delay_ms":30,"mock_result":nr[0]});
            let (wallet,w)=mock(wr,None).await; let (node,n)=mock(nr,None).await;
            let result=TEST_NOW.scope(std::cell::Cell::new(100),async {
                let (result,())=tokio::join!(identity_proof_observed(&wallet,&src,&node),async {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    TEST_NOW.with(|clock|clock.set(finished));
                }); result
            }).await;
            if finished==159 {let proof=result.unwrap(); assert_eq!(proof.checked_at_unix,100); assert_eq!(proof.valid_until_unix,100+EVIDENCE_LIFETIME_SECONDS);}
            else {let error=result.unwrap_err(); assert_eq!(error.error,ZecdFundingError::InvalidEvidence); assert_eq!(error.stage,"diagnostic_final_freshness");}
            assert_eq!(w.await.unwrap().len(),12); assert_eq!(n.await.unwrap().len(),6);
        }
    }

    #[tokio::test]
    async fn signer_first_cancellation_drops_pending_money_without_closing_or_restamping() {
        let (src,mut wr,mut nr)=identity_receipt_responses();
        wr[5]=json!({"mock_hold_money":true,"mock_money_result":wr[5]}); wr.truncate(7); nr.truncate(3);
        let (ready,ready_rx)=tokio::sync::oneshot::channel();
        let (wallet,w)=mock_with_money_ready(wr,None,Some(ready)).await; let (node,n)=mock(nr,None).await;
        let observer=FundingStageObserver::default();
        let result={
            let collector=identity_proof_observed_with_stage(&wallet,&src,&node,&observer);
            tokio::pin!(collector);
            tokio::select! {ready=ready_rx=>ready.unwrap(), result=&mut collector=>panic!("ended before money barrier: {result:?}")}
            assert_eq!(observer.stage(),"balance_inventory_read");
            tokio::time::timeout(Duration::ZERO,collector).await
        };
        assert!(result.is_err()); assert_eq!(observer.stage(),"balance_inventory_read");
        let reads=w.await.unwrap(); assert_eq!(reads.len(),7); assert_eq!(n.await.unwrap().len(),3);
        assert_eq!(reads.iter().filter(|r|r["method"]=="getrawtransaction").count(),0);
        assert_eq!(reads.iter().filter(|r|r["method"]=="z_getoperationstatus").count(),1);
        assert_eq!(COLLECTION_TIMEOUT_SECONDS,45); assert_eq!(EVIDENCE_LIFETIME_SECONDS,600);
    }

    #[tokio::test]
    async fn late_actor_death_during_money_is_rejected_despite_retained_ready_status_and_operation() {
        for shielded in [false,true] {
            let (src,mut wr,mut nr)=if shielded {shielded_identity_responses("orchard")}
                else {identity_receipt_responses()};
            let shift=usize::from(shielded);
            // The daemon's retained operation, source and status would all
            // still pass even if the actor stopped during the money reads.
            assert_eq!(wr[4],wr[8+shift]);
            assert_eq!(wr[1],wr[10+shift]);
            let h=wr[11+shift]["blocks"].as_u64().unwrap();
            assert!(wallet_metadata(&wr[10+shift],h,now().unwrap()+60).is_ok());
            assert!(source_owned(&wr[9+shift],&src).is_ok());
            wr[6+shift]=json!({"mock_delay_ms":20,"mock_result":wr[6+shift]});
            wr[7+shift]=json!({"mock_expected_method":"getrawtransaction",
                "mock_result":{"mock_error":{"code":-1,"message":"synthetic actor stopped"}}});
            wr.truncate(8+shift); nr.truncate(3+shift);
            let (wallet,w)=if shielded {shielded_mock(wr,false).await} else {mock(wr,None).await};
            let (node,n)=mock(nr,None).await;
            let failure=identity_proof_observed(&wallet,&src,&node).await.unwrap_err();
            assert_eq!(failure,FundingObservedError {error:ZecdFundingError::NotReady,
                stage:"signer_actor_read",category:"wallet_not_ready"});
            let reads=w.await.unwrap();
            assert_eq!(reads.len(),8+shift);
            assert_eq!(reads.last().unwrap()["params"],json!(["0".repeat(64),0]));
            assert_eq!(reads.iter().filter(|r|r["method"]=="getrawtransaction").count(),1);
            assert_eq!(reads.iter().filter(|r|r["method"]=="z_getoperationstatus").count(),1);
            assert_eq!(reads.iter().filter(|r|r["method"]=="getaddressinfo").count(),1);
            // Only the opening envelope; the post-signer re-envelope is gone.
            assert_eq!(reads.iter().filter(|r|r["method"]=="getwalletinfo").count(),1);
            assert_eq!(n.await.unwrap().len(),3+shift);
        }
    }

    #[tokio::test]
    async fn newest_receipt_selection_is_stable_and_does_not_query_old_missing_receipt() {
        for same_time in [false,true] {for reverse in [false,true] {
            let (src,mut wr,nr)=identity_receipt_responses();
            let mut newer=wr[4][0].clone(); let mut old=newer.clone();
            old["id"]=json!("opid-00000000-0000-4000-8000-000000000002");
            newer["creation_time"]=json!(now().unwrap());
            if same_time {old["creation_time"]=newer["creation_time"].clone();}
            old["result"]["txid"]=json!("b".repeat(64));
            wr[8]=json!([newer.clone()]);
            wr[4]=if reverse {json!([newer,old])} else {json!([old,newer])};
            let (wallet,w)=mock(wr,None).await; let (node,n)=mock(nr,None).await;
            assert!(collect_with_identity_proof(&wallet,&src,&node).await.is_ok());
            let reads=n.await.unwrap(); assert_eq!(reads.len(),6);
            assert_ne!(reads[0]["params"][0],json!("b".repeat(64)));
            assert_eq!(reads.iter().filter(|r|r["method"]=="getrawtransaction").count(),1);
            w.await.unwrap();
        }}
    }

    #[tokio::test]
    async fn discovery_skips_only_explicit_absence_then_completes_transparent_and_shielded_proof() {
        for shielded in [false,true] {
            let (src,mut wr,mut nr)=if shielded {shielded_identity_responses("orchard")}
                else {identity_receipt_responses()};
            let mut selected=wr[4][0].clone();selected["creation_time"]=json!(now().unwrap()-1);
            let mut missing=selected.clone();
            missing["id"]=json!("opid-00000000-0000-4000-8000-000000000003");
            missing["creation_time"]=json!(now().unwrap());
            missing["result"]["txid"]=json!("b".repeat(64));
            wr[8+usize::from(shielded)]=json!([selected.clone()]);
            wr[4]=json!([selected,missing]);
            nr.insert(0,json!({"mock_error":{"code":-5,"message":"synthetic absent"}}));
            let (wallet,w)=if shielded {shielded_mock(wr,false).await} else {mock(wr,None).await};
            let (node,n)=mock(nr,None).await;
            let proof=identity_proof_observed(&wallet,&src,&node).await
                .unwrap_or_else(|error|panic!("shielded={shielded}, stage={}, category={}",error.stage,error.category));
            assert_eq!(proof.signer_readiness,SignerReadiness::IdentityOperationVerified);
            let reads=n.await.unwrap();
            assert_eq!(reads[0]["params"],json!(["b".repeat(64),1]));
            assert_eq!(reads.iter().filter(|r|r["method"]=="getrawtransaction").count(),2);
            let wallet_reads=w.await.unwrap();
            assert_eq!(wallet_reads.len(),12+usize::from(shielded));
            assert_eq!(wallet_reads.iter().filter(|r|r["method"]=="getrawtransaction").count(),1);
        }
    }

    #[tokio::test]
    async fn discovery_queries_at_most_three_candidates_in_deterministic_order() {
        let (src,mut wr,_)=identity_receipt_responses(); let template=wr[4][0].clone();
        let at=now().unwrap();
        let ops=(1..=4).map(|i|{let mut op=template.clone();
            op["id"]=json!(format!("opid-00000000-0000-4000-8000-{i:012}"));
            op["creation_time"]=json!(at-i); op["result"]["txid"]=json!(format!("{i:064x}")); op}).collect::<Vec<_>>();
        wr[4]=json!(ops); wr.truncate(5);
        let absent=json!({"mock_error":{"code":-5,"message":"synthetic absent"}});
        let (wallet,w)=mock(wr,None).await; let (node,n)=mock(vec![absent;3],None).await;
        let error=collect_with_identity_proof(&wallet,&src,&node).await.unwrap_err();
        assert_eq!(error,ZecdFundingError::IdentitySignerNotProven);
        let reads=n.await.unwrap(); assert_eq!(reads.len(),3);
        for (i,read) in reads.iter().enumerate() {assert_eq!(read["params"],json!([format!("{:064x}",i+1),1]));}
        assert_eq!(w.await.unwrap().len(),5);
    }

    #[tokio::test]
    async fn discovery_cannot_fall_back_after_a_confirmed_receipt_proof_or_closing_check_fails() {
        for case in 0..5 {
            let shielded=case==3;
            let (src,mut wr,mut nr)=if shielded {shielded_identity_responses("orchard")}
                else {identity_receipt_responses()};
            let mut selected=wr[4][0].clone(); let mut older=selected.clone();
            selected["creation_time"]=json!(now().unwrap());
            older["creation_time"]=json!(now().unwrap()-1);
            older["id"]=json!("opid-00000000-0000-4000-8000-000000000003");
            older["result"]["txid"]=json!("b".repeat(64));
            wr[4]=json!([older,selected.clone()]); wr[8+usize::from(shielded)]=json!([selected]);
            match case {
                0=>{nr[1]["tx"]=json!(["b".repeat(64)]);nr.truncate(2);wr.truncate(5);},
                1=>{nr[2]=json!("b".repeat(64));nr.truncate(3);wr.truncate(5);},
                2=>{nr[0]["hex"]=json!("invalid");nr.truncate(3);wr.truncate(5);},
                3=>{wr[5]["blockhash"]=json!("b".repeat(64));nr.truncate(3);wr.truncate(6);},
                _=>{wr[8]=json!([]);wr.truncate(9);nr.truncate(3);},
            }
            let (wallet,w)=if shielded {shielded_mock(wr,false).await} else {mock(wr,None).await};
            let (node,n)=mock(nr,None).await;
            assert!(collect_with_identity_proof(&wallet,&src,&node).await.is_err(),"case {case}");
            let reads=n.await.unwrap();
            assert_eq!(reads.iter().filter(|r|r["method"]=="getrawtransaction").count(),1,"case {case}");
            w.await.unwrap();
        }
    }

    #[tokio::test]
    async fn discovery_preserves_original_clock_across_absence_and_never_renews_its_lease() {
        for finished in [159,100+EVIDENCE_LIFETIME_SECONDS] {
            let (src,mut wr,mut nr)=identity_receipt_responses();
            let mut selected=wr[4][0].clone();selected["creation_time"]=json!(98);
            let mut absent=selected.clone();absent["creation_time"]=json!(99);
            absent["id"]=json!("opid-00000000-0000-4000-8000-000000000002");
            absent["result"]["txid"]=json!("b".repeat(64));
            wr[4]=json!([selected.clone(),absent]);wr[8]=json!([selected]);
            nr.insert(0,json!({"mock_delay_ms":30,"mock_result":{"mock_error":{"code":-5,"message":"synthetic absent"}}}));
            let (wallet,w)=mock(wr,None).await;let (node,n)=mock(nr,None).await;
            let result=TEST_NOW.scope(std::cell::Cell::new(100),async {
                let (result,())=tokio::join!(collect_with_identity_proof(&wallet,&src,&node),async {
                    tokio::time::sleep(Duration::from_millis(5)).await;TEST_NOW.with(|clock|clock.set(finished));
                });result
            }).await;
            if finished==159 {let proof=result.unwrap();assert_eq!(proof.checked_at_unix,100);assert_eq!(proof.valid_until_unix,100+EVIDENCE_LIFETIME_SECONDS);}
            else {assert_eq!(result,Err(ZecdFundingError::InvalidEvidence));}
            assert_eq!(w.await.unwrap().len(),12);assert_eq!(n.await.unwrap().len(),7);
        }
    }

    #[tokio::test]
    async fn discovery_cancellation_closes_the_pending_lookup_without_trying_another_candidate() {
        let (src,mut wr,_)=identity_receipt_responses();
        let mut other=wr[4][0].clone();other["id"]=json!("opid-00000000-0000-4000-8000-000000000002");
        wr[4].as_array_mut().unwrap().push(other);wr.truncate(5);
        let (wallet,w)=mock(wr,None).await;
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let node=ZcashRpcClient::new(&format!("http://{}",listener.local_addr().unwrap()));
        let (ready,ready_rx)=tokio::sync::oneshot::channel();
        let server=tokio::spawn(async move {
            let (mut stream,_)=listener.accept().await.unwrap();let request=mock_request(&mut stream).await;
            assert_eq!(request["method"],"getrawtransaction");ready.send(()).unwrap();
            let mut byte=[0];
            assert_eq!(tokio::time::timeout(Duration::from_secs(2),stream.read(&mut byte)).await.unwrap().unwrap(),0);
            assert!(tokio::time::timeout(Duration::from_millis(15),listener.accept()).await.is_err());
        });
        let observer=FundingStageObserver::default();
        let result={
            let collector=identity_proof_observed_with_stage(&wallet,&src,&node,&observer);tokio::pin!(collector);
            tokio::select! {ready=ready_rx=>ready.unwrap(),result=&mut collector=>panic!("ended before discovery barrier: {result:?}")}
            assert_eq!(observer.stage(),"signer_raw_read");tokio::time::timeout(Duration::ZERO,collector).await
        };
        assert!(result.is_err());server.await.unwrap();assert_eq!(w.await.unwrap().len(),5);
        assert_eq!(COLLECTION_TIMEOUT_SECONDS,45);assert_eq!(EVIDENCE_LIFETIME_SECONDS,600);
    }

    fn newer_unsupported_receipt(older: &Value) -> Value {
        use zcash_address::{ToAddress, ZcashAddress};
        let address=ZcashAddress::from_tex(zcash_protocol::consensus::NetworkType::Test,[3;20]).encode();
        let mut newer=older.clone();
        newer["id"]=json!("opid-00000000-0000-4000-8000-000000000002");
        newer["creation_time"]=json!(now().unwrap());
        newer["params"]["amounts"][0]["address"]=json!(address);
        newer
    }

    #[tokio::test]
    async fn newest_unsupported_receipt_does_not_hide_older_qualifying_receipt() {
        for reverse in [false,true] {
            let (src,mut wr,nr)=identity_receipt_responses();
            let older=wr[4][0].clone();
            let newer=newer_unsupported_receipt(&older);
            assert!(operation_amounts(&newer,&src,now().unwrap()).is_err());
            wr[4]=if reverse {json!([newer,older])} else {json!([older,newer])};
            let (wallet,w)=mock(wr,None).await; let (node,n)=mock(nr,None).await;
            let proof=collect_with_identity_proof(&wallet,&src,&node).await.unwrap();
            assert_eq!(proof.signer_readiness,SignerReadiness::IdentityOperationVerified);
            let reads=w.await.unwrap();
            assert_eq!(reads.len(),12);
            assert_eq!(reads[8]["params"],json!([["opid-00000000-0000-4000-8000-000000000001"]]));
            assert_eq!(n.await.unwrap().len(),6);
        }
    }

    #[tokio::test]
    async fn no_locally_qualifying_receipt_is_unproven_without_transaction_reads() {
        let (src,mut wr,mut nr)=identity_receipt_responses();
        let unsupported=newer_unsupported_receipt(&wr[4][0]);
        let mut malformed=wr[4][0].clone(); malformed["result"]=json!({"txid":"invalid"});
        wr[4]=json!([unsupported,malformed]); wr.truncate(5); nr.clear();
        let (wallet,w)=mock(wr,None).await; let (node,n)=mock(nr,None).await;
        assert_eq!(collect_with_identity_proof(&wallet,&src,&node).await,
            Err(ZecdFundingError::IdentitySignerNotProven));
        assert_eq!(w.await.unwrap().len(),5);
        assert_eq!(n.await.unwrap().len(),0);
    }

    #[tokio::test]
    async fn discovery_never_skips_errors_unconfirmed_malformed_or_mismatching_raw_receipts() {
        for case in 0..8 {
            let (src,mut wr,mut nr)=identity_receipt_responses();
            let mut selected=wr[4][0].clone(); let mut older=selected.clone();
            selected["creation_time"]=json!(now().unwrap());
            older["id"]=json!("opid-00000000-0000-4000-8000-000000000002");
            older["result"]["txid"]=json!("b".repeat(64));
            wr[4]=json!([older,selected]); wr.truncate(5); nr.truncate(1);
            match case {
                0=>nr[0]=json!({"mock_error":{"code":-1,"message":"synthetic failure"}}),
                1=>nr[0]=json!({"mock_error":{"code":-32601,"message":"synthetic unsupported"}}),
                2=>nr[0]["confirmations"]=json!(0),
                3=>nr[0]["confirmations"]=json!(-1),
                4=>{nr[0].as_object_mut().unwrap().remove("confirmations");},
                5=>nr[0]["txid"]=json!("b".repeat(64)),
                6=>nr[0]=json!("malformed"),
                _=>nr[0]["confirmations"]=json!("1"),
            }
            let (wallet,w)=mock(wr,None).await; let (node,n)=mock(nr,None).await;
            assert!(collect_with_identity_proof(&wallet,&src,&node).await.is_err(),"case {case}");
            assert_eq!(w.await.unwrap().len(),5);
            let reads=n.await.unwrap(); assert_eq!(reads.len(),1,"case {case}");
            assert_ne!(reads[0]["params"],json!(["b".repeat(64),1]));
        }
    }

    #[tokio::test]
    async fn identity_receipt_rejects_lost_changed_and_noncanonical_proofs() {
        for case in 0..10 {
            let (src,mut wr,mut nr)=identity_receipt_responses();
            match case {
                0 => { wr[4]=json!([]); wr.truncate(5); nr.clear(); },
                1 => { wr[8]=json!([]); wr.truncate(9); nr.truncate(3); },
                2 => { wr[7]=json!({"mock_error":{"code":-1,"message":"actor unavailable"}}); wr.truncate(8); nr.truncate(3); },
                3 => { wr[8][0]["params"]["minconf"]=json!(2); wr.truncate(9); nr.truncate(3); },
                4 => { nr[0]["txid"]=json!("b".repeat(64)); nr.truncate(1); wr.truncate(5); },
                5 => { wr[4][0]["params"]["amounts"][0]["amount"]=json!(0.002); wr.truncate(5); nr.truncate(3); },
                6 => { nr[1]["tx"]=json!(["b".repeat(64)]); nr.truncate(2); wr.truncate(5); },
                7 => { nr[2]=json!("b".repeat(64)); nr.truncate(3); wr.truncate(5); },
                // Formerly the removed post-signer re-envelope: an expired
                // unlock / absurd tip jump is now caught at the closing envelope.
                8 => { wr[10]["unlocked_until"]=json!(0); nr.truncate(3); },
                _ => { wr[11]=tip(2_000_000,"b"); wr[10]["enhanced_through"]=json!(2_000_000); nr.truncate(3); },
            }
            let (wallet,w)=mock(wr,None).await; let (node,n)=mock(nr,None).await;
            assert!(collect_with_identity_proof(&wallet,&src,&node).await.is_err(),"case {case}");
            w.await.unwrap(); n.await.unwrap();
        }
    }

    #[tokio::test]
    async fn expired_passphrase_and_unowned_source_never_fall_back_to_historical_operations() {
        for unowned in [false,true] {
            let (src,mut wr,mut nr)=identity_receipt_responses();
            if unowned { wr[3]["ismine"]=json!(false); wr.truncate(4); }
            else { wr[1]["unlocked_until"]=json!(0); wr.truncate(3); }
            nr.clear();
            let (wallet,w)=mock(wr,None).await; let (node,n)=mock(nr,None).await;
            assert!(collect_testnet_funding(&wallet,&src,&node).await.is_err());
            let requests=w.await.unwrap(); assert!(requests.iter().all(|v|v["method"]!="z_getoperationstatus"));
            n.await.unwrap();
        }
    }

    #[tokio::test]
    async fn actor_probe_accepts_only_fixed_absence_and_signer_surface_stays_read_only() {
        for code in [-5,-1,-32601,-32602,-28,-13] {
            let (rpc,r)=mock(vec![json!({"mock_error":{"code":code,"message":"hidden detail"}})],None).await;
            assert_eq!(rpc.zecd_actor_live().await.is_ok(),code==-5);
            let request=r.await.unwrap(); assert_eq!(request[0]["params"],json!(["0".repeat(64),0]));
        }
        let (rpc,r)=mock(vec![json!(true)],None).await;
        assert!(rpc.zecd_actor_live().await.is_err()); r.await.unwrap();
        let body=json!({"id":1,"result":true,"error":null}).to_string();
        let response=format!("HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).into_bytes();
        let (rpc,r)=mock(vec![Value::Null],Some(response)).await;
        assert!(rpc.zecd_actor_live().await.is_err()); r.await.unwrap();
        let (rpc,r)=mock(vec![],None).await;
        assert!(rpc.zecd_signer_read("z_sendmany",json!([])).await.is_err());
        assert!(rpc.zecd_signer_read("z_getoperationresult",json!([])).await.is_err());
        assert!(r.await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn raw_discovery_absence_is_narrow_typed_and_accepts_only_exact_error_envelopes() {
        let absent=json!({"jsonrpc":"2.0","id":1,"error":{"code":-5,"message":"synthetic absent"}});
        for omit_result in [false,true] {for success_status in [false,true] {
            let mut envelope=absent.clone();if !omit_result {envelope["result"]=Value::Null;}
            let body=envelope.to_string();let status=if success_status {"200 OK"} else {"500 Internal Server Error"};
            let wire=format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).into_bytes();
            let (rpc,r)=mock(vec![Value::Null],Some(wire)).await;
            assert_eq!(rpc.zecd_signer_raw_lookup(&"a".repeat(64)).await,Ok(None));
            assert_eq!(r.await.unwrap()[0]["params"],json!(["a".repeat(64),1]));
        }}
        for case in 0..10 {
            let mut envelope=absent.clone();let mut status="500 Internal Server Error";
            match case {
                0=>envelope["id"]=json!(2),
                1=>envelope["result"]=json!(false),
                2=>envelope["error"]["code"]=json!("-5"),
                3=>envelope["error"]["code"]=json!(-1),
                4=>envelope["error"]=json!([-5]),
                5=>{envelope["error"].as_object_mut().unwrap().remove("message");},
                6=>{envelope["error"]=Value::Null;envelope["result"]=json!({"txid":"a".repeat(64)});},
                7=>status="503 Service Unavailable",
                8=>status="302 Found",
                _=>envelope["id"]=json!("1"),
            }
            let body=envelope.to_string();
            let wire=format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).into_bytes();
            let (rpc,r)=mock(vec![Value::Null],Some(wire)).await;
            assert!(rpc.zecd_signer_raw_lookup(&"a".repeat(64)).await.is_err(),"case {case}");r.await.unwrap();
        }
        // No general error or actor interpretation changes, even with -5.
        let (rpc,r)=mock(vec![json!({"mock_error":{"code":-5,"message":"synthetic absent"}})],None).await;
        assert_eq!(rpc.zecd_signer_read("getrawtransaction",json!(["a".repeat(64),1])).await,Err(ZecdFundingError::Unavailable));r.await.unwrap();
        let (rpc,r)=mock(vec![],None).await;
        assert_eq!(rpc.zecd_signer_raw_lookup("not-a-txid").await,Err(ZecdFundingError::InvalidEvidence));
        assert!(r.await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn rpc_body_limit_applies_to_content_length_and_chunked_streams() {
        let advertised=format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",MAX_RPC_BODY_BYTES+1).into_bytes();
        let mut chunked=b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec();
        chunked.extend_from_slice(format!("{:x}\r\n",MAX_RPC_BODY_BYTES+1).as_bytes());
        chunked.extend(vec![b' ';MAX_RPC_BODY_BYTES+1]); chunked.extend_from_slice(b"\r\n0\r\n\r\n");
        for bytes in [advertised,chunked] {
            let (rpc,r)=mock(vec![Value::Null],Some(bytes)).await;
            assert_eq!(rpc.zecd_funding_read("listunspent",json!([10,u32::MAX,[],false])).await,
                Err(ZecdFundingError::ResponseTooLarge));
            r.await.unwrap();
        }
    }

    #[tokio::test]
    async fn redirects_and_nonread_methods_never_reach_another_endpoint() {
        let trap=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let response=format!("HTTP/1.1 302 Found\r\nLocation: http://{}/elsewhere\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            trap.local_addr().unwrap()).into_bytes();
        let (rpc,r)=mock(vec![Value::Null],Some(response)).await;
        assert_eq!(rpc.zecd_funding_read("getbalance",json!(["*",10])).await,Err(ZecdFundingError::Unavailable));
        r.await.unwrap();
        assert!(tokio::time::timeout(Duration::from_millis(50),trap.accept()).await.is_err());
        let (rpc,r)=mock(vec![],None).await;
        assert_eq!(rpc.zecd_funding_read("z_sendmany",json!([])).await,Err(ZecdFundingError::InvalidEvidence));
        assert!(r.await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn conventional_transport_has_separate_allowlist_and_never_retries_a_send() {
        for method in ["walletpassphrase","walletlock","z_getoperationresult","sendrawtransaction",
            "getbalance","arbitrary"] {
            let (rpc,r)=mock(vec![],None).await;
            assert_eq!(rpc.zecd_conventional_rpc(method,json!([])).await,Err(ZecdFundingError::InvalidEvidence));
            assert!(r.await.unwrap().is_empty());
        }
        let response=b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec();
        let (rpc,r)=mock(vec![Value::Null],Some(response)).await;
        assert_eq!(rpc.zecd_conventional_rpc("z_sendmany",json!(["synthetic",[],10,null,"AllowRevealedRecipients"])).await,
            Err(ZecdFundingError::Unavailable));
        let sent=r.await.unwrap(); assert_eq!(sent.len(),1); assert_eq!(sent[0]["method"],"z_sendmany");
        for method in ["z_getoperationstatus","getrawtransaction","getblock","getblockhash","gettransaction","getwalletinfo"] {
            let (rpc,r)=mock(vec![json!("synthetic-result")],None).await;
            assert_eq!(rpc.zecd_conventional_rpc(method,json!([])).await.unwrap(),json!("synthetic-result"));
            assert_eq!(r.await.unwrap().len(),1);
        }
    }

    #[tokio::test]
    async fn wallet_history_is_bounded_read_only_and_not_a_general_funding_read() {
        let (rpc,r)=mock(vec![],None).await;
        assert_eq!(rpc.zecd_funding_read("gettransaction",json!([])).await,Err(ZecdFundingError::InvalidEvidence));
        assert_eq!(rpc.zecd_signer_read("getwalletinfo",json!([])).await,Err(ZecdFundingError::InvalidEvidence));
        assert!(r.await.unwrap().is_empty());
        let (rpc,r)=mock(vec![json!({"synthetic":"wallet history"})],None).await;
        assert!(rpc.zecd_signer_read("gettransaction",json!(["a".repeat(64)])).await.is_ok());
        let reads=r.await.unwrap(); assert_eq!(reads.len(),1); assert_eq!(reads[0]["method"],"gettransaction");
        let advertised=format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",MAX_RPC_BODY_BYTES+1).into_bytes();
        let mut chunked=b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec();
        chunked.extend_from_slice(format!("{:x}\r\n",MAX_RPC_BODY_BYTES+1).as_bytes());
        chunked.extend(vec![b' ';MAX_RPC_BODY_BYTES+1]); chunked.extend_from_slice(b"\r\n0\r\n\r\n");
        for bytes in [advertised,chunked] {
            for conventional in [false,true] {
                let (rpc,r)=mock(vec![Value::Null],Some(bytes.clone())).await;
                let result=if conventional { rpc.zecd_conventional_rpc("gettransaction",json!([])).await }
                    else { rpc.zecd_signer_read("gettransaction",json!([])).await };
                assert_eq!(result,Err(ZecdFundingError::ResponseTooLarge)); r.await.unwrap();
            }
        }
    }

    #[tokio::test]
    async fn conventional_transport_refuses_redirects_and_oversized_streams() {
        let trap=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let response=format!("HTTP/1.1 302 Found\r\nLocation: http://{}/elsewhere\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            trap.local_addr().unwrap()).into_bytes();
        let (rpc,r)=mock(vec![Value::Null],Some(response)).await;
        assert_eq!(rpc.zecd_conventional_rpc("z_sendmany",json!([])).await,Err(ZecdFundingError::Unavailable));
        r.await.unwrap();
        assert!(tokio::time::timeout(Duration::from_millis(50),trap.accept()).await.is_err());
        let advertised=format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",MAX_RPC_BODY_BYTES+1).into_bytes();
        let mut chunked=b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec();
        chunked.extend_from_slice(format!("{:x}\r\n",MAX_RPC_BODY_BYTES+1).as_bytes());
        chunked.extend(vec![b' ';MAX_RPC_BODY_BYTES+1]); chunked.extend_from_slice(b"\r\n0\r\n\r\n");
        for bytes in [advertised,chunked] {
            let (rpc,r)=mock(vec![Value::Null],Some(bytes)).await;
            assert_eq!(rpc.zecd_conventional_rpc("getrawtransaction",json!([])).await,Err(ZecdFundingError::ResponseTooLarge));
            r.await.unwrap();
        }
    }
}
