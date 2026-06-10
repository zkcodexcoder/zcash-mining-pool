-- Operator acknowledgement for clawbacks: records the absorb/net decision
-- so the reconciler stops alerting on handled entries.
ALTER TABLE orphan_clawbacks ADD COLUMN acknowledged INTEGER NOT NULL DEFAULT 0;
ALTER TABLE orphan_clawbacks ADD COLUMN ack_note TEXT;
