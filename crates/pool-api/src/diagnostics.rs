use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, Json};
use serde::Serialize;

use crate::handlers::{ApiError, AppState};

const ZATOSHIS_PER_ZEC: f64 = 100_000_000.0;

#[derive(Serialize)]
pub struct WorkerInfoDiag {
    pub name: String,
    pub hashrate_1m: f64,
    pub hashrate_10m: f64,
    pub current_difficulty: Option<f64>,
    pub shares_1m: i64,
    pub shares_10m: i64,
    pub total_shares: i64,
    pub last_seen: String,
    pub is_online: bool,
}

#[derive(Serialize)]
pub struct ShareEntry {
    pub time: String,
    pub worker: String,
    pub session_id: Option<String>,
    pub difficulty: f64,
    pub is_block: bool,
}

#[derive(Serialize)]
pub struct BlockEntry {
    pub height: i64,
    pub hash: String,
    pub reward_zec: f64,
    pub status: String,
    pub found_at: String,
}

#[derive(Serialize)]
pub struct PayoutEntry {
    pub amount_zec: f64,
    pub txid: Option<String>,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct MinerDiagnostics {
    pub address: String,
    pub pending_zec: f64,
    pub paid_zec: f64,
    pub total_hashrate_1m: f64,
    pub total_hashrate_10m: f64,
    pub workers: Vec<WorkerInfoDiag>,
    pub recent_shares: Vec<ShareEntry>,
    pub blocks_found: Vec<BlockEntry>,
    pub payouts: Vec<PayoutEntry>,
    /// Live rejection stats summed across this miner's currently-active sessions.
    pub live_session_stats: LiveSessionStats,
}

#[derive(Serialize, Default)]
pub struct LiveSessionStats {
    pub active_sessions: usize,
    pub accepted: u64,
    pub rejected_low_diff: u64,
    pub rejected_job_not_found: u64,
    pub rejected_other: u64,
    pub rejection_pct: f64,
}

pub async fn get_miner_diagnostics(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<MinerDiagnostics>, (StatusCode, Json<ApiError>)> {
    let miner = state
        .db
        .get_miner_by_address(&address)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError { error: "Database error".to_string() }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError { error: "Miner not found".to_string() }),
            )
        })?;

    let now = chrono::Utc::now();
    let since_1m = now
        .checked_sub_signed(chrono::Duration::minutes(1))
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();
    let since_10m = now
        .checked_sub_signed(chrono::Duration::minutes(10))
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();
    let online_threshold = now
        .checked_sub_signed(chrono::Duration::minutes(5))
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();

    let balance = state.db.get_or_create_balance(miner.id).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError { error: "Database error".to_string() }),
        )
    })?;

    let worker_stats = state
        .db
        .get_worker_stats_for_miner(miner.id, &since_1m, &since_10m)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError { error: "Database error".to_string() }),
            )
        })?;

    let recent_shares = state
        .db
        .get_recent_shares_for_miner(miner.id, 500)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError { error: "Database error".to_string() }),
            )
        })?;

    let blocks = state.db.get_miner_blocks(miner.id).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError { error: "Database error".to_string() }),
        )
    })?;

    let payouts = state
        .db
        .get_miner_payouts(miner.id, 50)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError { error: "Database error".to_string() }),
            )
        })?;

    let mult = state.difficulty_multiplier;

    let mut total_hashrate_1m = 0.0;
    let mut total_hashrate_10m = 0.0;

    let workers: Vec<WorkerInfoDiag> = worker_stats
        .into_iter()
        .map(|w| {
            let hr_1m = (w.diff_sum_1m / 60.0) * mult;
            let hr_10m = (w.diff_sum_10m / 600.0) * mult;
            total_hashrate_1m += hr_1m;
            total_hashrate_10m += hr_10m;
            WorkerInfoDiag {
                name: w.name,
                hashrate_1m: hr_1m,
                hashrate_10m: hr_10m,
                current_difficulty: w.current_difficulty,
                shares_1m: w.shares_1m,
                shares_10m: w.shares_10m,
                total_shares: w.total_shares,
                is_online: w.last_seen >= online_threshold,
                last_seen: w.last_seen,
            }
        })
        .collect();

    let share_entries: Vec<ShareEntry> = recent_shares
        .into_iter()
        .map(|s| ShareEntry {
            time: s.created_at,
            worker: s.worker_name,
            session_id: s.session_id,
            difficulty: s.difficulty,
            is_block: s.is_block,
        })
        .collect();

    let block_entries: Vec<BlockEntry> = blocks
        .into_iter()
        .map(|b| BlockEntry {
            height: b.height,
            hash: b.hash,
            reward_zec: b.reward as f64 / ZATOSHIS_PER_ZEC,
            status: b.status,
            found_at: b.created_at,
        })
        .collect();

    let payout_entries: Vec<PayoutEntry> = payouts
        .into_iter()
        .map(|p| PayoutEntry {
            amount_zec: p.amount as f64 / ZATOSHIS_PER_ZEC,
            txid: p.txid,
            created_at: p.created_at,
        })
        .collect();

    // Aggregate live-session rejection stats for this miner from the
    // sessions_snapshot maintained by pool-core.
    let live_session_stats = aggregate_live_session_stats(&state, &miner.address).await;

    Ok(Json(MinerDiagnostics {
        address: miner.address,
        pending_zec: balance.pending as f64 / ZATOSHIS_PER_ZEC,
        paid_zec: balance.paid as f64 / ZATOSHIS_PER_ZEC,
        total_hashrate_1m,
        total_hashrate_10m,
        workers,
        recent_shares: share_entries,
        blocks_found: block_entries,
        payouts: payout_entries,
        live_session_stats,
    }))
}

/// Sum per-session rejection counters across all active sessions for one
/// miner address. Reads the sessions_snapshot pool_status row written by
/// pool-core every few seconds.
async fn aggregate_live_session_stats(state: &AppState, address: &str) -> LiveSessionStats {
    let snap = match state.db.get_pool_status("sessions_snapshot").await {
        Ok(Some((v, _))) => v,
        _ => return LiveSessionStats::default(),
    };
    let sessions: Vec<crate::sessions::SessionSnapshot> =
        match serde_json::from_str(&snap) {
            Ok(s) => s,
            Err(_) => return LiveSessionStats::default(),
        };
    let mut stats = LiveSessionStats::default();
    for s in sessions.iter() {
        // Match the address prefix of the worker_name (everything before the dot).
        let worker_addr = s.worker_name.split('.').next().unwrap_or("");
        if worker_addr != address {
            continue;
        }
        stats.active_sessions += 1;
        stats.accepted = stats.accepted.saturating_add(s.shares_accepted);
        stats.rejected_low_diff = stats.rejected_low_diff.saturating_add(s.shares_rejected_low_diff);
        stats.rejected_job_not_found = stats.rejected_job_not_found.saturating_add(s.shares_rejected_job_not_found);
        stats.rejected_other = stats.rejected_other.saturating_add(s.shares_rejected_other);
    }
    let total_rej = stats.rejected_low_diff + stats.rejected_job_not_found + stats.rejected_other;
    let total = stats.accepted + total_rej;
    stats.rejection_pct = if total > 0 { (total_rej as f64 / total as f64) * 100.0 } else { 0.0 };
    stats
}

pub async fn miner_page(State(state): State<AppState>) -> Html<String> {
    let explorer = crate::routes::explorer_for(&state.network);
    Html(MINER_DIAGNOSTICS_HTML.replace("__INITIAL_EXPLORER__", explorer))
}

const MINER_DIAGNOSTICS_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Miner Diagnostics - Zcash Mining Pool</title>
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

        .container { max-width: 1400px; margin: 0 auto; padding: 1rem 1.5rem; }

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

        .hero-grid {
            display: grid;
            grid-template-columns: repeat(6, 1fr);
            gap: 1px;
            background: #222;
            border: 1px solid #222;
            margin-bottom: 1rem;
        }
        .hero-card {
            background: #111;
            padding: 0.75rem 1rem;
        }
        .hero-card .label {
            font-size: 0.55rem;
            text-transform: uppercase;
            letter-spacing: 0.1em;
            color: #555;
            margin-bottom: 0.15rem;
        }
        .hero-card .value {
            font-family: 'JetBrains Mono', 'Fira Code', 'Courier New', monospace;
            font-size: 1.2rem;
            font-weight: 700;
            color: #f4b728;
        }
        .hero-card .value.green { color: #48bb78; }

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

        .status-online { color: #48bb78; }
        .status-offline { color: #fc8181; }
        .status-dot {
            width: 8px;
            height: 8px;
            border-radius: 50%;
            display: inline-block;
            margin-right: 4px;
        }
        .dot-online { background: #48bb78; }
        .dot-offline { background: #fc8181; }

        .block-row { background: rgba(244, 183, 40, 0.06); }
        .block-row td { color: #f4b728; }

        .chart-panel {
            background: #111;
            border: 1px solid #222;
            padding: 0.75rem 1rem;
            margin-bottom: 1rem;
        }
        .chart-panel .panel-title {
            font-size: 0.6rem;
            text-transform: uppercase;
            letter-spacing: 0.1em;
            color: #555;
            margin-bottom: 0.5rem;
        }
        .chart-panel canvas {
            width: 100% !important;
            height: 200px !important;
        }

        .status-confirmed { color: #48bb78; }
        .status-pending { color: #ecc94b; }
        .status-orphaned { color: #fc8181; }

        .empty-msg {
            color: #333;
            font-style: italic;
            padding: 1rem;
            text-align: center;
        }

        #diag-content { display: none; }
        #loading-msg { color: #555; padding: 2rem; text-align: center; font-size: 0.8rem; }
        #error-msg { color: #fc8181; padding: 2rem; text-align: center; font-size: 0.8rem; display: none; }

        @media (max-width: 900px) {
            .hero-grid { grid-template-columns: repeat(3, 1fr); }
        }
        @media (max-width: 600px) {
            .hero-grid { grid-template-columns: repeat(2, 1fr); }
        }
    </style>
</head>
<body style="opacity:0;transition:opacity 0.15s">

<div class="header">
    <h1>MINER DIAGNOSTICS</h1>
    <span class="badge" id="network-badge">Testnet</span>
    <div class="header-right">
        <a href="/" class="header-link">Dashboard</a>
        <a href="/network" class="header-link">Network</a>
        <a href="/zallet" class="header-link">Wallet</a>
    </div>
</div>

<div class="container">
    <div class="lookup-row">
        <input type="text" id="miner-address" placeholder="Enter miner address..." onkeydown="if(event.key==='Enter')navigate()">
        <button onclick="navigate()">Lookup</button>
    </div>

    <div id="loading-msg">Loading miner data...</div>
    <div id="error-msg"></div>

    <div id="diag-content">
        <div class="hero-grid">
            <div class="hero-card">
                <div class="label">Hashrate (1m)</div>
                <div class="value" id="hr-1m">--</div>
            </div>
            <div class="hero-card">
                <div class="label">Hashrate (10m)</div>
                <div class="value" id="hr-10m">--</div>
            </div>
            <div class="hero-card">
                <div class="label">Pending Balance</div>
                <div class="value" id="bal-pending">--</div>
            </div>
            <div class="hero-card">
                <div class="label">Total Paid</div>
                <div class="value green" id="bal-paid">--</div>
            </div>
            <div class="hero-card">
                <div class="label">Total Workers</div>
                <div class="value" id="total-workers">--</div>
            </div>
            <div class="hero-card">
                <div class="label">Blocks Found</div>
                <div class="value" id="blocks-found">--</div>
            </div>
            <div class="hero-card">
                <div class="label" title="Rejection rate across this miner's currently-active sessions (low-diff + job-not-found + other / total submissions)">Live Reject %</div>
                <div class="value" id="reject-pct" title="Hover for breakdown">--</div>
            </div>
        </div>

        <div class="section-title">Workers</div>
        <div class="table-wrap">
            <table id="workers-table">
                <thead>
                    <tr>
                        <th>Status</th>
                        <th>Name</th>
                        <th>Hashrate (1m)</th>
                        <th>Hashrate (10m)</th>
                        <th>Current Difficulty</th>
                        <th>Shares (1m)</th>
                        <th>Shares (10m)</th>
                        <th>Total Shares</th>
                        <th>Last Seen</th>
                    </tr>
                </thead>
                <tbody></tbody>
            </table>
        </div>

        <div class="section-title" style="display:flex;align-items:center;gap:1rem">
            Difficulty History
            <select id="worker-filter" onchange="applyWorkerFilter()" style="background:#111;border:1px solid #333;color:#999;padding:0.2rem 0.5rem;font-size:0.7rem;font-family:inherit;cursor:pointer">
                <option value="">All Workers</option>
            </select>
        </div>
        <div class="chart-panel">
            <canvas id="diff-chart"></canvas>
        </div>

        <div class="section-title">Recent Shares (last 200)</div>
        <div class="table-wrap">
            <table id="shares-table">
                <thead>
                    <tr>
                        <th>Time</th>
                        <th>Worker</th>
                        <th>Session</th>
                        <th>Difficulty</th>
                        <th>Block?</th>
                    </tr>
                </thead>
                <tbody></tbody>
            </table>
        </div>

        <div class="section-title">Blocks Found</div>
        <div class="table-wrap">
            <table id="blocks-table">
                <thead>
                    <tr>
                        <th>Height</th>
                        <th>Hash</th>
                        <th>Reward</th>
                        <th>Status</th>
                        <th>Found</th>
                    </tr>
                </thead>
                <tbody></tbody>
            </table>
        </div>

        <div class="section-title">Payouts</div>
        <div class="table-wrap">
            <table id="payouts-table">
                <thead>
                    <tr>
                        <th>Amount</th>
                        <th>TxID</th>
                        <th>Date</th>
                    </tr>
                </thead>
                <tbody></tbody>
            </table>
        </div>
    </div>
</div>

<script>
const WORKER_COLORS = ['#f4b728','#4a9eff','#48bb78','#fc8181','#a78bfa','#f687b3','#68d391','#ed8936'];
let diffChart = null;
let refreshTimer = null;
let COIN = 'TAZ';
let EXPLORER = '__INITIAL_EXPLORER__';
let allShares = [];
let allWorkerNames = [];
async function initCoin() {
    try {
        const r = await fetch('/api/pool/info');
        const d = await r.json();
        COIN = d.coin || 'TAZ';
        EXPLORER = d.network === 'mainnet' ? 'https://cipherscan.app' : 'https://testnet.cipherscan.app';
        const badge = document.getElementById('network-badge');
        if (badge) badge.textContent = d.network === 'mainnet' ? 'Mainnet' : 'Testnet';
    } catch(e) {}
}

function formatHashrate(h) {
    if (h == null || isNaN(h)) return '--';
    if (h >= 1e12) return (h / 1e12).toFixed(2) + ' TSol/s';
    if (h >= 1e9) return (h / 1e9).toFixed(2) + ' GSol/s';
    if (h >= 1e6) return (h / 1e6).toFixed(2) + ' MSol/s';
    if (h >= 1e3) return (h / 1e3).toFixed(2) + ' KSol/s';
    return h.toFixed(1) + ' Sol/s';
}

function getAddress() {
    const path = window.location.pathname;
    const match = path.match(/^\/miner\/(.+)$/);
    return match ? decodeURIComponent(match[1]) : '';
}

function navigate() {
    const addr = document.getElementById('miner-address').value.trim();
    if (addr) window.location.href = '/miner/' + encodeURIComponent(addr);
}

function buildDiffChart(shares) {
    // Group shares by worker, reverse to chronological order
    const byWorker = {};
    const reversed = [...shares].reverse();
    for (const s of reversed) {
        if (!byWorker[s.worker]) byWorker[s.worker] = [];
        byWorker[s.worker].push({ time: s.time, difficulty: s.difficulty });
    }

    // Assign a sequential index to each share per worker for proper x-axis alignment.
    const workerNames = Object.keys(byWorker);
    const datasets = workerNames.map((name, i) => ({
        label: name,
        data: byWorker[name].map((p, idx) => ({ x: idx, y: p.difficulty })),
        borderColor: WORKER_COLORS[i % WORKER_COLORS.length],
        borderWidth: 1.5,
        fill: false,
        pointRadius: 1,
        tension: 0.2,
    }));

    const ctx = document.getElementById('diff-chart');
    if (diffChart) diffChart.destroy();
    diffChart = new Chart(ctx, {
        type: 'line',
        data: { datasets },
        options: {
            responsive: true,
            maintainAspectRatio: false,
            plugins: {
                legend: {
                    display: workerNames.length > 1,
                    labels: { color: '#666', font: { size: 10 } }
                },
                tooltip: {
                    mode: 'nearest',
                    intersect: false,
                    backgroundColor: '#1a1a1a',
                    titleColor: '#888',
                    bodyColor: '#ccc',
                    borderColor: '#333',
                    borderWidth: 1,
                    titleFont: { size: 10 },
                    bodyFont: { size: 10, family: "'JetBrains Mono', monospace" },
                    callbacks: {
                        title: function(items) {
                            if (!items.length) return '';
                            const name = items[0].dataset.label;
                            const idx = items[0].parsed.x;
                            const point = byWorker[name] && byWorker[name][idx];
                            return point ? point.time : '';
                        }
                    }
                }
            },
            scales: {
                x: {
                    type: 'linear',
                    display: true,
                    grid: { color: '#1a1a1a' },
                    ticks: { color: '#333', font: { size: 9 }, maxTicksLimit: 10 },
                    title: { display: true, text: 'Share #', color: '#444', font: { size: 10 } }
                },
                y: {
                    display: true,
                    grid: { color: '#1a1a1a' },
                    ticks: { color: '#333', font: { size: 9 } },
                    title: { display: true, text: 'Difficulty', color: '#444', font: { size: 10 } }
                }
            },
            animation: { duration: 0 }
        }
    });
}

function applyWorkerFilter() {
    const selected = document.getElementById('worker-filter').value;
    const filtered = selected ? allShares.filter(s => s.worker === selected) : allShares;
    buildDiffChart(filtered);
    renderSharesTable(filtered);
}

function renderSharesTable(shares) {
    const sTbody = document.querySelector('#shares-table tbody');
    const display = shares.slice(0, 200);
    if (display.length === 0) {
        sTbody.innerHTML = '<tr><td colspan="5" class="empty-msg">No shares found</td></tr>';
    } else {
        sTbody.innerHTML = display.map(s =>
            '<tr class="' + (s.is_block ? 'block-row' : '') + '">' +
            '<td>' + s.time + '</td>' +
            '<td>' + s.worker + '</td>' +
            '<td>' + (s.session_id ? s.session_id.substring(0, 8) : '--') + '</td>' +
            '<td>' + s.difficulty.toFixed(4) + '</td>' +
            '<td>' + (s.is_block ? 'BLOCK' : '') + '</td>' +
            '</tr>'
        ).join('');
    }
}

async function fetchDiagnostics() {
    const addr = getAddress();
    if (!addr) {
        document.getElementById('loading-msg').textContent = 'Enter a miner address above to look up diagnostics.';
        return;
    }

    document.getElementById('miner-address').value = addr;

    try {
        const resp = await fetch('/api/miner/' + encodeURIComponent(addr) + '/diagnostics');
        if (!resp.ok) {
            document.getElementById('loading-msg').style.display = 'none';
            document.getElementById('error-msg').style.display = 'block';
            document.getElementById('error-msg').textContent = resp.status === 404 ? 'Miner not found: ' + addr : 'Error loading diagnostics';
            document.getElementById('diag-content').style.display = 'none';
            return;
        }
        const d = await resp.json();

        document.getElementById('loading-msg').style.display = 'none';
        document.getElementById('error-msg').style.display = 'none';
        document.getElementById('diag-content').style.display = 'block';
        document.title = 'Miner: ' + addr.substring(0, 16) + '...';

        // Summary cards
        document.getElementById('hr-1m').textContent = formatHashrate(d.total_hashrate_1m);
        document.getElementById('hr-10m').textContent = formatHashrate(d.total_hashrate_10m);
        document.getElementById('bal-pending').textContent = d.pending_zec.toFixed(8) + ' ' + COIN;
        document.getElementById('bal-paid').textContent = d.paid_zec.toFixed(8) + ' ' + COIN;
        document.getElementById('total-workers').textContent = d.workers.length;
        document.getElementById('blocks-found').textContent = d.blocks_found.length;

        // Live rejection rate from active sessions
        const lss = d.live_session_stats || {active_sessions:0, accepted:0, rejected_low_diff:0, rejected_job_not_found:0, rejected_other:0, rejection_pct:0};
        const totalSubs = lss.accepted + lss.rejected_low_diff + lss.rejected_job_not_found + lss.rejected_other;
        const rejEl = document.getElementById('reject-pct');
        if (lss.active_sessions === 0 || totalSubs === 0) {
            rejEl.textContent = '--';
            rejEl.style.color = '#888';
            rejEl.title = 'No active sessions';
        } else {
            const pct = lss.rejection_pct;
            const color = pct > 10 ? '#fc8181' : pct > 2 ? '#f4b728' : '#48bb78';
            rejEl.textContent = pct.toFixed(1) + '%';
            rejEl.style.color = color;
            rejEl.title = 'sessions=' + lss.active_sessions
                + ' accepted=' + lss.accepted
                + ' low_diff=' + lss.rejected_low_diff
                + ' job_not_found=' + lss.rejected_job_not_found
                + ' other=' + lss.rejected_other;
        }

        // Workers table
        const wTbody = document.querySelector('#workers-table tbody');
        if (d.workers.length === 0) {
            wTbody.innerHTML = '<tr><td colspan="9" class="empty-msg">No workers found</td></tr>';
        } else {
            wTbody.innerHTML = d.workers.map(w =>
                '<tr>' +
                '<td><span class="status-dot ' + (w.is_online ? 'dot-online' : 'dot-offline') + '"></span>' +
                '<span class="' + (w.is_online ? 'status-online' : 'status-offline') + '">' + (w.is_online ? 'Online' : 'Offline') + '</span></td>' +
                '<td><a href="/sessions?worker=' + encodeURIComponent(w.name) + '" style="color:#e0e0e0;text-decoration:none" title="View live sessions">' + w.name + '</a></td>' +
                '<td>' + formatHashrate(w.hashrate_1m) + '</td>' +
                '<td>' + formatHashrate(w.hashrate_10m) + '</td>' +
                '<td>' + (w.current_difficulty != null ? w.current_difficulty.toFixed(4) : '--') + '</td>' +
                '<td>' + w.shares_1m.toLocaleString() + '</td>' +
                '<td>' + w.shares_10m.toLocaleString() + '</td>' +
                '<td>' + w.total_shares.toLocaleString() + '</td>' +
                '<td>' + w.last_seen + '</td>' +
                '</tr>'
            ).join('');
        }

        // Store shares globally and populate worker filter
        allShares = d.recent_shares;
        const workerSet = new Set(allShares.map(s => s.worker));
        allWorkerNames = [...workerSet].sort();
        const filterEl = document.getElementById('worker-filter');
        const prevSelection = filterEl.value;
        filterEl.innerHTML = '<option value="">All Workers</option>' +
            allWorkerNames.map(n => '<option value="' + n + '"' + (n === prevSelection ? ' selected' : '') + '>' + n + '</option>').join('');

        // Apply current filter to chart + shares table
        applyWorkerFilter();

        // Blocks table
        const bTbody = document.querySelector('#blocks-table tbody');
        if (d.blocks_found.length === 0) {
            bTbody.innerHTML = '<tr><td colspan="5" class="empty-msg">No blocks found by this miner</td></tr>';
        } else {
            bTbody.innerHTML = d.blocks_found.map(b =>
                '<tr>' +
                '<td><a href="' + EXPLORER + '/block/' + b.height + '" target="_blank" style="color:#e0e0e0;text-decoration:none" onmouseover="this.style.color=\'#f4b728\'" onmouseout="this.style.color=\'#e0e0e0\'">' + b.height + '</a></td>' +
                '<td title="' + b.hash + '">' + b.hash.substring(0, 16) + '...</td>' +
                '<td>' + b.reward_zec.toFixed(4) + ' ' + COIN + '</td>' +
                '<td class="status-' + b.status + '">' + b.status + '</td>' +
                '<td>' + b.found_at + '</td>' +
                '</tr>'
            ).join('');
        }

        // Payouts table
        const pTbody = document.querySelector('#payouts-table tbody');
        if (d.payouts.length === 0) {
            pTbody.innerHTML = '<tr><td colspan="3" class="empty-msg">No payouts yet</td></tr>';
        } else {
            pTbody.innerHTML = d.payouts.map(p => {
                const txid = p.txid ? p.txid.substring(0, 20) + '...' : '--';
                const txTitle = p.txid || '';
                return '<tr>' +
                    '<td style="color:#48bb78">' + p.amount_zec.toFixed(8) + ' ' + COIN + '</td>' +
                    '<td title="' + txTitle + '">' + txid + '</td>' +
                    '<td>' + p.created_at + '</td>' +
                    '</tr>';
            }).join('');
        }

    } catch (e) {
        document.getElementById('loading-msg').style.display = 'none';
        document.getElementById('error-msg').style.display = 'block';
        document.getElementById('error-msg').textContent = 'Failed to fetch diagnostics: ' + e;
    }
}

document.addEventListener('DOMContentLoaded', () => {
    document.body.style.opacity = '1';
    initCoin();
    fetchDiagnostics();
    refreshTimer = setInterval(fetchDiagnostics, 15000);
});
</script>
</body>
</html>
"##;
