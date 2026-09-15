//! Explicit bounded operator policy for live PPS; never inferred from defaults.
//! The one exception is `settle_confirmations`: it comes from `[payout]
//! pps_settle_confirmations` (default 10), which the payout loop re-reads every cycle.
use serde::Deserialize;

/// PPS payout settlement depth when `[payout] pps_settle_confirmations` is absent.
pub const DEFAULT_SETTLE_CONFIRMATIONS: u64 = 10;
/// Allowed settlement depths: never settle on one or two confirmations (audit B2).
pub const SETTLE_CONFIRMATIONS_RANGE: std::ops::RangeInclusive<u64> = 3..=100;

fn default_settle_confirmations() -> u64 {
    DEFAULT_SETTLE_CONFIRMATIONS
}

/// Confirmations a wallet note needs before a payout may spend it (the send's
/// minconf). Must equal node-rpc's `zecd_funding::PAYOUT_NOTE_MATURITY`; pool-core
/// asserts it at compile time.
pub const PAYOUT_NOTE_MATURITY: u32 = 10;
fn default_funding_maturity_confirmations() -> u32 { PAYOUT_NOTE_MATURITY }

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PpsPolicy {
    pub network: String,
    pub epoch: String,
    pub fee_bps: u16,
    pub max_liability_zatoshis: i64,
    pub total_exposure_zatoshis: i64,
    pub fee_allowance_zatoshis: i64,
    pub reserve_min_zatoshis: i64,
    pub max_payout_zatoshis: i64,
    /// Confirmations a wallet note needs before it backs NEW CREDITS (1..=10).
    /// Payouts always spend notes at `PAYOUT_NOTE_MATURITY`; a lower credit
    /// maturity lets a payout's own change count as soon as it confirms, so
    /// admission does not pause while the wallet holds the money.
    #[serde(default = "default_funding_maturity_confirmations")]
    pub funding_maturity_confirmations: u32,
    /// Confirmations before a sent payout settles (paying -> paid). Never read from
    /// `[pps]`; the dashboard sets it from `[payout] pps_settle_confirmations`.
    #[serde(skip, default = "default_settle_confirmations")]
    pub settle_confirmations: u64,
}

impl PpsPolicy {
    pub fn validate(&self, network: &str) -> Result<(), &'static str> {
        if !matches!(self.network.as_str(), "mainnet" | "testnet") || self.network != network {
            return Err("PPS network must explicitly match pool network");
        }
        if self.epoch.is_empty()
            || self.epoch.len() > 64
            || !self
                .epoch
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("PPS epoch must be 1..64 ASCII letters, digits, hyphen or underscore");
        }
        if self.fee_bps >= 10_000 {
            return Err("PPS fee_bps must be below 10000");
        }
        const MAX_MONEY: i64 = 21_000_000 * 100_000_000;
        if self.max_liability_zatoshis <= 0
            || self.max_liability_zatoshis > MAX_MONEY
            || self.reserve_min_zatoshis <= 0
            || self.reserve_min_zatoshis > MAX_MONEY
            || self.max_payout_zatoshis <= 0
            || self.max_payout_zatoshis > self.max_liability_zatoshis
            || self.total_exposure_zatoshis <= 0
            || self.total_exposure_zatoshis > MAX_MONEY
            || self.fee_allowance_zatoshis <= 0
            || self
                .max_liability_zatoshis
                .checked_add(self.fee_allowance_zatoshis)
                .map_or(true, |v| v > self.total_exposure_zatoshis)
        {
            return Err(
                "PPS requires explicit positive bounded liability, reserve and payout limits",
            );
        }
        if !SETTLE_CONFIRMATIONS_RANGE.contains(&self.settle_confirmations) {
            return Err("PPS settle confirmations must be within 3..=100");
        }
        if !(1..=PAYOUT_NOTE_MATURITY).contains(&self.funding_maturity_confirmations) {
            return Err("PPS funding_maturity_confirmations must be within 1..=10");
        }
        Ok(())
    }

    pub fn epoch_config(&self) -> crate::pps_live::PpsEpoch {
        crate::pps_live::PpsEpoch {
            id: self.epoch.clone(),
            network: self.network.clone(),
            fee_bps: self.fee_bps,
            max_liability_zatoshis: self.max_liability_zatoshis,
            total_exposure_zatoshis: self.total_exposure_zatoshis,
            fee_allowance_zatoshis: self.fee_allowance_zatoshis,
            reserve_floor_zatoshis: self.reserve_min_zatoshis,
            quote_provenance: "pps-v1-fixed-job-target-miner-subsidy".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> PpsPolicy {
        PpsPolicy {
            network: "testnet".into(),
            epoch: "test-1".into(),
            fee_bps: 100,
            max_liability_zatoshis: 100_000_000,
            total_exposure_zatoshis: 110_000_000,
            fee_allowance_zatoshis: 10_000_000,
            reserve_min_zatoshis: 10_000_000,
            max_payout_zatoshis: 20_000_000,
            funding_maturity_confirmations: PAYOUT_NOTE_MATURITY,
            settle_confirmations: DEFAULT_SETTLE_CONFIRMATIONS,
        }
    }

    #[test]
    fn funding_maturity_is_bounded_by_the_payout_maturity() {
        for bad in [0, PAYOUT_NOTE_MATURITY + 1, u32::MAX] {
            let mut p = policy();
            p.funding_maturity_confirmations = bad;
            assert!(p.validate("testnet").is_err());
        }
        let mut p = policy();
        p.funding_maturity_confirmations = 1;
        assert!(p.validate("testnet").is_ok());
    }

    #[test]
    fn bounded_policy_requires_matching_network_and_positive_limits() {
        assert!(policy().validate("testnet").is_ok());
        assert!(policy().validate("mainnet").is_err());
        for bad in [0, -1, i64::MAX] {
            let mut p = policy();
            p.max_liability_zatoshis = bad;
            assert!(p.validate("testnet").is_err());
            let mut p = policy();
            p.reserve_min_zatoshis = bad;
            assert!(p.validate("testnet").is_err());
            let mut p = policy();
            p.max_payout_zatoshis = bad;
            assert!(p.validate("testnet").is_err());
        }
        let mut p = policy();
        p.fee_bps = 10_000;
        assert!(p.validate("testnet").is_err());
        for epoch in ["", "space here", "../escape"] {
            let mut p = policy();
            p.epoch = epoch.into();
            assert!(p.validate("testnet").is_err());
        }
    }

    #[test]
    fn all_financial_policy_fields_are_explicit_and_manual_proof_is_rejected() {
        let valid = serde_json::json!({"network":"testnet", "epoch":"test-1", "fee_bps":100,
            "max_liability_zatoshis":100000000, "reserve_min_zatoshis":10000000,
            "total_exposure_zatoshis":110000000,"fee_allowance_zatoshis":10000000,
            "max_payout_zatoshis":20000000});
        assert!(serde_json::from_value::<PpsPolicy>(valid.clone()).is_ok());
        for key in valid.as_object().unwrap().keys() {
            let mut missing = valid.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(
                serde_json::from_value::<PpsPolicy>(missing).is_err(),
                "missing {key}"
            );
        }
        let mut extra = valid;
        extra
            .as_object_mut()
            .unwrap()
            .insert("unlimited".into(), serde_json::json!(true));
        assert!(serde_json::from_value::<PpsPolicy>(extra).is_err());
        let manual = serde_json::json!({"network":"testnet", "epoch":"test-1", "fee_bps":100,
            "max_liability_zatoshis":100000000, "reserve_min_zatoshis":10000000,
            "max_payout_zatoshis":20000000, "chain_verified_at_unix":1700000000_i64});
        assert!(serde_json::from_value::<PpsPolicy>(manual).is_err());
    }

    #[test]
    fn settle_confirmations_default_to_ten_never_come_from_pps_and_are_bounded() {
        let valid = serde_json::json!({"network":"testnet", "epoch":"test-1", "fee_bps":100,
            "max_liability_zatoshis":100000000, "reserve_min_zatoshis":10000000,
            "total_exposure_zatoshis":110000000,"fee_allowance_zatoshis":10000000,
            "max_payout_zatoshis":20000000});
        let parsed = serde_json::from_value::<PpsPolicy>(valid.clone()).unwrap();
        assert_eq!(parsed.settle_confirmations, DEFAULT_SETTLE_CONFIRMATIONS);
        // The depth is set from [payout], never from [pps].
        let mut in_pps = valid;
        in_pps.as_object_mut().unwrap().insert("settle_confirmations".into(), serde_json::json!(5));
        assert!(serde_json::from_value::<PpsPolicy>(in_pps).is_err());
        for (depth, ok) in [(2, false), (3, true), (5, true), (100, true), (101, false)] {
            let mut p = policy();
            p.settle_confirmations = depth;
            assert_eq!(p.validate("testnet").is_ok(), ok, "depth {depth}");
        }
    }
}
