# zecd evaluation — running notes for the zecd team (zec.rocks)

Context: evaluating zecd 0.7.0 (built from source, commit 2c610c7) as the payout
wallet for a mining pool. Watch-only wallet restored from a UFVK with ~214k blocks
of history to scan (birthday 4,100,000, testnet) containing 60,000+ wallet
transactions (months of 5-minute pool payouts + coinbase shielding). Backend:
remote zebra-family node over `zebra://` JSON-RPC. Box: 8-core Linux, otherwise idle.

## Issues to report

1. **`/status` and `/readyz` starve during initial sync.** From ~4 minutes after
   start (once scanning got busy) through many hours, `/status` and `/readyz` do
   not answer within 15–20s; only `/healthz` responds ("ok"). Presumably the
   handlers query the wallet actor, which is saturated by scanning. Impact:
   monitoring cannot observe sync progress during exactly the phase where you
   want it most (we fell back to grepping the log for `fetching compact blocks`
   ranges). Suggestion: serve status from a cached snapshot the sync loop
   updates, rather than a live actor round-trip.

2. **Docs/config confusion around Ironwood as a pool name.** Release notes and
   docs advertise Ironwood support (correct — the daemon loads an Ironwood
   proving key and stores Ironwood notes), but `[pools] enabled = ["ironwood"]`
   is rejected ("supported pools are sapling, orchard, transparent"). Since
   Ironwood rides Orchard receivers this is technically right, but a line in the
   example config saying "Ironwood is implicit via orchard" would save
   head-scratching.

## Data points they may want (positive)

- **Large-wallet initial sync benchmark**: 60,000+ transactions absorbed inline
  during a single scanning pass; RSS flat at ~317MB throughout (no growth with
  history depth); CPU 40–70% of one core; zero errors over 6+ hours of
  continuous scanning. Dense regions (~10k wallet txs per 10k blocks) process at
  ~130 tx/min stored. For comparison, the incumbent wallet (zallet alpha)
  required a multi-hour scan plus ~3 days of a pegged core to enhance the same
  history after a rescan.
- `zebra://` against a zebra *fork* (zakurad 1.2.0) works, including the
  non-loopback plaintext warning being warn-not-refuse (useful for LAN
  topologies).
- `chain-info`, `config check` (rejects unknown keys with exact field lists),
  and non-interactive `init --restore` / `--ufvk` are all excellent operator UX.

## Update at sync completion (t+20.5h)

3. **`/status` starvation persists even after reaching tip** while the wallet
   works through its post-scan transparent spend-search/enhancement backlog
   (CPU ~75%, single-block ChainTip ranges). So the starvation is not just an
   "initial sync" cosmetic issue — any sustained wallet-actor load blocks the
   monitoring surface. Strengthens the cached-snapshot suggestion in issue 1.

4. **(Minor, integration note)** `getbalances` returns amounts as JSON numbers
   (e.g. `1.25000000`), while zcashd-lineage `z_gettotalbalance` returned
   strings. Fine per Bitcoin Core convention — just worth a compatibility note
   in the migration docs, since zcashd-era clients parse strings.

### Final initial-sync benchmark (the headline data point)

- Wallet: UFVK watch-only, birthday 4,100,000, testnet; scanned to tip 4,315,437
  (~215.4k blocks) in **~20.5 hours** over a remote `zebra://` JSON-RPC upstream.
- **128,470 transactions stored** inline during the pass (a mining pool's payout
  wallet: months of 5-minute payouts + coinbase shielding).
- **Zero errors or restarts.** RSS 191MB → 438MB peak (essentially flat per-tx);
  CPU 40–82% of one core. Dense regions (~1 wallet tx per block) ~130 tx/min.
- Balance surfaced correctly at tip (trusted/untrusted_pending/immature/coinbase
  split all populated, including mature-coinbase — useful for pool shielding).
- Notably, zecd's full-history view recovered balance visibility that the
  incumbent wallet had lost to note-commitment-tree corruption surgeries —
  the stateless rebuild-from-key model demonstrating exactly its advertised
  advantage.

## Reorg + resilience results (day 2 — the decisive test)

- **~25 chain reorgs handled flawlessly** over the first night at tip, including
  six within 25 minutes: each logged as "Chain reorg detected at H, rewinding"
  and processed in-process with zero crashes, corruption, or restarts. (Context:
  this identical testnet reorg pattern repeatedly corrupted the incumbent
  wallet's note commitment tree into deterministic crash-loops.)
- One transient upstream disconnect (getblock -5 during what looks like a reorg
  race) was WARNed and reconnected cleanly — resilient-by-default sync tasks.

5. **Steady-state CPU at tip is significant for a wallet with many funded
   transparent addresses**: ~61% of a core in the hours after reaching tip,
   easing to ~44% by the next morning. Activity mix suggests per-cycle
   transparent spend-detection over a large funded-address set (this wallet's
   payouts loop back to its own addresses, so the set is unusually large —
   admittedly pathological). The docs do warn about this; a data point on how it
   scales may still be useful, plus it keeps `/status` starved (issue 1/3).

## Final active-test results (evaluation COMPLETE except live pool trial)

- Spot-check of payout txids: PASSED (3/3 found, correct confirmations/amounts).
- Restart recovery: PASSED — the 128k-tx wallet returned to ready (lag 0,
  enhancements 0) in minutes from its cache; no history re-scan.
- Steady-state latency: `getbalances` ~385ms. **`gettransaction` ~4.6s flat on a
  128k-tx wallet even when idle** (finding 6: per-lookup cost appears to scale
  with wallet size — likely worth an index).
- `z_sendmany`: works as documented; **operation completed with txid in ~5
  seconds** (the incumbent needed minutes for equivalent sends). Minor: the
  `ANY_ORCHARD` wildcard from the v0.7.0 docs was rejected as an invalid from
  address (finding 7 — docs/impl drift, or arrives post-0.7.0); the wallet's UA
  and `ANY_TADDR` both work.
- **Operation status is memory-only** (finding 8, suggestion not bug): after a
  daemon restart, `z_getoperationstatus` returns `[]` for both mid-flight and
  completed operations — same semantics as zcashd. The transaction itself is
  durable and immediately queryable post-restart. Payment processors that crash
  between send and status-read must resolve fate by txid on chain (ours does);
  a persisted op journal would remove that burden.
- `z_shieldcoinbase` empty case: exactly as documented (`-6` "Could not find any
  coinbase funds to shield").

## Spending-wallet restore (2026-09-02, seed-restored wallet at 130k txs)

- Full restore from mnemonic on the same daemon as the UFVK watch-only wallet:
  synced all history with zero errors; both wallets report identical
  `getbalances` at the same `lastprocessedblock`, and trusted+immature matches
  the incumbent wallet's `z_gettotalbalance` total to the zatoshi.
- Finding 9: **`listtransactions` pagination cost is O(history), not O(page).**
  On the 130k-tx wallet, default (10 entries) answers in seconds, `count=100`
  takes ~140s, and `count=300` or any `skip` offset (e.g. `["*",30,300]`)
  exceeds 170s and times out. `skip` does not avoid the walk. A pool does not
  need this method in its hot path, but incident forensics ("did this send
  ever leave the wallet?") do — we ended up reading `data.sqlite`
  (`sent_notes` ⋈ `transactions`) directly, which answers instantly.
  Suggest paging from an indexed cursor (id_tx / mined_height).

## Overall verdict for our use (pool payout wallet)

PASS on everything testable without holding the spending seed. Standout results:
reorg resilience (25+ reorgs, zero incidents), stateless restart recovery,
send-path speed, and large-wallet absorption. All integration deltas are small
and enumerated above. Next step on our side: wallet-dialect adapters, then a
live testnet trial as the pool's payout wallet.
