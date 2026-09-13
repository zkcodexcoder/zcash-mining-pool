//! Metadata-only, sampled PPS credit gates. This is never admission authority.
//! No wallet value, identifier, address, configuration or error text is serialized.
use std::{cell::Cell, sync::{Arc, Mutex}, time::{Duration, Instant}};
use pool_db::{PoolDb, pps_live::{PpsChainLease, PpsDbError, PpsEpoch},
    pps_funding::PpsFundingLease};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use stratum::ServerMessage;
use crate::pps_funding::{PpsFundingError, PpsFundingRoute};

pub const CREDIT_HEALTH_MAX_AGE_SECONDS: i64 = 15;
pub const CREDIT_HEALTH_KEY: &str = "pps_credit_health";
const QUOTE_MAX_AGE_SECONDS: i64 = 15;

/// What the credit path is doing right now, as shown on the /pps page.
///
/// * `Ready` — shares are being priced and credited, funding and chain proofs are
///   current.
/// * `Degraded` — shares are STILL being credited (a valid share is never rejected
///   for the pool's own solvency timing), but a send-side proof is stale, so
///   payouts may be held until it recovers: a missing/expired/insufficient
///   funding lease, a generation bump after a payout, exhausted fee capacity, or
///   an operator halt.
/// * `Paused` — a hard gate is actually rejecting valid shares: the chain-agreement
///   lease is invalid, the cumulative cap is exhausted, accounting is unreadable,
///   or the next share would not fit under the cap.
/// * `Unknown` — the sampler could not determine the state at all (telemetry
///   missing, malformed, stale, or a concurrent change mid-sample). A merely
///   idle pool (no share priced in the last few seconds, or a new tip) is NOT
///   unknown; that is surfaced as informational quote timing instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreditAdmissionState { Ready, Degraded, Paused, Unknown }

/// Warnings and send-side reasons: crediting continues. Chain agreement is a
/// warning only; the rest may hold sending. Everything else in `assess` is a
/// hard credit gate and maps to `Paused`.
fn degrades_only(reason: &str) -> bool {
    matches!(reason,
        "chain_invalid" | "financial_halt" | "funding_missing" | "funding_expired" | "generation_changed"
        | "invalid_evidence" | "funding_insufficient" | "fee_capacity_exhausted")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PpsCreditHealth {
    pub version: u8,
    pub sampled_at_unix: i64,
    pub state: CreditAdmissionState,
    pub category: String,
    pub refresh_in_progress: bool,
    pub last_attempt_unix: Option<i64>,
    pub last_success_unix: Option<i64>,
    pub last_refresh_stage: String,
    pub last_refresh_result: String,
    pub funding_checked_at_unix: Option<i64>,
    pub funding_expires_at_unix: Option<i64>,
    pub funding_expiry_valid: bool,
    pub generation_matches: Option<bool>,
    pub chain_expires_at_unix: Option<i64>,
    pub chain_expiry_valid: bool,
    /// Version 2 requires explicit quote context; old heartbeats fail unknown.
    pub quote_required: bool,
    pub quote_checked_at_unix: Option<i64>,
    pub quote_expires_at_unix: Option<i64>,
    pub current_quote_fits: Option<bool>,
    /// Fixed under-20% warning only, never a balance or admission capability.
    pub budget_low: bool,
    /// Process-local count of denials at PPS-specific admission gates, not bad PoW.
    pub denial_count: u64,
    pub last_denial_category: Option<String>,
}

const CATEGORIES: &[&str] = &[
    "unknown", "not_attempted", "ok", "missing", "malformed", "stale",
    "funding_missing", "funding_expired", "chain_invalid", "generation_changed",
    "financial_halt", "payout_halted", "cap_exhausted", "fee_capacity_exhausted", "funding_insufficient",
    "accounting_unavailable", "accounting_invalid", "epoch_mismatch", "legacy_recovery",
    "invalid_input", "duplicate_mismatch", "chain_lease_required", "funding_lease_required",
    "route_policy", "unsupported_recipient", "network_mismatch", "fee_contract_unavailable",
    "rpc_unavailable", "rpc_unsupported", "invalid_evidence", "response_too_large",
    "enumeration_too_large", "identity_mismatch", "wallet_not_ready",
    "identity_signer_not_proven", "unsupported_source", "concurrent_change",
    "chain_mismatch", "deadline_exceeded", "actor_probe_missing",
    "stale_job", "subsidy_timeout", "subsidy_unavailable", "fixed_target_unavailable",
    "target_invalid", "quote_invalid", "quote_missing", "quote_stale",
    "quote_context_changed", "current_quote_insufficient",
];
const STAGES: &[&str] = &[
    "not_attempted", "collection_start", "complete", "route_policy", "funding_before",
    "wallet_collect", "funding_after", "funding_finish", "credit_capacity",
    "pczt_contract", "pczt_network", "pczt_signer",
    "source_input", "diagnostic_clock", "wallet_identity_read", "wallet_identity_check",
    "opening_anchor_read", "opening_anchor_check", "opening_metadata_read",
    "opening_metadata_check", "opening_source_read", "opening_source_check",
    "balance_read", "inventory_read", "balance_inventory_read", "inventory_check",
    "closing_anchor_read", "closing_anchor_check", "closing_metadata_read",
    "closing_metadata_check", "funding_bracket_check", "node_tip_read", "node_tip_check",
    "funding_anchor_canonical_read", "funding_anchor_canonical_check",
    "diagnostic_final_freshness", "evidence_final_freshness", "signer_operations_read",
    "signer_operation_selection", "signer_raw_read", "signer_raw_check", "signer_block_read",
    "signer_block_check", "signer_canonical_read", "signer_canonical_check",
    "signer_history_actor_read", "signer_history_read", "signer_actor_read",
    "signer_history_check", "signer_history_canonical_read", "signer_history_canonical_check",
    "signer_transparent_receipt_check", "signer_final_operation_read",
    "signer_final_operation_check", "signer_final_metadata_read", "signer_final_metadata_check",
    "signer_final_anchor_read", "signer_final_anchor_check", "signer_final_source_read",
    "signer_final_source_check",
    "signer_closing_canonical_read", "signer_closing_canonical_check",
];

impl PpsCreditHealth {
    fn unknown(now: i64, category: &str) -> Self {
        Self { version:2, sampled_at_unix:now.max(0), state:CreditAdmissionState::Unknown,
            category:category.into(), refresh_in_progress:false, last_attempt_unix:None,
            last_success_unix:None, last_refresh_stage:"not_attempted".into(),
            last_refresh_result:"not_attempted".into(), funding_checked_at_unix:None,
            funding_expires_at_unix:None, funding_expiry_valid:false, generation_matches:None,
            chain_expires_at_unix:None, chain_expiry_valid:false, denial_count:0,
            quote_required:false, quote_checked_at_unix:None, quote_expires_at_unix:None,
            current_quote_fits:None, budget_low:false,
            last_denial_category:None }
    }
}

/// Never forwards malformed/private strings. Rechecks original deadlines at read
/// time: a recently published green heartbeat cannot survive lease expiration.
pub fn decode_credit_health(raw: Option<&str>, now_unix: i64) -> PpsCreditHealth {
    let Some(raw) = raw else { return PpsCreditHealth::unknown(now_unix,"missing"); };
    let parsed = (raw.len() <= 8192).then(|| serde_json::from_str::<PpsCreditHealth>(raw));
    let Some(Ok(mut h)) = parsed else { return PpsCreditHealth::unknown(now_unix,"malformed"); };
    if h.version != 2 || !CATEGORIES.contains(&h.category.as_str())
        || !CATEGORIES.contains(&h.last_refresh_result.as_str())
        || !STAGES.contains(&h.last_refresh_stage.as_str())
        || h.last_denial_category.as_deref().is_some_and(|v| !CATEGORIES.contains(&v))
        || [h.last_attempt_unix,h.last_success_unix,h.funding_checked_at_unix]
            .into_iter().flatten().any(|v| v < 0 || v > now_unix)
        || [h.funding_expires_at_unix,h.chain_expires_at_unix]
            .into_iter().flatten().any(|v| v < 0)
        || h.funding_expires_at_unix.zip(h.funding_checked_at_unix)
            .is_some_and(|(end,start)| end.checked_sub(start).is_none_or(|d| !(1..=pool_db::pps_funding::FUNDING_LEASE_SECONDS).contains(&d)))
        || h.funding_expires_at_unix.is_some() != h.funding_checked_at_unix.is_some()
        || h.chain_expires_at_unix.is_some_and(|end| end.checked_sub(now_unix).is_none_or(|d|d>pool_db::pps_funding::CHAIN_LEASE_SECONDS))
        || h.quote_checked_at_unix.is_some_and(|v|v<0 || v>now_unix)
        || h.quote_checked_at_unix.is_some() != h.quote_expires_at_unix.is_some()
        || h.quote_expires_at_unix.zip(h.quote_checked_at_unix)
            .is_some_and(|(end,start)| end.checked_sub(start)!=Some(QUOTE_MAX_AGE_SECONDS))
        || (!h.quote_required && (h.quote_checked_at_unix.is_some() || h.current_quote_fits.is_some()))
    { return PpsCreditHealth::unknown(now_unix,"malformed"); }
    if now_unix < 0 || h.sampled_at_unix < 0 || h.sampled_at_unix > now_unix
        || now_unix.checked_sub(h.sampled_at_unix).is_none_or(|d|d>CREDIT_HEALTH_MAX_AGE_SECONDS)
    { return PpsCreditHealth::unknown(now_unix,"stale"); }
    h.funding_expiry_valid &= h.funding_checked_at_unix.is_some_and(|v| v <= now_unix)
        && h.funding_expires_at_unix.is_some_and(|v| now_unix < v);
    h.chain_expiry_valid &= h.chain_expires_at_unix.is_some_and(|v| now_unix < v);
    if matches!(h.state, CreditAdmissionState::Ready | CreditAdmissionState::Degraded) {
        // Chain agreement is a warning only: a lapsed proof degrades, never pauses.
        if !h.chain_expiry_valid { h.state=CreditAdmissionState::Degraded; h.category="chain_invalid".into(); }
        else if h.state == CreditAdmissionState::Ready {
            // A lapsed funding lease only degrades: shares are still credited,
            // sends may be held until it recovers.
            if !h.funding_expiry_valid { h.state=CreditAdmissionState::Degraded; h.category="funding_expired".into(); }
            else if h.generation_matches != Some(true) || h.category != "ok" {
                return PpsCreditHealth::unknown(now_unix,"malformed");
            }
        }
    }
    // Price telemetry is informational (when the last share was priced) and never
    // downgrades a healthy state to Unknown. The one price gate — a fresh quote
    // that would not fit under the cap — is a hard pause; once that quote is no
    // longer current its verdict is unknown, not still-failing.
    if h.quote_required {
        if h.quote_expires_at_unix.is_none_or(|end| now_unix>=end) { h.current_quote_fits=None; }
        if h.current_quote_fits==Some(false) {
            h.state=CreditAdmissionState::Paused; h.category="current_quote_insufficient".into();
        } else if h.category=="current_quote_insufficient" && h.current_quote_fits==Some(true) {
            return PpsCreditHealth::unknown(now_unix,"malformed");
        }
    }
    h
}

/// One bounded private observation of an actually validated share price. Never
/// Debug/Serialize, persisted, returned to an API, or consulted by admission.
#[derive(Clone, PartialEq, Eq)]
struct QuoteObservation {
    job_id:String, prev_hash:String, amount:u128, checked_at:i64, checked:Instant,
}
#[derive(Clone, PartialEq, Eq)]
struct QuoteJob { job_id:String, prev_hash:String }
impl QuoteJob {
    fn new(job_id:&str,prev_hash:&str)->Option<Self> {
        if job_id.is_empty() || job_id.len()>128
            || !job_id.bytes().all(|v|v.is_ascii_alphanumeric() || matches!(v,b'-'|b'_'))
            || prev_hash.len()!=64 || !prev_hash.bytes().all(|v|v.is_ascii_hexdigit()) {return None;}
        Some(Self {job_id:job_id.into(),prev_hash:prev_hash.into()})
    }
    fn from_notify(notify:Option<&ServerMessage>)->Option<Self> {
        match notify {Some(ServerMessage::Notify {job_id,prev_hash,..})=>Self::new(job_id,prev_hash),_=>None}
    }
}
pub(crate) fn quote_required(epoch:&PpsEpoch,route:&PpsFundingRoute)->bool {
    epoch.network=="testnet" && route.holds_new_legacy_sends()
}
pub(crate) fn validated_quote(health:&SharedCreditHealth,job_id:&str,prev_hash:&str,
    amount:u128,checked_at:i64,checked:Instant) {
    if let Ok(mut h)=health.lock() {
        h.quote=QuoteJob::new(job_id,prev_hash).filter(|_|amount>0 && checked_at>=0
            && checked_at.checked_add(QUOTE_MAX_AGE_SECONDS).is_some())
            .map(|job|QuoteObservation {job_id:job.job_id,prev_hash:job.prev_hash,amount,checked_at,checked});
    }
}
fn quote_snapshot(health:&SharedCreditHealth)->Option<QuoteObservation> {
    health.lock().ok().and_then(|h|h.quote.clone())
}
fn same_quote(a:Option<&QuoteObservation>,b:Option<&QuoteObservation>)->bool {
    match (a,b) {
        (None,None)=>true,
        (Some(a),Some(b))=>a.job_id==b.job_id && a.prev_hash==b.prev_hash && a.amount==b.amount,
        _=>false,
    }
}
fn assess_quote(h:&mut PpsCreditHealth,quote:Option<&QuoteObservation>,job:Option<&QuoteJob>,
    context_unchanged:bool,unused:u128,now:i64,instant:Instant) {
    if !h.quote_required {return;}
    // Informational: when the last share was priced. A pool that has not priced
    // a share in the last few seconds, or whose tip just moved, is idle — never
    // "unknown" — so this records timing only and changes no state.
    let Some(q)=quote else {return;};
    h.quote_checked_at_unix=Some(q.checked_at);
    h.quote_expires_at_unix=q.checked_at.checked_add(QUOTE_MAX_AGE_SECONDS);
    // A hard pause takes precedence over the price gate.
    if matches!(h.state, CreditAdmissionState::Paused | CreditAdmissionState::Unknown) {return;}
    // Only a FRESH quote for the CURRENT job says anything about the next share.
    let fresh = context_unchanged
        && job.is_some_and(|j| j.job_id==q.job_id && j.prev_hash==q.prev_hash)
        && now>=q.checked_at
        && now.checked_sub(q.checked_at).is_some_and(|v| v<QUOTE_MAX_AGE_SECONDS)
        && instant.checked_duration_since(q.checked)
            .is_some_and(|v| v<Duration::from_secs(QUOTE_MAX_AGE_SECONDS as u64));
    if !fresh {
        // Audit B14: headroom below the price of one share rejects EVERY share
        // (CapExceeded) even though unused is not exactly zero. At a fixed target the
        // last observed price is a sound proxy for the next share, fresh or not.
        if q.amount>unused {
            h.state=CreditAdmissionState::Paused;
            h.category="cap_exhausted".into();
        }
        return;
    }
    h.current_quote_fits=Some(q.amount<=unused);
    if q.amount>unused {
        // The next share would not fit under the cumulative cap. That IS a hard
        // credit gate (CapExceeded rejects it), so it pauses.
        h.state=CreditAdmissionState::Paused;
        h.category="current_quote_insufficient".into();
    }
}

#[derive(Default)]
pub(crate) struct CreditHealthTracker {
    last_attempt: Option<i64>, last_success: Option<i64>, refreshing: bool,
    stage: Option<&'static str>, result: Option<&'static str>,
    denial_count: u64, last_denial: Option<&'static str>, last_denial_log: Option<i64>,
    quote: Option<QuoteObservation>,
}
pub(crate) type SharedCreditHealth = Arc<Mutex<CreditHealthTracker>>;
impl CreditHealthTracker {
    pub(crate) fn begin(&mut self, now: i64) { self.last_attempt=Some(now); self.refreshing=true; }
    pub(crate) fn finish(&mut self, now:i64, stage:&'static str, result:&'static str) {
        self.refreshing=false; self.stage=Some(stage); self.result=Some(result);
        if result == "ok" { self.last_success=Some(now); }
    }
}
pub(crate) fn denial(health:&SharedCreditHealth, stage:&'static str, category:&'static str) {
    let now=chrono::Utc::now().timestamp();
    if let Ok(mut h)=health.lock() {
        h.denial_count=h.denial_count.saturating_add(1);
        if h.last_denial != Some(category) || h.last_denial_log.is_none_or(|t| now.saturating_sub(t)>=30) {
            tracing::warn!(stage,category,"PPS credit admission denied"); h.last_denial_log=Some(now);
        }
        h.last_denial=Some(category);
    }
}
pub(crate) fn db_category(error:&PpsDbError) -> &'static str {
    match error {
        PpsDbError::Invalid=>"invalid_input", PpsDbError::EpochMismatch=>"epoch_mismatch",
        PpsDbError::LegacyRecoveryRequired=>"legacy_recovery", PpsDbError::DuplicateMismatch=>"duplicate_mismatch",
        PpsDbError::ChainLeaseRequired=>"chain_lease_required", PpsDbError::CapExceeded=>"cap_exhausted",
        PpsDbError::FundingLeaseRequired=>"funding_lease_required", PpsDbError::FundingInsufficient=>"funding_insufficient",
        PpsDbError::FeeBudgetExceeded=>"fee_capacity_exhausted", PpsDbError::Invariant=>"accounting_invalid",
        PpsDbError::PayoutHalted=>"payout_halted",
        PpsDbError::Database(_)=>"accounting_unavailable",
    }
}
pub(crate) fn funding_category(error:&PpsFundingError) -> &'static str {
    match error {
        PpsFundingError::RoutePolicy=>"route_policy", PpsFundingError::UnsupportedRecipient=>"unsupported_recipient",
        PpsFundingError::Timeout=>"deadline_exceeded", PpsFundingError::WalletUnavailable=>"rpc_unavailable",
        PpsFundingError::NetworkMismatch=>"network_mismatch", PpsFundingError::FeeContractUnavailable=>"fee_contract_unavailable",
        PpsFundingError::AccountingUnavailable=>"accounting_unavailable", PpsFundingError::ConcurrentChange=>"concurrent_change",
        PpsFundingError::InsufficientFunding=>"funding_insufficient", PpsFundingError::InvalidEvidence=>"invalid_evidence",
        PpsFundingError::IdentitySignerNotProven=>"identity_signer_not_proven", PpsFundingError::FeeCapacityExhausted=>"fee_capacity_exhausted",
    }
}
tokio::task_local! { static REFRESH_STAGE: Cell<(&'static str, Option<&'static str>)>; }
pub(crate) fn stage(stage:&'static str) { let _=REFRESH_STAGE.try_with(|s| s.set((stage,None))); }
pub(crate) fn current_stage() -> Option<&'static str> {
    REFRESH_STAGE.try_with(|s|s.get().0).ok()
}
pub(crate) fn wallet_failure(stage:&'static str, category:&'static str) {
    let _=REFRESH_STAGE.try_with(|s| s.set((stage,Some(category))));
    tracing::warn!(stage,category,"PPS funding wallet evidence rejected");
}
pub(crate) async fn observe_refresh<F>(future:F) -> (Result<PpsFundingLease,PpsFundingError>, &'static str, &'static str)
where F:std::future::Future<Output=Result<PpsFundingLease,PpsFundingError>> {
    REFRESH_STAGE.scope(Cell::new(("collection_start",None)),async {
        let result=future.await;
        let (stage,detail)=REFRESH_STAGE.with(Cell::get);
        let (stage,category)=match &result {
            Ok(_)=>("complete","ok"), Err(error)=>(stage,detail.unwrap_or_else(||funding_category(error))),
        };
        (result,stage,category)
    }).await
}

fn assess(h:&mut PpsCreditHealth, epoch:&PpsEpoch, route:&PpsFundingRoute,
    lease:Option<&PpsFundingLease>, snapshot:&pool_db::pps_funding::PpsCreditReadinessSnapshot, now:i64) {
    h.quote_required=quote_required(epoch,route);
    h.budget_low=h.quote_required && snapshot.funding.as_ref().is_some_and(|s|
        s.cap_subzatoshis>0 && s.unused_credit_subzatoshis
            < s.cap_subzatoshis/5 + u128::from(s.cap_subzatoshis%5!=0));
    h.generation_matches=lease.map(|l| l.generation==snapshot.generation);
    // Audit B14: report the WORST state. Gates that actually reject valid shares
    // (unreadable accounting, exhausted cap) are evaluated before the reasons that
    // only degrade, so a warning can never mask a hard rejection. Chain agreement
    // is a warning only and is reported first among those.
    let funding=snapshot.funding.as_ref();
    let reason=if funding.is_none() { Some("accounting_invalid") }
        else if funding.is_some_and(|s| s.unused_credit_subzatoshis==0) { Some("cap_exhausted") }
        else if !h.chain_expiry_valid { Some("chain_invalid") }
        else if snapshot.financial_halt { Some("financial_halt") }
        else if lease.is_none() { Some("funding_missing") }
        else if !h.funding_expiry_valid { Some("funding_expired") }
        else if h.generation_matches != Some(true) { Some("generation_changed") }
        else if let (Some(l),Some(s))=(lease,funding) {
            if crate::pps_funding::validate_funding_lease(l,epoch,now).is_err() { Some("invalid_evidence") }
            else if l.spendable_zatoshis < s.required_spendable_zatoshis { Some("funding_insufficient") }
            else if s.paid_fees_zatoshis.checked_add(s.reserved_fees_zatoshis)
                .is_none_or(|v|v>=s.fee_allowance_zatoshis)
                || crate::pps_funding::validate_credit_fee_capacity(route,s).is_err() { Some("fee_capacity_exhausted") }
            else { None }
        } else { Some("accounting_invalid") };
    h.state=match reason {
        None => CreditAdmissionState::Ready,
        Some(r) if degrades_only(r) => CreditAdmissionState::Degraded,
        Some(_) => CreditAdmissionState::Paused,
    };
    h.category=reason.unwrap_or("ok").into();
}

fn same_chain(a:Option<&PpsChainLease>,b:Option<&PpsChainLease>)->bool {
    match (a,b) {
        (None,None)=>true,
        (Some(a),Some(b))=>a.network==b.network && a.checked_at_unix==b.checked_at_unix
            && a.valid_until_unix==b.valid_until_unix && a.agreeing_references==b.agreeing_references
            && a.disagreement==b.disagreement,
        _=>false,
    }
}

/// Bounded read-only telemetry sample, never an admission authorization.
pub(crate) async fn sample_health(db:&PoolDb,epoch:&PpsEpoch,route:&PpsFundingRoute,
    chain:&Arc<RwLock<Option<PpsChainLease>>>,funding:&Arc<RwLock<Option<PpsFundingLease>>>,tracker:&SharedCreditHealth,
    latest:&Arc<RwLock<Option<ServerMessage>>>)->PpsCreditHealth {
        let sampled=chrono::Utc::now().timestamp();
        let mut h=PpsCreditHealth::unknown(sampled,"accounting_unavailable");
        // The entire observation is bounded; no locks are held across DB work.
        let observation=tokio::time::timeout(Duration::from_secs(2),async {
            let l=funding.read().await.clone(); let c=chain.read().await.clone();
            let q=quote_snapshot(tracker);
            let job=QuoteJob::from_notify(latest.read().await.as_ref());
            let s=db.pps_credit_readiness_snapshot(epoch).await?;
            // A revoked/replaced proof while the snapshot was pending must not
            // produce green from the previously cloned cache. This remains a
            // point-in-time observation, never a lease for the reader.
            let cache_unchanged=l==*funding.read().await
                && same_chain(c.as_ref(),chain.read().await.as_ref());
            // Another share at the same job/price may refresh the tracker.
            // Keep q's ORIGINAL times; neither renew it nor self-invalidate
            // busy mining merely because an equivalent price was observed.
            let quote_unchanged=same_quote(q.as_ref(),quote_snapshot(tracker).as_ref())
                && job==QuoteJob::from_notify(latest.read().await.as_ref());
            Ok::<_,PpsDbError>((l,c,s,cache_unchanged,q,job,quote_unchanged))
        }).await;
        let now=chrono::Utc::now().timestamp();
        if let Ok(Ok((l,c,s,cache_unchanged,q,job,quote_unchanged)))=&observation {
            h.funding_checked_at_unix=l.as_ref().map(|v|v.checked_at_unix);
            h.funding_expires_at_unix=l.as_ref().map(|v|v.valid_until_unix);
            h.funding_expiry_valid=l.as_ref().is_some_and(|v| v.checked_at_unix>=0
                && v.checked_at_unix<=now && now<v.valid_until_unix
                && v.valid_until_unix.checked_sub(v.checked_at_unix).is_some_and(|d|(1..=pool_db::pps_funding::FUNDING_LEASE_SECONDS).contains(&d)));
            h.chain_expires_at_unix=c.as_ref().map(|v|v.valid_until_unix);
            h.chain_expiry_valid=c.as_ref().is_some_and(|v| v.network==epoch.network && !v.disagreement
                && v.agreeing_references>=2 && v.checked_at_unix>=0 && v.checked_at_unix<=now
                && now<v.valid_until_unix && v.valid_until_unix.checked_sub(v.checked_at_unix)
                    .is_some_and(|d|(1..=pool_db::pps_funding::CHAIN_LEASE_SECONDS).contains(&d)));
            assess(&mut h,epoch,route,l.as_ref(),s,now);
            if let Some(snapshot)=&s.funding {
                assess_quote(&mut h,q.as_ref(),job.as_ref(),*quote_unchanged,
                    snapshot.unused_credit_subzatoshis,now,Instant::now());
            }
            if !cache_unchanged { h.state=CreditAdmissionState::Unknown; h.category="concurrent_change".into(); }
        } else if let Ok(Err(error))=&observation { h.category=db_category(error).into(); }
        if let Ok(t)=tracker.lock() {
            h.last_attempt_unix=t.last_attempt; h.last_success_unix=t.last_success;
            h.refresh_in_progress=t.refreshing; h.last_refresh_stage=t.stage.unwrap_or("not_attempted").into();
            h.last_refresh_result=t.result.unwrap_or("not_attempted").into(); h.denial_count=t.denial_count;
            h.last_denial_category=t.last_denial.map(str::to_string);
        } else { h.state=CreditAdmissionState::Unknown; h.category="unknown".into(); }
        h
}

pub(crate) async fn publish_loop(db:&PoolDb,epoch:&PpsEpoch,route:&PpsFundingRoute,
    chain:&Arc<RwLock<Option<PpsChainLease>>>,funding:&Arc<RwLock<Option<PpsFundingLease>>>,tracker:&SharedCreditHealth,
    latest:&Arc<RwLock<Option<ServerMessage>>>) {
    loop {
        let started=Instant::now();
        let h=sample_health(db,epoch,route,chain,funding,tracker,latest).await;
        // sampled_at predates DB work. Never make a slow observation look newer.
        if let Ok(raw)=serde_json::to_string(&h) {
            let _=tokio::time::timeout(Duration::from_secs(1),db.set_pool_status(CREDIT_HEALTH_KEY,&raw)).await;
        }
        tokio::time::sleep(Duration::from_secs(5).saturating_sub(started.elapsed())).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ready() -> PpsCreditHealth {
        let mut h=PpsCreditHealth::unknown(100,"ok");
        h.state=CreditAdmissionState::Ready; h.funding_checked_at_unix=Some(80);
        h.funding_expires_at_unix=Some(140); h.funding_expiry_valid=true;
        h.chain_expires_at_unix=Some(170); h.chain_expiry_valid=true;
        h.generation_matches=Some(true); h
    }
    fn decode(h:&PpsCreditHealth,now:i64)->PpsCreditHealth {
        decode_credit_health(Some(&serde_json::to_string(h).unwrap()),now)
    }
    #[test]
    fn credit_health_missing_malformed_future_stale_never_green() {
        assert_eq!(decode_credit_health(None,100).state,CreditAdmissionState::Unknown);
        for raw in ["{}","not json",r#"{"source":"private-sentinel"}"#] {
            let h=decode_credit_health(Some(raw),100);
            assert_eq!(h.state,CreditAdmissionState::Unknown);
            assert!(!serde_json::to_string(&h).unwrap().contains("private-sentinel"));
        }
        assert_eq!(decode(&ready(),99).state,CreditAdmissionState::Unknown);
        assert_eq!(decode(&ready(),115).state,CreditAdmissionState::Ready);
        assert_eq!(decode(&ready(),116).state,CreditAdmissionState::Unknown);
        for field in ["category","last_refresh_stage","last_refresh_result","last_denial_category"] {
            let mut value=serde_json::to_value(ready()).unwrap(); value[field]="private-sentinel".into();
            let h=decode_credit_health(Some(&value.to_string()),100);
            assert_eq!(h.state,CreditAdmissionState::Unknown);
            assert!(!serde_json::to_string(&h).unwrap().contains("private-sentinel"));
        }
    }
    #[test]
    fn credit_health_reader_rechecks_both_original_expiries_and_generation() {
        let mut h=ready(); h.sampled_at_unix=139;
        assert_eq!(decode(&h,139).state,CreditAdmissionState::Ready);
        let expired=decode(&h,140);
        // A lapsed funding lease only degrades now: shares are still credited,
        // sends may be held. It is no longer reported as a pause.
        assert_eq!(expired.state,CreditAdmissionState::Degraded);
        assert_eq!(expired.category,"funding_expired");
        assert_eq!(expired.funding_expires_at_unix,Some(140));
        h.chain_expires_at_unix=Some(139);
        // Chain agreement is a warning only: a lapsed proof degrades, never pauses.
        let lapsed=decode(&h,139);
        assert_eq!((lapsed.state,lapsed.category.as_str()),(CreditAdmissionState::Degraded,"chain_invalid"));
        h=ready(); h.generation_matches=Some(false);
        assert_eq!(decode(&h,100).state,CreditAdmissionState::Unknown);
        // A lease span beyond the funding-lease ceiling is incoherent -> malformed.
        h=ready(); h.funding_expires_at_unix=Some(80+pool_db::pps_funding::FUNDING_LEASE_SECONDS+1);
        assert_eq!(decode(&h,100).state,CreditAdmissionState::Unknown);
    }
    #[test]
    fn credit_health_decoder_handles_extreme_untrusted_clocks_without_panics() {
        for now in [i64::MIN,-1,0,100,i64::MAX] {
            for timestamp in [i64::MIN,-1,0,100,i64::MAX] {
                let mut h=ready(); h.funding_checked_at_unix=Some(timestamp);
                h.funding_expires_at_unix=Some(timestamp); h.chain_expires_at_unix=Some(timestamp);
                assert_ne!(decode(&h,now).state,CreditAdmissionState::Ready);
            }
        }
        // A refresh may finish during the bounded read-side snapshot. Metadata
        // from its completion is valid without restamping sampled_at or a lease.
        let mut h=ready(); h.last_attempt_unix=Some(100); h.last_success_unix=Some(101);
        assert_eq!(decode(&h,101).state,CreditAdmissionState::Ready);
        assert_eq!(decode(&h,101).sampled_at_unix,100);
    }
    #[test]
    fn credit_health_cache_identity_checks_replacement_revocation_and_disagreement() {
        let l=PpsChainLease {network:"testnet".into(),checked_at_unix:80,valid_until_unix:170,
            agreeing_references:2,disagreement:false};
        assert!(same_chain(Some(&l),Some(&l)));
        assert!(!same_chain(Some(&l),None));
        let mut changed=l.clone(); changed.checked_at_unix+=1;
        assert!(!same_chain(Some(&l),Some(&changed)));
        changed=l.clone(); changed.disagreement=true;
        assert!(!same_chain(Some(&l),Some(&changed)));
    }
    fn state_fixture() -> (PpsEpoch,PpsFundingLease,pool_db::pps_funding::PpsCreditReadinessSnapshot) {
        let e=PpsEpoch {id:"synthetic".into(),network:"testnet".into(),fee_bps:0,
            max_liability_zatoshis:95_000_000_000,total_exposure_zatoshis:100_000_000_000,
            fee_allowance_zatoshis:5_000_000_000,reserve_floor_zatoshis:1,quote_provenance:"synthetic".into()};
        let l=PpsFundingLease {network:e.network.clone(),checked_at_unix:80,valid_until_unix:140,
            spendable_zatoshis:100_000_000_001,reserve_floor_zatoshis:1,
            reserved_fee_allowance_zatoshis:5_000_000_000,generation:7};
        let s=pool_db::pps_funding::PpsFundingSnapshot {network:e.network.clone(),generation:7,
            legacy_pending_zatoshis:0,legacy_paying_zatoshis:0,pps_outstanding_subzatoshis:0,
            unused_credit_subzatoshis:95_000_000_000_u128*pool_db::pps_live::PPS_SCALE,
            gross_subzatoshis:0,paid_zatoshis:0,cap_subzatoshis:95_000_000_000_u128*pool_db::pps_live::PPS_SCALE,
            total_exposure_zatoshis:100_000_000_000,reserve_floor_zatoshis:1,fee_allowance_zatoshis:5_000_000_000,
            paid_fees_zatoshis:0,reserved_fees_zatoshis:0,required_spendable_zatoshis:100_000_000_001};
        (e,l,pool_db::pps_funding::PpsCreditReadinessSnapshot {generation:7,financial_halt:false,funding:Some(s)})
    }
    #[test]
    fn credit_health_accounts_for_generation_backing_cap_halt_and_next_batch() {
        let route=PpsFundingRoute::ZecdConventionalTestnet{hold_new_legacy_sends:true};
        let (e,l,mut s)=state_fixture(); let mut h=ready();
        assess(&mut h,&e,&route,Some(&l),&s,100); assert_eq!(h.state,CreditAdmissionState::Ready);
        s.generation+=1; assess(&mut h,&e,&route,Some(&l),&s,100); assert_eq!(h.category,"generation_changed");
        s.generation=7; s.funding.as_mut().unwrap().required_spendable_zatoshis+=1;
        assess(&mut h,&e,&route,Some(&l),&s,100); assert_eq!(h.category,"funding_insufficient");
        s.funding.as_mut().unwrap().required_spendable_zatoshis-=1;
        s.funding.as_mut().unwrap().unused_credit_subzatoshis=0;
        assess(&mut h,&e,&route,Some(&l),&s,100); assert_eq!(h.category,"cap_exhausted");
        s.funding.as_mut().unwrap().unused_credit_subzatoshis=1;
        s.funding.as_mut().unwrap().reserved_fees_zatoshis=4_980_000_000;
        assess(&mut h,&e,&route,Some(&l),&s,100); assert_eq!(h.category,"fee_capacity_exhausted");
        s.funding.as_mut().unwrap().reserved_fees_zatoshis=0;
        // A halt fences sends only: crediting continues, so it degrades.
        s.financial_halt=true;
        assess(&mut h,&e,&route,Some(&l),&s,100);
        assert_eq!((h.state,h.category.as_str()),(CreditAdmissionState::Degraded,"financial_halt"));
        // Audit B14: the WORST state is reported. Gates that actually reject shares
        // win over send-side reasons that only degrade.
        s.funding.as_mut().unwrap().unused_credit_subzatoshis=0;
        assess(&mut h,&e,&route,Some(&l),&s,100);
        assert_eq!((h.state,h.category.as_str()),(CreditAdmissionState::Paused,"cap_exhausted"));
        s.funding.as_mut().unwrap().unused_credit_subzatoshis=1;
        s.funding.as_mut().unwrap().required_spendable_zatoshis+=1;
        h.chain_expiry_valid=false;
        assess(&mut h,&e,&route,Some(&l),&s,100);
        // Chain agreement is a warning only: it degrades and is reported first
        // among warnings (here ahead of funding_insufficient)...
        assert_eq!((h.state,h.category.as_str()),(CreditAdmissionState::Degraded,"chain_invalid"));
        // ...while a gate that really rejects shares still outranks it.
        s.funding.as_mut().unwrap().unused_credit_subzatoshis=0;
        assess(&mut h,&e,&route,Some(&l),&s,100);
        assert_eq!((h.state,h.category.as_str()),(CreditAdmissionState::Paused,"cap_exhausted"));
        s.funding.as_mut().unwrap().unused_credit_subzatoshis=1;
        h.chain_expiry_valid=true;
        // Unreadable accounting rejects credits, so it outranks a halt too.
        s.funding=None;
        assess(&mut h,&e,&route,Some(&l),&s,100);
        assert_eq!((h.state,h.category.as_str()),(CreditAdmissionState::Paused,"accounting_invalid"));
    }
    #[tokio::test]
    async fn credit_refresh_diagnostics_keep_exact_failure_before_collapse_and_are_task_local() {
        let first=observe_refresh(async {
            stage("wallet_collect"); tokio::task::yield_now().await;
            wallet_failure("opening_anchor_check","wallet_not_ready");
            Err(PpsFundingError::WalletUnavailable)
        });
        let second=observe_refresh(async {
            stage("funding_before"); tokio::task::yield_now().await;
            Err(PpsFundingError::AccountingUnavailable)
        });
        let (a,b)=tokio::join!(first,second);
        assert_eq!((a.1,a.2),("opening_anchor_check","wallet_not_ready"));
        assert_eq!((b.1,b.2),("funding_before","accounting_unavailable"));
        assert_eq!(a.0,Err(PpsFundingError::WalletUnavailable));
    }
    #[test]
    fn credit_denials_are_counted_without_serializing_error_text() {
        let tracker=Arc::new(Mutex::new(CreditHealthTracker::default()));
        denial(&tracker,"ledger_credit","funding_lease_required");
        denial(&tracker,"subsidy","subsidy_timeout");
        let mut h=tracker.lock().unwrap(); assert_eq!(h.denial_count,2);
        assert_eq!(h.last_denial,Some("subsidy_timeout"));
        h.begin(100); h.finish(134,"complete","ok"); h.begin(135);
        h.finish(140,"opening_anchor_check","wallet_not_ready");
        assert_eq!(h.last_success,Some(134)); assert_eq!(h.last_attempt,Some(135));
        assert!(!h.refreshing); assert_eq!(h.result,Some("wallet_not_ready"));
    }
    fn quote_fixture() -> (QuoteObservation,QuoteJob) {
        let job=QuoteJob::new("42",&"a".repeat(64)).unwrap();
        let quote=QuoteObservation {job_id:job.job_id.clone(),prev_hash:job.prev_hash.clone(),
            amount:10,checked_at:100,checked:Instant::now()};
        (quote,job)
    }
    #[test]
    fn validated_current_quote_requires_full_capacity_not_merely_nonzero_budget() {
        let (quote,job)=quote_fixture();
        for (unused,state,category) in [(9,CreditAdmissionState::Paused,"current_quote_insufficient"),
            (10,CreditAdmissionState::Ready,"ok"),(11,CreditAdmissionState::Ready,"ok")] {
            let mut h=ready(); h.quote_required=true;
            assess_quote(&mut h,Some(&quote),Some(&job),true,unused,100,quote.checked);
            assert_eq!(h.state,state); assert_eq!(h.category,category);
            assert_eq!(h.current_quote_fits,Some(unused>=quote.amount));
            assert_eq!(h.quote_expires_at_unix,Some(115));
        }
    }
    #[test]
    fn quote_gaps_are_informational_and_never_unknown() {
        // A missing quote, a quote for a different job/tip, a changed context, or
        // an expired quote clock all mean the pool is merely idle between priced
        // shares (or the tip moved). That is NOT unknown: the state stays as it
        // was and only the fresh-quote verdict is withheld.
        let (quote,job)=quote_fixture();
        for case in 0..8 {
            let mut h=ready(); h.quote_required=true;
            let mut changed=job.clone();
            if case==1 {changed.job_id="43".into();}
            if case==2 {changed.prev_hash="b".repeat(64);}
            assess_quote(&mut h,if case==0 {None}else{Some(&quote)},Some(&changed),case!=3,100,
                if case==4 {115}else if case==5 {99}else{100},
                if case==6 {quote.checked+Duration::from_secs(15)} else if case==7 {quote.checked-Duration::from_secs(1)}else{quote.checked});
            assert_eq!(h.state,CreditAdmissionState::Ready,"case {case}");
            assert_eq!(h.category,"ok","case {case}");
            assert!(h.current_quote_fits.is_none(),"case {case}");
        }
        let mut h=ready(); h.quote_required=true;
        assess_quote(&mut h,Some(&quote),Some(&job),true,10,114,quote.checked+Duration::from_secs(14));
        assert_eq!(h.state,CreditAdmissionState::Ready);
        // Once the quote clock lapses at read time the verdict is withheld, but
        // the healthy state is untouched — no "quote_stale" downgrade.
        let lapsed=decode(&h,115);
        assert_eq!(lapsed.state,CreditAdmissionState::Ready);
        assert_eq!(lapsed.category,"ok");
        assert!(lapsed.current_quote_fits.is_none());
        h.sampled_at_unix=114; // publishing again cannot restamp the quote
        assert_eq!(decode(&h,115).state,CreditAdmissionState::Ready);
        // A recorded hard pause (next share would not fit the cap) stands after
        // the quote lapses: the sampler's verdict is kept until it resamples.
        h.current_quote_fits=Some(false); h.state=CreditAdmissionState::Paused; h.category="current_quote_insufficient".into();
        assert_eq!(decode(&h,115).state,CreditAdmissionState::Paused);
        // But a recorded pause that contradicts a fresh fitting quote is malformed.
        h.current_quote_fits=Some(true);
        assert_eq!(decode(&h,110).state,CreditAdmissionState::Unknown);
    }
    #[test]
    fn equivalent_newer_quote_does_not_invalidate_or_renew_original_observation() {
        let (q,job)=quote_fixture(); let mut newer=q.clone();
        newer.checked_at+=10; newer.checked+=Duration::from_secs(10);
        assert!(same_quote(Some(&q),Some(&newer)));
        let mut h=ready(); h.quote_required=true;
        assess_quote(&mut h,Some(&q),Some(&job),same_quote(Some(&q),Some(&newer)),10,114,newer.checked);
        assert_eq!(h.quote_checked_at_unix,Some(100)); assert_eq!(h.quote_expires_at_unix,Some(115));
        // The original observation lapsing is informational: still Ready, no downgrade.
        let lapsed=decode(&h,115);
        assert_eq!(lapsed.state,CreditAdmissionState::Ready); assert_eq!(lapsed.category,"ok");
        newer.amount+=1; assert!(!same_quote(Some(&q),Some(&newer)));
        newer=q.clone(); newer.job_id="43".into(); assert!(!same_quote(Some(&q),Some(&newer)));
        newer=q.clone(); newer.prev_hash="b".repeat(64); assert!(!same_quote(Some(&q),Some(&newer)));
        assert!(!same_quote(Some(&q),None));
    }
    #[test]
    fn quote_migration_and_metadata_are_fail_closed_and_private() {
        let mut h=ready(); h.quote_required=true;
        // No quote yet is just "nothing priced yet" — healthy, not missing.
        assert_eq!(decode(&h,100).state,CreditAdmissionState::Ready);
        assert_eq!(decode(&h,100).category,"ok");
        let mut old=serde_json::to_value(ready()).unwrap(); old["version"]=1.into();
        assert_eq!(decode_credit_health(Some(&old.to_string()),100).state,CreditAdmissionState::Unknown);
        for field in ["quote_required","budget_low"] {
            let mut missing=serde_json::to_value(ready()).unwrap(); missing.as_object_mut().unwrap().remove(field);
            assert_eq!(decode_credit_health(Some(&missing.to_string()),100).state,CreditAdmissionState::Unknown);
        }
        let tracker=Arc::new(Mutex::new(CreditHealthTracker::default()));
        validated_quote(&tracker,"private-job",&"a".repeat(64),987654321,100,Instant::now());
        let q=quote_snapshot(&tracker).unwrap(); let job=QuoteJob::new("private-job",&"a".repeat(64)).unwrap();
        assess_quote(&mut h,Some(&q),Some(&job),true,u128::MAX,100,q.checked);
        let json=serde_json::to_string(&h).unwrap();
        for private in ["private-job","987654321",&"a".repeat(64)] {assert!(!json.contains(private));}
        for (id,hash,amount,at) in [("x".repeat(129),"a".repeat(64),1,100),
            ("private\ninput".into(),"a".repeat(64),1,100),("42".into(),"bad".into(),1,100),
            ("42".into(),"a".repeat(64),0,100),("42".into(),"a".repeat(64),1,i64::MAX)] {
            validated_quote(&tracker,&id,&hash,amount,at,Instant::now()); assert!(quote_snapshot(&tracker).is_none());
        }
    }
    #[test]
    fn low_budget_is_exact_under_twenty_percent_and_mainnet_is_unchanged() {
        let (e,l,mut s)=state_fixture(); let route=PpsFundingRoute::ZecdConventionalTestnet{hold_new_legacy_sends:true};
        let cap=s.funding.as_ref().unwrap().cap_subzatoshis;
        for (unused,low) in [(cap/5,false),(cap/5-1,true),(cap,false)] {
            s.funding.as_mut().unwrap().unused_credit_subzatoshis=unused; let mut h=ready();
            assess(&mut h,&e,&route,Some(&l),&s,100); assert_eq!(h.budget_low,low);
        }
        let mut h=ready(); assess(&mut h,&e,&PpsFundingRoute::ZalletPczt,Some(&l),&s,100);
        assess_quote(&mut h,None,None,false,0,100,Instant::now());
        assert!(!h.quote_required); assert!(!h.budget_low); assert_eq!(h.state,CreditAdmissionState::Ready);
    }
}
