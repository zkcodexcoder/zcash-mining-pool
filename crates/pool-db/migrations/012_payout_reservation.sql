-- Round-3 pre-debit payout saga.
--
-- The double-pay class came from debiting `pending` AFTER the on-chain send: a
-- crash between z_sendmany and recording left balances payable, so the next
-- payout loop re-paid them. The fix is to RESERVE funds (move pending -> paying)
-- BEFORE the send, record the per-miner breakdown in payout_items for crash
-- recovery, then on confirmation move paying -> paid. A crash anywhere in
-- between leaves the funds in `paying` (out of `pending`), so the loop can't
-- re-select them; startup reconciliation resolves each in-flight reservation by
-- checking the txid on chain (confirm) or refunding it (paying -> pending).
--
-- Conserved invariant becomes: pending + paying + paid.

-- Per-miner breakdown of an in-flight (reserved-but-not-finalized) payout.
-- Rows exist only while funds sit in `balances.paying`; confirm/refund delete
-- them, which is what makes those operations idempotent under retry/restart.
CREATE TABLE IF NOT EXISTS payout_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    attempt_id INTEGER NOT NULL REFERENCES payout_attempts(id),
    miner_id INTEGER NOT NULL REFERENCES miners(id),
    amount INTEGER NOT NULL,            -- zatoshis reserved (moved pending -> paying)
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_payout_items_attempt ON payout_items(attempt_id);

-- In-flight reserved balance: funds debited from `pending` for a payout that is
-- neither confirmed on chain nor refunded yet. (ALTER is ignored on re-runs by
-- the migration runner once the column exists.)
ALTER TABLE balances ADD COLUMN paying INTEGER NOT NULL DEFAULT 0;
