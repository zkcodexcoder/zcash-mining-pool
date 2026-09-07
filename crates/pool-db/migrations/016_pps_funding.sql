-- No PPS activation or inferred financial policy: only an independent wallet
-- mutation generation exists before the operator supplies a funded epoch.
CREATE TABLE IF NOT EXISTS pps_funding_generation (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 generation INTEGER NOT NULL CHECK(typeof(generation)='integer' AND generation>=0)
);
INSERT OR IGNORE INTO pps_funding_generation VALUES(1,0);
CREATE TABLE IF NOT EXISTS pps_funding_policy (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1), network TEXT NOT NULL,
 credit_cap INTEGER NOT NULL CHECK(typeof(credit_cap)='integer' AND credit_cap>0),
 total_cap INTEGER NOT NULL CHECK(typeof(total_cap)='integer' AND total_cap>0),
 fee_cap INTEGER NOT NULL CHECK(typeof(fee_cap)='integer' AND fee_cap>0),
 reserve_floor INTEGER NOT NULL CHECK(typeof(reserve_floor)='integer' AND reserve_floor>0)
);
CREATE TABLE IF NOT EXISTS pps_fee_reservations (
 attempt_id INTEGER PRIMARY KEY REFERENCES payout_attempts(id),
 proposal_id TEXT NOT NULL UNIQUE CHECK(length(proposal_id)=64),
 fee_zats INTEGER NOT NULL CHECK(typeof(fee_zats)='integer' AND fee_zats>0),
 status TEXT NOT NULL CHECK(status IN ('reserved','paid','released')),
 txid TEXT,
 sealed INTEGER NOT NULL DEFAULT 0 CHECK(typeof(sealed)='integer' AND sealed IN (0,1)),
 expected_txid TEXT,
 CHECK(expected_txid IS NULL OR (sealed=1 AND length(expected_txid)=64)),
 CHECK(status<>'released' OR sealed=0),
 CHECK((status='paid' AND txid IS NOT NULL) OR (status<>'paid' AND txid IS NULL))
);
