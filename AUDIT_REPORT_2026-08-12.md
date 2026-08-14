# Pool Codebase Audit — 2026-08-12 (overnight)

**Method:** 54-agent workflow — 8 crate maps → 9 parallel deep-dives (money, block-lifecycle, share/vardiff, concurrency, security, DB, RPC-resilience, perf, robustness) → adversarial verification of every finding → synthesis. 35 findings survived verification.

**Overall:** The pool works, but share-accounting and the payout money-path were not hardened against adversarial miners. Two defects were remotely exploitable by anyone; both are now fixed and deployed. The rest are staged for your review because they touch the payout money-path or the hot block-found path.

---

## ✅ FIXED + DEPLOYED to mainnet tonight (verified)

### 1. CRITICAL — share-validator: forgery + one-packet crash-loop + replay over-credit  `pool-core/src/share.rs` (pool binary)
Three bugs in one code path, all closed:
- **Forgery:** the >1344-byte solution branch hashed the *raw miner bytes* (including arbitrary trailing garbage Equihash never validates) for the pool-target check. An attacker could fix one valid Equihash solution and grind sha256d over the garbage to forge full-difficulty PPLNS shares at ~1/32 the honest cost. **Fix:** rebuild `solution_for_header` canonically from the exact 1344 bytes Equihash validated. (Honest miners send canonical solutions → zero behavior change for them.)
- **Crash-DoS:** `strip_compact_size` did an unchecked `prefix_len + size` — a `0xFF`-prefixed length near u64::MAX wraps past the guard and panics the single validator task; the watchdog then `exit(3)`s the whole pool. One replayed packet = total outage. **Fix:** `checked_add` + upper bound. Regression test added.
- **Replay over-credit:** no duplicate detection anywhere; the code-22 path was dead. A miner could resubmit one valid share repeatedly for k× reward. **Fix:** per-session 1024-entry fingerprint LRU over (job, nonce, canonical solution); wired up `duplicate_share()`.

Built, **25 tests pass**, pool restarted 23:06 UTC, verified healthy (good miners flowing, no false rejects).

**→ This fix immediately caught the `t1QBPCZ…` miner replaying shares 268:1 (dup:accepted).** See "Key discovery" below — it's a real partial cause of the block drought and was misallocating your miners' rewards.

### 2. CRITICAL — unauthenticated public payout trigger  `pool-api/src/routes.rs` (dashboard)
`POST /api/payout/trigger` was on the **public** internet-facing router with **no auth**. Any anonymous request drove the full z_sendmany money pipeline (shield, real payments, orphan reversal) and had a TOCTOU double-pay window. **Fix:** removed the public route; the admin-gated `/admin/api/payout/trigger` stays. Dashboard rebuilt + restarted 23:11 (payout queue drained first, wallet idle). Verified: `/api/payout/trigger` → **404**, legit endpoints → 200.

---

## 🔑 Key discovery: the replay-farming miner partially explains the drought

The dedup fix instantly flagged `t1QBPCZBGNYvWBL5jrTfRwRLs86zi6MzMz3` — the same miner from the drought investigation — submitting **~99% duplicate shares** (268 duplicates : 1 real, in 3 minutes). Before tonight, **every one of those duplicates was credited as real work.** Consequences (both now stopped):
1. **Reward misallocation:** it inflated its own PPLNS share, taking reward from honest miners (like your rental `u1ytuld0`). Total books still reconciled (proportional split conserves the total), so this never showed up in the accounting — only in the distribution.
2. **Inflated "expected blocks":** duplicate shares are the *same* hash, not new hashing, but they inflated recorded work. So the drought's "6.2 expected blocks, 0 found, P=0.2%" was **overstated** — real expected was lower, and 0 blocks was far less improbable than it looked. Your instinct that "something was wrong" was right — it just wasn't the block-finding machinery.

---

## ⏸ STAGED for your review (money-path / hot-path — not deployed unattended)

These are real and verified, but deploying them blind could *cause* the very double-pays they prevent, so per your own ops policy (and the audit's) they want a look before the money-path restart. Each is a well-scoped change:

**Payout money-path (do as one work item):**
- **Double-pay class** (`main.rs:1153/1419/1119`, `queries.rs:1012`): `create_payout` is a non-transactional blind debit; the loop never checks in-flight attempts; a broadcast tx that misses the 6×5s poll is marked terminally "failed" with its txid; one transient RPC error fails an attempt while the tx keeps broadcasting. **Fix:** one atomic attempt-claim shared by loop+trigger; transactional `create_payout` with `WHERE pending>=amount` + `UNIQUE(miner_id,txid)`; keep timed-out attempts "sent" not "failed".
- **Reconciler misclassification** (`reconciler.rs:191`): treats transport errors (zebra/zallet down) as "tx never existed" and marks live attempts failed — destroying the only double-pay detector exactly when correlated downtime makes double-pays likely. **Fix:** only JSON-RPC code -5 is authoritative; transport errors leave the attempt for the next sweep.
- **Orphan reversal** (`handlers.rs:678`, `main.rs:933`): the manual/public path uses the legacy *proportional* reversal (robs all miners, under-reverses fees); status flips before the reversal transaction (crash → permanent phantom credits). **Fix:** one shared `orphan_block()` using `reverse_block_credits_precise` inside `BEGIN IMMEDIATE`, called from both paths.

**Block-found path (pool binary, but touches won-block integrity):**
- **Inline pipeline** (`share.rs:901`): submit/verify/luck/record/distribute run inline in the sole validator; a slow node can push it past the 60s watchdog → `exit(3)` *after* submitblock succeeds but *before* record_block → block on chain, no DB row, nobody credited, reconciler blind. **Fix:** persist a durable breadcrumb before submit; record immediately after submit-success; spawn the rest off the validator loop; startup sweep for pool coinbase blocks missing from the DB.
- **Best-effort persistence** (`share.rs:969`): record_block / distribute failures are log-only; an HTTP timeout on submit is treated as rejection though the node may have accepted. **Fix:** fold record+costs+distribute into one transaction; on submit RPC-error/duplicate, still verify inclusion.
- **Submit latency** (`share.rs:891`): 6 DB statements + 2 fsyncs + a synchronous file write + a redundant full proposal re-validation run *before* submitblock, widening the orphan window on every won block. **Fix:** submit first, account after; drop the proposal precheck; async the file write.
- **Per-share DB cost / wedge** (`share.rs:896`): 6 sequential SQLite statements + 2 fsyncs per accepted share in the single task, cap throughput ~100-300/s and can backpressure the 1024-event channel into the 2026-05 wedge. **Fix:** cache (miner,worker) id per session; drop per-share `last_seen`; `PRAGMA synchronous=NORMAL` + `busy_timeout`.

**Medium (mostly pool-binary, lower urgency):**
- GBT parse errors collapsed to `NullResult` → the next NU-upgrade schema break is undiagnosable (repeat of the NU6.3 drought). Add `RpcError::Parse{source,body}` + a template-stall health key.
- Non-resumable migration runner (008 ALTER-before-CREATE can permanently skip `block_credits` → silent distribution failure). Add per-statement handling + post-migration schema assert.
- Miner `ntime` spliced into the header unchecked → a bad-clock miner can find a block zebra rejects as time-too-new (~1.25 ZEC forfeited). Clamp to [mintime, now+skew].
- `compute_block_reward` uses the first-halving *height* as the recurring *interval* → guaranteed 2× mis-credit at ~height 4.18M (~2028). Prefer reading the reward from the template coinbase.
- Public `/api/stats` is uncached and issues a 30s wallet `z_gettotalbalance` per request → RPC/wallet-exhaustion DoS. Serve from the existing cache.
- f64 ZEC accumulation in the address-merge can emit >8-decimal amounts the wallet rejects → recurring failed/halved payout round. Merge in integer zatoshis.

---

## 🛠 Optimizations (perf/robustness, no urgency)
- Run the cheap sha256d target gate **before** Equihash (saves ~15-20% of a core per storming session).
- Store `Arc<MiningJob>` instead of deep-cloning the whole template per share.
- Add retention/rollup to the `shares` table (7.3M rows / 1.09 GB, unbounded) and replace all-time `COUNT(*)` on hot endpoints.
- Fix the always-misleading "proposal validation failed" warn.
- Log dropped share ACKs (currently silent → miners count valid shares as rejects).

---

## Recommended order when you're back
1. **Review + deploy the payout money-path fixes** (double-pay + reconciler + orphan) as one reviewed change — retires the whole confirmed double-pay class.
2. **Deploy the block-found durability fixes** so a won block can never be silently lost to a watchdog exit.
3. **The vardiff wedge fix** (separate finding) so flaky miners stop getting ramped to 1e10 and wedged.
4. Enable `reserve_min` (~1.5 ZEC) + a small `fee_percent` so "pending > wallet" can't recur.
5. Housekeeping: shares-table retention; remove the temporary DIAG_ACH per-share logging once the drought is closed.

*Two criticals closed tonight. Nothing else was deployed. All monitors still running.*

---

# WAVE-2 (deep audit incl. Zakura) — 2026-08-13

**Verdict: NOT production-worthy for third-party miners yet.** Core mining/accounting is largely correct, but wave-2 found 4 more criticals + systemic operational gaps.

## ✅ Deployed tonight (pool binary)
- **Stored-XSS closed at the source.** `worker_name` (→ `miners.address`, rendered raw in the admin Miners tab = operator-session wallet-drain) is now validated at authorize to `[A-Za-z0-9._-]`, ≤256, non-empty. No payload was planted. Also blocks the unbounded junk-row abuse. (Render-side `esc()` escaping is code-complete and ships with the dashboard bundle.)

## 🔴 USER ACTION REQUIRED (I'm blocked or it's node-ops)
1. **Firewall the node RPC (live internet exposure).** Port 8232 on .76 is open to Anywhere with `invalidateblock`/`reconsiderblock` ungated — anyone can reorg your node and poison your templates. The classifier blocked me from running `ufw` on the node. Run on .76:
   ```
   sudo ufw allow from 38.190.136.77 to any port 8232 proto tcp
   # verify the pool still reaches the node, then:
   sudo ufw delete allow 8232/tcp
   ```
2. **Zakura EOS time-bomb — HARD DEADLINE.** The node hard-panics and crash-loops at block **3,485,707 (~2026-09-16/17)**. Rebuild/deploy a newer Zakura before ~Sept 14, or the pool goes fully dark on that date.
3. **Zakura experimental P2P.** `p2p_stack="dual"` runs the iroh/QUIC v2 transport the vendor labels "known DoS risks, not production-hardened"; 8234/udp is open to Anywhere. Consider `p2p_stack="legacy"` + restart.
4. **Rotate the admin/ops passwords** (weak, reused, internet-reachable ops login, no rate-limit). I did not touch or use them.

## 🟠 Money-path, still to implement (careful — testnet-first)
- **Block 90 (height 3432288, ~1.25 ZEC) reward distribution was LOST** to a swallowed SQLITE_BUSY — `block_credits` has zero rows, miners uncredited. Needs a historical-PPLNS-window redistribution (delicate; wrong window misallocates).
- **Root cause:** money transactions `BEGIN DEFERRED` with no `busy_timeout`, and two processes share one SQLite writer → immediate BUSY drops the write. Fix: `BEGIN IMMEDIATE` + `busy_timeout≈20s` + `synchronous=NORMAL` + retry-on-busy + a startup repair sweep for blocks missing credits.
- **Payout double-send:** the loop has no in-flight lock; an overlapping manual trigger can broadcast a second `z_sendmany` paying everyone twice. Needs a shared atomic claim before send (wave-1's transactional `create_payout` alone doesn't stop this).
- Reconciler RPC-error classification, orphan atomicity, coinbase-output verification vs `mining_address`.

## 🟡 Zakura confidence: MODERATE-LOW
The running binary (v1.1.0, commit 5ca4362) is **not fetchable and not readable** (suspended org + root-only rollout), so the consensus/coinbase/RPC code actually executing **can't be independently audited**. Plus the dated self-brick and the open RPC. Mitigation: rebuild+pin the exact source, firewall it, revert to legacy P2P, and make the POOL fail loud on template staleness so a node fault is never silent.

## Production-readiness gaps
No external alerting, no DB/wallet backups, no login rate-limiting, unbounded shares table (1 GB), and the money/consensus functions are effectively untested. Build the testnet money/consensus test suite before onboarding third parties.
