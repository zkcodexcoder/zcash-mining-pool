use pool_db::PoolDb;
use sqlx::sqlite::SqlitePoolOptions;

async fn setup_db() -> PoolDb {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory database");
    let db = PoolDb::new(pool);
    db.run_migrations().await.expect("Failed to run migrations");
    db
}

#[tokio::test]
async fn test_miner_create_and_get() {
    let db = setup_db().await;

    let miner = db.get_or_create_miner("t1TestAddress123").await.unwrap();
    assert_eq!(miner.address, "t1TestAddress123");
    assert!(miner.id > 0);

    // Getting again should return the same miner
    let miner2 = db.get_or_create_miner("t1TestAddress123").await.unwrap();
    assert_eq!(miner.id, miner2.id);
}

#[tokio::test]
async fn test_worker_create_and_list() {
    let db = setup_db().await;

    let miner = db.get_or_create_miner("t1Miner1").await.unwrap();
    let w1 = db.get_or_create_worker(miner.id, "rig1").await.unwrap();
    let w2 = db.get_or_create_worker(miner.id, "rig2").await.unwrap();
    assert_ne!(w1.id, w2.id);

    let workers = db.get_workers_for_miner(miner.id).await.unwrap();
    assert_eq!(workers.len(), 2);
}

#[tokio::test]
async fn test_share_recording() {
    let db = setup_db().await;

    let miner = db.get_or_create_miner("t1ShareMiner").await.unwrap();
    let worker = db.get_or_create_worker(miner.id, "gpu0").await.unwrap();

    for i in 0..5 {
        db.record_share(worker.id, &format!("job{i}"), 1.0, false, "test_session")
            .await
            .unwrap();
    }

    let count = db.get_total_shares_count().await.unwrap();
    assert_eq!(count, 5);

    let shares = db.get_last_n_shares(3).await.unwrap();
    assert_eq!(shares.len(), 3);
}

#[tokio::test]
async fn test_block_recording() {
    let db = setup_db().await;

    let miner = db.get_or_create_miner("t1BlockMiner").await.unwrap();
    let worker = db.get_or_create_worker(miner.id, "asic1").await.unwrap();

    let block_id = db
        .record_block(100, "aabbccdd", 1_000_000_000, Some(1_000_135_000), worker.id, Some(85.5))
        .await
        .unwrap();
    assert!(block_id > 0);

    let blocks = db.get_recent_blocks(10).await.unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].height, 100);
    assert_eq!(blocks[0].status, "pending");

    db.update_block_status(block_id, "confirmed").await.unwrap();
    let blocks = db.get_recent_blocks(10).await.unwrap();
    assert_eq!(blocks[0].status, "confirmed");
}

#[tokio::test]
async fn test_balance_crediting() {
    let db = setup_db().await;

    let miner = db.get_or_create_miner("t1BalanceMiner").await.unwrap();

    db.credit_balance(miner.id, 500_000_000).await.unwrap();
    db.credit_balance(miner.id, 250_000_000).await.unwrap();

    let balance = db.get_or_create_balance(miner.id).await.unwrap();
    assert_eq!(balance.pending, 750_000_000);
    assert_eq!(balance.paid, 0);
}

#[tokio::test]
async fn test_pplns_shares_query() {
    let db = setup_db().await;

    let miner_a = db.get_or_create_miner("t1MinerA").await.unwrap();
    let miner_b = db.get_or_create_miner("t1MinerB").await.unwrap();
    let worker_a = db.get_or_create_worker(miner_a.id, "w1").await.unwrap();
    let worker_b = db.get_or_create_worker(miner_b.id, "w1").await.unwrap();

    // Miner A: 3 shares of difficulty 2.0 = 6.0 total
    for _ in 0..3 {
        db.record_share(worker_a.id, "j1", 2.0, false, "sess_a").await.unwrap();
    }
    // Miner B: 2 shares of difficulty 3.0 = 6.0 total
    for _ in 0..2 {
        db.record_share(worker_b.id, "j1", 3.0, false, "sess_b").await.unwrap();
    }

    let pplns = db.get_pplns_shares(100).await.unwrap();
    assert_eq!(pplns.len(), 2);

    let total: f64 = pplns.iter().map(|p| p.total_difficulty).sum();
    assert!((total - 12.0).abs() < 0.001);
}

// ===== Audit Phase C tests =====

#[tokio::test]
async fn test_distribute_block_credits_atomic_and_balances() {
    let db = setup_db().await;
    let m1 = db.get_or_create_miner("utest1m1").await.unwrap();
    let m2 = db.get_or_create_miner("utest1m2").await.unwrap();
    let w = db.get_or_create_worker(m1.id, "rig").await.unwrap();
    let block_id = db
        .record_block(200, "feed01", 125_000_000, Some(125_300_000), w.id, None)
        .await
        .unwrap();

    db.distribute_block_credits(block_id, &[(m1.id, 90_000_000), (m2.id, 35_300_000)])
        .await
        .unwrap();

    let b1 = db.get_or_create_balance(m1.id).await.unwrap();
    let b2 = db.get_or_create_balance(m2.id).await.unwrap();
    assert_eq!(b1.pending, 90_000_000);
    assert_eq!(b2.pending, 35_300_000);
}

#[tokio::test]
async fn test_precise_reversal_exact() {
    let db = setup_db().await;
    let m1 = db.get_or_create_miner("utest1r1").await.unwrap();
    let m2 = db.get_or_create_miner("utest1r2").await.unwrap();
    let w = db.get_or_create_worker(m1.id, "rig").await.unwrap();
    let block_id = db
        .record_block(201, "feed02", 125_000_000, None, w.id, None)
        .await
        .unwrap();
    db.distribute_block_credits(block_id, &[(m1.id, 100_000_000), (m2.id, 25_000_000)])
        .await
        .unwrap();
    // An unrelated miner with pending must be untouched (the old proportional
    // reversal would have clawed from them).
    let bystander = db.get_or_create_miner("utest1by").await.unwrap();
    db.credit_balance(bystander.id, 500_000_000).await.unwrap();

    let outcome = db.reverse_block_credits_precise(block_id).await.unwrap();
    assert_eq!(outcome, Some((125_000_000, 0)));

    assert_eq!(db.get_or_create_balance(m1.id).await.unwrap().pending, 0);
    assert_eq!(db.get_or_create_balance(m2.id).await.unwrap().pending, 0);
    assert_eq!(
        db.get_or_create_balance(bystander.id).await.unwrap().pending,
        500_000_000,
        "bystander must not fund someone else's orphan"
    );

    // Double reversal is a no-op (credit rows consumed).
    let again = db.reverse_block_credits_precise(block_id).await.unwrap();
    assert_eq!(again, None);
}

#[tokio::test]
async fn test_precise_reversal_shortfall_creates_clawback() {
    let db = setup_db().await;
    let m = db.get_or_create_miner("utest1cb").await.unwrap();
    let w = db.get_or_create_worker(m.id, "rig").await.unwrap();
    let block_id = db
        .record_block(202, "feed03", 125_000_000, None, w.id, None)
        .await
        .unwrap();
    db.distribute_block_credits(block_id, &[(m.id, 125_000_000)])
        .await
        .unwrap();
    // Simulate the credit being paid out before the orphan was detected.
    sqlx::query("UPDATE balances SET pending = pending - 125000000, paid = paid + 125000000 WHERE miner_id = ?1")
        .bind(m.id)
        .execute(db.inner())
        .await
        .unwrap();

    let outcome = db.reverse_block_credits_precise(block_id).await.unwrap();
    assert_eq!(outcome, Some((0, 125_000_000)), "all of it should be clawback");

    let clawbacks = db.get_recent_clawbacks(1).await.unwrap();
    assert_eq!(clawbacks.len(), 1);
    assert_eq!(clawbacks[0], (block_id, m.id, 125_000_000));
    // Pending must not go negative.
    assert_eq!(db.get_or_create_balance(m.id).await.unwrap().pending, 0);
}

#[tokio::test]
async fn test_legacy_block_returns_none() {
    let db = setup_db().await;
    let m = db.get_or_create_miner("utest1leg").await.unwrap();
    let w = db.get_or_create_worker(m.id, "rig").await.unwrap();
    // Pre-008-style block: recorded but no block_credits rows.
    let block_id = db
        .record_block(203, "feed04", 125_000_000, None, w.id, None)
        .await
        .unwrap();
    assert_eq!(db.reverse_block_credits_precise(block_id).await.unwrap(), None);
}

#[tokio::test]
async fn test_invariant_uses_actual_reward_and_clawbacks() {
    let db = setup_db().await;
    let m = db.get_or_create_miner("utest1inv").await.unwrap();
    let w = db.get_or_create_worker(m.id, "rig").await.unwrap();
    // Block with fees: subsidy 1.25, actual 1.253.
    db.record_block(204, "feed05", 125_000_000, Some(125_300_000), w.id, None)
        .await
        .unwrap();
    sqlx::query("INSERT INTO orphan_clawbacks (block_id, miner_id, amount) VALUES (999, ?1, 700)")
        .bind(m.id)
        .execute(db.inner())
        .await
        .unwrap();
    let (reward, _balances, clawbacks) = db.get_accounting_invariant().await.unwrap();
    assert_eq!(reward, 125_300_000, "must use actual_reward when present");
    assert_eq!(clawbacks, 700);
}

#[tokio::test]
async fn test_immature_block_credits_not_payable() {
    let db = setup_db().await;
    let m = db.get_or_create_miner("utest1mat").await.unwrap();
    let w = db.get_or_create_worker(m.id, "rig").await.unwrap();
    // Immature block credited 1.2375 — must NOT be payable.
    let block_id = db
        .record_block(300, "feedaa", 125_000_000, Some(125_000_000), w.id, None)
        .await
        .unwrap();
    db.distribute_block_credits(block_id, &[(m.id, 123_750_000)])
        .await
        .unwrap();

    let payable = db.get_pending_payouts(1_000_000, false, 0, i64::MAX).await.unwrap();
    assert!(
        payable.iter().all(|p| p.miner_id != m.id),
        "immature credits must not be payable: {payable:?}"
    );

    // With pay_immature (faucet mode) the same credits ARE payable at once.
    let payable = db.get_pending_payouts(1_000_000, true, 0, i64::MAX).await.unwrap();
    let row = payable.iter().find(|p| p.miner_id == m.id).expect("immediately payable");
    assert_eq!(row.amount, 123_750_000);

    // Block confirms → becomes payable in full.
    db.update_block_status(block_id, "confirmed").await.unwrap();
    let payable = db.get_pending_payouts(1_000_000, false, 0, i64::MAX).await.unwrap();
    let row = payable.iter().find(|p| p.miner_id == m.id).expect("now payable");
    assert_eq!(row.amount, 123_750_000);
}

#[tokio::test]
async fn test_payout_coalescing_cooldown_and_override() {
    let db = setup_db().await;
    let recent = db.get_or_create_miner("utest1recent").await.unwrap();
    let quiet = db.get_or_create_miner("utest1quiet").await.unwrap();
    db.credit_balance(recent.id, 50_000_000).await.unwrap();
    db.credit_balance(quiet.id, 50_000_000).await.unwrap();

    // `recent` was just paid; `quiet` has no payout history.
    db.credit_balance(recent.id, 10_000_000).await.unwrap();
    db.create_payout(recent.id, 10_000_000, "aa11").await.unwrap();

    // Cooldown off: both payable.
    let p = db.get_pending_payouts(1_000_000, false, 0, i64::MAX).await.unwrap();
    assert!(p.iter().any(|x| x.miner_id == recent.id));
    assert!(p.iter().any(|x| x.miner_id == quiet.id));

    // 30-min cooldown: the just-paid miner is skipped, the quiet one is not.
    let p = db.get_pending_payouts(1_000_000, false, 1800, i64::MAX).await.unwrap();
    assert!(
        p.iter().all(|x| x.miner_id != recent.id),
        "recently-paid miner must be coalesced: {p:?}"
    );
    assert!(p.iter().any(|x| x.miner_id == quiet.id));

    // Override at/below the pending balance beats the cooldown.
    let p = db.get_pending_payouts(1_000_000, false, 1800, 50_000_000).await.unwrap();
    assert!(
        p.iter().any(|x| x.miner_id == recent.id && x.amount == 50_000_000),
        "override must bypass cooldown: {p:?}"
    );

    // Cooldown shorter than the payout's age no longer blocks (age > 0s is
    // untestable without clock control, so assert the boundary via a payout
    // stamped in the past).
    sqlx::query("UPDATE payouts SET created_at = datetime('now','-3600 seconds') WHERE miner_id = ?1")
        .bind(recent.id)
        .execute(db.inner())
        .await
        .unwrap();
    let p = db.get_pending_payouts(1_000_000, false, 1800, i64::MAX).await.unwrap();
    assert!(
        p.iter().any(|x| x.miner_id == recent.id),
        "hour-old payout must not block a 30-min cooldown: {p:?}"
    );
}

#[tokio::test]
async fn test_clawback_acknowledgement_silences_alerts() {
    let db = setup_db().await;
    let m = db.get_or_create_miner("utest1ack").await.unwrap();
    sqlx::query("INSERT INTO orphan_clawbacks (block_id, miner_id, amount) VALUES (1, ?1, 5000)")
        .bind(m.id)
        .execute(db.inner())
        .await
        .unwrap();
    assert_eq!(db.get_recent_clawbacks(24).await.unwrap().len(), 1);
    let n = db.acknowledge_clawbacks("absorbed from reserve per operator").await.unwrap();
    assert_eq!(n, 1);
    assert!(db.get_recent_clawbacks(24).await.unwrap().is_empty());
}

#[tokio::test]
async fn test_immature_exposure_and_per_block_ack() {
    let db = setup_db().await;
    let m = db.get_or_create_miner("utest1exp").await.unwrap();
    let w = db.get_or_create_worker(m.id, "rig").await.unwrap();

    // Two pending blocks with credits, one confirmed: exposure counts only
    // the pending ones.
    let b1 = db.record_block(500, "feede0", 125_000_000, None, w.id, None).await.unwrap();
    let b2 = db.record_block(501, "feede1", 125_000_000, None, w.id, None).await.unwrap();
    let b3 = db.record_block(502, "feede2", 125_000_000, None, w.id, None).await.unwrap();
    db.distribute_block_credits(b1, &[(m.id, 100)]).await.unwrap();
    db.distribute_block_credits(b2, &[(m.id, 200)]).await.unwrap();
    db.distribute_block_credits(b3, &[(m.id, 400)]).await.unwrap();
    db.update_block_status(b3, "confirmed").await.unwrap();
    assert_eq!(db.get_immature_exposure().await.unwrap(), 300);

    // Per-block acknowledgement only silences that block's clawbacks.
    sqlx::query("INSERT INTO orphan_clawbacks (block_id, miner_id, amount) VALUES (?1, ?2, 100), (?3, ?2, 200)")
        .bind(b1)
        .bind(m.id)
        .bind(b2)
        .execute(db.inner())
        .await
        .unwrap();
    let n = db
        .acknowledge_clawbacks_for_block(b1, "absorbed from reserve (immediate-payout policy)")
        .await
        .unwrap();
    assert_eq!(n, 1);
    let remaining = db.get_recent_clawbacks(24).await.unwrap();
    assert_eq!(remaining.len(), 1, "b2's clawback must still alert");
    assert_eq!(remaining[0].0, b2);
}

#[tokio::test]
async fn test_take_costs_caps_and_stamps_block() {
    let db = setup_db().await;
    let m = db.get_or_create_miner("utest1tc").await.unwrap();
    let w = db.get_or_create_worker(m.id, "rig").await.unwrap();
    let block_a = db
        .record_block(400, "feedc0", 125_000_000, None, w.id, None)
        .await
        .unwrap();
    let block_b = db
        .record_block(401, "feedc1", 125_000_000, None, w.id, None)
        .await
        .unwrap();

    db.record_tx_cost("shield", "txid-s1", 260_000).await.unwrap();
    db.record_tx_cost("payout", "txid-p1", 30_000).await.unwrap();
    db.record_tx_cost("shield", "txid-s2", 2_500_000).await.unwrap();

    // Cap admits the first two rows (290k) but not the third (whole rows only).
    let taken = db.take_costs_for_block(block_a, 2_500_000).await.unwrap();
    assert_eq!(taken, 290_000);
    let stamped: (i64,) =
        sqlx::query_as("SELECT COALESCE(costs_recovered, 0) FROM blocks WHERE id = ?1")
            .bind(block_a)
            .fetch_one(db.inner())
            .await
            .unwrap();
    assert_eq!(stamped.0, 290_000);

    // Next block picks up the remainder; recovered rows are never re-taken.
    let taken_b = db.take_costs_for_block(block_b, 2_500_000).await.unwrap();
    assert_eq!(taken_b, 2_500_000);
    let taken_again = db.take_costs_for_block(block_b, 2_500_000).await.unwrap();
    assert_eq!(taken_again, 0, "queue drained, nothing left to take");
}

#[tokio::test]
async fn test_orphan_requeues_recovered_costs() {
    let db = setup_db().await;
    let m = db.get_or_create_miner("utest1rq").await.unwrap();
    let w = db.get_or_create_worker(m.id, "rig").await.unwrap();
    let block_id = db
        .record_block(402, "feedc2", 125_000_000, Some(125_000_000), w.id, None)
        .await
        .unwrap();
    db.record_tx_cost("shield", "txid-rq", 260_000).await.unwrap();
    let taken = db.take_costs_for_block(block_id, 2_500_000).await.unwrap();
    assert_eq!(taken, 260_000);
    db.distribute_block_credits(block_id, &[(m.id, 123_490_000)])
        .await
        .unwrap();

    // Orphaned: credits reverse AND the costs it absorbed go back in the
    // queue so the next real block recovers them.
    let outcome = db.reverse_block_credits_precise(block_id).await.unwrap();
    assert_eq!(outcome, Some((123_490_000, 0)));

    let stamped: (i64,) =
        sqlx::query_as("SELECT COALESCE(costs_recovered, 0) FROM blocks WHERE id = ?1")
            .bind(block_id)
            .fetch_one(db.inner())
            .await
            .unwrap();
    assert_eq!(stamped.0, 0, "orphaned block must not claim cost recovery");

    let requeued: (String, i64) = sqlx::query_as(
        "SELECT kind, fee FROM pool_tx_costs WHERE recovered = 0 AND kind = 'reorphaned'",
    )
    .fetch_one(db.inner())
    .await
    .unwrap();
    assert_eq!(requeued.1, 260_000);

    // A later block recovers the re-queued amount.
    let block_next = db
        .record_block(403, "feedc3", 125_000_000, None, w.id, None)
        .await
        .unwrap();
    assert_eq!(
        db.take_costs_for_block(block_next, 2_500_000).await.unwrap(),
        260_000
    );
}

#[tokio::test]
async fn test_invariant_subtracts_costs_recovered() {
    let db = setup_db().await;
    let m = db.get_or_create_miner("utest1ic").await.unwrap();
    let w = db.get_or_create_worker(m.id, "rig").await.unwrap();
    let block_id = db
        .record_block(404, "feedc4", 125_000_000, Some(125_300_000), w.id, None)
        .await
        .unwrap();
    db.record_tx_cost("payout", "txid-ic", 40_000).await.unwrap();
    db.take_costs_for_block(block_id, 2_500_000).await.unwrap();
    db.distribute_block_credits(block_id, &[(m.id, 125_260_000)])
        .await
        .unwrap();

    let (reward, balances, clawbacks) = db.get_accounting_invariant().await.unwrap();
    assert_eq!(reward, 125_260_000, "reward basis must be net of recovered costs");
    assert_eq!(balances, 125_260_000);
    assert_eq!(clawbacks, 0);
    // With fee 0 the invariant balances exactly: reward - balances - clawbacks = 0.
    assert_eq!(reward - balances - clawbacks, 0);
}

// ===== Audit #15: block-found durability =====

#[tokio::test]
async fn test_block_submission_breadcrumb_lifecycle() {
    let db = setup_db().await;
    let m = db.get_or_create_miner("utest1bc").await.unwrap();
    let w = db.get_or_create_worker(m.id, "rig").await.unwrap();

    let id = db
        .record_block_submission(700, "beefcrumb", w.id, 125_000_000, Some(125_100_000))
        .await
        .unwrap();
    let open = db.get_open_block_submissions().await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].0, id);
    assert_eq!(open[0].1, 700);
    assert_eq!(open[0].5, Some(125_100_000));

    db.resolve_block_submission(id, "recorded").await.unwrap();
    assert!(db.get_open_block_submissions().await.unwrap().is_empty());
}

#[tokio::test]
async fn test_missing_credits_query_finds_undistributed_block() {
    let db = setup_db().await;
    let m = db.get_or_create_miner("utest1mc").await.unwrap();
    let w = db.get_or_create_worker(m.id, "rig").await.unwrap();
    let block_id = db
        .record_block(701, "feedmc", 125_000_000, Some(125_200_000), w.id, None)
        .await
        .unwrap();

    // Recorded but never distributed -> the sweep must find it.
    let missing = db.get_recent_blocks_missing_credits(7).await.unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].0, block_id);
    assert_eq!(missing[0].2, 125_200_000, "basis = actual_reward");
    assert_eq!(missing[0].3, w.id, "found_by worker for solo mode");

    // Once distributed, it drops out.
    db.distribute_block_credits(block_id, &[(m.id, 125_200_000)])
        .await
        .unwrap();
    assert!(db.get_recent_blocks_missing_credits(7).await.unwrap().is_empty());
}

// ===== Audit #14: atomic orphan_block =====

#[tokio::test]
async fn test_orphan_block_atomic_precise() {
    let db = setup_db().await;
    let m = db.get_or_create_miner("utest1ob").await.unwrap();
    let w = db.get_or_create_worker(m.id, "rig").await.unwrap();
    let block_id = db
        .record_block(600, "feedob", 125_000_000, None, w.id, None)
        .await
        .unwrap();
    db.distribute_block_credits(block_id, &[(m.id, 125_000_000)])
        .await
        .unwrap();

    let outcome = db.orphan_block(block_id, 125_000_000).await.unwrap();
    assert_eq!(outcome, Some((125_000_000, 0)));
    // Status and reversal landed together.
    let blocks = db.get_recent_blocks(5).await.unwrap();
    let b = blocks.iter().find(|b| b.id == block_id).unwrap();
    assert_eq!(b.status, "orphaned");
    assert_eq!(db.get_or_create_balance(m.id).await.unwrap().pending, 0);
}

#[tokio::test]
async fn test_orphan_block_legacy_fallback_pre008() {
    let db = setup_db().await;
    let m = db.get_or_create_miner("utest1obl").await.unwrap();
    let w = db.get_or_create_worker(m.id, "rig").await.unwrap();
    // Pre-008 shape: block exists, credits were applied to pending but NO
    // block_credits rows.
    let block_id = db
        .record_block(601, "feedobl", 100_000_000, None, w.id, None)
        .await
        .unwrap();
    db.credit_balance(m.id, 100_000_000).await.unwrap();

    let outcome = db.orphan_block(block_id, 100_000_000).await.unwrap();
    assert_eq!(outcome, None, "no credit rows -> legacy fallback");
    let blocks = db.get_recent_blocks(5).await.unwrap();
    let b = blocks.iter().find(|b| b.id == block_id).unwrap();
    assert_eq!(b.status, "orphaned");
    assert_eq!(
        db.get_or_create_balance(m.id).await.unwrap().pending,
        0,
        "legacy proportional reversal ran inside the same tx"
    );
}

// ===== Round-3 pre-debit payout saga tests =====

async fn paying_of(db: &PoolDb, miner_id: i64) -> i64 {
    let row: (i64,) = sqlx::query_as("SELECT paying FROM balances WHERE miner_id = ?1")
        .bind(miner_id)
        .fetch_one(db.inner())
        .await
        .unwrap();
    row.0
}

async fn payouts_count(db: &PoolDb) -> i64 {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM payouts")
        .fetch_one(db.inner())
        .await
        .unwrap();
    row.0
}

#[tokio::test]
async fn test_reserve_then_confirm_moves_pending_to_paid() {
    let db = setup_db().await;
    let m = db.get_or_create_miner("utest1sc").await.unwrap();
    db.credit_balance(m.id, 100_000_000).await.unwrap();
    let attempt = db.create_payout_attempt(1, 60_000_000, "test").await.unwrap();

    let reserved = db.reserve_payout(attempt, &[(m.id, 60_000_000)]).await.unwrap();
    assert_eq!(reserved, vec![(m.id, 60_000_000)]);
    // pending debited BEFORE any send; funds now sit in `paying`, not payable.
    assert_eq!(db.get_or_create_balance(m.id).await.unwrap().pending, 40_000_000);
    assert_eq!(paying_of(&db, m.id).await, 60_000_000);
    assert_eq!(payouts_count(&db).await, 0);

    let n = db.confirm_payout(attempt, "txconfirm").await.unwrap();
    assert_eq!(n, 1);
    let b = db.get_or_create_balance(m.id).await.unwrap();
    assert_eq!(b.pending, 40_000_000);
    assert_eq!(b.paid, 60_000_000);
    assert_eq!(paying_of(&db, m.id).await, 0);
    assert_eq!(payouts_count(&db).await, 1);
    // Conserved across the whole saga: pending + paying + paid == original credit.
    assert_eq!(b.pending + paying_of(&db, m.id).await + b.paid, 100_000_000);
}

#[tokio::test]
async fn test_reserve_then_refund_returns_to_pending() {
    let db = setup_db().await;
    let m = db.get_or_create_miner("utest1sr").await.unwrap();
    db.credit_balance(m.id, 100_000_000).await.unwrap();
    let attempt = db.create_payout_attempt(1, 60_000_000, "test").await.unwrap();

    db.reserve_payout(attempt, &[(m.id, 60_000_000)]).await.unwrap();
    assert_eq!(paying_of(&db, m.id).await, 60_000_000);

    let n = db.refund_payout(attempt).await.unwrap();
    assert_eq!(n, 1);
    let b = db.get_or_create_balance(m.id).await.unwrap();
    assert_eq!(b.pending, 100_000_000, "funds returned to pending on refund");
    assert_eq!(b.paid, 0);
    assert_eq!(paying_of(&db, m.id).await, 0);
    assert_eq!(payouts_count(&db).await, 0, "a refunded attempt records no payment");
}

#[tokio::test]
async fn test_reserve_skips_insufficient_and_partial_batch() {
    let db = setup_db().await;
    let rich = db.get_or_create_miner("utest1rich").await.unwrap();
    let poor = db.get_or_create_miner("utest1poor").await.unwrap();
    db.credit_balance(rich.id, 100_000_000).await.unwrap();
    db.credit_balance(poor.id, 10_000_000).await.unwrap();
    let attempt = db.create_payout_attempt(2, 130_000_000, "test").await.unwrap();

    // poor's pending (10M) can't cover 60M -> skipped; rich is reserved.
    let reserved = db
        .reserve_payout(attempt, &[(rich.id, 60_000_000), (poor.id, 60_000_000)])
        .await
        .unwrap();
    assert_eq!(reserved, vec![(rich.id, 60_000_000)]);
    assert_eq!(paying_of(&db, rich.id).await, 60_000_000);
    assert_eq!(paying_of(&db, poor.id).await, 0);
    assert_eq!(
        db.get_or_create_balance(poor.id).await.unwrap().pending,
        10_000_000,
        "skipped miner keeps full pending"
    );
}

#[tokio::test]
async fn test_confirm_is_idempotent() {
    let db = setup_db().await;
    let m = db.get_or_create_miner("utest1idem").await.unwrap();
    db.credit_balance(m.id, 100_000_000).await.unwrap();
    let attempt = db.create_payout_attempt(1, 60_000_000, "test").await.unwrap();
    db.reserve_payout(attempt, &[(m.id, 60_000_000)]).await.unwrap();

    assert_eq!(db.confirm_payout(attempt, "txid1").await.unwrap(), 1);
    // A second confirm (startup reconciliation racing the loop) must be a no-op.
    assert_eq!(db.confirm_payout(attempt, "txid1").await.unwrap(), 0);
    let b = db.get_or_create_balance(m.id).await.unwrap();
    assert_eq!(b.paid, 60_000_000, "no double credit to paid");
    assert_eq!(payouts_count(&db).await, 1, "no duplicate payout row");
}

#[tokio::test]
async fn test_get_reserved_attempts_lifecycle() {
    let db = setup_db().await;
    let m = db.get_or_create_miner("utest1ra").await.unwrap();
    db.credit_balance(m.id, 100_000_000).await.unwrap();
    let attempt = db.create_payout_attempt(1, 60_000_000, "test").await.unwrap();
    db.reserve_payout(attempt, &[(m.id, 60_000_000)]).await.unwrap();

    let reserved = db.get_reserved_attempts(None).await.unwrap();
    assert_eq!(reserved.len(), 1);
    assert_eq!(reserved[0].0, attempt);
    assert_eq!(reserved[0].4, 60_000_000, "total reserved reported");

    // Once confirmed, the attempt no longer holds reserved items.
    db.confirm_payout(attempt, "txid1").await.unwrap();
    assert!(db.get_reserved_attempts(None).await.unwrap().is_empty());
}
