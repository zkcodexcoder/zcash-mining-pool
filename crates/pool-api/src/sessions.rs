use axum::extract::State;
use axum::response::{Html, Json};
use serde::Deserialize;

use crate::handlers::AppState;

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct DiffAdjustment {
    pub secs_since_connect: u64,
    pub difficulty: f64,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct SessionSnapshot {
    pub session_id: String,
    pub worker_name: String,
    pub peer_addr: String,
    pub local_port: u16,
    pub difficulty: f64,
    pub hashrate: f64,
    pub connected_secs: u64,
    pub shares_in_window: u32,
    pub smoothed_ratio: f64,
    pub window_elapsed_secs: f64,
    #[serde(default)]
    pub diff_history: Vec<DiffAdjustment>,
}

pub async fn get_sessions(State(state): State<AppState>) -> Json<Vec<SessionSnapshot>> {
    let snapshots = state.db.get_pool_status("sessions_snapshot").await
        .ok()
        .flatten()
        .and_then(|(json, _)| serde_json::from_str::<Vec<SessionSnapshot>>(&json).ok())
        .unwrap_or_default();
    Json(snapshots)
}

pub async fn sessions_page() -> Html<&'static str> {
    Html(SESSIONS_HTML)
}

const SESSIONS_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Live Sessions - Zcash Mining Pool</title>
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
        .container { max-width: 1600px; margin: 0 auto; padding: 1rem 1.5rem; }
        .summary {
            display: flex;
            gap: 1px;
            background: #222;
            border: 1px solid #222;
            margin-bottom: 1rem;
        }
        .summary-card {
            background: #111;
            padding: 0.6rem 1rem;
            flex: 1;
        }
        .summary-card .label {
            font-size: 0.55rem;
            text-transform: uppercase;
            letter-spacing: 0.1em;
            color: #555;
        }
        .summary-card .value {
            font-family: 'JetBrains Mono', 'Fira Code', 'Courier New', monospace;
            font-size: 1.1rem;
            font-weight: 700;
            color: #f4b728;
        }
        .table-wrap {
            border: 1px solid #222;
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
            white-space: nowrap;
        }
        td {
            font-family: 'JetBrains Mono', 'Fira Code', 'Courier New', monospace;
            font-size: 0.75rem;
            padding: 0.4rem 0.75rem;
            border-bottom: 1px solid #1a1a1a;
            color: #999;
            white-space: nowrap;
        }
        tr:hover td { background: #151515; }
        .updated {
            font-size: 0.6rem;
            color: #333;
            text-align: right;
            padding: 0.5rem 0;
        }
        .diff-up { color: #48bb78; }
        .diff-down { color: #fc8181; }
        .worker-link { color: #f4b728; text-decoration: none; }
        .worker-link:hover { text-decoration: underline; }
        .expandable { cursor: pointer; }
        .expandable:hover td:first-child { color: #f4b728; }
        .detail-row td { padding: 0; border-bottom: 1px solid #222; }
        .detail-cell { padding: 0.75rem 1rem; background: #0a0a0a; }
        .detail-cell canvas { height: 120px !important; width: 100% !important; }
        @media (max-width: 900px) {
            .summary { flex-direction: column; }
        }
    </style>
</head>
<body style="opacity:0;transition:opacity 0.15s">
<div class="header">
    <h1>LIVE SESSIONS</h1>
    <span class="badge" id="network-badge">Testnet</span>
    <div class="header-right">
        <a href="/" class="header-link">Dashboard</a>
        <a href="/network" class="header-link">Network</a>
        <a href="/zallet" class="header-link">Wallet</a>
    </div>
</div>
<div class="container">
    <div class="summary">
        <div class="summary-card">
            <div class="label">Active Sessions</div>
            <div class="value" id="session-count">--</div>
        </div>
        <div class="summary-card">
            <div class="label">Total Hashrate</div>
            <div class="value" id="total-hashrate">--</div>
        </div>
        <div class="summary-card">
            <div class="label">Unique IPs</div>
            <div class="value" id="unique-ips">--</div>
        </div>
    </div>
    <div class="table-wrap">
        <table>
            <thead>
                <tr>
                    <th>Session</th>
                    <th>Worker</th>
                    <th>IP</th>
                    <th>Port</th>
                    <th>Difficulty</th>
                    <th>Hashrate</th>
                    <th>Connected</th>
                    <th>Shares/Window</th>
                    <th>Window (s)</th>
                    <th>Ratio</th>
                </tr>
            </thead>
            <tbody id="session-body"></tbody>
        </table>
    </div>
    <div class="updated" id="updated"></div>
</div>
<script>
let prevDiffs = {};
let COIN = 'TAZ';
let sessionCharts = {};
let expandedSessions = new Set();
const params = new URLSearchParams(window.location.search);
const filterWorker = params.get('worker');

async function initCoin() {
    try {
        const r = await fetch('/api/pool/info');
        const d = await r.json();
        COIN = d.coin || 'TAZ';
        const badge = document.getElementById('network-badge');
        if (badge) badge.textContent = d.network === 'mainnet' ? 'Mainnet' : 'Testnet';
    } catch(e) {}
}

function formatHashrate(h) {
    if (h == null || isNaN(h) || h <= 0) return '--';
    if (h >= 1e12) return (h / 1e12).toFixed(2) + ' TH/s';
    if (h >= 1e9) return (h / 1e9).toFixed(2) + ' GH/s';
    if (h >= 1e6) return (h / 1e6).toFixed(2) + ' MH/s';
    if (h >= 1e3) return (h / 1e3).toFixed(2) + ' KH/s';
    return h.toFixed(1) + ' H/s';
}

function formatDuration(secs) {
    if (secs < 60) return secs + 's';
    if (secs < 3600) return Math.floor(secs/60) + 'm ' + (secs%60) + 's';
    const h = Math.floor(secs/3600);
    const m = Math.floor((secs%3600)/60);
    return h + 'h ' + m + 'm';
}

function formatDifficulty(d) {
    if (d >= 1e6) return (d / 1e6).toFixed(2) + 'M';
    if (d >= 1e3) return (d / 1e3).toFixed(1) + 'K';
    return d.toFixed(2);
}

function getMinerAddr(workerName) {
    return workerName.split('.')[0];
}

async function fetchSessions() {
    try {
        const r = await fetch('/api/sessions');
        let sessions = await r.json();

        if (filterWorker) {
            sessions = sessions.filter(s => {
                // Match full worker_name, or the worker suffix after the dot
                const parts = s.worker_name.split('.');
                const suffix = parts.length > 1 ? parts.slice(1).join('.') : s.worker_name;
                const addr = parts[0];
                return s.worker_name === filterWorker || suffix === filterWorker || addr === filterWorker;
            });
        }

        document.getElementById('session-count').textContent = sessions.length;

        const totalHr = sessions.reduce((s, x) => s + (x.hashrate || 0), 0);
        document.getElementById('total-hashrate').textContent = formatHashrate(totalHr);

        const ips = new Set(sessions.map(s => s.peer_addr.split(':')[0]));
        document.getElementById('unique-ips').textContent = ips.size;

        const tbody = document.getElementById('session-body');
        if (sessions.length === 0) {
            tbody.innerHTML = '<tr><td colspan="10" style="text-align:center;color:#333;padding:2rem">No active sessions</td></tr>';
        } else {
            // Sort by difficulty descending
            sessions.sort((a, b) => b.difficulty - a.difficulty);
            // Destroy all existing charts before rebuilding DOM
            for (const sid of Object.keys(sessionCharts)) {
                try { sessionCharts[sid].destroy(); } catch(e) {}
                delete sessionCharts[sid];
            }

            let html = '';
            for (const s of sessions) {
                const prev = prevDiffs[s.session_id];
                let diffClass = '';
                if (prev != null) {
                    if (s.difficulty > prev * 1.01) diffClass = 'diff-up';
                    else if (s.difficulty < prev * 0.99) diffClass = 'diff-down';
                }
                prevDiffs[s.session_id] = s.difficulty;
                const addr = getMinerAddr(s.worker_name);
                const shortWorker = s.worker_name.length > 20
                    ? s.worker_name.substring(0, 8) + '...' + s.worker_name.slice(-8)
                    : s.worker_name;
                const ratioColor = s.smoothed_ratio > 1.5 ? '#fc8181'
                    : s.smoothed_ratio < 0.6 ? '#fc8181'
                    : s.smoothed_ratio > 1.2 ? '#f4b728'
                    : s.smoothed_ratio < 0.8 ? '#f4b728'
                    : '#48bb78';
                const expanded = expandedSessions.has(s.session_id);
                const arrow = expanded ? '&#9660;' : '&#9654;';
                html += '<tr class="expandable" onclick="toggleSession(\'' + s.session_id + '\')">' +
                    '<td>' + arrow + ' ' + s.session_id.substring(0, 8) + '</td>' +
                    '<td><a class="worker-link" href="/miner/' + encodeURIComponent(addr) + '" title="' + s.worker_name + '" onclick="event.stopPropagation()">' + shortWorker + '</a></td>' +
                    '<td>' + s.peer_addr.split(':')[0] + '</td>' +
                    '<td>' + s.local_port + '</td>' +
                    '<td class="' + diffClass + '">' + formatDifficulty(s.difficulty) + '</td>' +
                    '<td>' + formatHashrate(s.hashrate) + '</td>' +
                    '<td>' + formatDuration(s.connected_secs) + '</td>' +
                    '<td>' + s.shares_in_window + '</td>' +
                    '<td>' + s.window_elapsed_secs.toFixed(1) + '</td>' +
                    '<td style="color:' + ratioColor + '">' + s.smoothed_ratio.toFixed(2) + '</td>' +
                    '</tr>';
                if (expanded) {
                    if (s.diff_history && s.diff_history.length > 1) {
                        html += '<tr class="detail-row"><td colspan="10"><div class="detail-cell">' +
                            '<canvas id="chart-' + s.session_id + '"></canvas></div></td></tr>';
                    } else {
                        html += '<tr class="detail-row"><td colspan="10"><div class="detail-cell" style="color:#333;font-size:0.75rem;text-align:center;padding:1rem">' +
                            'No difficulty adjustments — steady state since connect</div></td></tr>';
                    }
                }
            }
            tbody.innerHTML = html;

            // Render charts for expanded sessions
            for (const s of sessions) {
                if (expandedSessions.has(s.session_id) && s.diff_history && s.diff_history.length > 1) {
                    renderDiffChart(s.session_id, s.diff_history);
                }
            }
        }

        // Clean stale diffs
        const activeIds = new Set(sessions.map(s => s.session_id));
        for (const k of Object.keys(prevDiffs)) {
            if (!activeIds.has(k)) delete prevDiffs[k];
        }

        document.getElementById('updated').textContent = 'Updated: ' + new Date().toLocaleTimeString();
    } catch(e) {
        console.error('Failed to fetch sessions:', e);
    }
}

function toggleSession(sid) {
    if (expandedSessions.has(sid)) {
        expandedSessions.delete(sid);
        if (sessionCharts[sid]) { sessionCharts[sid].destroy(); delete sessionCharts[sid]; }
    } else {
        expandedSessions.add(sid);
    }
    fetchSessions();
}

function renderDiffChart(sid, history) {
    const el = document.getElementById('chart-' + sid);
    if (!el) return;
    sessionCharts[sid] = new Chart(el, {
        type: 'line',
        data: {
            labels: history.map(h => formatDuration(h.secs_since_connect)),
            datasets: [{
                label: 'Difficulty',
                data: history.map(h => h.difficulty),
                borderColor: '#f4b728',
                borderWidth: 1.5,
                fill: false,
                pointRadius: 2,
                pointBackgroundColor: '#f4b728',
                tension: 0.2,
            }]
        },
        options: {
            responsive: true,
            maintainAspectRatio: false,
            plugins: {
                legend: { display: false },
                tooltip: {
                    backgroundColor: '#1a1a1a',
                    titleColor: '#888',
                    bodyColor: '#ccc',
                    borderColor: '#333',
                    borderWidth: 1,
                    callbacks: {
                        label: (ctx) => 'Diff: ' + formatDifficulty(ctx.parsed.y)
                    }
                }
            },
            scales: {
                x: {
                    display: true,
                    grid: { color: '#1a1a1a' },
                    ticks: { color: '#333', font: { size: 9 }, maxTicksLimit: 10 }
                },
                y: {
                    display: true,
                    grid: { color: '#1a1a1a' },
                    ticks: { color: '#333', font: { size: 9 },
                        callback: (v) => formatDifficulty(v)
                    }
                }
            },
            animation: { duration: 0 }
        }
    });
}

document.addEventListener('DOMContentLoaded', () => {
    document.body.style.opacity = '1';
    initCoin();
    if (filterWorker) {
        const banner = document.createElement('div');
        banner.style.cssText = 'background:#1a1a1a;border:1px solid #333;padding:0.5rem 1rem;margin-bottom:1rem;font-size:0.75rem;display:flex;align-items:center;gap:0.75rem';
        banner.innerHTML = '<span style="color:#555">Filtered:</span> <span style="color:#f4b728;font-family:monospace">' +
            (filterWorker.length > 30 ? filterWorker.substring(0,12) + '...' + filterWorker.slice(-12) : filterWorker) +
            '</span> <a href="/sessions" style="color:#555;margin-left:auto;font-size:0.65rem;text-transform:uppercase;letter-spacing:0.06em;text-decoration:none">Show All</a>';
        document.querySelector('.container').insertBefore(banner, document.querySelector('.summary'));
    }
    fetchSessions();
    setInterval(fetchSessions, 5000);
});
</script>
</body>
</html>
"##;
