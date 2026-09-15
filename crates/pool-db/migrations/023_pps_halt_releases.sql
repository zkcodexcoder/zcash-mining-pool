-- An operator's audited release of a PPS financial halt (a send that failed
-- byte-for-byte verification, or an over-bound fee). It lifts the fence on sends
-- and new credits for that halt; the attempt itself stays for the confirm and
-- reversal tools. Immutable: a release is never edited or removed.
CREATE TABLE IF NOT EXISTS pps_halt_releases (
    attempt_id INTEGER PRIMARY KEY REFERENCES pps_conventional_attempts(attempt_id),
    kind TEXT NOT NULL CHECK(kind IN ('contract_halt','excess_fee')),
    operator TEXT NOT NULL CHECK(length(operator) BETWEEN 1 AND 128),
    reason TEXT NOT NULL CHECK(length(reason) BETWEEN 1 AND 1024),
    reconciler_last_run TEXT NOT NULL CHECK(length(reconciler_last_run) BETWEEN 1 AND 64),
    released_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TRIGGER IF NOT EXISTS pps_halt_releases_no_update
BEFORE UPDATE ON pps_halt_releases BEGIN
    SELECT RAISE(ABORT, 'immutable PPS halt release');
END;
CREATE TRIGGER IF NOT EXISTS pps_halt_releases_no_delete
BEFORE DELETE ON pps_halt_releases BEGIN
    SELECT RAISE(ABORT, 'immutable PPS halt release');
END;
