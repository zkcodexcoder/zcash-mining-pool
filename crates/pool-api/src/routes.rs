use axum::extract::State;
use axum::response::{Html, Json};
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;

use crate::diagnostics;
use crate::handlers::*;
use crate::network;
use crate::previews;

/// Build the full API router.
pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/api/stats", get(get_pool_stats_nomp))
        .route("/api/pool/stats", get(get_pool_stats))
        .route("/api/miners", get(get_miners))
        .route("/api/miner/{address}", get(get_miner_stats))
        .route("/api/blocks", get(get_blocks))
        .route("/api/payouts", get(get_payouts))
        .route("/api/zallet/status", get(get_zallet_status))
        .route("/api/blocks/immature", get(get_immature_blocks))
        .route("/api/payout/trigger", post(trigger_payout))
        .route("/api/pool/stats/history", get(get_stats_history))
        .route("/api/network/blocks", get(network::get_network_blocks))
        .route("/health", get(get_health))
        .route("/api/miner/{address}/diagnostics", get(diagnostics::get_miner_diagnostics))
        .route("/api/pool/info", get(get_pool_info));

    Router::new()
        .merge(api)
        .route("/", get(dashboard))
        .route("/miner/{address}", get(diagnostics::miner_page))
        .route("/network", get(network::network_page))
        .route("/zallet", get(zallet_dashboard))
        .route("/preview1", get(previews::preview1))
        .route("/preview2", get(previews::preview2))
        .route("/preview3", get(previews::preview3))
        .route("/preview4", get(previews::preview4))
        .route("/preview5", get(previews::preview5))
        .route("/preview6", get(previews::preview6))
        .route("/preview7", get(previews::preview7))
        .route("/preview8", get(previews::preview8))
        .route("/preview9", get(previews::preview9))
        .route("/preview10", get(previews::preview10))
        .route("/preview11", get(previews::preview11))
        .route("/preview12", get(previews::preview12))
        .route("/preview13", get(previews::preview13))
        .route("/preview14", get(previews::preview14))
        .route("/preview15", get(previews::preview15))
        .route("/preview16", get(previews::preview16))
        .route("/preview17", get(previews::preview17))
        .route("/preview18", get(previews::preview18))
        .route("/preview19", get(previews::preview19))
        .route("/preview20", get(previews::preview20))
        .route("/preview21", get(previews::preview21))
        .route("/preview22", get(previews::preview22))
        .route("/preview23", get(previews::preview23))
        .route("/preview24", get(previews::preview24))
        .route("/preview25", get(previews::preview25))
        .route("/previews", get(previews::gallery))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Lightweight endpoint returning just network/coin info (no RPC calls).
async fn get_pool_info(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let coin = if state.network == "mainnet" { "ZEC" } else { "TAZ" };
    Json(serde_json::json!({
        "network": state.network,
        "coin": coin,
        "pool_name": state.pool_name,
    }))
}

async fn dashboard() -> Html<String> {
    Html(DASHBOARD_HTML.to_string())
}

const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Zcash Mining Pool</title>
    <script src="https://cdn.jsdelivr.net/npm/chart.js@4/dist/chart.umd.min.js"></script>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: #0b0b0b;
            color: #c8c8c8;
            min-height: 100vh;
        }

        /* ── Header ── */
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
        .status-dot {
            width: 8px;
            height: 8px;
            border-radius: 50%;
            background: #48bb78;
            display: inline-block;
        }
        .status-dot.offline { background: #fc8181; }
        @keyframes pulse {
            0%, 100% { opacity: 1; }
            50% { opacity: 0.4; }
        }
        .status-dot.pulsing { animation: pulse 2s ease-in-out infinite; }
        .header-link {
            color: #555;
            font-size: 0.7rem;
            text-decoration: none;
            text-transform: uppercase;
            letter-spacing: 0.06em;
        }
        .header-link:hover { color: #999; }

        /* ── Container ── */
        .container { max-width: 1400px; margin: 0 auto; padding: 1rem 1.5rem; }

        /* ── Hero Metrics ── */
        .hero-grid {
            display: grid;
            grid-template-columns: repeat(4, 1fr);
            gap: 1px;
            background: #222;
            border: 1px solid #222;
            margin-bottom: 1rem;
        }
        .hero-card {
            background: #111;
            padding: 0.75rem 1rem;
            display: flex;
            flex-direction: column;
            min-height: 90px;
        }
        .hero-card .label {
            font-size: 0.6rem;
            text-transform: uppercase;
            letter-spacing: 0.1em;
            color: #555;
            margin-bottom: 0.25rem;
        }
        .hero-card .value-row {
            display: flex;
            align-items: baseline;
            gap: 0.5rem;
        }
        .hero-card .value {
            font-family: 'JetBrains Mono', 'Fira Code', 'Courier New', monospace;
            font-size: 1.6rem;
            font-weight: 700;
            color: #f4b728;
            transition: color 0.3s ease;
        }
        .hero-card .sub {
            font-family: 'JetBrains Mono', 'Fira Code', 'Courier New', monospace;
            font-size: 0.65rem;
            color: #555;
        }
        .hero-card .sparkline-wrap {
            flex: 1;
            display: flex;
            align-items: flex-end;
            margin-top: 0.25rem;
        }
        .hero-card .sparkline-wrap canvas {
            width: 100% !important;
            height: 32px !important;
        }

        /* ── Secondary Metrics ── */
        .metrics-grid {
            display: grid;
            grid-template-columns: repeat(7, 1fr);
            gap: 1px;
            background: #222;
            border: 1px solid #222;
            margin-bottom: 1rem;
        }
        .metric-cell {
            background: #111;
            padding: 0.6rem 0.75rem;
        }
        .metric-cell .label {
            font-size: 0.55rem;
            text-transform: uppercase;
            letter-spacing: 0.1em;
            color: #555;
            margin-bottom: 0.15rem;
        }
        .metric-cell .value {
            font-family: 'JetBrains Mono', 'Fira Code', 'Courier New', monospace;
            font-size: 1rem;
            font-weight: 600;
            color: #e0e0e0;
        }

        /* ── Chart Panels ── */
        .chart-row {
            display: grid;
            grid-template-columns: 1fr 1fr;
            gap: 1px;
            background: #222;
            border: 1px solid #222;
            margin-bottom: 1rem;
        }
        .chart-panel {
            background: #111;
            padding: 0.75rem 1rem;
        }
        .chart-panel .panel-header {
            display: flex;
            align-items: center;
            justify-content: space-between;
            margin-bottom: 0.5rem;
        }
        .chart-panel .panel-title {
            font-size: 0.6rem;
            text-transform: uppercase;
            letter-spacing: 0.1em;
            color: #555;
        }
        .chart-panel .panel-legend {
            display: flex;
            gap: 0.75rem;
        }
        .chart-panel .legend-item {
            font-size: 0.55rem;
            color: #555;
            display: flex;
            align-items: center;
            gap: 0.25rem;
        }
        .legend-swatch {
            width: 10px;
            height: 3px;
            display: inline-block;
        }
        .chart-panel canvas {
            width: 100% !important;
            height: 120px !important;
        }

        /* ── Section Headers ── */
        .section-title {
            font-size: 0.6rem;
            text-transform: uppercase;
            letter-spacing: 0.1em;
            color: #555;
            padding: 0.5rem 0;
            margin-bottom: 0;
        }

        /* ── Tables ── */
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
        .addr-cell { max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
        .addr-link { color: #e0e0e0; cursor: pointer; text-decoration: none; }
        .addr-link:hover { color: #f4b728; text-decoration: underline; }
        .status-confirmed { color: #48bb78; }
        .status-pending { color: #ecc94b; }
        .status-orphaned { color: #fc8181; }

        /* ── Miner Lookup ── */
        .lookup-row {
            display: flex;
            gap: 1px;
            background: #222;
            border: 1px solid #222;
            margin-bottom: 1rem;
        }
        .lookup-row input {
            flex: 1;
            padding: 0.5rem 0.75rem;
            background: #111;
            border: none;
            color: #c8c8c8;
            font-family: 'JetBrains Mono', 'Fira Code', 'Courier New', monospace;
            font-size: 0.75rem;
            outline: none;
        }
        .lookup-row input::placeholder { color: #333; }
        .lookup-row button {
            padding: 0.5rem 1rem;
            background: #1a1a1a;
            border: none;
            color: #666;
            font-size: 0.65rem;
            text-transform: uppercase;
            letter-spacing: 0.06em;
            cursor: pointer;
        }
        .lookup-row button:hover { color: #f4b728; background: #222; }
        #miner-info { display: none; }
        .miner-stats-row {
            display: grid;
            grid-template-columns: repeat(3, 1fr);
            gap: 1px;
            background: #222;
            border: 1px solid #222;
            margin-bottom: 1rem;
        }

        /* ── Status Footer ── */
        .status-footer {
            background: #0e0e0e;
            border-top: 1px solid #1a1a1a;
            padding: 0.4rem 1.5rem;
            display: flex;
            align-items: center;
            gap: 1.5rem;
            font-size: 0.6rem;
            color: #444;
            position: fixed;
            bottom: 0;
            left: 0;
            right: 0;
        }
        .status-footer .sf-item {
            display: flex;
            align-items: center;
            gap: 0.35rem;
        }
        .sf-dot {
            width: 6px;
            height: 6px;
            border-radius: 50%;
            display: inline-block;
        }
        .sf-dot.ok { background: #48bb78; }
        .sf-dot.err { background: #fc8181; }
        .footer-spacer { height: 2rem; }

        /* ── Flash Animation ── */
        @keyframes flash {
            0% { color: #fff; }
            100% { color: inherit; }
        }
        .flash { animation: flash 0.5s ease-out; }

        /* ── Responsive ── */
        @media (max-width: 900px) {
            .hero-grid { grid-template-columns: repeat(2, 1fr); }
            .metrics-grid { grid-template-columns: repeat(3, 1fr); }
            .chart-row { grid-template-columns: 1fr; }
        }
        @media (max-width: 600px) {
            .hero-grid { grid-template-columns: 1fr; }
            .metrics-grid { grid-template-columns: repeat(2, 1fr); }
        }
        /* ── Stratum Config ── */
        .stratum-config {
            margin-bottom: 1rem;
        }
        .config-grid {
            display: grid;
            grid-template-columns: repeat(3, 1fr);
            gap: 1px;
            background: #222;
            border: 1px solid #222;
        }
        .config-item {
            background: #111;
            padding: 0.5rem 0.75rem;
            display: flex;
            align-items: baseline;
            gap: 0.5rem;
        }
        .config-label {
            font-size: 0.55rem;
            text-transform: uppercase;
            letter-spacing: 0.1em;
            color: #555;
            white-space: nowrap;
        }
        .config-value {
            font-family: 'JetBrains Mono', 'Fira Code', 'Courier New', monospace;
            font-size: 0.75rem;
            color: #e0e0e0;
            word-break: break-all;
        }
        .config-coin {
            font-family: 'JetBrains Mono', 'Fira Code', 'Courier New', monospace;
            font-size: 0.75rem;
            color: #555;
        }
        @media (max-width: 900px) {
            .config-grid { grid-template-columns: repeat(2, 1fr); }
        }
        @media (max-width: 600px) {
            .config-grid { grid-template-columns: 1fr; }
        }

        .loading { color: #333; font-style: italic; font-family: inherit; }
        .toggle-row td {
            text-align: center;
            color: #555;
            cursor: pointer;
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            font-size: 0.65rem;
            text-transform: uppercase;
            letter-spacing: 0.06em;
            padding: 0.5rem;
        }
        .toggle-row:hover td { color: #f4b728; background: #151515; }
        .hidden-rows { display: none; }
    </style>
</head>
<body style="opacity:0;transition:opacity 0.15s">

<!-- ── Header ── -->
<div class="header">
    <h1 id="pool-name">ZCASH MINING POOL</h1>
    <span class="badge" id="network-badge"></span>
    <div class="header-right">
        <span class="status-dot pulsing" id="header-status-dot"></span>
        <span style="font-size:0.6rem;color:#555" id="header-status-text">Connected</span>
        <a id="mine-link" href="#" target="_blank" rel="noopener" class="header-link" style="color:#f4b728;font-weight:600">Mine in Browser</a>
        <a href="/network" class="header-link">Network</a>
        <a href="/zallet" class="header-link">Wallet</a>
        <a href="/previews" class="header-link">Themes</a>
    </div>
</div>

<div class="container">

    <!-- ── Hero Metrics ── -->
    <div class="hero-grid">
        <div class="hero-card">
            <div class="label">Pool Hashrate</div>
            <div class="value-row">
                <div class="value" id="stat-hashrate">--</div>
                <div class="sub" id="stat-hashrate-avg"></div>
            </div>
            <div class="sparkline-wrap"><canvas id="spark-hashrate"></canvas></div>
        </div>
        <div class="hero-card">
            <div class="label">Network Hashrate</div>
            <div class="value-row">
                <div class="value" id="stat-net-hashrate">--</div>
            </div>
            <div class="sparkline-wrap"><canvas id="spark-net-hashrate"></canvas></div>
        </div>
        <div class="hero-card">
            <div class="label">Blocks Found</div>
            <div class="value-row">
                <div class="value" id="stat-blocks">0</div>
                <div class="sub" id="stat-blocks-record"></div>
            </div>
            <div class="sparkline-wrap"><canvas id="spark-blocks"></canvas></div>
        </div>
        <div class="hero-card">
            <div class="label">Connected Miners</div>
            <div class="value-row">
                <div class="value" id="stat-miners">0</div>
                <div class="sub" id="stat-miners-peak"></div>
            </div>
            <div class="sparkline-wrap"><canvas id="spark-miners"></canvas></div>
        </div>
    </div>

    <!-- ── Secondary Metrics ── -->
    <div class="metrics-grid">
        <div class="metric-cell">
            <div class="label">Total Shares</div>
            <div class="value" id="stat-shares">0</div>
        </div>
        <div class="metric-cell">
            <div class="label">Luck</div>
            <div class="value" id="stat-luck">--</div>
        </div>
        <div class="metric-cell">
            <div class="label">Network Share (24h)</div>
            <div class="value" id="stat-pool-pct">--</div>
        </div>
        <div class="metric-cell">
            <div class="label">Immature Blocks</div>
            <div class="value" id="stat-immature">0</div>
        </div>
        <div class="metric-cell">
            <div class="label">Pending Payout</div>
            <div class="value" id="stat-pending-payout">0</div>
        </div>
        <div class="metric-cell">
            <div class="label">Pool Fee</div>
            <div class="value" id="stat-fee">--</div>
        </div>
        <div class="metric-cell" style="grid-column: span 2">
            <div class="label">Stratum Ports</div>
            <div id="stat-ports" style="display:flex;flex-wrap:wrap;gap:6px;margin-top:4px"></div>
        </div>
    </div>

    <!-- ── Stratum Configuration ── -->
    <div class="stratum-config">
        <div class="section-title">How to Connect</div>
        <div class="config-grid">
            <div class="config-item">
                <span class="config-label">Stratum URL</span>
                <span class="config-value" id="stratum-url">loading...</span>
            </div>
            <div class="config-item">
                <span class="config-label">Algorithm</span>
                <span class="config-value">Equihash 200,9</span>
            </div>
            <div class="config-item">
                <span class="config-label">Worker</span>
                <span class="config-value">YOUR_ZCASH_ADDRESS.worker_name</span>
            </div>
            <div class="config-item">
                <span class="config-label">Password</span>
                <span class="config-value">x</span>
            </div>
            <div class="config-item">
                <span class="config-label">Payout Scheme</span>
                <span class="config-value">PPLNS</span>
            </div>
            <div class="config-item">
                <span class="config-label">Min Payout</span>
                <span class="config-value" id="config-min-payout">0.01</span>
                <span class="config-coin" id="config-coin">TAZ</span>
            </div>
        </div>
    </div>

    <!-- ── Chart Panels ── -->
    <div class="chart-row">
        <div class="chart-panel">
            <div class="panel-header">
                <span class="panel-title">Hashrate</span>
                <div class="panel-legend">
                    <span class="legend-item"><span class="legend-swatch" style="background:#f4b728"></span>Pool</span>
                    <span class="legend-item"><span class="legend-swatch" style="background:#4a9eff"></span>Network</span>
                </div>
            </div>
            <canvas id="chart-hashrate"></canvas>
        </div>
        <div class="chart-panel">
            <div class="panel-header">
                <span class="panel-title">Mining</span>
                <div class="panel-legend">
                    <span class="legend-item"><span class="legend-swatch" style="background:#48bb78"></span>Shares/min</span>
                    <span class="legend-item"><span class="legend-swatch" style="background:#f4b728"></span>Blocks</span>
                </div>
            </div>
            <canvas id="chart-mining"></canvas>
        </div>
    </div>

    <!-- ── Miner Lookup ── -->
    <div class="section-title">Miner Lookup</div>
    <div class="lookup-row">
        <input type="text" id="miner-address" placeholder="Enter Zcash address..." onkeydown="if(event.key==='Enter')lookupMiner()">
        <button onclick="lookupMiner()">Lookup</button>
    </div>
    <div id="miner-info">
        <div class="miner-stats-row" id="miner-stats-grid"></div>
        <div class="section-title">Workers</div>
        <div class="table-wrap">
            <table id="workers-table">
                <thead><tr><th>Name</th><th>Last Seen</th></tr></thead>
                <tbody></tbody>
            </table>
        </div>
    </div>

    <!-- ── Miners Table ── -->
    <div class="section-title">Miners</div>
    <div class="table-wrap">
        <table id="miners-table">
            <thead>
                <tr>
                    <th>Address</th>
                    <th>1m Avg</th>
                    <th>10m Avg</th>
                    <th>Workers</th>
                    <th>Shares</th>
                    <th>Pending</th>
                    <th>Joined</th>
                </tr>
            </thead>
            <tbody><tr><td colspan="7" class="loading">Loading...</td></tr></tbody>
        </table>
    </div>

    <!-- ── Blocks Table ── -->
    <div class="section-title">Recent Blocks</div>
    <div class="table-wrap">
        <table id="blocks-table">
            <thead>
                <tr>
                    <th>Height</th>
                    <th>Hash</th>
                    <th>Reward</th>
                    <th>Luck</th>
                    <th>Status</th>
                    <th>Found</th>
                </tr>
            </thead>
            <tbody><tr><td colspan="6" class="loading">Loading...</td></tr></tbody>
        </table>
    </div>

    <!-- ── Payouts Table ── -->
    <div class="section-title">Recent Payouts</div>
    <div class="table-wrap">
        <table id="payouts-table">
            <thead>
                <tr>
                    <th>Miner</th>
                    <th>Amount</th>
                    <th>TxID</th>
                    <th>Date</th>
                </tr>
            </thead>
            <tbody><tr><td colspan="4" class="loading">Loading...</td></tr></tbody>
        </table>
    </div>

    <div class="footer-spacer"></div>
</div>

<!-- ── Status Footer ── -->
<div class="status-footer">
    <div class="sf-item">
        <span class="sf-dot ok" id="sf-node-dot"></span>
        <span>Node</span>
        <span id="sf-node-text" style="color:#666">--</span>
    </div>
    <div class="sf-item">
        <span class="sf-dot ok" id="sf-wallet-dot"></span>
        <span>Wallet</span>
        <span id="sf-wallet-text" style="color:#666">--</span>
    </div>
    <div class="sf-item" id="sf-template-item">
        <span>Last Template</span>
        <span id="sf-template-text" style="color:#666">--</span>
    </div>
    <div style="margin-left:auto" class="sf-item">
        <span id="sf-uptime">--</span>
    </div>
</div>

<script>
const MAX_HISTORY = 360;
const REFRESH_STATS = 10000;
const REFRESH_MINERS = 10000;
const REFRESH_BLOCKS = 30000;
let COIN = 'TAZ'; // updated from /api/pool/info

const history = {
    hashrate: [],
    netHashrate: [],
    miners: [],
    blocks: [],
    shares: [],
    labels: []
};
let prevShares = null;
let peakMiners = 0;
let dashStarted = Date.now();

function formatHashrate(h) {
    if (h == null || isNaN(h)) return '--';
    if (h >= 1e12) return (h / 1e12).toFixed(2) + ' TSol/s';
    if (h >= 1e9) return (h / 1e9).toFixed(2) + ' GSol/s';
    if (h >= 1e6) return (h / 1e6).toFixed(2) + ' MSol/s';
    if (h >= 1e3) return (h / 1e3).toFixed(2) + ' KSol/s';
    return h.toFixed(1) + ' Sol/s';
}

function formatDuration(ms) {
    const s = Math.floor(ms / 1000);
    const m = Math.floor(s / 60);
    const h = Math.floor(m / 60);
    if (h > 0) return h + 'h ' + (m % 60) + 'm';
    if (m > 0) return m + 'm ' + (s % 60) + 's';
    return s + 's';
}

function flashEl(id) {
    const el = document.getElementById(id);
    if (!el) return;
    el.classList.remove('flash');
    void el.offsetWidth;
    el.classList.add('flash');
}

function pushHistory(arr, val) {
    arr.push(val);
    if (arr.length > MAX_HISTORY) arr.shift();
}

/* ── Sparkline Charts ── */
const sparkCfg = (color) => ({
    type: 'line',
    data: { labels: [], datasets: [{ data: [], borderColor: color, borderWidth: 1.5, fill: false, pointRadius: 0, tension: 0.3 }] },
    options: {
        responsive: true, maintainAspectRatio: false,
        plugins: { legend: { display: false }, tooltip: { enabled: false } },
        scales: { x: { display: false }, y: { display: false } },
        animation: { duration: 300 }
    }
});

let sparkHashrate, sparkNetHashrate, sparkBlocks, sparkMiners;

function initSparklines() {
    sparkHashrate = new Chart(document.getElementById('spark-hashrate'), sparkCfg('#f4b728'));
    sparkNetHashrate = new Chart(document.getElementById('spark-net-hashrate'), sparkCfg('#4a9eff'));
    sparkBlocks = new Chart(document.getElementById('spark-blocks'), sparkCfg('#48bb78'));
    sparkMiners = new Chart(document.getElementById('spark-miners'), sparkCfg('#a78bfa'));
}

function updateSparkline(chart, data, labels) {
    chart.data.labels = labels;
    chart.data.datasets[0].data = data;
    chart.update('none');
}

/* ── Full Charts ── */
let chartHashrate, chartMining;

function initCharts() {
    const shared = {
        responsive: true, maintainAspectRatio: false,
        plugins: { legend: { display: false }, tooltip: {
            mode: 'index', intersect: false,
            backgroundColor: '#1a1a1a', titleColor: '#888', bodyColor: '#ccc', borderColor: '#333', borderWidth: 1,
            titleFont: { size: 10 }, bodyFont: { size: 10, family: "'JetBrains Mono', monospace" },
            padding: 6
        }},
        scales: {
            x: { display: true, grid: { color: '#1a1a1a' }, ticks: { color: '#333', font: { size: 9 }, maxTicksLimit: 8 } },
            y: { display: true, grid: { color: '#1a1a1a' }, ticks: { color: '#333', font: { size: 9 }, maxTicksLimit: 5 }, beginAtZero: true }
        },
        animation: { duration: 300 }
    };

    chartHashrate = new Chart(document.getElementById('chart-hashrate'), {
        type: 'line',
        data: {
            labels: [],
            datasets: [
                { label: 'Pool', data: [], borderColor: '#f4b728', borderWidth: 1.5, fill: false, pointRadius: 0, tension: 0.3 },
                { label: 'Network', data: [], borderColor: '#4a9eff', borderWidth: 1.5, fill: false, pointRadius: 0, tension: 0.3 }
            ]
        },
        options: structuredClone(shared)
    });

    chartMining = new Chart(document.getElementById('chart-mining'), {
        type: 'line',
        data: {
            labels: [],
            datasets: [
                { label: 'Shares/min', data: [], borderColor: '#48bb78', borderWidth: 1.5, fill: false, pointRadius: 0, tension: 0.3, yAxisID: 'y' },
                { label: 'Blocks', data: [], borderColor: '#f4b728', borderWidth: 1.5, fill: false, pointRadius: 2, pointBackgroundColor: '#f4b728', tension: 0, yAxisID: 'y1' }
            ]
        },
        options: {
            ...structuredClone(shared),
            scales: {
                ...structuredClone(shared.scales),
                y1: { display: false, position: 'right', grid: { display: false }, beginAtZero: true }
            }
        }
    });
}

function updateCharts() {
    const labels = history.labels.map(t => {
        const d = new Date(t);
        return d.getHours().toString().padStart(2,'0') + ':' + d.getMinutes().toString().padStart(2,'0') + ':' + d.getSeconds().toString().padStart(2,'0');
    });

    chartHashrate.data.labels = labels;
    chartHashrate.data.datasets[0].data = [...history.hashrate];
    chartHashrate.data.datasets[1].data = [...history.netHashrate];
    chartHashrate.update('none');

    chartMining.data.labels = labels;
    chartMining.data.datasets[0].data = [...history.shares];
    chartMining.data.datasets[1].data = [...history.blocks];
    chartMining.update('none');
}

/* ── Collapsible Table Helper ── */
const COLLAPSE_LIMIT = 10;
function renderCollapsible(rows, colspan, id) {
    if (rows.length <= COLLAPSE_LIMIT) return rows.join('');
    const visible = rows.slice(0, COLLAPSE_LIMIT).join('');
    const hidden = rows.slice(COLLAPSE_LIMIT).join('');
    const remaining = rows.length - COLLAPSE_LIMIT;
    return visible +
        '<tr class="hidden-rows" id="' + id + '-hidden">' +
            '<td colspan="' + colspan + '" style="padding:0;border:0">' +
                '<table style="width:100%">' + hidden + '</table>' +
            '</td>' +
        '</tr>' +
        '<tr class="toggle-row" onclick="toggleTable(\'' + id + '\')">' +
            '<td colspan="' + colspan + '" id="' + id + '-toggle">Show all ' + rows.length + ' ' + id + ' ▾</td>' +
        '</tr>';
}
function toggleTable(id) {
    const hidden = document.getElementById(id + '-hidden');
    const toggle = document.getElementById(id + '-toggle');
    if (!hidden || !toggle) return;
    const expanded = hidden.style.display === 'table-row';
    hidden.style.display = expanded ? 'none' : 'table-row';
    const total = hidden.querySelector('table').rows.length + COLLAPSE_LIMIT;
    toggle.textContent = expanded
        ? 'Show all ' + total + ' ' + id + ' ▾'
        : 'Show less ▴';
}

/* ── Data Fetching ── */
async function fetchStats() {
    try {
        const resp = await fetch('/api/pool/stats');
        const d = await resp.json();
        const now = Date.now();

        document.getElementById('pool-name').textContent = d.name.toUpperCase();
        COIN = d.network === 'mainnet' ? 'ZEC' : 'TAZ';
        const badge = document.getElementById('network-badge');
        if (badge) badge.textContent = d.network === 'mainnet' ? 'Mainnet' : 'Testnet';
        const mineLink = document.getElementById('mine-link');
        if (mineLink && d.stratum_url) {
            const host = d.stratum_url.replace(/^stratum\+tcp:\/\//, '').replace(/:\d+$/, '');
            mineLink.href = 'http://' + host + ':3000';
        }

        const setVal = (id, val) => {
            const el = document.getElementById(id);
            if (el && el.textContent !== String(val)) { el.textContent = val; flashEl(id); }
        };

        setVal('stat-hashrate', formatHashrate(d.hashrate_current));
        document.getElementById('stat-hashrate-avg').textContent = '10m: ' + formatHashrate(d.hashrate_estimate);
        setVal('stat-net-hashrate', formatHashrate(d.network_hashrate));
        setVal('stat-blocks', d.total_blocks);
        setVal('stat-miners', d.connected_miners);

        if (d.connected_miners > peakMiners) peakMiners = d.connected_miners;
        document.getElementById('stat-miners-peak').textContent = 'Peak: ' + peakMiners;

        setVal('stat-shares', d.total_shares.toLocaleString());
        setVal('stat-fee', d.fee_percent + '%');
        if (d.stratum_ports && d.stratum_ports.length) {
            const el = document.getElementById('stat-ports');
            el.innerHTML = d.stratum_ports.map(p =>
                '<div style="background:#1a1a2e;border:1px solid #333;border-radius:6px;padding:4px 10px;text-align:center">' +
                '<div style="font-size:1.1rem;color:#f4b728;font-weight:600">' + p.port + '</div>' +
                '<div style="font-size:0.65rem;color:#888">' + p.description + '</div></div>'
            ).join('');
        }
        setVal('stat-immature', d.immature_blocks);
        setVal('stat-pending-payout', d.pending_payout_blocks);

        // Update stratum config section
        const configCoin = document.getElementById('config-coin');
        if (configCoin) configCoin.textContent = COIN;
        const stratumUrl = document.getElementById('stratum-url');
        if (stratumUrl) stratumUrl.textContent = d.stratum_url || ('stratum+tcp://' + window.location.hostname + ':' + d.stratum_port);

        const luckEl = document.getElementById('stat-luck');
        if (d.luck_percent != null) {
            const lv = d.luck_percent;
            luckEl.textContent = lv.toFixed(0) + '%';
            luckEl.style.color = lv <= 100 ? '#48bb78' : lv <= 150 ? '#ecc94b' : '#fc8181';
        } else {
            luckEl.textContent = '--';
            luckEl.style.color = '#555';
        }

        const pctEl = document.getElementById('stat-pool-pct');
        if (d.pool_percent_24h != null) {
            pctEl.textContent = d.pool_percent_24h.toFixed(2) + '%';
            pctEl.style.color = '#f4b728';
        } else {
            pctEl.textContent = '--';
            pctEl.style.color = '#555';
        }

        const headerDot = document.getElementById('header-status-dot');
        const headerText = document.getElementById('header-status-text');
        const nodeOk = d.node_ok !== false;
        headerDot.className = 'status-dot pulsing' + (nodeOk ? '' : ' offline');
        headerText.textContent = nodeOk ? 'Connected' : 'Stalled';

        const sfNodeDot = document.getElementById('sf-node-dot');
        sfNodeDot.className = 'sf-dot ' + (nodeOk ? 'ok' : 'err');
        document.getElementById('sf-node-text').textContent = nodeOk ? 'OK' : 'Stalled';

        const walletOk = d.wallet_ok !== false;
        const sfWalletDot = document.getElementById('sf-wallet-dot');
        sfWalletDot.className = 'sf-dot ' + (walletOk ? 'ok' : 'err');
        document.getElementById('sf-wallet-text').textContent = walletOk ? 'Online' : 'Offline';

        if (d.last_template_at) {
            document.getElementById('sf-template-text').textContent = d.last_template_at.replace('T', ' ').slice(0, 19);
        }
        document.getElementById('sf-uptime').textContent = 'Dashboard ' + formatDuration(now - dashStarted);

        const sharesPerMin = prevShares !== null ? Math.max(0, (d.total_shares - prevShares) * (60000 / REFRESH_STATS)) : 0;
        prevShares = d.total_shares;

        pushHistory(history.hashrate, d.hashrate_estimate || 0);
        pushHistory(history.netHashrate, d.network_hashrate || 0);
        pushHistory(history.miners, d.connected_miners || 0);
        pushHistory(history.blocks, d.total_blocks || 0);
        pushHistory(history.shares, Math.round(sharesPerMin));
        pushHistory(history.labels, now);

        const idxLabels = history.labels.map(() => '');
        updateSparkline(sparkHashrate, history.hashrate, idxLabels);
        updateSparkline(sparkNetHashrate, history.netHashrate, idxLabels);
        updateSparkline(sparkBlocks, history.blocks, idxLabels);
        updateSparkline(sparkMiners, history.miners, idxLabels);
        updateCharts();
    } catch (e) {
        console.error('Failed to fetch stats:', e);
    }
}

async function fetchBlocks() {
    try {
        const resp = await fetch('/api/blocks');
        const blocks = await resp.json();
        const tbody = document.querySelector('#blocks-table tbody');
        if (blocks.length === 0) {
            tbody.innerHTML = '<tr><td colspan="6" class="loading">No blocks found yet</td></tr>';
            return;
        }
        const rows = blocks.map(b => {
            let luckStr = '--';
            let luckColor = '#555';
            if (b.luck_percent != null) {
                luckStr = b.luck_percent.toFixed(0) + '%';
                luckColor = b.luck_percent <= 100 ? '#48bb78' : b.luck_percent <= 150 ? '#ecc94b' : '#fc8181';
            }
            return '<tr>' +
                '<td style="color:#e0e0e0">' + b.height + '</td>' +
                '<td title="' + b.hash + '">' + b.hash.substring(0, 16) + '...</td>' +
                '<td>' + b.reward_zec.toFixed(4) + ' ' + COIN + '</td>' +
                '<td style="color:' + luckColor + '">' + luckStr + '</td>' +
                '<td class="status-' + b.status + '">' + b.status + '</td>' +
                '<td>' + b.found_at + '</td>' +
                '</tr>';
        });
        tbody.innerHTML = renderCollapsible(rows, 6, 'blocks');
    } catch (e) {
        console.error('Failed to fetch blocks:', e);
    }
}

async function fetchMiners() {
    try {
        const resp = await fetch('/api/miners');
        const miners = await resp.json();
        miners.sort((a, b) => (b.hashrate || 0) - (a.hashrate || 0));
        const tbody = document.querySelector('#miners-table tbody');
        if (miners.length === 0) {
            tbody.innerHTML = '<tr><td colspan="7" class="loading">No miners yet</td></tr>';
            return;
        }
        const rows = miners.map(m =>
            '<tr>' +
            '<td class="addr-cell" title="' + m.address + '"><a class="addr-link" href="/miner/' + encodeURIComponent(m.address) + '">' + m.address + '</a></td>' +
            '<td>' + formatHashrate(m.hashrate_1m) + '</td>' +
            '<td>' + formatHashrate(m.hashrate) + '</td>' +
            '<td>' + m.worker_count + '</td>' +
            '<td>' + m.share_count.toLocaleString() + '</td>' +
            '<td>' + m.pending_zec.toFixed(8) + ' ' + COIN + '</td>' +
            '<td>' + m.joined + '</td>' +
            '</tr>'
        );
        tbody.innerHTML = renderCollapsible(rows, 7, 'miners');
    } catch (e) {
        console.error('Failed to fetch miners:', e);
    }
}

async function fetchPayouts() {
    try {
        const resp = await fetch('/api/payouts');
        const payouts = await resp.json();
        const tbody = document.querySelector('#payouts-table tbody');
        if (payouts.length === 0) {
            tbody.innerHTML = '<tr><td colspan="4" class="loading">No payouts yet</td></tr>';
            return;
        }
        const rows = payouts.map(p => {
            const txid = p.txid ? p.txid.substring(0, 16) + '...' : '--';
            const txTitle = p.txid || '';
            return '<tr>' +
                '<td class="addr-cell" title="' + p.miner_address + '" style="color:#e0e0e0">' + p.miner_address + '</td>' +
                '<td style="color:#48bb78">' + p.amount_zec.toFixed(8) + ' ' + COIN + '</td>' +
                '<td title="' + txTitle + '">' + txid + '</td>' +
                '<td>' + p.created_at + '</td>' +
                '</tr>';
        });
        tbody.innerHTML = renderCollapsible(rows, 4, 'payouts');
    } catch (e) {
        console.error('Failed to fetch payouts:', e);
    }
}

function doLookup(addr) {
    document.getElementById('miner-address').value = addr;
    lookupMiner();
}

async function lookupMiner() {
    const addr = document.getElementById('miner-address').value.trim();
    if (!addr) return;
    try {
        const resp = await fetch('/api/miner/' + encodeURIComponent(addr));
        if (!resp.ok) { alert('Miner not found'); return; }
        const data = await resp.json();
        document.getElementById('miner-info').style.display = 'block';
        document.getElementById('miner-stats-grid').innerHTML =
            '<div class="metric-cell"><div class="label">Pending Balance</div><div class="value" style="color:#f4b728">' + data.balance.pending_zec.toFixed(8) + ' ' + COIN + '</div></div>' +
            '<div class="metric-cell"><div class="label">Total Paid</div><div class="value" style="color:#48bb78">' + data.balance.paid_zec.toFixed(8) + ' ' + COIN + '</div></div>' +
            '<div class="metric-cell"><div class="label">Workers</div><div class="value">' + data.workers.length + '</div></div>';
        const tbody = document.querySelector('#workers-table tbody');
        tbody.innerHTML = data.workers.map(w => '<tr><td>' + w.name + '</td><td>' + w.last_seen + '</td></tr>').join('');
    } catch (e) {
        console.error('Lookup failed:', e);
    }
}

/* ── History Pre-fill ── */
async function fetchHistory() {
    try {
        const resp = await fetch('/api/pool/stats/history');
        const snapshots = await resp.json();
        if (!snapshots || snapshots.length === 0) return;

        // Pre-fill core history arrays from server snapshots
        for (const s of snapshots) {
            history.hashrate.push(s.pool_hashrate || 0);
            history.netHashrate.push(s.network_hashrate || 0);
            history.miners.push(s.connected_miners || 0);
            history.blocks.push(s.total_blocks || 0);
            history.labels.push(s.timestamp_ms);
        }

        // Compute shares/min deltas between consecutive snapshots
        for (let i = 0; i < snapshots.length; i++) {
            if (i === 0) {
                history.shares.push(0);
            } else {
                const dt = (snapshots[i].timestamp_ms - snapshots[i-1].timestamp_ms) / 1000;
                const ds = snapshots[i].total_shares - snapshots[i-1].total_shares;
                const sharesPerMin = dt > 0 ? Math.max(0, (ds / dt) * 60) : 0;
                history.shares.push(Math.round(sharesPerMin));
            }
        }

        // Set prevShares so the first live poll computes a correct delta
        prevShares = snapshots[snapshots.length - 1].total_shares;

        // Track peak miners from history
        for (const s of snapshots) {
            if (s.connected_miners > peakMiners) peakMiners = s.connected_miners;
        }
        document.getElementById('stat-miners-peak').textContent = 'Peak: ' + peakMiners;

        // Update all charts with pre-filled data
        const idxLabels = history.labels.map(() => '');
        updateSparkline(sparkHashrate, history.hashrate, idxLabels);
        updateSparkline(sparkNetHashrate, history.netHashrate, idxLabels);
        updateSparkline(sparkBlocks, history.blocks, idxLabels);
        updateSparkline(sparkMiners, history.miners, idxLabels);
        updateCharts();
    } catch (e) {
        console.error('Failed to fetch stats history:', e);
    }
}

/* ── Init ── */
document.addEventListener('DOMContentLoaded', () => {
    document.body.style.opacity = '1';
    initSparklines();
    initCharts();
    fetchHistory();
    fetchStats();
    fetchMiners();
    fetchBlocks();
    fetchPayouts();
    setInterval(fetchStats, REFRESH_STATS);
    setInterval(fetchMiners, REFRESH_MINERS);
    setInterval(fetchBlocks, REFRESH_BLOCKS);
    setInterval(fetchPayouts, REFRESH_BLOCKS);
});
</script>
</body>
</html>
"##;
