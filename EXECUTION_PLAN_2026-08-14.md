# Execution Plan — drafted overnight 2026-08-14 (~01:00 UTC)

Scope: the 8 open tasks (#10-#18 board). Ordered by money-risk first, then integrity, then breadth.
Everything below is line-anchored from tonight's code reads — tomorrow is implementation, not discovery.

**Standing rules:** money-path → testnet-validate first; drain-check (`payout_attempts` queued/sent + `payout_items`)
before any dashboard restart; warn before pool restarts (miners disconnect); build ON testnet for testnet
(glibc 2.35 vs 2.39 — binaries don't travel).

---

## Day 1 — #13 + #14: dashboard money-integrity pair (one deploy)

### #13 Reconciler hardening (transport ≠ not-found)
**Problem:** any RPC failure is treated as "tx doesn't exist". In `resolve_stale_attempts` that fails live
attempts; in round-3's `reconcile_reserved_payouts` an `Err(_)` from `get_raw_transaction` past the 60-min
expiry **refunds a reservation whose tx may be mined** → double-pay. A >60-min node outage with one payout
in flight is the trigger scenario.

**Design** (`RpcError` already carries what we need — `JsonRpc(JsonRpcError{ code: i64, .. })`, types.rs:127):
1. node-rpc: add `impl RpcError { pub fn is_definitely_not_found(&self) -> bool }` →
   `matches!(self, RpcError::JsonRpc(e) if e.code == -5)`.
2. reconciler.rs `resolve_stale_attempts`: the `Err(_)` arm of `get_raw_transaction` splits:
   `-5` → current "failed / txid not on chain" path; anything else → log + `continue` (leave for next sweep).
   Same split in the opid branch (`z_get_operation_status` transport error ≠ op-failed).
3. `reconcile_reserved_payouts`: refund ONLY on (`-5` past expiry) or (op reported `failed`).
   Transport error → alert + leave, regardless of age. An un-resolvable reservation parks funds in
   `paying` (safe) instead of risking a refund-then-mine double-pay.
4. Ongoing coinbase check: in the reconciler sweep, for blocks confirmed since last sweep, verify the
   coinbase pays `mining_address` (one `getrawtransaction` per new block; alert on mismatch).
**Tests:** reconciler mock-RPC tests exist (reconciler.rs tests mod) — add: transport-error leaves attempt;
-5 fails it; reserved+transport never refunds. **Validate:** testnet (kill zebra RPC reachability for a
sweep window; observe "leave for next sweep").

### #14 Neutralize the parallel money-path in `trigger_payout` + orphan unification
**Problem (bigger than the audit recorded):** `pool-api/src/handlers.rs:620-930` reimplements the whole
pipeline: own maturity check with **legacy proportional reversal** + status-flip-before-reversal (677-678),
own shielding, own payouts with **create_payout after z_sendmany** (line ~920) — fully bypassing round-3's
reservation saga. Any admin trigger crash mid-loop re-opens the double-pay class.

**Design — delete, don't maintain:**
1. `AdminState` gains `payout_wake: Arc<tokio::sync::Notify>`; dashboard's `run_payout_loop` waits on
   `select!{ sleep(interval), wake.notified() }`.
2. `trigger_payout` handler body → drain-check info + `wake.notify_one()` + JSON "nudged the loop"
   (keeps the admin button working; kills ~300 lines of duplicated money code).
3. Shared orphan path: new pool-db method `orphan_block(block_id)` — one BEGIN IMMEDIATE tx doing
   `update_block_status('orphaned')` **after** `reverse_block_credits_precise_txn` (reuse the round-2 `_txn`
   body; status flip and reversal commit together). `check_block_maturity` (main.rs:945-985) switches to it;
   legacy `reverse_block_credits` (queries.rs:922) kept only for pre-008 fallback inside `orphan_block`.
**Tests:** pool-db test: orphan_block reverses precisely + status atomic (crash-sim = assert both-or-neither);
trigger returns 200 and loop runs once (notify). **Validate:** testnet — trigger via admin, watch loop wake.

**Deploy Day 1:** both changes are dashboard+pool-db (dashboard binary only). Testnet build+soak (solo mode,
a few payout cycles + one forced-orphan if we can craft one), then mainnet drain-checked dashboard restart.

---

## Day 2 — #15 Block-found durability (pool binary)

**Anchored sequence today** (share.rs): `record_share`:938 → `submit_block`:957 → `verify_inclusion`:989 →
`compute_block_luck`:999 → `record_block`:1007 → `distribute`:1011 — all inline in the single validator task;
watchdog exit(3) after submit-success but before record_block = block on chain, nobody credited (block-90's
sibling). Failures at 1007/1011 are log-only.

**Design:**
1. **Breadcrumb:** before submit, `INSERT INTO block_submissions(height, hash, job_id, worker_id, at)`
   (new tiny table, migration 013). Cheap single insert, no fsync tuning needed beyond existing.
2. **Submit first:** move `submit_block` ahead of luck/record extras; on `Ok` **immediately** `record_block`
   (synchronous, minimal); then `tokio::spawn` the rest (inclusion verify → luck update → distribute) off
   the validator loop so a slow node can't hit the watchdog.
3. **Timeout ≠ rejection:** on submit transport error/timeout, still run `verify_block_inclusion`; if
   included → proceed as success.
4. **Startup sweep:** on pool start, for `block_submissions` rows with no matching `blocks` row, check the
   chain (getblockhash(height) + coinbase pays us) → if ours, `record_block` + distribute (idempotent via
   `INSERT OR IGNORE`-style guards). Completes the block-90-class prevention.
5. **Per-share cost:** cache `(miner_id, worker_id)` per session after first auth (share.rs currently
   re-resolves per share); batch/skip per-share `last_seen` (update on a 30s timer per session instead).
**Tests:** unit-test the sweep against a fake chain; simulate submit-timeout-but-included. **Validate:**
testnet finds blocks every ~10-15 min — deploy there, watch several real block-found cycles + kill the pool
between submit and record once (manual SIGKILL timing or a test hook) and watch the startup sweep recover it.
**Deploy:** miner-safe mainnet pool restart (user go-ahead; combines with #16 batch A if ready).

---

## Day 3 — #16 medium bundle + #10 fee revert (Aug 17)

Batch A (pool binary, rides one restart — ideally the same as #15's if timing works):
- **GBT parse classification** (drought-repeat preventer): node-rpc `RpcError::Parse{source, body_snippet}`
  instead of collapsing to `NullResult`; pool-core sets a `template_stale` pool_status key when template
  fetch fails N consecutive times → dashboard/API surfaces it loudly.
- **Halving bug** (share.rs:1159 `compute_block_reward`): prefer the reward already present in the template
  coinbase (we parse it — `actual_reward` path exists); keep the formula only as fallback with the interval
  fixed (854_916-block halving interval, not first-halving height).
- **ntime clamp:** locate the header splice (block assembly, verify at impl) → clamp miner ntime to
  `[curtime(template), now + 90s]`; reject outside (protects ~1.25 ZEC per bad-clock block).
- **ACK-drop logging + proposal-warn fix + sha256d-before-Equihash + Arc<MiningJob>** — small, same restart.

Batch B (dashboard, no urgency, next dashboard restart):
- **/api/stats** (routes.rs:16 `get_pool_stats_nomp`): serve wallet balance from the payout_health cache
  (pool_status) instead of a live 30s `z_gettotalbalance` per anonymous request.
- **Migration runner:** per-statement error handling + post-migration `PRAGMA table_info` asserts on
  critical tables (block_credits, payout_items); refuse to start if a critical table is missing.

**#10 fee revert (Aug 17):** config `fee_percent 1.0 → 0.0`; pool restart (warn user) + dashboard restart
(reconciler fee match, drain-checked). Fold into whatever restart lands that day. Collected margin stays.

---

## Day 4 — #11 convergence + #12 vardiff (testnet day)

**#11 (measured tonight):** queries.rs/models.rs = mainnet strict superset (ship as-is);
network.rs is bidirectional — port testnet's 🦓/🌸 marker feature (+77 lines: `marker`,
`dominant_marker`, `is_zebrad/is_zakura` display bits) INTO mainnet network.rs alongside labels;
admin.rs: reconcile session-helper variants (12 testnet lines vs mainnet's refactor — take mainnet's,
verify testnet login still works). Then ship the unified tree testnet-ward, build there, restart both
services, validate: labels + markers + ops-gating + native solo payouts. End state = one tree, per-env
config only. (GitHub still suspended: scp only.)

**#12 vardiff (5090-gui):** on testnet: enable per-share DIAG for that worker (tooling exists — DIAG_ACH),
capture assigned-vs-mined target for ~1h of its shares; check nheqminer's set_target handling upstream;
then decide pool-side accommodation: per-port/per-worker min-diff floor, or a grace window that re-sends
set_target and tolerates N stale-target shares before rejecting. Goal: real GPU miners with this client
don't bleed 8.7% rejects. Then remove/gate DIAG_ACH from mainnet share.rs (cleanup rider).

---

## Parallel track (no code deploys needed)

**#17 Zakura (USER + me, HARD DEADLINE ~Sept 14 — 31 days):**
1. Discovery (I can do): locate buildable Zakura source ≥ the EOS-fixed tag — check .76 and .77 for local
   clones/tarballs (`/opt`, `~zebra`, `~root` hints), valargroup mirrors, vendor contact. The running
   v1.1.0 (5ca4362) is unfetchable (suspended org) — if NO newer source exists, fallback plan: patch the
   EOS check out of whatever source we do have and pin it.
2. Build on .76 (its own glibc!), stage as `/opt/.../zakura-<tag>`, systemd swap with rollback, off-peak.
   Propose executing the swap ~Sept 1 (2-week buffer).
3. Same session: `p2p_stack="legacy"`, and the ufw 8232 allow-from-.77 rule (user checking with friends).
4. User: rotate admin + ops passwords (2 min in config + dashboard restart — can ride any Day-1/3 restart).

**#18 production-readiness (rolling, start Day 1-2):**
- **Backups (first — it's embarrassing we have none):** nightly cron: `sqlite3 pool.db ".backup"` to a
  dated file + `tar` the zallet datadir (service-stop-free: use the sqlite backup API + wal checkpoint;
  zallet: keep using the cold pre-rescan copy + add weekly warm copy), rotate 7 days, `scp` a copy off-box
  (testnet box as the off-site, reciprocal).
- **Alerting:** smallest thing that works — a 5-min cron healthcheck script (services active, API 200,
  shares-per-10min > 0, neg-balance check, zallet responsive) that on failure hits a user-provided webhook
  (Discord/Telegram/ntfy — ask user which they use). All checks already exist as SQL/curl from this session.
- **Login rate-limit:** simple per-IP token bucket in the admin/ops login handlers (in-memory HashMap).
- **Shares retention:** rollup table `shares_hourly` + batched delete of raw rows >30d (with #16's stats
  changes so no endpoint needs raw all-time COUNT(*)).
- **Money/consensus test suite:** extend pool-db integration tests into a lifecycle sim (find→credit→
  mature→pay→orphan→reverse over a fake chain) — grows out of the Day-1/2 tests naturally.

## Decision points for you (can answer any time)
1. Alerting webhook: which channel (Discord/Telegram/ntfy/email)?
2. `reserve_min`: enable at ~1.5 ZEC when the fee reverts (recommended), or skip?
3. Zakura swap date: propose ~Sept 1. OK?
4. Password rotation: you do it, or I generate+set and hand you the new ones?

## Suggested day map
| Day | Deploys | Items |
|-----|---------|-------|
| 1 (Aug 14) | 1 dashboard restart (mainnet) after testnet soak | #13, #14, backups+alerting start (#18) |
| 2 (Aug 15) | 1 pool restart (mainnet, miner-safe) after testnet block-cycles | #15 (+#16A if ready) |
| 3 (Aug 16-17) | ride-along restarts only | #16 A/B, #10 fee revert (17th), rate-limit |
| 4 (Aug 18) | testnet only | #11 convergence, #12 vardiff |
| ~Sept 1 | node swap (.76, off-peak) | #17 rebuild + firewall + p2p legacy |
