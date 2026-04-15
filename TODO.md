# Zcash Mining Pool — TODO

Longer-term work that doesn't fit in a single session. Shorter task tracking
lives in `git log` / PR descriptions.

## Refactors

- [ ] **Hot-reloadable runtime config.** Lift vardiff + payout parameters
  into an `Arc<RwLock<RuntimeConfig>>` so changes to `pool.toml` don't
  require a pool restart (every restart disconnects miners and can break
  MRR-proxy rentals).
  - Candidates: `target_shares_per_minute`, `retarget_interval_secs`,
    vardiff stable-zone bounds, ramp clamp, EMA alpha, early-trigger
    threshold; `payout.minimum_payout`, `payout.interval_secs`,
    `payout.maturity_confirmations`; `coinbase_tag`; per-port
    `initial_difficulty`; `pool.banner`.
  - Add `POST /admin/api/config/reload` that re-reads `pool.toml`, diffs,
    updates only hot fields, logs every change.
  - Optionally: inotify watcher on `pool.toml` for auto-apply.
  - Keep restart-only (correctness-critical): stratum listen addrs,
    database URL, node RPC URL, pool/mining addresses, admin password.
  - Do this as **one clean pass** after vardiff parameters stabilize —
    not piecemeal while we're still tuning.

## Pool optimizations (from the priority list)

- [ ] **Race-to-tip empty-block notify.** On new prev_hash, issue a
  `clean_jobs=true` notify with an empty tx set immediately, then
  follow up with a full template once zebrad's `getblocktemplate`
  returns. Claws back ~100-200 ms of stale hashrate per block.
  Deferred — requires splitting job build into two phases and carries
  orphan risk if misconfigured.

- [ ] **zebrad peering / geo-placement.** Add a few well-connected
  peers to zebrad's peer list so we hear about new blocks faster.
  Infra work, not code.

## Vardiff (once parameters stabilize)

- [ ] Consider smoothing `shares_per_minute` itself with an EMA rather
  than only smoothing the ratio (`compute_retarget` in
  `crates/pool-core/src/difficulty.rs`).

## Monitoring / reliability

- [ ] Auto-detect the Zallet "stuck sync" state from the admin health
  page and offer a one-click repair (we've already built the
  `/admin/api/repair-zallet` endpoint — make the health page surface a
  button when the stuck pattern is detected).

- [ ] Alert route for critical conditions (pool down, zallet stuck,
  consecutive payout failures). Currently we have flags in the
  background `monitor.sh` but no notification path.
