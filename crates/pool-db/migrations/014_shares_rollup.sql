-- Audit #16: shares retention. Raw share rows older than the retention window
-- are aggregated into hourly buckets and deleted (same transaction, per hour),
-- so the shares table stops growing unboundedly (7.4M rows / 1.1GB when this
-- shipped) while all-time totals stay exact: total = rollup + remaining raw.
CREATE TABLE IF NOT EXISTS shares_rollup (
    bucket TEXT PRIMARY KEY,   -- UTC hour, 'YYYY-MM-DD HH:00:00'
    cnt INTEGER NOT NULL,
    diff_sum REAL NOT NULL
);
