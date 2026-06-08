use pool_db::PoolDb;
use tracing::info;

/// Reward distribution mode. PPLNS spreads each block across all miners in the
/// share window proportionally; Solo credits the full reward (minus pool_fee)
/// to the single miner whose worker found the block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewardMode {
    Pplns,
    Solo,
}

/// Reward calculator. Routes to PPLNS or Solo distribution based on `mode`.
/// Name retained as `PplnsCalculator` for backwards compatibility; the struct
/// now handles both modes.
pub struct PplnsCalculator {
    db: PoolDb,
    /// Number of shares in the PPLNS window. Ignored when mode == Solo.
    window_size: i64,
    /// Pool fee as a fraction (e.g., 0.01 = 1%).
    pool_fee: f64,
    /// Reward distribution mode.
    mode: RewardMode,
}

/// Result of a PPLNS distribution for a single miner.
#[derive(Debug, Clone)]
pub struct PplnsReward {
    pub miner_id: i64,
    pub share_fraction: f64,
    pub amount_zatoshis: i64,
}

impl PplnsCalculator {
    pub fn new(db: PoolDb, window_size: i64, pool_fee: f64, mode: RewardMode) -> Self {
        Self {
            db,
            window_size,
            pool_fee: pool_fee.clamp(0.0, 1.0),
            mode,
        }
    }

    /// Distribute a block reward.
    ///
    /// - `block_reward`: The total block reward in zatoshis.
    /// - `block_id`: The database ID of the block (for logging).
    /// - `found_by_worker_id`: The worker that submitted the share that solved
    ///   the block. Used by Solo mode to determine the sole recipient; ignored
    ///   by PPLNS mode.
    ///
    /// Returns the list of individual rewards credited.
    pub async fn distribute(
        &self,
        block_reward: i64,
        block_id: i64,
        found_by_worker_id: i64,
    ) -> Result<Vec<PplnsReward>, RewardError> {
        // Pool fee applies in both modes.
        let distributable = ((block_reward as f64) * (1.0 - self.pool_fee)) as i64;
        let pool_fee_amount = block_reward - distributable;

        if self.mode == RewardMode::Solo {
            let miner_id = self.db.get_miner_id_for_worker(found_by_worker_id).await?;
            info!(
                block_id,
                block_reward,
                pool_fee_amount,
                distributable,
                miner_id,
                found_by_worker_id,
                "Distributing solo reward (100% to block finder)"
            );
            if distributable > 0 {
                self.db.credit_balance(miner_id, distributable).await?;
            }
            return Ok(vec![PplnsReward {
                miner_id,
                share_fraction: 1.0,
                amount_zatoshis: distributable,
            }]);
        }

        let shares = self.db.get_pplns_shares(self.window_size).await?;

        if shares.is_empty() {
            info!(block_id, "No shares in PPLNS window, skipping distribution");
            return Ok(vec![]);
        }

        let total_difficulty: f64 = shares.iter().map(|s| s.total_difficulty).sum();
        if total_difficulty <= 0.0 {
            return Ok(vec![]);
        }

        // Pool fee (`distributable` and `pool_fee_amount` already computed above
        // before the Solo-mode short-circuit; recompute here for clarity).
        let distributable = ((block_reward as f64) * (1.0 - self.pool_fee)) as i64;
        let pool_fee_amount = block_reward - distributable;

        info!(
            block_id,
            block_reward,
            pool_fee_amount,
            distributable,
            miners = shares.len(),
            total_difficulty,
            window_size = self.window_size,
            "Distributing PPLNS rewards"
        );

        let mut rewards = Vec::with_capacity(shares.len());
        let mut distributed_total: i64 = 0;

        for (i, entry) in shares.iter().enumerate() {
            let fraction = entry.total_difficulty / total_difficulty;
            // For the last miner, give whatever's left to avoid rounding errors
            let amount = if i == shares.len() - 1 {
                distributable - distributed_total
            } else {
                (distributable as f64 * fraction).floor() as i64
            };

            if amount > 0 {
                self.db.credit_balance(entry.miner_id, amount).await?;

                info!(
                    miner_id = entry.miner_id,
                    amount,
                    fraction = format!("{:.4}", fraction),
                    "Credited PPLNS reward"
                );
            }

            distributed_total += amount;

            rewards.push(PplnsReward {
                miner_id: entry.miner_id,
                share_fraction: fraction,
                amount_zatoshis: amount,
            });
        }

        Ok(rewards)
    }

    pub fn window_size(&self) -> i64 {
        self.window_size
    }

    pub fn pool_fee(&self) -> f64 {
        self.pool_fee
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RewardError {
    #[error("Database error: {0}")]
    Db(#[from] pool_db::DbError),
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_pplns_distribution_logic() {
        // Unit test for the proportion logic (without a real DB)
        let total_difficulty = 100.0;
        let block_reward: i64 = 1_000_000_000; // 10 ZEC
        let pool_fee = 0.01;
        let distributable = ((block_reward as f64) * (1.0 - pool_fee)) as i64;

        let miner_a_difficulty = 60.0;
        let miner_b_difficulty = 40.0;

        let a_fraction = miner_a_difficulty / total_difficulty;
        let b_fraction = miner_b_difficulty / total_difficulty;

        let a_reward = (distributable as f64 * a_fraction).floor() as i64;
        let b_reward = distributable - a_reward;

        assert_eq!(a_fraction, 0.6);
        assert_eq!(b_fraction, 0.4);
        assert!(a_reward + b_reward == distributable);
        assert!(a_reward > b_reward);
    }
}
