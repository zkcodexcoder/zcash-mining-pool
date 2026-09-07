//! Durable PPS accounting, isolated from legacy block credits and reversals.
//! The trusted validation caller MUST establish canonical proof identity, PoW,
//! immutable job target, miner-only subsidy and quote provenance before calling.
//! The cap is cumulative gross credits, including already-paid claims: neither
//! restart, payout nor a new fee epoch replenishes this conservative loss cap.
pub use crate::pps_funding::{
    PpsConventionalAttempt, PpsConventionalHalt, PpsConventionalIntent, PpsConventionalRecipient,
    PpsConventionalReservation, PpsFeeReservation, PpsFundingLease,
    PpsFundingSnapshot,
};
use crate::{PendingPayout, PoolDb};
use sqlx::{Connection, Row, SqliteConnection};
use std::collections::BTreeMap;

pub const PPS_SCALE: u128 = 1_000_000_000_000;
#[derive(Debug, thiserror::Error)]
pub enum PpsDbError {
    #[error("invalid PPS input or configuration")]
    Invalid,
    #[error("PPS epoch or cumulative cap mismatch")]
    EpochMismatch,
    #[error("legacy recovery must complete before PPS activation")]
    LegacyRecoveryRequired,
    #[error("duplicate proof has different immutable data")]
    DuplicateMismatch,
    #[error("current canonical chain agreement required")]
    ChainLeaseRequired,
    #[error("cumulative PPS liability cap exceeded")]
    CapExceeded,
    #[error("current PPS wallet funding evidence required")]
    FundingLeaseRequired,
    #[error("PPS wallet funding does not cover protected obligations")]
    FundingInsufficient,
    #[error("cumulative PPS fee or total exposure budget exceeded")]
    FeeBudgetExceeded,
    #[error("PPS accounting invariant failed")]
    Invariant,
    #[error("PPS database operation failed")]
    Database(#[from] sqlx::Error),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpsEpoch {
    pub id: String,
    pub network: String,
    pub fee_bps: u16,
    pub max_liability_zatoshis: i64,
    pub total_exposure_zatoshis: i64,
    pub fee_allowance_zatoshis: i64,
    pub reserve_floor_zatoshis: i64,
    pub quote_provenance: String,
}
#[derive(Debug, Clone)]
pub struct PpsChainLease {
    pub network: String,
    pub checked_at_unix: i64,
    pub valid_until_unix: i64,
    pub agreeing_references: u8,
    pub disagreement: bool,
}
#[derive(Debug, Clone)]
pub struct PpsCredit {
    pub proof_id: String,
    pub quote_id: String,
    pub worker_id: i64,
    pub job_id: String,
    pub session_id: String,
    pub difficulty: f64,
    pub is_block: bool,
    pub amount_subzatoshis: u128,
    pub accepted_at_unix: i64,
    pub quote_height: u64,
    pub network_target_be: [u8; 32],
    pub assigned_share_target_be: [u8; 32],
    pub miner_subsidy_zats: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpsReceipt {
    pub share_id: i64,
    pub duplicate: bool,
    pub credited_subzatoshis: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpsLedgerSummary {
    pub accepted_events: u64,
    pub gross_subzatoshis: u128,
    pub max_liability_subzatoshis: u128,
    pub pending_zatoshis: i64,
    pub paying_zatoshis: i64,
    pub paid_zatoshis: i64,
    pub fractional_subzatoshis: u128,
}
fn id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}
fn hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn parts(a: u128) -> Result<(i64, i64), PpsDbError> {
    Ok((
        i64::try_from(a / PPS_SCALE).map_err(|_| PpsDbError::Invalid)?,
        (a % PPS_SCALE) as i64,
    ))
}
fn amount(w: i64, f: i64) -> Result<u128, PpsDbError> {
    if w < 0 || f < 0 || f as u128 >= PPS_SCALE {
        return Err(PpsDbError::Invariant);
    }
    Ok(w as u128 * PPS_SCALE + f as u128)
}
fn add(a: i64, b: i64) -> Result<i64, PpsDbError> {
    a.checked_add(b)
        .filter(|v| *v >= 0)
        .ok_or(PpsDbError::Invariant)
}
pub(crate) async fn epoch_check(c: &mut SqliteConnection, e: &PpsEpoch) -> Result<(), PpsDbError> {
    crate::pps_funding::policy_check(c, e, false).await?;
    let row = sqlx::query("SELECT * FROM pps_epochs WHERE id=?1")
        .bind(&e.id)
        .fetch_optional(&mut *c)
        .await?
        .ok_or(PpsDbError::EpochMismatch)?;
    let active: Option<(String,)> =
        sqlx::query_as("SELECT active_epoch FROM pps_meta WHERE singleton=1")
            .fetch_optional(&mut *c)
            .await?;
    if row.try_get::<String, _>("network")? != e.network
        || row.try_get::<i64, _>("fee_bps")? != i64::from(e.fee_bps)
        || row.try_get::<i64, _>("cap_zats")? != e.max_liability_zatoshis
        || row.try_get::<String, _>("quote_provenance")? != e.quote_provenance
        || active.map(|v| v.0) != Some(e.id.clone())
    {
        return Err(PpsDbError::EpochMismatch);
    }
    Ok(())
}
impl PoolDb {
    pub async fn assert_reward_mode(&self, pps: bool) -> Result<(), PpsDbError> {
        let active: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pps_meta")
            .fetch_one(self.inner())
            .await?;
        if active > 0 && !pps {
            return Err(PpsDbError::EpochMismatch);
        }
        Ok(())
    }
    pub async fn initialize_pps_epoch(
        &self,
        e: &PpsEpoch,
        funding: Option<&PpsFundingLease>,
    ) -> Result<(), PpsDbError> {
        if !id(&e.id)
            || !id(&e.quote_provenance)
            || !matches!(e.network.as_str(), "mainnet" | "testnet")
            || e.fee_bps >= 10000
            || e.max_liability_zatoshis <= 0
        {
            return Err(PpsDbError::Invalid);
        }
        let mut c = self.inner().acquire().await?;
        sqlx::query("PRAGMA synchronous=FULL")
            .execute(&mut *c)
            .await?;
        let mut tx = c.begin_with("BEGIN IMMEDIATE").await?;
        crate::pps_funding::check(
            &mut tx,
            e,
            funding,
            crate::pps_funding::runtime_now()?,
            true,
        )
        .await?;
        crate::pps_funding::persist_policy(&mut tx, e).await?;
        let meta = sqlx::query("SELECT * FROM pps_meta WHERE singleton=1")
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(meta) = meta {
            if meta.try_get::<String, _>("network")? != e.network
                || meta.try_get::<i64, _>("cap_zats")? != e.max_liability_zatoshis
            {
                return Err(PpsDbError::EpochMismatch);
            }
        } else {
            let unresolved:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM block_submissions WHERE resolved=0)+(SELECT COUNT(*) FROM blocks b WHERE b.status IN ('pending','confirmed') AND b.created_at>datetime('now','-30 days') AND NOT EXISTS(SELECT 1 FROM block_credits bc WHERE bc.block_id=b.id))").fetch_one(&mut *tx).await?;
            if unresolved != 0 {
                return Err(PpsDbError::LegacyRecoveryRequired);
            }
            sqlx::query(
                "INSERT INTO pps_meta(singleton,network,active_epoch,cap_zats) VALUES(1,?1,?2,?3)",
            )
            .bind(&e.network)
            .bind(&e.id)
            .bind(e.max_liability_zatoshis)
            .execute(&mut *tx)
            .await?;
        }
        let old: Option<(String,)> = sqlx::query_as("SELECT id FROM pps_epochs WHERE id=?1")
            .bind(&e.id)
            .fetch_optional(&mut *tx)
            .await?;
        if old.is_none() {
            sqlx::query("INSERT INTO pps_epochs VALUES(?1,?2,?3,?4,?5)")
                .bind(&e.id)
                .bind(&e.network)
                .bind(i64::from(e.fee_bps))
                .bind(e.max_liability_zatoshis)
                .bind(&e.quote_provenance)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE pps_meta SET active_epoch=?1 WHERE singleton=1")
            .bind(&e.id)
            .execute(&mut *tx)
            .await?;
        epoch_check(&mut tx, e).await?;
        reconcile(&mut tx).await?;
        crate::pps_funding::check(
            &mut tx,
            e,
            funding,
            crate::pps_funding::runtime_now()?,
            false,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn verify_pps_epoch(&self, e: &PpsEpoch) -> Result<(), PpsDbError> {
        let mut c = self.inner().acquire().await?;
        epoch_check(&mut c, e).await
    }
    /// Read-only: lets the lease refresh loop notice a payout-side generation
    /// bump and start a replacement collection immediately instead of waiting
    /// out its cadence.
    pub async fn pps_funding_generation(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT generation FROM pps_funding_generation WHERE singleton=1")
            .fetch_one(self.inner())
            .await
    }
    pub async fn credit_pps_share(
        &self,
        e: &PpsEpoch,
        s: &PpsCredit,
        lease: Option<&PpsChainLease>,
        funding: Option<&PpsFundingLease>,
        now: i64,
    ) -> Result<PpsReceipt, PpsDbError> {
        self.credit_pps_share_with_clock(e, s, lease, funding, now, || {
            #[cfg(test)]
            {
                Ok(now)
            }
            #[cfg(not(test))]
            {
                let elapsed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| PpsDbError::ChainLeaseRequired)?;
                i64::try_from(elapsed.as_secs()).map_err(|_| PpsDbError::ChainLeaseRequired)
            }
        })
        .await
    }
    // Private clock injection keeps unit tests deterministic. Production uses
    // SystemTime after lock acquisition AND immediately before commit.
    async fn credit_pps_share_with_clock<F: Fn() -> Result<i64, PpsDbError>>(
        &self,
        e: &PpsEpoch,
        s: &PpsCredit,
        lease: Option<&PpsChainLease>,
        funding: Option<&PpsFundingLease>,
        request_now: i64,
        clock: F,
    ) -> Result<PpsReceipt, PpsDbError> {
        if !hex(&s.proof_id)
            || !hex(&s.quote_id)
            || s.worker_id <= 0
            || !id(&s.job_id)
            || !id(&s.session_id)
            || !s.difficulty.is_finite()
            || s.difficulty <= 0.0
            || s.amount_subzatoshis == 0
            || s.accepted_at_unix < 0
            || request_now < 0
        {
            return Err(PpsDbError::Invalid);
        }
        let (whole, fraction) = parts(s.amount_subzatoshis)?;
        let height = i64::try_from(s.quote_height).map_err(|_| PpsDbError::Invalid)?;
        let subsidy = i64::try_from(s.miner_subsidy_zats).map_err(|_| PpsDbError::Invalid)?;
        if subsidy <= 0
            || s.network_target_be == [0; 32]
            || s.assigned_share_target_be < s.network_target_be
        {
            return Err(PpsDbError::Invalid);
        }
        let mut c = self.inner().acquire().await?;
        sqlx::query("PRAGMA synchronous=FULL")
            .execute(&mut *c)
            .await?;
        let mut tx = c.begin_with("BEGIN IMMEDIATE").await?;
        let m: i64 = sqlx::query_scalar("SELECT miner_id FROM workers WHERE id=?1")
            .bind(s.worker_id)
            .fetch_one(&mut *tx)
            .await?;
        let quote = sqlx::query("SELECT * FROM pps_quotes WHERE quote_id=?1")
            .bind(&s.quote_id)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(q) = &quote {
            if q.try_get::<String, _>("epoch_id")? != e.id
                || q.try_get::<i64, _>("height")? != height
                || q.try_get::<i64, _>("miner_subsidy")? != subsidy
                || q.try_get::<Vec<u8>, _>("network_target")?.as_slice() != s.network_target_be
                || q.try_get::<Vec<u8>, _>("assigned_target")?.as_slice()
                    != s.assigned_share_target_be
                || amount(q.try_get("amount_whole")?, q.try_get("amount_fraction")?)?
                    != s.amount_subzatoshis
            {
                return Err(PpsDbError::DuplicateMismatch);
            }
        }
        if let Some(row) = sqlx::query("SELECT * FROM pps_events WHERE proof_id=?1")
            .bind(&s.proof_id)
            .fetch_optional(&mut *tx)
            .await?
        {
            if row.try_get::<String, _>("epoch_id")? != e.id
                || row.try_get::<String, _>("quote_id")? != s.quote_id
                || row.try_get::<i64, _>("miner_id")? != m
                || amount(
                    row.try_get("amount_whole")?,
                    row.try_get("amount_fraction")?,
                )? != s.amount_subzatoshis
            {
                return Err(PpsDbError::DuplicateMismatch);
            }
            if quote.is_none() {
                return Err(PpsDbError::Invariant);
            }
            let receipt = PpsReceipt {
                share_id: row.try_get("share_id")?,
                duplicate: true,
                credited_subzatoshis: s.amount_subzatoshis,
            };
            tx.commit().await?;
            return Ok(receipt);
        }
        epoch_check(&mut tx, e).await?;
        let l = lease.ok_or(PpsDbError::ChainLeaseRequired)?;
        let now = clock()?;
        crate::pps_funding::check_credit(&mut tx, e, funding, now).await?;
        if l.network != e.network
            || l.disagreement
            || l.agreeing_references < 2
            || l.checked_at_unix < 0
            || l.valid_until_unix < l.checked_at_unix
            || l.valid_until_unix - l.checked_at_unix > 300
            || now < l.checked_at_unix
            || now >= l.valid_until_unix
            || s.accepted_at_unix < l.checked_at_unix
            || s.accepted_at_unix > now
        {
            return Err(PpsDbError::ChainLeaseRequired);
        }
        let meta = sqlx::query("SELECT * FROM pps_meta WHERE singleton=1")
            .fetch_one(&mut *tx)
            .await?;
        let gross = amount(
            meta.try_get("gross_whole")?,
            meta.try_get("gross_fraction")?,
        )?
        .checked_add(s.amount_subzatoshis)
        .ok_or(PpsDbError::Invariant)?;
        if gross > e.max_liability_zatoshis as u128 * PPS_SCALE {
            return Err(PpsDbError::CapExceeded);
        }
        if quote.is_none() {
            sqlx::query("INSERT INTO pps_quotes VALUES(?1,?2,?3,?4,?5,?6,?7,?8)")
                .bind(&s.quote_id)
                .bind(&e.id)
                .bind(height)
                .bind(s.network_target_be.as_slice())
                .bind(s.assigned_share_target_be.as_slice())
                .bind(subsidy)
                .bind(whole)
                .bind(fraction)
                .execute(&mut *tx)
                .await?;
        }
        let count = add(meta.try_get("event_count")?, 1)?;
        let gp = parts(gross)?;
        sqlx::query("INSERT OR IGNORE INTO pps_accounts(miner_id) VALUES(?1)")
            .bind(m)
            .execute(&mut *tx)
            .await?;
        let a = sqlx::query("SELECT pending,fraction FROM pps_accounts WHERE miner_id=?1")
            .bind(m)
            .fetch_one(&mut *tx)
            .await?;
        let np = parts(
            amount(a.try_get("pending")?, a.try_get("fraction")?)?
                .checked_add(s.amount_subzatoshis)
                .ok_or(PpsDbError::Invariant)?,
        )?;
        let r=sqlx::query("INSERT INTO shares(worker_id,job_id,difficulty,is_block,session_id) VALUES(?1,?2,?3,?4,?5)").bind(s.worker_id).bind(&s.job_id).bind(s.difficulty).bind(i64::from(s.is_block)).bind(&s.session_id).execute(&mut *tx).await?;
        let share_id = r.last_insert_rowid();
        sqlx::query("INSERT INTO pps_events VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)")
            .bind(&s.proof_id)
            .bind(&e.id)
            .bind(&s.quote_id)
            .bind(m)
            .bind(s.worker_id)
            .bind(&s.job_id)
            .bind(&s.session_id)
            .bind(s.difficulty)
            .bind(i64::from(s.is_block))
            .bind(whole)
            .bind(fraction)
            .bind(s.accepted_at_unix)
            .bind(share_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE pps_accounts SET pending=?1,fraction=?2 WHERE miner_id=?3")
            .bind(np.0)
            .bind(np.1)
            .bind(m)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE pps_meta SET gross_whole=?1,gross_fraction=?2,event_count=?3 WHERE singleton=1",
        )
        .bind(gp.0)
        .bind(gp.1)
        .bind(count)
        .execute(&mut *tx)
        .await?;
        let commit_now = clock()?;
        if commit_now < l.checked_at_unix
            || commit_now >= l.valid_until_unix
            || s.accepted_at_unix > commit_now
        {
            return Err(PpsDbError::ChainLeaseRequired);
        }
        crate::pps_funding::check(&mut tx, e, funding, commit_now, false).await?;
        tx.commit().await?;
        Ok(PpsReceipt {
            share_id,
            duplicate: false,
            credited_subzatoshis: s.amount_subzatoshis,
        })
    }
    pub async fn pps_invariant(&self) -> Result<PpsLedgerSummary, PpsDbError> {
        let mut tx = self.inner().begin_with("BEGIN IMMEDIATE").await?;
        let s = reconcile(&mut tx).await?;
        tx.commit().await?;
        Ok(s)
    }
    pub async fn get_pending_pps_payouts(
        &self,
        min_amount: i64,
    ) -> Result<Vec<PendingPayout>, PpsDbError> {
        if min_amount <= 0 {
            return Err(PpsDbError::Invalid);
        }
        let rows=sqlx::query("SELECT p.miner_id,m.address,m.created_at,p.pending FROM pps_accounts p JOIN miners m ON m.id=p.miner_id WHERE p.pending>=?1 ORDER BY p.miner_id").bind(min_amount).fetch_all(self.inner()).await?;
        Ok(rows
            .into_iter()
            .map(|r| PendingPayout {
                miner_id: r.get("miner_id"),
                address: r.get("address"),
                amount: r.get("pending"),
                created_at: r.get("created_at"),
            })
            .collect())
    }
    pub async fn reserve_pps_payout(
        &self,
        attempt_id: i64,
        items: &[(i64, i64)],
        funding: &PpsFundingLease,
        fee: &PpsFeeReservation,
    ) -> Result<Vec<(i64, i64)>, PpsDbError> {
        self.reserve_pps_payout_inner(attempt_id, items, funding, fee, None)
            .await
    }
    pub async fn reserve_pps_conventional_payout(
        &self,
        attempt_id: i64,
        items: &[(i64, i64)],
        funding: &PpsFundingLease,
        intent: &PpsConventionalReservation,
    ) -> Result<Vec<(i64, i64)>, PpsDbError> {
        let fee = PpsFeeReservation {
            proposal_id: intent.intent_id.clone(),
            fee_zatoshis: intent.fee_upper_bound_zatoshis,
        };
        self.reserve_pps_payout_inner(attempt_id, items, funding, &fee, Some(intent))
            .await
    }
    async fn reserve_pps_payout_inner(
        &self,
        attempt_id: i64,
        items: &[(i64, i64)],
        funding: &PpsFundingLease,
        fee: &PpsFeeReservation,
        conventional: Option<&PpsConventionalReservation>,
    ) -> Result<Vec<(i64, i64)>, PpsDbError> {
        if items.is_empty() || items.len() > 1024 || items.iter().any(|(m, a)| *m <= 0 || *a <= 0) {
            return Err(PpsDbError::Invalid);
        }
        let mut c = self.inner().acquire().await?;
        sqlx::query("PRAGMA synchronous=FULL")
            .execute(&mut *c)
            .await?;
        let mut tx = c.begin_with("BEGIN IMMEDIATE").await?;
        reconcile(&mut tx).await?;
        let e = crate::pps_funding::active_epoch(&mut tx).await?;
        if conventional.is_some() && e.network != "testnet" {
            return Err(PpsDbError::Invalid);
        }
        if let Some(intent) = conventional {
            let parsed = intent.intent()?;
            if parsed.epoch != e.id || parsed.items.len() != items.len() {
                return Err(PpsDbError::Invalid);
            }
            let expected: BTreeMap<i64, i64> = items.iter().copied().collect();
            if expected.len() != items.len() { return Err(PpsDbError::Invalid); }
            for item in parsed.items {
                let address: String = sqlx::query_scalar("SELECT address FROM miners WHERE id=?1")
                    .bind(item.miner_id).fetch_one(&mut *tx).await?;
                if expected.get(&item.miner_id) != Some(&item.amount_zatoshis)
                    || address != item.address
                { return Err(PpsDbError::Invalid); }
            }
        }
        crate::pps_funding::check(
            &mut tx,
            &e,
            Some(funding),
            crate::pps_funding::runtime_now()?,
            false,
        )
        .await?;
        let attempt: Option<(String,)> =
            sqlx::query_as("SELECT status FROM payout_attempts WHERE id=?1")
                .bind(attempt_id)
                .fetch_optional(&mut *tx)
                .await?;
        if !matches!(attempt.as_ref().map(|v| v.0.as_str()), Some("queued")) {
            return Err(PpsDbError::Invariant);
        }
        let legacy: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM payout_items WHERE attempt_id=?1")
                .bind(attempt_id)
                .fetch_one(&mut *tx)
                .await?;
        if legacy != 0 {
            return Err(PpsDbError::Invariant);
        }
        let prior:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM pps_payout_items WHERE attempt_id=?1)+(SELECT COUNT(*) FROM pps_payouts WHERE attempt_id=?1)").bind(attempt_id).fetch_one(&mut *tx).await?;
        if prior != 0 {
            return Err(PpsDbError::Invariant);
        }
        let mut out = Vec::new();
        for &(miner, amount) in items {
            if amount <= 0 {
                return Err(PpsDbError::Invalid);
            }
            let row = sqlx::query("SELECT pending,paying FROM pps_accounts WHERE miner_id=?1")
                .bind(miner)
                .fetch_optional(&mut *tx)
                .await?;
            let Some(row) = row else {
                return Err(PpsDbError::Invariant);
            };
            let pending: i64 = row.try_get("pending")?;
            if pending < amount {
                // A fixed PCZT proposal must never be reserved for a subset of
                // its inspected recipients. Retry with a new proposal instead.
                return Err(PpsDbError::Invariant);
            }
            let paying = add(row.try_get("paying")?, amount)?;
            sqlx::query("UPDATE pps_accounts SET pending=?1,paying=?2 WHERE miner_id=?3")
                .bind(pending - amount)
                .bind(paying)
                .bind(miner)
                .execute(&mut *tx)
                .await?;
            sqlx::query("INSERT INTO pps_payout_items VALUES(?1,?2,?3)")
                .bind(attempt_id)
                .bind(miner)
                .bind(amount)
                .execute(&mut *tx)
                .await?;
            out.push((miner, amount));
        }
        if !out.is_empty() {
            crate::pps_funding::reserve_fee(&mut tx, attempt_id, fee, conventional).await?;
            crate::pps_funding::check(
                &mut tx,
                &e,
                Some(funding),
                crate::pps_funding::runtime_now()?,
                false,
            )
            .await?;
            crate::pps_funding::bump_generation(&mut tx).await?;
        }
        tx.commit().await?;
        Ok(out)
    }
    pub async fn confirm_pps_payout(&self, attempt_id: i64, txid: &str) -> Result<i64, PpsDbError> {
        if !hex(txid) {
            return Err(PpsDbError::Invalid);
        }
        self.settle_pps_payout(attempt_id, Some(txid), None).await
    }
    pub async fn confirm_pps_conventional_payout(
        &self,
        attempt_id: i64,
        txid: &str,
        actual_fee_zatoshis: i64,
    ) -> Result<i64, PpsDbError> {
        if !hex(txid) || actual_fee_zatoshis < 0 {
            return Err(PpsDbError::Invalid);
        }
        self.settle_pps_payout(attempt_id, Some(txid), Some(actual_fee_zatoshis))
            .await
    }
    pub async fn refund_pps_payout(&self, attempt_id: i64) -> Result<i64, PpsDbError> {
        self.settle_pps_payout(attempt_id, None, None).await
    }
    async fn settle_pps_payout(
        &self,
        attempt_id: i64,
        txid: Option<&str>,
        actual_fee: Option<i64>,
    ) -> Result<i64, PpsDbError> {
        let mut c = self.inner().acquire().await?;
        sqlx::query("PRAGMA synchronous=FULL")
            .execute(&mut *c)
            .await?;
        let mut tx = c.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(txid) = txid {
            if crate::pps_funding::validate_confirmation(&mut tx, attempt_id, txid, actual_fee)
                .await?
            {
                // An authenticated actual fee above its pre-send ceiling is a
                // durable PPS-only halt, not a reason to erase miner claims.
                crate::pps_funding::bump_generation(&mut tx).await?;
                tx.commit().await?;
                return Err(PpsDbError::FeeBudgetExceeded);
            }
        }
        reconcile(&mut tx).await?;
        let legacy: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM payout_items WHERE attempt_id=?1")
                .bind(attempt_id)
                .fetch_one(&mut *tx)
                .await?;
        if legacy != 0 {
            return Err(PpsDbError::Invariant);
        }
        if let Some(txid) = txid {
            let reused:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM pps_payouts WHERE txid=?1 AND attempt_id<>?2)+(SELECT COUNT(*) FROM payouts WHERE txid=?1)")
                .bind(txid).bind(attempt_id).fetch_one(&mut *tx).await?;
            if reused != 0 {
                return Err(PpsDbError::Invariant);
            }
        }
        let items = sqlx::query("SELECT miner_id,amount FROM pps_payout_items WHERE attempt_id=?1")
            .bind(attempt_id)
            .fetch_all(&mut *tx)
            .await?;
        if items.is_empty() {
            let settled = sqlx::query("SELECT txid FROM pps_payouts WHERE attempt_id=?1")
                .bind(attempt_id)
                .fetch_all(&mut *tx)
                .await?;
            for row in settled {
                if txid != Some(row.try_get::<String, _>("txid")?.as_str()) {
                    return Err(PpsDbError::Invariant);
                }
            }
        }
        for item in &items {
            let miner: i64 = item.try_get("miner_id")?;
            let amt: i64 = item.try_get("amount")?;
            let row = sqlx::query("SELECT pending,paying,paid FROM pps_accounts WHERE miner_id=?1")
                .bind(miner)
                .fetch_one(&mut *tx)
                .await?;
            let paying: i64 = row.try_get("paying")?;
            if paying < amt {
                return Err(PpsDbError::Invariant);
            }
            let pending = if txid.is_none() {
                add(row.try_get("pending")?, amt)?
            } else {
                row.try_get("pending")?
            };
            let paid = if txid.is_some() {
                add(row.try_get("paid")?, amt)?
            } else {
                row.try_get("paid")?
            };
            sqlx::query("UPDATE pps_accounts SET pending=?1,paying=?2,paid=?3 WHERE miner_id=?4")
                .bind(pending)
                .bind(paying - amt)
                .bind(paid)
                .bind(miner)
                .execute(&mut *tx)
                .await?;
            if let Some(txid) = txid {
                sqlx::query(
                    "INSERT INTO pps_payouts(attempt_id,miner_id,txid,amount) VALUES(?1,?2,?3,?4)",
                )
                .bind(attempt_id)
                .bind(miner)
                .bind(txid)
                .bind(amt)
                .execute(&mut *tx)
                .await?;
            }
        }
        sqlx::query("DELETE FROM pps_payout_items WHERE attempt_id=?1")
            .bind(attempt_id)
            .execute(&mut *tx)
            .await?;
        if !items.is_empty() {
            crate::pps_funding::settle_fee(&mut tx, attempt_id, txid, actual_fee).await?;
            sqlx::query("UPDATE payout_attempts SET status=?1,updated_at=datetime('now') WHERE id=?2 AND EXISTS (SELECT 1 FROM pps_conventional_attempts ca WHERE ca.attempt_id=payout_attempts.id)")
                .bind(if txid.is_some() { "confirmed" } else { "failed" })
                .bind(attempt_id).execute(&mut *tx).await?;
            crate::pps_funding::bump_generation(&mut tx).await?;
        }
        tx.commit().await?;
        Ok(items.len() as i64)
    }
    pub async fn get_reserved_pps_attempts(
        &self,
        older_than_minutes: Option<i64>,
    ) -> Result<Vec<(i64, String, Option<String>, Option<String>, i64, String)>, PpsDbError> {
        let rows=sqlx::query("SELECT pa.id,pa.status,pa.opid,COALESCE(f.expected_txid,pa.txid) AS txid,pa.created_at,pi.amount FROM payout_attempts pa JOIN pps_payout_items pi ON pi.attempt_id=pa.id JOIN pps_fee_reservations f ON f.attempt_id=pa.id WHERE (?1 IS NULL OR pa.created_at<datetime('now','-'||?1||' minutes')) AND NOT EXISTS (SELECT 1 FROM pps_conventional_attempts ca WHERE ca.attempt_id=pa.id) ORDER BY pa.id").bind(older_than_minutes).fetch_all(self.inner()).await?;
        let mut out: Vec<(i64, String, Option<String>, Option<String>, i64, String)> = Vec::new();
        for r in rows {
            let id: i64 = r.try_get("id")?;
            let amt: i64 = r.try_get("amount")?;
            if let Some(last) = out.last_mut().filter(|v| v.0 == id) {
                last.4 = add(last.4, amt)?;
            } else {
                out.push((
                    id,
                    r.try_get("status")?,
                    r.try_get("opid")?,
                    r.try_get("txid")?,
                    amt,
                    r.try_get("created_at")?,
                ))
            }
        }
        Ok(out)
    }
}
pub(crate) async fn reconcile(c: &mut SqliteConnection) -> Result<PpsLedgerSummary, PpsDbError> {
    if sqlx::query("PRAGMA foreign_key_check")
        .fetch_optional(&mut *c)
        .await?
        .is_some()
    {
        return Err(PpsDbError::Invariant);
    }
    let bad_quotes:i64=sqlx::query_scalar("SELECT COUNT(*) FROM pps_events e JOIN pps_quotes q ON q.quote_id=e.quote_id WHERE e.epoch_id<>q.epoch_id OR e.amount_whole<>q.amount_whole OR e.amount_fraction<>q.amount_fraction")
        .fetch_one(&mut *c).await?;
    if bad_quotes != 0 {
        return Err(PpsDbError::Invariant);
    }
    let orphan_attribution:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM pps_payout_items pi WHERE NOT EXISTS(SELECT 1 FROM pps_accounts a WHERE a.miner_id=pi.miner_id))+(SELECT COUNT(*) FROM pps_payouts p WHERE NOT EXISTS(SELECT 1 FROM pps_accounts a WHERE a.miner_id=p.miner_id))+(SELECT COUNT(*) FROM pps_payout_items pi JOIN payout_items li ON li.attempt_id=pi.attempt_id)")
        .fetch_one(&mut *c).await?;
    if orphan_attribution != 0 {
        return Err(PpsDbError::Invariant);
    }
    let meta = sqlx::query("SELECT * FROM pps_meta WHERE singleton=1")
        .fetch_optional(&mut *c)
        .await?
        .ok_or(PpsDbError::EpochMismatch)?;
    let gross = amount(
        meta.try_get("gross_whole")?,
        meta.try_get("gross_fraction")?,
    )?;
    let cap = amount(meta.try_get("cap_zats")?, 0)?;
    let expected: i64 = meta.try_get("event_count")?;
    let mut per_miner = BTreeMap::<i64, u128>::new();
    let mut total = 0u128;
    let mut count = 0i64;
    let mut cursor = String::new();
    loop {
        let rows=sqlx::query("SELECT proof_id,miner_id,amount_whole,amount_fraction FROM pps_events WHERE proof_id>?1 ORDER BY proof_id LIMIT 256").bind(&cursor).fetch_all(&mut *c).await?;
        if rows.is_empty() {
            break;
        }
        for r in rows {
            cursor = r.try_get("proof_id")?;
            if !hex(&cursor) {
                return Err(PpsDbError::Invariant);
            }
            let m: i64 = r.try_get("miner_id")?;
            let a = amount(r.try_get("amount_whole")?, r.try_get("amount_fraction")?)?;
            if a == 0 {
                return Err(PpsDbError::Invariant);
            }
            total = total.checked_add(a).ok_or(PpsDbError::Invariant)?;
            count = add(count, 1)?;
            let v = per_miner.entry(m).or_default();
            *v = v.checked_add(a).ok_or(PpsDbError::Invariant)?;
        }
    }
    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pps_events")
        .fetch_one(&mut *c)
        .await?;
    if total != gross || count != expected || count != stored || gross > cap {
        return Err(PpsDbError::Invariant);
    }
    let mut s = PpsLedgerSummary {
        accepted_events: count as u64,
        gross_subzatoshis: gross,
        max_liability_subzatoshis: cap,
        pending_zatoshis: 0,
        paying_zatoshis: 0,
        paid_zatoshis: 0,
        fractional_subzatoshis: 0,
    };
    let accounts = sqlx::query("SELECT * FROM pps_accounts")
        .fetch_all(&mut *c)
        .await?;
    for r in accounts {
        let m: i64 = r.try_get("miner_id")?;
        let pending: i64 = r.try_get("pending")?;
        let paying: i64 = r.try_get("paying")?;
        let paid: i64 = r.try_get("paid")?;
        let fraction: i64 = r.try_get("fraction")?;
        let all = amount(add(add(pending, paying)?, paid)?, fraction)?;
        if per_miner.remove(&m) != Some(all) {
            return Err(PpsDbError::Invariant);
        }
        let reserved = sqlx::query("SELECT amount FROM pps_payout_items WHERE miner_id=?1")
            .bind(m)
            .fetch_all(&mut *c)
            .await?;
        let mut reservation = 0i64;
        for r in reserved {
            reservation = add(reservation, r.try_get("amount")?)?;
        }
        if reservation != paying {
            return Err(PpsDbError::Invariant);
        }
        let settled = sqlx::query("SELECT amount FROM pps_payouts WHERE miner_id=?1")
            .bind(m)
            .fetch_all(&mut *c)
            .await?;
        let mut settled_sum = 0i64;
        for r in settled {
            settled_sum = add(settled_sum, r.try_get("amount")?)?;
        }
        if settled_sum != paid {
            return Err(PpsDbError::Invariant);
        }
        s.pending_zatoshis = add(s.pending_zatoshis, pending)?;
        s.paying_zatoshis = add(s.paying_zatoshis, paying)?;
        s.paid_zatoshis = add(s.paid_zatoshis, paid)?;
        s.fractional_subzatoshis = s
            .fractional_subzatoshis
            .checked_add(fraction as u128)
            .ok_or(PpsDbError::Invariant)?;
    }
    if !per_miner.is_empty() {
        return Err(PpsDbError::Invariant);
    }
    let e = crate::pps_funding::active_epoch(c).await?;
    crate::pps_funding::snapshot(c, &e, false).await?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    const NOW: i64 = 1_700_000_000;
    struct Fixture {
        db: PoolDb,
        e: PpsEpoch,
        l: PpsChainLease,
        w: i64,
        m: i64,
    }
    async fn setup(active: bool, cap: i64) -> Fixture {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let db = PoolDb::new(pool);
        setup_db(db, active, cap).await
    }
    async fn setup_db(db: PoolDb, active: bool, cap: i64) -> Fixture {
        db.run_migrations().await.unwrap();
        db.assert_critical_schema().await.unwrap();
        let m = db.get_or_create_miner("synthetic-miner").await.unwrap().id;
        let w = db
            .get_or_create_worker(m, "fixture-worker")
            .await
            .unwrap()
            .id;
        let e = PpsEpoch {
            id: "epoch-one".into(),
            network: "testnet".into(),
            fee_bps: 100,
            max_liability_zatoshis: cap,
            total_exposure_zatoshis: cap + 100,
            fee_allowance_zatoshis: 100,
            reserve_floor_zatoshis: 10,
            quote_provenance: "pps-v1-fixed-job-target-miner-subsidy".into(),
        };
        let l = PpsChainLease {
            network: "testnet".into(),
            checked_at_unix: NOW,
            valid_until_unix: NOW + 60,
            agreeing_references: 2,
            disagreement: false,
        };
        if active {
            db.initialize_pps_epoch(&e, Some(&funding_for(&db, &e).await))
                .await
                .unwrap();
        }
        Fixture { db, e, l, w, m }
    }
    async fn funding_for(db: &PoolDb, e: &PpsEpoch) -> PpsFundingLease {
        let generation: i64 =
            sqlx::query_scalar("SELECT generation FROM pps_funding_generation WHERE singleton=1")
                .fetch_one(db.inner())
                .await
                .unwrap();
        PpsFundingLease {
            network: e.network.clone(),
            checked_at_unix: NOW,
            valid_until_unix: NOW + 90,
            spendable_zatoshis: 1_000_000_000,
            reserve_floor_zatoshis: e.reserve_floor_zatoshis,
            reserved_fee_allowance_zatoshis: e.fee_allowance_zatoshis,
            generation: generation as u64,
        }
    }
    async fn funded(f: &Fixture) -> PpsFundingLease {
        funding_for(&f.db, &f.e).await
    }
    fn fee(attempt: i64) -> PpsFeeReservation {
        PpsFeeReservation {
            proposal_id: format!("{attempt:064x}"),
            fee_zatoshis: 1,
        }
    }
    async fn signed(f: &Fixture, attempt: i64, txid: &str) {
        f.db.seal_pps_payout(attempt, &fee(attempt).proposal_id)
            .await
            .unwrap();
        f.db.mark_pps_payout_signed(attempt, &fee(attempt).proposal_id, txid)
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn funding_activation_is_atomic_and_preserves_legacy_pending_and_paying() {
        let f = setup(false, 10).await;
        f.db.credit_balance(f.m, 40).await.unwrap();
        let a = f.db.create_payout_attempt(1, 15, "legacy").await.unwrap();
        f.db.reserve_payout(a, &[(f.m, 15)]).await.unwrap();
        let s = f.db.pps_funding_snapshot_for_epoch(&f.e).await.unwrap();
        assert_eq!(
            (s.legacy_pending_zatoshis, s.legacy_paying_zatoshis),
            (25, 15)
        );
        assert_eq!(s.required_spendable_zatoshis, 160);
        let mut l = funded(&f).await;
        l.spendable_zatoshis = 159;
        assert!(matches!(
            f.db.initialize_pps_epoch(&f.e, Some(&l)).await,
            Err(PpsDbError::FundingInsufficient)
        ));
        assert!(matches!(
            f.db.initialize_pps_epoch(&f.e, None).await,
            Err(PpsDbError::FundingLeaseRequired)
        ));
        let n: i64 = sqlx::query_scalar(
            "SELECT (SELECT COUNT(*) FROM pps_meta)+(SELECT COUNT(*) FROM pps_funding_policy)",
        )
        .fetch_one(f.db.inner())
        .await
        .unwrap();
        assert_eq!(n, 0);
        l.spendable_zatoshis = 160;
        f.db.initialize_pps_epoch(&f.e, Some(&l)).await.unwrap();
        f.db.check_pps_funding(&l).await.unwrap();
    }

    #[tokio::test]
    async fn credit_readiness_snapshot_is_read_only_and_tracks_non_generation_liabilities() {
        let f=setup(true,10).await;
        let before=f.db.pps_funding_snapshot().await.unwrap();
        let observed=f.db.pps_credit_readiness_snapshot(&f.e).await.unwrap();
        assert!(!observed.financial_halt);
        assert_eq!(observed.generation,before.generation);
        assert!(observed.funding.as_ref()==Some(&before));
        assert!(f.db.pps_funding_snapshot().await.unwrap()==before);
        // Legacy credits do not bump payout generation, but must still change
        // readiness's required backing; generation alone cannot report green.
        f.db.credit_balance(f.m,1).await.unwrap();
        let changed=f.db.pps_credit_readiness_snapshot(&f.e).await.unwrap();
        assert_eq!(changed.generation,before.generation);
        assert_eq!(changed.funding.unwrap().required_spendable_zatoshis,before.required_spendable_zatoshis+1);
        let mut wrong=f.e.clone(); wrong.id="another-epoch".into();
        assert!(matches!(f.db.pps_credit_readiness_snapshot(&wrong).await,Err(PpsDbError::EpochMismatch)));
    }
    #[tokio::test]
    async fn funding_absent_stale_wrong_network_or_policy_cannot_credit() {
        let f = setup(true, 10).await;
        let share = event(&f, 1, 1);
        assert!(matches!(
            f.db.credit_pps_share(&f.e, &share, Some(&f.l), None, NOW)
                .await,
            Err(PpsDbError::FundingLeaseRequired)
        ));
        for kind in 0..8 {
            let mut l = funded(&f).await;
            match kind {
                0 => l.valid_until_unix = NOW,
                1 => l.valid_until_unix = NOW + crate::pps_funding::FUNDING_LEASE_SECONDS + 1,
                2 => l.checked_at_unix = NOW + 1,
                3 => l.network = "mainnet".into(),
                4 => l.reserve_floor_zatoshis -= 1,
                5 => l.reserved_fee_allowance_zatoshis -= 1,
                6 => l.generation += 1,
                _ => l.spendable_zatoshis = 119,
            }
            assert!(f
                .db
                .credit_pps_share(&f.e, &share, Some(&f.l), Some(&l), NOW)
                .await
                .is_err());
        }
        assert_eq!(f.db.pps_invariant().await.unwrap().accepted_events, 0);
        assert_eq!(f.db.get_total_shares_count().await.unwrap(), 0);
    }
    #[tokio::test]
    async fn funding_rechecks_clock_before_commit_and_current_legacy_obligations() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let f = setup(true, 10).await;
        let mut l = funded(&f).await;
        l.valid_until_unix = NOW + 1;
        let calls = AtomicUsize::new(0);
        assert!(matches!(
            f.db.credit_pps_share_with_clock(
                &f.e,
                &event(&f, 1, 1),
                Some(&f.l),
                Some(&l),
                NOW,
                || Ok(NOW + calls.fetch_add(1, Ordering::SeqCst) as i64)
            )
            .await,
            Err(PpsDbError::FundingLeaseRequired)
        ));
        assert_eq!(f.db.pps_invariant().await.unwrap().accepted_events, 0);
        l = funded(&f).await;
        l.spendable_zatoshis = 120;
        // A new legacy liability after attestation is observed inside the credit
        // transaction, even though it has not changed wallet spend generation.
        f.db.credit_balance(f.m, 1).await.unwrap();
        assert!(matches!(
            f.db.credit_pps_share(&f.e, &event(&f, 1, 1), Some(&f.l), Some(&l), NOW)
                .await,
            Err(PpsDbError::FundingInsufficient)
        ));
    }
    #[tokio::test]
    async fn funding_generation_invalidates_all_legacy_wallet_outflow_lifecycles() {
        let f = setup(true, 10).await;
        f.db.credit_balance(f.m, 20).await.unwrap();
        let a = f.db.create_payout_attempt(1, 2, "legacy").await.unwrap();
        let l = funded(&f).await;
        f.db.reserve_payout(a, &[(f.m, 2)]).await.unwrap();
        assert!(f.db.check_pps_funding(&l).await.is_err());
        let l = funded(&f).await;
        f.db.refund_payout(a).await.unwrap();
        assert!(f.db.check_pps_funding(&l).await.is_err());
        let a = f.db.create_payout_attempt(1, 2, "legacy").await.unwrap();
        f.db.reserve_payout(a, &[(f.m, 2)]).await.unwrap();
        let l = funded(&f).await;
        f.db.confirm_payout(a, &"a".repeat(64)).await.unwrap();
        assert!(f.db.check_pps_funding(&l).await.is_err());
        let l = funded(&f).await;
        f.db.record_tx_cost("payout", "synthetic", 1).await.unwrap();
        assert!(f.db.check_pps_funding(&l).await.is_err());
        let l = funded(&f).await;
        f.db.create_payout(f.m, 1, &"b".repeat(64)).await.unwrap();
        assert!(f.db.check_pps_funding(&l).await.is_err());
    }
    #[tokio::test]
    async fn funding_generation_race_is_atomic_and_fees_never_replenish_on_payment_or_epoch() {
        let f = setup(true, 10).await;
        f.db.credit_pps_share(
            &f.e,
            &event(&f, 1, 3 * PPS_SCALE + 1),
            Some(&f.l),
            Some(&funded(&f).await),
            NOW,
        )
        .await
        .unwrap();
        let a = f.db.create_payout_attempt(1, 1, "pps").await.unwrap();
        let b = f.db.create_payout_attempt(1, 1, "pps").await.unwrap();
        let l = funded(&f).await;
        f.db.reserve_pps_payout(a, &[(f.m, 1)], &l, &fee(a))
            .await
            .unwrap();
        assert!(matches!(
            f.db.reserve_pps_payout(b, &[(f.m, 1)], &l, &fee(b)).await,
            Err(PpsDbError::FundingLeaseRequired)
        ));
        assert_eq!(f.db.pps_invariant().await.unwrap().paying_zatoshis, 1);
        signed(&f, a, &"c".repeat(64)).await;
        f.db.confirm_pps_payout(a, &"c".repeat(64)).await.unwrap();
        let s = f.db.pps_funding_snapshot().await.unwrap();
        assert_eq!((s.paid_fees_zatoshis, s.reserved_fees_zatoshis), (1, 0));
        assert_eq!(s.gross_subzatoshis, 3 * PPS_SCALE + 1);
        assert_eq!(s.required_spendable_zatoshis, 118);
        let mut e = f.e.clone();
        e.id = "next".into();
        f.db.initialize_pps_epoch(&e, Some(&funded(&f).await))
            .await
            .unwrap();
        assert_eq!(f.db.pps_funding_snapshot().await.unwrap(), s);
        for kind in 0..3 {
            let mut e = e.clone();
            match kind {
                0 => e.total_exposure_zatoshis += 1,
                1 => e.reserve_floor_zatoshis += 1,
                _ => {
                    e.fee_allowance_zatoshis += 1;
                    e.total_exposure_zatoshis += 1
                }
            }
            assert!(f
                .db
                .initialize_pps_epoch(&e, Some(&funded(&f).await))
                .await
                .is_err());
        }
    }
    #[tokio::test]
    async fn funding_fee_cap_and_exact_recipient_set_roll_back_all_reservations() {
        let f = setup(true, 10).await;
        f.db.credit_pps_share(
            &f.e,
            &event(&f, 1, 3 * PPS_SCALE + 1),
            Some(&f.l),
            Some(&funded(&f).await),
            NOW,
        )
        .await
        .unwrap();
        let a = f.db.create_payout_attempt(1, 1, "pps").await.unwrap();
        let l = funded(&f).await;
        for fee_zatoshis in [0, 100, 101, i64::MAX] {
            let q = PpsFeeReservation {
                proposal_id: format!("{a:064x}"),
                fee_zatoshis,
            };
            assert!(f
                .db
                .reserve_pps_payout(a, &[(f.m, 1)], &l, &q)
                .await
                .is_err());
            assert_eq!(f.db.pps_invariant().await.unwrap().paying_zatoshis, 0);
            assert_eq!(
                f.db.pps_funding_snapshot().await.unwrap().generation,
                l.generation
            );
        }
        assert!(f
            .db
            .reserve_pps_payout(a, &[(f.m, 4)], &l, &fee(a))
            .await
            .is_err());
        assert!(f
            .db
            .reserve_pps_payout(a, &[(f.m, 1), (f.m, 1)], &l, &fee(a))
            .await
            .is_err());
        assert!(f
            .db
            .reserve_pps_payout(a, &[(f.m, 1), (9999, 1)], &l, &fee(a))
            .await
            .is_err());
        assert_eq!(f.db.pps_invariant().await.unwrap().pending_zatoshis, 3);
    }
    #[tokio::test]
    async fn funding_final_fee_can_only_cover_all_claims_then_new_credits_stop() {
        let f = setup(true, 10).await;
        f.db.credit_pps_share(
            &f.e,
            &event(&f, 1, PPS_SCALE),
            Some(&f.l),
            Some(&funded(&f).await),
            NOW,
        )
        .await
        .unwrap();
        let a = f.db.create_payout_attempt(1, 1, "pps").await.unwrap();
        let q = PpsFeeReservation {
            proposal_id: format!("{a:064x}"),
            fee_zatoshis: 100,
        };
        f.db.reserve_pps_payout(a, &[(f.m, 1)], &funded(&f).await, &q)
            .await
            .unwrap();
        assert!(matches!(
            f.db.credit_pps_share(
                &f.e,
                &event(&f, 2, 1),
                Some(&f.l),
                Some(&funded(&f).await),
                NOW
            )
            .await,
            Err(PpsDbError::FeeBudgetExceeded)
        ));
        signed(&f, a, &"c".repeat(64)).await;
        f.db.confirm_pps_payout(a, &"c".repeat(64)).await.unwrap();
        assert_eq!(
            f.db.pps_funding_snapshot()
                .await
                .unwrap()
                .paid_fees_zatoshis,
            100
        );
        assert!(matches!(
            f.db.credit_pps_share(
                &f.e,
                &event(&f, 2, 1),
                Some(&f.l),
                Some(&funded(&f).await),
                NOW
            )
            .await,
            Err(PpsDbError::FeeBudgetExceeded)
        ));
    }
    #[tokio::test]
    async fn funding_signed_fence_parks_ambiguity_and_binds_expected_transaction() {
        let f = setup(true, 10).await;
        f.db.credit_pps_share(
            &f.e,
            &event(&f, 1, PPS_SCALE),
            Some(&f.l),
            Some(&funded(&f).await),
            NOW,
        )
        .await
        .unwrap();
        let a = f.db.create_payout_attempt(1, 1, "pps").await.unwrap();
        f.db.reserve_pps_payout(a, &[(f.m, 1)], &funded(&f).await, &fee(a))
            .await
            .unwrap();
        assert!(f
            .db
            .mark_pps_payout_signed(a, &fee(a).proposal_id, &"c".repeat(64))
            .await
            .is_err());
        assert!(f.db.seal_pps_payout(a, &"f".repeat(64)).await.is_err());
        f.db.seal_pps_payout(a, &fee(a).proposal_id).await.unwrap();
        assert!(f.db.refund_pps_payout(a).await.is_err());
        assert!(f.db.confirm_pps_payout(a, &"c".repeat(64)).await.is_err());
        f.db.mark_pps_payout_signed(a, &fee(a).proposal_id, &"c".repeat(64))
            .await
            .unwrap();
        assert!(f
            .db
            .mark_pps_payout_signed(a, &fee(a).proposal_id, &"d".repeat(64))
            .await
            .is_err());
        assert!(f.db.confirm_pps_payout(a, &"d".repeat(64)).await.is_err());
        assert_eq!(
            f.db.get_reserved_pps_attempts(None).await.unwrap()[0].3,
            Some("c".repeat(64))
        );
        f.db.confirm_pps_payout(a, &"c".repeat(64)).await.unwrap();
        assert!(f.db.refund_pps_payout(a).await.is_err());
        assert_eq!(
            f.db.pps_funding_snapshot()
                .await
                .unwrap()
                .paid_fees_zatoshis,
            1
        );
    }
    #[tokio::test]
    async fn funding_rejects_overflow_generation_exhaustion_and_fee_attribution_corruption() {
        let f = setup(true, 10).await;
        f.db.credit_balance(f.m, i64::MAX).await.unwrap();
        assert!(f.db.pps_funding_snapshot().await.is_err());
        sqlx::query("UPDATE balances SET pending=0 WHERE miner_id=?1")
            .bind(f.m)
            .execute(f.db.inner())
            .await
            .unwrap();
        f.db.credit_pps_share(
            &f.e,
            &event(&f, 1, PPS_SCALE),
            Some(&f.l),
            Some(&funded(&f).await),
            NOW,
        )
        .await
        .unwrap();
        let a = f.db.create_payout_attempt(1, 1, "pps").await.unwrap();
        sqlx::query("UPDATE pps_funding_generation SET generation=?1")
            .bind(i64::MAX)
            .execute(f.db.inner())
            .await
            .unwrap();
        assert!(f
            .db
            .reserve_pps_payout(a, &[(f.m, 1)], &funded(&f).await, &fee(a))
            .await
            .is_err());
        assert_eq!(f.db.pps_invariant().await.unwrap().paying_zatoshis, 0);
        sqlx::query("UPDATE pps_funding_generation SET generation=0")
            .execute(f.db.inner())
            .await
            .unwrap();
        f.db.reserve_pps_payout(a, &[(f.m, 1)], &funded(&f).await, &fee(a))
            .await
            .unwrap();
        sqlx::query("DELETE FROM pps_fee_reservations WHERE attempt_id=?1")
            .bind(a)
            .execute(f.db.inner())
            .await
            .unwrap();
        assert!(f.db.pps_invariant().await.is_err());
        assert!(f.db.pps_funding_snapshot().await.is_err());
    }
    #[tokio::test]
    async fn funding_pages_all_legacy_rows_and_rejects_unattributed_or_noninteger_money() {
        let f = setup(true, 10).await;
        let mut last = 0;
        for i in 0..300 {
            last =
                f.db.get_or_create_miner(&format!("fixture-{i}"))
                    .await
                    .unwrap()
                    .id;
            f.db.credit_balance(last, 1).await.unwrap();
        }
        let a = f.db.create_payout_attempt(1, 1, "legacy").await.unwrap();
        f.db.reserve_payout(a, &[(last, 1)]).await.unwrap();
        let s = f.db.pps_funding_snapshot().await.unwrap();
        assert_eq!(
            (
                s.legacy_pending_zatoshis,
                s.legacy_paying_zatoshis,
                s.required_spendable_zatoshis
            ),
            (299, 1, 420)
        );
        sqlx::query("UPDATE balances SET paying=0 WHERE miner_id=?1")
            .bind(last)
            .execute(f.db.inner())
            .await
            .unwrap();
        assert!(f.db.pps_funding_snapshot().await.is_err());
        sqlx::query("UPDATE balances SET paying=1,pending=0.5 WHERE miner_id=?1")
            .bind(last)
            .execute(f.db.inner())
            .await
            .unwrap();
        assert!(f.db.pps_funding_snapshot().await.is_err());
        sqlx::query("UPDATE balances SET pending=0 WHERE miner_id=?1")
            .bind(last)
            .execute(f.db.inner())
            .await
            .unwrap();
        sqlx::query("UPDATE payout_items SET amount=0.5 WHERE attempt_id=?1")
            .bind(a)
            .execute(f.db.inner())
            .await
            .unwrap();
        assert!(f.db.pps_funding_snapshot().await.is_err());
    }
    #[tokio::test]
    async fn funding_debug_redacts_every_financial_and_proposal_field() {
        let f = setup(true, 10).await;
        assert_eq!(
            format!("{:?}", funded(&f).await),
            "PpsFundingLease { redacted }"
        );
        assert_eq!(
            format!("{:?}", f.db.pps_funding_snapshot().await.unwrap()),
            "PpsFundingSnapshot { redacted }"
        );
        assert_eq!(format!("{:?}", fee(42)), "PpsFeeReservation { redacted }");
    }
    const CONVENTIONAL_BOUND: i64 = 5_000 * (1 + 2_000_000 / 352);
    fn conventional_intent(f: &Fixture, attempt: i64, amount: i64) -> PpsConventionalReservation {
        PpsConventionalReservation::from_intent(PpsConventionalIntent {
            version: 1, network: "testnet".into(), epoch: f.e.id.clone(),
            target_height: 3_000_000 + attempt as u64,
            source: "synthetic-source".into(), profile: "consensus-size-v1".into(),
            max_recipients: 100,
            items: vec![PpsConventionalRecipient {
                miner_id: f.m, address: "synthetic-miner".into(), amount_zatoshis: amount,
            }],
        }).unwrap()
    }
    async fn conventional_fixture() -> (Fixture, i64, PpsConventionalReservation) {
        conventional_fixture_from(setup(false, 10).await).await
    }
    async fn conventional_fixture_from(mut f: Fixture) -> (Fixture, i64, PpsConventionalReservation) {
        f.e.fee_allowance_zatoshis = CONVENTIONAL_BOUND * 4;
        f.e.total_exposure_zatoshis = f.e.max_liability_zatoshis + f.e.fee_allowance_zatoshis;
        f.db.initialize_pps_epoch(&f.e, Some(&funded(&f).await)).await.unwrap();
        f.db.credit_pps_share(
            &f.e,
            &event(&f, 1, 3 * PPS_SCALE),
            Some(&f.l),
            Some(&funded(&f).await),
            NOW,
        )
        .await
        .unwrap();
        let a =
            f.db.create_payout_attempt(1, 1, "conventional")
                .await
                .unwrap();
        let intent = conventional_intent(&f, a, 1);
        f.db.reserve_pps_conventional_payout(a, &[(f.m, 1)], &funded(&f).await, &intent)
            .await
            .unwrap();
        (f, a, intent)
    }
    async fn conventional_sent(f: &Fixture, a: i64, intent: &PpsConventionalReservation) {
        f.db.seal_pps_conventional_payout(a, &intent.intent_id)
            .await
            .unwrap();
        f.db.record_pps_conventional_operation(a, &intent.intent_id, "opid-fixture")
            .await
            .unwrap();
        f.db.record_pps_conventional_transaction(
            a,
            &intent.intent_id,
            "opid-fixture",
            &"c".repeat(64),
        )
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn conventional_funded_seal_rechecks_time_generation_and_current_claims() {
        for kind in 0..4 {
            let (f, a, intent) = conventional_fixture().await;
            let mut lease = funded(&f).await;
            match kind {
                0 => lease.valid_until_unix = NOW,
                1 => {
                    f.db.credit_balance(f.m, 1).await.unwrap();
                    let old = f.db.create_payout_attempt(1, 1, "legacy").await.unwrap();
                    f.db.check_pps_funding(&lease).await.unwrap();
                    f.db.reserve_payout(old, &[(f.m, 1)]).await.unwrap();
                }
                2 => {
                    lease.spendable_zatoshis = f.db.pps_funding_snapshot().await.unwrap().required_spendable_zatoshis;
                    f.db.check_pps_funding(&lease).await.unwrap();
                    f.db.credit_balance(f.m, 1).await.unwrap();
                }
                _ => lease.network = "mainnet".into(),
            }
            assert!(f.db.seal_pps_conventional_payout_funded(a, &intent.intent_id, &lease).await.is_err());
            assert!(!f.db.get_pps_conventional_attempt(a).await.unwrap().unwrap().sealed);
        }
        let (f, a, intent) = conventional_fixture().await;
        f.db.seal_pps_conventional_payout_funded(a, &intent.intent_id, &funded(&f).await).await.unwrap();
        assert!(f.db.get_pps_conventional_attempt(a).await.unwrap().unwrap().sealed);
        assert!(f.db.seal_pps_conventional_payout_funded(a, &intent.intent_id, &funded(&f).await).await.is_err());
    }
    #[tokio::test]
    async fn conventional_intent_encoding_profile_and_bound_are_strict() {
        let (f, _, original) = conventional_fixture().await;
        let valid = original.intent().unwrap();
        assert_eq!(format!("{valid:?}"), "PpsConventionalIntent { redacted }");
        for bad in 0..9 {
            let mut value = valid.clone();
            match bad {
                0 => value.version = 2,
                1 => value.network = "mainnet".into(),
                2 => value.target_height = 0,
                3 => value.profile = "unchecked".into(),
                4 => value.max_recipients = 101,
                5 => value.items[0].amount_zatoshis = 0,
                6 => value.items.push(value.items[0].clone()),
                7 => value.source = "bad source".into(),
                _ => value.items = vec![],
            }
            assert!(PpsConventionalReservation::from_intent(value).is_err());
        }
        let mut repeated = valid.clone();
        repeated.items.push(PpsConventionalRecipient {
            miner_id: f.m + 1, address: "synthetic-miner".into(), amount_zatoshis: 1,
        });
        assert_eq!(PpsConventionalReservation::from_intent(repeated.clone()).unwrap().fee_upper_bound_zatoshis, CONVENTIONAL_BOUND);
        repeated.items[1].address = "second-address".into();
        assert_eq!(PpsConventionalReservation::from_intent(repeated).unwrap().fee_upper_bound_zatoshis, CONVENTIONAL_BOUND + 5_000);
        let mut changed = original.clone();
        changed.canonical_intent.push(' ');
        assert!(changed.intent().is_err());
        changed = original;
        changed.fee_upper_bound_zatoshis -= 1;
        assert!(changed.intent().is_err());
    }
    #[tokio::test]
    async fn conventional_intent_mismatches_and_sql_failure_reserve_nothing() {
        let (f, _, _) = conventional_fixture().await;
        for bad in 0..4 {
            let a = f.db.create_payout_attempt(1, 1, "fixture").await.unwrap();
            let mut value = conventional_intent(&f, a, 1).intent().unwrap();
            match bad {
                0 => value.items[0].amount_zatoshis = 2,
                1 => value.items[0].address = "wrong-recipient".into(),
                2 => value.epoch = "wrong-epoch".into(),
                _ => { sqlx::raw_sql("CREATE TRIGGER reject_intent BEFORE INSERT ON pps_conventional_intents BEGIN SELECT RAISE(ABORT,'fixture'); END;").execute(f.db.inner()).await.unwrap(); }
            }
            let intent = PpsConventionalReservation::from_intent(value).unwrap();
            assert!(f.db.reserve_pps_conventional_payout(a, &[(f.m, 1)], &funded(&f).await, &intent).await.is_err());
            let n: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM pps_fee_reservations WHERE attempt_id=?1)+(SELECT COUNT(*) FROM pps_conventional_attempts WHERE attempt_id=?1)+(SELECT COUNT(*) FROM pps_payout_items WHERE attempt_id=?1)")
                .bind(a).fetch_one(f.db.inner()).await.unwrap();
            assert_eq!(n, 0);
        }
        assert_eq!(f.db.pps_invariant().await.unwrap().paying_zatoshis, 1);
    }
    #[tokio::test]
    async fn conventional_intent_survives_file_reopen_and_old_migration_stays_held() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("synthetic.sqlite");
        let options = sqlx::sqlite::SqliteConnectOptions::new().filename(&path)
            .create_if_missing(true).foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Full);
        let db = PoolDb::new(sqlx::sqlite::SqlitePoolOptions::new().max_connections(1)
            .connect_with(options.clone()).await.unwrap());
        let (f, a, original) = conventional_fixture_from(setup_db(db, false, 10).await).await;
        for statement in ["UPDATE pps_conventional_intents SET canonical_json='{}'", "DELETE FROM pps_conventional_intents"] {
            assert!(sqlx::query(statement).execute(f.db.inner()).await.is_err());
        }
        f.db.inner().close().await;
        let db = PoolDb::new(sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(options.clone()).await.unwrap());
        db.run_migrations().await.unwrap();
        let stored = db.get_pps_conventional_attempt(a).await.unwrap().unwrap();
        assert_eq!(stored.canonical_intent, Some(original.canonical_intent));
        assert!(stored.intent().unwrap().is_some());
        db.inner().close().await;
        let db = PoolDb::new(sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap());
        assert!(db.get_pps_conventional_attempt(a).await.unwrap().unwrap().intent().unwrap().is_some());
        // Synthetic pre-018 database: migration cannot invent its lost intent.
        sqlx::query("DROP TABLE pps_conventional_intents").execute(db.inner()).await.unwrap();
        db.run_migrations().await.unwrap();
        let old = db.get_pps_conventional_attempt(a).await.unwrap().unwrap();
        assert!(old.intent().unwrap().is_none());
        assert!(db.seal_pps_conventional_payout(a, &old.intent_id).await.is_err());
        assert!(db.pps_funding_snapshot().await.is_err());
        db.inner().close().await;
    }
    #[tokio::test]
    async fn conventional_terminal_states_never_reenter_generic_sagas() {
        for paid in [false, true] {
            let (f, a, intent) = conventional_fixture().await;
            assert!(f.db.get_reserved_pps_attempts(None).await.unwrap().is_empty());
            assert_eq!(f.db.get_reserved_pps_conventional_attempts(100).await.unwrap().len(), 1);
            if paid {
                conventional_sent(&f, a, &intent).await;
                f.db.confirm_pps_conventional_payout(a, &"c".repeat(64), 7).await.unwrap();
            } else { f.db.refund_pps_payout(a).await.unwrap(); }
            let status: String = sqlx::query_scalar("SELECT status FROM payout_attempts WHERE id=?1").bind(a).fetch_one(f.db.inner()).await.unwrap();
            assert_eq!(status, if paid { "confirmed" } else { "failed" });
            assert!(f.db.get_reserved_pps_conventional_attempts(100).await.unwrap().is_empty());
            // A historical stale shared row stays excluded after item removal.
            sqlx::query("UPDATE payout_attempts SET status='sent',created_at=datetime('now','-2 hours') WHERE id=?1").bind(a).execute(f.db.inner()).await.unwrap();
            assert!(f.db.get_stale_payout_attempts(1).await.unwrap().is_empty());
            assert!(f.db.reserve_payout(a, &[(f.m, 1)]).await.is_err());
            assert!(f.db.confirm_payout(a, &"c".repeat(64)).await.is_err());
            assert!(f.db.refund_payout(a).await.is_err());
            assert!(f.db.get_reserved_attempts(None).await.unwrap().is_empty());
        }
    }
    #[tokio::test]
    async fn conventional_contract_halt_is_permanent_global_and_preserves_claims() {
        let (f, a, intent) = conventional_fixture().await;
        assert!(f.db.halt_pps_conventional_payout(a, PpsConventionalHalt::RecipientMismatch).await.is_err());
        let b = f.db.create_payout_attempt(1, 1, "second").await.unwrap();
        f.db.reserve_pps_payout(b, &[(f.m, 1)], &funded(&f).await, &fee(b)).await.unwrap();
        conventional_sent(&f, a, &intent).await;
        f.db.halt_pps_conventional_payout(a, PpsConventionalHalt::RecipientMismatch).await.unwrap();
        f.db.halt_pps_conventional_payout(a, PpsConventionalHalt::RecipientMismatch).await.unwrap();
        assert!(f.db.halt_pps_conventional_payout(a, PpsConventionalHalt::FeeMismatch).await.is_err());
        assert!(f.db.pps_funding_snapshot().await.is_err());
        assert!(f.db.seal_pps_payout(b, &fee(b).proposal_id).await.is_err());
        assert!(f.db.credit_pps_share(&f.e, &event(&f, 2, 1), Some(&f.l), Some(&funded(&f).await), NOW).await.is_err());
        assert!(f.db.confirm_pps_conventional_payout(a, &"c".repeat(64), 7).await.is_err());
        assert!(f.db.refund_pps_payout(a).await.is_err());
        assert!(sqlx::query("DELETE FROM pps_conventional_halts").execute(f.db.inner()).await.is_err());
        assert!(sqlx::query("UPDATE pps_conventional_halts SET category='fee_mismatch'").execute(f.db.inner()).await.is_err());
        assert_eq!(f.db.get_pps_conventional_attempt(a).await.unwrap().unwrap().halt_category, Some(PpsConventionalHalt::RecipientMismatch));
        let state: (i64, i64, i64) = sqlx::query_as("SELECT pending,paying,paid FROM pps_accounts WHERE miner_id=?1").bind(f.m).fetch_one(f.db.inner()).await.unwrap();
        assert_eq!(state, (1, 2, 0));
        f.db.run_migrations().await.unwrap();
        assert!(f.db.pps_funding_snapshot().await.is_err());
    }
    #[tokio::test]
    async fn conventional_fee_bound_actual_and_principal_are_exactly_conserved() {
        let (f, a, intent) = conventional_fixture().await;
        assert_eq!(
            f.db.pps_funding_snapshot()
                .await
                .unwrap()
                .reserved_fees_zatoshis,
            CONVENTIONAL_BOUND
        );
        conventional_sent(&f, a, &intent).await;
        assert!(f.db.confirm_pps_payout(a, &"c".repeat(64)).await.is_err());
        assert_eq!(
            f.db.confirm_pps_conventional_payout(a, &"c".repeat(64), 7)
                .await
                .unwrap(),
            1
        );
        let s = f.db.pps_funding_snapshot().await.unwrap();
        assert_eq!(
            (
                s.paid_fees_zatoshis,
                s.reserved_fees_zatoshis,
                s.paid_zatoshis,
                s.required_spendable_zatoshis
            ),
            (7, 0, 1, 12 + CONVENTIONAL_BOUND * 4)
        );
        assert_eq!(s.gross_subzatoshis, 3 * PPS_SCALE);
        let receipt = f.db.get_pps_conventional_attempt(a).await.unwrap().unwrap();
        assert_eq!(receipt.fee_upper_bound_zatoshis, CONVENTIONAL_BOUND);
        assert_eq!(receipt.actual_fee_zatoshis, Some(7));
        assert_eq!(receipt.operation_id.as_deref(), Some("opid-fixture"));
        assert_eq!(receipt.expected_txid, Some("c".repeat(64)));
        assert_eq!(receipt.status, "paid");
        assert_eq!(
            format!("{receipt:?}"),
            "PpsConventionalAttempt { redacted }"
        );
        assert_eq!(
            format!("{intent:?}"),
            "PpsConventionalReservation { redacted }"
        );
        assert_eq!(
            f.db.confirm_pps_conventional_payout(a, &"c".repeat(64), 7)
                .await
                .unwrap(),
            0
        );
        assert!(matches!(
            f.db.confirm_pps_conventional_payout(a, &"c".repeat(64), 8)
                .await,
            Err(PpsDbError::DuplicateMismatch)
        ));
        assert!(f
            .db
            .confirm_pps_conventional_payout(a, &"d".repeat(64), 7)
            .await
            .is_err());
        assert!(f.db.refund_pps_payout(a).await.is_err());
        let mut next = f.e.clone();
        next.id = "next".into();
        f.db.initialize_pps_epoch(&next, Some(&funded(&f).await))
            .await
            .unwrap();
        assert_eq!(f.db.pps_funding_snapshot().await.unwrap(), s);
    }
    #[tokio::test]
    async fn conventional_zero_or_exact_bound_fee_is_recorded_without_rounding() {
        for actual in [0, CONVENTIONAL_BOUND] {
            let (f, a, intent) = conventional_fixture().await;
            conventional_sent(&f, a, &intent).await;
            f.db.confirm_pps_conventional_payout(a, &"c".repeat(64), actual)
                .await
                .unwrap();
            assert_eq!(
                f.db.pps_funding_snapshot()
                    .await
                    .unwrap()
                    .paid_fees_zatoshis,
                actual
            );
            assert_eq!(
                f.db.get_pps_conventional_attempt(a)
                    .await
                    .unwrap()
                    .unwrap()
                    .actual_fee_zatoshis,
                Some(actual)
            );
        }
    }
    #[tokio::test]
    async fn conventional_fee_overrun_halts_pps_durably_without_erasing_or_pausing_legacy() {
        let (f, a, intent) = conventional_fixture().await;
        f.db.credit_balance(f.m, 123).await.unwrap();
        conventional_sent(&f, a, &intent).await;
        assert!(matches!(
            f.db.confirm_pps_conventional_payout(a, &"c".repeat(64), CONVENTIONAL_BOUND + 1)
                .await,
            Err(PpsDbError::FeeBudgetExceeded)
        ));
        let receipt = f.db.get_pps_conventional_attempt(a).await.unwrap().unwrap();
        assert_eq!(
            (receipt.actual_fee_zatoshis, receipt.excess_fee_zatoshis),
            (None, Some(CONVENTIONAL_BOUND + 1))
        );
        assert_eq!(receipt.status, "reserved");
        assert!(f.db.pps_funding_snapshot().await.is_err());
        assert!(f
            .db
            .credit_pps_share(
                &f.e,
                &event(&f, 2, 1),
                Some(&f.l),
                Some(&funded(&f).await),
                NOW
            )
            .await
            .is_err());
        assert!(f.db.refund_pps_payout(a).await.is_err());
        assert!(f
            .db
            .confirm_pps_conventional_payout(a, &"c".repeat(64), 10)
            .await
            .is_err());
        let state: (i64, i64, i64) =
            sqlx::query_as("SELECT pending,paying,paid FROM pps_accounts WHERE miner_id=?1")
                .bind(f.m)
                .fetch_one(f.db.inner())
                .await
                .unwrap();
        assert_eq!(state, (2, 1, 0));
        // No silent legacy pause or debit: its independently authorized saga
        // remains available and cannot consume the parked PPS principal.
        let old = f.db.create_payout_attempt(1, 1, "legacy").await.unwrap();
        f.db.reserve_payout(old, &[(f.m, 1)]).await.unwrap();
        f.db.refund_payout(old).await.unwrap();
        assert_eq!(f.db.get_or_create_balance(f.m).await.unwrap().pending, 123);
        let reopened = PoolDb::new(f.db.inner().clone());
        assert_eq!(
            reopened
                .get_pps_conventional_attempt(a)
                .await
                .unwrap()
                .unwrap(),
            receipt
        );
        assert!(reopened.pps_invariant().await.is_err());
    }
    #[tokio::test]
    async fn conventional_overrun_blocks_new_seals_on_every_reserved_pps_mode() {
        for conventional in [false, true] {
            let (f, a, intent) = conventional_fixture().await;
            let b = f.db.create_payout_attempt(1, 1, "second").await.unwrap();
            let second = conventional_intent(&f, b, 1);
            if conventional {
                f.db.reserve_pps_conventional_payout(
                    b,
                    &[(f.m, 1)],
                    &funded(&f).await,
                    &second,
                )
                .await
                .unwrap();
            } else {
                f.db.reserve_pps_payout(b, &[(f.m, 1)], &funded(&f).await, &fee(b))
                    .await
                    .unwrap();
            }
            conventional_sent(&f, a, &intent).await;
            assert!(matches!(
                f.db.confirm_pps_conventional_payout(a, &"c".repeat(64), CONVENTIONAL_BOUND + 1)
                    .await,
                Err(PpsDbError::FeeBudgetExceeded)
            ));
            let result = if conventional {
                f.db.seal_pps_conventional_payout(b, &second.intent_id).await
            } else {
                f.db.seal_pps_payout(b, &fee(b).proposal_id).await
            };
            assert!(matches!(result, Err(PpsDbError::FeeBudgetExceeded)));
            let sealed: i64 = sqlx::query_scalar(
                "SELECT sealed FROM pps_fee_reservations WHERE attempt_id=?1",
            )
            .bind(b)
            .fetch_one(f.db.inner())
            .await
            .unwrap();
            assert_eq!(sealed, 0);
        }
    }
    #[tokio::test]
    async fn conventional_receipt_metadata_rejects_generic_updates_in_every_phase() {
        let (f, a, intent) = conventional_fixture().await;
        for phase in 0..3 {
            if phase == 1 {
                conventional_sent(&f, a, &intent).await;
            } else if phase == 2 {
                f.db.confirm_pps_conventional_payout(a, &"c".repeat(64), 7)
                    .await
                    .unwrap();
            }
            let before: (String, Option<String>, Option<String>, Option<String>) =
                sqlx::query_as("SELECT status,opid,txid,error_message FROM payout_attempts WHERE id=?1")
                    .bind(a)
                    .fetch_one(f.db.inner())
                    .await
                    .unwrap();
            assert!(f
                .db
                .update_payout_attempt(
                    a,
                    "failed",
                    Some("opid-other"),
                    Some(&"d".repeat(64)),
                    Some("synthetic"),
                )
                .await
                .is_err());
            let after: (String, Option<String>, Option<String>, Option<String>) =
                sqlx::query_as("SELECT status,opid,txid,error_message FROM payout_attempts WHERE id=?1")
                    .bind(a)
                    .fetch_one(f.db.inner())
                    .await
                    .unwrap();
            assert_eq!(before, after);
        }
        let legacy = f.db.create_payout_attempt(1, 1, "legacy").await.unwrap();
        f.db.update_payout_attempt(legacy, "sent", Some("legacy-op"), None, None)
            .await
            .unwrap();
        let status: String = sqlx::query_scalar("SELECT status FROM payout_attempts WHERE id=?1")
            .bind(legacy)
            .fetch_one(f.db.inner())
            .await
            .unwrap();
        assert_eq!(status, "sent");
    }
    #[tokio::test]
    async fn conventional_seal_is_one_shot_and_ambiguity_never_releases_claims() {
        let (f, a, intent) = conventional_fixture().await;
        assert!(f.db.seal_pps_payout(a, &intent.intent_id).await.is_err());
        assert!(f
            .db
            .mark_pps_payout_signed(a, &intent.intent_id, &"c".repeat(64))
            .await
            .is_err());
        assert!(f
            .db
            .record_pps_conventional_operation(a, &intent.intent_id, "opid-fixture")
            .await
            .is_err());
        f.db.seal_pps_conventional_payout(a, &intent.intent_id)
            .await
            .unwrap();
        assert!(f
            .db
            .seal_pps_conventional_payout(a, &intent.intent_id)
            .await
            .is_err());
        assert!(f.db.refund_pps_payout(a).await.is_err());
        assert!(f
            .db
            .confirm_pps_conventional_payout(a, &"c".repeat(64), 7)
            .await
            .is_err());
        f.db.record_pps_conventional_operation(a, &intent.intent_id, "opid-fixture")
            .await
            .unwrap();
        f.db.record_pps_conventional_operation(a, &intent.intent_id, "opid-fixture")
            .await
            .unwrap();
        assert!(f
            .db
            .record_pps_conventional_operation(a, &intent.intent_id, "opid-different")
            .await
            .is_err());
        assert!(f
            .db
            .record_pps_conventional_transaction(
                a,
                &intent.intent_id,
                "opid-different",
                &"c".repeat(64)
            )
            .await
            .is_err());
        f.db.record_pps_conventional_transaction(
            a,
            &intent.intent_id,
            "opid-fixture",
            &"c".repeat(64),
        )
        .await
        .unwrap();
        f.db.record_pps_conventional_transaction(
            a,
            &intent.intent_id,
            "opid-fixture",
            &"c".repeat(64),
        )
        .await
        .unwrap();
        assert!(f
            .db
            .record_pps_conventional_transaction(
                a,
                &intent.intent_id,
                "opid-fixture",
                &"d".repeat(64)
            )
            .await
            .is_err());
        let receipt = f.db.get_pps_conventional_attempt(a).await.unwrap().unwrap();
        let reopened = PoolDb::new(f.db.inner().clone());
        reopened.run_migrations().await.unwrap();
        assert_eq!(
            reopened
                .get_pps_conventional_attempt(a)
                .await
                .unwrap()
                .unwrap(),
            receipt
        );
        assert_eq!(reopened.pps_invariant().await.unwrap().paying_zatoshis, 1);
        assert_eq!(
            reopened
                .pps_funding_snapshot()
                .await
                .unwrap()
                .reserved_fees_zatoshis,
            CONVENTIONAL_BOUND
        );
    }
    #[tokio::test]
    async fn conventional_actual_fee_sql_failure_rolls_back_principal_and_receipt() {
        let (f, a, intent) = conventional_fixture().await;
        conventional_sent(&f, a, &intent).await;
        let before = f.db.pps_funding_snapshot().await.unwrap();
        sqlx::query("CREATE TRIGGER reject_actual BEFORE UPDATE OF actual_fee ON pps_conventional_attempts BEGIN SELECT RAISE(ABORT,'fixture'); END").execute(f.db.inner()).await.unwrap();
        assert!(f
            .db
            .confirm_pps_conventional_payout(a, &"c".repeat(64), 7)
            .await
            .is_err());
        assert_eq!(f.db.pps_funding_snapshot().await.unwrap(), before);
        assert_eq!(
            f.db.get_pps_conventional_attempt(a)
                .await
                .unwrap()
                .unwrap()
                .actual_fee_zatoshis,
            None
        );
        assert_eq!(f.db.pps_invariant().await.unwrap().paying_zatoshis, 1);
    }
    #[tokio::test]
    async fn conventional_mainnet_boundary_and_pczt_exact_fee_contract_stay_closed() {
        let mut f = setup(false, 10).await;
        f.e.network = "mainnet".into();
        f.l.network = "mainnet".into();
        f.db.initialize_pps_epoch(&f.e, Some(&funded(&f).await))
            .await
            .unwrap();
        f.db.credit_pps_share(
            &f.e,
            &event(&f, 1, PPS_SCALE),
            Some(&f.l),
            Some(&funded(&f).await),
            NOW,
        )
        .await
        .unwrap();
        let a = f.db.create_payout_attempt(1, 1, "fixture").await.unwrap();
        let intent = conventional_intent(&f, a, 1);
        assert!(f
            .db
            .reserve_pps_conventional_payout(a, &[(f.m, 1)], &funded(&f).await, &intent)
            .await
            .is_err());
        assert!(f
            .db
            .get_pps_conventional_attempt(a)
            .await
            .unwrap()
            .is_none());
        assert_eq!(f.db.pps_invariant().await.unwrap().paying_zatoshis, 0);
        f.db.reserve_pps_payout(a, &[(f.m, 1)], &funded(&f).await, &fee(a))
            .await
            .unwrap();
        assert!(f
            .db
            .seal_pps_conventional_payout(a, &intent.intent_id)
            .await
            .is_err());
        signed(&f, a, &"c".repeat(64)).await;
        assert!(f
            .db
            .confirm_pps_conventional_payout(a, &"c".repeat(64), 1)
            .await
            .is_err());
        f.db.confirm_pps_payout(a, &"c".repeat(64)).await.unwrap();
        assert_eq!(
            f.db.pps_funding_snapshot()
                .await
                .unwrap()
                .paid_fees_zatoshis,
            1
        );
    }
    #[tokio::test]
    async fn conventional_unsealed_failure_can_release_only_unused_bound() {
        let (f, a, _) = conventional_fixture().await;
        f.db.refund_pps_payout(a).await.unwrap();
        let receipt = f.db.get_pps_conventional_attempt(a).await.unwrap().unwrap();
        assert_eq!(receipt.status, "released");
        assert!(!receipt.sealed);
        assert_eq!(receipt.actual_fee_zatoshis, None);
        let s = f.db.pps_funding_snapshot().await.unwrap();
        assert_eq!((s.paid_fees_zatoshis, s.reserved_fees_zatoshis), (0, 0));
        assert_eq!(s.gross_subzatoshis, 3 * PPS_SCALE);
        assert_eq!(f.db.pps_invariant().await.unwrap().pending_zatoshis, 3);
    }
    fn event(f: &Fixture, index: u64, amount: u128) -> PpsCredit {
        PpsCredit {
            proof_id: format!("{index:064x}"),
            quote_id: format!("{:064x}", amount),
            worker_id: f.w,
            job_id: "fixture-job".into(),
            session_id: "fixture-session".into(),
            difficulty: 1.0,
            is_block: false,
            amount_subzatoshis: amount,
            accepted_at_unix: NOW,
            quote_height: 3_000_000,
            network_target_be: [1; 32],
            assigned_share_target_be: [2; 32],
            miner_subsidy_zats: 125_000_000,
        }
    }

    async fn extension_fixture() -> Fixture {
        extension_fixture_from(setup(false, 950_000_000).await).await
    }
    async fn extension_fixture_from(mut f: Fixture) -> Fixture {
        f.e.fee_allowance_zatoshis = 50_000_000;
        f.e.total_exposure_zatoshis = 1_000_000_000;
        let mut lease = funded(&f).await;
        lease.spendable_zatoshis = 100_000_000_100;
        f.db.initialize_pps_epoch(&f.e, Some(&lease)).await.unwrap();
        f.db.credit_pps_share(&f.e, &event(&f, 1, 3 * PPS_SCALE + 7),
            Some(&f.l), Some(&lease), NOW).await.unwrap();
        f
    }
    async fn extension_lease(f: &Fixture) -> PpsFundingLease {
        let projected = f.db.pps_testnet_budget_extension_snapshot(&f.e).await.unwrap();
        PpsFundingLease { network:"testnet".into(), checked_at_unix:NOW,
            valid_until_unix:NOW+60, spendable_zatoshis:projected.required_spendable_zatoshis,
            reserve_floor_zatoshis:projected.reserve_floor_zatoshis,
            reserved_fee_allowance_zatoshis:projected.fee_allowance_zatoshis,
            generation:projected.generation }
    }
    // Complete, typed row representation for synthetic preservation assertions.
    async fn extension_history(f: &Fixture) -> Vec<Vec<Vec<String>>> {
        use sqlx::{Column, TypeInfo, ValueRef};
        let mut all = Vec::new();
        for table in ["pps_epochs","pps_accounts","pps_quotes","pps_events","pps_payouts",
            "pps_payout_items","pps_fee_reservations","pps_conventional_attempts",
            "pps_conventional_intents","payout_attempts","payouts","payout_items",
            "balances","shares","miners","workers","blocks","block_submissions"] {
            let filter = if table == "pps_epochs" { " WHERE id<>'testnet-pps-1000-20260907'" } else { "" };
            let rows = sqlx::query(&format!("SELECT * FROM {table}{filter} ORDER BY rowid"))
                .fetch_all(f.db.inner()).await.unwrap();
            let mut encoded = Vec::new();
            for row in rows {
                let mut fields = Vec::new();
                for column in row.columns() {
                    let index = column.ordinal();
                    let raw = row.try_get_raw(index).unwrap();
                    let kind = raw.type_info().name().to_owned();
                    let value = if raw.is_null() { "null".into() } else { match kind.as_str() {
                        "INTEGER" => row.try_get::<i64,_>(index).unwrap().to_string(),
                        "REAL" => row.try_get::<f64,_>(index).unwrap().to_bits().to_string(),
                        "BLOB" => format!("{:?}", row.try_get::<Vec<u8>,_>(index).unwrap()),
                        "TEXT" => row.try_get::<String,_>(index).unwrap(),
                        _ => panic!("unsupported synthetic field type"),
                    }};
                    fields.push(format!("{kind}:{}:{value}", value.len()));
                }
                encoded.push(fields);
            }
            all.push(encoded);
        }
        all
    }
    #[tokio::test]
    async fn exact_budget_extension_preserves_paid_history_and_never_refills() {
        let f = extension_fixture().await;
        f.db.credit_balance(f.m, 17).await.unwrap();
        let attempt = f.db.create_payout_attempt(1, 1, "conventional").await.unwrap();
        let intent = conventional_intent(&f, attempt, 1);
        let mut old_lease = funded(&f).await; old_lease.spendable_zatoshis=100_000_000_100;
        f.db.reserve_pps_conventional_payout(attempt,&[(f.m,1)],&old_lease,&intent).await.unwrap();
        conventional_sent(&f,attempt,&intent).await;
        f.db.confirm_pps_conventional_payout(attempt,&"c".repeat(64),7).await.unwrap();
        let before = f.db.pps_invariant().await.unwrap();
        let history = extension_history(&f).await;
        let lease = extension_lease(&f).await;
        assert_eq!(lease.spendable_zatoshis,100_000_000_000 - 1 - 7 + 17 + 10);
        assert!(f.db.verify_pps_epoch(&crate::pps_funding::testnet_budget_extension_epoch(&f.e).unwrap()).await.is_err());
        f.db.extend_testnet_pps_budget(&f.e,&lease).await.unwrap();
        assert_eq!(extension_history(&f).await,history);
        let after = f.db.pps_invariant().await.unwrap();
        assert_eq!((after.gross_subzatoshis,after.accepted_events,after.paid_zatoshis,
            after.pending_zatoshis,after.fractional_subzatoshis),
            (before.gross_subzatoshis,before.accepted_events,before.paid_zatoshis,
            before.pending_zatoshis,before.fractional_subzatoshis));
        let current = f.db.pps_funding_snapshot().await.unwrap();
        assert_eq!((current.paid_fees_zatoshis,current.generation),(7,lease.generation+1));
        assert!(f.db.check_pps_funding(&lease).await.is_err());
        assert!(f.db.extend_testnet_pps_budget(&f.e,&lease).await.is_err());
        assert!(f.db.pps_testnet_budget_extension_snapshot(&f.e).await.is_err());
        f.db.run_migrations().await.unwrap();
        assert!(f.db.extend_testnet_pps_budget(&f.e,&lease).await.is_err());
        assert!(sqlx::query("UPDATE pps_budget_extensions SET paid_fees=0").execute(f.db.inner()).await.is_err());
        assert!(sqlx::query("DELETE FROM pps_budget_extensions").execute(f.db.inner()).await.is_err());
        assert_eq!(f.db.pps_funding_snapshot().await.unwrap(),current);
    }
    #[tokio::test]
    async fn budget_extension_rejects_wrong_policy_stale_or_insufficient_proof_atomically() {
        for variant in 0..10 {
            let f=extension_fixture().await;
            let before=f.db.pps_funding_snapshot().await.unwrap();
            let history=extension_history(&f).await;
            let mut previous=f.e.clone(); let mut lease=extension_lease(&f).await;
            match variant {
                0=>previous.network="mainnet".into(),
                1=>previous.id="wrong-epoch".into(),
                2=>previous.max_liability_zatoshis+=1,
                3=>previous.fee_allowance_zatoshis+=1,
                4=>previous.fee_bps+=1,
                5=>lease.spendable_zatoshis-=1,
                6=>lease.generation+=1,
                7=>lease.valid_until_unix=NOW,
                8=>lease.valid_until_unix=NOW+crate::pps_funding::FUNDING_LEASE_SECONDS+1,
                _=>lease.reserved_fee_allowance_zatoshis=50_000_000,
            }
            assert!(f.db.extend_testnet_pps_budget(&previous,&lease).await.is_err());
            assert_eq!(f.db.pps_funding_snapshot().await.unwrap(),before);
            assert_eq!(extension_history(&f).await,history);
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM pps_budget_extensions").fetch_one(f.db.inner()).await.unwrap(),0);
        }
    }
    #[tokio::test]
    async fn budget_extension_sql_failure_or_precommit_expiry_rolls_back_everything() {
        for sql_failure in [false,true] {
            let f=extension_fixture().await;
            let lease=extension_lease(&f).await;
            let before=f.db.pps_funding_snapshot().await.unwrap();
            let history=extension_history(&f).await;
            if sql_failure {
                sqlx::query("CREATE TRIGGER reject_extension BEFORE UPDATE ON pps_funding_generation BEGIN SELECT RAISE(ABORT,'synthetic'); END").execute(f.db.inner()).await.unwrap();
            }
            let calls=std::cell::Cell::new(0);
            assert!(f.db.extend_testnet_pps_budget_with_clock(&f.e,&lease,||{
                let n=calls.get(); calls.set(n+1); Ok(if n>=2 {NOW+60} else {NOW})
            }).await.is_err());
            assert_eq!(f.db.pps_funding_snapshot().await.unwrap(),before);
            assert_eq!(extension_history(&f).await,history);
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM pps_budget_extensions").fetch_one(f.db.inner()).await.unwrap(),0);
        }
    }
    #[tokio::test]
    async fn budget_extension_rejects_unresolved_attempts_even_without_reservations() {
        for status in ["queued","submitting","sent","unknown"] {
            let f=extension_fixture().await;
            let lease=extension_lease(&f).await;
            let a=f.db.create_payout_attempt(0,0,"synthetic").await.unwrap();
            sqlx::query("UPDATE payout_attempts SET status=?1 WHERE id=?2").bind(status).bind(a).execute(f.db.inner()).await.unwrap();
            let history=extension_history(&f).await;
            assert!(f.db.pps_testnet_budget_extension_snapshot(&f.e).await.is_err());
            assert!(f.db.extend_testnet_pps_budget(&f.e,&lease).await.is_err());
            assert_eq!(extension_history(&f).await,history);
        }
    }
    #[tokio::test]
    async fn budget_extension_predecessor_schema_projection_is_read_only_and_schema_rolls_back() {
        let f=extension_fixture().await;
        sqlx::query("DROP TABLE pps_budget_extensions").execute(f.db.inner()).await.unwrap();
        let mut lease=extension_lease(&f).await;
        let exists=|| sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM sqlite_master WHERE name='pps_budget_extensions'");
        assert_eq!(exists().fetch_one(f.db.inner()).await.unwrap(),0);
        lease.spendable_zatoshis-=1;
        assert!(f.db.extend_testnet_pps_budget(&f.e,&lease).await.is_err());
        assert_eq!(exists().fetch_one(f.db.inner()).await.unwrap(),0);
        lease.spendable_zatoshis+=1;
        f.db.extend_testnet_pps_budget(&f.e,&lease).await.unwrap();
        assert_eq!(exists().fetch_one(f.db.inner()).await.unwrap(),1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM pps_budget_extensions").fetch_one(f.db.inner()).await.unwrap(),1);
    }
    #[tokio::test]
    async fn budget_extension_partial_policy_or_journal_is_not_repaired() {
        for variant in 0..4 {
            let f=extension_fixture().await;
            let lease=extension_lease(&f).await;
            match variant {
                0=>{sqlx::query("DROP TRIGGER pps_budget_extensions_no_delete").execute(f.db.inner()).await.unwrap();},
                1=>{sqlx::query("UPDATE pps_funding_policy SET fee_cap=5000000000").execute(f.db.inner()).await.unwrap();},
                2=>{sqlx::query("UPDATE pps_meta SET cap_zats=95000000000").execute(f.db.inner()).await.unwrap();},
                _=>{sqlx::query("INSERT INTO pps_epochs SELECT 'testnet-pps-1000-20260907',network,fee_bps,95000000000,quote_provenance FROM pps_epochs LIMIT 1").execute(f.db.inner()).await.unwrap();},
            }
            let history=extension_history(&f).await;
            assert!(f.db.pps_testnet_budget_extension_snapshot(&f.e).await.is_err());
            assert!(f.db.extend_testnet_pps_budget(&f.e,&lease).await.is_err());
            assert_eq!(extension_history(&f).await,history);
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM pps_budget_extensions").fetch_one(f.db.inner()).await.unwrap(),0);
            if variant==0 {
                assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM sqlite_master WHERE name='pps_budget_extensions_no_delete'").fetch_one(f.db.inner()).await.unwrap(),0);
            }
        }
    }
    #[tokio::test]
    async fn budget_extension_real_file_reopen_preserves_epoch_journal_and_rejects_replay() {
        let directory=tempfile::tempdir().unwrap();
        let path=directory.path().join("synthetic-extension.sqlite");
        let options=sqlx::sqlite::SqliteConnectOptions::new().filename(&path).create_if_missing(true);
        let pool=sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(options.clone()).await.unwrap();
        let mut f=extension_fixture_from(setup_db(PoolDb::new(pool),false,950_000_000).await).await;
        let lease=extension_lease(&f).await;
        let history=extension_history(&f).await;
        f.db.extend_testnet_pps_budget(&f.e,&lease).await.unwrap();
        let state=f.db.pps_funding_snapshot().await.unwrap();
        f.db.inner().close().await;
        let pool=sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(options.create_if_missing(false)).await.unwrap();
        f.db=PoolDb::new(pool);
        f.db.run_migrations().await.unwrap();
        assert_eq!(extension_history(&f).await,history);
        assert_eq!(f.db.pps_funding_snapshot().await.unwrap(),state);
        assert!(f.db.verify_pps_epoch(&crate::pps_funding::testnet_budget_extension_epoch(&f.e).unwrap()).await.is_ok());
        assert!(f.db.extend_testnet_pps_budget(&f.e,&lease).await.is_err());
        assert!(f.db.check_pps_funding(&lease).await.is_err());
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM pps_budget_extensions").fetch_one(f.db.inner()).await.unwrap(),1);
        f.db.inner().close().await;
    }

    #[tokio::test]
    async fn live_credit_carry_is_atomic_and_separate_from_legacy() {
        let f = setup(true, 10).await;
        f.db.credit_balance(f.m, 50).await.unwrap();
        f.db.credit_pps_share(
            &f.e,
            &event(&f, 1, PPS_SCALE - 1),
            Some(&f.l),
            Some(&funded(&f).await),
            NOW,
        )
        .await
        .unwrap();
        f.db.credit_pps_share(
            &f.e,
            &event(&f, 2, 2),
            Some(&f.l),
            Some(&funded(&f).await),
            NOW,
        )
        .await
        .unwrap();
        let s = f.db.pps_invariant().await.unwrap();
        assert_eq!(s.accepted_events, 2);
        assert_eq!(s.gross_subzatoshis, PPS_SCALE + 1);
        assert_eq!(s.pending_zatoshis, 1);
        assert_eq!(s.fractional_subzatoshis, 1);
        assert_eq!(f.db.get_or_create_balance(f.m).await.unwrap().pending, 50);
        assert_eq!(f.db.get_total_shares_count().await.unwrap(), 2);
    }
    #[tokio::test]
    async fn duplicate_is_canonical_not_session_or_time_dependent() {
        let f = setup(true, 10).await;
        let e = event(&f, 1, PPS_SCALE);
        f.db.credit_pps_share(&f.e, &e, Some(&f.l), Some(&funded(&f).await), NOW)
            .await
            .unwrap();
        let mut retry = e.clone();
        retry.worker_id =
            f.db.get_or_create_worker(f.m, "new-worker")
                .await
                .unwrap()
                .id;
        retry.job_id = "new-job".into();
        retry.session_id = "new-session".into();
        retry.accepted_at_unix = NOW + 1000;
        retry.difficulty = 2.0;
        assert!(
            f.db.credit_pps_share(&f.e, &retry, None, None, NOW + 1000)
                .await
                .unwrap()
                .duplicate
        );
        retry.quote_id = "a".repeat(64);
        assert!(matches!(
            f.db.credit_pps_share(&f.e, &retry, None, None, NOW + 1000)
                .await,
            Err(PpsDbError::DuplicateMismatch)
        ));
        retry = e;
        let other =
            f.db.get_or_create_miner("synthetic-other")
                .await
                .unwrap()
                .id;
        retry.worker_id =
            f.db.get_or_create_worker(other, "other-worker")
                .await
                .unwrap()
                .id;
        assert!(matches!(
            f.db.credit_pps_share(&f.e, &retry, None, None, NOW).await,
            Err(PpsDbError::DuplicateMismatch)
        ));
        assert_eq!(f.db.get_total_shares_count().await.unwrap(), 1);
    }
    #[tokio::test]
    async fn stale_forked_and_missing_chain_proof_never_accept_a_share() {
        let f = setup(true, 10).await;
        let s = event(&f, 1, 1);
        assert!(matches!(
            f.db.credit_pps_share(&f.e, &s, None, None, NOW).await,
            Err(PpsDbError::ChainLeaseRequired)
        ));
        let mut bad = f.l.clone();
        bad.disagreement = true;
        assert!(matches!(
            f.db.credit_pps_share(&f.e, &s, Some(&bad), Some(&funded(&f).await), NOW)
                .await,
            Err(PpsDbError::ChainLeaseRequired)
        ));
        assert!(matches!(
            f.db.credit_pps_share(&f.e, &s, Some(&f.l), Some(&funded(&f).await), NOW + 61)
                .await,
            Err(PpsDbError::ChainLeaseRequired)
        ));
        assert_eq!(f.db.get_total_shares_count().await.unwrap(), 0);
        assert_eq!(f.db.pps_invariant().await.unwrap().accepted_events, 0);
    }
    #[tokio::test]
    async fn concurrent_cap_includes_fractional_claims_and_survives_epoch_change() {
        let f = setup(true, 1).await;
        let a = event(&f, 1, 600_000_000_000);
        let b = event(&f, 2, 600_000_000_000);
        let funding = funded(&f).await;
        let (a, b) = tokio::join!(
            f.db.credit_pps_share(&f.e, &a, Some(&f.l), Some(&funding), NOW),
            f.db.credit_pps_share(&f.e, &b, Some(&f.l), Some(&funding), NOW)
        );
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        assert!(
            matches!(a, Err(PpsDbError::CapExceeded)) || matches!(b, Err(PpsDbError::CapExceeded))
        );
        let mut next = f.e.clone();
        next.id = "epoch-two".into();
        next.fee_bps = 200;
        f.db.initialize_pps_epoch(&next, Some(&funded(&f).await))
            .await
            .unwrap();
        assert!(matches!(
            f.db.credit_pps_share(
                &next,
                &event(&f, 3, 500_000_000_000),
                Some(&f.l),
                Some(&funded(&f).await),
                NOW
            )
            .await,
            Err(PpsDbError::CapExceeded)
        ));
        next.max_liability_zatoshis = 2;
        next.total_exposure_zatoshis = 102;
        assert!(matches!(
            f.db.initialize_pps_epoch(&next, Some(&funded(&f).await))
                .await,
            Err(PpsDbError::EpochMismatch)
        ));
        assert_eq!(f.db.pps_invariant().await.unwrap().accepted_events, 1);
    }
    #[tokio::test]
    async fn saga_preserves_legacy_and_gross_cap_after_confirmation_and_refund() {
        let f = setup(true, 4).await;
        f.db.credit_balance(f.m, 100).await.unwrap();
        f.db.credit_pps_share(
            &f.e,
            &event(&f, 1, 3 * PPS_SCALE + PPS_SCALE / 2),
            Some(&f.l),
            Some(&funded(&f).await),
            NOW,
        )
        .await
        .unwrap();
        let attempt =
            f.db.create_payout_attempt(1, 2, "fixture-pps")
                .await
                .unwrap();
        assert_eq!(
            f.db.reserve_pps_payout(attempt, &[(f.m, 2)], &funded(&f).await, &fee(attempt))
                .await
                .unwrap(),
            vec![(f.m, 2)]
        );
        let s = f.db.pps_invariant().await.unwrap();
        assert_eq!(
            (s.pending_zatoshis, s.paying_zatoshis, s.paid_zatoshis),
            (1, 2, 0)
        );
        assert_eq!(f.db.get_reserved_pps_attempts(None).await.unwrap().len(), 1);
        let txid = "c".repeat(64);
        signed(&f, attempt, &txid).await;
        assert_eq!(f.db.confirm_pps_payout(attempt, &txid).await.unwrap(), 1);
        assert_eq!(f.db.confirm_pps_payout(attempt, &txid).await.unwrap(), 0);
        assert!(f.db.refund_pps_payout(attempt).await.is_err());
        assert!(f
            .db
            .confirm_pps_payout(attempt, &"d".repeat(64))
            .await
            .is_err());
        assert_eq!(f.db.void_reorged_payout(&txid).await.unwrap(), (0, 0));
        assert_eq!(f.db.get_or_create_balance(f.m).await.unwrap().pending, 100);
        let attempt =
            f.db.create_payout_attempt(1, 1, "fixture-pps")
                .await
                .unwrap();
        f.db.reserve_pps_payout(attempt, &[(f.m, 1)], &funded(&f).await, &fee(attempt))
            .await
            .unwrap();
        assert_eq!(f.db.refund_pps_payout(attempt).await.unwrap(), 1);
        assert_eq!(f.db.refund_pps_payout(attempt).await.unwrap(), 0);
        let s = f.db.pps_invariant().await.unwrap();
        assert_eq!(
            (s.pending_zatoshis, s.paying_zatoshis, s.paid_zatoshis),
            (1, 0, 2)
        );
        assert_eq!(s.gross_subzatoshis, 3 * PPS_SCALE + PPS_SCALE / 2);
        assert!(matches!(
            f.db.credit_pps_share(
                &f.e,
                &event(&f, 2, PPS_SCALE),
                Some(&f.l),
                Some(&funded(&f).await),
                NOW
            )
            .await,
            Err(PpsDbError::CapExceeded)
        ));
    }
    #[tokio::test]
    async fn legacy_orphans_cannot_debit_pps_and_pps_blocks_never_get_pplns() {
        let f = setup(false, 10).await;
        let old =
            f.db.record_block(1, &"a".repeat(64), 60, Some(60), f.w, None)
                .await
                .unwrap();
        f.db.distribute_block_credits(old, &[(f.m, 60)])
            .await
            .unwrap();
        f.db.initialize_pps_epoch(&f.e, Some(&funded(&f).await))
            .await
            .unwrap();
        f.db.credit_pps_share(
            &f.e,
            &event(&f, 1, 3 * PPS_SCALE),
            Some(&f.l),
            Some(&funded(&f).await),
            NOW,
        )
        .await
        .unwrap();
        let before = f.db.pps_invariant().await.unwrap();
        f.db.orphan_block(old, 60).await.unwrap();
        assert_eq!(f.db.get_or_create_balance(f.m).await.unwrap().pending, 0);
        assert_eq!(f.db.pps_invariant().await.unwrap(), before);
        f.db.credit_balance(f.m, 40).await.unwrap();
        let current =
            f.db.record_block(2, &"b".repeat(64), 60, Some(60), f.w, None)
                .await
                .unwrap();
        assert!(f
            .db
            .distribute_block_credits(current, &[(f.m, 60)])
            .await
            .is_err());
        assert!(f
            .db
            .get_recent_blocks_missing_credits(30)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(f.db.orphan_block(current, 60).await.unwrap(), Some((0, 0)));
        assert_eq!(f.db.get_or_create_balance(f.m).await.unwrap().pending, 40);
        assert_eq!(f.db.pps_invariant().await.unwrap(), before);
        let submission =
            f.db.record_block_submission(3, &"c".repeat(64), f.w, 60, None)
                .await
                .unwrap();
        let marked: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pps_submission_markers WHERE submission_id=?1",
        )
        .bind(submission)
        .fetch_one(f.db.inner())
        .await
        .unwrap();
        assert_eq!(marked, 1);
        assert!(f.db.assert_reward_mode(false).await.is_err());
        assert!(f.db.assert_reward_mode(true).await.is_ok());
    }
    #[tokio::test]
    async fn activation_rejects_legacy_unresolved_work_and_preserves_reservations() {
        let f = setup(false, 10).await;
        let submission =
            f.db.record_block_submission(1, &"c".repeat(64), f.w, 60, None)
                .await
                .unwrap();
        assert!(matches!(
            f.db.initialize_pps_epoch(&f.e, Some(&funded(&f).await))
                .await,
            Err(PpsDbError::LegacyRecoveryRequired)
        ));
        f.db.resolve_block_submission(submission, "rejected")
            .await
            .unwrap();
        f.db.credit_balance(f.m, 100).await.unwrap();
        let old =
            f.db.create_payout_attempt(1, 50, "fixture-legacy")
                .await
                .unwrap();
        f.db.reserve_payout(old, &[(f.m, 50)]).await.unwrap();
        f.db.initialize_pps_epoch(&f.e, Some(&funded(&f).await))
            .await
            .unwrap();
        assert_eq!(f.db.get_reserved_attempts(None).await.unwrap().len(), 1);
        assert!(f
            .db
            .get_reserved_pps_attempts(None)
            .await
            .unwrap()
            .is_empty());
        f.db.refund_payout(old).await.unwrap();
        assert_eq!(f.db.get_or_create_balance(f.m).await.unwrap().pending, 100);
    }
    #[tokio::test]
    async fn sql_failure_rolls_back_share_acceptance_and_all_credits() {
        let f = setup(true, 10).await;
        sqlx::query("CREATE TRIGGER synthetic_failure BEFORE INSERT ON pps_events BEGIN SELECT RAISE(ABORT,'fixture'); END").execute(f.db.inner()).await.unwrap();
        assert!(f
            .db
            .credit_pps_share(
                &f.e,
                &event(&f, 1, PPS_SCALE),
                Some(&f.l),
                Some(&funded(&f).await),
                NOW
            )
            .await
            .is_err());
        assert_eq!(f.db.get_total_shares_count().await.unwrap(), 0);
        assert_eq!(f.db.pps_invariant().await.unwrap().gross_subzatoshis, 0);
    }
    #[tokio::test]
    async fn restart_or_send_refuses_schema_valid_counter_and_attribution_corruption() {
        let f = setup(true, 10).await;
        f.db.credit_pps_share(
            &f.e,
            &event(&f, 1, PPS_SCALE),
            Some(&f.l),
            Some(&funded(&f).await),
            NOW,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE pps_meta SET gross_whole=0")
            .execute(f.db.inner())
            .await
            .unwrap();
        assert!(matches!(
            f.db.initialize_pps_epoch(&f.e, Some(&funded(&f).await))
                .await,
            Err(PpsDbError::Invariant)
        ));
        assert!(f.db.pps_invariant().await.is_err());
        let attempt =
            f.db.create_payout_attempt(1, 1, "fixture-pps")
                .await
                .unwrap();
        assert!(f
            .db
            .reserve_pps_payout(attempt, &[(f.m, 1)], &funded(&f).await, &fee(attempt))
            .await
            .is_err());
    }
    #[tokio::test]
    async fn legacy_stale_sweep_does_not_consume_pps_reservations() {
        let f = setup(true, 10).await;
        f.db.credit_pps_share(
            &f.e,
            &event(&f, 1, PPS_SCALE),
            Some(&f.l),
            Some(&funded(&f).await),
            NOW,
        )
        .await
        .unwrap();
        let attempt =
            f.db.create_payout_attempt(1, 1, "fixture-pps")
                .await
                .unwrap();
        f.db.reserve_pps_payout(attempt, &[(f.m, 1)], &funded(&f).await, &fee(attempt))
            .await
            .unwrap();
        sqlx::query(
            "UPDATE payout_attempts SET created_at=datetime('now','-120 minutes') WHERE id=?1",
        )
        .bind(attempt)
        .execute(f.db.inner())
        .await
        .unwrap();
        assert!(f.db.get_stale_payout_attempts(60).await.unwrap().is_empty());
        assert_eq!(
            f.db.get_reserved_pps_attempts(Some(60))
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn wrong_saga_methods_refuse_cross_ledger_reservations() {
        let f = setup(true, 10).await;
        f.db.credit_balance(f.m, 10).await.unwrap();
        f.db.credit_pps_share(
            &f.e,
            &event(&f, 1, 2 * PPS_SCALE),
            Some(&f.l),
            Some(&funded(&f).await),
            NOW,
        )
        .await
        .unwrap();
        let pps =
            f.db.create_payout_attempt(1, 1, "fixture-pps")
                .await
                .unwrap();
        f.db.reserve_pps_payout(pps, &[(f.m, 1)], &funded(&f).await, &fee(pps))
            .await
            .unwrap();
        assert!(f.db.reserve_payout(pps, &[(f.m, 1)]).await.is_err());
        assert!(f.db.confirm_payout(pps, &"c".repeat(64)).await.is_err());
        assert!(f.db.refund_payout(pps).await.is_err());
        let old =
            f.db.create_payout_attempt(1, 1, "fixture-legacy")
                .await
                .unwrap();
        f.db.reserve_payout(old, &[(f.m, 1)]).await.unwrap();
        assert!(f
            .db
            .reserve_pps_payout(old, &[(f.m, 1)], &funded(&f).await, &fee(old))
            .await
            .is_err());
        assert!(f.db.confirm_pps_payout(old, &"c".repeat(64)).await.is_err());
        assert!(f.db.refund_pps_payout(old).await.is_err());
        assert_eq!(f.db.pps_invariant().await.unwrap().paying_zatoshis, 1);
        assert_eq!(f.db.get_reserved_attempts(None).await.unwrap().len(), 1);
    }
    #[tokio::test]
    async fn settlement_failure_is_atomic_and_txid_cannot_settle_two_attempts() {
        let f = setup(true, 10).await;
        f.db.credit_pps_share(
            &f.e,
            &event(&f, 1, 3 * PPS_SCALE),
            Some(&f.l),
            Some(&funded(&f).await),
            NOW,
        )
        .await
        .unwrap();
        let attempt =
            f.db.create_payout_attempt(1, 1, "fixture-pps")
                .await
                .unwrap();
        f.db.reserve_pps_payout(attempt, &[(f.m, 1)], &funded(&f).await, &fee(attempt))
            .await
            .unwrap();
        let before = f.db.pps_invariant().await.unwrap();
        sqlx::query("CREATE TRIGGER synthetic_settle_failure BEFORE INSERT ON pps_payouts BEGIN SELECT RAISE(ABORT,'fixture'); END").execute(f.db.inner()).await.unwrap();
        let txid = "c".repeat(64);
        signed(&f, attempt, &txid).await;
        assert!(f.db.confirm_pps_payout(attempt, &txid).await.is_err());
        assert_eq!(f.db.pps_invariant().await.unwrap(), before);
        sqlx::query("DROP TRIGGER synthetic_settle_failure")
            .execute(f.db.inner())
            .await
            .unwrap();
        f.db.confirm_pps_payout(attempt, &txid).await.unwrap();
        let next =
            f.db.create_payout_attempt(1, 1, "fixture-pps")
                .await
                .unwrap();
        f.db.reserve_pps_payout(next, &[(f.m, 1)], &funded(&f).await, &fee(next))
            .await
            .unwrap();
        assert!(f.db.confirm_pps_payout(next, &txid).await.is_err());
        assert_eq!(f.db.pps_invariant().await.unwrap().paying_zatoshis, 1);
    }

    #[tokio::test]
    async fn lease_expiry_after_lock_or_before_commit_rejects_all_writes() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let f = setup(true, 10).await;
        let share = event(&f, 1, PPS_SCALE);
        assert!(matches!(
            f.db.credit_pps_share_with_clock(
                &f.e,
                &share,
                Some(&f.l),
                Some(&funded(&f).await),
                NOW,
                || Ok(NOW + 60)
            )
            .await,
            Err(PpsDbError::ChainLeaseRequired)
        ));
        let calls = AtomicUsize::new(0);
        assert!(matches!(
            f.db.credit_pps_share_with_clock(
                &f.e,
                &share,
                Some(&f.l),
                Some(&funded(&f).await),
                NOW,
                || Ok(if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    NOW
                } else {
                    NOW + 60
                })
            )
            .await,
            Err(PpsDbError::ChainLeaseRequired)
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(f.db.get_total_shares_count().await.unwrap(), 0);
        assert_eq!(f.db.pps_invariant().await.unwrap().accepted_events, 0);
    }
    #[tokio::test]
    async fn canonical_pps_proofs_and_liabilities_survive_raw_share_pruning() {
        let f = setup(true, 10).await;
        let share = event(&f, 1, PPS_SCALE + 1);
        let receipt =
            f.db.credit_pps_share(&f.e, &share, Some(&f.l), Some(&funded(&f).await), NOW)
                .await
                .unwrap();
        let before = f.db.pps_invariant().await.unwrap();
        sqlx::query("DELETE FROM shares")
            .execute(f.db.inner())
            .await
            .unwrap();
        assert_eq!(f.db.pps_invariant().await.unwrap(), before);
        let replay =
            f.db.credit_pps_share(&f.e, &share, None, None, NOW + 1000)
                .await
                .unwrap();
        assert!(replay.duplicate);
        assert_eq!(replay.share_id, receipt.share_id);
        assert_eq!(f.db.get_total_shares_count().await.unwrap(), 0);
        f.db.initialize_pps_epoch(&f.e, Some(&funded(&f).await))
            .await
            .unwrap();
        assert_eq!(f.db.pps_invariant().await.unwrap(), before);
    }

    #[tokio::test]
    async fn quote_reuse_requires_exact_immutable_pricing_inputs() {
        let f = setup(true, 10).await;
        let original = event(&f, 1, PPS_SCALE);
        f.db.credit_pps_share(&f.e, &original, Some(&f.l), Some(&funded(&f).await), NOW)
            .await
            .unwrap();
        let mut changes = Vec::new();
        let mut s = event(&f, 2, PPS_SCALE);
        s.quote_height += 1;
        changes.push(s);
        let mut s = event(&f, 2, PPS_SCALE);
        s.miner_subsidy_zats += 1;
        changes.push(s);
        let mut s = event(&f, 2, PPS_SCALE);
        s.network_target_be[31] = 2;
        changes.push(s);
        let mut s = event(&f, 2, PPS_SCALE);
        s.assigned_share_target_be[31] = 3;
        changes.push(s);
        for s in changes {
            assert!(matches!(
                f.db.credit_pps_share(&f.e, &s, Some(&f.l), Some(&funded(&f).await), NOW)
                    .await,
                Err(PpsDbError::DuplicateMismatch)
            ));
        }
        let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pps_quotes")
            .fetch_one(f.db.inner())
            .await
            .unwrap();
        assert_eq!(stored, 1);
        assert_eq!(f.db.pps_invariant().await.unwrap().accepted_events, 1);
        assert_eq!(f.db.get_total_shares_count().await.unwrap(), 1);
    }
}
