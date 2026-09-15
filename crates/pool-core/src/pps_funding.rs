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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpsFundingRoute {
    /// zecd's conventional `z_sendmany` send, verified byte-for-byte against the
    /// recorded intent. The only route since 2026-09-15 (operator decision #1):
    /// the PCZT/zallet path never ran and was retired. Configured as
    /// `route = "zecd_conventional"` (the testnet trial's name
    /// `zecd_conventional_testnet` still parses).
    ZecdConventional { hold_new_legacy_sends: bool },
}

impl<'de> Deserialize<'de> for PpsFundingRoute {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self,D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Protocol {
            #[serde(alias = "zecd_conventional_testnet")]
            ZecdConventional,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw { route: Protocol, hold_new_legacy_sends: Option<bool> }
        let raw=Raw::deserialize(deserializer)?;
        match (raw.route,raw.hold_new_legacy_sends) {
            (Protocol::ZecdConventional,Some(hold_new_legacy_sends)) =>
                Ok(Self::ZecdConventional { hold_new_legacy_sends }),
            _ => Err(serde::de::Error::custom("PPS funding route fields do not match its explicit protocol")),
        }
    }
}

impl PpsFundingRoute {
    pub fn validate(&self, epoch: &PpsEpoch) -> Result<(), PpsFundingError> {
        match self {
            Self::ZecdConventional { hold_new_legacy_sends } => {
                // Structural bounds only; the policy's own validation holds the
                // amounts. A known network, a fee below 100%, positive liability
                // and fee allowance within the exposure, a non-negative floor,
                // and legacy sends held while PPS runs.
                if !matches!(epoch.network.as_str(), "mainnet" | "testnet") || epoch.fee_bps >= 10_000
                    || epoch.max_liability_zatoshis <= 0 || epoch.fee_allowance_zatoshis <= 0
                    || epoch.max_liability_zatoshis.checked_add(epoch.fee_allowance_zatoshis)
                        .is_none_or(|v| v > epoch.total_exposure_zatoshis)
                    || epoch.reserve_floor_zatoshis < 0 || !hold_new_legacy_sends
                { return Err(PpsFundingError::RoutePolicy); }
                Ok(())
            }
        }
    }

    pub fn holds_new_legacy_sends(&self) -> bool {
        matches!(self, Self::ZecdConventional { hold_new_legacy_sends: true })
    }

    /// Recipient admission for the explicitly selected route and the pool's network.
    pub fn validate_recipient(&self, network: &str, address: &str)
        -> Result<(), PpsFundingError>
    {
        match self {
            Self::ZecdConventional { hold_new_legacy_sends: true } =>
                node_rpc::zecd_conventional::validate_recipient(network, address)
                    .map_err(|_| PpsFundingError::UnsupportedRecipient),
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
/// fresh wallet read bracketed by matching DB snapshots and the wallet's verified
/// RPC contract. Config cannot supply a manual wallet attestation.
const _: () = assert!(node_rpc::zecd_funding::PAYOUT_NOTE_MATURITY == pool_db::pps_policy::PAYOUT_NOTE_MATURITY);

pub fn validate_funding_lease(lease: &PpsFundingLease, epoch: &PpsEpoch, now: i64)
    -> Result<(), PpsFundingError>
{
    if lease.network != epoch.network || lease.checked_at_unix < 0
        || lease.checked_at_unix > now || now >= lease.valid_until_unix
        || lease.valid_until_unix.saturating_sub(lease.checked_at_unix) > FUNDING_LEASE_SECONDS
        || lease.valid_until_unix <= lease.checked_at_unix
        || lease.spendable_zatoshis < 0
        || lease.mature_spendable_zatoshis < 0
        || lease.reserve_floor_zatoshis != epoch.reserve_floor_zatoshis
        || lease.reserved_fee_allowance_zatoshis != epoch.fee_allowance_zatoshis
    { return Err(PpsFundingError::InvalidEvidence); }
    Ok(())
}

/// What a collection does when the proven balance is below the requirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortfall {
    /// Refuse the proof: payout preflight, startup and the budget extension must
    /// not act on a wallet that cannot cover what is owed.
    Refuse,
    /// Keep the proof: share admission caches it, and the ledger refuses new
    /// credits against it with `FundingInsufficient` (operator decision 2026-09-15).
    Keep,
}

fn finish_evidence(before: &PpsFundingSnapshot, after: &PpsFundingSnapshot,
    spendable: i64, mature: i64, epoch: &PpsEpoch, checked_at: i64, now: i64, shortfall: Shortfall)
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
    if after.required_spendable_zatoshis <= 0
        || (spendable < required && shortfall == Shortfall::Refuse)
    {
        return Err(PpsFundingError::InsufficientFunding);
    }
    let lease = PpsFundingLease {
        network: epoch.network.clone(), checked_at_unix: checked_at,
        valid_until_unix: checked_at.checked_add(FUNDING_LEASE_SECONDS)
            .ok_or(PpsFundingError::InvalidEvidence)?,
        spendable_zatoshis: spendable,
        mature_spendable_zatoshis: mature.min(spendable),
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
    // Payout preflight keeps a short proof: the ledger's payout rule (committed
    // outflow, which may spend into the reserve floor) decides at reservation
    // and at the seal. Only the budget extension refuses a short proof outright.
    collect_for_route(db, wallet, epoch, from, node, route, Shortfall::Keep).await
}

async fn collect_for_route(
    db: &PoolDb, wallet: &ZcashRpcClient, epoch: &PpsEpoch, from: &str,
    node: &ZcashRpcClient, route: &PpsFundingRoute, shortfall: Shortfall,
) -> Result<PpsFundingLease, PpsFundingError> {
    crate::pps_credit_health::stage("route_policy");
    route.validate(epoch)?;
    match route {
        PpsFundingRoute::ZecdConventional { .. } =>
            collect_testnet_for_snapshot(db, wallet, epoch, from, node, None, shortfall).await,
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
    PpsFundingRoute::ZecdConventional { hold_new_legacy_sends:true }.validate(&next)?;
    collect_testnet_for_snapshot(db, wallet, &next, from, node, Some(previous), Shortfall::Refuse).await
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
    node: &ZcashRpcClient, previous: Option<&PpsEpoch>, shortfall: Shortfall,
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
                    chrono::Utc::now().timestamp(), shortfall)
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
    // Share admission keeps a proof that shows the wallet short: the ledger refuses
    // new credits against it (proven insolvency), while a missing or stale proof
    // stays advisory. Found blocks are still submitted (audit B1).
    let lease=collect_for_route(db,wallet,epoch,from,node,route,Shortfall::Keep).await?;
    if matches!(route,PpsFundingRoute::ZecdConventional { .. }) {
        crate::pps_credit_health::stage("credit_capacity");
        let snapshot=db.pps_funding_snapshot_for_epoch(epoch).await
            .map_err(|_|PpsFundingError::AccountingUnavailable)?;
        if snapshot.generation != lease.generation || snapshot.network != lease.network {
            return Err(PpsFundingError::ConcurrentChange);
        }
        validate_credit_fee_capacity(route,&snapshot)?;
        validate_funding_lease(&lease,epoch,chrono::Utc::now().timestamp())?;
        if lease.spendable_zatoshis < snapshot.required_spendable_zatoshis {
            tracing::warn!("PPS wallet cannot cover what miners are owed (proven); new shares are refused until income or a top-up is spendable");
        }
    }
    Ok(lease)
}

/// Read-only capacity predicate, also used by sampled status. This never issues
/// a funding lease or authorizes a credit; the DB still checks admission itself.
pub fn validate_credit_fee_capacity(_route:&PpsFundingRoute,snapshot:&PpsFundingSnapshot)
    -> Result<(),PpsFundingError>
{
    if snapshot.paid_fees_zatoshis < 0 || snapshot.reserved_fees_zatoshis < 0 {
        return Err(PpsFundingError::InvalidEvidence);
    }
    let available=snapshot.fee_allowance_zatoshis.checked_sub(snapshot.paid_fees_zatoshis)
        .and_then(|v|v.checked_sub(snapshot.reserved_fees_zatoshis))
        .ok_or(PpsFundingError::InvalidEvidence)?;
    let ceiling=node_rpc::zecd_conventional::ConventionalProfile::consensus_size_bound(&snapshot.network,100)
        .and_then(|p|p.fee_ceiling_zatoshis(100)).map_err(|_|PpsFundingError::InvalidEvidence)?;
    if available < ceiling { return Err(PpsFundingError::FeeCapacityExhausted); }
    Ok(())
}

fn finish_testnet_evidence(before: &PpsFundingSnapshot, after: &PpsFundingSnapshot,
    proof: &node_rpc::zecd_funding::ZecdFundingEvidence, epoch: &PpsEpoch,
    checked_at: i64, now: i64, shortfall: Shortfall) -> Result<PpsFundingLease, PpsFundingError>
{
    if proof.checked_at_unix < checked_at || proof.checked_at_unix > now
        || proof.valid_until_unix <= now
        || proof.valid_until_unix.checked_sub(proof.checked_at_unix) != Some(FUNDING_LEASE_SECONDS)
    { return Err(PpsFundingError::InvalidEvidence); }
    // The outer generation bracket starts no later than the wallet evidence.
    // Using its earlier clock shortens the lease and never refreshes stale data.
    let lease = finish_evidence(before, after, proof.confirmed_eligible_zatoshis,
        proof.mature_eligible_zatoshis, epoch, checked_at, now, shortfall)?;
    if lease.valid_until_unix > proof.valid_until_unix {
        return Err(PpsFundingError::InvalidEvidence);
    }
    Ok(lease)
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
    fn conventional_route_requires_a_known_network_sane_bounds_and_legacy_hold() {
        let route = PpsFundingRoute::ZecdConventional { hold_new_legacy_sends: true };
        let e = conventional_epoch();
        assert!(route.validate(&e).is_ok());
        // Any consistent policy on either network is accepted (operator decision
        // 2026-09-15 #1: the same route serves mainnet).
        let mut mainnet = e.clone();
        mainnet.network = "mainnet".into(); mainnet.fee_bps = 10;
        mainnet.max_liability_zatoshis = 5_000_000_000; mainnet.fee_allowance_zatoshis = 100_000_000;
        mainnet.total_exposure_zatoshis = 5_100_000_000; mainnet.reserve_floor_zatoshis = 10_000_000_000;
        assert!(route.validate(&mainnet).is_ok());
        for fee in [0, 100, 9_999] {
            let mut same_policy = e.clone(); same_policy.fee_bps = fee;
            assert!(route.validate(&same_policy).is_ok());
        }
        for altered in 0..6 {
            let mut bad = e.clone();
            match altered {
                0 => bad.network = "regtest".into(),
                1 => bad.max_liability_zatoshis = 0,
                2 => bad.fee_allowance_zatoshis = 0,
                3 => bad.total_exposure_zatoshis = bad.max_liability_zatoshis + bad.fee_allowance_zatoshis - 1,
                4 => bad.reserve_floor_zatoshis = -1,
                _ => bad.fee_bps = 10_000,
            }
            assert_eq!(route.validate(&bad), Err(PpsFundingError::RoutePolicy), "{altered}");
        }
        assert!(PpsFundingRoute::ZecdConventional { hold_new_legacy_sends: false }.validate(&e).is_err());
        assert!(route.holds_new_legacy_sends());
        assert!(!PpsFundingRoute::ZecdConventional { hold_new_legacy_sends: false }.holds_new_legacy_sends());
    }
    #[test]
    fn route_configuration_is_explicit_and_rejects_manual_signer_proofs() {
        let route: PpsFundingRoute = toml::from_str("route='zecd_conventional'\nhold_new_legacy_sends=true").unwrap();
        assert_eq!(route, PpsFundingRoute::ZecdConventional { hold_new_legacy_sends: true });
        // The testnet trial's name still parses; the retired PCZT route does not.
        let trial: PpsFundingRoute = toml::from_str("route='zecd_conventional_testnet'\nhold_new_legacy_sends=true").unwrap();
        assert_eq!(trial, route);
        for config in ["", "route='zecd_conventional'", "route='unknown'",
            "route='zallet_pczt'", "route='zallet_pczt'\nhold_new_legacy_sends=true",
            "route='zecd_conventional'\nhold_new_legacy_sends=true\nsigner_ready=true"]
        { assert!(toml::from_str::<PpsFundingRoute>(config).is_err(), "{config}"); }
    }
    #[test]
    fn recipient_admission_follows_the_network_of_the_pool() {
        let conventional = PpsFundingRoute::ZecdConventional { hold_new_legacy_sends:true };
        let transparent = "tm9iNYCVAhLLa4rJtfqqHauR5xL1REdpiDs";
        let sapling = "ztestsapling1ywvgdtat0cemx5y6ejpu5wapc5x2j0c08f9lee3fd9s6wvv6n079w5nplr48qne73w6swec4vzv";
        let unified = "utest1rak2faln6pat6jx7rmfulvm80c0mjcnj5z2zvsynqn0zhu9gtk97ll5cyvu7maglwgazje4t00958n2yyadc8ee2vskkmg0e7wscqxaaahke023r8pejc097tf0e5zu6ltq9g6f99xxtpfprujl4uhaph3yj7mu52w3da6x0lgj3j0qy";
        for recipient in [transparent,sapling,unified] {
            assert!(conventional.validate_recipient("testnet",recipient).is_ok());
            // A testnet address is not payable on mainnet.
            assert_eq!(conventional.validate_recipient("mainnet",recipient),Err(PpsFundingError::UnsupportedRecipient));
            assert_eq!(PpsFundingRoute::ZecdConventional { hold_new_legacy_sends:false }
                .validate_recipient("testnet",recipient),Err(PpsFundingError::RoutePolicy));
        }
        let mainnet_transparent = "t1HsdDMzmJfq4vc7T17XYjEkLMLvbgM1fCi";
        assert!(conventional.validate_recipient("mainnet",mainnet_transparent).is_ok());
        assert_eq!(conventional.validate_recipient("testnet",mainnet_transparent),Err(PpsFundingError::UnsupportedRecipient));
        assert_eq!(conventional.validate_recipient("regtest",mainnet_transparent),Err(PpsFundingError::UnsupportedRecipient));
        for bad in ["", "unsupported", "MAINNET", " unified "] {
            assert!(conventional.validate_recipient("testnet",bad).is_err());
            assert!(conventional.validate_recipient("mainnet",bad).is_err());
        }
    }
    #[test]
    fn lease_rejects_expiry_future_and_mutated_economic_bounds() {
        let e = epoch();
        let lease = PpsFundingLease { network: "testnet".into(), checked_at_unix:100,
            valid_until_unix:160, spendable_zatoshis:1_000_000_001, mature_spendable_zatoshis:1_000_000_001,
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
            pps_paying_zatoshis:0, committed_outflow_zatoshis:5,
        };
        assert!(finish_evidence(&s,&s,1_000_000_006,1_000_000_006,&e,100,100,Shortfall::Refuse).is_ok());
        assert_eq!(finish_evidence(&s,&s,1_000_000_005,1_000_000_005,&e,100,100,Shortfall::Refuse),Err(PpsFundingError::InsufficientFunding));
        assert_eq!(finish_evidence(&s,&s,1_000_000_006,1_000_000_006,&e,100,100+FUNDING_LEASE_SECONDS,Shortfall::Refuse),Err(PpsFundingError::InvalidEvidence));
        let mut changed=s.clone(); changed.generation+=1;
        assert_eq!(finish_evidence(&s,&changed,i64::MAX,i64::MAX,&e,100,100,Shortfall::Refuse),Err(PpsFundingError::ConcurrentChange));
        // A credit between the reads moves the requirement, not the generation:
        // the refresh succeeds only if the proof covers the larger requirement.
        let mut changed=s.clone(); changed.required_spendable_zatoshis+=1;
        assert!(finish_evidence(&s,&changed,1_000_000_007,1_000_000_007,&e,100,100,Shortfall::Refuse).is_ok());
        assert_eq!(finish_evidence(&s,&changed,1_000_000_006,1_000_000_006,&e,100,100,Shortfall::Refuse),Err(PpsFundingError::InsufficientFunding));
        assert_eq!(finish_evidence(&changed,&s,1_000_000_006,1_000_000_006,&e,100,100,Shortfall::Refuse),Err(PpsFundingError::InsufficientFunding));
        // Share admission keeps a short proof, unchanged, so the ledger can refuse
        // new credits with it; a broken requirement is still refused.
        let short = finish_evidence(&s,&s,1_000_000_005,1_000_000_005,&e,100,100,Shortfall::Keep).unwrap();
        assert_eq!((short.spendable_zatoshis, short.generation), (1_000_000_005, 7));
        // The mature balance rides along for the payout rule, never above the
        // confirmed one and never negative.
        let split = finish_evidence(&s,&s,1_000_000_006,400,&e,100,100,Shortfall::Refuse).unwrap();
        assert_eq!((split.spendable_zatoshis, split.mature_spendable_zatoshis), (1_000_000_006, 400));
        assert_eq!(finish_evidence(&s,&s,7,9,&e,100,100,Shortfall::Keep).unwrap().mature_spendable_zatoshis, 7);
        assert_eq!(finish_evidence(&s,&s,1_000_000_006,-1,&e,100,100,Shortfall::Refuse),Err(PpsFundingError::InvalidEvidence));
        let mut broken = s.clone(); broken.required_spendable_zatoshis = 0;
        assert_eq!(finish_evidence(&broken,&broken,1,1,&e,100,100,Shortfall::Keep),Err(PpsFundingError::InsufficientFunding));
    }

    #[test]
    fn conventional_credit_capacity_reserves_next_max_batch_but_not_payout_funding() {
        let route=PpsFundingRoute::ZecdConventional { hold_new_legacy_sends:true };
        let s=PpsFundingSnapshot {
            network:"testnet".into(),generation:0,legacy_pending_zatoshis:0,legacy_paying_zatoshis:0,
            pps_outstanding_subzatoshis:0,unused_credit_subzatoshis:0,gross_subzatoshis:0,
            paid_zatoshis:0,cap_subzatoshis:0,total_exposure_zatoshis:1_000_000_000,
            reserve_floor_zatoshis:1,fee_allowance_zatoshis:50_000_000,paid_fees_zatoshis:0,
            reserved_fees_zatoshis:0,required_spendable_zatoshis:1,
            pps_paying_zatoshis:0,committed_outflow_zatoshis:0,
        };
        let ceiling=28_905_000;
        assert!(validate_credit_fee_capacity(&route,&s).is_ok());
        let mut at_limit=s.clone(); at_limit.paid_fees_zatoshis=50_000_000-ceiling;
        assert!(validate_credit_fee_capacity(&route,&at_limit).is_ok());
        at_limit.paid_fees_zatoshis+=1;
        assert_eq!(validate_credit_fee_capacity(&route,&at_limit),Err(PpsFundingError::FeeCapacityExhausted));
        let mut reserved=s.clone(); reserved.reserved_fees_zatoshis=ceiling;
        assert_eq!(validate_credit_fee_capacity(&route,&reserved),Err(PpsFundingError::FeeCapacityExhausted));
        // The ordinary/payout funding finish remains available after reserving
        // a batch; only the explicitly named credit wrapper applies this gate.
        let e=conventional_epoch();
        assert!(finish_evidence(&reserved,&reserved,1,1,&e,100,100,Shortfall::Refuse).is_ok());
        reserved.reserved_fees_zatoshis=-1;
        assert_eq!(validate_credit_fee_capacity(&route,&reserved),Err(PpsFundingError::InvalidEvidence));
    }
}
