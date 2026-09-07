-- PPS has separate liabilities and payout attribution: legacy reorg/reversal
-- and pre-existing reservations must never consume PPS earnings.
CREATE TABLE IF NOT EXISTS pps_meta (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1), network TEXT NOT NULL,
 active_epoch TEXT NOT NULL, cap_zats INTEGER NOT NULL CHECK(typeof(cap_zats)='integer' AND cap_zats>0),
 gross_whole INTEGER NOT NULL DEFAULT 0 CHECK(typeof(gross_whole)='integer' AND gross_whole>=0),
 gross_fraction INTEGER NOT NULL DEFAULT 0 CHECK(typeof(gross_fraction)='integer' AND gross_fraction>=0 AND gross_fraction<1000000000000),
 event_count INTEGER NOT NULL DEFAULT 0 CHECK(typeof(event_count)='integer' AND event_count>=0)
);
CREATE TABLE IF NOT EXISTS pps_epochs (
 id TEXT PRIMARY KEY NOT NULL, network TEXT NOT NULL, fee_bps INTEGER NOT NULL CHECK(fee_bps>=0 AND fee_bps<10000),
 cap_zats INTEGER NOT NULL CHECK(typeof(cap_zats)='integer' AND cap_zats>0), quote_provenance TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS pps_accounts (
 miner_id INTEGER PRIMARY KEY REFERENCES miners(id),
 pending INTEGER NOT NULL DEFAULT 0 CHECK(typeof(pending)='integer' AND pending>=0),
 paying INTEGER NOT NULL DEFAULT 0 CHECK(typeof(paying)='integer' AND paying>=0),
 paid INTEGER NOT NULL DEFAULT 0 CHECK(typeof(paid)='integer' AND paid>=0),
 fraction INTEGER NOT NULL DEFAULT 0 CHECK(typeof(fraction)='integer' AND fraction>=0 AND fraction<1000000000000)
);
CREATE TABLE IF NOT EXISTS pps_quotes (
 quote_id TEXT PRIMARY KEY NOT NULL, epoch_id TEXT NOT NULL REFERENCES pps_epochs(id),
 height INTEGER NOT NULL CHECK(typeof(height)='integer' AND height>=0),
 network_target BLOB NOT NULL CHECK(typeof(network_target)='blob' AND length(network_target)=32),
 assigned_target BLOB NOT NULL CHECK(typeof(assigned_target)='blob' AND length(assigned_target)=32),
 miner_subsidy INTEGER NOT NULL CHECK(typeof(miner_subsidy)='integer' AND miner_subsidy>0),
 amount_whole INTEGER NOT NULL CHECK(typeof(amount_whole)='integer' AND amount_whole>=0),
 amount_fraction INTEGER NOT NULL CHECK(typeof(amount_fraction)='integer' AND amount_fraction>=0 AND amount_fraction<1000000000000)
);
CREATE TABLE IF NOT EXISTS pps_events (
 proof_id TEXT PRIMARY KEY NOT NULL, epoch_id TEXT NOT NULL REFERENCES pps_epochs(id), quote_id TEXT NOT NULL REFERENCES pps_quotes(quote_id),
 miner_id INTEGER NOT NULL REFERENCES miners(id), worker_id INTEGER NOT NULL REFERENCES workers(id),
 job_id TEXT NOT NULL, session_id TEXT NOT NULL, difficulty REAL NOT NULL, is_block INTEGER NOT NULL,
 amount_whole INTEGER NOT NULL CHECK(typeof(amount_whole)='integer' AND amount_whole>=0),
 amount_fraction INTEGER NOT NULL CHECK(typeof(amount_fraction)='integer' AND amount_fraction>=0 AND amount_fraction<1000000000000),
 accepted_at INTEGER NOT NULL, share_id INTEGER NOT NULL UNIQUE
);
CREATE TABLE IF NOT EXISTS pps_payout_items (
 attempt_id INTEGER NOT NULL REFERENCES payout_attempts(id), miner_id INTEGER NOT NULL REFERENCES miners(id),
 amount INTEGER NOT NULL CHECK(typeof(amount)='integer' AND amount>0), PRIMARY KEY(attempt_id,miner_id)
);
CREATE TABLE IF NOT EXISTS pps_payouts (
 id INTEGER PRIMARY KEY, attempt_id INTEGER NOT NULL REFERENCES payout_attempts(id), miner_id INTEGER NOT NULL REFERENCES miners(id),
 txid TEXT NOT NULL, amount INTEGER NOT NULL CHECK(typeof(amount)='integer' AND amount>0),
 created_at TEXT NOT NULL DEFAULT (datetime('now')), UNIQUE(attempt_id,miner_id)
);
CREATE TABLE IF NOT EXISTS pps_block_markers (block_id INTEGER PRIMARY KEY REFERENCES blocks(id), epoch_id TEXT NOT NULL REFERENCES pps_epochs(id));
CREATE TABLE IF NOT EXISTS pps_submission_markers (submission_id INTEGER PRIMARY KEY REFERENCES block_submissions(id), epoch_id TEXT NOT NULL REFERENCES pps_epochs(id));
