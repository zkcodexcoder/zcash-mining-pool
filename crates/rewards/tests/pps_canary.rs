#[allow(dead_code)]
#[path = "../examples/pps_canary.rs"]
mod canary;

#[tokio::test]
async fn synthetic_pricing_and_shadow_ledger_survive_replay_and_restart() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "pps-integration-{}-{nonce}.pps-shadow.sqlite",
        std::process::id(),
    ));
    let report = canary::run_canary(&path).await.unwrap();
    assert_eq!(report.accepted_events, 10_000);
    assert_eq!(report.duplicate_replays, 1_000);
    assert_eq!(report.rejected_cases, 10);
    assert!(report.persistence_verified && report.non_spendable);
    assert!(canary::run_canary(&path).await.is_err());
    // Only the exact synthetic test-created database is removed.
    std::fs::remove_file(&path).unwrap();
}
