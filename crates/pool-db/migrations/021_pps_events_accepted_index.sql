-- /pps Pool P&L: daily credit totals read pps_events by epoch and accepted_at.
-- Without this index every page refresh scanned the whole share ledger. Additive only.
CREATE INDEX IF NOT EXISTS pps_events_epoch_accepted ON pps_events(epoch_id, accepted_at);
