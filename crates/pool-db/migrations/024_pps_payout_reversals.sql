-- A settled PPS payout whose transaction can never confirm (the wallet reports
-- it conflicted after a reorg): the operator tool returns the amount from paid
-- to pending and journals it here. The pps_payouts row stays for the audit
-- trail; reconcile subtracts reversed amounts and the fee was never paid.
-- Immutable: a reversal is never edited or removed.
CREATE TABLE IF NOT EXISTS pps_payout_reversals (
    payout_id INTEGER PRIMARY KEY REFERENCES pps_payouts(id),
    attempt_id INTEGER NOT NULL REFERENCES payout_attempts(id),
    miner_id INTEGER NOT NULL REFERENCES miners(id),
    txid TEXT NOT NULL CHECK(length(txid)=64),
    amount INTEGER NOT NULL CHECK(typeof(amount)='integer' AND amount>0),
    operator TEXT NOT NULL CHECK(length(operator) BETWEEN 1 AND 128),
    evidence TEXT NOT NULL CHECK(length(evidence) BETWEEN 1 AND 4096),
    reversed_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TRIGGER IF NOT EXISTS pps_payout_reversals_no_update
BEFORE UPDATE ON pps_payout_reversals BEGIN
    SELECT RAISE(ABORT, 'immutable PPS payout reversal');
END;
CREATE TRIGGER IF NOT EXISTS pps_payout_reversals_no_delete
BEFORE DELETE ON pps_payout_reversals BEGIN
    SELECT RAISE(ABORT, 'immutable PPS payout reversal');
END;
