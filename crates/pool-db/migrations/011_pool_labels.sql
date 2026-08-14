-- Operator-defined miner-address -> pool-name overrides for the Network tab.
-- Editable at runtime from the admin "Labels" tab; read live by the network
-- handler so changes apply without a dashboard restart. The built-in map in
-- pool-api/src/network.rs::identify_pool remains the fallback for any address
-- not present here.
CREATE TABLE IF NOT EXISTS pool_labels (
    address    TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    note       TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
