-- Audit B21: reconcile() looks up pps_payout_items and pps_payouts once per miner.
-- Without these indexes each lookup scanned the whole table. Additive only.
CREATE INDEX IF NOT EXISTS pps_payout_items_miner ON pps_payout_items(miner_id);
CREATE INDEX IF NOT EXISTS pps_payouts_miner ON pps_payouts(miner_id);
