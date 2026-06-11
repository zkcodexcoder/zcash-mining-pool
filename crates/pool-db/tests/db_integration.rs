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

    let payable = db.get_pending_payouts(1_000_000, false).await.unwrap();
    assert!(
        payable.iter().all(|p| p.miner_id != m.id),
        "immature credits must not be payable: {payable:?}"
    );

    // With pay_immature (faucet mode) the same credits ARE payable at once.
    let payable = db.get_pending_payouts(1_000_000, true).await.unwrap();
    let row = payable.iter().find(|p| p.miner_id == m.id).expect("immediately payable");
    assert_eq!(row.amount, 123_750_000);

    // Block confirms → becomes payable in full.
    db.update_block_status(block_id, "confirmed").await.unwrap();
    let payable = db.get_pending_payouts(1_000_000, false).await.unwrap();
    let row = payable.iter().find(|p| p.miner_id == m.id).expect("now payable");
    assert_eq!(row.amount, 123_750_000);
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
