-- Withholding defence (2026-09-15, #10): an account younger than
-- young_account_seconds may not hold more than young_account_exposure zatoshis
-- outstanding, so a block-withholding miner's payoff is bounded before its luck
-- has statistics. Written from [pps] at startup; the credit path reads it.
CREATE TABLE IF NOT EXISTS pps_admission_limits (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    young_account_seconds INTEGER NOT NULL CHECK(typeof(young_account_seconds)='integer' AND young_account_seconds>=0),
    young_account_exposure INTEGER NOT NULL CHECK(typeof(young_account_exposure)='integer' AND young_account_exposure>=0),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
