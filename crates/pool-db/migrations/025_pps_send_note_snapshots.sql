-- The wallet's unspent note transaction ids just before a conventional PPS send
-- (lost-operation recovery, 2026-09-15 #8). A send spends at least one of these
-- notes and its change note carries the send's txid, so a forgotten operation
-- id can be recovered from the notes or, when every note is still unspent,
-- released. Insert-once: never edited or removed.
CREATE TABLE IF NOT EXISTS pps_send_note_snapshots (
    attempt_id INTEGER PRIMARY KEY REFERENCES pps_conventional_attempts(attempt_id),
    txids TEXT NOT NULL CHECK(length(txids) BETWEEN 2 AND 8388608),
    taken_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TRIGGER IF NOT EXISTS pps_send_note_snapshots_no_update
BEFORE UPDATE ON pps_send_note_snapshots BEGIN
    SELECT RAISE(ABORT, 'immutable PPS send note snapshot');
END;
CREATE TRIGGER IF NOT EXISTS pps_send_note_snapshots_no_delete
BEFORE DELETE ON pps_send_note_snapshots BEGIN
    SELECT RAISE(ABORT, 'immutable PPS send note snapshot');
END;
