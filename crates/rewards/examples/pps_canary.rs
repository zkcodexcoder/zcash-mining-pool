//! Synthetic-only PPS pricing rehearsal. No database, RPC, miner, or wallet access.
//! The fixture subsidy, fee and targets are test data, NOT proposed production
//! settings. It covers pricing only; the live ledger (pool-db `pps_live`) has its
//! own tests.

use rewards::pps::{quote_standard_pps, PpsNetwork, PpsQuoteInput, PPS_SCALE};
use std::error::Error;

const EVENTS: u64 = 10_000;

#[derive(Debug)]
pub struct CanaryReport {
    pub priced_events: u64,
    pub per_share_subzatoshis: u128,
    pub total_subzatoshis: u128,
    pub one_block_cap_verified: bool,
}

fn target(value: u64) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[24..].copy_from_slice(&value.to_be_bytes());
    bytes
}

pub fn run_canary() -> Result<CanaryReport, Box<dyn Error>> {
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
    if quote.amount_subzatoshis != expected_per_share || quote.fractional_subzatoshi_discarded {
        return Err("synthetic quote differs from the independent fixture".into());
    }
    let mut total = 0u128;
    for _ in 0..EVENTS {
        total = total
            .checked_add(quote_standard_pps(&input)?.amount_subzatoshis)
            .ok_or("synthetic total overflow")?;
    }
    if total != expected_per_share * u128::from(EVENTS) {
        return Err("synthetic total is not exactly events * price".into());
    }
    // A share target harder than the network target is itself a block: one block, never more.
    let mut harder = input.clone();
    harder.network_target_be = target(3);
    harder.assigned_share_target_be = target(1);
    let one_block_cap_verified =
        quote_standard_pps(&harder)?.amount_subzatoshis == 122_500_000u128 * PPS_SCALE;
    if !one_block_cap_verified {
        return Err("share harder than the network target is not priced at one block".into());
    }
    Ok(CanaryReport {
        priced_events: EVENTS,
        per_share_subzatoshis: expected_per_share,
        total_subzatoshis: total,
        one_block_cap_verified,
    })
}

fn main() -> std::process::ExitCode {
    match run_canary() {
        Ok(report) => {
            println!("{report:?}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("PPS pricing canary failed: {error}");
            std::process::ExitCode::from(1)
        }
    }
}
