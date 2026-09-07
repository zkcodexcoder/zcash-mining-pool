//! Synthetic-only PPS accounting rehearsal. No RPC, miner, or wallet access.
//! The fixture subsidy, fee, identities, timestamps, and reserve are test data,
//! NOT proposed production settings or evidence of a live testnet canary.

use pool_db::pps_shadow::{
    ChainVerificationLease, PpsShadowLedger, ShadowEpoch, ShadowError, ShadowShare,
};
use rewards::pps::{quote_standard_pps, PpsNetwork, PpsQuoteInput, PPS_SCALE};
use std::{error::Error, path::Path};

const EVENTS: u64 = 10_000;
const FIXTURE_TIME: i64 = 1_700_000_000;

#[derive(Debug)]
pub struct CanaryReport {
    pub accepted_events: u64,
    pub duplicate_replays: u64,
    pub rejected_cases: u64,
    pub persistence_verified: bool,
    pub non_spendable: bool,
}

fn target(value: u64) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[24..].copy_from_slice(&value.to_be_bytes());
    bytes
}

fn fixture_share(index: u64, amount: u128) -> ShadowShare {
    ShadowShare {
        event_id: format!("synthetic-event-{index}"),
        miner_id: if index % 2 == 0 {
            "synthetic-a"
        } else {
            "synthetic-b"
        }
        .into(),
        // Fixture identifier, NOT a cryptographic proof of a real quote.
        quote_id: format!("{:064x}", 1),
        amount_subzatoshis: amount,
        accepted_at_unix: FIXTURE_TIME,
    }
}

pub async fn run_canary(path: &Path) -> Result<CanaryReport, Box<dyn Error>> {
    // Do not append fixtures to an existing canary (or any other file).
    if path.try_exists()? {
        return Err("synthetic rehearsal requires a new .pps-shadow.sqlite path".into());
    }
    let input = PpsQuoteInput {
        network: PpsNetwork::Testnet,
        height: 3_000_000,
        network_target_be: target(1),
        assigned_share_target_be: target(8191),
        miner_subsidy_zats: 125_000_000, // Synthetic, not a testnet subsidy claim.
        fee_bps: 200,                    // Synthetic, not an operator fee decision.
    };
    let quote = quote_standard_pps(&input)?;
    // Independent integer fixture: 125,000,000 * 98% / 4096 zatoshis/share.
    let expected_per_share = 122_500_000u128 * PPS_SCALE / 4096;
    assert_eq!(quote.amount_subzatoshis, expected_per_share);
    assert!(!quote.fractional_subzatoshi_discarded);
    let expected_total = expected_per_share.checked_mul(EVENTS.into()).unwrap();
    let epoch = ShadowEpoch {
        id: "synthetic-testnet-v1".into(),
        network: "testnet".into(),
        fee_bps: input.fee_bps,
        reserve_cap_subzatoshis: expected_total,
        quote_provenance: "synthetic-fixture-not-chain-proof".into(),
    };
    let lease = ChainVerificationLease {
        network: "testnet".into(),
        checked_at_unix: FIXTURE_TIME,
        valid_until_unix: FIXTURE_TIME + 60,
        agreeing_references: 2,
        disagreement: false,
    };
    let ledger = PpsShadowLedger::open(path, epoch.clone()).await?;
    let next = fixture_share(EVENTS, expected_per_share);
    let mut rejected_cases = 0;

    // Prove missing, disagreeing, stale, future, and cross-network evidence do
    // not create liabilities. These are injected fixtures, not live probes.
    assert!(matches!(
        ledger.credit_share(&next, None, FIXTURE_TIME).await,
        Err(ShadowError::ChainLeaseRequired)
    ));
    rejected_cases += 1;
    for (bad_lease, now) in [
        (
            ChainVerificationLease {
                disagreement: true,
                ..lease.clone()
            },
            FIXTURE_TIME,
        ),
        (
            ChainVerificationLease {
                agreeing_references: 1,
                ..lease.clone()
            },
            FIXTURE_TIME,
        ),
        (
            ChainVerificationLease {
                network: "mainnet".into(),
                ..lease.clone()
            },
            FIXTURE_TIME,
        ),
        (lease.clone(), FIXTURE_TIME + 61),
        (lease.clone(), FIXTURE_TIME - 1),
    ] {
        assert!(matches!(
            ledger.credit_share(&next, Some(&bad_lease), now).await,
            Err(ShadowError::ChainLeaseRequired)
        ));
        rejected_cases += 1;
    }
    assert_eq!(ledger.summary().await?.accepted_events, 0);

    for index in 0..EVENTS {
        let receipt = ledger
            .credit_share(
                &fixture_share(index, expected_per_share),
                Some(&lease),
                FIXTURE_TIME,
            )
            .await?;
        assert!(!receipt.duplicate && !receipt.spendable);
        assert_eq!(receipt.credited_subzatoshis, expected_per_share);
    }
    let before = ledger.summary().await?;
    assert_eq!(before.accepted_events, EVENTS);
    assert_eq!(before.total_liability_subzatoshis, expected_total);
    assert!(!before.spendable);

    let mut duplicate_replays = 0;
    for index in (0..EVENTS).step_by(10) {
        // The old event remains idempotent even after its lease expires.
        let receipt = ledger
            .credit_share(
                &fixture_share(index, expected_per_share),
                None,
                FIXTURE_TIME + 1000,
            )
            .await?;
        assert!(receipt.duplicate && !receipt.spendable);
        duplicate_replays += 1;
    }
    let mut mismatch = fixture_share(0, expected_per_share);
    mismatch.amount_subzatoshis += 1;
    assert!(matches!(
        ledger
            .credit_share(&mismatch, Some(&lease), FIXTURE_TIME)
            .await,
        Err(ShadowError::DuplicateMismatch)
    ));
    rejected_cases += 1;
    assert!(matches!(
        ledger.credit_share(&next, Some(&lease), FIXTURE_TIME).await,
        Err(ShadowError::ReserveExceeded)
    ));
    rejected_cases += 1;
    assert_eq!(before, ledger.summary().await?);

    let a = ledger.miner_balance("synthetic-a").await?;
    let b = ledger.miner_balance("synthetic-b").await?;
    assert_eq!(a.total_subzatoshis + b.total_subzatoshis, expected_total);
    assert!(a.fractional_subzatoshis > 0 && b.fractional_subzatoshis > 0);
    assert!(!a.spendable && !b.spendable);
    ledger.close().await;

    let reopened = PpsShadowLedger::open(path, epoch.clone()).await?;
    assert_eq!(before, reopened.summary().await?);
    assert_eq!(a, reopened.miner_balance("synthetic-a").await?);
    assert_eq!(b, reopened.miner_balance("synthetic-b").await?);
    reopened.close().await;

    // A fresh fee epoch must NOT reset the global budget or accumulated carry.
    let new_epoch = ShadowEpoch {
        id: "synthetic-testnet-v2".into(),
        fee_bps: 300,
        ..epoch.clone()
    };
    let second_epoch = PpsShadowLedger::open(path, new_epoch).await?;
    assert_eq!(before, second_epoch.summary().await?);
    assert!(matches!(
        second_epoch
            .credit_share(&next, Some(&lease), FIXTURE_TIME)
            .await,
        Err(ShadowError::ReserveExceeded)
    ));
    rejected_cases += 1;
    second_epoch.close().await;
    let raised_budget = ShadowEpoch {
        reserve_cap_subzatoshis: expected_total + 1,
        ..epoch
    };
    assert!(matches!(
        PpsShadowLedger::open(path, raised_budget).await,
        Err(ShadowError::BudgetMismatch)
    ));
    rejected_cases += 1;

    Ok(CanaryReport {
        accepted_events: EVENTS,
        duplicate_replays,
        rejected_cases,
        persistence_verified: true,
        non_spendable: true,
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 1 {
        return Err(
            "usage: pps_canary <NEW .pps-shadow.sqlite path>; synthetic-only; no live PPS".into(),
        );
    }
    let report = run_canary(Path::new(&args[0])).await?;
    println!("synthetic_testnet_only=true");
    println!("accepted_events={}", report.accepted_events);
    println!("duplicate_replays={}", report.duplicate_replays);
    println!("rejected_failure_cases={}", report.rejected_cases);
    println!("persistence_verified={}", report.persistence_verified);
    println!("non_spendable={}", report.non_spendable);
    println!("live_testnet_deployed=false\nmainnet_deployed=false");
    Ok(())
}
