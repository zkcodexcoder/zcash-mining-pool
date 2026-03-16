use axum::extract::{Query, State};
use axum::response::{Html, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::task::JoinSet;

use crate::handlers::AppState;

/// Number of blocks to fetch concurrently per batch.
const CONCURRENCY: usize = 25;

#[derive(Deserialize)]
pub struct NetworkQuery {
    #[serde(default = "default_range")]
    pub range: String,
}

fn default_range() -> String {
    "1h".to_string()
}

/// Validate and normalize the range parameter.
fn normalize_range(raw: &str) -> &'static str {
    match raw {
        "24h" => "24h",
        "1w" => "1w",
        _ => "1h",
    }
}

/// (range_seconds, max_blocks_to_scan)
fn range_params(range: &str) -> (i64, u64) {
    match range {
        "24h" => (24 * 3600, 1500),
        "1w" => (7 * 24 * 3600, 3000),
        _ => (3600, 100),
    }
}

/// Per-range cache TTL in milliseconds.
fn cache_ttl_ms(range: &str) -> i64 {
    match range {
        "24h" => 120_000,
        "1w" => 300_000,
        _ => 60_000,
    }
}

#[derive(Clone, Serialize)]
pub struct NetworkBlock {
    pub height: u64,
    pub hash: String,
    pub time: i64,
    pub miner_address: String,
    pub miner_label: String,
    pub reward_zec: f64,
    pub is_our_pool: bool,
    pub coinbase_text: String,
}

#[derive(Clone, Serialize)]
pub struct MinerDistribution {
    pub label: String,
    pub address: String,
    pub block_count: u64,
    pub percent: f64,
    pub is_our_pool: bool,
}

#[derive(Clone, Serialize)]
pub struct NetworkMiningStats {
    pub blocks: Vec<NetworkBlock>,
    pub distribution: Vec<MinerDistribution>,
    pub total_blocks: u64,
    pub our_pool_blocks: u64,
    pub our_pool_percent: f64,
    pub unique_miners: u64,
}

fn hex_to_ascii_lossy(hex: &str) -> String {
    hex::decode(hex)
        .unwrap_or_default()
        .iter()
        .map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' })
        .collect()
}

fn truncate_address(addr: &str) -> String {
    if addr.len() > 16 {
        format!("{}...{}", &addr[..8], &addr[addr.len() - 6..])
    } else {
        addr.to_string()
    }
}

/// Extract coinbase info from a block JSON (verbosity=2, full tx objects inline).
fn extract_coinbase_from_block(block_data: &serde_json::Value) -> (String, f64, String) {
    let unknown = ("unknown".to_string(), 0.0, String::new());

    let tx_array = match block_data.get("tx").and_then(|v| v.as_array()) {
        Some(arr) if !arr.is_empty() => arr,
        _ => return unknown,
    };

    let coinbase_tx = &tx_array[0];

    // With verbosity=2, tx should be a full object. If it's a txid string, we
    // can't extract info without another RPC call — just mark as unknown.
    if coinbase_tx.is_string() {
        return unknown;
    }

    // Extract coinbase text from vin[0].coinbase
    let coinbase_text = coinbase_tx
        .get("vin")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|vin| vin.get("coinbase"))
        .and_then(|v| v.as_str())
        .map(hex_to_ascii_lossy)
        .unwrap_or_default();

    // Extract miner address and reward from vout[0]
    let vout = match coinbase_tx.get("vout").and_then(|v| v.as_array()) {
        Some(arr) if !arr.is_empty() => arr,
        _ => return ("unknown".to_string(), 0.0, coinbase_text),
    };

    let first_vout = &vout[0];

    let reward_zec = first_vout
        .get("valueZat")
        .and_then(|v| v.as_i64())
        .map(|z| z as f64 / 100_000_000.0)
        .or_else(|| first_vout.get("value").and_then(|v| v.as_f64()))
        .unwrap_or(0.0);

    let miner_address = first_vout
        .get("scriptPubKey")
        .and_then(|spk| {
            spk.get("addresses")
                .and_then(|a| a.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| {
                    spk.get("address")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                })
        })
        .unwrap_or_else(|| "unknown".to_string());

    (miner_address, reward_zec, coinbase_text)
}

async fn fetch_network_blocks(state: &AppState, range: &str) -> Result<NetworkMiningStats, String> {
    let tip = state.rpc.get_block_count().await.map_err(|e| format!("getblockcount: {e}"))?;

    let (range_secs, max_blocks) = range_params(range);
    let cutoff_time = chrono::Utc::now().timestamp() - range_secs;

    // Estimate how many blocks to scan with 20% buffer for block time variance.
    let estimated_blocks = ((range_secs as f64 / 75.0) * 1.2) as u64;
    let scan_count = estimated_blocks.min(max_blocks);
    let start_height = (tip).saturating_sub(scan_count).max(1);

    let our_mining_address = state.mining_address.clone().unwrap_or_default();

    // Collect heights to fetch (newest first).
    let heights: Vec<u64> = (start_height..=tip).rev().collect();

    // Fetch blocks in parallel batches.
    let mut raw_blocks: Vec<(u64, serde_json::Value)> = Vec::with_capacity(heights.len());

    for chunk in heights.chunks(CONCURRENCY) {
        let mut set = JoinSet::new();
        for &h in chunk {
            let rpc = Arc::clone(&state.rpc);
            set.spawn(async move {
                // get_block_hash then get_block(hash, 2) for full tx objects
                let hash = match rpc.get_block_hash(h).await {
                    Ok(hash) => hash,
                    Err(_) => return None,
                };
                match rpc.get_block(&hash, 2).await {
                    Ok(data) => Some((h, data)),
                    Err(_) => {
                        // Fallback to verbosity=1
                        match rpc.get_block(&hash, 1).await {
                            Ok(data) => Some((h, data)),
                            Err(_) => None,
                        }
                    }
                }
            });
        }
        while let Some(res) = set.join_next().await {
            if let Ok(Some(pair)) = res {
                raw_blocks.push(pair);
            }
        }
    }

    // Sort by height descending.
    raw_blocks.sort_by(|a, b| b.0.cmp(&a.0));

    // Extract block info, filtering by cutoff time.
    let mut blocks = Vec::with_capacity(raw_blocks.len());
    for (height, block_data) in &raw_blocks {
        let time = block_data.get("time").and_then(|v| v.as_i64()).unwrap_or(0);
        if time < cutoff_time {
            continue;
        }

        let hash = block_data
            .get("hash")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let (miner_address, reward_zec, coinbase_text) = extract_coinbase_from_block(block_data);

        let is_our_pool = !our_mining_address.is_empty() && miner_address == our_mining_address;
        let miner_label = if is_our_pool {
            "Our Pool".to_string()
        } else {
            truncate_address(&miner_address)
        };

        blocks.push(NetworkBlock {
            height: *height,
            hash,
            time,
            miner_address,
            miner_label,
            reward_zec,
            is_our_pool,
            coinbase_text,
        });
    }

    // Build distribution.
    let mut addr_counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for b in &blocks {
        *addr_counts.entry(b.miner_address.clone()).or_insert(0) += 1;
    }

    let total = blocks.len() as f64;
    let mut distribution: Vec<MinerDistribution> = addr_counts
        .into_iter()
        .map(|(addr, count)| {
            let is_our_pool = !our_mining_address.is_empty() && addr == our_mining_address;
            MinerDistribution {
                label: if is_our_pool {
                    "Our Pool".to_string()
                } else {
                    truncate_address(&addr)
                },
                address: addr,
                block_count: count,
                percent: if total > 0.0 {
                    (count as f64 / total) * 100.0
                } else {
                    0.0
                },
                is_our_pool,
            }
        })
        .collect();
    distribution.sort_by(|a, b| b.block_count.cmp(&a.block_count));

    let our_pool_blocks = blocks.iter().filter(|b| b.is_our_pool).count() as u64;
    let our_pool_percent = if total > 0.0 {
        (our_pool_blocks as f64 / total) * 100.0
    } else {
        0.0
    };

    Ok(NetworkMiningStats {
        total_blocks: blocks.len() as u64,
        our_pool_blocks,
        our_pool_percent,
        unique_miners: distribution.len() as u64,
        blocks,
        distribution,
    })
}

fn empty_stats() -> NetworkMiningStats {
    NetworkMiningStats {
        blocks: vec![],
        distribution: vec![],
        total_blocks: 0,
        our_pool_blocks: 0,
        our_pool_percent: 0.0,
        unique_miners: 0,
    }
}

pub async fn get_network_blocks(
    State(state): State<AppState>,
    Query(query): Query<NetworkQuery>,
) -> Json<NetworkMiningStats> {
    let range = normalize_range(&query.range);
    let ttl = cache_ttl_ms(range);

    // Check per-range cache.
    {
        let cache = state.network_blocks_cache.read().await;
        if let Some((ref data, ts)) = cache.get(range) {
            let now = chrono::Utc::now().timestamp_millis();
            if now - ts < ttl {
                return Json(data.clone());
            }
        }
    }

    // Cache miss — fetch fresh data.
    let stats = match fetch_network_blocks(&state, range).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, range = range, "Failed to fetch network blocks");
            empty_stats()
        }
    };

    // Update cache.
    {
        let mut cache = state.network_blocks_cache.write().await;
        cache.insert(range.to_string(), (stats.clone(), chrono::Utc::now().timestamp_millis()));
    }

    Json(stats)
}

/// Pre-fetch and cache network block data for the given ranges.
/// Called from a background task to keep the cache warm so the /network page
/// loads instantly instead of waiting for RPC calls.
pub async fn warm_cache(state: &AppState, ranges: &[&str]) {
    for &range in ranges {
        let range = normalize_range(range);
        let ttl = cache_ttl_ms(range);

        // Skip if cache is still fresh.
        {
            let cache = state.network_blocks_cache.read().await;
            if let Some((_, ts)) = cache.get(range) {
                let now = chrono::Utc::now().timestamp_millis();
                if now - *ts < ttl {
                    continue;
                }
            }
        }

        match fetch_network_blocks(state, range).await {
            Ok(stats) => {
                let mut cache = state.network_blocks_cache.write().await;
                cache.insert(range.to_string(), (stats, chrono::Utc::now().timestamp_millis()));
            }
            Err(e) => {
                tracing::warn!(error = %e, range = range, "Network cache warm failed");
            }
        }
    }
}

pub async fn network_page() -> Html<String> {
    Html(NETWORK_HTML.to_string())
}

const NETWORK_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Network Miners - Zcash Mining Pool</title>
    <script src="https://cdn.jsdelivr.net/npm/chart.js@4/dist/chart.umd.min.js"></script>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: #0b0b0b;
            color: #c8c8c8;
            min-height: 100vh;
        }

        .header {
            background: #111;
            border-bottom: 1px solid #222;
            padding: 0.6rem 1.5rem;
            display: flex;
            align-items: center;
            gap: 0.75rem;
        }
        .header h1 {
            font-family: 'JetBrains Mono', 'Fira Code', 'Courier New', monospace;
            color: #f4b728;
            font-size: 1rem;
            font-weight: 700;
            letter-spacing: 0.04em;
        }
        .badge {
            background: #1a1a1a;
            border: 1px solid #333;
            padding: 0.15rem 0.5rem;
            font-size: 0.6rem;
            color: #666;
            text-transform: uppercase;
            letter-spacing: 0.08em;
        }
        .header-right {
            margin-left: auto;
            display: flex;
            align-items: center;
            gap: 1rem;
        }
        .header-link {
            color: #555;
            font-size: 0.7rem;
            text-decoration: none;
            text-transform: uppercase;
            letter-spacing: 0.06em;
        }
        .header-link:hover { color: #999; }
        .header-link.active { color: #f4b728; }

        .container { max-width: 1400px; margin: 0 auto; padding: 1rem 1.5rem; }

        /* ── Range Selector ── */
        .range-bar {
            display: flex;
            align-items: center;
            gap: 0;
            margin-bottom: 1rem;
            border: 1px solid #222;
            background: #222;
        }
        .range-btn {
            padding: 0.5rem 1.25rem;
            background: #111;
            border: none;
            color: #555;
            font-family: 'JetBrains Mono', 'Fira Code', 'Courier New', monospace;
            font-size: 0.7rem;
            font-weight: 600;
            text-transform: uppercase;
            letter-spacing: 0.08em;
            cursor: pointer;
            transition: color 0.15s, background 0.15s;
        }
        .range-btn:hover { color: #999; background: #151515; }
        .range-btn.active { color: #f4b728; background: #1a1a1a; }
        .range-status {
            margin-left: auto;
            padding: 0 1rem;
            font-size: 0.6rem;
            color: #444;
            background: #111;
            height: 100%;
            display: flex;
            align-items: center;
        }

        .summary-grid {
            display: grid;
            grid-template-columns: repeat(4, 1fr);
            gap: 1px;
            background: #222;
            border: 1px solid #222;
            margin-bottom: 1rem;
        }
        .summary-cell {
            background: #111;
            padding: 0.75rem 1rem;
        }
        .summary-cell .label {
            font-size: 0.6rem;
            text-transform: uppercase;
            letter-spacing: 0.1em;
            color: #555;
            margin-bottom: 0.25rem;
        }
        .summary-cell .value {
            font-family: 'JetBrains Mono', 'Fira Code', 'Courier New', monospace;
            font-size: 1.6rem;
            font-weight: 700;
            color: #f4b728;
        }

        .content-row {
            display: grid;
            grid-template-columns: 380px 1fr;
            gap: 1px;
            background: #222;
            border: 1px solid #222;
            margin-bottom: 1rem;
        }
        .chart-panel {
            background: #111;
            padding: 1rem;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
        }
        .chart-panel canvas {
            max-width: 320px !important;
            max-height: 320px !important;
        }
        .dist-panel {
            background: #111;
            padding: 0.75rem 1rem;
            overflow-y: auto;
            max-height: 400px;
        }

        .section-title {
            font-size: 0.6rem;
            text-transform: uppercase;
            letter-spacing: 0.1em;
            color: #555;
            padding: 0.5rem 0;
        }

        .table-wrap {
            border: 1px solid #222;
            margin-bottom: 1rem;
            overflow-x: auto;
        }
        table {
            width: 100%;
            border-collapse: collapse;
            background: #111;
        }
        th {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            font-size: 0.55rem;
            text-transform: uppercase;
            letter-spacing: 0.1em;
            color: #444;
            padding: 0.5rem 0.75rem;
            text-align: left;
            background: #0e0e0e;
            border-bottom: 1px solid #222;
        }
        td {
            font-family: 'JetBrains Mono', 'Fira Code', 'Courier New', monospace;
            font-size: 0.75rem;
            padding: 0.4rem 0.75rem;
            border-bottom: 1px solid #1a1a1a;
            color: #999;
        }
        tr:hover td { background: #151515; }
        tr.our-pool td { color: #f4b728; }

        .loading { color: #333; font-style: italic; font-family: inherit; }
        .last-updated {
            font-size: 0.6rem;
            color: #444;
            text-align: right;
            padding: 0.25rem 0;
        }

        @keyframes pulse-load {
            0%, 100% { opacity: 1; }
            50% { opacity: 0.4; }
        }
        .loading-indicator {
            color: #f4b728;
            font-size: 0.6rem;
            animation: pulse-load 1.5s ease-in-out infinite;
        }

        @media (max-width: 900px) {
            .summary-grid { grid-template-columns: repeat(2, 1fr); }
            .content-row { grid-template-columns: 1fr; }
        }
        @media (max-width: 600px) {
            .summary-grid { grid-template-columns: 1fr; }
        }
    </style>
</head>
<body style="opacity:0;transition:opacity 0.15s">

<div class="header">
    <h1>NETWORK MINERS</h1>
    <span class="badge" id="network-badge">Testnet</span>
    <div class="header-right">
        <a href="/" class="header-link">Dashboard</a>
        <a href="/network" class="header-link active">Network</a>
        <a href="/zallet" class="header-link">Wallet</a>
    </div>
</div>

<div class="container">

    <!-- Range Selector -->
    <div class="range-bar">
        <button class="range-btn active" data-range="1h" onclick="setRange('1h')">1 Hour</button>
        <button class="range-btn" data-range="24h" onclick="setRange('24h')">24 Hours</button>
        <button class="range-btn" data-range="1w" onclick="setRange('1w')">1 Week</button>
        <div class="range-status" id="range-status"></div>
    </div>

    <div class="summary-grid">
        <div class="summary-cell">
            <div class="label">Blocks Scanned</div>
            <div class="value" id="stat-total">--</div>
        </div>
        <div class="summary-cell">
            <div class="label">Our Pool Blocks</div>
            <div class="value" id="stat-ours">--</div>
        </div>
        <div class="summary-cell">
            <div class="label">Our Pool Share</div>
            <div class="value" id="stat-share">--</div>
        </div>
        <div class="summary-cell">
            <div class="label">Unique Miners</div>
            <div class="value" id="stat-unique">--</div>
        </div>
    </div>

    <div class="content-row">
        <div class="chart-panel">
            <canvas id="dist-chart"></canvas>
        </div>
        <div class="dist-panel">
            <div class="section-title">Mining Distribution</div>
            <table id="dist-table">
                <thead>
                    <tr>
                        <th>Miner</th>
                        <th>Blocks</th>
                        <th>Share</th>
                    </tr>
                </thead>
                <tbody><tr><td colspan="3" class="loading">Loading...</td></tr></tbody>
            </table>
        </div>
    </div>

    <div class="section-title">Recent Network Blocks</div>
    <div class="table-wrap">
        <table id="blocks-table">
            <thead>
                <tr>
                    <th>Height</th>
                    <th>Hash</th>
                    <th>Miner</th>
                    <th>Reward</th>
                    <th>Coinbase</th>
                    <th>Time</th>
                </tr>
            </thead>
            <tbody><tr><td colspan="6" class="loading">Loading...</td></tr></tbody>
        </table>
    </div>

    <div class="last-updated" id="last-updated"></div>
</div>

<script>
let COIN = 'ZEC';
async function initCoin() {
    try {
        const r = await fetch('/api/pool/stats');
        const d = await r.json();
        COIN = d.network === 'mainnet' ? 'ZEC' : 'TAZ';
        const badge = document.getElementById('network-badge');
        if (badge) badge.textContent = d.network === 'mainnet' ? 'Mainnet' : 'Testnet';
    } catch(e) {}
}
const CHART_COLORS = [
    '#f4b728', '#4a9eff', '#48bb78', '#fc8181', '#a78bfa',
    '#f687b3', '#68d391', '#63b3ed', '#fbd38d', '#b794f4',
    '#76e4f7', '#fca5a5', '#86efac', '#c4b5fd', '#fdba74'
];

const REFRESH_MS = { '1h': 60000, '24h': 120000, '1w': 300000 };

let distChart = null;
let currentRange = '1h';
let refreshTimer = null;
let fetching = false;

function formatTime(ts) {
    const d = new Date(ts * 1000);
    return d.toLocaleString();
}

function setRange(range) {
    if (fetching) return;
    currentRange = range;
    document.querySelectorAll('.range-btn').forEach(b => b.classList.remove('active'));
    document.querySelector('[data-range="' + range + '"]').classList.add('active');

    // Reset auto-refresh interval for this range
    if (refreshTimer) clearInterval(refreshTimer);
    refreshTimer = setInterval(fetchData, REFRESH_MS[range] || 60000);

    fetchData();
}

async function fetchData() {
    if (fetching) return;
    fetching = true;
    const status = document.getElementById('range-status');
    status.innerHTML = '<span class="loading-indicator">Loading...</span>';

    try {
        const resp = await fetch('/api/network/blocks?range=' + currentRange);
        const data = await resp.json();

        document.getElementById('stat-total').textContent = data.total_blocks;
        document.getElementById('stat-ours').textContent = data.our_pool_blocks;
        document.getElementById('stat-share').textContent = data.our_pool_percent.toFixed(1) + '%';
        document.getElementById('stat-unique').textContent = data.unique_miners;

        // Distribution table
        const distBody = document.querySelector('#dist-table tbody');
        if (data.distribution.length === 0) {
            distBody.innerHTML = '<tr><td colspan="3" class="loading">No data</td></tr>';
        } else {
            distBody.innerHTML = data.distribution.map(d => {
                const cls = d.is_our_pool ? ' class="our-pool"' : '';
                return '<tr' + cls + '>' +
                    '<td title="' + d.address + '">' + d.label + '</td>' +
                    '<td>' + d.block_count + '</td>' +
                    '<td>' + d.percent.toFixed(1) + '%</td>' +
                    '</tr>';
            }).join('');
        }

        // Doughnut chart
        const labels = data.distribution.map(d => d.label);
        const counts = data.distribution.map(d => d.block_count);
        const colors = data.distribution.map((d, i) => {
            if (d.is_our_pool) return '#f4b728';
            return CHART_COLORS[(i) % CHART_COLORS.length];
        });

        if (distChart) {
            distChart.data.labels = labels;
            distChart.data.datasets[0].data = counts;
            distChart.data.datasets[0].backgroundColor = colors;
            distChart.update();
        } else {
            distChart = new Chart(document.getElementById('dist-chart'), {
                type: 'doughnut',
                data: {
                    labels: labels,
                    datasets: [{
                        data: counts,
                        backgroundColor: colors,
                        borderColor: '#0b0b0b',
                        borderWidth: 2
                    }]
                },
                options: {
                    responsive: true,
                    maintainAspectRatio: true,
                    plugins: {
                        legend: { display: false },
                        tooltip: {
                            backgroundColor: '#1a1a1a',
                            titleColor: '#888',
                            bodyColor: '#ccc',
                            borderColor: '#333',
                            borderWidth: 1,
                            bodyFont: { size: 11, family: "'JetBrains Mono', monospace" },
                            callbacks: {
                                label: function(ctx) {
                                    return ctx.label + ': ' + ctx.raw + ' blocks (' + data.distribution[ctx.dataIndex].percent.toFixed(1) + '%)';
                                }
                            }
                        }
                    },
                    cutout: '55%'
                }
            });
        }

        // Blocks table
        const blocksBody = document.querySelector('#blocks-table tbody');
        if (data.blocks.length === 0) {
            blocksBody.innerHTML = '<tr><td colspan="6" class="loading">No blocks</td></tr>';
        } else {
            blocksBody.innerHTML = data.blocks.map(b => {
                const cls = b.is_our_pool ? ' class="our-pool"' : '';
                const hashShort = b.hash.substring(0, 16) + '...';
                const cbShort = b.coinbase_text.length > 40 ? b.coinbase_text.substring(0, 40) + '...' : b.coinbase_text;
                return '<tr' + cls + '>' +
                    '<td style="color:#e0e0e0">' + b.height + '</td>' +
                    '<td title="' + b.hash + '">' + hashShort + '</td>' +
                    '<td title="' + b.miner_address + '">' + b.miner_label + '</td>' +
                    '<td>' + b.reward_zec.toFixed(4) + ' ' + COIN + '</td>' +
                    '<td title="' + b.coinbase_text.replace(/"/g, '&quot;') + '">' + cbShort + '</td>' +
                    '<td>' + formatTime(b.time) + '</td>' +
                    '</tr>';
            }).join('');
        }

        status.textContent = data.total_blocks + ' blocks \u00b7 Updated ' + new Date().toLocaleTimeString();
    } catch (e) {
        console.error('Failed to fetch network data:', e);
        status.textContent = 'Error loading data';
    } finally {
        fetching = false;
    }
}

document.addEventListener('DOMContentLoaded', async () => {
    await initCoin();
    document.body.style.opacity = '1';
    fetchData();
    refreshTimer = setInterval(fetchData, REFRESH_MS[currentRange]);
});
</script>
</body>
</html>
"##;
