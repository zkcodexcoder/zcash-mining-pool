-- Audit #15: block-found durability breadcrumb.
--
-- A row is written BEFORE submitblock. If the process dies anywhere between
-- submit and record_block (watchdog exit, crash, OOM), the startup sweep finds
-- the open (resolved=0) row, asks the chain whether the block is ours, and
-- records + distributes it — a won block can no longer be silently lost
-- (block-90's sibling failure class).
CREATE TABLE IF NOT EXISTS block_submissions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    height INTEGER NOT NULL,
    hash TEXT NOT NULL,                 -- same byte order as blocks.hash
    worker_id INTEGER NOT NULL,
    reward INTEGER NOT NULL,            -- zatoshis (subsidy)
    actual_reward INTEGER,              -- zatoshis incl. tx fees (NULL = unknown)
    resolved INTEGER NOT NULL DEFAULT 0,-- 0 = open (fate unknown), 1 = settled
    outcome TEXT,                       -- 'recorded' | 'rejected' | 'swept-recorded' | 'swept-rejected'
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_block_submissions_open ON block_submissions(resolved);
