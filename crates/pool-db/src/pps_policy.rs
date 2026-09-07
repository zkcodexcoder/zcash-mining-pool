//! Explicit bounded operator policy for live PPS; never inferred from defaults.
use serde::Deserialize;

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
        }
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
}
