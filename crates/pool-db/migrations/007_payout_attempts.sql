CREATE TABLE IF NOT EXISTS payout_attempts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    opid TEXT,
    txid TEXT,
    status TEXT NOT NULL,                  -- queued, sent, confirmed, failed
    miner_count INTEGER NOT NULL DEFAULT 0,
    total_zatoshis INTEGER NOT NULL DEFAULT 0,
    source TEXT NOT NULL DEFAULT 'loop',   -- loop, manual
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_payout_attempts_opid ON payout_attempts(opid);
CREATE INDEX IF NOT EXISTS idx_payout_attempts_txid ON payout_attempts(txid);
CREATE INDEX IF NOT EXISTS idx_payout_attempts_status_created ON payout_attempts(status, created_at);
