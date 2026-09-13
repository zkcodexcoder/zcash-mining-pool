//! Chain-agreement check against fixed references. WARNING ONLY: a missing,
//! stale or disagreeing proof never blocks a send, seal or settlement. It is
//! logged here, raised as a reconciler alert, and shown on the /pps health page.
use node_rpc::ZcashRpcClient;
use pool_db::pps_live::PpsChainLease;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Minimum spacing between repeated "chain agreement unproven" log lines; the
/// payout loop and reconciler ask several times per round.
const WARN_INTERVAL_SECONDS: i64 = 300;

pub(crate) struct PpsGate {
    rpc: Arc<ZcashRpcClient>,
    network: String,
    cached: Mutex<Option<PpsChainLease>>,
    last_warned_unix: AtomicI64,
    #[cfg(test)]
    supplied: Option<PpsChainLease>,
}

impl PpsGate {
    pub fn new(rpc: Arc<ZcashRpcClient>, network: &str) -> Self {
        Self {
            rpc,
            network: network.into(),
            cached: Mutex::new(None),
            last_warned_unix: AtomicI64::new(i64::MIN),
            #[cfg(test)]
            supplied: None,
        }
    }

    #[cfg(test)]
    pub fn synthetic(rpc: Arc<ZcashRpcClient>, network: &str, lease: PpsChainLease) -> Self {
        Self {
            rpc,
            network: network.into(),
            cached: Mutex::new(Some(lease.clone())),
            last_warned_unix: AtomicI64::new(i64::MIN),
            supplied: Some(lease),
        }
    }

    /// The current chain-agreement proof, or `Ok(None)` when agreement is
    /// unproven (references down, lagging or disagreeing). Never returns `Err`:
    /// callers go ahead either way; the `Result` only keeps the payout phases'
    /// `?` plumbing unchanged.
    pub async fn fresh_lease(&self) -> anyhow::Result<Option<PpsChainLease>> {
        #[cfg(test)]
        if let Some(lease) = &self.supplied {
            let now = chrono::Utc::now().timestamp();
            let verified = validate(lease, &self.network, now).map(|()| lease.clone());
            return Ok(self.warn_unless_proven(verified));
        }
        let mut cached = self.cached.lock().await;
        // Re-read the clock AFTER waiting for another verifier.
        let now = chrono::Utc::now().timestamp();
        if let Some(lease) = cached.as_ref() {
            if now.saturating_sub(lease.checked_at_unix) < 30
                && validate(lease, &self.network, now).is_ok()
            {
                return Ok(Some(lease.clone()));
            }
        }
        *cached = None;
        let verified = async {
            let network = self
                .network
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid PPS network"))?;
            let lease = pool_core::pps_chain::verify_pps_chain(&self.rpc, network).await?;
            validate(&lease, &self.network, chrono::Utc::now().timestamp())?;
            Ok::<_, anyhow::Error>(lease)
        }
        .await;
        if let Ok(lease) = &verified {
            *cached = Some(lease.clone());
        }
        Ok(self.warn_unless_proven(verified))
    }

    fn warn_unless_proven(&self, verified: anyhow::Result<PpsChainLease>) -> Option<PpsChainLease> {
        let reason = match verified {
            Ok(lease) => return Some(lease),
            Err(reason) => reason,
        };
        let now = chrono::Utc::now().timestamp();
        if now.saturating_sub(self.last_warned_unix.load(Ordering::Relaxed)) >= WARN_INTERVAL_SECONDS {
            self.last_warned_unix.store(now, Ordering::Relaxed);
            tracing::warn!(%reason,
                "PPS chain agreement unproven (warning only; credits and payouts continue)");
        }
        None
    }
}

fn validate(lease: &PpsChainLease, network: &str, now: i64) -> anyhow::Result<()> {
    anyhow::ensure!(
        lease.network == network
            && lease.agreeing_references >= 2
            && !lease.disagreement
            && lease.checked_at_unix <= now
            && now < lease.valid_until_unix
            && lease.valid_until_unix.saturating_sub(lease.checked_at_unix)
                <= pool_db::pps_funding::CHAIN_LEASE_SECONDS,
        "independently verified PPS chain agreement unavailable"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uncached_gate() -> PpsGate {
        // No listener: a refresh cannot produce a proof.
        PpsGate::new(Arc::new(ZcashRpcClient::new("http://127.0.0.1:0")),"testnet")
    }

    fn proof(checked_at: i64) -> PpsChainLease {
        PpsChainLease { network:"testnet".into(),checked_at_unix:checked_at,
            valid_until_unix:checked_at+90,agreeing_references:2,disagreement:false }
    }

    #[tokio::test]
    async fn unproven_chain_agreement_is_a_warning_not_an_error() {
        let gate=uncached_gate();
        assert!(gate.fresh_lease().await.unwrap().is_none());
        // A disagreeing cached proof is dropped, still without an error.
        let mut bad=proof(chrono::Utc::now().timestamp()-5);
        bad.disagreement=true;
        *gate.cached.lock().await=Some(bad);
        assert!(gate.fresh_lease().await.unwrap().is_none());
        assert!(gate.cached.lock().await.is_none());
    }

    #[tokio::test]
    async fn a_recent_valid_proof_is_served_from_cache_without_refresh() {
        let gate=uncached_gate();
        let lease=proof(chrono::Utc::now().timestamp()-5);
        *gate.cached.lock().await=Some(lease.clone());
        let served=gate.fresh_lease().await.unwrap().expect("cached proof");
        assert_eq!(served.checked_at_unix,lease.checked_at_unix);
        assert_eq!(served.valid_until_unix,lease.valid_until_unix);
    }

    #[test]
    fn validate_detects_expired_future_wrong_network_or_disagreeing_evidence() {
        let lease = PpsChainLease {
            network: "testnet".into(),
            checked_at_unix: 100,
            valid_until_unix: 190,
            agreeing_references: 2,
            disagreement: false,
        };
        assert!(validate(&lease, "testnet", 100).is_ok());
        assert!(validate(&lease, "testnet", 189).is_ok());
        assert!(validate(&lease, "testnet", 190).is_err());
        assert!(validate(&lease, "testnet", 99).is_err());
        assert!(validate(&lease, "mainnet", 100).is_err());
        let mut bad = lease.clone();
        bad.disagreement = true;
        assert!(validate(&bad, "testnet", 100).is_err());
        let mut bad = lease;
        bad.agreeing_references = 1;
        assert!(validate(&bad, "testnet", 100).is_err());
    }
}
