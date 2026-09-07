-- Additive: historical conventional attempts lack an intent and remain held.
CREATE TABLE IF NOT EXISTS pps_conventional_intents (
    attempt_id INTEGER PRIMARY KEY REFERENCES pps_conventional_attempts(attempt_id),
    canonical_json TEXT NOT NULL CHECK(length(canonical_json) BETWEEN 1 AND 262144)
);
CREATE TRIGGER IF NOT EXISTS pps_conventional_intents_no_update
BEFORE UPDATE ON pps_conventional_intents BEGIN
    SELECT RAISE(ABORT, 'immutable PPS intent');
END;
CREATE TABLE IF NOT EXISTS pps_conventional_halts (
    attempt_id INTEGER PRIMARY KEY REFERENCES pps_conventional_attempts(attempt_id),
    category TEXT NOT NULL CHECK(category IN ('recipient_mismatch','fee_mismatch','transaction_version_mismatch','source_mismatch'))
);
CREATE TRIGGER IF NOT EXISTS pps_conventional_halts_no_update
BEFORE UPDATE ON pps_conventional_halts BEGIN
    SELECT RAISE(ABORT, 'immutable PPS halt');
END;
CREATE TRIGGER IF NOT EXISTS pps_conventional_halts_no_delete
BEFORE DELETE ON pps_conventional_halts BEGIN
    SELECT RAISE(ABORT, 'immutable PPS halt');
END;
CREATE TRIGGER IF NOT EXISTS pps_conventional_intents_no_delete
BEFORE DELETE ON pps_conventional_intents BEGIN
    SELECT RAISE(ABORT, 'immutable PPS intent');
END;
