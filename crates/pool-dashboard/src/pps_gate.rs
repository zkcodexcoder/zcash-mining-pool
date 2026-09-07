//! Private fixed-reference chain gate, with no configured attestation bypass.
use node_rpc::ZcashRpcClient;
use pool_db::pps_live::PpsChainLease;
use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};

/// Keeps the verified cache entry protected from a concurrent refresh or
/// invalidation until the funded seal completes. This is not fresh evidence.
pub(crate) struct PpsSealLease<'a> {
    _cached: MutexGuard<'a, Option<PpsChainLease>>,
    valid_until_unix: i64,
}

impl PpsSealLease<'_> {
    pub fn valid_until_unix(&self) -> i64 { self.valid_until_unix }
}

pub(crate) struct PpsGate {
    rpc: Arc<ZcashRpcClient>,
    network: String,
    cached: Mutex<Option<PpsChainLease>>,
    #[cfg(test)]
    supplied: Option<PpsChainLease>,
}

impl PpsGate {
    pub fn new(rpc: Arc<ZcashRpcClient>, network: &str) -> Self {
        Self {
            rpc,
            network: network.into(),
            cached: Mutex::new(None),
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
            supplied: Some(lease),
        }
    }

    /// Final cache-only acquisition: never starts a reference RPC, renews a
    /// timestamp or falls back to an older proof. Keep the guard through the
    /// funded database seal, not through a wallet send.
    pub async fn valid_cached_lease(&self) -> anyhow::Result<PpsSealLease<'_>> {
        self.valid_cached_lease_with_clock(|| chrono::Utc::now().timestamp()).await
    }

    async fn valid_cached_lease_with_clock(&self, clock: impl FnOnce() -> i64)
        -> anyhow::Result<PpsSealLease<'_>>
    {
        let cached = self.cached.lock().await;
        // The clock must be sampled after waiting on the same mutex held by
        // fresh_lease, whose failed refresh leaves this cache empty.
        let now = clock();
        let lease = cached.as_ref().ok_or_else(||
            anyhow::anyhow!("current cached PPS chain agreement required"))?;
        validate(lease, &self.network, now)?;
        let valid_until_unix = lease.valid_until_unix;
        Ok(PpsSealLease { _cached: cached, valid_until_unix })
    }

    pub async fn fresh_lease(&self) -> anyhow::Result<PpsChainLease> {
        #[cfg(test)]
        if let Some(lease) = &self.supplied {
            validate(lease, &self.network, chrono::Utc::now().timestamp())?;
            return Ok(lease.clone());
        }
        let mut cached = self.cached.lock().await;
        // Re-read the clock AFTER waiting for another verifier. A timestamp
        // taken before that wait could incorrectly accept an expired lease.
        let now = chrono::Utc::now().timestamp();
        if let Some(lease) = cached.as_ref() {
            if now.saturating_sub(lease.checked_at_unix) < 30
                && validate(lease, &self.network, now).is_ok()
            {
                return Ok(lease.clone());
            }
        }
        // An expired cache or failed verification must never authorize sends.
        *cached = None;
        let network = self
            .network
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid PPS network"))?;
        let lease = pool_core::pps_chain::verify_pps_chain(&self.rpc, network).await?;
        validate(&lease, &self.network, chrono::Utc::now().timestamp())?;
        *cached = Some(lease.clone());
        Ok(lease)
    }
}

fn validate(lease: &PpsChainLease, network: &str, now: i64) -> anyhow::Result<()> {
    anyhow::ensure!(
        lease.network == network
            && lease.agreeing_references >= 2
            && !lease.disagreement
            && lease.checked_at_unix <= now
            && now < lease.valid_until_unix
            && lease.valid_until_unix.saturating_sub(lease.checked_at_unix) <= 90,
        "current independently verified PPS chain agreement required"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uncached_gate() -> PpsGate {
        // No listener and no synthetic fresh_lease bypass: these tests exercise
        // the production cache. A refresh cannot supply a successful proof.
        PpsGate::new(Arc::new(ZcashRpcClient::new("http://127.0.0.1:0")),"testnet")
    }

    fn proof(checked_at: i64) -> PpsChainLease {
        PpsChainLease { network:"testnet".into(),checked_at_unix:checked_at,
            valid_until_unix:checked_at+90,agreeing_references:2,disagreement:false }
    }

    #[tokio::test]
    async fn final_cache_acquisition_keeps_old_valid_proof_without_refresh_or_renewal() {
        let gate=uncached_gate();
        let lease=proof(chrono::Utc::now().timestamp()-35);
        *gate.cached.lock().await=Some(lease.clone());
        let held=gate.valid_cached_lease().await.unwrap();
        assert_eq!(held.valid_until_unix(),lease.valid_until_unix);
        assert!(gate.cached.try_lock().is_err());
        drop(held);
        let cached=gate.cached.lock().await;
        let saved=cached.as_ref().unwrap();
        assert_eq!(saved.checked_at_unix,lease.checked_at_unix);
        assert_eq!(saved.valid_until_unix,lease.valid_until_unix);
        assert_eq!(saved.agreeing_references,lease.agreeing_references);
        assert_eq!(saved.network,lease.network);
        assert_eq!(saved.disagreement,lease.disagreement);
    }

    #[tokio::test]
    async fn final_cache_acquisition_enforces_exact_original_expiry_and_all_proof_checks() {
        let gate=uncached_gate();
        assert!(gate.valid_cached_lease_with_clock(||100).await.is_err());
        *gate.cached.lock().await=Some(proof(100));
        assert!(gate.valid_cached_lease_with_clock(||189).await.is_ok());
        assert!(gate.valid_cached_lease_with_clock(||190).await.is_err());
        assert!(gate.valid_cached_lease_with_clock(||99).await.is_err());
        for field in ["network","disagreement","references","lifetime"] {
            let mut bad=proof(100);
            match field {
                "network" => bad.network="mainnet".into(),
                "disagreement" => bad.disagreement=true,
                "references" => bad.agreeing_references=1,
                _ => bad.valid_until_unix=191,
            }
            *gate.cached.lock().await=Some(bad);
            assert!(gate.valid_cached_lease_with_clock(||100).await.is_err());
        }
    }

    #[tokio::test]
    async fn final_cache_wait_samples_clock_after_lock_and_observes_revocation() {
        use std::{cell::Cell, future::{poll_fn,Future}, task::Poll};
        for revoke in [false,true] {
            let gate=uncached_gate();
            let mut verifier=gate.cached.lock().await;
            *verifier=Some(proof(100));
            let clock=Cell::new(189);
            let clock_read=Cell::new(false);
            let waiting=gate.valid_cached_lease_with_clock(|| {
                clock_read.set(true);
                clock.get()
            });
            tokio::pin!(waiting);
            assert!(poll_fn(|cx|Poll::Ready(waiting.as_mut().poll(cx).is_pending())).await);
            assert!(!clock_read.get());
            if revoke {
                // Same state transition as fresh_lease clearing a cache before
                // a failed verifier releases this exact mutex.
                *verifier=None;
            } else {
                clock.set(190);
            }
            drop(verifier);
            assert!(waiting.await.is_err());
            assert!(clock_read.get());
        }
    }

    #[tokio::test]
    async fn final_guard_blocks_the_real_refresh_mutex_until_seal_scope_ends() {
        use std::{future::{poll_fn,Future}, task::Poll};
        let gate=uncached_gate();
        *gate.cached.lock().await=Some(proof(chrono::Utc::now().timestamp()-35));
        let held=gate.valid_cached_lease().await.unwrap();
        let mut refresh=Box::pin(gate.fresh_lease());
        assert!(poll_fn(|cx|Poll::Ready(refresh.as_mut().poll(cx).is_pending())).await);
        assert!(gate.cached.try_lock().is_err());
        // Do not poll a refresh after releasing the guard: this test performs
        // no reference RPC and needs only to prove the shared-mutex boundary.
        // Cancel its FIFO waiter first; otherwise try_lock correctly yields
        // the unlocked mutex to that queued (but deliberately unpolled) future.
        drop(refresh);
        drop(held);
        assert!(gate.cached.try_lock().is_ok());
    }

    #[test]
    fn gate_rejects_expired_future_wrong_network_or_disagreeing_evidence() {
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
