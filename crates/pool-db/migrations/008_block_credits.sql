-- Audit Phase C (P2 + P5): per-block credit ledger, actual coinbase value,
-- and an explicit clawback ledger for orphaned-but-already-paid credits.

-- P2: the actual coinbase output value (subsidy + collected tx fees) from
-- the template the block was mined against. NULL for blocks recorded
-- before this migration; consumers fall back to `reward` (subsidy only).
ALTER TABLE blocks ADD COLUMN actual_reward INTEGER;

-- P5: exactly which miner was credited how much for which block. Written
-- atomically with the balance updates at distribution time; consulted by
-- orphan reversal so the precise credits are debited from the precise
-- miners (instead of the old proportional claw from whoever happens to
-- hold pending balance at detection time).
CREATE TABLE IF NOT EXISTS block_credits (
    block_id INTEGER NOT NULL REFERENCES blocks(id),
    miner_id INTEGER NOT NULL REFERENCES miners(id),
    amount   INTEGER NOT NULL,           -- zatoshis credited to pending
    PRIMARY KEY (block_id, miner_id)
);

-- P5: when an orphaned block's credit was already paid out (pending
-- insufficient at reversal time), the shortfall is recorded here instead
-- of being stolen from other miners' pending. Operator decides recovery
-- (typically: net against the miner's future earnings or absorb).
CREATE TABLE IF NOT EXISTS orphan_clawbacks (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    block_id   INTEGER NOT NULL,
    miner_id   INTEGER NOT NULL,
    amount     INTEGER NOT NULL,         -- zatoshis credited but unrecoverable from pending
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_orphan_clawbacks_created ON orphan_clawbacks(created_at);
