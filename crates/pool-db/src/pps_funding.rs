//! Funding is a short-lived attestation from the trusted wallet collector, not
//! a wallet balance oracle. The collector must bracket its exact wallet read
//! with equal DB generations, exclude unavailable/immature funds, and prove
//! the network-specific bounded fee path. No balance, address or proposal is logged.
//! The policy is lifetime cumulative: payments, block wins and new epochs do
//! not replenish miner-credit or fee budgets.
use crate::{
    pps_live::{PpsDbError, PpsEpoch, PPS_SCALE},
    PoolDb,
};
use sqlx::{Connection, Row, SqliteConnection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PpsConventionalRecipient {
    pub miner_id: i64,
    pub address: String,
    pub amount_zatoshis: i64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PpsConventionalIntent {
    pub version: u8,
    pub network: String,
    pub epoch: String,
    pub target_height: u64,
    pub source: String,
    pub profile: String,
    pub max_recipients: u16,
    pub items: Vec<PpsConventionalRecipient>,
}
impl std::fmt::Debug for PpsConventionalIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PpsConventionalIntent { redacted }")
    }
}
impl std::fmt::Debug for PpsConventionalRecipient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PpsConventionalRecipient { redacted }")
    }
}
impl PpsConventionalIntent {
    fn fee_bound(&self) -> Result<i64, PpsDbError> {
        let text = |s: &str, limit: usize| !s.is_empty() && s.len() <= limit
            && s.bytes().all(|b| b.is_ascii_graphic());
        if self.version != 1 || self.network != "testnet"
            || !text(&self.epoch, 128) || self.target_height == 0
            || self.target_height > u32::MAX as u64 || !text(&self.source, 512)
            || self.profile != "consensus-size-v1" || self.max_recipients != 100
            || self.items.is_empty() || self.items.len() > 100
        { return Err(PpsDbError::Invalid); }
        let mut previous = 0;
        let mut total = 0_i64;
        let mut addresses = std::collections::BTreeSet::new();
        for item in &self.items {
            if item.miner_id <= previous || item.amount_zatoshis <= 0
                || !text(&item.address, 512)
            { return Err(PpsDbError::Invalid); }
            previous = item.miner_id;
            total = total.checked_add(item.amount_zatoshis).ok_or(PpsDbError::Invalid)?;
            addresses.insert(&item.address);
        }
        5_000_i64.checked_mul(addresses.len() as i64 + 2_000_000 / 352)
            .ok_or(PpsDbError::Invalid)
    }
}

/// Wallet-evidence collection against zecd takes ~40 s on this wallet, so the
/// lease must outlive several collection attempts or admission can never stay
/// armed; solvency is still re-proven from the DB inside every credit tx.
pub const FUNDING_LEASE_SECONDS: i64 = 600;

/// Maximum chain-agreement proof lifetime accepted at credit time. Defined
/// here (not in pool-core's pps_chain, which stamps it) because pool-db cannot
/// depend on pool-core; pool-core re-exports and compile-time-asserts equality,
/// so the stamp and this credit-time bound can never silently diverge.
pub const CHAIN_LEASE_SECONDS: i64 = 300;

#[derive(Clone, PartialEq, Eq)]
pub struct PpsFundingLease {
    pub network: String,
    pub checked_at_unix: i64,
    pub valid_until_unix: i64,
    pub spendable_zatoshis: i64,
    pub reserve_floor_zatoshis: i64,
    pub reserved_fee_allowance_zatoshis: i64,
    pub generation: u64,
}

#[derive(Clone, PartialEq, Eq)]
pub struct PpsFundingSnapshot {
    pub network: String,
    pub generation: u64,
    pub legacy_pending_zatoshis: i64,
    pub legacy_paying_zatoshis: i64,
    pub pps_outstanding_subzatoshis: u128,
    pub unused_credit_subzatoshis: u128,
    pub gross_subzatoshis: u128,
    pub paid_zatoshis: i64,
    pub cap_subzatoshis: u128,
    pub total_exposure_zatoshis: i64,
    pub reserve_floor_zatoshis: i64,
    pub fee_allowance_zatoshis: i64,
    pub paid_fees_zatoshis: i64,
    pub reserved_fees_zatoshis: i64,
    pub required_spendable_zatoshis: i64,
}

/// Read-side telemetry only; not a funding capability or credit authorization.
pub struct PpsCreditReadinessSnapshot {
    pub generation: u64,
    pub financial_halt: bool,
    pub funding: Option<PpsFundingSnapshot>,
}

pub const TESTNET_BUDGET_EXTENSION_EPOCH: &str = "testnet-pps-1000-20260907";

/// The only supported extension. The previous epoch is supplied by the owner
/// from the exact existing configuration; no historical epoch is rewritten.
pub fn testnet_budget_extension_epoch(previous: &PpsEpoch) -> Result<PpsEpoch, PpsDbError> {
    validate_policy(previous)?;
    if previous.network != "testnet" || previous.max_liability_zatoshis != 950_000_000
        || previous.fee_allowance_zatoshis != 50_000_000
        || previous.total_exposure_zatoshis != 1_000_000_000
        || previous.id == TESTNET_BUDGET_EXTENSION_EPOCH
        || previous.id.is_empty() || previous.id.len() > 128
        || !previous.id.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    { return Err(PpsDbError::EpochMismatch); }
    let mut next = previous.clone();
    next.id = TESTNET_BUDGET_EXTENSION_EPOCH.into();
    next.max_liability_zatoshis = 95_000_000_000;
    next.fee_allowance_zatoshis = 5_000_000_000;
    next.total_exposure_zatoshis = 100_000_000_000;
    Ok(next)
}

async fn extension_snapshot(c: &mut SqliteConnection, previous: &PpsEpoch)
    -> Result<PpsFundingSnapshot, PpsDbError>
{
    let next = testnet_budget_extension_epoch(previous)?;
    crate::pps_live::epoch_check(c, previous).await?;
    if extension_journal_present(c).await? && sqlx::query_scalar::<_,bool>(
        "SELECT EXISTS(SELECT 1 FROM pps_budget_extensions)").fetch_one(&mut *c).await?
    { return Err(PpsDbError::EpochMismatch); }
    let blocked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pps_epochs WHERE id=?1 OR network<>'testnet' OR cap_zats<>950000000) OR EXISTS(SELECT 1 FROM payout_attempts WHERE status NOT IN ('confirmed','failed')) OR EXISTS(SELECT 1 FROM payout_items) OR EXISTS(SELECT 1 FROM pps_payout_items) OR EXISTS(SELECT 1 FROM pps_fee_reservations WHERE status='reserved') OR EXISTS(SELECT 1 FROM balances WHERE paying<>0) OR EXISTS(SELECT 1 FROM pps_accounts WHERE paying<>0)")
        .bind(&next.id).fetch_one(&mut *c).await?;
    if blocked { return Err(PpsDbError::EpochMismatch); }
    let mut projected = snapshot(c, previous, false).await?;
    // Historical gross, paid principal and actual paid fees are unchanged.
    let credit_delta = next.max_liability_zatoshis - previous.max_liability_zatoshis;
    projected.cap_subzatoshis = next.max_liability_zatoshis as u128 * PPS_SCALE;
    projected.unused_credit_subzatoshis = projected.unused_credit_subzatoshis
        .checked_add(credit_delta as u128 * PPS_SCALE).ok_or(PpsDbError::Invariant)?;
    projected.fee_allowance_zatoshis = next.fee_allowance_zatoshis;
    projected.total_exposure_zatoshis = next.total_exposure_zatoshis;
    projected.required_spendable_zatoshis = plus(projected.required_spendable_zatoshis,
        next.total_exposure_zatoshis - previous.total_exposure_zatoshis)?;
    Ok(projected)
}

// Absence is supported only for the untouched predecessor. When present,
// require precisely the reviewed table and its two immutable-record triggers.
async fn extension_journal_present(c: &mut SqliteConnection) -> Result<bool, PpsDbError> {
    let rows = sqlx::query("SELECT name,sql FROM sqlite_master WHERE name IN ('pps_budget_extensions','pps_budget_extensions_no_update','pps_budget_extensions_no_delete') OR (tbl_name='pps_budget_extensions' AND type='trigger') ORDER BY name")
        .fetch_all(&mut *c).await?;
    if rows.is_empty() { return Ok(false); }
    let schema = include_str!("../migrations/019_pps_budget_extension.sql");
    let table = schema.find("CREATE TABLE").ok_or(PpsDbError::Invariant)?;
    let first_trigger = schema.find("CREATE TRIGGER").ok_or(PpsDbError::Invariant)?;
    let second_trigger = schema[first_trigger+1..].find("CREATE TRIGGER")
        .map(|offset| offset+first_trigger+1).ok_or(PpsDbError::Invariant)?;
    let expected = [("pps_budget_extensions", &schema[table..first_trigger]),
        ("pps_budget_extensions_no_delete", &schema[second_trigger..]),
        ("pps_budget_extensions_no_update", &schema[first_trigger..second_trigger])];
    let normalized = |sql: &str| sql.replace("IF NOT EXISTS ", "")
        .split_whitespace().collect::<Vec<_>>().join(" ").trim_end_matches(';').to_owned();
    if rows.len()!=expected.len() { return Err(PpsDbError::Invariant); }
    for (row,(name,sql)) in rows.iter().zip(expected) {
        if row.try_get::<String,_>("name")? != name
            || normalized(&row.try_get::<String,_>("sql")?) != normalized(sql)
        { return Err(PpsDbError::Invariant); }
    }
    Ok(true)
}

fn validate_extension_lease(snapshot: &PpsFundingSnapshot, lease: &PpsFundingLease, now: i64)
    -> Result<(), PpsDbError>
{
    if lease.valid_until_unix.checked_sub(lease.checked_at_unix).map_or(true, |v| v > FUNDING_LEASE_SECONDS) {
        return Err(PpsDbError::FundingLeaseRequired);
    }
    validate_lease(snapshot, Some(lease), now)
}

/// Hash of the immutable inspected transaction proposal, not a job/session ID.
/// The fee must have been obtained from that same fixed proposal before signing.
#[derive(Clone, PartialEq, Eq)]
pub struct PpsFeeReservation {
    pub proposal_id: String,
    pub fee_zatoshis: i64,
}
/// Caller-attested canonical send intent and enforceable fee ceiling. This is
/// NOT a PCZT effect commitment; only the explicitly testnet conventional API
/// accepts it. The wallet controller must prove the ceiling before any send.
#[derive(Clone, PartialEq, Eq)]
pub struct PpsConventionalReservation {
    pub intent_id: String,
    pub canonical_intent: String,
    pub fee_upper_bound_zatoshis: i64,
}
impl PpsConventionalReservation {
    pub fn from_intent(intent: PpsConventionalIntent) -> Result<Self, PpsDbError> {
        let fee_upper_bound_zatoshis = intent.fee_bound()?;
        let canonical_intent = serde_json::to_string(&intent).map_err(|_| PpsDbError::Invalid)?;
        if canonical_intent.len() > 262_144 { return Err(PpsDbError::Invalid); }
        let intent_id = format!("{:x}", Sha256::digest(canonical_intent.as_bytes()));
        Ok(Self { intent_id, canonical_intent, fee_upper_bound_zatoshis })
    }
    pub fn intent(&self) -> Result<PpsConventionalIntent, PpsDbError> {
        if self.canonical_intent.is_empty() || self.canonical_intent.len() > 262_144 {
            return Err(PpsDbError::Invalid);
        }
        let value: PpsConventionalIntent = serde_json::from_str(&self.canonical_intent)
            .map_err(|_| PpsDbError::Invalid)?;
        if Self::from_intent(value.clone())? != *self { return Err(PpsDbError::Invalid); }
        Ok(value)
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct PpsConventionalAttempt {
    pub attempt_id: i64,
    pub intent_id: String,
    pub canonical_intent: Option<String>,
    pub halt_category: Option<PpsConventionalHalt>,
    pub operation_id: Option<String>,
    pub expected_txid: Option<String>,
    pub fee_upper_bound_zatoshis: i64,
    pub actual_fee_zatoshis: Option<i64>,
    pub excess_fee_zatoshis: Option<i64>,
    pub sealed: bool,
    pub status: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PpsConventionalHalt {
    RecipientMismatch,
    FeeMismatch,
    TransactionVersionMismatch,
    SourceMismatch,
}
impl PpsConventionalHalt {
    fn code(self) -> &'static str {
        match self {
            Self::RecipientMismatch => "recipient_mismatch",
            Self::FeeMismatch => "fee_mismatch",
            Self::TransactionVersionMismatch => "transaction_version_mismatch",
            Self::SourceMismatch => "source_mismatch",
        }
    }
    fn parse(s: &str) -> Result<Self, PpsDbError> {
        match s {
            "recipient_mismatch" => Ok(Self::RecipientMismatch),
            "fee_mismatch" => Ok(Self::FeeMismatch),
            "transaction_version_mismatch" => Ok(Self::TransactionVersionMismatch),
            "source_mismatch" => Ok(Self::SourceMismatch),
            _ => Err(PpsDbError::Invariant),
        }
    }
}
impl PpsConventionalAttempt {
    pub fn intent(&self) -> Result<Option<PpsConventionalIntent>, PpsDbError> {
        self.canonical_intent.as_ref().map(|text| PpsConventionalReservation {
            intent_id: self.intent_id.clone(), canonical_intent: text.clone(),
            fee_upper_bound_zatoshis: self.fee_upper_bound_zatoshis,
        }.intent()).transpose()
    }
}
impl std::fmt::Debug for PpsConventionalReservation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PpsConventionalReservation { redacted }")
    }
}
impl std::fmt::Debug for PpsConventionalAttempt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PpsConventionalAttempt { redacted }")
    }
}
impl std::fmt::Debug for PpsFundingLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PpsFundingLease { redacted }")
    }
}
impl std::fmt::Debug for PpsFundingSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PpsFundingSnapshot { redacted }")
    }
}
impl std::fmt::Debug for PpsFeeReservation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PpsFeeReservation { redacted }")
    }
}

pub(crate) fn runtime_now() -> Result<i64, PpsDbError> {
    #[cfg(test)]
    {
        Ok(1_700_000_000)
    }
    #[cfg(not(test))]
    {
        i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| PpsDbError::FundingLeaseRequired)?
                .as_secs(),
        )
        .map_err(|_| PpsDbError::FundingLeaseRequired)
    }
}
fn plus(a: i64, b: i64) -> Result<i64, PpsDbError> {
    if a < 0 || b < 0 {
        return Err(PpsDbError::Invariant);
    }
    a.checked_add(b).ok_or(PpsDbError::Invariant)
}
fn subunits(whole: i64, fraction: i64) -> Result<u128, PpsDbError> {
    if whole < 0 || fraction < 0 || fraction as u128 >= PPS_SCALE {
        return Err(PpsDbError::Invariant);
    }
    Ok(whole as u128 * PPS_SCALE + fraction as u128)
}
fn canonical_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn operation_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
}
fn conventional_from_row(
    r: &sqlx::sqlite::SqliteRow,
) -> Result<PpsConventionalAttempt, PpsDbError> {
    let a = PpsConventionalAttempt {
        attempt_id: r.try_get("attempt_id")?,
        intent_id: r.try_get("intent_id")?,
        canonical_intent: r.try_get("canonical_json")?,
        halt_category: r.try_get::<Option<String>, _>("halt_category")?
            .map(|v| PpsConventionalHalt::parse(&v)).transpose()?,
        operation_id: r.try_get("operation_id")?,
        expected_txid: r.try_get("observed_txid")?,
        fee_upper_bound_zatoshis: r.try_get("fee_bound")?,
        actual_fee_zatoshis: r.try_get("actual_fee")?,
        excess_fee_zatoshis: r.try_get("excess_fee")?,
        sealed: r.try_get::<i64, _>("sealed")? == 1,
        status: r.try_get("status")?,
    };
    let sealed: i64 = r.try_get("sealed")?;
    if a.attempt_id <= 0
        || !canonical_hash(&a.intent_id)
        || !matches!(sealed, 0 | 1)
        || r.try_get::<String, _>("proposal_id")? != a.intent_id
        || r.try_get::<i64, _>("fee_zats")? != a.fee_upper_bound_zatoshis
        || a.fee_upper_bound_zatoshis <= 0
        || a.halt_category.is_some_and(|_| !a.sealed || a.expected_txid.is_none() || a.status != "reserved")
        || a.operation_id
            .as_deref()
            .is_some_and(|v| !operation_id(v) || !a.sealed)
        || a.expected_txid
            .as_deref()
            .is_some_and(|v| !canonical_hash(v) || a.operation_id.is_none() || !a.sealed)
        || r.try_get::<Option<String>, _>("expected_txid")? != a.expected_txid
        || a.actual_fee_zatoshis
            .is_some_and(|v| v < 0 || v > a.fee_upper_bound_zatoshis || a.expected_txid.is_none())
        || a.excess_fee_zatoshis.is_some_and(|v| {
            v <= a.fee_upper_bound_zatoshis || !a.sealed || a.expected_txid.is_none()
        })
        || (a.actual_fee_zatoshis.is_some() && a.excess_fee_zatoshis.is_some())
        || match a.status.as_str() {
            "reserved" => a.actual_fee_zatoshis.is_some(),
            "paid" => a.actual_fee_zatoshis.is_none() || a.excess_fee_zatoshis.is_some(),
            "released" => {
                a.sealed || a.actual_fee_zatoshis.is_some() || a.excess_fee_zatoshis.is_some()
            }
            _ => true,
        }
    {
        return Err(PpsDbError::Invariant);
    }
    a.intent()?;
    Ok(a)
}
async fn conventional_attempt(
    c: &mut SqliteConnection,
    attempt: i64,
) -> Result<Option<PpsConventionalAttempt>, PpsDbError> {
    let r=sqlx::query("SELECT f.*,p.intent_id,p.fee_bound,p.actual_fee,p.operation_id,p.observed_txid,p.excess_fee,i.canonical_json,h.category AS halt_category FROM pps_conventional_attempts p LEFT JOIN pps_fee_reservations f ON f.attempt_id=p.attempt_id LEFT JOIN pps_conventional_intents i ON i.attempt_id=p.attempt_id LEFT JOIN pps_conventional_halts h ON h.attempt_id=p.attempt_id WHERE p.attempt_id=?1")
        .bind(attempt).fetch_optional(&mut *c).await?;
    r.as_ref().map(conventional_from_row).transpose()
}
/// Returns true only when an authenticated observed over-bound fee was durably
/// marked within the caller's transaction. It must then commit the halt alone.
pub(crate) async fn validate_confirmation(
    c: &mut SqliteConnection,
    attempt: i64,
    txid: &str,
    actual: Option<i64>,
) -> Result<bool, PpsDbError> {
    let record = conventional_attempt(c, attempt).await?;
    match (record, actual) {
        (None, None) => Ok(false),
        (Some(a), Some(actual)) => {
            if active_epoch(c).await?.network != "testnet"
                || a.intent()?.is_none()
                || a.halt_category.is_some()
                || !a.sealed
                || a.expected_txid.as_deref() != Some(txid)
                || a.operation_id.is_none()
                || actual < 0
                || a.status == "released"
            {
                return Err(PpsDbError::Invalid);
            }
            if let Some(old) = a.actual_fee_zatoshis {
                if old != actual {
                    return Err(PpsDbError::DuplicateMismatch);
                }
                return Ok(false);
            }
            if let Some(old) = a.excess_fee_zatoshis {
                if old != actual {
                    return Err(PpsDbError::DuplicateMismatch);
                }
                return Err(PpsDbError::FeeBudgetExceeded);
            }
            if actual > a.fee_upper_bound_zatoshis {
                sqlx::query("UPDATE pps_conventional_attempts SET excess_fee=?1 WHERE attempt_id=?2 AND actual_fee IS NULL AND excess_fee IS NULL")
                    .bind(actual).bind(attempt).execute(&mut *c).await?;
                return Ok(true);
            }
            Ok(false)
        }
        _ => Err(PpsDbError::Invalid),
    }
}
pub(crate) fn validate_policy(e: &PpsEpoch) -> Result<(), PpsDbError> {
    const MAX: i64 = 21_000_000 * 100_000_000;
    if !matches!(e.network.as_str(), "mainnet" | "testnet")
        || e.max_liability_zatoshis <= 0
        || e.fee_allowance_zatoshis <= 0
        || e.reserve_floor_zatoshis <= 0
        || e.reserve_floor_zatoshis > MAX
        || e.total_exposure_zatoshis <= 0
        || e.total_exposure_zatoshis > MAX
        || e.max_liability_zatoshis
            .checked_add(e.fee_allowance_zatoshis)
            .map_or(true, |v| v > e.total_exposure_zatoshis)
    {
        return Err(PpsDbError::Invalid);
    }
    Ok(())
}
pub(crate) async fn policy_check(
    c: &mut SqliteConnection,
    e: &PpsEpoch,
    allow_absent: bool,
) -> Result<(), PpsDbError> {
    validate_policy(e)?;
    let row = sqlx::query("SELECT * FROM pps_funding_policy WHERE singleton=1")
        .fetch_optional(&mut *c)
        .await?;
    match row {
        Some(r)
            if r.try_get::<String, _>("network")? == e.network
                && r.try_get::<i64, _>("credit_cap")? == e.max_liability_zatoshis
                && r.try_get::<i64, _>("total_cap")? == e.total_exposure_zatoshis
                && r.try_get::<i64, _>("fee_cap")? == e.fee_allowance_zatoshis
                && r.try_get::<i64, _>("reserve_floor")? == e.reserve_floor_zatoshis =>
        {
            Ok(())
        }
        None if allow_absent => Ok(()),
        _ => Err(PpsDbError::EpochMismatch),
    }
}
pub(crate) async fn persist_policy(
    c: &mut SqliteConnection,
    e: &PpsEpoch,
) -> Result<(), PpsDbError> {
    policy_check(c, e, true).await?;
    sqlx::query("INSERT OR IGNORE INTO pps_funding_policy VALUES(1,?1,?2,?3,?4,?5)")
        .bind(&e.network)
        .bind(e.max_liability_zatoshis)
        .bind(e.total_exposure_zatoshis)
        .bind(e.fee_allowance_zatoshis)
        .bind(e.reserve_floor_zatoshis)
        .execute(&mut *c)
        .await?;
    Ok(())
}
pub(crate) async fn active_epoch(c: &mut SqliteConnection) -> Result<PpsEpoch, PpsDbError> {
    let r=sqlx::query("SELECT p.*,m.active_epoch,e.fee_bps,e.quote_provenance FROM pps_funding_policy p JOIN pps_meta m ON m.singleton=p.singleton JOIN pps_epochs e ON e.id=m.active_epoch WHERE p.singleton=1")
        .fetch_optional(&mut *c).await?.ok_or(PpsDbError::FundingLeaseRequired)?;
    Ok(PpsEpoch {
        id: r.try_get("active_epoch")?,
        network: r.try_get("network")?,
        fee_bps: u16::try_from(r.try_get::<i64, _>("fee_bps")?)
            .map_err(|_| PpsDbError::Invariant)?,
        max_liability_zatoshis: r.try_get("credit_cap")?,
        total_exposure_zatoshis: r.try_get("total_cap")?,
        fee_allowance_zatoshis: r.try_get("fee_cap")?,
        reserve_floor_zatoshis: r.try_get("reserve_floor")?,
        quote_provenance: r.try_get("quote_provenance")?,
    })
}
/// Invalidates attestations on wallet outflow/reservation lifecycle mutations.
/// Called inside their existing transaction; arithmetic overflow rolls it back.
pub(crate) async fn bump_generation(c: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    let n: i64 =
        sqlx::query_scalar("SELECT generation FROM pps_funding_generation WHERE singleton=1")
            .fetch_one(&mut *c)
            .await?;
    let next = n
        .checked_add(1)
        .filter(|_| n >= 0)
        .ok_or_else(|| sqlx::Error::Protocol("funding generation exhausted".into()))?;
    sqlx::query("UPDATE pps_funding_generation SET generation=?1 WHERE singleton=1")
        .bind(next)
        .execute(&mut *c)
        .await?;
    Ok(())
}
pub(crate) async fn snapshot(
    c: &mut SqliteConnection,
    e: &PpsEpoch,
    allow_absent: bool,
) -> Result<PpsFundingSnapshot, PpsDbError> {
    policy_check(c, e, allow_absent).await?;
    // A financial halt no longer bricks this snapshot. Halting the SENDING of
    // money must not also stop crediting, admission, settlement of other
    // attempts, funding refresh, the accounting invariant, or daemon startup
    // (all of which route through here) -- that used to convert one payout
    // anomaly into an unrecoverable, monitoring-killing full outage. The halt
    // is now enforced only where a NEW send is authorized (the seal's
    // send fence below), so a halt still reliably stops sending.
    let gen: i64 =
        sqlx::query_scalar("SELECT generation FROM pps_funding_generation WHERE singleton=1")
            .fetch_one(&mut *c)
            .await?;
    let mut s = PpsFundingSnapshot {
        network: e.network.clone(),
        generation: u64::try_from(gen).map_err(|_| PpsDbError::Invariant)?,
        legacy_pending_zatoshis: 0,
        legacy_paying_zatoshis: 0,
        pps_outstanding_subzatoshis: 0,
        unused_credit_subzatoshis: 0,
        gross_subzatoshis: 0,
        paid_zatoshis: 0,
        cap_subzatoshis: e.max_liability_zatoshis as u128 * PPS_SCALE,
        total_exposure_zatoshis: e.total_exposure_zatoshis,
        reserve_floor_zatoshis: e.reserve_floor_zatoshis,
        fee_allowance_zatoshis: e.fee_allowance_zatoshis,
        paid_fees_zatoshis: 0,
        reserved_fees_zatoshis: 0,
        required_spendable_zatoshis: 0,
    };
    // Checked Rust folds, never SQLite SUM (which can overflow or become REAL).
    let mut legacy_reserved = std::collections::BTreeMap::<i64, i64>::new();
    let mut cursor: Option<i64> = None;
    loop {
        let rows=sqlx::query("SELECT miner_id,pending,paying,(typeof(pending)='integer' AND typeof(paying)='integer') AS exact_types FROM balances WHERE (?1 IS NULL OR miner_id>?1) ORDER BY miner_id LIMIT 256")
            .bind(cursor).fetch_all(&mut *c).await?;
        if rows.is_empty() {
            break;
        }
        for r in rows {
            let m: i64 = r.try_get("miner_id")?;
            if m <= 0 || r.try_get::<i64, _>("exact_types")? != 1 {
                return Err(PpsDbError::Invariant);
            }
            cursor = Some(m);
            s.legacy_pending_zatoshis = plus(s.legacy_pending_zatoshis, r.try_get("pending")?)?;
            let paying: i64 = r.try_get("paying")?;
            s.legacy_paying_zatoshis = plus(s.legacy_paying_zatoshis, paying)?;
            if paying > 0 {
                legacy_reserved.insert(m, paying);
            }
        }
    }
    cursor = None;
    loop {
        let rows=sqlx::query("SELECT id,miner_id,amount,typeof(amount)='integer' AS exact_types FROM payout_items WHERE (?1 IS NULL OR id>?1) ORDER BY id LIMIT 256")
            .bind(cursor).fetch_all(&mut *c).await?;
        if rows.is_empty() {
            break;
        }
        for r in rows {
            let rowid: i64 = r.try_get("id")?;
            let miner: i64 = r.try_get("miner_id")?;
            let amount: i64 = r.try_get("amount")?;
            if rowid <= 0 || miner <= 0 || amount <= 0 || r.try_get::<i64, _>("exact_types")? != 1 {
                return Err(PpsDbError::Invariant);
            }
            cursor = Some(rowid);
            let remaining = legacy_reserved
                .get_mut(&miner)
                .ok_or(PpsDbError::Invariant)?;
            *remaining = remaining
                .checked_sub(amount)
                .filter(|v| *v >= 0)
                .ok_or(PpsDbError::Invariant)?;
        }
    }
    if legacy_reserved.values().any(|v| *v != 0) {
        return Err(PpsDbError::Invariant);
    }
    cursor = None;
    loop {
        let rows=sqlx::query("SELECT miner_id,pending,paying,paid,fraction FROM pps_accounts WHERE (?1 IS NULL OR miner_id>?1) ORDER BY miner_id LIMIT 256")
            .bind(cursor).fetch_all(&mut *c).await?;
        if rows.is_empty() {
            break;
        }
        for r in rows {
            let m: i64 = r.try_get("miner_id")?;
            if m <= 0 {
                return Err(PpsDbError::Invariant);
            }
            cursor = Some(m);
            let value = subunits(
                plus(r.try_get("pending")?, r.try_get("paying")?)?,
                r.try_get("fraction")?,
            )?;
            s.pps_outstanding_subzatoshis = s
                .pps_outstanding_subzatoshis
                .checked_add(value)
                .ok_or(PpsDbError::Invariant)?;
            s.paid_zatoshis = plus(s.paid_zatoshis, r.try_get("paid")?)?;
        }
    }
    let meta = sqlx::query("SELECT * FROM pps_meta WHERE singleton=1")
        .fetch_optional(&mut *c)
        .await?;
    if let Some(r) = meta {
        if r.try_get::<String, _>("network")? != e.network
            || r.try_get::<i64, _>("cap_zats")? != e.max_liability_zatoshis
        {
            return Err(PpsDbError::EpochMismatch);
        }
        s.gross_subzatoshis = subunits(r.try_get("gross_whole")?, r.try_get("gross_fraction")?)?;
    } else if !allow_absent {
        return Err(PpsDbError::FundingLeaseRequired);
    }
    // Conservation is the audit rule: every credited sub-zatoshi is exactly one of
    // outstanding (pending + paying) or paid. `gross` is the lifetime total and only
    // ever grows.
    //
    // The cap bounds OUTSTANDING liability, not lifetime credits (refill model): a
    // PPS pool's promises are backed by the block rewards it earns, so capacity must
    // return as payouts settle and income lands. `max_liability` is therefore the
    // pool's variance capital — how far in the hole it tolerates being before it
    // pauses — and lifetime `gross` is expected to exceed it many times over.
    if s.pps_outstanding_subzatoshis
        .checked_add(subunits(s.paid_zatoshis, 0)?)
        != Some(s.gross_subzatoshis)
        || s.pps_outstanding_subzatoshis > s.cap_subzatoshis
    {
        return Err(PpsDbError::Invariant);
    }
    s.unused_credit_subzatoshis = s.cap_subzatoshis - s.pps_outstanding_subzatoshis;
    cursor = None;
    loop {
        let rows=sqlx::query("SELECT f.*,p.attempt_id AS conventional_id,p.intent_id,p.fee_bound,p.actual_fee,p.operation_id,p.observed_txid,p.excess_fee,i.canonical_json,h.category AS halt_category FROM pps_fee_reservations f LEFT JOIN pps_conventional_attempts p ON p.attempt_id=f.attempt_id LEFT JOIN pps_conventional_intents i ON i.attempt_id=f.attempt_id LEFT JOIN pps_conventional_halts h ON h.attempt_id=f.attempt_id WHERE (?1 IS NULL OR f.attempt_id>?1) ORDER BY f.attempt_id LIMIT 256")
            .bind(cursor).fetch_all(&mut *c).await?;
        if rows.is_empty() {
            break;
        }
        for r in rows {
            let attempt: i64 = r.try_get("attempt_id")?;
            let fee: i64 = r.try_get("fee_zats")?;
            if attempt <= 0 || fee <= 0 || !canonical_hash(&r.try_get::<String, _>("proposal_id")?)
            {
                return Err(PpsDbError::Invariant);
            }
            cursor = Some(attempt);
            let txid: Option<String> = r.try_get("txid")?;
            let sealed: i64 = r.try_get("sealed")?;
            let expected: Option<String> = r.try_get("expected_txid")?;
            let conventional = if r.try_get::<Option<i64>, _>("conventional_id")?.is_some() {
                if e.network != "testnet" {
                    return Err(PpsDbError::Invariant);
                }
                Some(conventional_from_row(&r)?)
            } else {
                None
            };
            if conventional.as_ref().is_some_and(|a| a.status == "reserved" && a.canonical_intent.is_none()) {
                return Err(PpsDbError::FundingLeaseRequired);
            }
            // An over-ceiling actual fee is a durable anomaly that fences new
            // sends (see the seal), but it must not brick this accounting
            // snapshot: the reservation is simply counted at its reserved
            // fee_bound by the "reserved" arm below, exactly as before the send.
            if !matches!(sealed, 0 | 1)
                || expected
                    .as_deref()
                    .is_some_and(|v| sealed != 1 || !canonical_hash(v))
            {
                return Err(PpsDbError::Invariant);
            }
            match r.try_get::<String, _>("status")?.as_str() {
                "reserved" if txid.is_none() => {
                    s.reserved_fees_zatoshis = plus(s.reserved_fees_zatoshis, fee)?
                }
                "paid"
                    if sealed == 1
                        && txid == expected
                        && txid.as_deref().is_some_and(canonical_hash) =>
                {
                    s.paid_fees_zatoshis = plus(
                        s.paid_fees_zatoshis,
                        conventional
                            .as_ref()
                            .and_then(|a| a.actual_fee_zatoshis)
                            .unwrap_or(fee),
                    )?
                }
                "released" if txid.is_none() && sealed == 0 => (),
                _ => return Err(PpsDbError::Invariant),
            }
        }
    }
    let committed_fees = plus(s.paid_fees_zatoshis, s.reserved_fees_zatoshis)?;
    let orphan_conventional:i64=sqlx::query_scalar("SELECT COUNT(*) FROM pps_conventional_attempts p LEFT JOIN pps_fee_reservations f ON f.attempt_id=p.attempt_id WHERE f.attempt_id IS NULL")
        .fetch_one(&mut *c).await?;
    if orphan_conventional != 0 {
        return Err(PpsDbError::Invariant);
    }
    let bad_attribution:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM pps_payout_items p LEFT JOIN pps_fee_reservations f ON f.attempt_id=p.attempt_id WHERE f.attempt_id IS NULL OR f.status<>'reserved')+(SELECT COUNT(*) FROM pps_payouts p LEFT JOIN pps_fee_reservations f ON f.attempt_id=p.attempt_id WHERE f.attempt_id IS NULL OR f.status<>'paid' OR f.txid<>p.txid)+(SELECT COUNT(*) FROM pps_fee_reservations f WHERE (f.status='reserved' AND NOT EXISTS(SELECT 1 FROM pps_payout_items p WHERE p.attempt_id=f.attempt_id)) OR (f.status='paid' AND NOT EXISTS(SELECT 1 FROM pps_payouts p WHERE p.attempt_id=f.attempt_id AND p.txid=f.txid)) OR (f.status='released' AND (EXISTS(SELECT 1 FROM pps_payout_items p WHERE p.attempt_id=f.attempt_id) OR EXISTS(SELECT 1 FROM pps_payouts p WHERE p.attempt_id=f.attempt_id))))")
        .fetch_one(&mut *c).await?;
    if bad_attribution != 0 {
        return Err(PpsDbError::Invariant);
    }
    // Exposure is outstanding liability plus committed fees (refill model) — not
    // lifetime gross, which is expected to grow past the cap indefinitely.
    if committed_fees > e.fee_allowance_zatoshis
        || s.pps_outstanding_subzatoshis
            .checked_add(subunits(committed_fees, 0)?)
            .map_or(true, |v| v > e.total_exposure_zatoshis as u128 * PPS_SCALE)
    {
        return Err(PpsDbError::FeeBudgetExceeded);
    }
    // Outstanding + unused capacity is exactly the cap: the wallet must always hold
    // its full variance capital, so any outstanding amount (≤ cap) is payable even
    // with zero further income. Reserved fees stay included once; paid fees need no
    // remaining backing.
    let remaining = s
        .pps_outstanding_subzatoshis
        .checked_add(s.unused_credit_subzatoshis)
        .ok_or(PpsDbError::Invariant)?;
    let remaining = i64::try_from(
        remaining
            .checked_add(PPS_SCALE - 1)
            .ok_or(PpsDbError::Invariant)?
            / PPS_SCALE,
    )
    .map_err(|_| PpsDbError::Invariant)?;
    s.required_spendable_zatoshis = plus(
        plus(
            plus(
                plus(s.legacy_pending_zatoshis, s.legacy_paying_zatoshis)?,
                e.reserve_floor_zatoshis,
            )?,
            remaining,
        )?,
        e.fee_allowance_zatoshis - s.paid_fees_zatoshis,
    )?;
    Ok(s)
}
pub(crate) fn validate_lease(
    s: &PpsFundingSnapshot,
    l: Option<&PpsFundingLease>,
    now: i64,
) -> Result<(), PpsDbError> {
    let l = l.ok_or(PpsDbError::FundingLeaseRequired)?;
    if l.network != s.network
        || l.generation != s.generation
        || l.checked_at_unix < 0
        || l.valid_until_unix <= l.checked_at_unix
        || l.valid_until_unix
            .checked_sub(l.checked_at_unix)
            .map_or(true, |v| v > FUNDING_LEASE_SECONDS)
        || now < l.checked_at_unix
        || now >= l.valid_until_unix
        || l.spendable_zatoshis < 0
        || l.spendable_zatoshis > 21_000_000 * 100_000_000
        || l.reserve_floor_zatoshis != s.reserve_floor_zatoshis
        || l.reserved_fee_allowance_zatoshis != s.fee_allowance_zatoshis
    {
        return Err(PpsDbError::FundingLeaseRequired);
    }
    if l.spendable_zatoshis < s.required_spendable_zatoshis {
        return Err(PpsDbError::FundingInsufficient);
    }
    Ok(())
}
pub(crate) async fn check(
    c: &mut SqliteConnection,
    e: &PpsEpoch,
    l: Option<&PpsFundingLease>,
    now: i64,
    allow_absent: bool,
) -> Result<(), PpsDbError> {
    validate_lease(&snapshot(c, e, allow_absent).await?, l, now)
}
pub(crate) async fn check_credit(
    c: &mut SqliteConnection,
    e: &PpsEpoch,
    l: Option<&PpsFundingLease>,
    now: i64,
) -> Result<(), PpsDbError> {
    let s = snapshot(c, e, false).await?;
    validate_lease(&s, l, now)?;
    if plus(s.paid_fees_zatoshis, s.reserved_fees_zatoshis)? >= s.fee_allowance_zatoshis {
        return Err(PpsDbError::FeeBudgetExceeded);
    }
    Ok(())
}
pub(crate) async fn reserve_fee(
    c: &mut SqliteConnection,
    attempt: i64,
    fee: &PpsFeeReservation,
    conventional: Option<&PpsConventionalReservation>,
) -> Result<(), PpsDbError> {
    if attempt <= 0 || fee.fee_zatoshis <= 0 || !canonical_hash(&fee.proposal_id) {
        return Err(PpsDbError::Invalid);
    }
    sqlx::query("INSERT INTO pps_fee_reservations(attempt_id,proposal_id,fee_zats,status) VALUES(?1,?2,?3,'reserved')")
        .bind(attempt).bind(&fee.proposal_id).bind(fee.fee_zatoshis).execute(&mut *c).await?;
    let e = active_epoch(c).await?;
    if let Some(intent) = conventional {
        let parsed = intent.intent()?;
        if e.network != "testnet"
            || parsed.epoch != e.id
            || intent.intent_id != fee.proposal_id
            || intent.fee_upper_bound_zatoshis != fee.fee_zatoshis
        {
            return Err(PpsDbError::Invalid);
        }
        sqlx::query("INSERT INTO pps_conventional_attempts(attempt_id,intent_id,fee_bound) VALUES(?1,?2,?3)")
            .bind(attempt).bind(&intent.intent_id).bind(intent.fee_upper_bound_zatoshis).execute(&mut *c).await?;
        sqlx::query("INSERT INTO pps_conventional_intents(attempt_id,canonical_json) VALUES(?1,?2)")
            .bind(attempt).bind(&intent.canonical_intent).execute(&mut *c).await?;
    }
    let s = snapshot(c, &e, false).await?;
    if plus(s.paid_fees_zatoshis, s.reserved_fees_zatoshis)? == s.fee_allowance_zatoshis {
        // Spending the final fee capacity cannot strand any unreserved whole
        // or fractional miner claim. Further credits then halt automatically.
        let stranded: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM pps_accounts WHERE pending<>0 OR fraction<>0")
                .fetch_one(&mut *c)
                .await?;
        if stranded != 0 {
            return Err(PpsDbError::FeeBudgetExceeded);
        }
    }
    Ok(())
}
pub(crate) async fn settle_fee(
    c: &mut SqliteConnection,
    attempt: i64,
    txid: Option<&str>,
    actual_fee: Option<i64>,
) -> Result<(), PpsDbError> {
    let r = sqlx::query(
        "SELECT status,txid,sealed,expected_txid FROM pps_fee_reservations WHERE attempt_id=?1",
    )
    .bind(attempt)
    .fetch_optional(&mut *c)
    .await?;
    let Some(r) = r else {
        return Err(PpsDbError::Invariant);
    };
    let status: String = r.try_get("status")?;
    let old: Option<String> = r.try_get("txid")?;
    let sealed: i64 = r.try_get("sealed")?;
    let expected: Option<String> = r.try_get("expected_txid")?;
    if (txid.is_none() && sealed != 0)
        || (txid.is_some() && (sealed != 1 || txid != expected.as_deref()))
    {
        return Err(PpsDbError::Invariant);
    }
    if status != "reserved" {
        if (status == "paid" && old.as_deref() == txid && txid.is_some())
            || (status == "released" && txid.is_none())
        {
            return Ok(());
        }
        return Err(PpsDbError::Invariant);
    }
    if txid.is_some_and(|s| !canonical_hash(s)) {
        return Err(PpsDbError::Invalid);
    }
    if let Some(actual) = actual_fee {
        let changed=sqlx::query("UPDATE pps_conventional_attempts SET actual_fee=?1 WHERE attempt_id=?2 AND actual_fee IS NULL AND excess_fee IS NULL AND fee_bound>=?1 AND observed_txid=?3")
            .bind(actual).bind(attempt).bind(txid).execute(&mut *c).await?;
        if changed.rows_affected() != 1 {
            return Err(PpsDbError::Invariant);
        }
    }
    sqlx::query("UPDATE pps_fee_reservations SET status=?1,txid=?2 WHERE attempt_id=?3")
        .bind(if txid.is_some() { "paid" } else { "released" })
        .bind(txid)
        .bind(attempt)
        .execute(&mut *c)
        .await?;
    Ok(())
}
impl PoolDb {
    /// The caller must first prove a canonical node-confirmed transaction
    /// violates the stored contract. Unknown reads/timeouts are not violations.
    /// This is a durable hold, never a refund, retry, or budget replenishment.
    pub async fn halt_pps_conventional_payout(
        &self,
        attempt: i64,
        category: PpsConventionalHalt,
    ) -> Result<(), PpsDbError> {
        let mut c = self.inner().acquire().await?;
        sqlx::query("PRAGMA synchronous=FULL").execute(&mut *c).await?;
        let mut tx = c.begin_with("BEGIN IMMEDIATE").await?;
        let a = conventional_attempt(&mut tx, attempt).await?.ok_or(PpsDbError::Invalid)?;
        if active_epoch(&mut tx).await?.network != "testnet" || a.intent()?.is_none()
            || !a.sealed || a.expected_txid.is_none() || a.operation_id.is_none()
            || a.status != "reserved"
        { return Err(PpsDbError::Invalid); }
        if let Some(old) = a.halt_category {
            if old != category { return Err(PpsDbError::DuplicateMismatch); }
            return Ok(());
        }
        sqlx::query("INSERT INTO pps_conventional_halts(attempt_id,category) VALUES(?1,?2)")
            .bind(attempt).bind(category.code()).execute(&mut *tx).await?;
        sqlx::query("UPDATE payout_attempts SET status='held',updated_at=datetime('now') WHERE id=?1")
            .bind(attempt).execute(&mut *tx).await?;
        bump_generation(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }
    /// One-way fence BEFORE invoking any signer. After this succeeds, even an
    /// ambiguous signer/extraction failure keeps principal and fees reserved.
    pub async fn seal_pps_payout(&self, attempt: i64, proposal_id: &str) -> Result<(), PpsDbError> {
        self.mark_pps_proposal(attempt, proposal_id, None, false)
            .await
    }
    pub async fn seal_pps_conventional_payout(
        &self,
        attempt: i64,
        intent_id: &str,
    ) -> Result<(), PpsDbError> {
        // One-shot send authorization: a second seal rejects even if the
        // caller never received the first successful return. Recovery reads
        // the immutable receipt; it must never invoke send again.
        self.mark_pps_proposal(attempt, intent_id, None, true).await
    }
    /// Production send fence: freshness, generation, current obligations and
    /// the one-shot marker are validated under the same exclusive transaction.
    pub async fn seal_pps_conventional_payout_funded(
        &self,
        attempt: i64,
        intent_id: &str,
        funding: &PpsFundingLease,
    ) -> Result<(), PpsDbError> {
        self.mark_pps_proposal_with_operation(attempt, intent_id, None, true, None, Some(funding)).await
    }
    /// Bind the extracted fixed transaction before broadcast. An interrupted
    /// sealed proposal without this metadata stays parked; never replan it.
    pub async fn mark_pps_payout_signed(
        &self,
        attempt: i64,
        proposal_id: &str,
        expected_txid: &str,
    ) -> Result<(), PpsDbError> {
        if !canonical_hash(expected_txid) {
            return Err(PpsDbError::Invalid);
        }
        self.mark_pps_proposal(attempt, proposal_id, Some(expected_txid), false)
            .await
    }
    pub async fn record_pps_conventional_operation(
        &self,
        attempt: i64,
        intent_id: &str,
        opid: &str,
    ) -> Result<(), PpsDbError> {
        if !canonical_hash(intent_id) || !operation_id(opid) {
            return Err(PpsDbError::Invalid);
        }
        let mut c = self.inner().acquire().await?;
        sqlx::query("PRAGMA synchronous=FULL")
            .execute(&mut *c)
            .await?;
        let mut tx = c.begin_with("BEGIN IMMEDIATE").await?;
        let a = conventional_attempt(&mut tx, attempt)
            .await?
            .ok_or(PpsDbError::Invalid)?;
        if active_epoch(&mut tx).await?.network != "testnet"
            || a.intent()?.is_none()
            || a.halt_category.is_some()
            || !a.sealed
            || a.intent_id != intent_id
            || a.status != "reserved"
            || a.operation_id.as_deref().is_some_and(|v| v != opid)
            || a.excess_fee_zatoshis.is_some()
        {
            return Err(PpsDbError::Invalid);
        }
        let old: Option<String> =
            sqlx::query_scalar("SELECT opid FROM payout_attempts WHERE id=?1")
                .bind(attempt)
                .fetch_one(&mut *tx)
                .await?;
        if old.as_deref().is_some_and(|v| v != opid) {
            return Err(PpsDbError::DuplicateMismatch);
        }
        sqlx::query("UPDATE pps_conventional_attempts SET operation_id=?1 WHERE attempt_id=?2")
            .bind(opid)
            .bind(attempt)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE payout_attempts SET opid=?1,status='sent' WHERE id=?2")
            .bind(opid)
            .bind(attempt)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    /// The result transaction is learned AFTER the one asynchronous wallet
    /// invocation. No signer/broadcast action is performed by this method.
    pub async fn record_pps_conventional_transaction(
        &self,
        attempt: i64,
        intent_id: &str,
        opid: &str,
        txid: &str,
    ) -> Result<(), PpsDbError> {
        if !canonical_hash(txid) || !operation_id(opid) {
            return Err(PpsDbError::Invalid);
        }
        // The immutable operation check is repeated in the same transaction as
        // the txid binding by passing the expected operation into the helper.
        self.mark_pps_conventional_transaction(attempt, intent_id, opid, txid)
            .await
    }
    async fn mark_pps_conventional_transaction(
        &self,
        attempt: i64,
        intent_id: &str,
        opid: &str,
        txid: &str,
    ) -> Result<(), PpsDbError> {
        self.mark_pps_proposal_with_operation(attempt, intent_id, Some(txid), true, Some(opid), None)
            .await
    }
    pub async fn get_pps_conventional_attempt(
        &self,
        attempt: i64,
    ) -> Result<Option<PpsConventionalAttempt>, PpsDbError> {
        if attempt <= 0 {
            return Err(PpsDbError::Invalid);
        }
        let mut tx = self.inner().begin_with("BEGIN IMMEDIATE").await?;
        let a = conventional_attempt(&mut tx, attempt).await?;
        if a.is_some() && active_epoch(&mut tx).await?.network != "testnet" {
            return Err(PpsDbError::Invalid);
        }
        tx.commit().await?;
        Ok(a)
    }
    /// Bounded typed recovery inventory. These journals never enter the PCZT
    /// or legacy recovery lists, including after principal rows are removed.
    pub async fn get_reserved_pps_conventional_attempts(
        &self,
        limit: u16,
    ) -> Result<Vec<PpsConventionalAttempt>, PpsDbError> {
        if limit == 0 || limit > 100 { return Err(PpsDbError::Invalid); }
        let mut tx = self.inner().begin_with("BEGIN IMMEDIATE").await?;
        if active_epoch(&mut tx).await?.network != "testnet" { return Err(PpsDbError::Invalid); }
        let ids: Vec<i64> = sqlx::query_scalar("SELECT p.attempt_id FROM pps_conventional_attempts p JOIN pps_fee_reservations f ON f.attempt_id=p.attempt_id WHERE f.status='reserved' ORDER BY p.attempt_id LIMIT ?1")
            .bind(i64::from(limit)).fetch_all(&mut *tx).await?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids { out.push(conventional_attempt(&mut tx, id).await?.ok_or(PpsDbError::Invariant)?); }
        tx.commit().await?;
        Ok(out)
    }
    async fn mark_pps_proposal(
        &self,
        attempt: i64,
        proposal_id: &str,
        expected: Option<&str>,
        conventional: bool,
    ) -> Result<(), PpsDbError> {
        self.mark_pps_proposal_with_operation(attempt, proposal_id, expected, conventional, None, None)
            .await
    }
    async fn mark_pps_proposal_with_operation(
        &self,
        attempt: i64,
        proposal_id: &str,
        expected: Option<&str>,
        conventional: bool,
        opid: Option<&str>,
        funding: Option<&PpsFundingLease>,
    ) -> Result<(), PpsDbError> {
        if attempt <= 0 || !canonical_hash(proposal_id) {
            return Err(PpsDbError::Invalid);
        }
        let mut c = self.inner().acquire().await?;
        sqlx::query("PRAGMA synchronous=FULL")
            .execute(&mut *c)
            .await?;
        let mut tx = c.begin_with("BEGIN IMMEDIATE").await?;
        let record = conventional_attempt(&mut tx, attempt).await?;
        if conventional != record.is_some() {
            return Err(PpsDbError::Invalid);
        }
        if let Some(a) = &record {
            if active_epoch(&mut tx).await?.network != "testnet"
                || a.intent()?.is_none()
                || a.halt_category.is_some()
                || a.intent_id != proposal_id
                || (expected.is_none() && a.sealed)
                || (expected.is_some() && (opid.is_none() || a.operation_id.as_deref() != opid))
                || a.excess_fee_zatoshis.is_some()
            {
                return Err(PpsDbError::Invalid);
            }
        }
        let r=sqlx::query("SELECT proposal_id,status,sealed,expected_txid FROM pps_fee_reservations WHERE attempt_id=?1").bind(attempt).fetch_optional(&mut *tx).await?.ok_or(PpsDbError::Invariant)?;
        let old: Option<String> = r.try_get("expected_txid")?;
        if r.try_get::<String, _>("proposal_id")? != proposal_id
            || r.try_get::<String, _>("status")? != "reserved"
            || (expected.is_some() && r.try_get::<i64, _>("sealed")? != 1)
            || (old.is_some() && expected.is_some() && old.as_deref() != expected)
        {
            return Err(PpsDbError::Invariant);
        }
        let mut funded_snapshot = None;
        if let Some(expected) = expected {
            let reused:i64=sqlx::query_scalar("SELECT COUNT(*) FROM pps_fee_reservations WHERE expected_txid=?1 AND attempt_id<>?2")
                .bind(expected).bind(attempt).fetch_one(&mut *tx).await?;
            if reused != 0 {
                return Err(PpsDbError::Invariant);
            }
        } else {
            // New signer/send authorization must observe the global financial
            // halt under this same write lock. A controller's earlier funding
            // check could race an overrun on another reserved attempt.
            // Immutable metadata for an already-sent transaction stays writable.
            let epoch = active_epoch(&mut tx).await?;
            // Send fence: no NEW send may be authorized while any conventional
            // halt or over-ceiling fee stands. Enforced here (not in snapshot),
            // inside BEGIN IMMEDIATE, so it serializes against the halt-recording
            // commit -- a halt stops sending without bricking anything else.
            let send_blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pps_conventional_halts) \
                 OR EXISTS(SELECT 1 FROM pps_conventional_attempts WHERE excess_fee IS NOT NULL)")
                .fetch_one(&mut *tx).await?;
            if send_blocked { return Err(PpsDbError::PayoutHalted); }
            let state = snapshot(&mut tx, &epoch, false).await?;
            if funding.is_some() {
                validate_lease(&state, funding, runtime_now()?)?;
                funded_snapshot = Some(state);
            }
        }
        sqlx::query("UPDATE pps_fee_reservations SET sealed=1,expected_txid=COALESCE(?1,expected_txid) WHERE attempt_id=?2")
            .bind(expected).bind(attempt).execute(&mut *tx).await?;
        if conventional {
            if let Some(txid) = expected {
                sqlx::query(
                    "UPDATE pps_conventional_attempts SET observed_txid=?1 WHERE attempt_id=?2",
                )
                .bind(txid)
                .bind(attempt)
                .execute(&mut *tx)
                .await?;
                let old: Option<String> =
                    sqlx::query_scalar("SELECT txid FROM payout_attempts WHERE id=?1")
                        .bind(attempt)
                        .fetch_one(&mut *tx)
                        .await?;
                if old.as_deref().is_some_and(|v| v != txid) {
                    return Err(PpsDbError::DuplicateMismatch);
                }
                sqlx::query("UPDATE payout_attempts SET txid=?1 WHERE id=?2")
                    .bind(txid)
                    .bind(attempt)
                    .execute(&mut *tx)
                    .await?;
            } else {
                sqlx::query("UPDATE payout_attempts SET status='submitting' WHERE id=?1 AND status='queued'")
                    .bind(attempt).execute(&mut *tx).await?;
            }
        }
        if let Some(state) = &funded_snapshot {
            validate_lease(state, funding, runtime_now()?)?;
        }
        tx.commit().await?;
        Ok(())
    }
    /// Deferred, SELECT-only transaction for a bounded low-frequency heartbeat.
    /// Reuses accounting checks without taking the admission writer reservation.
    pub async fn pps_credit_readiness_snapshot(
        &self, e: &PpsEpoch,
    ) -> Result<PpsCreditReadinessSnapshot, PpsDbError> {
        let mut tx=self.inner().begin().await?;
        if active_epoch(&mut tx).await? != *e { return Err(PpsDbError::EpochMismatch); }
        let generation:i64=sqlx::query_scalar("SELECT generation FROM pps_funding_generation WHERE singleton=1")
            .fetch_one(&mut *tx).await?;
        let financial_halt:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pps_conventional_halts) OR EXISTS(SELECT 1 FROM pps_conventional_attempts WHERE excess_fee IS NOT NULL)")
            .fetch_one(&mut *tx).await?;
        // snapshot no longer bricks under a halt, so surface live funding even
        // while financial_halt is set: ops can see the ledger is solvent and
        // only SENDING is fenced. financial_halt stays a distinct health flag.
        let funding=Some(snapshot(&mut tx,e,false).await?);
        tx.rollback().await?;
        Ok(PpsCreditReadinessSnapshot { generation:u64::try_from(generation).map_err(|_|PpsDbError::Invariant)?,
            financial_halt, funding })
    }
    /// Owner-run read-only projection. Normal startup never calls this or
    /// substitutes a proposed policy for the current persisted policy.
    pub async fn pps_testnet_budget_extension_snapshot(&self, previous: &PpsEpoch)
        -> Result<PpsFundingSnapshot, PpsDbError>
    {
        let mut tx = self.inner().begin().await?;
        let projected = extension_snapshot(&mut tx, previous).await?;
        tx.rollback().await?;
        Ok(projected)
    }

    /// Explicit one-time owner operation. No startup caller, no refill, and no
    /// wallet RPC. A successful commit invalidates even the supplied lease;
    /// runtime must collect a new proof rather than relabel its generation.
    pub async fn extend_testnet_pps_budget(&self, previous: &PpsEpoch, lease: &PpsFundingLease)
        -> Result<(), PpsDbError>
    {
        self.extend_testnet_pps_budget_with_clock(previous, lease, runtime_now).await
    }

    pub(crate) async fn extend_testnet_pps_budget_with_clock<F: Fn() -> Result<i64, PpsDbError>>(
        &self, previous: &PpsEpoch, lease: &PpsFundingLease, clock: F,
    ) -> Result<(), PpsDbError> {
        let next = testnet_budget_extension_epoch(previous)?;
        let mut c = self.inner().acquire().await?;
        sqlx::query("PRAGMA synchronous=FULL").execute(&mut *c).await?;
        let mut tx = c.begin_with("BEGIN IMMEDIATE").await?;
        // Check an existing journal before CREATE IF NOT EXISTS could repair
        // a partial schema. On the predecessor, add schema atomically with
        // the funded extension; failed evidence leaves even schema unchanged.
        if !extension_journal_present(&mut tx).await? {
            sqlx::raw_sql(include_str!("../migrations/019_pps_budget_extension.sql"))
                .execute(&mut *tx).await?;
        }
        let projected = extension_snapshot(&mut tx, previous).await?;
        validate_extension_lease(&projected, lease, clock()?)?;
        let before = crate::pps_live::reconcile(&mut tx).await?;
        let generation = i64::try_from(projected.generation).map_err(|_| PpsDbError::Invariant)?;
        let next_generation = generation.checked_add(1).ok_or(PpsDbError::Invariant)?;

        sqlx::query("INSERT INTO pps_epochs VALUES(?1,?2,?3,?4,?5)")
            .bind(&next.id).bind(&next.network).bind(i64::from(next.fee_bps))
            .bind(next.max_liability_zatoshis).bind(&next.quote_provenance)
            .execute(&mut *tx).await?;
        let policy_changed = sqlx::query("UPDATE pps_funding_policy SET credit_cap=95000000000,fee_cap=5000000000,total_cap=100000000000 WHERE singleton=1 AND network='testnet' AND credit_cap=950000000 AND fee_cap=50000000 AND total_cap=1000000000 AND reserve_floor=?1")
            .bind(previous.reserve_floor_zatoshis).execute(&mut *tx).await?.rows_affected();
        let meta_changed = sqlx::query("UPDATE pps_meta SET cap_zats=95000000000,active_epoch=?1 WHERE singleton=1 AND network='testnet' AND cap_zats=950000000 AND active_epoch=?2")
            .bind(&next.id).bind(&previous.id).execute(&mut *tx).await?.rows_affected();
        if policy_changed != 1 || meta_changed != 1 { return Err(PpsDbError::EpochMismatch); }
        crate::pps_live::epoch_check(&mut tx, &next).await?;
        let after = crate::pps_live::reconcile(&mut tx).await?;
        let mut expected_ledger = before.clone();
        expected_ledger.max_liability_subzatoshis = projected.cap_subzatoshis;
        if after != expected_ledger || snapshot(&mut tx, &next, false).await? != projected {
            return Err(PpsDbError::Invariant);
        }
        bump_generation(&mut tx).await?;
        sqlx::query("INSERT INTO pps_budget_extensions VALUES(1,'testnet-10-to-1000-v1','testnet',?1,?2,950000000,50000000,1000000000,95000000000,5000000000,100000000000,?3,?4,?5,?6,?7,?8,?9,?10,?11)")
            .bind(&previous.id).bind(&next.id).bind(next.reserve_floor_zatoshis)
            .bind(generation).bind(next_generation)
            .bind(i64::try_from(before.gross_subzatoshis / PPS_SCALE).map_err(|_| PpsDbError::Invariant)?)
            .bind((before.gross_subzatoshis % PPS_SCALE) as i64)
            .bind(i64::try_from(before.accepted_events).map_err(|_| PpsDbError::Invariant)?)
            .bind(projected.paid_zatoshis).bind(projected.paid_fees_zatoshis).bind(clock()?)
            .execute(&mut *tx).await?;
        // Writer lock prevents intervening accounting changes. Validate the
        // original pre-bump proof immediately before commit; never renew it.
        validate_extension_lease(&projected, lease, clock()?)?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn pps_funding_snapshot_for_epoch(
        &self,
        e: &PpsEpoch,
    ) -> Result<PpsFundingSnapshot, PpsDbError> {
        let mut tx = self.inner().begin_with("BEGIN IMMEDIATE").await?;
        let s = snapshot(&mut tx, e, true).await?;
        tx.commit().await?;
        Ok(s)
    }
    pub async fn pps_funding_snapshot(&self) -> Result<PpsFundingSnapshot, PpsDbError> {
        let mut tx = self.inner().begin_with("BEGIN IMMEDIATE").await?;
        let e = active_epoch(&mut tx).await?;
        let s = snapshot(&mut tx, &e, false).await?;
        tx.commit().await?;
        Ok(s)
    }
    pub async fn check_pps_funding(&self, l: &PpsFundingLease) -> Result<(), PpsDbError> {
        let mut tx = self.inner().begin_with("BEGIN IMMEDIATE").await?;
        let e = active_epoch(&mut tx).await?;
        let s = snapshot(&mut tx, &e, false).await?;
        validate_lease(&s, Some(l), runtime_now()?)?;
        tx.commit().await?;
        Ok(())
    }
}
