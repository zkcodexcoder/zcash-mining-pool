-- A sealed conventional PPS send that the wallet reported failed before building
-- any transaction, released back to pending by the operator tool. Immutable.
CREATE TABLE IF NOT EXISTS pps_released_sends (
    attempt_id INTEGER PRIMARY KEY REFERENCES pps_conventional_attempts(attempt_id),
    operation_id TEXT NOT NULL UNIQUE CHECK(length(operation_id) BETWEEN 1 AND 128),
    released_zatoshis INTEGER NOT NULL CHECK(typeof(released_zatoshis)='integer' AND released_zatoshis>0),
    evidence TEXT NOT NULL CHECK(length(evidence) BETWEEN 1 AND 4096),
    released_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TRIGGER IF NOT EXISTS pps_released_sends_no_update
BEFORE UPDATE ON pps_released_sends BEGIN
    SELECT RAISE(ABORT, 'immutable PPS release record');
END;
CREATE TRIGGER IF NOT EXISTS pps_released_sends_no_delete
BEFORE DELETE ON pps_released_sends BEGIN
    SELECT RAISE(ABORT, 'immutable PPS release record');
END;
