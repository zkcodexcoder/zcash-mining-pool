//! Pure integer standard-PPS pricing, not FPPS and not a payout implementation.
//!
//! The pool's target predicate accepts `hash <= target` (target is big-endian;
//! sha256d bytes are reversed by the validator). For a uniformly distributed
//! valid solution hash, conditional block probability given an accepted share
//! is `(network_target + 1) / (assigned_share_target + 1)`. Price the ASSIGNED
//! target committed to the immutable job when issued, including an explicitly
//! recorded grace assignment, not its achieved hash or a floating difficulty
//! reconstructed later. Do not pick a current/grace target because this
//! particular hash happens to meet it; that produces biased PPS credits.
//!
//! One zatoshi is exactly 10^12 sub-zatoshis. We floor the exact rational once
//! per quote. Underpayment is strictly less than one sub-zatoshi per accepted
//! share, hence less than n/10^12 zatoshis over n shares, with no overpayment.
//! Persist/aggregate the integer sub-zatoshi entitlement; round only at payout.
//! The shadow ledger must atomically bind each unique accepted share to its
//! immutable quote and preserve its remaining sub-zatoshi balance across runs.
//!
//! This math layer DOES NOT attest a subsidy or the consensus schedule. Its
//! caller MUST prove the miner-only subsidy (excluding fees/funding outputs) at
//! the immutable job's exact height and network through authoritative consensus
//! or validated RPC evidence. Bind that evidence and exact targets into the
//! immutable quote/epoch provenance. A plausible self-claimed subsidy is not
//! proof. No production credit path may bypass that separate attestation gate.

use num_bigint::BigUint;
use num_traits::{One, ToPrimitive, Zero};

pub const PPS_SCALE: u128 = 1_000_000_000_000;
const BPS: u16 = 10_000;
// Conservative upper envelope, NOT a consensus-subsidy validation rule.
const MAX_MINER_SUBSIDY_ZATS: u64 = 1_250_000_000;
// Zcash mainnet PoW limit 0007ffff... (2^243 - 1), not testnet's 07ffff....
const MAINNET_POW_LIMIT_BE: [u8; 32] = [
    0x00, 0x07, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];
const TESTNET_POW_LIMIT_BE: [u8; 32] = [
    0x07, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpsNetwork {
    Mainnet,
    Testnet,
}

impl std::str::FromStr for PpsNetwork {
    type Err = PpsError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "mainnet" => Ok(Self::Mainnet),
            "testnet" => Ok(Self::Testnet),
            _ => Err(PpsError::UnsupportedNetwork),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpsQuoteInput {
    pub network: PpsNetwork,
    /// Height of the immutable job/template, not a later chain-tip height.
    pub height: u64,
    pub network_target_be: [u8; 32],
    /// Exact immutable PER-JOB assigned target, including its recorded grace
    /// assignment. Never infer/select it from this share's achieved hash.
    pub assigned_share_target_be: [u8; 32],
    /// Miner portion only, excluding transaction fees and other subsidy outputs.
    /// Caller MUST attest this at the exact network/height, never coinbase total.
    /// This module checks bounds only, not external consensus provenance.
    pub miner_subsidy_zats: u64,
    pub fee_bps: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpsQuote {
    pub amount_subzatoshis: u128,
    /// False means the rational is represented exactly at PPS_SCALE.
    pub fractional_subzatoshi_discarded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PpsError {
    #[error("PPS network is unsupported")]
    UnsupportedNetwork,
    #[error("PPS quote requires a nonzero u32 job height")]
    InvalidHeight,
    #[error("PPS requires a positive, bounded miner-only subsidy")]
    InvalidSubsidy,
    #[error("PPS fee must be less than 10000 basis points")]
    InvalidFee,
    #[error("PPS target must be exactly 64 hexadecimal characters")]
    InvalidTargetEncoding,
    #[error("PPS targets must be nonzero")]
    ZeroTarget,
    #[error("PPS network target exceeds the network PoW limit")]
    InvalidNetworkTarget,
    #[error("PPS assigned share target must not be harder than the network target")]
    ShareTargetTooHard,
    #[error("PPS quote would round to zero; reject this pricing configuration")]
    ZeroQuote,
    #[error("PPS monetary arithmetic overflow")]
    ArithmeticOverflow,
}

/// Strict full-width big-endian target parser. No prefixes, odd widths,
/// whitespace, truncation or padding. Uppercase hex is accepted canonically.
pub fn parse_target_be(value: &str) -> Result<[u8; 32], PpsError> {
    if value.len() != 64 || !value.is_ascii() {
        return Err(PpsError::InvalidTargetEncoding);
    }
    let mut result = [0u8; 32];
    for (i, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let digit = |c: u8| -> Result<u8, PpsError> {
            match c {
                b'0'..=b'9' => Ok(c - b'0'),
                b'a'..=b'f' => Ok(c - b'a' + 10),
                b'A'..=b'F' => Ok(c - b'A' + 10),
                _ => Err(PpsError::InvalidTargetEncoding),
            }
        };
        result[i] = (digit(pair[0])? << 4) | digit(pair[1])?;
    }
    Ok(result)
}

pub fn quote_standard_pps(input: &PpsQuoteInput) -> Result<PpsQuote, PpsError> {
    if input.height == 0 || input.height > u32::MAX as u64 {
        return Err(PpsError::InvalidHeight);
    }
    if input.miner_subsidy_zats == 0 || input.miner_subsidy_zats > MAX_MINER_SUBSIDY_ZATS {
        return Err(PpsError::InvalidSubsidy);
    }
    if input.fee_bps >= BPS {
        return Err(PpsError::InvalidFee);
    }
    if input.network_target_be == [0; 32] || input.assigned_share_target_be == [0; 32] {
        return Err(PpsError::ZeroTarget);
    }
    let pow_limit = match input.network {
        PpsNetwork::Mainnet => MAINNET_POW_LIMIT_BE,
        PpsNetwork::Testnet => TESTNET_POW_LIMIT_BE,
    };
    if input.network_target_be > pow_limit {
        return Err(PpsError::InvalidNetworkTarget);
    }
    if input.assigned_share_target_be < input.network_target_be {
        return Err(PpsError::ShareTargetTooHard);
    }

    // Bounded exact arithmetic: 32-byte targets, <=1,250,000,000-zatoshi subsidy,
    // <=10,000 fee multiplier and fixed 10^12 scale. Intermediates need <342
    // bits. BigUint cannot machine-overflow; conversion to ledger u128 is
    // explicitly checked. The denominator is positive by target validation.
    let network = BigUint::from_bytes_be(&input.network_target_be) + BigUint::one();
    let assigned = BigUint::from_bytes_be(&input.assigned_share_target_be) + BigUint::one();
    let numerator = network
        * BigUint::from(input.miner_subsidy_zats)
        * BigUint::from(BPS - input.fee_bps)
        * BigUint::from(PPS_SCALE);
    let denominator = assigned * BigUint::from(BPS);
    let units = (&numerator / &denominator)
        .to_u128()
        .ok_or(PpsError::ArithmeticOverflow)?;
    if units == 0 {
        return Err(PpsError::ZeroQuote);
    }
    Ok(PpsQuote {
        amount_subzatoshis: units,
        fractional_subzatoshi_discarded: !(&numerator % &denominator).is_zero(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn target(value: u64) -> [u8; 32] {
        let mut result = [0; 32];
        result[24..].copy_from_slice(&value.to_be_bytes());
        result
    }
    fn input(n: u64, s: u64) -> PpsQuoteInput {
        PpsQuoteInput {
            network: PpsNetwork::Mainnet,
            height: 3_000_000,
            network_target_be: target(n),
            assigned_share_target_be: target(s),
            miner_subsidy_zats: 125_000_000,
            fee_bps: 0,
        }
    }
    #[test]
    fn inclusive_probability_is_not_plain_target_ratio() {
        let q = quote_standard_pps(&input(1, 3)).unwrap();
        assert_eq!(q.amount_subzatoshis, 62_500_000 * PPS_SCALE);
        assert!(!q.fractional_subzatoshi_discarded);
    }
    #[test]
    fn block_difficulty_share_earns_the_explicit_miner_subsidy() {
        assert_eq!(
            quote_standard_pps(&input(42, 42))
                .unwrap()
                .amount_subzatoshis,
            125_000_000 * PPS_SCALE
        );
        // This module does not manufacture a subsidy from network or height;
        // the independent provenance gate must reject a wrong-but-plausible
        // coinbase value. Doubling job height does not double its price.
        let mut later = input(42, 42);
        later.height *= 2;
        assert_eq!(
            quote_standard_pps(&later).unwrap(),
            quote_standard_pps(&input(42, 42)).unwrap()
        );
    }
    #[test]
    fn fee_is_applied_before_one_exact_floor() {
        let mut value = input(1, 5);
        value.fee_bps = 123;
        let q = quote_standard_pps(&value).unwrap();
        let numerator = 125_000_000_u128 * 9877 * 2 * PPS_SCALE;
        assert_eq!(q.amount_subzatoshis, numerator / 60000);
        assert_eq!(q.fractional_subzatoshi_discarded, numerator % 60000 != 0);
        value.fee_bps = 9999;
        assert!(quote_standard_pps(&value).is_ok());
        for fee in [10000, u16::MAX] {
            value.fee_bps = fee;
            assert_eq!(quote_standard_pps(&value), Err(PpsError::InvalidFee));
        }
    }
    #[test]
    fn zero_harder_and_out_of_network_targets_fail_closed() {
        for value in [input(0, 1), input(1, 0)] {
            assert_eq!(quote_standard_pps(&value), Err(PpsError::ZeroTarget));
        }
        assert_eq!(
            quote_standard_pps(&input(3, 1)),
            Err(PpsError::ShareTargetTooHard)
        );
        let mut value = input(1, 1);
        value.network_target_be = [255; 32];
        value.assigned_share_target_be = [255; 32];
        assert_eq!(
            quote_standard_pps(&value),
            Err(PpsError::InvalidNetworkTarget)
        );
    }
    #[test]
    fn full_width_assigned_target_handles_257_bit_plus_one_without_wrapping() {
        let mut value = input(1, 1);
        value.network_target_be = MAINNET_POW_LIMIT_BE;
        value.assigned_share_target_be = [255; 32];
        let q = quote_standard_pps(&value).unwrap();
        assert_eq!(q.amount_subzatoshis, 125_000_000 * PPS_SCALE / 8192);
        assert!(!q.fractional_subzatoshi_discarded);
    }
    #[test]
    fn exact_pow_limit_and_endian_boundary() {
        let mut value = input(1, 1);
        value.network_target_be = MAINNET_POW_LIMIT_BE;
        value.assigned_share_target_be = MAINNET_POW_LIMIT_BE;
        assert!(quote_standard_pps(&value).is_ok());
        value.network_target_be = [0; 32];
        value.network_target_be[1] = 8;
        value.assigned_share_target_be = [255; 32];
        assert_eq!(
            quote_standard_pps(&value),
            Err(PpsError::InvalidNetworkTarget)
        );
        value.network_target_be = target(1);
        value.network_target_be.reverse();
        assert_eq!(
            quote_standard_pps(&value),
            Err(PpsError::InvalidNetworkTarget)
        );
    }
    #[test]
    fn parser_is_full_width_big_endian_and_rejects_ambiguous_forms() {
        let parsed = parse_target_be(&format!("{:064x}", 256)).unwrap();
        assert_eq!(parsed, target(256));
        assert_eq!(parse_target_be(&"FF".repeat(32)).unwrap(), [255; 32]);
        for bad in [
            "1".to_string(),
            "0x".to_string() + &"00".repeat(32),
            "gg".repeat(32),
            "00".repeat(33),
            "00".repeat(31),
            "é".repeat(32),
        ] {
            assert_eq!(parse_target_be(&bad), Err(PpsError::InvalidTargetEncoding));
        }
    }
    #[test]
    fn halving_attested_miner_portion_halves_standard_pps_quote() {
        for network in [PpsNetwork::Mainnet, PpsNetwork::Testnet] {
            let mut value = input(1, 3);
            value.network = network;
            let prior = quote_standard_pps(&value).unwrap();
            value.height += 1;
            value.miner_subsidy_zats = 62_500_000;
            let next = quote_standard_pps(&value).unwrap();
            assert_eq!(next.amount_subzatoshis * 2, prior.amount_subzatoshis);
            assert_eq!(next.amount_subzatoshis, 31_250_000 * PPS_SCALE);
        }
    }
    #[test]
    fn mainnet_testnet_and_invalid_network_height_boundaries() {
        let mut value = input(1, 1);
        value.network = PpsNetwork::Testnet;
        assert!(quote_standard_pps(&value).is_ok());
        assert_eq!(
            "regtest".parse::<PpsNetwork>(),
            Err(PpsError::UnsupportedNetwork)
        );
        for h in [0, u64::MAX] {
            value.height = h;
            assert_eq!(quote_standard_pps(&value), Err(PpsError::InvalidHeight));
        }
        value.height = u32::MAX as u64;
        assert!(quote_standard_pps(&value).is_ok());
        value.network_target_be = TESTNET_POW_LIMIT_BE;
        value.assigned_share_target_be = [255; 32];
        assert!(quote_standard_pps(&value).is_ok());
        value.network = PpsNetwork::Mainnet;
        assert_eq!(
            quote_standard_pps(&value),
            Err(PpsError::InvalidNetworkTarget)
        );
    }
    #[test]
    fn zero_subsidy_max_subsidy_and_dust_quote_rejected() {
        let mut value = input(1, 1);
        for subsidy in [0, u64::MAX] {
            value.miner_subsidy_zats = subsidy;
            assert_eq!(quote_standard_pps(&value), Err(PpsError::InvalidSubsidy));
        }
        value.miner_subsidy_zats = 125_000_000;
        value.assigned_share_target_be = [255; 32];
        assert_eq!(quote_standard_pps(&value), Err(PpsError::ZeroQuote));
        value.assigned_share_target_be = target(1);
        value.miner_subsidy_zats = MAX_MINER_SUBSIDY_ZATS;
        assert_eq!(
            quote_standard_pps(&value).unwrap().amount_subzatoshis,
            1_250_000_000 * PPS_SCALE
        );
    }
    #[test]
    fn bounded_small_targets_prove_floor_and_error_bound() {
        for n in 1..32 {
            for s in n..64 {
                let value = input(n, s);
                let q = quote_standard_pps(&value).unwrap();
                let num = 125_000_000_u128 * (n as u128 + 1) * PPS_SCALE;
                let den = s as u128 + 1;
                assert!(q.amount_subzatoshis * den <= num);
                assert!((q.amount_subzatoshis + 1) * den > num);
            }
        }
    }
}
