# Block-90 / July Double-Pay Netting — 2026-08-13 ~23:55 UTC (user-approved "B-clamped")

## Background (full money-trail, all independently verified)
- **Block 90** (height 3432288, found 2026-08-01 04:36, confirmed, actual_reward 1.25379760 ZEC)
  lost its PPLNS distribution to a swallowed SQLITE_BUSY → `block_credits` empty, miners uncredited.
- **2026-07-22/23**: payout attempts 40-45 were marked `failed`
  (`wait_for_operation error: z_getoperationstatus failed: HTTP error`) but Zallet had already
  broadcast each tx → balances never debited → the 5-min loop re-paid from the remaining balance.
  **2.54575785 ZEC double-paid** across 6 on-chain txs (heights 3421521/23/25, 3422039/44/49).
- Ruled out en route: lost notes (full Zallet rescan from 3273000 — orchard byte-identical),
  Orchard-stuck funds (post-NU6.3 shielding lands in Ironwood; orchard spends fine),
  coinbase misdirection (105/105 blocks paid t1Uo7…), phantom income (all blocks on-chain).
  Recorded payouts matched wallet txs exactly (126.3184). Real fees 0.0659 vs 0.0198 booked.
- The double-paid recipients ARE the block-90 window miners (matched via
  z_listunifiedreceivers(UA).orchard == sent_notes.to_address).

## What was applied (single BEGIN IMMEDIATE tx on pool.db)
1. **Block-90 credited** (block_credits rows + pending):
   - 6466760 (u1ytuld0…): +1.2313 (123,128,149 zat)
   - 5882996 (u1cdr2zy…): +0.0117 (1,168,247 zat)
   - 1828308 (u1l2qz3…): +0.0108 (1,083,363 zat)
2. **July double-pays booked as payouts** (pending→paid, real on-chain txids, clamped at available pending):
   - 6466760: 2.4406 booked, 0.0019 forgiven
   - 5882996: 0.0299 booked, 0.0211 forgiven
   - 1828308: 0.0233 booked, 0.0290 forgiven
   - total booked 2.4938, total forgiven 0.0519 (clamp — pending never negative)

## End state (verified)
- All three miners: pending = 0.0000, no negative balances anywhere
- sum(payouts) == sum(paid) exact (ledger_ok=1)
- Accounting drift: −1.1921 → ~+0.06 (the known benign residue)
- Pool effectively solvent; the July overpayment is settled against block-90's debt
- Reconciler will raise ONE expected drift-delta alert on its next sweep, then self-clear

## Prevention (already live before this netting)
Round-3 pre-debit saga (deployed 2026-08-13 18:40) closes the exact mechanism:
reserve-before-send, never refund on ambiguity, chain-lookup reconciliation.
