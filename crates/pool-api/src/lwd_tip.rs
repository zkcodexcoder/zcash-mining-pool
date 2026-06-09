//! Authoritative chain-tip lookup across multiple public lightwalletd
//! servers.
//!
//! Zebra's local `sync_estimated_network_tip_height` is a peer-gossip
//! estimate that can lag (or, oddly, report *below* its own verified tip).
//! For pool ops we need a second, independent opinion on "where is the
//! network's tip *really*?" — so we fan-out `GetLatestBlock` to a quorum of
//! lwd servers and take the max responding height.
//!
//! Why max? A server that's behind reports a smaller tip; a server that's
//! at-tip reports the latest. Picking the maximum is the most up-to-date
//! known value. An attacker would have to control enough servers to be
//! *first* with a higher-than-real claim, which the surrounding
//! "responses_ok / responses_total" lets the operator notice.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;
use tonic::transport::{ClientTlsConfig, Endpoint};

pub mod proto {
    tonic::include_proto!("cash.z.wallet.sdk.rpc");
}

use proto::compact_tx_streamer_client::CompactTxStreamerClient;
use proto::ChainSpec;

/// Per-server timeout. Public lwd servers usually answer in 50-300ms;
/// 3s is plenty of headroom for an under-provisioned one.
const PER_SERVER_TIMEOUT: Duration = Duration::from_secs(3);

/// Aggregated snapshot of authoritative-tip queries — what the dashboard
/// renders.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthoritativeTip {
    /// Max height across all responding servers. `None` when no server
    /// responded.
    pub max_height: Option<u64>,
    /// Per-server best height — populated for any host that answered.
    pub per_server_height: HashMap<String, u64>,
    /// Per-server error string for hosts that failed; gives the operator
    /// visibility into which endpoints are flaky.
    pub per_server_error: HashMap<String, String>,
    pub responses_ok: usize,
    pub responses_total: usize,
    /// Wall-clock time of the snapshot, unix seconds.
    pub fetched_at_unix: Option<i64>,
}

/// Fan-out `GetLatestBlock` to every server in `servers` concurrently and
/// aggregate the responses. Never errors out — slow/dead servers populate
/// `per_server_error` instead.
pub async fn fetch_authoritative_tip(servers: &[String]) -> AuthoritativeTip {
    let mut set = JoinSet::new();
    for host in servers.iter().cloned() {
        set.spawn(async move {
            let res = query_one(&host).await;
            (host, res)
        });
    }

    let mut ok = HashMap::new();
    let mut err = HashMap::new();
    let total = servers.len();
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok((host, Ok(h))) => {
                ok.insert(host, h);
            }
            Ok((host, Err(e))) => {
                err.insert(host, e);
            }
            Err(e) => {
                tracing::warn!(error = %e, "lwd_tip: spawned task panicked");
            }
        }
    }

    AuthoritativeTip {
        max_height: ok.values().copied().max(),
        responses_ok: ok.len(),
        responses_total: total,
        per_server_height: ok,
        per_server_error: err,
        fetched_at_unix: Some(chrono::Utc::now().timestamp()),
    }
}

/// Single-server query. `host` is a bare hostname like `eu.zec.rocks`;
/// we always use HTTPS on 443. If a future operator needs plain gRPC on
/// 9067, change the URL builder here.
async fn query_one(host: &str) -> Result<u64, String> {
    let url = format!("https://{host}");
    let tls = ClientTlsConfig::new()
        .with_native_roots()
        .domain_name(host.to_string());

    let endpoint = Endpoint::from_shared(url)
        .map_err(|e| format!("invalid url: {e}"))?
        .timeout(PER_SERVER_TIMEOUT)
        .connect_timeout(PER_SERVER_TIMEOUT)
        .tls_config(tls)
        .map_err(|e| format!("tls config: {e}"))?;

    let channel = endpoint
        .connect()
        .await
        .map_err(|e| format!("connect: {e}"))?;

    let mut client = CompactTxStreamerClient::new(channel);

    // Belt-and-suspenders: even though Endpoint::timeout caps per-request
    // time, wrap the whole call in tokio::time::timeout in case the gRPC
    // status arrives in trailers and stalls there.
    let response = tokio::time::timeout(
        PER_SERVER_TIMEOUT,
        client.get_latest_block(ChainSpec {}),
    )
    .await
    .map_err(|_| "rpc timeout".to_string())?
    .map_err(|e| format!("rpc: {}", e.message()))?;

    Ok(response.into_inner().height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_server_list_returns_empty_tip() {
        // Sanity: with no servers configured, we should produce an empty
        // snapshot (no calls, no errors), not panic.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let snap = rt.block_on(fetch_authoritative_tip(&[]));
        assert!(snap.max_height.is_none());
        assert_eq!(snap.responses_ok, 0);
        assert_eq!(snap.responses_total, 0);
        assert!(snap.per_server_height.is_empty());
        assert!(snap.per_server_error.is_empty());
    }

    #[test]
    fn bogus_host_lands_in_error_map_without_panic() {
        // A non-resolvable host should turn into a per-server error entry
        // and the aggregate snapshot should reflect responses_ok=0.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let snap = rt.block_on(fetch_authoritative_tip(&[
            "this-host-does-not-exist.invalid".to_string(),
        ]));
        assert_eq!(snap.responses_total, 1);
        assert_eq!(snap.responses_ok, 0);
        assert!(snap.per_server_height.is_empty());
        assert_eq!(snap.per_server_error.len(), 1);
    }
}
