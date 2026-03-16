use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

use crate::models::*;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),
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
        Ok(())
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
            "SELECT id, miner_id, name, last_seen FROM workers WHERE miner_id = ?1 AND name = ?2",
        )
        .bind(miner_id)
        .bind(name)
        .fetch_one(&self.pool)
        .await?;

        Ok(worker)
    }

    pub async fn get_workers_for_miner(&self, miner_id: i64) -> Result<Vec<Worker>, DbError> {
        let workers: Vec<Worker> = sqlx::query_as(
            "SELECT id, miner_id, name, last_seen FROM workers WHERE miner_id = ?1",
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
    ) -> Result<i64, DbError> {
        let is_block_int: i32 = if is_block { 1 } else { 0 };
        let result = sqlx::query(
            "INSERT INTO shares (worker_id, job_id, difficulty, is_block) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(worker_id)
        .bind(job_id)
        .bind(difficulty)
        .bind(is_block_int)
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
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM shares")
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
        found_by: i64,
        luck_percent: Option<f64>,
    ) -> Result<i64, DbError> {
        let result = sqlx::query(
            "INSERT INTO blocks (height, hash, reward, found_by, luck_percent) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(height)
        .bind(hash)
        .bind(reward)
        .bind(found_by)
        .bind(luck_percent)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
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

    /// Reverse PPLNS credits for an orphaned block by subtracting reward from pending balances.
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
            "SELECT id, miner_id, txid, amount, created_at FROM payouts ORDER BY id DESC LIMIT ?1",
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

    /// Find miners with pending balance >= min_amount (in zatoshis).
    pub async fn get_pending_payouts(&self, min_amount: i64) -> Result<Vec<PendingPayout>, DbError> {
        let rows: Vec<SqliteRow> = sqlx::query(
            "SELECT b.miner_id, m.address, b.pending \
             FROM balances b \
             JOIN miners m ON m.id = b.miner_id \
             WHERE b.pending >= ?1 \
             ORDER BY b.pending DESC",
        )
        .bind(min_amount)
        .fetch_all(&self.pool)
        .await?;

        let entries = rows
            .iter()
            .map(|row| PendingPayout {
                miner_id: row.get("miner_id"),
                address: row.get("address"),
                amount: row.get("pending"),
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
        let result = sqlx::query(
            "INSERT INTO payouts (miner_id, txid, amount) VALUES (?1, ?2, ?3)",
        )
        .bind(miner_id)
        .bind(txid)
        .bind(amount)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "UPDATE balances SET pending = pending - ?1, paid = paid + ?1 WHERE miner_id = ?2",
        )
        .bind(amount)
        .bind(miner_id)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
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
                    COALESCE((SELECT COUNT(*) FROM shares s WHERE s.worker_id = w.id AND s.created_at >= ?2), 0) as shares_10m, \
                    (SELECT COUNT(*) FROM shares s WHERE s.worker_id = w.id) as total_shares \
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
                shares_10m: row.get("shares_10m"),
                total_shares: row.get("total_shares"),
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
            "SELECT s.id, s.difficulty, s.is_block, s.created_at, w.name as worker_name \
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
            "SELECT w.miner_id, SUM(s.difficulty) as total_difficulty \
             FROM (SELECT * FROM shares ORDER BY id DESC LIMIT ?1) s \
             JOIN workers w ON s.worker_id = w.id \
             GROUP BY w.miner_id",
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
}

/// A miner eligible for payout.
#[derive(Debug, Clone)]
pub struct PendingPayout {
    pub miner_id: i64,
    pub address: String,
    pub amount: i64,
}

/// Aggregated share data for PPLNS calculation.
#[derive(Debug, Clone)]
pub struct PplnsShareEntry {
    pub miner_id: i64,
    pub total_difficulty: f64,
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
