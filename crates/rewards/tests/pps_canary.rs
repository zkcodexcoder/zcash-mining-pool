#[allow(dead_code)]
#[path = "../examples/pps_canary.rs"]
mod canary;

#[test]
fn synthetic_pricing_is_exact_over_ten_thousand_shares_and_caps_at_one_block() {
    let report = canary::run_canary().unwrap();
    assert_eq!(report.priced_events, 10_000);
    assert_eq!(report.total_subzatoshis, report.per_share_subzatoshis * 10_000);
    assert!(report.one_block_cap_verified);
}
