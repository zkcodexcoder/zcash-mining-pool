//! Short-lived wallet funding proof for PPS. Read-only RPCs, exact integers,
//! fixed failure categories, and a DB generation bracket prevent a cached
//! pre-payout balance from authorizing another credit or send.
use node_rpc::ZcashRpcClient;
use pool_db::{PoolDb, pps_live::PpsEpoch, pps_funding::{PpsFundingLease, PpsFundingSnapshot}};
use std::time::Duration;
use serde::Deserialize;

pub use pool_db::pps_funding::FUNDING_LEASE_SECONDS;
// The wallet evidence is stamped in node-rpc and validated here and in
// pool-db; the three crates must agree on the lifetime or every collection
// is rejected as InvalidEvidence.
const _: () = assert!(FUNDING_LEASE_SECONDS == node_rpc::zecd_funding::EVIDENCE_LIFETIME_SECONDS);

/// An explicit protocol selection, not a wallet capability attestation. Missing
/// configuration retains the existing PCZT route; a failed route never probes
/// or falls back to another wallet protocol. Deployment must independently pin
/// the conventional wallet binary and its standard-fee contract.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PpsFundingRoute {
    #[default]
    ZalletPczt,
    ZecdConventionalTestnet { hold_new_legacy_sends: bool },
}

impl<'de> Deserialize<'de> for PpsFundingRoute {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self,D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Protocol { ZalletPczt, ZecdConventionalTestnet }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw { route: Protocol, hold_new_legacy_sends: Option<bool> }
        let raw=Raw::deserialize(deserializer)?;
        match (raw.route,raw.hold_new_legacy_sends) {
            (Protocol::ZalletPczt,None) => Ok(Self::ZalletPczt),
            (Protocol::ZecdConventionalTestnet,Some(hold_new_legacy_sends)) =>
                Ok(Self::ZecdConventionalTestnet { hold_new_legacy_sends }),
            _ => Err(serde::de::Error::custom("PPS funding route fields do not match its explicit protocol")),
        }
    }
}

impl PpsFundingRoute {
    pub fn validate(&self, epoch: &PpsEpoch) -> Result<(), PpsFundingError> {
        match self {
            Self::ZalletPczt => Ok(()),
            Self::ZecdConventionalTestnet { hold_new_legacy_sends } => {
                // Exact, limited testnet authorization. Neither config drift
                // nor a future mainnet policy may silently expand this route.
                if epoch.network != "testnet" || epoch.fee_bps >= 10_000
                    || epoch.max_liability_zatoshis != 95_000_000_000
                    || epoch.fee_allowance_zatoshis != 5_000_000_000
                    || epoch.total_exposure_zatoshis != 100_000_000_000
                    || epoch.reserve_floor_zatoshis <= 0 || !hold_new_legacy_sends
                { return Err(PpsFundingError::RoutePolicy); }
                Ok(())
            }
        }
    }

    pub fn holds_new_legacy_sends(&self) -> bool {
        matches!(self, Self::ZecdConventionalTestnet { hold_new_legacy_sends: true })
    }

    /// Recipient admission follows the explicitly selected payout protocol.
    /// Shielded recipients require the testnet conventional wallet-history
    /// contract; they must never silently broaden the existing PCZT route.
    pub fn validate_recipient(&self, network: &str, address: &str)
        -> Result<(), PpsFundingError>
    {
        match self {
            Self::ZalletPczt => node_rpc::pczt::validate_pps_recipient(network, address)
                .map_err(|_| PpsFundingError::UnsupportedRecipient),
            Self::ZecdConventionalTestnet { hold_new_legacy_sends: true }
                if network == "testnet" =>
            {
                node_rpc::zecd_conventional::validate_testnet_recipient(address)
                    .map_err(|_| PpsFundingError::UnsupportedRecipient)
            }
            _ => Err(PpsFundingError::RoutePolicy),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PpsFundingError {
    #[error("PPS funding route or bounded policy invalid")]
    RoutePolicy,
    #[error("PPS payout recipient unsupported for selected route")]
    UnsupportedRecipient,
    #[error("PPS funding verification timed out")]
    Timeout,
    #[error("PPS funding wallet unavailable or invalid")]
    WalletUnavailable,
    #[error("PPS funding wallet network mismatch")]
    NetworkMismatch,
    #[error("PPS bounded wallet fee contract unavailable")]
    FeeContractUnavailable,
    #[error("PPS funding accounting unavailable")]
    AccountingUnavailable,
    #[error("PPS funding changed during verification")]
    ConcurrentChange,
    #[error("PPS confirmed funding insufficient")]
    InsufficientFunding,
    #[error("PPS funding evidence stale or invalid")]
    InvalidEvidence,
    #[error("PPS testnet identity signer readiness not proven")]
    IdentitySignerNotProven,
    #[error("PPS fee capacity exhausted for further credits")]
    FeeCapacityExhausted,
}

/// This is validation, not a capability factory. A lease is issued only by a
/// fresh wallet read bracketed by matching DB snapshots and a verified fixed
/// PCZT RPC contract. Config cannot supply a manual wallet attestation.
pub fn validate_funding_lease(lease: &PpsFundingLease, epoch: &PpsEpoch, now: i64)
    -> Result<(), PpsFundingError>
{
    if lease.network != epoch.network || lease.checked_at_unix < 0
        || lease.checked_at_unix > now || now >= lease.valid_until_unix
        || lease.valid_until_unix.saturating_sub(lease.checked_at_unix) > FUNDING_LEASE_SECONDS
        || lease.valid_until_unix <= lease.checked_at_unix
        || lease.spendable_zatoshis < 0
        || lease.reserve_floor_zatoshis != epoch.reserve_floor_zatoshis
        || lease.reserved_fee_allowance_zatoshis != epoch.fee_allowance_zatoshis
    { return Err(PpsFundingError::InvalidEvidence); }
    Ok(())
}

fn finish_evidence(before: &PpsFundingSnapshot, after: &PpsFundingSnapshot,
    spendable: i64, epoch: &PpsEpoch, checked_at: i64, now: i64)
    -> Result<PpsFundingLease, PpsFundingError>
{
    // The requirement counts outstanding liability, so every credit between the
    // two reads moves it; bracketing on it would fail every refresh while miners
    // are active. The generation still brackets wallet-side changes (payouts),
    // and each use of the lease re-checks the proven balance against the
    // requirement at that moment (pool-db validate_lease). Cover the larger one.
    if before.generation != after.generation || before.network != after.network
        || before.network != epoch.network
    { return Err(PpsFundingError::ConcurrentChange); }
    let required = before.required_spendable_zatoshis.max(after.required_spendable_zatoshis);
    if after.required_spendable_zatoshis <= 0 || spendable < required {
        return Err(PpsFundingError::InsufficientFunding);
    }
    let lease = PpsFundingLease {
        network: epoch.network.clone(), checked_at_unix: checked_at,
        valid_until_unix: checked_at.checked_add(FUNDING_LEASE_SECONDS)
            .ok_or(PpsFundingError::InvalidEvidence)?,
        spendable_zatoshis: spendable,
        reserve_floor_zatoshis: epoch.reserve_floor_zatoshis,
        reserved_fee_allowance_zatoshis: epoch.fee_allowance_zatoshis,
        generation: after.generation,
    };
    validate_funding_lease(&lease, epoch, now)?;
    Ok(lease)
}

/// Explicit route dispatcher used by startup, share-lease refresh and payout
/// preflight. Testnet evidence is independently bounded and does not gain a new
/// timestamp when converted into a ledger lease. Unknown identity-mode signing
/// readiness remains a distinct failure; a config boolean cannot substitute for
/// positive key readiness. No RPC mutation is performed by either collector.
pub async fn collect_pps_funding_for_route(
    db: &PoolDb, wallet: &ZcashRpcClient, epoch: &PpsEpoch, from: &str,
    node: &ZcashRpcClient, route: &PpsFundingRoute,
) -> Result<PpsFundingLease, PpsFundingError> {
    crate::pps_credit_health::stage("route_policy");
    route.validate(epoch)?;
    match route {
        PpsFundingRoute::ZalletPczt => collect_pps_funding(db, wallet, epoch, from, node).await,
        PpsFundingRoute::ZecdConventionalTestnet { .. } =>
            collect_testnet_for_snapshot(db, wallet, epoch, from, node, None).await,
    }
}

/// Explicit owner preflight only; neither startup nor regular refresh falls
/// back to this projection when its current policy disagrees with the DB.
/// Performs exactly the same bounded read-only wallet proof as runtime.
pub async fn collect_pps_testnet_budget_extension_funding(
    db: &PoolDb, wallet: &ZcashRpcClient, previous: &PpsEpoch, from: &str,
    node: &ZcashRpcClient,
) -> Result<PpsFundingLease, PpsFundingError> {
    let next = pool_db::pps_funding::testnet_budget_extension_epoch(previous)
        .map_err(|_| PpsFundingError::RoutePolicy)?;
    PpsFundingRoute::ZecdConventionalTestnet { hold_new_legacy_sends:true }.validate(&next)?;
    collect_testnet_for_snapshot(db, wallet, &next, from, node, Some(previous)).await
}

async fn testnet_snapshot(db: &PoolDb, epoch: &PpsEpoch, previous: Option<&PpsEpoch>)
    -> Result<PpsFundingSnapshot, PpsFundingError>
{
    match previous {
        Some(previous) => db.pps_testnet_budget_extension_snapshot(previous).await,
        None => db.pps_funding_snapshot_for_epoch(epoch).await,
    }.map_err(|_| PpsFundingError::AccountingUnavailable)
}

async fn collect_testnet_for_snapshot(
    db: &PoolDb, wallet: &ZcashRpcClient, epoch: &PpsEpoch, from: &str,
    node: &ZcashRpcClient, previous: Option<&PpsEpoch>,
) -> Result<PpsFundingLease, PpsFundingError> {
            let node_stage=node_rpc::zecd_funding::FundingStageObserver::default();
            let result=tokio::time::timeout(Duration::from_secs(node_rpc::zecd_funding::COLLECTION_TIMEOUT_SECONDS), async {
                let checked_at = chrono::Utc::now().timestamp();
                crate::pps_credit_health::stage("funding_before");
                let before = testnet_snapshot(db, epoch, previous).await?;
                crate::pps_credit_health::stage("wallet_collect");
                let proof = node_rpc::zecd_funding::collect_testnet_funding_observed_with_stage(wallet, from, node,&node_stage).await
                    .map_err(|observed| {
                        crate::pps_credit_health::wallet_failure(observed.stage,observed.category);
                        match observed.error {
                        node_rpc::zecd_funding::ZecdFundingError::IdentitySignerNotProven =>
                            PpsFundingError::IdentitySignerNotProven,
                        _ => PpsFundingError::WalletUnavailable,
                    }})?;
                crate::pps_credit_health::stage("funding_after");
                let after = testnet_snapshot(db, epoch, previous).await?;
                crate::pps_credit_health::stage("funding_finish");
                finish_testnet_evidence(&before, &after, &proof, epoch, checked_at,
                    chrono::Utc::now().timestamp())
            }).await;
            match result {
                Ok(result)=>result,
                Err(_)=>{
                    // Only an interrupted wallet collection uses the node
                    // observer; later accounting timeouts retain their own stage.
                    if crate::pps_credit_health::current_stage()==Some("wallet_collect") {
                        crate::pps_credit_health::wallet_failure(node_stage.stage(),"deadline_exceeded");
                    }
                    Err(PpsFundingError::Timeout)
                }
            }
}

/// Share admission needs room for the next maximum supported payout batch.
/// Payout preflight must use collect_pps_funding_for_route instead: its own
/// reservation already occupies fee capacity and must not deadlock its send.
pub async fn collect_pps_credit_funding_for_route(
    db: &PoolDb, wallet: &ZcashRpcClient, epoch: &PpsEpoch, from: &str,
    node: &ZcashRpcClient, route: &PpsFundingRoute,
) -> Result<PpsFundingLease, PpsFundingError> {
    let lease=collect_pps_funding_for_route(db,wallet,epoch,from,node,route).await?;
    if matches!(route,PpsFundingRoute::ZecdConventionalTestnet { .. }) {
        crate::pps_credit_health::stage("credit_capacity");
        let snapshot=db.pps_funding_snapshot_for_epoch(epoch).await
            .map_err(|_|PpsFundingError::AccountingUnavailable)?;
        if snapshot.generation != lease.generation || snapshot.network != lease.network {
            return Err(PpsFundingError::ConcurrentChange);
        }
        validate_credit_fee_capacity(route,&snapshot)?;
        validate_funding_lease(&lease,epoch,chrono::Utc::now().timestamp())?;
    }
    Ok(lease)
}

/// Read-only capacity predicate, also used by sampled status. This never issues
/// a funding lease or authorizes a credit; the DB still checks admission itself.
pub fn validate_credit_fee_capacity(route:&PpsFundingRoute,snapshot:&PpsFundingSnapshot)
    -> Result<(),PpsFundingError>
{
    if matches!(route,PpsFundingRoute::ZalletPczt) { return Ok(()); }
    if snapshot.paid_fees_zatoshis < 0 || snapshot.reserved_fees_zatoshis < 0 {
        return Err(PpsFundingError::InvalidEvidence);
    }
    let available=snapshot.fee_allowance_zatoshis.checked_sub(snapshot.paid_fees_zatoshis)
        .and_then(|v|v.checked_sub(snapshot.reserved_fees_zatoshis))
        .ok_or(PpsFundingError::InvalidEvidence)?;
    let ceiling=node_rpc::zecd_conventional::TestnetConventionalProfile::consensus_size_bound("testnet",100)
        .and_then(|p|p.fee_ceiling_zatoshis(100)).map_err(|_|PpsFundingError::InvalidEvidence)?;
    if available < ceiling { return Err(PpsFundingError::FeeCapacityExhausted); }
    Ok(())
}

fn finish_testnet_evidence(before: &PpsFundingSnapshot, after: &PpsFundingSnapshot,
    proof: &node_rpc::zecd_funding::ZecdFundingEvidence, epoch: &PpsEpoch,
    checked_at: i64, now: i64) -> Result<PpsFundingLease, PpsFundingError>
{
    if proof.checked_at_unix < checked_at || proof.checked_at_unix > now
        || proof.valid_until_unix <= now
        || proof.valid_until_unix.checked_sub(proof.checked_at_unix) != Some(FUNDING_LEASE_SECONDS)
    { return Err(PpsFundingError::InvalidEvidence); }
    // The outer generation bracket starts no later than the wallet evidence.
    // Using its earlier clock shortens the lease and never refreshes stale data.
    let lease = finish_evidence(before, after, proof.confirmed_eligible_zatoshis,
        epoch, checked_at, now)?;
    if lease.valid_until_unix > proof.valid_until_unix {
        return Err(PpsFundingError::InvalidEvidence);
    }
    Ok(lease)
}

/// Collects before initial epoch activation too. The ledger must recheck the
/// generation and required amount atomically at activation/admission/reserve,
/// and immediately before signing/broadcast. No wallet mutation occurs here.
pub async fn collect_pps_funding(db: &PoolDb, wallet: &ZcashRpcClient, epoch: &PpsEpoch, from: &str, node: &ZcashRpcClient)
    -> Result<PpsFundingLease, PpsFundingError>
{
    tokio::time::timeout(Duration::from_secs(25), async {
        let checked_at = chrono::Utc::now().timestamp();
        crate::pps_credit_health::stage("funding_before");
        let before = db.pps_funding_snapshot_for_epoch(epoch).await
            .map_err(|_| PpsFundingError::AccountingUnavailable)?;
        // This opaque result can only be created by strict read-only discovery
        // of the fixed PCZT contract. Unsupported wallets never fall back to
        // an unbounded z_sendmany call or a configuration-supplied assertion.
        crate::pps_credit_health::stage("pczt_contract");
        let _contract = node_rpc::pczt::verify_pps_wallet_capability(wallet).await
            .map_err(|_| PpsFundingError::FeeContractUnavailable)?;
        // Pinned Zallet has no getblockchaininfo/getinfo wallet methods.
        // The verifier uses fixed synthetic network-specific addresses with
        // validateaddress (positive own-network AND negative other-network),
        // never a wallet-owned address, key or a config-supplied attestation.
        crate::pps_credit_health::stage("pczt_network");
        node_rpc::pczt::verify_pps_wallet_network(wallet, &epoch.network).await
            .map_err(|_| PpsFundingError::NetworkMismatch)?;
        crate::pps_credit_health::stage("pczt_signer");
        wallet.pps_wallet_signer_ready(checked_at.checked_add(FUNDING_LEASE_SECONDS)
            .ok_or(PpsFundingError::InvalidEvidence)?).await
            .map_err(|_| PpsFundingError::WalletUnavailable)?;
        crate::pps_credit_health::stage("wallet_collect");
        let spendable = wallet.confirmed_spendable_zatoshis_on_chain(from, node).await
            .map_err(|_| PpsFundingError::WalletUnavailable)?;
        crate::pps_credit_health::stage("funding_after");
        let after = db.pps_funding_snapshot_for_epoch(epoch).await
            .map_err(|_| PpsFundingError::AccountingUnavailable)?;
        crate::pps_credit_health::stage("funding_finish");
        finish_evidence(&before, &after, spendable, epoch, checked_at, chrono::Utc::now().timestamp())
    }).await.map_err(|_| PpsFundingError::Timeout)?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn epoch() -> PpsEpoch {
        PpsEpoch { id: "funding-test".into(), network: "testnet".into(), fee_bps: 0,
            max_liability_zatoshis: 990_000_000, total_exposure_zatoshis: 1_000_000_000,
            fee_allowance_zatoshis: 10_000_000, reserve_floor_zatoshis: 1,
            quote_provenance: "fixed-test".into() }
    }
    fn conventional_epoch() -> PpsEpoch {
        let mut e = epoch();
        e.max_liability_zatoshis = 95_000_000_000;
        e.fee_allowance_zatoshis = 5_000_000_000;
        e.total_exposure_zatoshis = 100_000_000_000;
        e
    }
    #[test]
    fn explicit_testnet_route_requires_exact_all_in_bounds_and_legacy_hold() {
        let route = PpsFundingRoute::ZecdConventionalTestnet { hold_new_legacy_sends: true };
        let e = conventional_epoch();
        assert!(route.validate(&e).is_ok());
        let mut predecessor=e.clone();
        predecessor.max_liability_zatoshis=950_000_000;
        predecessor.fee_allowance_zatoshis=50_000_000;
        predecessor.total_exposure_zatoshis=1_000_000_000;
        assert_eq!(route.validate(&predecessor),Err(PpsFundingError::RoutePolicy));
        assert!(PpsFundingRoute::ZalletPczt.validate(&predecessor).is_ok());
        for fee in [0, 100, 9_999] {
            let mut same_policy = e.clone(); same_policy.fee_bps = fee;
            assert!(route.validate(&same_policy).is_ok());
        }
        for altered in 0..6 {
            let mut bad = e.clone();
            match altered {
                0 => bad.network = "mainnet".into(),
                1 => bad.max_liability_zatoshis += 1,
                2 => bad.fee_allowance_zatoshis += 1,
                3 => bad.total_exposure_zatoshis += 1,
                4 => bad.reserve_floor_zatoshis = 0,
                _ => bad.fee_bps = 10_000,
            }
            assert_eq!(route.validate(&bad), Err(PpsFundingError::RoutePolicy));
        }
        assert!(PpsFundingRoute::ZecdConventionalTestnet { hold_new_legacy_sends: false }.validate(&e).is_err());
        assert!(route.holds_new_legacy_sends());
        assert!(!PpsFundingRoute::ZalletPczt.holds_new_legacy_sends());
    }
    #[test]
    fn route_configuration_is_explicit_and_rejects_manual_signer_proofs() {
        let route: PpsFundingRoute = toml::from_str("route='zecd_conventional_testnet'\nhold_new_legacy_sends=true").unwrap();
        assert_eq!(route, PpsFundingRoute::ZecdConventionalTestnet { hold_new_legacy_sends: true });
        for config in ["", "route='zecd_conventional_testnet'", "route='unknown'",
            "route='zallet_pczt'\nhold_new_legacy_sends=true",
            "route='zecd_conventional_testnet'\nhold_new_legacy_sends=true\nsigner_ready=true"]
        { assert!(toml::from_str::<PpsFundingRoute>(config).is_err(), "{config}"); }
        assert_eq!(PpsFundingRoute::default(), PpsFundingRoute::ZalletPczt);
    }
    #[test]
    fn existing_pczt_route_does_not_change_its_network_or_economic_policy() {
        let mut e = epoch();
        for network in ["mainnet", "testnet"] {
            e.network = network.into();
            assert!(PpsFundingRoute::ZalletPczt.validate(&e).is_ok());
        }
    }
    #[test]
    fn recipient_admission_is_specific_to_the_explicit_testnet_route() {
        let conventional = PpsFundingRoute::ZecdConventionalTestnet { hold_new_legacy_sends:true };
        let transparent = "tm9ty64b2UE2PWqVH1NN7hBmZr27U771NKY";
        let sapling = "ztestsapling1ywvgdtat0cemx5y6ejpu5wapc5x2j0c08f9lee3fd9s6wvv6n079w5nplr48qne73w6swec4vzv";
        let unified = "utest1rak2faln6pat6jx7rmfulvm80c0mjcnj5z2zvsynqn0zhu9gtk97ll5cyvu7maglwgazje4t00958n2yyadc8ee2vskkmg0e7wscqxaaahke023r8pejc097tf0e5zu6ltq9g6f99xxtpfprujl4uhaph3yj7mu52w3da6x0lgj3j0qy";
        for recipient in [transparent,sapling,unified] {
            assert!(conventional.validate_recipient("testnet",recipient).is_ok());
            assert_eq!(conventional.validate_recipient("mainnet",recipient),Err(PpsFundingError::RoutePolicy));
            assert_eq!(PpsFundingRoute::ZecdConventionalTestnet { hold_new_legacy_sends:false }
                .validate_recipient("testnet",recipient),Err(PpsFundingError::RoutePolicy));
        }
        assert!(PpsFundingRoute::ZalletPczt.validate_recipient("testnet",transparent).is_ok());
        assert!(PpsFundingRoute::ZalletPczt.validate_recipient("testnet",sapling).is_err());
        assert!(PpsFundingRoute::ZalletPczt.validate_recipient("testnet",unified).is_err());
        for bad in ["", "unsupported", "MAINNET", " unified "] {
            assert!(conventional.validate_recipient("testnet",bad).is_err());
            assert!(PpsFundingRoute::ZalletPczt.validate_recipient("testnet",bad).is_err());
        }
    }
    #[test]
    fn lease_rejects_expiry_future_and_mutated_economic_bounds() {
        let e = epoch();
        let lease = PpsFundingLease { network: "testnet".into(), checked_at_unix:100,
            valid_until_unix:160, spendable_zatoshis:1_000_000_001,
            reserve_floor_zatoshis:1, reserved_fee_allowance_zatoshis:10_000_000, generation:0 };
        assert!(validate_funding_lease(&lease,&e,159).is_ok());
        for now in [99,160,170] { assert!(validate_funding_lease(&lease,&e,now).is_err()); }
        let mut bad=lease.clone(); bad.reserve_floor_zatoshis=0;
        assert!(validate_funding_lease(&bad,&e,100).is_err());
        let mut bad=lease.clone(); bad.reserved_fee_allowance_zatoshis=0;
        assert!(validate_funding_lease(&bad,&e,100).is_err());
        let mut bad=lease; bad.network="mainnet".into();
        assert!(validate_funding_lease(&bad,&e,100).is_err());
    }

    #[test]
    fn balance_read_brackets_generation_and_covers_the_larger_requirement() {
        let e = epoch();
        let s = PpsFundingSnapshot {
            network: "testnet".into(), generation:7, legacy_pending_zatoshis:2,
            legacy_paying_zatoshis:3, pps_outstanding_subzatoshis:0,
            unused_credit_subzatoshis:990_000_000*pool_db::pps_live::PPS_SCALE,
            gross_subzatoshis:0, paid_zatoshis:0,
            cap_subzatoshis:990_000_000*pool_db::pps_live::PPS_SCALE,
            total_exposure_zatoshis:1_000_000_000, reserve_floor_zatoshis:1,
            fee_allowance_zatoshis:10_000_000,paid_fees_zatoshis:0,reserved_fees_zatoshis:0,
            required_spendable_zatoshis:1_000_000_006,
        };
        assert!(finish_evidence(&s,&s,1_000_000_006,&e,100,100).is_ok());
        assert_eq!(finish_evidence(&s,&s,1_000_000_005,&e,100,100),Err(PpsFundingError::InsufficientFunding));
        assert_eq!(finish_evidence(&s,&s,1_000_000_006,&e,100,100+FUNDING_LEASE_SECONDS),Err(PpsFundingError::InvalidEvidence));
        let mut changed=s.clone(); changed.generation+=1;
        assert_eq!(finish_evidence(&s,&changed,i64::MAX,&e,100,100),Err(PpsFundingError::ConcurrentChange));
        // A credit between the reads moves the requirement, not the generation:
        // the refresh succeeds only if the proof covers the larger requirement.
        let mut changed=s.clone(); changed.required_spendable_zatoshis+=1;
        assert!(finish_evidence(&s,&changed,1_000_000_007,&e,100,100).is_ok());
        assert_eq!(finish_evidence(&s,&changed,1_000_000_006,&e,100,100),Err(PpsFundingError::InsufficientFunding));
        assert_eq!(finish_evidence(&changed,&s,1_000_000_006,&e,100,100),Err(PpsFundingError::InsufficientFunding));
    }

    #[test]
    fn conventional_credit_capacity_reserves_next_max_batch_but_not_pczt_or_payout_funding() {
        let route=PpsFundingRoute::ZecdConventionalTestnet { hold_new_legacy_sends:true };
        let s=PpsFundingSnapshot {
            network:"testnet".into(),generation:0,legacy_pending_zatoshis:0,legacy_paying_zatoshis:0,
            pps_outstanding_subzatoshis:0,unused_credit_subzatoshis:0,gross_subzatoshis:0,
            paid_zatoshis:0,cap_subzatoshis:0,total_exposure_zatoshis:1_000_000_000,
            reserve_floor_zatoshis:1,fee_allowance_zatoshis:50_000_000,paid_fees_zatoshis:0,
            reserved_fees_zatoshis:0,required_spendable_zatoshis:1,
        };
        let ceiling=28_905_000;
        assert!(validate_credit_fee_capacity(&route,&s).is_ok());
        let mut at_limit=s.clone(); at_limit.paid_fees_zatoshis=50_000_000-ceiling;
        assert!(validate_credit_fee_capacity(&route,&at_limit).is_ok());
        at_limit.paid_fees_zatoshis+=1;
        assert_eq!(validate_credit_fee_capacity(&route,&at_limit),Err(PpsFundingError::FeeCapacityExhausted));
        let mut reserved=s.clone(); reserved.reserved_fees_zatoshis=ceiling;
        assert_eq!(validate_credit_fee_capacity(&route,&reserved),Err(PpsFundingError::FeeCapacityExhausted));
        assert!(validate_credit_fee_capacity(&PpsFundingRoute::ZalletPczt,&reserved).is_ok());
        // The ordinary/payout funding finish remains available after reserving
        // a batch; only the explicitly named credit wrapper applies this gate.
        let e=conventional_epoch();
        assert!(finish_evidence(&reserved,&reserved,1,&e,100,100).is_ok());
        reserved.reserved_fees_zatoshis=-1;
        assert_eq!(validate_credit_fee_capacity(&route,&reserved),Err(PpsFundingError::InvalidEvidence));
    }
}
