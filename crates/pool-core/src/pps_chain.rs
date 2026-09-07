//! Short-lived, fail-closed canonical-chain proof for PPS credit admission.
//!
//! Consults two fixed, separately operated public services. Only public block
//! heights leave this process; no local RPC identity, credentials or wallet data
//! are sent. TLS/server authentication and gRPC status handling are mandatory.
//! This is independent-operator agreement, not a replacement for consensus
//! validation. Missing, stale or conflicting evidence never renews the lease.

use chrono::Utc;
use node_rpc::ZcashRpcClient;
pub use pool_db::pps_live::PpsChainLease;
use rewards::PpsNetwork;
use serde_json::{json, Value};
use std::{future::Future, time::Duration};
use tonic::{
    transport::{Channel, ClientTlsConfig, Endpoint},
    Request, Response,
};

const QUERY_LIMIT: Duration = Duration::from_secs(10);
const CHECK_LIMIT: Duration = Duration::from_secs(45);
const BODY_LIMIT: usize = 65_536;
const TIP_SPREAD: u64 = 24;
const LEASE_SECONDS: i64 = 90;
const RECENT_ANCHOR_MAX_AGE: i64 = 30 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PpsChainError {
    #[error("PPS chain verification timed out")]
    Timeout,
    #[error("PPS chain reference unavailable")]
    Unavailable,
    #[error("PPS chain evidence malformed")]
    InvalidEvidence,
    #[error("PPS chain network identity mismatch")]
    NetworkMismatch,
    #[error("PPS chain node is not synchronized")]
    NotSynchronized,
    #[error("PPS chain consensus branch mismatch")]
    BranchMismatch,
    #[error("PPS chain reference tips are not aligned")]
    TipMismatch,
    #[error("PPS chain block hash disagreement")]
    HashMismatch,
    #[error("PPS chain evidence is stale")]
    StaleEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChainInfo {
    height: u64,
    branch: String,
}

// Minimal wire-compatible subset of lightwalletd's published service.proto.
#[derive(Clone, PartialEq, prost::Message)]
struct Empty {}
#[derive(Clone, PartialEq, prost::Message)]
struct BlockId {
    #[prost(uint64, tag = "1")]
    height: u64,
}
#[derive(Clone, PartialEq, prost::Message)]
struct LightdInfo {
    #[prost(string, tag = "4")]
    chain_name: String,
    #[prost(string, tag = "6")]
    consensus_branch_id: String,
    #[prost(uint64, tag = "7")]
    block_height: u64,
}
#[derive(Clone, PartialEq, prost::Message)]
struct TreeState {
    #[prost(string, tag = "1")]
    network: String,
    #[prost(uint64, tag = "2")]
    height: u64,
    #[prost(string, tag = "3")]
    hash: String,
    #[prost(uint32, tag = "4")]
    time: u32,
}

fn network_name(network: PpsNetwork) -> &'static str {
    match network {
        PpsNetwork::Mainnet => "mainnet",
        PpsNetwork::Testnet => "testnet",
    }
}
fn check_network(value: &str, network: PpsNetwork) -> Result<(), PpsChainError> {
    let matches = match network {
        PpsNetwork::Mainnet => matches!(value, "main" | "mainnet"),
        PpsNetwork::Testnet => matches!(value, "test" | "testnet"),
    };
    if matches {
        Ok(())
    } else {
        Err(PpsChainError::NetworkMismatch)
    }
}
fn normalized_hex(value: &str, length: usize) -> Result<String, PpsChainError> {
    if value.len() != length || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(PpsChainError::InvalidEvidence);
    }
    Ok(value.to_ascii_lowercase())
}
fn info(height: u64, branch: &str) -> Result<ChainInfo, PpsChainError> {
    if !(157..=u32::MAX as u64).contains(&height) {
        return Err(PpsChainError::InvalidEvidence);
    }
    Ok(ChainInfo {
        height,
        branch: normalized_hex(branch, 8)?,
    })
}
fn rpc_info(
    value: &Value,
    network: PpsNetwork,
    require_synced: bool,
) -> Result<ChainInfo, PpsChainError> {
    check_network(
        value
            .get("chain")
            .and_then(Value::as_str)
            .ok_or(PpsChainError::InvalidEvidence)?,
        network,
    )?;
    if require_synced {
        crate::pps_economics::validate_local_sync(value)
            .map_err(|_| PpsChainError::NotSynchronized)?;
    }
    info(
        value
            .get("blocks")
            .and_then(Value::as_u64)
            .ok_or(PpsChainError::InvalidEvidence)?,
        value
            .pointer("/consensus/chaintip")
            .and_then(Value::as_str)
            .ok_or(PpsChainError::InvalidEvidence)?,
    )
}
fn comparison_heights(infos: &[ChainInfo; 3]) -> Result<[u64; 2], PpsChainError> {
    if infos.iter().any(|i| i.branch != infos[0].branch) {
        return Err(PpsChainError::BranchMismatch);
    }
    let low = infos
        .iter()
        .map(|i| i.height)
        .min()
        .ok_or(PpsChainError::InvalidEvidence)?;
    let high = infos
        .iter()
        .map(|i| i.height)
        .max()
        .ok_or(PpsChainError::InvalidEvidence)?;
    if high - low > TIP_SPREAD {
        return Err(PpsChainError::TipMismatch);
    }
    Ok([
        low.checked_sub(12).ok_or(PpsChainError::InvalidEvidence)?,
        low.checked_sub(156).ok_or(PpsChainError::InvalidEvidence)?,
    ])
}
fn tree_hash(
    tree: &TreeState,
    network: PpsNetwork,
    height: u64,
    recent: bool,
    now: i64,
) -> Result<String, PpsChainError> {
    check_network(&tree.network, network)?;
    if tree.height != height {
        return Err(PpsChainError::InvalidEvidence);
    }
    let timestamp = i64::from(tree.time);
    if recent && (timestamp > now + 120 || timestamp < now - RECENT_ANCHOR_MAX_AGE) {
        return Err(PpsChainError::StaleEvidence);
    }
    normalized_hex(&tree.hash, 64)
}
fn rest_hash(value: &Value, height: u64) -> Result<String, PpsChainError> {
    // CipherScan's block route serializes SQL block heights as decimal strings,
    // while blockchain-info uses JSON numbers. Admit only exact canonical
    // integer forms, never floating-point coercion or a best-effort parse.
    let returned_height = match value.get("height") {
        Some(Value::Number(number)) => number.as_u64(),
        Some(Value::String(text))
            if !text.is_empty()
                && text.len() <= 10
                && text.bytes().all(|c| c.is_ascii_digit())
                && (text.len() == 1 || !text.starts_with('0')) =>
        {
            text.parse::<u64>().ok()
        }
        _ => None,
    };
    if returned_height != Some(height)
        || value.get("isOrphaned").and_then(Value::as_bool) != Some(false)
    {
        return Err(PpsChainError::InvalidEvidence);
    }
    normalized_hex(
        value
            .get("hash")
            .and_then(Value::as_str)
            .ok_or(PpsChainError::InvalidEvidence)?,
        64,
    )
}
fn agree(hashes: [&str; 3]) -> Result<(), PpsChainError> {
    if hashes[0] == hashes[1] && hashes[0] == hashes[2] {
        Ok(())
    } else {
        Err(PpsChainError::HashMismatch)
    }
}
async fn bounded<T>(
    future: impl Future<Output = Result<T, PpsChainError>>,
) -> Result<T, PpsChainError> {
    tokio::time::timeout(QUERY_LIMIT, future)
        .await
        .map_err(|_| PpsChainError::Timeout)?
}
async fn grpc_connect(host: &'static str) -> Result<Channel, PpsChainError> {
    bounded(async {
        Endpoint::from_shared(format!("https://{host}:443"))
            .map_err(|_| PpsChainError::Unavailable)?
            .connect_timeout(QUERY_LIMIT)
            .timeout(QUERY_LIMIT)
            .tls_config(ClientTlsConfig::new().with_native_roots().domain_name(host))
            .map_err(|_| PpsChainError::Unavailable)?
            .connect()
            .await
            .map_err(|_| PpsChainError::Unavailable)
    })
    .await
}
async fn grpc_info(channel: Channel, network: PpsNetwork) -> Result<ChainInfo, PpsChainError> {
    bounded(async {
        let mut grpc = tonic::client::Grpc::new(channel).max_decoding_message_size(BODY_LIMIT);
        grpc.ready().await.map_err(|_| PpsChainError::Unavailable)?;
        let response: Response<LightdInfo> = grpc
            .unary(
                Request::new(Empty {}),
                tonic::codegen::http::uri::PathAndQuery::from_static(
                    "/cash.z.wallet.sdk.rpc.CompactTxStreamer/GetLightdInfo",
                ),
                tonic::codec::ProstCodec::default(),
            )
            .await
            .map_err(|_| PpsChainError::Unavailable)?;
        let value = response.into_inner();
        check_network(&value.chain_name, network)?;
        info(value.block_height, &value.consensus_branch_id)
    })
    .await
}
async fn grpc_hash(
    channel: Channel,
    network: PpsNetwork,
    height: u64,
    recent: bool,
    now: i64,
) -> Result<String, PpsChainError> {
    bounded(async {
        let mut grpc = tonic::client::Grpc::new(channel).max_decoding_message_size(BODY_LIMIT);
        grpc.ready().await.map_err(|_| PpsChainError::Unavailable)?;
        let response: Response<TreeState> = grpc
            .unary(
                Request::new(BlockId { height }),
                tonic::codegen::http::uri::PathAndQuery::from_static(
                    "/cash.z.wallet.sdk.rpc.CompactTxStreamer/GetTreeState",
                ),
                tonic::codec::ProstCodec::default(),
            )
            .await
            .map_err(|_| PpsChainError::Unavailable)?;
        tree_hash(&response.into_inner(), network, height, recent, now)
    })
    .await
}
async fn rest_get(client: &reqwest::Client, path: &str) -> Result<Value, PpsChainError> {
    bounded(async {
        // Paths are constructed only below, never from caller-controlled text.
        let mut response = client
            .get(format!("https://api.testnet.cipherscan.app/api/{path}"))
            .send()
            .await
            .map_err(|_| PpsChainError::Unavailable)?;
        if response.status() != reqwest::StatusCode::OK
            || response
                .content_length()
                .is_some_and(|n| n > BODY_LIMIT as u64)
        {
            return Err(PpsChainError::Unavailable);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| PpsChainError::Unavailable)?
        {
            if bytes.len().saturating_add(chunk.len()) > BODY_LIMIT {
                return Err(PpsChainError::InvalidEvidence);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| PpsChainError::InvalidEvidence)
    })
    .await
}
async fn local_info(rpc: &ZcashRpcClient, network: PpsNetwork) -> Result<ChainInfo, PpsChainError> {
    bounded(async {
        let value: Value = rpc
            .call_raw("getblockchaininfo", json!([]))
            .await
            .map_err(|_| PpsChainError::Unavailable)?;
        rpc_info(&value, network, true)
    })
    .await
}
async fn local_hash(rpc: &ZcashRpcClient, height: u64) -> Result<String, PpsChainError> {
    bounded(async {
        let hash = rpc
            .get_block_hash(height)
            .await
            .map_err(|_| PpsChainError::Unavailable)?;
        normalized_hex(&hash, 64)
    })
    .await
}

/// Renew only after both operators and the local node agree on network,
/// consensus branch, bounded tip spread, and hashes at common tip minus 12 and
/// 156. Public requests contain no local data other than those block heights.
/// The caller must discard an expired lease and must never refresh on failure.
pub async fn verify_pps_chain(
    rpc: &ZcashRpcClient,
    network: PpsNetwork,
) -> Result<PpsChainLease, PpsChainError> {
    tokio::time::timeout(CHECK_LIMIT, verify_inner(rpc, network))
        .await
        .map_err(|_| PpsChainError::Timeout)?
}
async fn verify_inner(
    rpc: &ZcashRpcClient,
    network: PpsNetwork,
) -> Result<PpsChainLease, PpsChainError> {
    let client = reqwest::Client::builder()
        .https_only(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(QUERY_LIMIT)
        .timeout(QUERY_LIMIT)
        .build()
        .map_err(|_| PpsChainError::Unavailable)?;
    let host_a = match network {
        PpsNetwork::Mainnet => "zec.rocks",
        PpsNetwork::Testnet => "testnet.zec.rocks",
    };
    let (local, channel_a, channel_b) =
        tokio::try_join!(local_info(rpc, network), grpc_connect(host_a), async {
            if network == PpsNetwork::Mainnet {
                grpc_connect("lightwalletd.mainnet.cipherscan.app")
                    .await
                    .map(Some)
            } else {
                Ok(None)
            }
        })?;
    let (a, b) = tokio::try_join!(grpc_info(channel_a.clone(), network), async {
        match &channel_b {
            Some(channel) => grpc_info(channel.clone(), network).await,
            None => rpc_info(&rest_get(&client, "blockchain-info").await?, network, false),
        }
    })?;
    let mut infos = [local.clone(), a, b];
    let heights = comparison_heights(&infos)?;
    let now = Utc::now().timestamp();
    let mut initial_hashes = Vec::with_capacity(2);
    for (index, height) in heights.into_iter().enumerate() {
        let recent = index == 0;
        let (ours, a, b) = tokio::try_join!(
            local_hash(rpc, height),
            grpc_hash(channel_a.clone(), network, height, recent, now),
            async {
                match &channel_b {
                    Some(channel) => grpc_hash(channel.clone(), network, height, recent, now).await,
                    None => rest_hash(
                        &rest_get(&client, &format!("block/{height}")).await?,
                        height,
                    ),
                }
            }
        )?;
        agree([&ours, &a, &b])?;
        initial_hashes.push(ours);
    }
    // Reject local regression/branch change during collection. Re-read both
    // anchors to reject a local reorg between the two initial comparisons.
    let (final_local, recent_hash, historical_hash) = tokio::try_join!(
        local_info(rpc, network),
        local_hash(rpc, heights[0]),
        local_hash(rpc, heights[1])
    )?;
    if final_local.branch != local.branch
        || final_local.height < local.height
        || final_local.height - local.height > TIP_SPREAD
    {
        return Err(PpsChainError::TipMismatch);
    }
    if recent_hash != initial_hashes[0] || historical_hash != initial_hashes[1] {
        return Err(PpsChainError::HashMismatch);
    }
    infos[0] = final_local;
    comparison_heights(&infos)?;
    let checked_at_unix = Utc::now().timestamp();
    Ok(PpsChainLease {
        network: network_name(network).to_owned(),
        checked_at_unix,
        valid_until_unix: checked_at_unix + LEASE_SECONDS,
        agreeing_references: 2,
        disagreement: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Explicit opt-in public-only transport check. No local node is contacted,
    /// and this test does NOT issue a production/local-chain agreement lease.
    #[tokio::test]
    #[ignore = "contacts the fixed public reference operators; opt-in only"]
    async fn public_reference_transport_smoke() -> Result<(), PpsChainError> {
        tokio::time::timeout(CHECK_LIMIT, async {
            let client = reqwest::Client::builder()
                .https_only(true)
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(QUERY_LIMIT)
                .timeout(QUERY_LIMIT)
                .build()
                .map_err(|_| PpsChainError::Unavailable)?;
            for network in [PpsNetwork::Mainnet, PpsNetwork::Testnet] {
                let host = match network {
                    PpsNetwork::Mainnet => "zec.rocks",
                    PpsNetwork::Testnet => "testnet.zec.rocks",
                };
                let (a, b) = tokio::try_join!(grpc_connect(host), async {
                    if network == PpsNetwork::Mainnet {
                        grpc_connect("lightwalletd.mainnet.cipherscan.app")
                            .await
                            .map(Some)
                    } else {
                        Ok(None)
                    }
                })?;
                let (ai, bi) = tokio::try_join!(grpc_info(a.clone(), network), async {
                    match &b {
                        Some(b) => grpc_info(b.clone(), network).await,
                        None => {
                            rpc_info(&rest_get(&client, "blockchain-info").await?, network, false)
                        }
                    }
                })?;
                let heights = comparison_heights(&[ai.clone(), ai, bi])?;
                for (index, height) in heights.into_iter().enumerate() {
                    let now = Utc::now().timestamp();
                    let (ah, bh) = tokio::try_join!(
                        grpc_hash(a.clone(), network, height, index == 0, now),
                        async {
                            match &b {
                                Some(b) => {
                                    grpc_hash(b.clone(), network, height, index == 0, now).await
                                }
                                None => rest_hash(
                                    &rest_get(&client, &format!("block/{height}")).await?,
                                    height,
                                ),
                            }
                        }
                    )?;
                    agree([&ah, &ah, &bh])?;
                }
                println!(
                    "public PPS reference transport: {} passed",
                    network_name(network)
                );
            }
            Ok(())
        })
        .await
        .map_err(|_| PpsChainError::Timeout)?
    }
    fn metadata(height: u64) -> ChainInfo {
        info(height, "1234abcd").unwrap()
    }
    #[test]
    fn both_networks_require_exact_identity_and_sync_schema() {
        for (network, chain) in [(PpsNetwork::Mainnet, "main"), (PpsNetwork::Testnet, "test")] {
            let mut value = json!({"chain":chain,"blocks":1000,"initialblockdownload":false,"consensus":{"chaintip":"1234abcd"}});
            assert!(rpc_info(&value, network, true).is_ok());
            value["initialblockdownload"] = Value::Null;
            assert_eq!(
                rpc_info(&value, network, true),
                Err(PpsChainError::NotSynchronized)
            );
            value["chain"] = json!("regtest");
            assert_eq!(
                rpc_info(&value, network, false),
                Err(PpsChainError::NetworkMismatch)
            );
        }
    }
    #[test]
    fn comparison_requires_branch_and_bounded_tip_agreement() {
        assert_eq!(
            comparison_heights(&[metadata(1000), metadata(1008), metadata(1024)]).unwrap(),
            [988, 844]
        );
        assert_eq!(
            comparison_heights(&[metadata(1025), metadata(1000), metadata(1001)]),
            Err(PpsChainError::TipMismatch)
        );
        let mut wrong = metadata(1000);
        wrong.branch = "deadbeef".into();
        assert_eq!(
            comparison_heights(&[metadata(1000), metadata(1000), wrong]),
            Err(PpsChainError::BranchMismatch)
        );
        assert!(info(156, "1234abcd").is_err());
        assert!(info(1000, "abcd").is_err());
    }
    #[test]
    fn tree_evidence_binds_network_height_freshness_and_hash() {
        let now = 1_800_000_000;
        let mut tree = TreeState {
            network: "test".into(),
            height: 988,
            hash: "ab".repeat(32),
            time: (now - 900) as u32,
        };
        assert!(tree_hash(&tree, PpsNetwork::Testnet, 988, true, now).is_ok());
        assert!(tree_hash(&tree, PpsNetwork::Mainnet, 988, true, now).is_err());
        assert!(tree_hash(&tree, PpsNetwork::Testnet, 987, true, now).is_err());
        tree.time = (now - RECENT_ANCHOR_MAX_AGE - 1) as u32;
        assert_eq!(
            tree_hash(&tree, PpsNetwork::Testnet, 988, true, now),
            Err(PpsChainError::StaleEvidence)
        );
        tree.time = (now + 121) as u32;
        assert!(tree_hash(&tree, PpsNetwork::Testnet, 988, true, now).is_err());
        tree.time = now as u32;
        tree.hash = "x".repeat(64);
        assert!(tree_hash(&tree, PpsNetwork::Testnet, 988, true, now).is_err());
    }
    #[test]
    fn rest_rejects_orphans_missing_flags_wrong_heights_and_bad_hashes() {
        let mut v = json!({"height":988,"isOrphaned":false,"hash":"ab".repeat(32)});
        assert!(rest_hash(&v, 988).is_ok());
        assert!(rest_hash(&v, 987).is_err());
        v["height"] = json!("988");
        assert!(rest_hash(&v, 988).is_ok());
        for invalid_height in [
            json!("0988"),
            json!("+988"),
            json!("988.0"),
            json!(988.0),
            json!("9.88e2"),
        ] {
            v["height"] = invalid_height;
            assert!(rest_hash(&v, 988).is_err());
        }
        v["height"] = json!(988);
        for flag in [json!(true), Value::Null, json!(0)] {
            v["isOrphaned"] = flag;
            assert!(rest_hash(&v, 988).is_err());
        }
        v["isOrphaned"] = json!(false);
        v["hash"] = json!("a".repeat(63));
        assert!(rest_hash(&v, 988).is_err());
    }
    #[test]
    fn both_reference_hashes_must_match_local_not_just_each_other() {
        let good = "ab".repeat(32);
        let bad = "cd".repeat(32);
        assert!(agree([&good, &good, &good]).is_ok());
        assert_eq!(
            agree([&bad, &good, &good]),
            Err(PpsChainError::HashMismatch)
        );
        assert_eq!(
            agree([&good, &bad, &good]),
            Err(PpsChainError::HashMismatch)
        );
        assert_eq!(
            agree([&good, &good, &bad]),
            Err(PpsChainError::HashMismatch)
        );
    }
}
