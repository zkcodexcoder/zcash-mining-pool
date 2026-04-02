-- Composite index for per-miner hashrate queries (worker_id + created_at + difficulty).
-- Covers the correlated subqueries in get_all_miners_with_stats without table lookups.
CREATE INDEX IF NOT EXISTS idx_shares_worker_created_diff ON shares(worker_id, created_at, difficulty);
