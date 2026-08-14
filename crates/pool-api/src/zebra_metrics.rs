//! Best-effort scrape of zebrad's `/metrics` endpoint.
//!
//! Picks out a curated set of gauges/counters that tell us whether zebra is
//! healthy enough to serve mining: at-tip status, peer count, RPC health,
//! mempool depth, block-flow rates. Surfaced on the admin health page.
//!
//! Design constraints:
//!  - Must never error out the admin endpoint. On any failure (network,
//!    timeout, parse) the returned struct has `scrape_error` populated and
//!    the rest of the fields are `None`.
//!  - Tight timeout (2 s default). The local zebra answered in 16 ms in
//!    testing, so anything over a couple of seconds is already a problem.
//!  - We parse the Prom text format inline — only ~12 specific lines out of
//!    ~11 700, so no need for the `prometheus-parse` crate.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Total request timeout. History: zebra 4.3.x answered in ~16ms; 4.4.1 grew
/// to ~9 MB; zakura v1.1.0 emits ~117 MB (unbounded per-connection `addr`
/// labels, ~34k new dead series/day — task #17 tracks the node-side bug) AND
/// emits our curated targets at the very END of the body, so early-exit can't
/// save us: the full body must stream. 25s covers today's ~9.3s with ~2x
/// growth headroom; MAX_SCRAPE_BYTES bounds the pathological end.
const SCRAPE_TIMEOUT: Duration = Duration::from_secs(25);

/// Abort the scrape past this many body bytes with a clear error, so the
/// node-side cardinality leak can never balloon the dashboard's work
/// unboundedly.
const MAX_SCRAPE_BYTES: usize = 400 * 1024 * 1024;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct ZebraMetrics {
    // At-tip indicators
    pub verified_height: Option<u64>,
    pub network_tip_height: Option<u64>,
    pub distance_to_tip: Option<u64>,

    // RPC health (zebrad's own RPC server)
    pub rpc_active_requests: Option<u64>,
    pub gbt_requests_total: Option<u64>,
    pub gbt_errors_total: Option<u64>,

    // Block flow
    pub verified_blocks_total: Option<u64>,
    pub gossip_queued_blocks: Option<u64>,

    // Network
    pub peers: Option<u64>,

    // Mempool
    pub mempool_txs: Option<u64>,
    pub mempool_bytes: Option<u64>,

    // Build
    pub zebra_version: Option<String>,

    // Scrape meta
    pub scrape_duration_ms: Option<u64>,
    pub scrape_error: Option<String>,
}

/// Build a metrics URL from a node RPC URL by swapping the standard zebrad
/// RPC port (8232) for the standard metrics port (9999) and appending
/// `/metrics`. Operators with non-standard ports must set `[node].metrics_url`
/// explicitly in pool.toml.
pub fn derive_metrics_url(rpc_url: &str) -> String {
    // Audit #20: the old string-append fallback produced double-port URLs
    // ("http://host:18232:9999/metrics") whenever the RPC URL used a
    // non-8232 port — reqwest then failed with an opaque "builder error"
    // (testnet's exact symptom). Parse properly and SET the port instead.
    if let Ok(mut url) = reqwest::Url::parse(rpc_url.trim_end_matches('/')) {
        if url.set_port(Some(9999)).is_ok() {
            url.set_path("/metrics");
            return url.to_string();
        }
    }
    // Unparseable input: keep a best-effort string form (still better than
    // panicking); operators should set [node].metrics_url explicitly.
    let trimmed = rpc_url.trim_end_matches('/');
    let swapped = if trimmed.contains(":8232") {
        trimmed.replacen(":8232", ":9999", 1)
    } else {
        trimmed.to_string()
    };
    format!("{}/metrics", swapped.trim_end_matches('/'))
}

pub async fn fetch_zebra_metrics(metrics_url: &str) -> ZebraMetrics {
    let started = Instant::now();
    let client = match reqwest::Client::builder().timeout(SCRAPE_TIMEOUT).build() {
        Ok(c) => c,
        Err(e) => {
            return failed(format!("client build: {e}"), started);
        }
    };

    let mut resp = match client.get(metrics_url).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => return failed(format!("HTTP {} from {metrics_url}", r.status().as_u16()), started),
        // Include the URL: a malformed URL surfaces here as an opaque
        // "builder error" otherwise (audit #20 — self-diagnosing next time).
        Err(e) => return failed(format!("request to {metrics_url}: {e}"), started),
    };

    // Read body in chunks. Parse each complete line as soon as we have it
    // and bail out the moment every target we care about is filled — zebra
    // 4.4.x emits ~9 MB of per-peer rows after our targets, which we drop
    // by hanging up on the chunked reader.
    //
    // Implementation note: we keep partial trailing data in a String
    // (`leftover`) and only ever `.lines()` over complete-line slices.
    // Earlier attempts used `Vec<u8>::drain` per line, which is O(remaining)
    // per drain and silently turned the scrape into a 9 MB memcpy storm.
    let mut m = ZebraMetrics::default();
    let mut leftover = String::with_capacity(8 * 1024);
    let mut bytes_read: usize = 0;
    let mut early_exit = false;
    'outer: loop {
        let chunk = match resp.chunk().await {
            Ok(Some(c)) => { bytes_read += c.len(); c }
            Ok(None) => break,
            Err(e) => {
                m.scrape_error = Some(format!(
                    "chunk after {bytes_read} bytes from {metrics_url}: {e}"
                ));
                break;
            }
        };
        if bytes_read > MAX_SCRAPE_BYTES {
            m.scrape_error = Some(format!(
                "metrics body exceeded {} MB cap (node-side cardinality leak, see task #17)",
                MAX_SCRAPE_BYTES / (1024 * 1024)
            ));
            break;
        }
        // Treat the chunk as UTF-8 (prom text format is ASCII). Any
        // non-UTF-8 byte in the middle of a label value is exceedingly
        // rare; lossy decode just drops it.
        let chunk_str = String::from_utf8_lossy(&chunk);
        leftover.push_str(&chunk_str);
        if let Some(last_nl) = leftover.rfind('\n') {
            // Everything up to (and including) last_nl is complete-line
            // content; everything after is a partial trailing line we
            // carry over.
            let (complete, tail) = leftover.split_at(last_nl + 1);
            for line in complete.lines() {
                parse_line(line, &mut m);
                if all_targets_filled(&m) {
                    // We have every target — close the connection without
                    // draining the rest of the body. The trailing perf
                    // suite is exactly the per-peer noise we don't need.
                    early_exit = true;
                    break 'outer;
                }
            }
            // Drop the parsed portion; only keep the partial trailing line.
            let tail = tail.to_string();
            leftover = tail;
        }
    }
    // Parse any final partial line that arrived without a trailing newline.
    if !leftover.is_empty() {
        parse_line(&leftover, &mut m);
    }

    m.scrape_duration_ms = Some(started.elapsed().as_millis() as u64);
    tracing::debug!(
        bytes_read,
        early_exit,
        duration_ms = m.scrape_duration_ms,
        "zebra metrics scrape complete"
    );
    m
}

/// True when every target metric in [`ZebraMetrics`] (apart from scrape
/// meta) has been populated. Triggers the chunked reader's early exit.
///
/// `verified_blocks_total` and `mempool_bytes` are excluded because zebra
/// 4.5.0 stopped emitting them; requiring them would prevent early-exit
/// on every scrape against modern zebra and force us to read the full body.
fn all_targets_filled(m: &ZebraMetrics) -> bool {
    m.verified_height.is_some()
        && m.network_tip_height.is_some()
        && m.distance_to_tip.is_some()
        && m.rpc_active_requests.is_some()
        && m.gbt_requests_total.is_some()
        && m.gbt_errors_total.is_some()
        && m.gossip_queued_blocks.is_some()
        && m.peers.is_some()
        && m.mempool_txs.is_some()
        && m.zebra_version.is_some()
}

fn failed(msg: String, started: Instant) -> ZebraMetrics {
    ZebraMetrics {
        scrape_error: Some(msg),
        scrape_duration_ms: Some(started.elapsed().as_millis() as u64),
        ..ZebraMetrics::default()
    }
}

/// Parse the curated subset of metric lines we surface on the dashboard.
/// Lines we don't recognise are ignored — zebra adds metrics across
/// versions and we don't want a new line breaking the scrape.
pub fn parse_zebra_metrics(body: &str) -> ZebraMetrics {
    let mut m = ZebraMetrics::default();
    for line in body.lines() {
        parse_line(line, &mut m);
    }
    m
}

/// Parse a single metric line and merge its value into `m`. Used by both
/// the chunked streaming reader and the whole-body parser.
///
/// Several metric names changed between zebra 4.4.x and 4.5.0 (the
/// `zcash_chain_*` family was retired in favour of `state_memory_*`).
/// We accept both names per field so the dashboard keeps working across
/// the version transition without an operator-visible regression.
fn parse_line(line: &str, m: &mut ZebraMetrics) {
    if line.is_empty() || line.starts_with('#') {
        return;
    }

    // Simple (unlabeled) gauges / counters.
    //
    // Verified tip — pre-4.5.0 used `zcash_chain_verified_block_height`;
    // 4.5.0 exposes the equivalent as `state_memory_best_committed_block_height`.
    // Both are accepted; whichever zebra emits first wins.
    if m.verified_height.is_none() {
        if let Some(v) = unlabeled_u64(line, "zcash_chain_verified_block_height") {
            m.verified_height = Some(v);
            return;
        }
        if let Some(v) = unlabeled_u64(line, "state_memory_best_committed_block_height") {
            m.verified_height = Some(v);
            return;
        }
    }
    if let Some(v) = unlabeled_u64(line, "sync_estimated_network_tip_height") {
        m.network_tip_height = Some(v);
    } else if let Some(v) = unlabeled_u64(line, "sync_estimated_distance_to_tip") {
        m.distance_to_tip = Some(v);
    } else if let Some(v) = unlabeled_u64(line, "rpc_active_requests") {
        m.rpc_active_requests = Some(v);
    } else if let Some(v) = unlabeled_u64(line, "zcash_chain_verified_block_total") {
        // 4.4.x only — 4.5.0 dropped this counter with no direct replacement.
        // Stays None on newer zebra; the JS row is guarded with a null check.
        m.verified_blocks_total = Some(v);
    } else if let Some(v) = unlabeled_u64(line, "gossip_queued_block_count") {
        m.gossip_queued_blocks = Some(v);
    } else if let Some(v) = unlabeled_u64(line, "zcash_net_peers") {
        m.peers = Some(v);
    } else if let Some(v) = unlabeled_u64(line, "zcash_mempool_size_transactions") {
        m.mempool_txs = Some(v);
    } else if let Some(v) = unlabeled_u64(line, "mempool_queued_transactions_total") {
        // 4.5.0 replacement: name says "_total" but the value is the
        // current queued-tx count, not cumulative. Treat as a gauge.
        m.mempool_txs = Some(v);
    } else if let Some(v) = unlabeled_u64(line, "zcash_mempool_cost_bytes") {
        m.mempool_bytes = Some(v);
    }
    // Labelled counters: sum across label combinations for the metric
    // family we care about.
    else if line.starts_with("rpc_requests_total{") {
        if extract_label(line, "method").as_deref() == Some("getblocktemplate") {
            if let Some(v) = trailing_u64(line) {
                m.gbt_requests_total = Some(m.gbt_requests_total.unwrap_or(0) + v);
            }
        }
    } else if line.starts_with("rpc_errors_total{") {
        if extract_label(line, "method").as_deref() == Some("getblocktemplate") {
            if let Some(v) = trailing_u64(line) {
                m.gbt_errors_total = Some(m.gbt_errors_total.unwrap_or(0) + v);
            }
        }
    } else if line.starts_with("zebrad_build_info{") || line.starts_with("zakura_build_info{") {
        // zakura renamed zebrad_build_info -> zakura_build_info; without this
        // alias `all_targets_filled` could never fire against zakura and every
        // scrape read the full body (audit #20).
        if let Some(ver) = extract_label(line, "version") {
            m.zebra_version = Some(ver.to_string());
        }
    }
}

/// Match a line like `metric_name 12345` and parse the trailing number.
fn unlabeled_u64(line: &str, name: &str) -> Option<u64> {
    let rest = line.strip_prefix(name)?;
    // After the name there must be a space (no `{` — that means labelled).
    let rest = rest.strip_prefix(' ')?;
    rest.split_whitespace().next()?.parse::<u64>().ok()
}

/// Read the trailing whitespace-separated value as u64.
fn trailing_u64(line: &str) -> Option<u64> {
    let (_, val) = line.rsplit_once(' ')?;
    val.parse::<u64>().ok()
}

/// Read a single label value out of a Prometheus metric line. Returns the
/// content between the quotes, without the surrounding `"`.
fn extract_label<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let labels_start = line.find('{')?;
    let labels_end = line.find('}')?;
    if labels_end <= labels_start {
        return None;
    }
    let labels = &line[labels_start + 1..labels_end];
    for pair in labels.split(',') {
        let pair = pair.trim();
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return Some(v.trim_matches('"'));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_url_swaps_default_port() {
        assert_eq!(
            derive_metrics_url("http://operational-host.invalid:8232"),
            "http://operational-host.invalid:9999/metrics"
        );
        assert_eq!(
            derive_metrics_url("http://localhost:8232/"),
            "http://localhost:9999/metrics"
        );
    }

    #[test]
    fn derive_url_appends_metrics_port_when_no_rpc_port() {
        assert_eq!(
            derive_metrics_url("http://node.example.com"),
            "http://node.example.com:9999/metrics"
        );
    }

    #[test]
    fn derive_url_replaces_nonstandard_port_no_double_port() {
        // Audit #20 regression: testnet's rpc_url has :18232; the old
        // string-append fallback produced "http://…:18232:9999/metrics",
        // which reqwest rejected as an opaque "builder error".
        assert_eq!(
            derive_metrics_url("http://operational-host.invalid:18232"),
            "http://operational-host.invalid:9999/metrics"
        );
    }

    #[test]
    fn parses_zakura_build_info_alias() {
        let m = parse_zebra_metrics("zakura_build_info{version=\"1.1.0\"} 1\n");
        assert_eq!(m.zebra_version.as_deref(), Some("1.1.0"));
    }

    #[test]
    fn parses_curated_metric_lines() {
        let body = r#"
# HELP sync_estimated_distance_to_tip
# TYPE sync_estimated_distance_to_tip gauge
sync_estimated_distance_to_tip 2
zcash_chain_verified_block_height 3357913
sync_estimated_network_tip_height 3357915
state_memory_best_committed_block_height 3357913
state_finalized_block_height 3357814
rpc_active_requests 4
zcash_chain_verified_block_total 32
gossip_queued_block_count 1
zcash_net_peers 132
zcash_mempool_size_transactions 8
zcash_mempool_cost_bytes 85624
rpc_requests_total{method="getblocktemplate",status="success"} 127
rpc_requests_total{method="getblocktemplate",status="error"} 54
rpc_requests_total{method="getblock",status="success"} 36340
rpc_errors_total{method="getblocktemplate",error_code="0"} 30
rpc_errors_total{method="getblocktemplate",error_code="-10"} 24
zebrad_build_info{version="4.3.1"} 1
"#;
        let m = parse_zebra_metrics(body);
        assert_eq!(m.distance_to_tip, Some(2));
        // verified_height pulls from `zcash_chain_verified_block_height`
        // (the 4.4.x name), which appears before the 4.5.0 fallback name.
        assert_eq!(m.verified_height, Some(3_357_913));
        assert_eq!(m.network_tip_height, Some(3_357_915));
        assert_eq!(m.rpc_active_requests, Some(4));
        assert_eq!(m.verified_blocks_total, Some(32));
        assert_eq!(m.gossip_queued_blocks, Some(1));
        assert_eq!(m.peers, Some(132));
        assert_eq!(m.mempool_txs, Some(8));
        assert_eq!(m.mempool_bytes, Some(85_624));
        // GBT request total sums success+error for that method, ignores other methods.
        assert_eq!(m.gbt_requests_total, Some(127 + 54));
        // GBT errors sum across error_code labels.
        assert_eq!(m.gbt_errors_total, Some(30 + 24));
        assert_eq!(m.zebra_version.as_deref(), Some("4.3.1"));
    }

    #[test]
    fn parse_ignores_unknown_metrics_and_returns_partial_on_short_input() {
        let body = "zcash_net_peers 7\nsomething_else_total 99\n";
        let m = parse_zebra_metrics(body);
        assert_eq!(m.peers, Some(7));
        assert!(m.verified_height.is_none());
        assert!(m.gbt_errors_total.is_none());
    }

    #[test]
    fn parses_4_5_0_metric_names() {
        // Zebra 4.5.0 retired several `zcash_*` metric names; we accept the
        // new `state_memory_*` / `mempool_*` names per field so the
        // dashboard keeps populating across the version transition.
        let body = "\
state_memory_best_committed_block_height 3360561
sync_estimated_network_tip_height 3360561
sync_estimated_distance_to_tip 0
rpc_active_requests 0
gossip_queued_block_count 0
zcash_net_peers 138
mempool_queued_transactions_total 186
rpc_requests_total{method=\"getblocktemplate\",status=\"success\"} 1
zebrad_build_info{version=\"4.5.0\"} 1
";
        let m = parse_zebra_metrics(body);
        assert_eq!(m.verified_height, Some(3_360_561));
        assert_eq!(m.network_tip_height, Some(3_360_561));
        assert_eq!(m.distance_to_tip, Some(0));
        assert_eq!(m.peers, Some(138));
        assert_eq!(m.mempool_txs, Some(186));
        assert_eq!(m.gbt_requests_total, Some(1));
        assert_eq!(m.zebra_version.as_deref(), Some("4.5.0"));
        // Fields the new zebra doesn't expose stay None.
        assert!(m.verified_blocks_total.is_none());
        assert!(m.mempool_bytes.is_none());
    }

    #[test]
    fn extract_label_picks_correct_value() {
        let line = r#"rpc_errors_total{method="getblocktemplate",error_code="-10"} 24"#;
        assert_eq!(extract_label(line, "method"), Some("getblocktemplate"));
        assert_eq!(extract_label(line, "error_code"), Some("-10"));
        assert_eq!(extract_label(line, "nope"), None);
    }
}
