-- Authorization journal only: startup never changes an existing allowance.
-- The sole supported extension is explicit, funded and lifetime cumulative.
CREATE TABLE IF NOT EXISTS pps_budget_extensions (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 kind TEXT NOT NULL CHECK(kind='testnet-10-to-1000-v1'),
 network TEXT NOT NULL CHECK(network='testnet'),
 previous_epoch TEXT NOT NULL REFERENCES pps_epochs(id),
 next_epoch TEXT NOT NULL UNIQUE REFERENCES pps_epochs(id) CHECK(next_epoch='testnet-pps-1000-20260907'),
 previous_credit_cap INTEGER NOT NULL CHECK(previous_credit_cap=950000000),
 previous_fee_cap INTEGER NOT NULL CHECK(previous_fee_cap=50000000),
 previous_total_cap INTEGER NOT NULL CHECK(previous_total_cap=1000000000),
 next_credit_cap INTEGER NOT NULL CHECK(next_credit_cap=95000000000),
 next_fee_cap INTEGER NOT NULL CHECK(next_fee_cap=5000000000),
 next_total_cap INTEGER NOT NULL CHECK(next_total_cap=100000000000),
 reserve_floor INTEGER NOT NULL CHECK(typeof(reserve_floor)='integer' AND reserve_floor>0),
 previous_generation INTEGER NOT NULL CHECK(typeof(previous_generation)='integer' AND previous_generation>=0),
 next_generation INTEGER NOT NULL CHECK(typeof(next_generation)='integer' AND next_generation=previous_generation+1),
 gross_whole INTEGER NOT NULL CHECK(typeof(gross_whole)='integer' AND gross_whole>=0),
 gross_fraction INTEGER NOT NULL CHECK(typeof(gross_fraction)='integer' AND gross_fraction>=0 AND gross_fraction<1000000000000),
 event_count INTEGER NOT NULL CHECK(typeof(event_count)='integer' AND event_count>=0),
 paid_principal INTEGER NOT NULL CHECK(typeof(paid_principal)='integer' AND paid_principal>=0),
 paid_fees INTEGER NOT NULL CHECK(typeof(paid_fees)='integer' AND paid_fees>=0),
 applied_at_unix INTEGER NOT NULL CHECK(typeof(applied_at_unix)='integer' AND applied_at_unix>=0),
 CHECK(previous_epoch<>next_epoch)
);
CREATE TRIGGER IF NOT EXISTS pps_budget_extensions_no_update
BEFORE UPDATE ON pps_budget_extensions BEGIN
 SELECT RAISE(ABORT, 'immutable PPS budget extension');
END;
CREATE TRIGGER IF NOT EXISTS pps_budget_extensions_no_delete
BEFORE DELETE ON pps_budget_extensions BEGIN
 SELECT RAISE(ABORT, 'immutable PPS budget extension');
END;
