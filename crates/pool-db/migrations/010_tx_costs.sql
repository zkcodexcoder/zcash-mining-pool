-- Pipeline cost recovery (operator policy 2026-06-10): the pool's own
-- ZIP-317 tx fees (shielding coinbase, sending payouts) are recorded here
-- and deducted from the next block's distribution — miners collectively
-- bear actual pipeline costs, the pool advertises a true 0% (or whatever
-- fee_percent says) with no silent margins and no operator bleed.
CREATE TABLE IF NOT EXISTS pool_tx_costs (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    kind       TEXT NOT NULL,            -- 'shield' | 'payout' | 'reorphaned'
    ref        TEXT,                     -- opid or txid for audit
    fee        INTEGER NOT NULL,         -- zatoshis (ZIP-317 deterministic estimate)
    recovered  INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_pool_tx_costs_unrecovered ON pool_tx_costs(recovered, id);

-- Which costs each block's distribution absorbed (for the accounting
-- invariant and for re-queueing on orphan reversal).
ALTER TABLE blocks ADD COLUMN costs_recovered INTEGER NOT NULL DEFAULT 0;
