-- Additive, testnet-only async-wallet journal. The original fee reservation is
-- an immutable upper bound; only verified settlement records an actual fee.
CREATE TABLE IF NOT EXISTS pps_conventional_attempts (
 attempt_id INTEGER PRIMARY KEY REFERENCES pps_fee_reservations(attempt_id),
 intent_id TEXT NOT NULL UNIQUE CHECK(length(intent_id)=64),
 fee_bound INTEGER NOT NULL CHECK(typeof(fee_bound)='integer' AND fee_bound>0),
 actual_fee INTEGER CHECK(actual_fee IS NULL OR (typeof(actual_fee)='integer' AND actual_fee>=0 AND actual_fee<=fee_bound)),
 operation_id TEXT UNIQUE CHECK(operation_id IS NULL OR (length(operation_id)>0 AND length(operation_id)<=128)),
 observed_txid TEXT UNIQUE CHECK(observed_txid IS NULL OR (length(observed_txid)=64 AND operation_id IS NOT NULL)),
 excess_fee INTEGER CHECK(excess_fee IS NULL OR (typeof(excess_fee)='integer' AND excess_fee>fee_bound)),
 CHECK(actual_fee IS NULL OR (observed_txid IS NOT NULL AND excess_fee IS NULL))
);
