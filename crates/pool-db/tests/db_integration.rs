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
        .record_block(100, "aabbccdd", 1_000_000_000, worker.id, Some(85.5))
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
