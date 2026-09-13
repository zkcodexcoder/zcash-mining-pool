use sqlx::sqlite::SqliteRow;
use sqlx::{Connection, Row, SqlitePool};

use crate::models::*;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    /// The payout guard matched 0 rows: the miner's pending is below the amount
    /// (already settled). Returned instead of blindly driving pending negative.
    #[error("Payout guard: miner {miner_id} pending < {amount} zat (already settled?), refusing to double-debit")]
    InsufficientPending { miner_id: i64, amount: i64 },
}

/// Database access layer for the mining pool.
#[derive(Clone)]
pub struct PoolDb {
    pool: SqlitePool,
}

impl PoolDb {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn inner(&self) -> &SqlitePool {
        &self.pool
    }

    /// Run migrations from the embedded SQL files.
    pub async fn run_migrations(&self) -> Result<(), DbError> {
        let schema = include_str!("../migrations/001_initial.sql");
        sqlx::raw_sql(schema).execute(&self.pool).await?;
        let migration_002 = include_str!("../migrations/002_block_luck.sql");
        // ALTER TABLE may fail if column already exists; ignore that error.
        let _ = sqlx::raw_sql(migration_002).execute(&self.pool).await;
        let migration_003 = include_str!("../migrations/003_share_indexes.sql");
        let _ = sqlx::raw_sql(migration_003).execute(&self.pool).await;
        let migration_004 = include_str!("../migrations/004_worker_difficulty.sql");
        let _ = sqlx::raw_sql(migration_004).execute(&self.pool).await;
        let migration_005 = include_str!("../migrations/005_pool_status.sql");
        let _ = sqlx::raw_sql(migration_005).execute(&self.pool).await;
        let migration_006 = include_str!("../migrations/006_share_session_id.sql");
        let _ = sqlx::raw_sql(migration_006).execute(&self.pool).await;
        let migration_007 = include_str!("../migrations/007_payout_attempts.sql");
        let _ = sqlx::raw_sql(migration_007).execute(&self.pool).await;
        let migration_008 = include_str!("../migrations/008_block_credits.sql");
        let _ = sqlx::raw_sql(migration_008).execute(&self.pool).await;
        let migration_009 = include_str!("../migrations/009_clawback_ack_maturity.sql");
        let _ = sqlx::raw_sql(migration_009).execute(&self.pool).await;
        let migration_010 = include_str!("../migrations/010_tx_costs.sql");
        let _ = sqlx::raw_sql(migration_010).execute(&self.pool).await;
        let migration_011 = include_str!("../migrations/011_pool_labels.sql");
        let _ = sqlx::raw_sql(migration_011).execute(&self.pool).await;
        let migration_012 = include_str!("../migrations/012_payout_reservation.sql");
        let _ = sqlx::raw_sql(migration_012).execute(&self.pool).await;
        let migration_013 = include_str!("../migrations/013_block_submissions.sql");
        let _ = sqlx::raw_sql(migration_013).execute(&self.pool).await;
        let migration_014 = include_str!("../migrations/014_shares_rollup.sql");
        let _ = sqlx::raw_sql(migration_014).execute(&self.pool).await;
        let mut pps_migration = self.pool.begin().await?;
        sqlx::raw_sql(include_str!("../migrations/015_pps_accounts.sql")).execute(&mut *pps_migration).await?;
        sqlx::raw_sql(include_str!("../migrations/016_pps_funding.sql")).execute(&mut *pps_migration).await?;
        sqlx::raw_sql(include_str!("../migrations/017_pps_conventional.sql")).execute(&mut *pps_migration).await?;
        sqlx::raw_sql(include_str!("../migrations/018_pps_conventional_intents.sql")).execute(&mut *pps_migration).await?;
        sqlx::raw_sql(include_str!("../migrations/019_pps_budget_extension.sql")).execute(&mut *pps_migration).await?;
        sqlx::raw_sql(include_str!("../migrations/020_pps_indexes.sql")).execute(&mut *pps_migration).await?;
        pps_migration.commit().await?;
        Ok(())
    }

    /// Audit #16: migrations run best-effort for idempotence, which means a
    /// partially-applied migration could leave a schema hole the process then
    /// runs on top of (upstream of the block-90 class). Refuse to start
    /// instead: verify every table/column the money path depends on.
    pub async fn assert_critical_schema(&self) -> Result<(), DbError> {
        let tables = [
            "miners", "workers", "shares", "blocks", "balances", "payouts",
            "payout_attempts", "payout_items", "block_credits",
            "orphan_clawbacks", "pool_tx_costs", "block_submissions",
            "shares_rollup", "pool_status",
            "pps_meta", "pps_epochs", "pps_accounts", "pps_quotes", "pps_events", "pps_payout_items", "pps_payouts", "pps_block_markers", "pps_submission_markers",
            "pps_funding_generation", "pps_funding_policy", "pps_fee_reservations",
            "pps_conventional_attempts",
            "pps_conventional_intents",
            "pps_conventional_halts",
            "pps_budget_extensions",
        ];
        for t in tables {
            let n: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            )
            .bind(t)
            .fetch_one(&self.pool)
            .await?;
            if n.0 == 0 {
                return Err(DbError::Sqlx(sqlx::Error::Protocol(format!(
                    "critical table missing after migrations: {t}"
                ))));
            }
        }
        for (table, col) in [("balances", "paying"), ("blocks", "actual_reward"), ("blocks", "costs_recovered")] {
            let n: (i64,) = sqlx::query_as(&format!(
                "SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?1"
            ))
            .bind(col)
            .fetch_one(&self.pool)
            .await?;
            if n.0 == 0 {
                return Err(DbError::Sqlx(sqlx::Error::Protocol(format!(
                    "critical column missing after migrations: {table}.{col}"
                ))));
            }
        }
        Ok(())
    }

    /// Audit #16: aggregate full UTC hours of shares older than
    /// `older_than_days` into `shares_rollup` and delete the raw rows — one
    /// hour per BEGIN IMMEDIATE transaction (atomic: rollup and delete land
    /// together), up to `max_hours` per call. Returns (hours, rows_deleted).
    pub async fn rollup_and_prune_shares(
        &self,
        older_than_days: i64,
        max_hours: i64,
    ) -> Result<(i64, i64), DbError> {
        let mut hours = 0i64;
        let mut deleted = 0i64;
        loop {
            if hours >= max_hours {
                break;
            }
            let bucket: Option<(String,)> = sqlx::query_as(
                "SELECT strftime('%Y-%m-%d %H:00:00', created_at) AS b FROM shares \
                 WHERE created_at < datetime('now', '-' || ?1 || ' days') \
                 ORDER BY created_at ASC LIMIT 1",
            )
            .bind(older_than_days)
            .fetch_optional(&self.pool)
            .await?;
            let Some((bucket,)) = bucket else { break };

            let mut conn = self.pool.acquire().await?;
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            let res: Result<i64, DbError> = async {
                let agg: (i64, f64) = sqlx::query_as(
                    "SELECT COUNT(*), COALESCE(SUM(difficulty), 0.0) FROM shares \
                     WHERE created_at >= ?1 AND created_at < datetime(?1, '+1 hour')",
                )
                .bind(&bucket)
                .fetch_one(&mut *conn)
                .await?;
                sqlx::query(
                    "INSERT INTO shares_rollup (bucket, cnt, diff_sum) VALUES (?1, ?2, ?3) \
                     ON CONFLICT(bucket) DO UPDATE SET cnt = excluded.cnt + shares_rollup.cnt, \
                     diff_sum = excluded.diff_sum + shares_rollup.diff_sum",
                )
                .bind(&bucket)
                .bind(agg.0)
                .bind(agg.1)
                .execute(&mut *conn)
                .await?;
                let del = sqlx::query(
                    "DELETE FROM shares WHERE created_at >= ?1 AND created_at < datetime(?1, '+1 hour')",
                )
                .bind(&bucket)
                .execute(&mut *conn)
                .await?;
                Ok(del.rows_affected() as i64)
            }
            .await;
            match res {
                Ok(d) => match sqlx::query("COMMIT").execute(&mut *conn).await {
                    Ok(_) => {
                        hours += 1;
                        deleted += d;
                    }
                    Err(e) => {
                        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                        return Err(e.into());
                    }
                },
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    return Err(e);
                }
            }
        }
        Ok((hours, deleted))
    }

    /// Operator-defined address -> pool-name overrides, as a lookup map.
    /// Read live by the Network tab; a failure here is non-fatal (the caller
    /// falls back to the built-in map in `network.rs::identify_pool`).
    pub async fn get_pool_label_map(
        &self,
    ) -> Result<std::collections::HashMap<String, String>, DbError> {
        let rows = sqlx::query("SELECT address, name FROM pool_labels")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .iter()
            .map(|r| (r.get::<String, _>("address"), r.get::<String, _>("name")))
            .collect())
    }

    /// Full label rows for the admin Labels tab (includes note + updated_at).
    pub async fn list_pool_labels(&self) -> Result<Vec<PoolLabel>, DbError> {
        let rows =
            sqlx::query("SELECT address, name, note, updated_at FROM pool_labels ORDER BY name")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows
            .iter()
            .map(|r| PoolLabel {
                address: r.get("address"),
                name: r.get("name"),
                note: r.get("note"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    /// Insert or update a label by address.
    pub async fn upsert_pool_label(
        &self,
        address: &str,
        name: &str,
        note: &str,
    ) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO pool_labels (address, name, note, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))
             ON CONFLICT(address) DO UPDATE SET
                 name = excluded.name,
                 note = excluded.note,
                 updated_at = datetime('now')",
        )
        .bind(address)
        .bind(name)
        .bind(note)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Remove a label override by address.
    pub async fn delete_pool_label(&self, address: &str) -> Result<(), DbError> {
        sqlx::query("DELETE FROM pool_labels WHERE address = ?1")
            .bind(address)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Insert a new payout attempt row in 'queued' state. Returns the row id.
    pub async fn create_payout_attempt(
        &self,
        miner_count: i64,
        total_zatoshis: i64,
        source: &str,
    ) -> Result<i64, DbError> {
        let r = sqlx::query(
            "INSERT INTO payout_attempts (status, miner_count, total_zatoshis, source)
             VALUES ('queued', ?1, ?2, ?3)",
        )
        .bind(miner_count)
        .bind(total_zatoshis)
        .bind(source)
        .execute(&self.pool)
        .await?;
        Ok(r.last_insert_rowid())
    }

    /// Check whether any payout attempt is currently in flight (queued or sent
    /// in the last 5 minutes). Used by the manual trigger to refuse running
    /// while the dashboard's 5-min loop has work in progress. Returns
    /// (id, status, source) if one exists.
    pub async fn get_inflight_payout_attempt(
        &self,
    ) -> Result<Option<(i64, String, String)>, DbError> {
        let row = sqlx::query(
            "SELECT id, status, source FROM payout_attempts
             WHERE status IN ('queued', 'submitting', 'sent')
               AND created_at > datetime('now', '-5 minutes')
             ORDER BY id DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| {
            (
                r.get::<i64, _>("id"),
                r.get::<String, _>("status"),
                r.get::<String, _>("source"),
            )
        }))
    }

    /// Update a payout attempt row with a new status, optionally setting opid/txid/error.
    pub async fn update_payout_attempt(
        &self,
        id: i64,
        status: &str,
        opid: Option<&str>,
        txid: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<(), DbError> {
        // Conventional PPS receipts have typed one-way transitions. Keep their
        // shared recovery metadata immutable through this legacy/general API.
        // The write lock also fences a concurrent reservation marker insertion.
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let conventional: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pps_conventional_attempts WHERE attempt_id=?1",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if conventional != 0 {
            return Err(DbError::Sqlx(sqlx::Error::Protocol(
                "Conventional PPS attempt requires its typed journal API".into(),
            )));
        }
        sqlx::query(
            "UPDATE payout_attempts
             SET status = ?1,
                 opid = COALESCE(?2, opid),
                 txid = COALESCE(?3, txid),
                 error_message = COALESCE(?4, error_message),
                 updated_at = datetime('now')
             WHERE id = ?5",
        )
        .bind(status)
        .bind(opid)
        .bind(txid)
        .bind(error_message)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Payout attempts stuck in 'queued' or 'sent' for longer than
    /// `stale_minutes` (audit P4). These are the residue of crashes or RPC
    /// failures between z_sendmany and the status poll — exactly the gap
    /// that produced unrecorded on-chain payouts on mainnet (2026-06-08).
    /// Returns (id, status, opid, txid, total_zatoshis, created_at).
    pub async fn get_stale_payout_attempts(
        &self,
        stale_minutes: i64,
    ) -> Result<Vec<(i64, String, Option<String>, Option<String>, i64, String)>, DbError> {
        let rows: Vec<SqliteRow> = sqlx::query(
            "SELECT id, status, opid, txid, total_zatoshis, created_at
             FROM payout_attempts
             WHERE status IN ('queued', 'submitting', 'sent')
               AND created_at < datetime('now', '-' || ?1 || ' minutes')
               AND NOT EXISTS (SELECT 1 FROM payout_items pi WHERE pi.attempt_id = payout_attempts.id)
               AND NOT EXISTS (SELECT 1 FROM pps_payout_items pi WHERE pi.attempt_id = payout_attempts.id)
               AND NOT EXISTS (SELECT 1 FROM pps_conventional_attempts ca WHERE ca.attempt_id = payout_attempts.id)
             ORDER BY id ASC",
        )
        .bind(stale_minutes)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| {
                (
                    r.get("id"),
                    r.get("status"),
                    r.get("opid"),
                    r.get("txid"),
                    r.get("total_zatoshis"),
                    r.get("created_at"),
                )
            })
            .collect())
    }

    /// Distinct txids referenced by payout rows created in the last
    /// `hours` hours, with the summed amount per txid (audit P4). Used to
    /// verify every recorded payout exists on chain (catches phantom rows
    /// like the 2026-03-16 id=9 incident, where a failed broadcast was
    /// recorded as paid).
    pub async fn get_recent_payout_txids(
        &self,
        hours: i64,
    ) -> Result<Vec<(String, i64)>, DbError> {
        let rows: Vec<SqliteRow> = sqlx::query(
            "SELECT txid, SUM(amount) as total
             FROM payouts
             WHERE created_at > datetime('now', '-' || ?1 || ' hours')
             GROUP BY txid",
        )
        .bind(hours)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| (r.get("txid"), r.get("total"))).collect())
    }

    /// Accounting invariant inputs (audit P4): the sum of credited block
    /// rewards should approximately equal the sum of all miner balances
    /// (pending + paid). Sustained drift beyond rounding indicates a
    /// crediting bug (e.g. the imprecise orphan reversal, Finding #4).
    ///
    /// Includes BOTH 'confirmed' and 'pending' (immature) blocks: credits
    /// are written at find time, ~maturity_confirmations before the block
    /// confirms, so a confirmed-only sum would show every fresh block as
    /// drift for ~2 hours. Orphaned blocks are excluded — their credits
    /// are reversed (and any unrecoverable remainder is tracked in
    /// orphan_clawbacks, returned separately so the invariant can treat it
    /// as explained drift rather than an anomaly).
    ///
    /// Reward basis is COALESCE(actual_reward, reward): post-Phase-C blocks
    /// distribute the actual coinbase value (subsidy + tx fees); historical
    /// rows fall back to the subsidy-only column.
    /// Returns (credited_reward_zatoshis, balances_total_zatoshis, clawback_zatoshis).
    pub async fn get_accounting_invariant(&self) -> Result<(i64, i64, i64), DbError> {
        let reward: (i64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(COALESCE(actual_reward, reward) - COALESCE(costs_recovered, 0)), 0) \
             FROM blocks WHERE status IN ('pending', 'confirmed') AND NOT EXISTS (SELECT 1 FROM pps_block_markers pb WHERE pb.block_id=blocks.id)",
        )
        .fetch_one(&self.pool)
        .await?;
        // Include `paying` (round-3 in-flight reservations): funds mid-payout
        // are moved pending -> paying -> paid, so all three buckets count toward
        // what the pool owes, or the invariant would show false drift whenever a
        // payout is reserved-but-not-yet-confirmed.
        let balances: (i64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(pending), 0) + COALESCE(SUM(paying), 0) + COALESCE(SUM(paid), 0) FROM balances",
        )
        .fetch_one(&self.pool)
        .await?;
        let clawbacks: (i64,) =
            sqlx::query_as("SELECT COALESCE(SUM(amount), 0) FROM orphan_clawbacks")
                .fetch_one(&self.pool)
                .await?;
        Ok((reward.0, balances.0, clawbacks.0))
    }

    /// Enable WAL mode for concurrent reads (dashboard) + single writer (pool).
    pub async fn set_wal_mode(&self) -> Result<(), DbError> {
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Upsert a key/value pair into pool_status.
    pub async fn set_pool_status(&self, key: &str, value: &str) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO pool_status (key, value, updated_at) VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = datetime('now')",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Read a value from pool_status. Returns (value, updated_at) if found.
    pub async fn get_pool_status(&self, key: &str) -> Result<Option<(String, String)>, DbError> {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT value, updated_at FROM pool_status WHERE key = ?1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    // -- Miners --

    pub async fn get_or_create_miner(&self, address: &str) -> Result<Miner, DbError> {
        sqlx::query("INSERT OR IGNORE INTO miners (address) VALUES (?1)")
            .bind(address)
            .execute(&self.pool)
            .await?;

        let miner: Miner = sqlx::query_as(
            "SELECT id, address, created_at FROM miners WHERE address = ?1",
        )
        .bind(address)
        .fetch_one(&self.pool)
        .await?;

        Ok(miner)
    }

    pub async fn get_miner_by_address(&self, address: &str) -> Result<Option<Miner>, DbError> {
        let miner: Option<Miner> = sqlx::query_as(
            "SELECT id, address, created_at FROM miners WHERE address = ?1",
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await?;

        Ok(miner)
    }

    // -- Workers --

    pub async fn get_or_create_worker(
        &self,
        miner_id: i64,
        name: &str,
    ) -> Result<Worker, DbError> {
        sqlx::query("INSERT OR IGNORE INTO workers (miner_id, name) VALUES (?1, ?2)")
            .bind(miner_id)
            .bind(name)
            .execute(&self.pool)
            .await?;

        sqlx::query(
            "UPDATE workers SET last_seen = datetime('now') WHERE miner_id = ?1 AND name = ?2",
        )
        .bind(miner_id)
        .bind(name)
        .execute(&self.pool)
        .await?;

        let worker: Worker = sqlx::query_as(
            "SELECT id, miner_id, name, last_seen, last_difficulty FROM workers WHERE miner_id = ?1 AND name = ?2",
        )
        .bind(miner_id)
        .bind(name)
        .fetch_one(&self.pool)
        .await?;

        Ok(worker)
    }

    pub async fn update_worker_difficulty(&self, worker_id: i64, difficulty: f64) -> Result<(), DbError> {
        sqlx::query("UPDATE workers SET last_difficulty = ?1 WHERE id = ?2")
            .bind(difficulty)
            .bind(worker_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Returns the miner_id that owns a given worker. Used by Solo reward mode
    /// to credit the block-finder.
    pub async fn get_miner_id_for_worker(&self, worker_id: i64) -> Result<i64, DbError> {
        let row = sqlx::query("SELECT miner_id FROM workers WHERE id = ?1")
            .bind(worker_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get::<i64, _>("miner_id"))
    }

    /// Workers for the public miner dashboard. Hides workers idle for over a
    /// week — long-gone rigs otherwise clutter the table forever (the admin
    /// diagnostics view intentionally still returns everything).
    pub async fn get_workers_for_miner(&self, miner_id: i64) -> Result<Vec<Worker>, DbError> {
        let workers: Vec<Worker> = sqlx::query_as(
            "SELECT id, miner_id, name, last_seen, last_difficulty FROM workers \
             WHERE miner_id = ?1 AND last_seen >= datetime('now', '-7 days')",
        )
        .bind(miner_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(workers)
    }

    // -- Shares --

    pub async fn record_share(
        &self,
        worker_id: i64,
        job_id: &str,
        difficulty: f64,
        is_block: bool,
        session_id: &str,
    ) -> Result<i64, DbError> {
        let is_block_int: i32 = if is_block { 1 } else { 0 };
        let result = sqlx::query(
            "INSERT INTO shares (worker_id, job_id, difficulty, is_block, session_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(worker_id)
        .bind(job_id)
        .bind(difficulty)
        .bind(is_block_int)
        .bind(session_id)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get the last N shares ordered by creation time (most recent first).
    pub async fn get_last_n_shares(&self, n: i64) -> Result<Vec<Share>, DbError> {
        let shares: Vec<Share> = sqlx::query_as(
            "SELECT id, worker_id, job_id, difficulty, is_block, created_at \
             FROM shares ORDER BY id DESC LIMIT ?1",
        )
        .bind(n)
        .fetch_all(&self.pool)
        .await?;

        Ok(shares)
    }

    pub async fn get_total_shares_count(&self) -> Result<i64, DbError> {
        // Audit #16: all-time total = archived hourly rollup + remaining raw
        // rows. Exact across retention pruning, and no all-time table scan.
        let row: (i64,) = sqlx::query_as(
            "SELECT (SELECT COALESCE(SUM(cnt), 0) FROM shares_rollup) + \
                    (SELECT COUNT(*) FROM shares)",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Count of shares in a time window (each valid Equihash solution = 1 Sol).
    pub async fn get_shares_count_since(&self, since: &str) -> Result<f64, DbError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM shares WHERE created_at >= ?1",
        )
        .bind(since)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0 as f64)
    }

    /// Sum of difficulty of all shares in a time window.
    pub async fn get_difficulty_sum_since(&self, since: &str) -> Result<f64, DbError> {
        let row: (f64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(difficulty), 0.0) FROM shares WHERE created_at >= ?1",
        )
        .bind(since)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    // -- Blocks --

    pub async fn record_block(
        &self,
        height: i64,
        hash: &str,
        reward: i64,
        actual_reward: Option<i64>,
        found_by: i64,
        luck_percent: Option<f64>,
    ) -> Result<i64, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let result = sqlx::query(
            "INSERT INTO blocks (height, hash, reward, actual_reward, found_by, luck_percent) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(height)
        .bind(hash)
        .bind(reward)
        .bind(actual_reward)
        .bind(found_by)
        .bind(luck_percent)
        .execute(&mut *tx)
        .await?;
        let block_id = result.last_insert_rowid();
        sqlx::query("INSERT INTO pps_block_markers(block_id,epoch_id) SELECT ?1,active_epoch FROM pps_meta WHERE singleton=1")
            .bind(block_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(block_id)
    }

    /// Atomically record a block's per-miner credits AND apply them to
    /// pending balances (audit P5 + NEW-E). Either every credit lands or
    /// none do — no more partial distributions when a write fails mid-loop,
    /// and orphan reversal can later debit exactly these rows.
    pub async fn distribute_block_credits(
        &self,
        block_id: i64,
        credits: &[(i64, i64)], // (miner_id, amount_zatoshis)
    ) -> Result<(), DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let pps: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pps_block_markers WHERE block_id=?1").bind(block_id).fetch_one(&mut *tx).await?;
        if pps != 0 { return Err(DbError::Sqlx(sqlx::Error::Protocol("PPS block cannot receive legacy credits".into()))); }
        for (miner_id, amount) in credits {
            sqlx::query("INSERT OR IGNORE INTO balances (miner_id) VALUES (?1)")
                .bind(miner_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("INSERT INTO block_credits (block_id, miner_id, amount) VALUES (?1, ?2, ?3)")
                .bind(block_id)
                .bind(miner_id)
                .bind(amount)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE balances SET pending = pending + ?1 WHERE miner_id = ?2")
                .bind(amount)
                .bind(miner_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Precise orphan reversal (audit P5): debit exactly the miners that
    /// were credited for `block_id`, by exactly their credited amounts.
    /// Credits that can't be recovered from pending (already paid out) are
    /// recorded in `orphan_clawbacks` instead of being clawed from other
    /// miners. Returns (reversed_zatoshis, clawback_zatoshis).
    ///
    /// Blocks recorded before migration 008 have no block_credits rows;
    /// the caller should fall back to the legacy proportional reversal.
    pub async fn reverse_block_credits_precise(
        &self,
        block_id: i64,
    ) -> Result<Option<(i64, i64)>, DbError> {
        // Read-then-write: take the write lock UP FRONT with BEGIN IMMEDIATE. A
        // plain DEFERRED tx (self.pool.begin()) reads first and pins a WAL
        // snapshot; if the pool's block-find writer commits before our first
        // UPDATE, the write fails with SQLITE_BUSY_SNAPSHOT — which busy_timeout
        // does NOT retry. Holding the write lock before the first read makes the
        // whole transaction see one consistent, exclusive snapshot.
        let mut conn = self.pool.acquire().await?;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
        match Self::reverse_block_credits_precise_txn(&mut *conn, block_id).await {
            Ok(v) => match sqlx::query("COMMIT").execute(&mut *conn).await {
                Ok(_) => Ok(v),
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(e.into())
                }
            },
            Err(e) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                Err(e)
            }
        }
    }

    /// Body of `reverse_block_credits_precise`, run inside the BEGIN IMMEDIATE
    /// transaction held on `conn` so reads and writes share one exclusive
    /// snapshot (no BUSY_SNAPSHOT window between the SELECT and the UPDATEs).
    async fn reverse_block_credits_precise_txn(
        conn: &mut sqlx::SqliteConnection,
        block_id: i64,
    ) -> Result<Option<(i64, i64)>, DbError> {
        let pps: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pps_block_markers WHERE block_id=?1").bind(block_id).fetch_one(&mut *conn).await?;
        if pps != 0 { return Ok(Some((0, 0))); }
        let credits: Vec<(i64, i64)> = sqlx::query(
            "SELECT miner_id, amount FROM block_credits WHERE block_id = ?1",
        )
        .bind(block_id)
        .fetch_all(&mut *conn)
        .await?
        .iter()
        .map(|r| (r.get("miner_id"), r.get("amount")))
        .collect();

        if credits.is_empty() {
            return Ok(None);
        }

        let mut reversed: i64 = 0;
        let mut clawback: i64 = 0;
        for (miner_id, amount) in credits {
            let pending: (i64,) =
                sqlx::query_as("SELECT COALESCE(pending, 0) FROM balances WHERE miner_id = ?1")
                    .bind(miner_id)
                    .fetch_optional(&mut *conn)
                    .await?
                    .unwrap_or((0,));
            let recoverable = amount.min(pending.0).max(0);
            let shortfall = amount - recoverable;
            if recoverable > 0 {
                sqlx::query("UPDATE balances SET pending = pending - ?1 WHERE miner_id = ?2")
                    .bind(recoverable)
                    .bind(miner_id)
                    .execute(&mut *conn)
                    .await?;
                reversed += recoverable;
            }
            if shortfall > 0 {
                sqlx::query(
                    "INSERT INTO orphan_clawbacks (block_id, miner_id, amount) VALUES (?1, ?2, ?3)",
                )
                .bind(block_id)
                .bind(miner_id)
                .bind(shortfall)
                .execute(&mut *conn)
                .await?;
                clawback += shortfall;
            }
        }
        // Remove the credit rows so a double-reversal is impossible.
        sqlx::query("DELETE FROM block_credits WHERE block_id = ?1")
            .bind(block_id)
            .execute(&mut *conn)
            .await?;
        // The costs this block's distribution absorbed were real (the
        // wallet paid them) but the covering reward is gone — re-queue
        // them for the next block's distribution.
        let recovered: (i64,) =
            sqlx::query_as("SELECT COALESCE(costs_recovered, 0) FROM blocks WHERE id = ?1")
                .bind(block_id)
                .fetch_one(&mut *conn)
                .await?;
        if recovered.0 > 0 {
            sqlx::query(
                "INSERT INTO pool_tx_costs (kind, ref, fee) VALUES ('reorphaned', 'block:' || ?1, ?2)",
            )
            .bind(block_id)
            .bind(recovered.0)
            .execute(&mut *conn)
            .await?;
            sqlx::query("UPDATE blocks SET costs_recovered = 0 WHERE id = ?1")
                .bind(block_id)
                .execute(&mut *conn)
                .await?;
        }
        Ok(Some((reversed, clawback)))
    }

    /// Clawbacks recorded in the last `hours` hours (for reconciler alerts).
    pub async fn get_recent_clawbacks(
        &self,
        hours: i64,
    ) -> Result<Vec<(i64, i64, i64)>, DbError> {
        let rows: Vec<SqliteRow> = sqlx::query(
            "SELECT block_id, miner_id, amount FROM orphan_clawbacks
             WHERE created_at > datetime('now', '-' || ?1 || ' hours')
               AND acknowledged = 0",
        )
        .bind(hours)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| (r.get("block_id"), r.get("miner_id"), r.get("amount")))
            .collect())
    }

    /// Record the operator's decision for all unacknowledged clawbacks.
    pub async fn acknowledge_clawbacks(&self, note: &str) -> Result<u64, DbError> {
        let r = sqlx::query(
            "UPDATE orphan_clawbacks SET acknowledged = 1, ack_note = ?1 WHERE acknowledged = 0",
        )
        .bind(note)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected())
    }

    /// Acknowledge only the clawbacks of one block (immediate-payout policy:
    /// the reserve absorbs that block's orphan loss automatically; other
    /// blocks' clawbacks still need an operator decision).
    pub async fn acknowledge_clawbacks_for_block(
        &self,
        block_id: i64,
        note: &str,
    ) -> Result<u64, DbError> {
        let r = sqlx::query(
            "UPDATE orphan_clawbacks SET acknowledged = 1, ack_note = ?1 \
             WHERE acknowledged = 0 AND block_id = ?2",
        )
        .bind(note)
        .bind(block_id)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected())
    }

    /// Total credits sitting on still-pending (immature) blocks. Upper bound
    /// on what the reserve could lose if every pending block orphaned after
    /// being paid immediately — the immediate-payout safeguard compares this
    /// against reserve_min before skipping the maturity gate.
    pub async fn get_immature_exposure(&self) -> Result<i64, DbError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(bc.amount), 0) FROM block_credits bc \
             JOIN blocks bl ON bc.block_id = bl.id WHERE bl.status = 'pending'",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Record a pipeline tx cost (ZIP-317 fee the pool wallet paid for a
    /// shield or payout tx). Recovered from a future block's distribution.
    pub async fn record_tx_cost(&self, kind: &str, reference: &str, fee: i64) -> Result<(), DbError> {
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("INSERT INTO pool_tx_costs (kind, ref, fee) VALUES (?1, ?2, ?3)")
            .bind(kind)
            .bind(reference)
            .bind(fee)
            .execute(&mut *tx)
            .await?;
        crate::pps_funding::bump_generation(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Consume unrecovered pipeline costs for a block's distribution, up to
    /// `cap` zatoshis (whole rows only, oldest first). Marks the rows
    /// recovered and stamps the total on the block. Returns the amount the
    /// distribution should deduct. One transaction.
    pub async fn take_costs_for_block(&self, block_id: i64, cap: i64) -> Result<i64, DbError> {
        // Read-then-write: BEGIN IMMEDIATE takes the write lock before the initial
        // SELECT so a concurrent writer can't invalidate our snapshot before the
        // UPDATEs (SQLITE_BUSY_SNAPSHOT, which busy_timeout does not retry).
        let mut conn = self.pool.acquire().await?;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
        match Self::take_costs_for_block_txn(&mut *conn, block_id, cap).await {
            Ok(v) => match sqlx::query("COMMIT").execute(&mut *conn).await {
                Ok(_) => Ok(v),
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(e.into())
                }
            },
            Err(e) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                Err(e)
            }
        }
    }

    /// Body of `take_costs_for_block`, run inside the BEGIN IMMEDIATE transaction
    /// held on `conn`.
    async fn take_costs_for_block_txn(
        conn: &mut sqlx::SqliteConnection,
        block_id: i64,
        cap: i64,
    ) -> Result<i64, DbError> {
        let rows: Vec<(i64, i64)> = sqlx::query(
            "SELECT id, fee FROM pool_tx_costs WHERE recovered = 0 ORDER BY id ASC",
        )
        .fetch_all(&mut *conn)
        .await?
        .iter()
        .map(|r| (r.get("id"), r.get("fee")))
        .collect();

        let mut taken: i64 = 0;
        let mut ids: Vec<i64> = Vec::new();
        for (id, fee) in rows {
            if taken + fee > cap {
                break;
            }
            taken += fee;
            ids.push(id);
        }
        for id in &ids {
            sqlx::query("UPDATE pool_tx_costs SET recovered = 1 WHERE id = ?1")
                .bind(id)
                .execute(&mut *conn)
                .await?;
        }
        if taken > 0 {
            sqlx::query("UPDATE blocks SET costs_recovered = ?1 WHERE id = ?2")
                .bind(taken)
                .bind(block_id)
                .execute(&mut *conn)
                .await?;
        }
        Ok(taken)
    }

    // -- Audit #15: block-found durability (breadcrumb + startup sweep) --

    /// Durable breadcrumb written BEFORE submitblock so a crash between submit
    /// and record_block can be recovered by the startup sweep.
    pub async fn record_block_submission(
        &self,
        height: i64,
        hash: &str,
        worker_id: i64,
        reward: i64,
        actual_reward: Option<i64>,
    ) -> Result<i64, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let r = sqlx::query(
            "INSERT INTO block_submissions (height, hash, worker_id, reward, actual_reward) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(height)
        .bind(hash)
        .bind(worker_id)
        .bind(reward)
        .bind(actual_reward)
        .execute(&mut *tx)
        .await?;
        let submission_id = r.last_insert_rowid();
        sqlx::query("INSERT INTO pps_submission_markers(submission_id,epoch_id) SELECT ?1,active_epoch FROM pps_meta WHERE singleton=1")
            .bind(submission_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(submission_id)
    }

    /// Settle a breadcrumb once the block's fate is known.
    pub async fn resolve_block_submission(&self, id: i64, outcome: &str) -> Result<(), DbError> {
        sqlx::query("UPDATE block_submissions SET resolved = 1, outcome = ?1 WHERE id = ?2")
            .bind(outcome)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Breadcrumbs whose fate was never settled (crash between submit and
    /// record). Returns (id, height, hash, worker_id, reward, actual_reward).
    pub async fn get_open_block_submissions(
        &self,
    ) -> Result<Vec<(i64, i64, String, i64, i64, Option<i64>)>, DbError> {
        let rows: Vec<(i64, i64, String, i64, i64, Option<i64>)> = sqlx::query_as(
            "SELECT id, height, hash, worker_id, reward, actual_reward \
             FROM block_submissions WHERE resolved = 0 ORDER BY id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Block id for an exact stored hash, if recorded.
    pub async fn get_block_id_by_hash(&self, hash: &str) -> Result<Option<i64>, DbError> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM blocks WHERE hash = ?1")
            .bind(hash)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.0))
    }

    /// Recent non-orphaned blocks with ZERO credit rows — the block-90 failure
    /// class (distribution swallowed after recording). The startup sweep
    /// redistributes them. Recency bound keeps ancient pre-008 blocks out.
    /// Returns (block_id, height, distribution_basis, found_by_worker_id).
    pub async fn get_recent_blocks_missing_credits(
        &self,
        days: i64,
    ) -> Result<Vec<(i64, i64, i64, i64)>, DbError> {
        let rows: Vec<(i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT b.id, b.height, COALESCE(b.actual_reward, b.reward), b.found_by \
             FROM blocks b \
             WHERE b.status IN ('pending', 'confirmed') \
               AND b.created_at > datetime('now', '-' || ?1 || ' days') \
               AND NOT EXISTS (SELECT 1 FROM block_credits bc WHERE bc.block_id = b.id) \
               AND NOT EXISTS (SELECT 1 FROM pps_block_markers pb WHERE pb.block_id = b.id) \
             ORDER BY b.id ASC",
        )
        .bind(days)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Bump a worker's last_seen (audit #15: called on a ~30s cadence from the
    /// validator's per-session cache instead of once per share).
    pub async fn touch_worker(&self, worker_id: i64) -> Result<(), DbError> {
        sqlx::query("UPDATE workers SET last_seen = datetime('now') WHERE id = ?1")
            .bind(worker_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Confirmed blocks strictly above `height`, ascending, capped at `limit`.
    /// Used by the reconciler's coinbase-output check to walk forward from its
    /// persisted watermark. Returns (height, hash).
    pub async fn get_confirmed_blocks_above(
        &self,
        height: i64,
        limit: i64,
    ) -> Result<Vec<(i64, String)>, DbError> {
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT height, hash FROM blocks WHERE status = 'confirmed' AND height > ?1 \
             ORDER BY height ASC LIMIT ?2",
        )
        .bind(height)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get_recent_blocks(&self, limit: i64) -> Result<Vec<Block>, DbError> {
        let blocks: Vec<Block> = sqlx::query_as(
            "SELECT id, height, hash, reward, status, found_by, created_at, luck_percent \
             FROM blocks ORDER BY id DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(blocks)
    }

    /// Count shares submitted between two timestamps.
    pub async fn get_shares_count_between(&self, from: &str, to: &str) -> Result<i64, DbError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM shares WHERE created_at > ?1 AND created_at <= ?2",
        )
        .bind(from)
        .bind(to)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Sum of difficulty of shares submitted between two timestamps.
    pub async fn get_difficulty_sum_between(&self, from: &str, to: &str) -> Result<f64, DbError> {
        let row: (f64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(difficulty), 0.0) FROM shares WHERE created_at > ?1 AND created_at <= ?2",
        )
        .bind(from)
        .bind(to)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    pub async fn get_blocks_count(&self) -> Result<i64, DbError> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM blocks")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    /// Count blocks found since a given datetime string (e.g. "2026-02-19 12:00:00").
    pub async fn get_blocks_count_since(&self, since: &str) -> Result<i64, DbError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM blocks WHERE created_at >= ?1",
        )
        .bind(since)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    pub async fn update_block_luck(&self, block_id: i64, luck_percent: f64) -> Result<(), DbError> {
        sqlx::query("UPDATE blocks SET luck_percent = ?1 WHERE id = ?2")
            .bind(luck_percent)
            .bind(block_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Get all blocks ordered by height ascending (for backfill).
    pub async fn get_all_blocks_by_height(&self) -> Result<Vec<Block>, DbError> {
        let blocks: Vec<Block> = sqlx::query_as(
            "SELECT id, height, hash, reward, status, found_by, created_at, luck_percent \
             FROM blocks ORDER BY height ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(blocks)
    }

    pub async fn update_block_status(&self, block_id: i64, status: &str) -> Result<(), DbError> {
        sqlx::query("UPDATE blocks SET status = ?1 WHERE id = ?2")
            .bind(status)
            .bind(block_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Get all blocks with status 'pending' (not yet confirmed or orphaned).
    pub async fn get_pending_blocks(&self) -> Result<Vec<Block>, DbError> {
        let blocks: Vec<Block> = sqlx::query_as(
            "SELECT id, height, hash, reward, status, found_by, created_at, luck_percent \
             FROM blocks WHERE status = 'pending' ORDER BY height ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(blocks)
    }

    /// Count blocks with status 'pending' (immature, waiting for 100 confirmations).
    pub async fn get_immature_blocks_count(&self) -> Result<i64, DbError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM blocks WHERE status = 'pending'",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Count blocks with status 'confirmed' (mature, shielded, awaiting payout).
    pub async fn get_pending_payout_blocks_count(&self) -> Result<i64, DbError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM blocks WHERE status = 'confirmed'",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Average luck_percent across all blocks that have a recorded value (lifetime pool luck).
    /// Returns None if no blocks have luck recorded.
    pub async fn get_lifetime_luck(&self) -> Result<Option<f64>, DbError> {
        let row: (Option<f64>,) = sqlx::query_as(
            "SELECT AVG(luck_percent) FROM blocks WHERE luck_percent IS NOT NULL",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Reverse PPLNS credits for an orphaned block by subtracting reward from pending balances.
    /// Atomically orphan a block: reverse its credits AND flip status to
    /// 'orphaned' in ONE BEGIN IMMEDIATE transaction (audit #14 — the old
    /// order, status first / reversal second as two calls, left permanent
    /// phantom credits if the process died between them). Precise reversal
    /// when block_credits rows exist; legacy proportional fallback for
    /// pre-migration-008 blocks. Returns Some((reversed, clawback)) for
    /// precise, None when the legacy fallback ran.
    pub async fn orphan_block(
        &self,
        block_id: i64,
        block_reward: i64,
    ) -> Result<Option<(i64, i64)>, DbError> {
        let mut conn = self.pool.acquire().await?;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
        match Self::orphan_block_txn(&mut *conn, block_id, block_reward).await {
            Ok(v) => match sqlx::query("COMMIT").execute(&mut *conn).await {
                Ok(_) => Ok(v),
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(e.into())
                }
            },
            Err(e) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                Err(e)
            }
        }
    }

    async fn orphan_block_txn(
        conn: &mut sqlx::SqliteConnection,
        block_id: i64,
        block_reward: i64,
    ) -> Result<Option<(i64, i64)>, DbError> {
        let outcome = Self::reverse_block_credits_precise_txn(&mut *conn, block_id).await?;
        if outcome.is_none() {
            // Pre-008 block: no credit rows. Legacy proportional reversal,
            // inside the SAME transaction as the status flip.
            let total_pending: (i64,) = sqlx::query_as(
                "SELECT COALESCE(SUM(pending), 0) FROM balances WHERE pending > 0",
            )
            .fetch_one(&mut *conn)
            .await?;
            if total_pending.0 > 0 {
                let reversal = block_reward.min(total_pending.0);
                sqlx::query(
                    "UPDATE balances SET pending = MAX(0, pending - CAST(pending * 1.0 * ?1 / ?2 AS INTEGER)) \
                     WHERE pending > 0",
                )
                .bind(reversal)
                .bind(total_pending.0)
                .execute(&mut *conn)
                .await?;
            }
        }
        sqlx::query("UPDATE blocks SET status = 'orphaned' WHERE id = ?1")
            .bind(block_id)
            .execute(&mut *conn)
            .await?;
        Ok(outcome)
    }

    /// Legacy proportional reversal (pre-008 blocks). Prefer `orphan_block`,
    /// which wraps this logic atomically with the status flip.
    pub async fn reverse_block_credits(&self, block_reward: i64) -> Result<(), DbError> {
        // Distribute the reversal proportionally across all miners with pending balance
        let total_pending: (i64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(pending), 0) FROM balances WHERE pending > 0",
        )
        .fetch_one(&self.pool)
        .await?;

        if total_pending.0 <= 0 { return Ok(()); }

        // Cap the reversal at the total pending to avoid negative balances
        let reversal = block_reward.min(total_pending.0);

        // Proportionally reduce each miner's pending balance
        sqlx::query(
            "UPDATE balances SET pending = MAX(0, pending - CAST(pending * 1.0 * ?1 / ?2 AS INTEGER)) \
             WHERE pending > 0",
        )
        .bind(reversal)
        .bind(total_pending.0)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // -- Balances --

    pub async fn get_or_create_balance(&self, miner_id: i64) -> Result<Balance, DbError> {
        sqlx::query("INSERT OR IGNORE INTO balances (miner_id) VALUES (?1)")
            .bind(miner_id)
            .execute(&self.pool)
            .await?;

        let balance: Balance = sqlx::query_as(
            "SELECT miner_id, pending, paid FROM balances WHERE miner_id = ?1",
        )
        .bind(miner_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(balance)
    }

    pub async fn credit_balance(&self, miner_id: i64, amount: i64) -> Result<(), DbError> {
        sqlx::query("INSERT OR IGNORE INTO balances (miner_id) VALUES (?1)")
            .bind(miner_id)
            .execute(&self.pool)
            .await?;

        sqlx::query("UPDATE balances SET pending = pending + ?1 WHERE miner_id = ?2")
            .bind(amount)
            .bind(miner_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // -- Payouts --

    pub async fn get_recent_payouts(&self, limit: i64) -> Result<Vec<Payout>, DbError> {
        let payouts: Vec<Payout> = sqlx::query_as(
            "SELECT id, miner_id, txid, amount, created_at FROM payouts UNION ALL SELECT id, miner_id, txid, amount, created_at FROM pps_payouts ORDER BY created_at DESC, id DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(payouts)
    }

    pub async fn get_miner_address(&self, miner_id: i64) -> Result<String, DbError> {
        let row: (String,) = sqlx::query_as(
            "SELECT address FROM miners WHERE id = ?1",
        )
        .bind(miner_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Find miners with PAYABLE balance >= min_amount (in zatoshis).
    ///
    /// Payable = pending MINUS credits attached to blocks that haven't
    /// matured yet (operator policy 2026-06-10: never pay out a block's
    /// credits before the block confirms). When the block confirms, its
    /// credits become payable automatically; when it orphans, the precise
    /// reversal removes them from pending in full — which makes orphan
    /// clawbacks structurally impossible short of a >maturity-depth reorg.
    /// Credits from pre-008 blocks have no ledger rows and count as
    /// payable (those blocks are long confirmed).
    /// With `include_immature` (faucet-style immediate payouts, underwritten
    /// by the operator's reserve), the maturity gate is skipped and the full
    /// pending balance is payable at find time. The caller is responsible
    /// for the reserve safeguard (see `get_immature_exposure`).
    /// `cooldown_secs`/`override_amount` implement payout coalescing (#21):
    /// with a positive cooldown, a miner paid more recently than that many
    /// seconds ago is skipped this round — unless their payable balance is
    /// at or above `override_amount` (pass i64::MAX for "no override").
    /// Skipped funds simply stay in `pending`; no invariant is affected.
    pub async fn get_pending_payouts(
        &self,
        min_amount: i64,
        include_immature: bool,
        cooldown_secs: i64,
        override_amount: i64,
    ) -> Result<Vec<PendingPayout>, DbError> {
        let coalesce = " AND ( ?2 <= 0 OR t.payable >= ?3 OR NOT EXISTS ( \
                 SELECT 1 FROM payouts p \
                 WHERE p.miner_id = t.miner_id \
                   AND p.created_at >= datetime('now', '-' || ?2 || ' seconds') \
             )) ORDER BY t.payable DESC";
        let sql = if include_immature {
            format!(
                "SELECT * FROM ( \
                     SELECT b.miner_id, m.address, m.created_at, b.pending AS payable \
                     FROM balances b \
                     JOIN miners m ON m.id = b.miner_id \
                 ) AS t WHERE t.payable >= ?1{coalesce}"
            )
        } else {
            format!(
                "SELECT * FROM ( \
                     SELECT b.miner_id, m.address, m.created_at, \
                            b.pending - COALESCE(( \
                                SELECT SUM(bc.amount) FROM block_credits bc \
                                JOIN blocks bl ON bc.block_id = bl.id \
                                WHERE bc.miner_id = b.miner_id AND bl.status = 'pending' \
                            ), 0) AS payable \
                     FROM balances b \
                     JOIN miners m ON m.id = b.miner_id \
                 ) AS t WHERE t.payable >= ?1{coalesce}"
            )
        };
        let rows: Vec<SqliteRow> = sqlx::query(&sql)
            .bind(min_amount)
            .bind(cooldown_secs)
            .bind(override_amount)
            .fetch_all(&self.pool)
            .await?;

        let entries = rows
            .iter()
            .map(|row| PendingPayout {
                miner_id: row.get("miner_id"),
                address: row.get("address"),
                amount: row.get("payable"),
                created_at: row.get("created_at"),
            })
            .collect();

        Ok(entries)
    }

    /// Record a payout and move the amount from pending to paid.
    pub async fn create_payout(
        &self,
        miner_id: i64,
        amount: i64,
        txid: &str,
    ) -> Result<i64, DbError> {
        // Atomic + guarded. The debit runs first, guarded by `pending >= amount`
        // so it can never drive pending negative (double-debit); the payout row
        // is inserted in the SAME transaction so a crash between them can't leave
        // pending un-debited (which would re-pay next round — the documented
        // 2026-06-11 double-pay class). If the guard matches 0 rows the balance
        // is already settled: roll back and surface InsufficientPending rather
        // than record a phantom payout.
        let mut tx = self.pool.begin().await?;

        let debit = sqlx::query(
            "UPDATE balances SET pending = pending - ?1, paid = paid + ?1 \
             WHERE miner_id = ?2 AND pending >= ?1",
        )
        .bind(amount)
        .bind(miner_id)
        .execute(&mut *tx)
        .await?;

        if debit.rows_affected() == 0 {
            tx.rollback().await?;
            return Err(DbError::InsufficientPending { miner_id, amount });
        }

        let result = sqlx::query(
            "INSERT INTO payouts (miner_id, txid, amount) VALUES (?1, ?2, ?3)",
        )
        .bind(miner_id)
        .bind(txid)
        .bind(amount)
        .execute(&mut *tx)
        .await?;

        crate::pps_funding::bump_generation(&mut tx).await?;
        tx.commit().await?;
        Ok(result.last_insert_rowid())
    }

    /// Audit #19: auto-void a recorded payout whose tx was reorged out and
    /// expired (node authoritatively answered -5). Miners were never actually
    /// paid — the wallet kept the funds — so the books are corrected: paid ->
    /// pending per miner, payout rows deleted, matching attempt marked failed.
    /// The next payout cycle re-pays automatically.
    ///
    /// Guards (conservative by construction):
    ///  - every payout row for the txid must be older than 60 minutes (past
    ///    tx-expiry; a merely-slow tx can never be voided) — else Ok((0,0));
    ///  - the per-miner debit requires `paid >= amount` (never drives paid
    ///    negative) — a guard miss rolls the whole void back.
    /// One BEGIN IMMEDIATE transaction; idempotent (second call sees no rows).
    /// Returns (rows_voided, zatoshis_returned_to_pending).
    pub async fn void_reorged_payout(&self, txid: &str) -> Result<(i64, i64), DbError> {
        let mut conn = self.pool.acquire().await?;
        let mut tx = conn.begin_with("BEGIN IMMEDIATE").await?;
        let result = Self::void_reorged_payout_txn(&mut tx, txid).await?;
        tx.commit().await?;
        Ok(result)
    }

    async fn void_reorged_payout_txn(
        conn: &mut sqlx::SqliteConnection,
        txid: &str,
    ) -> Result<(i64, i64), DbError> {
        // Age guard: any row younger than 60 min -> not eligible yet.
        let fresh: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM payouts WHERE txid = ?1 \
             AND created_at > datetime('now', '-60 minutes')",
        )
        .bind(txid)
        .fetch_one(&mut *conn)
        .await?;
        if fresh.0 > 0 {
            return Ok((0, 0));
        }
        let rows: Vec<(i64, i64, i64)> = sqlx::query_as(
            "SELECT id, miner_id, amount FROM payouts WHERE txid = ?1",
        )
        .bind(txid)
        .fetch_all(&mut *conn)
        .await?;
        if rows.is_empty() {
            return Ok((0, 0));
        }
        let mut zats = 0i64;
        for (_id, miner_id, amount) in &rows {
            let upd = sqlx::query(
                "UPDATE balances SET paid = paid - ?1, pending = pending + ?1 \
                 WHERE miner_id = ?2 AND paid >= ?1",
            )
            .bind(amount)
            .bind(miner_id)
            .execute(&mut *conn)
            .await?;
            if upd.rows_affected() == 0 {
                // paid < amount: books don't support this void — abort whole tx.
                return Err(DbError::Sqlx(sqlx::Error::Protocol(format!(
                    "auto-void aborted: miner {miner_id} paid < {amount} for txid {txid}"
                ))));
            }
            zats += amount;
        }
        sqlx::query("DELETE FROM payouts WHERE txid = ?1")
            .bind(txid)
            .execute(&mut *conn)
            .await?;
        sqlx::query(
            "UPDATE payout_attempts SET status = 'failed', updated_at = datetime('now'), \
             error_message = 'auto-void: tx reorged out and expired (-5); funds returned to pending' \
             WHERE txid = ?1 AND status = 'confirmed' AND NOT EXISTS \
             (SELECT 1 FROM pps_conventional_attempts ca WHERE ca.attempt_id=payout_attempts.id)",
        )
        .bind(txid)
        .execute(&mut *conn)
        .await?;
        crate::pps_funding::bump_generation(conn).await?;
        Ok((rows.len() as i64, zats))
    }

    // -- Round-3 pre-debit payout saga (reserve -> confirm | refund) --
    //
    // Replaces the "debit in create_payout AFTER the send" flow. The payout loop
    // RESERVES funds (pending -> paying) before z_sendmany, then CONFIRMS
    // (paying -> paid) once the tx is on chain, or REFUNDS (paying -> pending) if
    // it never broadcast. A crash between reserve and confirm leaves the funds in
    // `paying` (out of `pending`), so the loop can't re-select them; startup
    // reconciliation resolves the in-flight attempt from its on-chain txid.

    /// Reserve funds for an in-flight payout: for each (miner_id, amount) move
    /// `pending -> paying` (guarded by `pending >= amount`) and record a
    /// payout_items row under `attempt_id`. Miners whose pending changed since
    /// selection (guard matches 0 rows) are skipped. Returns the pairs actually
    /// reserved so the caller sends to exactly those. One BEGIN IMMEDIATE tx.
    pub async fn reserve_payout(
        &self,
        attempt_id: i64,
        items: &[(i64, i64)],
    ) -> Result<Vec<(i64, i64)>, DbError> {
        let mut conn = self.pool.acquire().await?;
        let mut tx = conn.begin_with("BEGIN IMMEDIATE").await?;
        let result = Self::reserve_payout_txn(&mut tx, attempt_id, items).await?;
        tx.commit().await?;
        Ok(result)
    }

    async fn reserve_payout_txn(
        conn: &mut sqlx::SqliteConnection,
        attempt_id: i64,
        items: &[(i64, i64)],
    ) -> Result<Vec<(i64, i64)>, DbError> {
        let pps:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM pps_payout_items WHERE attempt_id=?1)+(SELECT COUNT(*) FROM pps_payouts WHERE attempt_id=?1)+(SELECT COUNT(*) FROM pps_conventional_attempts WHERE attempt_id=?1)").bind(attempt_id).fetch_one(&mut *conn).await?;
        if pps!=0{return Err(DbError::Sqlx(sqlx::Error::Protocol("PPS attempt cannot enter legacy payout saga".into())))}
        let mut reserved: Vec<(i64, i64)> = Vec::new();
        for &(miner_id, amount) in items {
            if amount <= 0 {
                continue;
            }
            let debit = sqlx::query(
                "UPDATE balances SET pending = pending - ?1, paying = paying + ?1 \
                 WHERE miner_id = ?2 AND pending >= ?1",
            )
            .bind(amount)
            .bind(miner_id)
            .execute(&mut *conn)
            .await?;
            if debit.rows_affected() == 0 {
                // pending fell below `amount` since selection — skip rather than
                // drive pending negative; the miner is picked up next cycle.
                continue;
            }
            sqlx::query(
                "INSERT INTO payout_items (attempt_id, miner_id, amount) VALUES (?1, ?2, ?3)",
            )
            .bind(attempt_id)
            .bind(miner_id)
            .bind(amount)
            .execute(&mut *conn)
            .await?;
            reserved.push((miner_id, amount));
        }
        if !reserved.is_empty() { crate::pps_funding::bump_generation(conn).await?; }
        Ok(reserved)
    }

    /// Finalize a confirmed on-chain payout: for each reserved item of
    /// `attempt_id` move `paying -> paid` and insert a `payouts` row with `txid`,
    /// then delete the items so a re-run (startup reconciliation) is a no-op.
    /// Returns the number of items finalized. One BEGIN IMMEDIATE tx.
    pub async fn confirm_payout(&self, attempt_id: i64, txid: &str) -> Result<i64, DbError> {
        let mut conn = self.pool.acquire().await?;
        let mut tx = conn.begin_with("BEGIN IMMEDIATE").await?;
        let result = Self::confirm_payout_txn(&mut tx, attempt_id, txid).await?;
        tx.commit().await?;
        Ok(result)
    }

    async fn confirm_payout_txn(
        conn: &mut sqlx::SqliteConnection,
        attempt_id: i64,
        txid: &str,
    ) -> Result<i64, DbError> {
        let pps:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM pps_payout_items WHERE attempt_id=?1)+(SELECT COUNT(*) FROM pps_payouts WHERE attempt_id=?1)+(SELECT COUNT(*) FROM pps_conventional_attempts WHERE attempt_id=?1)").bind(attempt_id).fetch_one(&mut *conn).await?;
        if pps!=0{return Err(DbError::Sqlx(sqlx::Error::Protocol("PPS attempt cannot enter legacy payout saga".into())))}
        let items: Vec<(i64, i64)> = sqlx::query(
            "SELECT miner_id, amount FROM payout_items WHERE attempt_id = ?1",
        )
        .bind(attempt_id)
        .fetch_all(&mut *conn)
        .await?
        .iter()
        .map(|r| (r.get("miner_id"), r.get("amount")))
        .collect();

        let mut finalized: i64 = 0;
        for (miner_id, amount) in &items {
            let upd = sqlx::query(
                "UPDATE balances SET paying = paying - ?1, paid = paid + ?1 \
                 WHERE miner_id = ?2 AND paying >= ?1",
            )
            .bind(amount)
            .bind(miner_id)
            .execute(&mut *conn)
            .await?;
            if upd.rows_affected() == 0 {
                // paying < amount. Within this design that state is unreachable
                // (reserve guards pending>=amt; only confirm/refund consume
                // paying, both all-or-nothing; a completed prior confirm deleted
                // the items so a re-run sees none) — so it can only mean external
                // corruption. Perfect-accounting policy: refuse to settle
                // anything and roll the whole confirm back, preserving the
                // reservation as evidence, rather than silently absorbing it.
                return Err(DbError::Sqlx(sqlx::Error::Protocol(format!(
                    "confirm_payout attempt {attempt_id}: miner {miner_id} has paying < {amount} \
                     zatoshis — refusing to settle; investigate before retry"
                ))));
            }
            sqlx::query("INSERT INTO payouts (miner_id, txid, amount) VALUES (?1, ?2, ?3)")
                .bind(miner_id)
                .bind(txid)
                .bind(amount)
                .execute(&mut *conn)
                .await?;
            finalized += 1;
        }
        sqlx::query("DELETE FROM payout_items WHERE attempt_id = ?1")
            .bind(attempt_id)
            .execute(&mut *conn)
            .await?;
        if finalized != 0 { crate::pps_funding::bump_generation(conn).await?; }
        Ok(finalized)
    }

    /// Refund an in-flight reservation that never reached the chain: for each
    /// reserved item of `attempt_id` move `paying -> pending` and delete the
    /// items. The funds return to payable and are retried next cycle. Idempotent.
    /// Returns the number of items refunded. One BEGIN IMMEDIATE tx.
    pub async fn refund_payout(&self, attempt_id: i64) -> Result<i64, DbError> {
        let mut conn = self.pool.acquire().await?;
        let mut tx = conn.begin_with("BEGIN IMMEDIATE").await?;
        let result = Self::refund_payout_txn(&mut tx, attempt_id).await?;
        tx.commit().await?;
        Ok(result)
    }

    async fn refund_payout_txn(
        conn: &mut sqlx::SqliteConnection,
        attempt_id: i64,
    ) -> Result<i64, DbError> {
        let pps:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM pps_payout_items WHERE attempt_id=?1)+(SELECT COUNT(*) FROM pps_payouts WHERE attempt_id=?1)+(SELECT COUNT(*) FROM pps_conventional_attempts WHERE attempt_id=?1)").bind(attempt_id).fetch_one(&mut *conn).await?;
        if pps!=0{return Err(DbError::Sqlx(sqlx::Error::Protocol("PPS attempt cannot enter legacy payout saga".into())))}
        let items: Vec<(i64, i64)> = sqlx::query(
            "SELECT miner_id, amount FROM payout_items WHERE attempt_id = ?1",
        )
        .bind(attempt_id)
        .fetch_all(&mut *conn)
        .await?
        .iter()
        .map(|r| (r.get("miner_id"), r.get("amount")))
        .collect();

        let mut refunded: i64 = 0;
        for (miner_id, amount) in &items {
            let upd = sqlx::query(
                "UPDATE balances SET paying = paying - ?1, pending = pending + ?1 \
                 WHERE miner_id = ?2 AND paying >= ?1",
            )
            .bind(amount)
            .bind(miner_id)
            .execute(&mut *conn)
            .await?;
            if upd.rows_affected() == 0 {
                // Same reasoning as confirm_payout_txn: a reservation item whose
                // paying can't cover it means external corruption. Roll the whole
                // refund back and keep the items as evidence — never delete a
                // reservation record whose funds didn't actually move.
                return Err(DbError::Sqlx(sqlx::Error::Protocol(format!(
                    "refund_payout attempt {attempt_id}: miner {miner_id} has paying < {amount} \
                     zatoshis — refusing to refund; investigate before retry"
                ))));
            }
            refunded += 1;
        }
        sqlx::query("DELETE FROM payout_items WHERE attempt_id = ?1")
            .bind(attempt_id)
            .execute(&mut *conn)
            .await?;
        if refunded != 0 { crate::pps_funding::bump_generation(conn).await?; }
        Ok(refunded)
    }

    /// In-flight reservations awaiting resolution: payout_attempts that still
    /// hold payout_items (funds sitting in `paying`). Startup reconciliation
    /// (older_than_minutes = None) walks all of them; the periodic reconciler
    /// passes a staleness so it never touches an attempt the live payout loop is
    /// still processing. Returns (attempt_id, status, opid, txid, total_zats).
    pub async fn get_reserved_attempts(
        &self,
        older_than_minutes: Option<i64>,
    ) -> Result<Vec<(i64, String, Option<String>, Option<String>, i64, String)>, DbError> {
        let base = "SELECT pa.id, pa.status, pa.opid, pa.txid, pa.created_at, \
                    COALESCE(SUM(pi.amount), 0) AS total \
             FROM payout_attempts pa \
             JOIN payout_items pi ON pi.attempt_id = pa.id \
             WHERE NOT EXISTS (SELECT 1 FROM pps_conventional_attempts ca WHERE ca.attempt_id=pa.id) ";
        let rows: Vec<SqliteRow> = match older_than_minutes {
            Some(mins) => {
                sqlx::query(&format!(
                    "{base} AND pa.created_at < datetime('now', '-' || ?1 || ' minutes') \
                     GROUP BY pa.id, pa.status, pa.opid, pa.txid, pa.created_at ORDER BY pa.id ASC"
                ))
                .bind(mins)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(&format!(
                    "{base} GROUP BY pa.id, pa.status, pa.opid, pa.txid, pa.created_at ORDER BY pa.id ASC"
                ))
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(rows
            .iter()
            .map(|r| {
                (
                    r.get::<i64, _>("id"),
                    r.get::<String, _>("status"),
                    r.get::<Option<String>, _>("opid"),
                    r.get::<Option<String>, _>("txid"),
                    r.get::<i64, _>("total"),
                    r.get::<String, _>("created_at"),
                )
            })
            .collect())
    }

    // -- Stats helpers --

    pub async fn get_connected_miners_count(&self) -> Result<i64, DbError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(DISTINCT miner_id) FROM workers \
             WHERE last_seen >= datetime('now', '-5 minutes')",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    pub async fn get_connected_workers_count(&self) -> Result<i64, DbError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM workers \
             WHERE last_seen >= datetime('now', '-5 minutes')",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Get all miners with their share counts, worker counts, and recent hashrate.
    pub async fn get_all_miners_with_stats(&self) -> Result<Vec<MinerListEntry>, DbError> {
        let rows: Vec<SqliteRow> = sqlx::query(
            "SELECT m.id, m.address, m.created_at, \
                    COALESCE(b.pending, 0) as pending, \
                    (SELECT COUNT(*) FROM shares s \
                     JOIN workers w ON s.worker_id = w.id \
                     WHERE w.miner_id = m.id) as share_count, \
                    (SELECT COUNT(*) FROM workers w WHERE w.miner_id = m.id) as worker_count, \
                    (SELECT COALESCE(SUM(s2.difficulty), 0.0) FROM shares s2 \
                     JOIN workers w2 ON s2.worker_id = w2.id \
                     WHERE w2.miner_id = m.id \
                       AND s2.created_at >= datetime('now', '-10 minutes')) as recent_diff, \
                    (SELECT COALESCE(SUM(s3.difficulty), 0.0) FROM shares s3 \
                     JOIN workers w3 ON s3.worker_id = w3.id \
                     WHERE w3.miner_id = m.id \
                       AND s3.created_at >= datetime('now', '-1 minutes')) as recent_diff_1m \
             FROM miners m \
             LEFT JOIN balances b ON b.miner_id = m.id \
             ORDER BY share_count DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        let entries = rows
            .iter()
            .map(|row| MinerListEntry {
                id: row.get("id"),
                address: row.get("address"),
                created_at: row.get("created_at"),
                pending_balance: row.get("pending"),
                share_count: row.get("share_count"),
                worker_count: row.get("worker_count"),
                recent_diff: row.get("recent_diff"),
                recent_diff_1m: row.get("recent_diff_1m"),
            })
            .collect();

        Ok(entries)
    }

    // -- Diagnostics --

    /// Get per-worker stats for a miner, including current difficulty and hashrate data.
    pub async fn get_worker_stats_for_miner(
        &self,
        miner_id: i64,
        since_1m: &str,
        since_10m: &str,
    ) -> Result<Vec<WorkerDiagnostics>, DbError> {
        let rows: Vec<SqliteRow> = sqlx::query(
            "SELECT w.id, w.name, w.last_seen, \
                    (SELECT s.difficulty FROM shares s WHERE s.worker_id = w.id ORDER BY s.id DESC LIMIT 1) as current_difficulty, \
                    COALESCE((SELECT SUM(s.difficulty) FROM shares s WHERE s.worker_id = w.id AND s.created_at >= ?1), 0.0) as diff_sum_1m, \
                    COALESCE((SELECT SUM(s.difficulty) FROM shares s WHERE s.worker_id = w.id AND s.created_at >= ?2), 0.0) as diff_sum_10m, \
                    COALESCE((SELECT COUNT(*) FROM shares s WHERE s.worker_id = w.id AND s.created_at >= ?1), 0) as shares_1m, \
                    COALESCE((SELECT COUNT(*) FROM shares s WHERE s.worker_id = w.id AND s.created_at >= ?2), 0) as shares_10m, \
                    (SELECT COUNT(*) FROM shares s WHERE s.worker_id = w.id) as total_shares, \
                    (SELECT s.created_at FROM shares s WHERE s.worker_id = w.id ORDER BY s.id ASC LIMIT 1) as first_share_at \
             FROM workers w WHERE w.miner_id = ?3",
        )
        .bind(since_1m)
        .bind(since_10m)
        .bind(miner_id)
        .fetch_all(&self.pool)
        .await?;

        let entries = rows
            .iter()
            .map(|row| WorkerDiagnostics {
                id: row.get("id"),
                name: row.get("name"),
                last_seen: row.get("last_seen"),
                current_difficulty: row.get("current_difficulty"),
                diff_sum_1m: row.get("diff_sum_1m"),
                diff_sum_10m: row.get("diff_sum_10m"),
                shares_1m: row.get("shares_1m"),
                shares_10m: row.get("shares_10m"),
                total_shares: row.get("total_shares"),
                first_share_at: row.get("first_share_at"),
            })
            .collect();

        Ok(entries)
    }

    /// Get recent shares for a miner with worker name, for the diagnostics page.
    pub async fn get_recent_shares_for_miner(
        &self,
        miner_id: i64,
        limit: i64,
    ) -> Result<Vec<ShareDetail>, DbError> {
        let rows: Vec<SqliteRow> = sqlx::query(
            "SELECT s.id, s.difficulty, s.is_block, s.created_at, w.name as worker_name, s.session_id \
             FROM shares s \
             JOIN workers w ON s.worker_id = w.id \
             WHERE w.miner_id = ?1 \
             ORDER BY s.id DESC \
             LIMIT ?2",
        )
        .bind(miner_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let entries = rows
            .iter()
            .map(|row| ShareDetail {
                id: row.get("id"),
                difficulty: row.get("difficulty"),
                is_block: row.get("is_block"),
                created_at: row.get("created_at"),
                worker_name: row.get("worker_name"),
                session_id: row.get("session_id"),
            })
            .collect();

        Ok(entries)
    }

    /// Get all blocks found by a specific miner's workers.
    pub async fn get_miner_blocks(&self, miner_id: i64) -> Result<Vec<Block>, DbError> {
        let blocks: Vec<Block> = sqlx::query_as(
            "SELECT b.id, b.height, b.hash, b.reward, b.status, b.found_by, b.created_at, b.luck_percent \
             FROM blocks b \
             JOIN workers w ON b.found_by = w.id \
             WHERE w.miner_id = ?1 \
             ORDER BY b.height DESC",
        )
        .bind(miner_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(blocks)
    }

    /// Get recent payouts for a specific miner.
    pub async fn get_miner_payouts(&self, miner_id: i64, limit: i64) -> Result<Vec<Payout>, DbError> {
        let payouts: Vec<Payout> = sqlx::query_as(
            "SELECT id, miner_id, txid, amount, created_at FROM payouts WHERE miner_id = ?1 ORDER BY id DESC LIMIT ?2",
        )
        .bind(miner_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(payouts)
    }

    /// Get shares grouped by miner for the last N shares (for PPLNS).
    pub async fn get_pplns_shares(
        &self,
        window_size: i64,
    ) -> Result<Vec<PplnsShareEntry>, DbError> {
        let rows: Vec<SqliteRow> = sqlx::query(
            // ORDER BY makes the iteration order deterministic (audit
            // Finding #10): the rounding remainder always lands on the
            // smallest contributor instead of whoever SQLite emits last.
            "SELECT w.miner_id, SUM(s.difficulty) as total_difficulty \
             FROM (SELECT * FROM shares ORDER BY id DESC LIMIT ?1) s \
             JOIN workers w ON s.worker_id = w.id \
             GROUP BY w.miner_id \
             ORDER BY total_difficulty DESC, w.miner_id ASC",
        )
        .bind(window_size)
        .fetch_all(&self.pool)
        .await?;

        let entries = rows
            .iter()
            .map(|row| PplnsShareEntry {
                miner_id: row.get("miner_id"),
                total_difficulty: row.get("total_difficulty"),
            })
            .collect();

        Ok(entries)
    }

    /// Adjust a miner's pending balance by delta (can be negative).
    /// Returns the new pending balance. Clamps to 0 minimum.
    pub async fn adjust_miner_balance(&self, miner_id: i64, delta_zatoshis: i64) -> Result<i64, DbError> {
        sqlx::query("INSERT OR IGNORE INTO balances (miner_id) VALUES (?1)")
            .bind(miner_id)
            .execute(&self.pool)
            .await?;

        sqlx::query("UPDATE balances SET pending = MAX(0, pending + ?1) WHERE miner_id = ?2")
            .bind(delta_zatoshis)
            .bind(miner_id)
            .execute(&self.pool)
            .await?;

        let row: (i64,) = sqlx::query_as(
            "SELECT pending FROM balances WHERE miner_id = ?1",
        )
        .bind(miner_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0)
    }

    /// Get all miners with full admin details (pending, paid, last_seen, shares).
    pub async fn get_all_miners_admin(&self) -> Result<Vec<AdminMinerEntry>, DbError> {
        let rows: Vec<SqliteRow> = sqlx::query(
            "SELECT m.id, m.address, m.created_at, \
                    COALESCE(b.pending, 0) as pending, \
                    COALESCE(b.paid, 0) as paid, \
                    (SELECT COUNT(*) FROM shares s \
                     JOIN workers w ON s.worker_id = w.id \
                     WHERE w.miner_id = m.id) as share_count, \
                    (SELECT COUNT(*) FROM workers w WHERE w.miner_id = m.id) as worker_count, \
                    (SELECT MAX(w2.last_seen) FROM workers w2 WHERE w2.miner_id = m.id) as last_seen \
             FROM miners m \
             LEFT JOIN balances b ON b.miner_id = m.id \
             ORDER BY pending DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        let entries = rows
            .iter()
            .map(|row| AdminMinerEntry {
                id: row.get("id"),
                address: row.get("address"),
                created_at: row.get("created_at"),
                pending: row.get("pending"),
                paid: row.get("paid"),
                share_count: row.get("share_count"),
                worker_count: row.get("worker_count"),
                last_seen: row.get("last_seen"),
            })
            .collect();

        Ok(entries)
    }
}

/// A miner eligible for payout.
#[derive(Debug, Clone)]
pub struct PendingPayout {
    pub miner_id: i64,
    pub address: String,
    pub amount: i64,
    pub created_at: String,
}

/// Aggregated share data for PPLNS calculation.
#[derive(Debug, Clone)]
pub struct PplnsShareEntry {
    pub miner_id: i64,
    pub total_difficulty: f64,
}

/// Admin miner entry with full details.
#[derive(Debug, Clone)]
pub struct AdminMinerEntry {
    pub id: i64,
    pub address: String,
    pub created_at: String,
    pub pending: i64,
    pub paid: i64,
    pub share_count: i64,
    pub worker_count: i64,
    pub last_seen: Option<String>,
}

/// Miner summary for the miners list.
#[derive(Debug, Clone)]
pub struct MinerListEntry {
    pub id: i64,
    pub address: String,
    pub created_at: String,
    pub pending_balance: i64,
    pub share_count: i64,
    pub worker_count: i64,
    /// Share count in the last 10 minutes.
    pub recent_diff: f64,
    /// Share count in the last 1 minute.
    pub recent_diff_1m: f64,
}
