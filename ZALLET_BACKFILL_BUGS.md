# Two zallet bugs found during a wallet-recovery incident (testnet)

**Version:** zallet 0.1.0-alpha.4 (v0.1.0-alpha.3-144-g5c4e11a, lightly patched fork —
patches only add retry-on-recoverable-error to sync tasks; both bugs are in unmodified
code paths). Backing node: Zebra-family (zakurad 1.2.0), Zcash **testnet**.
Wallet: single account, mnemonic-derived, in continuous production use as a mining-pool
payout wallet (thousands of txs).

These are separable issues; filing as one report because they compound: #2 turned a
routine #1-style repair into an unrecoverable crash-loop.

---

## Bug 1: `repair truncate-wallet` leaves rows above the truncation height in several tables

### Symptom

After `zallet repair truncate-wallet <H>` reports success
(`Wallet truncated to a maximum height of <H>`), restart crash-loops with:

```
Failed to synchronize zallet: UNIQUE constraint failed: tx_locator_map.block_height, tx_locator_map.tx_index
```

and, once those rows are manually removed, with:

```
An error occurred updating the Ironwood note commitment tree while adding blocks in
the range 4308248..4308259: Inserted root conflicts with existing root at address ...
```

### Cause

Truncation to H=4308148 left behind (verified by direct SQL inspection):

- `tx_locator_map`: 9,825 of 10,358 rows had `block_height > H` (max 4308248).
  Rescan re-inserts locators for re-scanned blocks → UNIQUE violation.
- `sapling_tree_checkpoints` / `orchard_tree_checkpoints` / `ironwood_tree_checkpoints`:
  max `checkpoint_id` was 4308248/4308248/4308249 — all above H. Rescan then collides
  with the stale tree state ("Inserted root conflicts") in exactly the first re-scanned
  range above H.

### Workaround that recovered the wallet

```sql
DELETE FROM tx_locator_map WHERE block_height > <H>;
DELETE FROM sapling_tree_checkpoints  WHERE checkpoint_id > <H>;
DELETE FROM orchard_tree_checkpoints  WHERE checkpoint_id > <H>;
DELETE FROM ironwood_tree_checkpoints WHERE checkpoint_id > <H>;
```

### Suggested fix

`truncate-wallet` should delete/rewind these tables (and shard contents past the
truncation frontier — see Bug 2's interaction) as part of the same operation.

---

## Bug 2: history-recovery backfill dies with `SubtreeDiscontinuity` on the range straddling pool activation

### Symptom

Deterministic wallet exit (clean exit, then the supervisor's restart → tight loop;
~6,000 restarts/day) with:

```
PutBlocksCommitmentTree {
  pool: Ironwood,
  block_range: BlockHeight(4133999)..BlockHeight(4135000),
  error: Storage(SubtreeDiscontinuity { attempted_insertion_range: 0..1, existing_range: 2..3 })
}
```

followed by `Exiting Zallet because an ongoing task exited`.

**Testnet NU6.3 (Ironwood) activation height is exactly 4134000** — the failing range
straddles the activation boundary, and it fails identically on every wallet state we
tried: the live wallet, a truncated wallet, and a restored day-old backup.

### Analysis

After a truncation/restore, the wallet seeds the Ironwood tree from the chain tip side
(the node's subtree roots — verified the node serves correct `z_getsubtreesbyindex`
data for ironwood subtrees 0 and 1), leaving shard state for subtree range `2..3`.
The `recover_history` backfill then scans historic ranges (re-derived from the wallet
birthday) and attempts to insert Ironwood commitments starting at the activation
boundary — subtree range `0..1`. The shard store rejects the non-contiguous insertion
(`existing_range: 2..3`), the task exits, and the exit takes the whole wallet down.
The backfill re-runs on every start, so the wallet can never pass its own history.

Notable secondary point: the exit is status 0 ("task exited" → graceful shutdown), so
process-crash monitoring undercounts this failure mode; only the supervisor's restart
counter reveals it.

### Impact

Any wallet whose scan queue ever includes a backfill range at/below a pool-activation
boundary while its tree was seeded from the tip becomes permanently unable to start.
We could not find any recovery path in the CLI: truncation floors are above the
boundary, `scan_queue` edits are re-derived from the birthday within minutes, and
wiping the ironwood tree tables just re-manifests the same discontinuity. The only
exits are re-initializing the wallet with a post-boundary birthday (abandoning older
notes) or patching the code.

### Suggested fixes (any one)

1. When backfill needs to insert below the tree's lowest existing shard, first fetch
   and insert the intervening subtree roots (the node serves them) so the insertion is
   contiguous; or
2. Clamp/skip note-commitment-tree insertion for ranges below the pool's activation
   height + tree-seed frontier, marking them scanned-for-notes only; or
3. At minimum, treat `SubtreeDiscontinuity` in `recover_history` as a recoverable
   "defer this range" rather than a fatal task exit — the current behavior converts a
   gap into a permanent crash-loop.

---

### Repro sketch (testnet)

1. Run a wallet with pre-Ironwood history and a birthday below activation (4134000).
2. `zallet repair truncate-wallet <floor>` at any height above activation (Bug 1 also
   manifests here; clean the leftover rows to get past it).
3. Restart: forward sync seeds the Ironwood tree from tip-side subtree roots; the
   history backfill then hits `SubtreeDiscontinuity` at 4133999..4135000 and the
   wallet exits, forever.

Happy to provide full logs, the broken wallet.db (testnet — no value at risk), or to
test candidate fixes on our infrastructure.
