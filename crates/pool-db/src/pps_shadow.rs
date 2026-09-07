//! Isolated, non-spendable PPS shadow accounting. Never opens `PoolDb`, runs
//! its migrations, writes live miner balances, or accesses a payout wallet.
//!
//! A trusted caller must validate PoW/share acceptance and produce the quote
//! with the reviewed integer pricer. A quote identifier records provenance;
//! this ledger does NOT independently verify that the supplied amount matches
//! a target, subsidy, or fee. Chain leases likewise come from a trusted fixed
//! verifier, not from miners. No result here is a promise of real payment.

use sqlx::{
    sqlite::SqliteConnectOptions, sqlite::SqlitePoolOptions, sqlite::SqliteSynchronous, Connection,
    Row, SqliteConnection, SqlitePool,
};
use std::{collections::BTreeMap, fs, path::Path, time::Duration};

pub const PPS_SHADOW_SCALE: u128 = 1_000_000_000_000;
const APPLICATION_ID: i64 = 0x50505343; // PPSC: canary, not the live pool DB.
const VERSION: i64 = 1;
const MAX_LEASE_SECONDS: i64 = 300;

const TABLES: [(&str, &str); 4] = [
    ("pps_shadow_meta", "CREATE TABLE pps_shadow_meta (singleton INTEGER PRIMARY KEY CHECK(singleton=1), network TEXT NOT NULL, cap_whole INTEGER NOT NULL CHECK(typeof(cap_whole)='integer' AND cap_whole>=0), cap_fraction INTEGER NOT NULL CHECK(typeof(cap_fraction)='integer' AND cap_fraction>=0 AND cap_fraction<1000000000000), liability_whole INTEGER NOT NULL CHECK(typeof(liability_whole)='integer' AND liability_whole>=0), liability_fraction INTEGER NOT NULL CHECK(typeof(liability_fraction)='integer' AND liability_fraction>=0 AND liability_fraction<1000000000000), accepted_events INTEGER NOT NULL CHECK(typeof(accepted_events)='integer' AND accepted_events>=0))"),
    ("pps_shadow_epochs", "CREATE TABLE pps_shadow_epochs (epoch_id TEXT PRIMARY KEY, network TEXT NOT NULL, fee_bps INTEGER NOT NULL CHECK(typeof(fee_bps)='integer' AND fee_bps>=0 AND fee_bps<=10000), cap_whole INTEGER NOT NULL CHECK(typeof(cap_whole)='integer' AND cap_whole>=0), cap_fraction INTEGER NOT NULL CHECK(typeof(cap_fraction)='integer' AND cap_fraction>=0 AND cap_fraction<1000000000000), quote_provenance TEXT NOT NULL)"),
    ("pps_shadow_miners", "CREATE TABLE pps_shadow_miners (miner_id TEXT PRIMARY KEY, whole INTEGER NOT NULL CHECK(typeof(whole)='integer' AND whole>=0), fraction INTEGER NOT NULL CHECK(typeof(fraction)='integer' AND fraction>=0 AND fraction<1000000000000))"),
    ("pps_shadow_events", "CREATE TABLE pps_shadow_events (event_id TEXT PRIMARY KEY, epoch_id TEXT NOT NULL REFERENCES pps_shadow_epochs(epoch_id), miner_id TEXT NOT NULL REFERENCES pps_shadow_miners(miner_id), quote_id TEXT NOT NULL, amount_whole INTEGER NOT NULL CHECK(typeof(amount_whole)='integer' AND amount_whole>=0), amount_fraction INTEGER NOT NULL CHECK(typeof(amount_fraction)='integer' AND amount_fraction>=0 AND amount_fraction<1000000000000), accepted_at_unix INTEGER NOT NULL CHECK(typeof(accepted_at_unix)='integer' AND accepted_at_unix>=0))"),
];

#[derive(Debug, thiserror::Error)]
pub enum ShadowError {
    #[error("not an isolated PPS canary database")]
    NotCanary,
    #[error("invalid shadow configuration or event")]
    InvalidInput,
    #[error("shadow epoch configuration is immutable")]
    EpochMismatch,
    #[error("global shadow network and reserve budget are immutable")]
    BudgetMismatch,
    #[error("duplicate share event has different immutable data")]
    DuplicateMismatch,
    #[error("a current agreeing chain-verification lease is required")]
    ChainLeaseRequired,
    #[error("shadow reserve budget would be exceeded")]
    ReserveExceeded,
    #[error("shadow integer amount cannot be represented safely")]
    AmountOverflow,
    #[error("shadow database state is inconsistent")]
    CorruptState,
    #[error("shadow database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("shadow file operation failed")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowEpoch {
    pub id: String,
    pub network: String,
    pub fee_bps: u16,
    pub reserve_cap_subzatoshis: u128,
    pub quote_provenance: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowShare {
    pub event_id: String,
    /// Synthetic/canary identifier, never a wallet address or spending key.
    pub miner_id: String,
    /// Lowercase 64-hex identifier of the immutable pricing input.
    pub quote_id: String,
    pub amount_subzatoshis: u128,
    pub accepted_at_unix: i64,
}

#[derive(Debug, Clone)]
pub struct ChainVerificationLease {
    pub network: String,
    pub checked_at_unix: i64,
    pub valid_until_unix: i64,
    pub agreeing_references: u8,
    pub disagreement: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowReceipt {
    pub credited_subzatoshis: u128,
    pub duplicate: bool,
    pub spendable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowBalance {
    pub zatoshis: u64,
    pub fractional_subzatoshis: u64,
    pub total_subzatoshis: u128,
    pub spendable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowSummary {
    pub accepted_events: u64,
    pub total_liability_subzatoshis: u128,
    pub reserve_cap_subzatoshis: u128,
    pub spendable: bool,
}

#[derive(Clone)]
pub struct PpsShadowLedger {
    pool: SqlitePool,
    epoch: ShadowEpoch,
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

fn split(amount: u128) -> Result<(i64, i64), ShadowError> {
    let whole =
        i64::try_from(amount / PPS_SHADOW_SCALE).map_err(|_| ShadowError::AmountOverflow)?;
    let fraction =
        i64::try_from(amount % PPS_SHADOW_SCALE).map_err(|_| ShadowError::AmountOverflow)?;
    Ok((whole, fraction))
}

fn combine(whole: i64, fraction: i64) -> Result<u128, ShadowError> {
    if whole < 0 || fraction < 0 || fraction as u128 >= PPS_SHADOW_SCALE {
        return Err(ShadowError::CorruptState);
    }
    (whole as u128)
        .checked_mul(PPS_SHADOW_SCALE)
        .and_then(|n| n.checked_add(fraction as u128))
        .ok_or(ShadowError::AmountOverflow)
}

/// Returns true only for a genuinely empty, unclaimed SQLite file. Existing
/// canaries must have the exact schema; live PoolDb files are never migrated.
async fn inspect(connection: &mut SqliteConnection) -> Result<bool, ShadowError> {
    let app: i64 = sqlx::query_scalar("PRAGMA application_id")
        .fetch_one(&mut *connection)
        .await?;
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut *connection)
        .await?;
    let rows =
        sqlx::query("SELECT name,type,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
            .fetch_all(&mut *connection)
            .await?;
    if app == 0 && version == 0 && rows.is_empty() {
        return Ok(true);
    }
    if app != APPLICATION_ID || version != VERSION || rows.len() != TABLES.len() {
        return Err(ShadowError::NotCanary);
    }
    for row in rows {
        let name: String = row.try_get("name")?;
        let kind: String = row.try_get("type")?;
        let sql: Option<String> = row.try_get("sql")?;
        let expected = TABLES
            .iter()
            .find(|(table, _)| *table == name)
            .map(|(_, sql)| *sql);
        if kind != "table" || sql.as_deref() != expected {
            return Err(ShadowError::NotCanary);
        }
    }
    Ok(false)
}

/// Reconcile the immutable event journal against both accounting projections.
/// Use paged rows and checked Rust integers, never SQLite SUM (which can
/// overflow signed integers or be silently converted to floating point).
/// Memory is proportional to canary miner count, not share count. This is
/// corruption detection, not authenticity against an owner rewriting all rows.
async fn reconcile(connection: &mut SqliteConnection) -> Result<(), ShadowError> {
    if sqlx::query("PRAGMA foreign_key_check")
        .fetch_optional(&mut *connection)
        .await?
        .is_some()
    {
        return Err(ShadowError::CorruptState);
    }
    let meta = sqlx::query("SELECT * FROM pps_shadow_meta WHERE singleton=1")
        .fetch_optional(&mut *connection)
        .await?
        .ok_or(ShadowError::CorruptState)?;
    let cap = combine(meta.try_get("cap_whole")?, meta.try_get("cap_fraction")?)?;
    let liability = combine(
        meta.try_get("liability_whole")?,
        meta.try_get("liability_fraction")?,
    )?;
    let expected_count: i64 = meta.try_get("accepted_events")?;
    let stored_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pps_shadow_events")
        .fetch_one(&mut *connection)
        .await?;
    if liability > cap || expected_count < 0 || expected_count != stored_count {
        return Err(ShadowError::CorruptState);
    }
    let mut event_total = 0_u128;
    let mut event_count = 0_i64;
    let mut miners = BTreeMap::<String, u128>::new();
    let mut cursor = String::new();
    loop {
        let rows = sqlx::query("SELECT event_id,miner_id,amount_whole,amount_fraction FROM pps_shadow_events WHERE event_id>?1 ORDER BY event_id LIMIT 256")
            .bind(&cursor).fetch_all(&mut *connection).await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            cursor = row.try_get("event_id")?;
            let miner: String = row.try_get("miner_id")?;
            if !identifier(&cursor) || !identifier(&miner) {
                return Err(ShadowError::CorruptState);
            }
            let amount = combine(
                row.try_get("amount_whole")?,
                row.try_get("amount_fraction")?,
            )?;
            if amount == 0 {
                return Err(ShadowError::CorruptState);
            }
            event_total = event_total
                .checked_add(amount)
                .ok_or(ShadowError::CorruptState)?;
            event_count = event_count
                .checked_add(1)
                .ok_or(ShadowError::CorruptState)?;
            if event_total > cap || event_count > expected_count {
                return Err(ShadowError::CorruptState);
            }
            let balance = miners.entry(miner).or_default();
            *balance = balance
                .checked_add(amount)
                .ok_or(ShadowError::CorruptState)?;
        }
    }
    if event_total != liability || event_count != expected_count {
        return Err(ShadowError::CorruptState);
    }
    cursor.clear();
    let stored_miner_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pps_shadow_miners")
        .fetch_one(&mut *connection)
        .await?;
    let mut miner_count = 0_i64;
    loop {
        let rows = sqlx::query("SELECT miner_id,whole,fraction FROM pps_shadow_miners WHERE miner_id>?1 ORDER BY miner_id LIMIT 256")
            .bind(&cursor).fetch_all(&mut *connection).await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            cursor = row.try_get("miner_id")?;
            miner_count = miner_count
                .checked_add(1)
                .ok_or(ShadowError::CorruptState)?;
            let balance = combine(row.try_get("whole")?, row.try_get("fraction")?)?;
            if miners.remove(&cursor) != Some(balance) {
                return Err(ShadowError::CorruptState);
            }
        }
    }
    if !miners.is_empty() || miner_count != stored_miner_count {
        return Err(ShadowError::CorruptState);
    }
    Ok(())
}

impl PpsShadowLedger {
    /// A separate `.pps-shadow.sqlite` file is mandatory. Existing non-canary
    /// databases are inspected read-only and rejected before any schema write.
    /// Additional epochs share the SAME global budget and miner carry; they
    /// cannot reset liability or increase the reserve by reopening the file.
    pub async fn open(path: &Path, epoch: ShadowEpoch) -> Result<Self, ShadowError> {
        if !identifier(&epoch.id)
            || !identifier(&epoch.quote_provenance)
            || !matches!(epoch.network.as_str(), "mainnet" | "testnet")
            || epoch.fee_bps >= 10000
            || epoch.reserve_cap_subzatoshis == 0
        {
            return Err(ShadowError::InvalidInput);
        }
        let cap = split(epoch.reserve_cap_subzatoshis)?;
        if !path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name.ends_with(".pps-shadow.sqlite"))
        {
            return Err(ShadowError::NotCanary);
        }
        if !path.exists() {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(path) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
        }
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.file_type().is_file() {
            return Err(ShadowError::NotCanary);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o777 != 0o600 {
                return Err(ShadowError::NotCanary);
            }
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .busy_timeout(Duration::from_secs(5))
            .pragma("trusted_schema", "OFF");
        let mut readonly = SqliteConnection::connect_with(&options.clone().read_only(true)).await?;
        inspect(&mut readonly).await?;
        readonly.close().await?;
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options.synchronous(SqliteSynchronous::Full))
            .await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        if inspect(&mut tx).await? {
            for (_, ddl) in TABLES {
                sqlx::query(ddl).execute(&mut *tx).await?;
            }
            sqlx::query("PRAGMA application_id = 1347441475")
                .execute(&mut *tx)
                .await?;
            sqlx::query("PRAGMA user_version = 1")
                .execute(&mut *tx)
                .await?;
            sqlx::query("INSERT INTO pps_shadow_meta VALUES (1,?1,?2,?3,0,0,0)")
                .bind(&epoch.network)
                .bind(cap.0)
                .bind(cap.1)
                .execute(&mut *tx)
                .await?;
        }
        reconcile(&mut tx).await?;
        let meta = sqlx::query("SELECT * FROM pps_shadow_meta WHERE singleton=1")
            .fetch_one(&mut *tx)
            .await?;
        if meta.try_get::<String, _>("network")? != epoch.network
            || combine(meta.try_get("cap_whole")?, meta.try_get("cap_fraction")?)?
                != epoch.reserve_cap_subzatoshis
        {
            return Err(ShadowError::BudgetMismatch);
        }
        let existing = sqlx::query("SELECT * FROM pps_shadow_epochs WHERE epoch_id=?1")
            .bind(&epoch.id)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(existing) = existing {
            if existing.try_get::<String, _>("network")? != epoch.network
                || existing.try_get::<i64, _>("fee_bps")? != i64::from(epoch.fee_bps)
                || combine(
                    existing.try_get("cap_whole")?,
                    existing.try_get("cap_fraction")?,
                )? != epoch.reserve_cap_subzatoshis
                || existing.try_get::<String, _>("quote_provenance")? != epoch.quote_provenance
            {
                return Err(ShadowError::EpochMismatch);
            }
        } else {
            sqlx::query("INSERT INTO pps_shadow_epochs VALUES (?1,?2,?3,?4,?5,?6)")
                .bind(&epoch.id)
                .bind(&epoch.network)
                .bind(i64::from(epoch.fee_bps))
                .bind(cap.0)
                .bind(cap.1)
                .bind(&epoch.quote_provenance)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(Self { pool, epoch })
    }

    /// Exact duplicate replay returns the original credited amount without a
    /// current lease: it creates no new liability. All NEW events require a
    /// fresh, agreeing lease covering both share acceptance and current time.
    /// Acceptance, miner carry, and global liability commit atomically.
    pub async fn credit_share(
        &self,
        share: &ShadowShare,
        lease: Option<&ChainVerificationLease>,
        now_unix: i64,
    ) -> Result<ShadowReceipt, ShadowError> {
        if !identifier(&share.event_id)
            || !identifier(&share.miner_id)
            || share.quote_id.len() != 64
            || !share
                .quote_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || share.amount_subzatoshis == 0
            || share.accepted_at_unix < 0
            || now_unix < 0
        {
            return Err(ShadowError::InvalidInput);
        }
        let amount = split(share.amount_subzatoshis)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(row) = sqlx::query("SELECT * FROM pps_shadow_events WHERE event_id=?1")
            .bind(&share.event_id)
            .fetch_optional(&mut *tx)
            .await?
        {
            if row.try_get::<String, _>("epoch_id")? != self.epoch.id
                || row.try_get::<String, _>("miner_id")? != share.miner_id
                || row.try_get::<String, _>("quote_id")? != share.quote_id
                || row.try_get::<i64, _>("accepted_at_unix")? != share.accepted_at_unix
                || combine(
                    row.try_get("amount_whole")?,
                    row.try_get("amount_fraction")?,
                )? != share.amount_subzatoshis
            {
                return Err(ShadowError::DuplicateMismatch);
            }
            tx.commit().await?;
            return Ok(ShadowReceipt {
                credited_subzatoshis: share.amount_subzatoshis,
                duplicate: true,
                spendable: false,
            });
        }
        let lease = lease.ok_or(ShadowError::ChainLeaseRequired)?;
        if lease.network != self.epoch.network
            || lease.disagreement
            || lease.agreeing_references < 2
            || lease.checked_at_unix < 0
            || lease.valid_until_unix < lease.checked_at_unix
            || lease.valid_until_unix - lease.checked_at_unix > MAX_LEASE_SECONDS
            || now_unix < lease.checked_at_unix
            || now_unix > lease.valid_until_unix
            || share.accepted_at_unix < lease.checked_at_unix
            || share.accepted_at_unix > now_unix
        {
            return Err(ShadowError::ChainLeaseRequired);
        }
        let meta = sqlx::query("SELECT * FROM pps_shadow_meta WHERE singleton=1")
            .fetch_one(&mut *tx)
            .await?;
        let total = combine(
            meta.try_get("liability_whole")?,
            meta.try_get("liability_fraction")?,
        )?;
        let cap = combine(meta.try_get("cap_whole")?, meta.try_get("cap_fraction")?)?;
        if cap != self.epoch.reserve_cap_subzatoshis {
            return Err(ShadowError::CorruptState);
        }
        let next = total
            .checked_add(share.amount_subzatoshis)
            .ok_or(ShadowError::AmountOverflow)?;
        if next > cap {
            return Err(ShadowError::ReserveExceeded);
        }
        let count: i64 = meta.try_get("accepted_events")?;
        let next_count = count.checked_add(1).ok_or(ShadowError::AmountOverflow)?;
        let balance = sqlx::query("SELECT whole,fraction FROM pps_shadow_miners WHERE miner_id=?1")
            .bind(&share.miner_id)
            .fetch_optional(&mut *tx)
            .await?;
        let old_balance = match balance {
            Some(row) => combine(row.try_get("whole")?, row.try_get("fraction")?)?,
            None => 0,
        };
        let next_balance = split(
            old_balance
                .checked_add(share.amount_subzatoshis)
                .ok_or(ShadowError::AmountOverflow)?,
        )?;
        let next_parts = split(next)?;
        sqlx::query("INSERT INTO pps_shadow_miners VALUES (?1,?2,?3) ON CONFLICT(miner_id) DO UPDATE SET whole=excluded.whole,fraction=excluded.fraction")
            .bind(&share.miner_id).bind(next_balance.0).bind(next_balance.1).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO pps_shadow_events VALUES (?1,?2,?3,?4,?5,?6,?7)")
            .bind(&share.event_id)
            .bind(&self.epoch.id)
            .bind(&share.miner_id)
            .bind(&share.quote_id)
            .bind(amount.0)
            .bind(amount.1)
            .bind(share.accepted_at_unix)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE pps_shadow_meta SET liability_whole=?1,liability_fraction=?2,accepted_events=?3 WHERE singleton=1")
            .bind(next_parts.0).bind(next_parts.1).bind(next_count).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(ShadowReceipt {
            credited_subzatoshis: share.amount_subzatoshis,
            duplicate: false,
            spendable: false,
        })
    }

    pub async fn summary(&self) -> Result<ShadowSummary, ShadowError> {
        let row = sqlx::query("SELECT * FROM pps_shadow_meta WHERE singleton=1")
            .fetch_one(&self.pool)
            .await?;
        Ok(ShadowSummary {
            accepted_events: u64::try_from(row.try_get::<i64, _>("accepted_events")?)
                .map_err(|_| ShadowError::CorruptState)?,
            total_liability_subzatoshis: combine(
                row.try_get("liability_whole")?,
                row.try_get("liability_fraction")?,
            )?,
            reserve_cap_subzatoshis: combine(
                row.try_get("cap_whole")?,
                row.try_get("cap_fraction")?,
            )?,
            spendable: false,
        })
    }

    pub async fn miner_balance(&self, miner_id: &str) -> Result<ShadowBalance, ShadowError> {
        if !identifier(miner_id) {
            return Err(ShadowError::InvalidInput);
        }
        let row = sqlx::query("SELECT whole,fraction FROM pps_shadow_miners WHERE miner_id=?1")
            .bind(miner_id)
            .fetch_optional(&self.pool)
            .await?;
        let total = match row {
            Some(row) => combine(row.try_get("whole")?, row.try_get("fraction")?)?,
            None => 0,
        };
        Ok(ShadowBalance {
            zatoshis: (total / PPS_SHADOW_SCALE) as u64,
            fractional_subzatoshis: (total % PPS_SHADOW_SCALE) as u64,
            total_subzatoshis: total,
            spendable: false,
        })
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    const NOW: i64 = 1_700_000_000;

    struct CanaryDir(PathBuf);
    impl CanaryDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "pps-shadow-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            }
            Self(path)
        }
        fn file(&self) -> PathBuf {
            self.0.join("fixture.pps-shadow.sqlite")
        }
    }
    impl Drop for CanaryDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn epoch(cap: u128) -> ShadowEpoch {
        ShadowEpoch {
            id: "epoch-one".into(),
            network: "testnet".into(),
            fee_bps: 100,
            reserve_cap_subzatoshis: cap,
            quote_provenance: "synthetic-standard-pps-v1".into(),
        }
    }
    fn lease() -> ChainVerificationLease {
        ChainVerificationLease {
            network: "testnet".into(),
            checked_at_unix: NOW - 10,
            valid_until_unix: NOW + 60,
            agreeing_references: 2,
            disagreement: false,
        }
    }
    fn share(id: &str, amount: u128) -> ShadowShare {
        ShadowShare {
            event_id: id.into(),
            miner_id: "synthetic-miner".into(),
            quote_id: "a".repeat(64),
            amount_subzatoshis: amount,
            accepted_at_unix: NOW,
        }
    }

    #[tokio::test]
    async fn carry_and_global_fractional_liability_survive_restart() {
        let dir = CanaryDir::new();
        let ledger = PpsShadowLedger::open(&dir.file(), epoch(2 * PPS_SHADOW_SCALE))
            .await
            .unwrap();
        ledger
            .credit_share(&share("one", PPS_SHADOW_SCALE - 1), Some(&lease()), NOW)
            .await
            .unwrap();
        assert_eq!(
            ledger
                .miner_balance("synthetic-miner")
                .await
                .unwrap()
                .zatoshis,
            0
        );
        ledger.close().await;
        let restarted = PpsShadowLedger::open(&dir.file(), epoch(2 * PPS_SHADOW_SCALE))
            .await
            .unwrap();
        restarted
            .credit_share(&share("two", 2), Some(&lease()), NOW)
            .await
            .unwrap();
        assert_eq!(
            restarted.miner_balance("synthetic-miner").await.unwrap(),
            ShadowBalance {
                zatoshis: 1,
                fractional_subzatoshis: 1,
                total_subzatoshis: PPS_SHADOW_SCALE + 1,
                spendable: false,
            }
        );
        assert_eq!(restarted.summary().await.unwrap().accepted_events, 2);
        assert_eq!(
            restarted
                .summary()
                .await
                .unwrap()
                .total_liability_subzatoshis,
            PPS_SHADOW_SCALE + 1
        );
        restarted.close().await;
    }

    #[tokio::test]
    async fn exact_duplicate_retries_without_a_current_lease() {
        let dir = CanaryDir::new();
        let ledger = PpsShadowLedger::open(&dir.file(), epoch(100))
            .await
            .unwrap();
        let event = share("one", 100);
        let first = ledger
            .credit_share(&event, Some(&lease()), NOW)
            .await
            .unwrap();
        assert!(!first.duplicate && !first.spendable);
        let retry = ledger
            .credit_share(&event, None, NOW + 86400)
            .await
            .unwrap();
        assert!(retry.duplicate && !retry.spendable);
        assert_eq!(retry.credited_subzatoshis, first.credited_subzatoshis);
        assert_eq!(ledger.summary().await.unwrap().accepted_events, 1);
        assert_eq!(
            ledger.summary().await.unwrap().total_liability_subzatoshis,
            100
        );
        ledger.close().await;
    }

    #[tokio::test]
    async fn duplicate_rejects_every_immutable_data_change() {
        let dir = CanaryDir::new();
        let ledger = PpsShadowLedger::open(&dir.file(), epoch(1000))
            .await
            .unwrap();
        let event = share("one", 100);
        ledger
            .credit_share(&event, Some(&lease()), NOW)
            .await
            .unwrap();
        let mut variations = Vec::new();
        let mut altered = event.clone();
        altered.miner_id = "other-miner".into();
        variations.push(altered);
        let mut altered = event.clone();
        altered.quote_id = "b".repeat(64);
        variations.push(altered);
        let mut altered = event.clone();
        altered.amount_subzatoshis += 1;
        variations.push(altered);
        let mut altered = event.clone();
        altered.accepted_at_unix -= 1;
        variations.push(altered);
        for altered in variations {
            assert!(matches!(
                ledger.credit_share(&altered, Some(&lease()), NOW).await,
                Err(ShadowError::DuplicateMismatch)
            ));
        }
        assert_eq!(
            ledger.summary().await.unwrap().total_liability_subzatoshis,
            100
        );
        ledger.close().await;
    }

    #[tokio::test]
    async fn epoch_fee_and_provenance_are_immutable_without_resetting_budget() {
        let dir = CanaryDir::new();
        let config = epoch(100);
        let ledger = PpsShadowLedger::open(&dir.file(), config.clone())
            .await
            .unwrap();
        ledger
            .credit_share(&share("one", 60), Some(&lease()), NOW)
            .await
            .unwrap();
        let mut changed = config.clone();
        changed.fee_bps = 200;
        assert!(matches!(
            PpsShadowLedger::open(&dir.file(), changed.clone()).await,
            Err(ShadowError::EpochMismatch)
        ));
        changed = config.clone();
        changed.quote_provenance = "different-pricer".into();
        assert!(matches!(
            PpsShadowLedger::open(&dir.file(), changed).await,
            Err(ShadowError::EpochMismatch)
        ));
        changed = config.clone();
        changed.id = "epoch-two".into();
        changed.fee_bps = 200;
        let second = PpsShadowLedger::open(&dir.file(), changed.clone())
            .await
            .unwrap();
        assert!(matches!(
            second.credit_share(&share("one", 60), None, NOW).await,
            Err(ShadowError::DuplicateMismatch)
        ));
        assert!(matches!(
            second
                .credit_share(&share("too-large", 41), Some(&lease()), NOW)
                .await,
            Err(ShadowError::ReserveExceeded)
        ));
        second
            .credit_share(&share("fits", 40), Some(&lease()), NOW)
            .await
            .unwrap();
        assert_eq!(
            second.summary().await.unwrap().total_liability_subzatoshis,
            100
        );
        assert_eq!(
            second
                .miner_balance("synthetic-miner")
                .await
                .unwrap()
                .fractional_subzatoshis,
            100
        );
        changed.id = "epoch-three".into();
        changed.reserve_cap_subzatoshis = 200;
        assert!(matches!(
            PpsShadowLedger::open(&dir.file(), changed.clone()).await,
            Err(ShadowError::BudgetMismatch)
        ));
        changed.reserve_cap_subzatoshis = 100;
        changed.network = "mainnet".into();
        assert!(matches!(
            PpsShadowLedger::open(&dir.file(), changed).await,
            Err(ShadowError::BudgetMismatch)
        ));
        second.close().await;
        ledger.close().await;
    }

    #[tokio::test]
    async fn concurrent_miners_cannot_overdraw_global_fractional_budget() {
        let dir = CanaryDir::new();
        let ledger = PpsShadowLedger::open(&dir.file(), epoch(PPS_SHADOW_SCALE))
            .await
            .unwrap();
        let first = share("one", 600_000_000_000);
        let mut second = share("two", 600_000_000_000);
        second.miner_id = "other-miner".into();
        let verification = lease();
        let (a, b) = tokio::join!(
            ledger.credit_share(&first, Some(&verification), NOW),
            ledger.credit_share(&second, Some(&verification), NOW)
        );
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        assert!(
            matches!(a, Err(ShadowError::ReserveExceeded))
                || matches!(b, Err(ShadowError::ReserveExceeded))
        );
        assert_eq!(ledger.summary().await.unwrap().accepted_events, 1);
        assert_eq!(
            ledger.summary().await.unwrap().total_liability_subzatoshis,
            600_000_000_000
        );
        ledger.close().await;
    }

    #[tokio::test]
    async fn concurrent_duplicate_is_credited_once() {
        let dir = CanaryDir::new();
        let ledger = PpsShadowLedger::open(&dir.file(), epoch(100))
            .await
            .unwrap();
        let event = share("one", 100);
        let verification = lease();
        let (a, b) = tokio::join!(
            ledger.credit_share(&event, Some(&verification), NOW),
            ledger.credit_share(&event, Some(&verification), NOW)
        );
        assert_ne!(a.unwrap().duplicate, b.unwrap().duplicate);
        assert_eq!(ledger.summary().await.unwrap().accepted_events, 1);
        ledger.close().await;
    }

    #[tokio::test]
    async fn new_credits_halt_on_absent_stale_or_disagreeing_chain_lease() {
        let dir = CanaryDir::new();
        let ledger = PpsShadowLedger::open(&dir.file(), epoch(100))
            .await
            .unwrap();
        let event = share("one", 10);
        assert!(matches!(
            ledger.credit_share(&event, None, NOW).await,
            Err(ShadowError::ChainLeaseRequired)
        ));
        let mut variants = Vec::new();
        let mut invalid = lease();
        invalid.valid_until_unix = NOW - 1;
        variants.push(invalid);
        let mut invalid = lease();
        invalid.checked_at_unix = NOW + 1;
        variants.push(invalid);
        let mut invalid = lease();
        invalid.disagreement = true;
        variants.push(invalid);
        let mut invalid = lease();
        invalid.agreeing_references = 1;
        variants.push(invalid);
        let mut invalid = lease();
        invalid.network = "mainnet".into();
        variants.push(invalid);
        let mut invalid = lease();
        invalid.valid_until_unix = NOW + 301;
        variants.push(invalid);
        for invalid in variants {
            assert!(matches!(
                ledger.credit_share(&event, Some(&invalid), NOW).await,
                Err(ShadowError::ChainLeaseRequired)
            ));
        }
        assert_eq!(ledger.summary().await.unwrap().accepted_events, 0);
        assert_eq!(
            ledger
                .miner_balance("synthetic-miner")
                .await
                .unwrap()
                .total_subzatoshis,
            0
        );
        ledger.close().await;
    }

    #[tokio::test]
    async fn sqlite_failure_rolls_back_acceptance_carry_and_liability() {
        let dir = CanaryDir::new();
        let ledger = PpsShadowLedger::open(&dir.file(), epoch(100))
            .await
            .unwrap();
        // Deliberate failure after the miner carry update. No production code
        // exports this connection; this fault exists only inside this fixture.
        sqlx::query("CREATE TRIGGER reject_fixture BEFORE INSERT ON pps_shadow_events BEGIN SELECT RAISE(ABORT,'synthetic failure'); END")
            .execute(&ledger.pool).await.unwrap();
        assert!(matches!(
            ledger
                .credit_share(&share("one", 10), Some(&lease()), NOW)
                .await,
            Err(ShadowError::Database(_))
        ));
        assert_eq!(ledger.summary().await.unwrap().accepted_events, 0);
        assert_eq!(
            ledger.summary().await.unwrap().total_liability_subzatoshis,
            0
        );
        assert_eq!(
            ledger
                .miner_balance("synthetic-miner")
                .await
                .unwrap()
                .total_subzatoshis,
            0
        );
        let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pps_shadow_events")
            .fetch_one(&ledger.pool)
            .await
            .unwrap();
        assert_eq!(events, 0);
        ledger.close().await;
    }

    #[tokio::test]
    async fn rejects_live_schema_without_modifying_it() {
        let dir = CanaryDir::new();
        let path = dir.file();
        let mut foreign = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true),
        )
        .await
        .unwrap();
        sqlx::query("CREATE TABLE balances (miner_id INTEGER PRIMARY KEY, pending INTEGER)")
            .execute(&mut foreign)
            .await
            .unwrap();
        sqlx::query("INSERT INTO balances VALUES (1,123)")
            .execute(&mut foreign)
            .await
            .unwrap();
        foreign.close().await.unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let original = fs::read(&path).unwrap();
        assert!(matches!(
            PpsShadowLedger::open(&path, epoch(100)).await,
            Err(ShadowError::NotCanary)
        ));
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(matches!(
            PpsShadowLedger::open(&dir.0.join("pool.db"), epoch(100)).await,
            Err(ShadowError::NotCanary)
        ));
        assert!(!dir.0.join("pool.db").exists());
    }

    #[tokio::test]
    async fn checked_integer_bounds_reject_overflow_and_zero_credit() {
        let dir = CanaryDir::new();
        assert!(matches!(
            PpsShadowLedger::open(&dir.file(), epoch(u128::MAX)).await,
            Err(ShadowError::AmountOverflow)
        ));
        let ledger = PpsShadowLedger::open(&dir.file(), epoch(100))
            .await
            .unwrap();
        assert!(matches!(
            ledger
                .credit_share(&share("overflow", u128::MAX), Some(&lease()), NOW)
                .await,
            Err(ShadowError::AmountOverflow)
        ));
        assert!(matches!(
            ledger
                .credit_share(&share("zero", 0), Some(&lease()), NOW)
                .await,
            Err(ShadowError::InvalidInput)
        ));
        assert_eq!(ledger.summary().await.unwrap().accepted_events, 0);
        ledger.close().await;
    }

    #[tokio::test]
    async fn restart_rejects_schema_valid_liability_or_count_undercount() {
        for mutation in [
            "UPDATE pps_shadow_meta SET liability_fraction=0",
            "UPDATE pps_shadow_meta SET accepted_events=0",
            "UPDATE pps_shadow_events SET amount_fraction=1",
        ] {
            let dir = CanaryDir::new();
            let ledger = PpsShadowLedger::open(&dir.file(), epoch(100))
                .await
                .unwrap();
            ledger
                .credit_share(&share("one", 60), Some(&lease()), NOW)
                .await
                .unwrap();
            sqlx::query(mutation).execute(&ledger.pool).await.unwrap();
            ledger.close().await;
            assert!(matches!(
                PpsShadowLedger::open(&dir.file(), epoch(100)).await,
                Err(ShadowError::CorruptState)
            ));
        }
    }

    #[tokio::test]
    async fn restart_rejects_per_miner_mismatch_even_when_global_total_matches() {
        let dir = CanaryDir::new();
        let ledger = PpsShadowLedger::open(&dir.file(), epoch(100))
            .await
            .unwrap();
        ledger
            .credit_share(&share("one", 60), Some(&lease()), NOW)
            .await
            .unwrap();
        let mut second = share("two", 20);
        second.miner_id = "synthetic-other".into();
        ledger
            .credit_share(&second, Some(&lease()), NOW)
            .await
            .unwrap();
        sqlx::query("UPDATE pps_shadow_miners SET fraction=CASE WHEN miner_id='synthetic-miner' THEN 50 ELSE 30 END")
            .execute(&ledger.pool).await.unwrap();
        ledger.close().await;
        assert!(matches!(
            PpsShadowLedger::open(&dir.file(), epoch(100)).await,
            Err(ShadowError::CorruptState)
        ));
    }

    #[tokio::test]
    async fn all_shadow_connections_use_full_synchronous_durability() {
        let dir = CanaryDir::new();
        let ledger = PpsShadowLedger::open(&dir.file(), epoch(100))
            .await
            .unwrap();
        let mut connections = Vec::new();
        for _ in 0..4 {
            let mut connection = ledger.pool.acquire().await.unwrap();
            let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
                .fetch_one(&mut *connection)
                .await
                .unwrap();
            assert_eq!(synchronous, 2);
            connections.push(connection);
        }
        drop(connections);
        ledger.close().await;
    }

    #[tokio::test]
    async fn restart_reconciles_multiple_event_and_miner_pages() {
        let dir = CanaryDir::new();
        let ledger = PpsShadowLedger::open(&dir.file(), epoch(1000))
            .await
            .unwrap();
        for number in 0..257 {
            let mut event = share(&format!("event-{number:03}"), 1);
            event.miner_id = format!("miner-{number:03}");
            ledger
                .credit_share(&event, Some(&lease()), NOW)
                .await
                .unwrap();
        }
        ledger.close().await;
        let reopened = PpsShadowLedger::open(&dir.file(), epoch(1000))
            .await
            .unwrap();
        let summary = reopened.summary().await.unwrap();
        assert_eq!(summary.accepted_events, 257);
        assert_eq!(summary.total_liability_subzatoshis, 257);
        assert_eq!(
            reopened
                .miner_balance("miner-256")
                .await
                .unwrap()
                .total_subzatoshis,
            1
        );
        reopened.close().await;
    }

    #[tokio::test]
    async fn restart_rejects_empty_or_null_ids_even_with_adjusted_counters() {
        for event_id in [Some(""), None] {
            let dir = CanaryDir::new();
            let ledger = PpsShadowLedger::open(&dir.file(), epoch(100))
                .await
                .unwrap();
            ledger
                .credit_share(&share("one", 60), Some(&lease()), NOW)
                .await
                .unwrap();
            // Ordinary SQLite TEXT PRIMARY KEY permits NULL. These malformed
            // rows must not escape reconciliation's keyset pagination.
            sqlx::query(
                "INSERT INTO pps_shadow_events VALUES (?1,'epoch-one','synthetic-miner',?2,0,1,?3)",
            )
            .bind(event_id)
            .bind("a".repeat(64))
            .bind(NOW)
            .execute(&ledger.pool)
            .await
            .unwrap();
            sqlx::query("UPDATE pps_shadow_meta SET liability_fraction=61,accepted_events=2")
                .execute(&ledger.pool)
                .await
                .unwrap();
            sqlx::query("UPDATE pps_shadow_miners SET fraction=61")
                .execute(&ledger.pool)
                .await
                .unwrap();
            ledger.close().await;
            assert!(matches!(
                PpsShadowLedger::open(&dir.file(), epoch(100)).await,
                Err(ShadowError::CorruptState)
            ));
        }
    }
}
