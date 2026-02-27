// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 21 — "Tempest" — Premium dashboard with canvas gauges, sparklines,
// VFD text glow, status LEDs, record tracking, time-series charts
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW21: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TAZ Mining Pool — Tempest</title>
<style>
:root {
  --bg0: #040510;
  --bg1: #0b1433;
  --panel: rgba(6, 10, 28, 0.78);
  --line: rgba(0, 229, 255, 0.26);
  --ink: #e8f6ff;
  --muted: #9cb6d4;
  --accentA: #00e5ff;
  --accentB: #ffe600;
  --accentC: #39ff14;
  --accentD: #ff2bd6;
  --neonRed: #ff2a55;
}
*{margin:0;padding:0;box-sizing:border-box}
html,body{background:var(--bg0);color:var(--ink);min-height:100vh;overflow-x:hidden}
body{font-family:'Avenir Next','Segoe UI','Helvetica Neue',sans-serif;font-size:13px;position:relative;isolation:isolate}

/* ── Background overlays ── */
#bgOverlay{position:fixed;top:0;left:0;width:100vw;height:100vh;z-index:-1;pointer-events:none;
  background:
    radial-gradient(1300px 650px at 6% -8%, rgba(0,229,255,0.14), transparent 58%),
    radial-gradient(950px 540px at 102% -4%, rgba(255,43,214,0.12), transparent 56%),
    radial-gradient(900px 500px at 52% 112%, rgba(57,255,20,0.08), transparent 62%);
}

main{max-width:1500px;margin:0 auto;padding:16px 20px;display:grid;gap:14px;position:relative;z-index:2}

/* ── Header bar ── */
.top{display:flex;justify-content:space-between;align-items:center;gap:12px;border-radius:14px;padding:12px 18px;
  position:relative;overflow:hidden;
  background:radial-gradient(120% 160% at 0% 0%, rgba(0,229,255,0.10), transparent 58%),
    radial-gradient(140% 150% at 100% 0%, rgba(255,43,214,0.10), transparent 58%),
    linear-gradient(155deg, rgba(7,12,30,0.93), rgba(7,10,27,0.87));
  border:1px solid var(--line);
  box-shadow:inset 0 0 0 1px rgba(0,229,255,0.06),0 0 26px rgba(0,229,255,0.18),0 0 36px rgba(255,43,214,0.12);
}
.top::before{content:"";position:absolute;inset:0;border-radius:inherit;padding:1px;
  background:linear-gradient(110deg,rgba(0,229,255,0.95),rgba(57,255,20,0.9),rgba(255,43,214,0.95),rgba(255,230,0,0.9));
  background-size:250% 250%;opacity:0.3;pointer-events:none;
  -webkit-mask:linear-gradient(#000 0 0) content-box,linear-gradient(#000 0 0);-webkit-mask-composite:xor;mask-composite:exclude;
}
.title{display:flex;flex-direction:column;gap:2px;z-index:2}
.title h1{font-size:22px;font-weight:700;color:#f5fbff;letter-spacing:0.5px;
  text-shadow:0 0 14px rgba(0,229,255,0.45),0 0 28px rgba(57,255,20,0.24)}
.subtitle{color:var(--muted);font-size:12px;text-shadow:0 0 9px rgba(0,229,255,0.14)}
.top-right{display:flex;align-items:center;gap:10px;z-index:2}
.status{display:inline-flex;align-items:center;gap:9px;font-size:12px;border:1px solid rgba(0,229,255,0.33);
  padding:6px 12px;border-radius:999px;background:rgba(2,6,23,0.80);
  box-shadow:inset 0 0 14px rgba(0,229,255,0.18),0 0 16px rgba(0,229,255,0.16)}
.dot{width:10px;height:10px;border-radius:50%;background:var(--accentC);
  box-shadow:0 0 0 7px rgba(57,255,20,0.2),0 0 14px rgba(57,255,20,0.5)}
.nav-link{color:var(--muted);font-size:12px;text-decoration:none;letter-spacing:0.4px;text-transform:uppercase;transition:color 0.2s}
.nav-link:hover{color:var(--accentA)}

/* ── Cards grid (12-column) ── */
.cards{display:grid;grid-template-columns:repeat(12,minmax(0,1fr));gap:10px}
.card{position:relative;overflow:hidden;grid-column:span 2;border-radius:14px;padding:8px 12px;min-height:44px;
  display:flex;flex-direction:column;justify-content:flex-start;gap:4px;
  background:radial-gradient(120% 160% at 0% 0%,rgba(0,229,255,0.08),transparent 58%),
    radial-gradient(140% 150% at 100% 0%,rgba(255,43,214,0.08),transparent 58%),
    linear-gradient(155deg,rgba(7,12,30,0.93),rgba(7,10,27,0.87));
  border:2px solid var(--card-border,rgba(100,180,255,0.45));
  box-shadow:inset 0 0 0 1px rgba(100,180,255,0.06),0 0 18px var(--card-glow,rgba(100,180,255,0.18)),0 0 32px var(--card-glow,rgba(100,180,255,0.18));
}
.label{color:#a5bfde;font-size:11px;text-transform:uppercase;letter-spacing:0.8px;text-shadow:0 0 8px rgba(0,229,255,0.18);z-index:2}
.value{font-family:'Courier New',monospace;font-size:26px;line-height:1;font-weight:400;letter-spacing:0.3px;
  color:#c8e8ff;z-index:2;
  text-shadow:0 0 5px rgba(100,160,255,0.7),0 0 14px rgba(60,120,255,0.55),0 0 28px rgba(40,80,255,0.45),0 0 48px rgba(30,60,220,0.3);
  filter:drop-shadow(0 0 8px rgba(60,130,255,0.5)) drop-shadow(0 0 18px rgba(40,80,255,0.35));
}
.value-inline{display:inline-flex;align-items:center;gap:8px}

/* ── Gauge cards ── */
.gauge-card{grid-column:span 3;grid-row:span 2;min-height:220px}
.gauge-card canvas{border:none;background:transparent;box-shadow:none;width:100%;height:auto}

/* ── Mini-metric cards with sparklines ── */
.mini-metric-card .mini-inline{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1.5fr);align-items:start;column-gap:8px;min-height:18px}
.mini-metric-card .mini-canvas{width:100%;max-width:100%;height:60px;border-radius:8px;border:none;
  background:linear-gradient(180deg,rgba(2,6,23,0.40),rgba(2,6,23,0.55)),
    repeating-linear-gradient(0deg,rgba(120,150,210,0.035) 0px,rgba(120,150,210,0.035) 1px,transparent 1px,transparent 4px);
  box-shadow:inset 0 0 14px rgba(0,229,255,0.10),0 0 12px rgba(0,229,255,0.09);
  z-index:2;flex:0 0 auto;justify-self:end;
}
.mini-metric-card .value{font-size:20px}

/* ── Status LED dots ── */
.metric-led{width:10px;height:10px;border-radius:50%;background:var(--accentC);
  box-shadow:0 0 0 4px rgba(57,255,20,0.22),0 0 12px rgba(57,255,20,0.6);flex:0 0 auto}

/* ── Record rows ── */
.record-row{display:flex;align-items:baseline;gap:6px;margin-top:2px;z-index:2}
.record-label{font-size:9px;color:var(--muted);letter-spacing:0.5px;text-transform:uppercase}
.record-value{font-family:'Courier New',monospace;font-size:10px;color:#c8e8ff;letter-spacing:0.3px;
  text-shadow:0 0 5px rgba(100,160,255,0.7),0 0 14px rgba(60,120,255,0.55);
  filter:drop-shadow(0 0 6px rgba(60,130,255,0.5))}

/* ── Chart panels ── */
.charts{display:grid;grid-template-columns:repeat(2,minmax(300px,1fr));gap:14px}
.panel{border-radius:14px;padding:14px;min-height:200px;display:flex;flex-direction:column;gap:6px;
  position:relative;overflow:hidden;
  background:radial-gradient(120% 160% at 0% 0%,rgba(0,229,255,0.08),transparent 58%),
    radial-gradient(140% 150% at 100% 0%,rgba(255,43,214,0.08),transparent 58%),
    linear-gradient(155deg,rgba(7,12,30,0.93),rgba(7,10,27,0.87));
  border:1px solid var(--line);
  box-shadow:inset 0 0 0 1px rgba(0,229,255,0.06),0 0 26px rgba(0,229,255,0.18),0 0 36px rgba(255,43,214,0.12);
}
.panel::before{content:"";position:absolute;inset:0;border-radius:inherit;padding:1px;
  background:linear-gradient(110deg,rgba(0,229,255,0.95),rgba(57,255,20,0.9),rgba(255,43,214,0.95),rgba(255,230,0,0.9));
  background-size:250% 250%;opacity:0.2;pointer-events:none;
  -webkit-mask:linear-gradient(#000 0 0) content-box,linear-gradient(#000 0 0);-webkit-mask-composite:xor;mask-composite:exclude;
}
.panel h2{margin:0;font-size:15px;font-weight:640;letter-spacing:0.35px;color:#effbff;z-index:2;
  text-shadow:0 0 10px rgba(0,229,255,0.28),0 0 22px rgba(255,43,214,0.18)}
.legend{display:flex;flex-wrap:wrap;gap:12px;color:#adc4df;font-size:11px;z-index:2}
.legend .sw{width:10px;height:10px;border-radius:2px;display:inline-block;margin-right:5px;position:relative;top:1px;box-shadow:0 0 10px currentColor}
.panel canvas{display:block;width:100%;height:160px;border-radius:10px;
  border:1px solid rgba(0,229,255,0.26);
  background:linear-gradient(180deg,rgba(2,6,23,0.55),rgba(2,6,23,0.65)),
    repeating-linear-gradient(0deg,rgba(120,150,210,0.04) 0px,rgba(120,150,210,0.04) 1px,transparent 1px,transparent 5px);
  box-shadow:inset 0 0 28px rgba(0,229,255,0.10),0 0 20px rgba(0,229,255,0.12);z-index:2;
}

/* ── Tables ── */
.section-title{font-size:11px;text-transform:uppercase;letter-spacing:1px;color:var(--muted);padding:6px 0;
  text-shadow:0 0 8px rgba(0,229,255,0.14)}
.table-wrap{border:1px solid rgba(0,229,255,0.26);border-radius:10px;overflow:hidden;margin-bottom:14px;
  box-shadow:0 0 16px rgba(0,229,255,0.08)}
table{width:100%;border-collapse:collapse;background:rgba(6,10,28,0.90)}
th{font-size:10px;text-transform:uppercase;letter-spacing:0.8px;color:#7a98b8;padding:8px 12px;text-align:left;
  background:rgba(4,8,22,0.95);border-bottom:1px solid rgba(0,229,255,0.15)}
td{font-family:'Courier New',monospace;font-size:12px;padding:6px 12px;border-bottom:1px solid rgba(0,229,255,0.08);color:#9cb6d4}
tr:hover td{background:rgba(0,229,255,0.04)}
.status-confirmed{color:#39ff14;text-shadow:0 0 8px rgba(57,255,20,0.4)}
.status-pending{color:#ffe600;text-shadow:0 0 8px rgba(255,230,0,0.4)}
.status-orphaned{color:#ff2a55;text-shadow:0 0 8px rgba(255,42,85,0.4)}
.addr-cell{max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.addr-link{color:#c8e8ff;cursor:pointer;text-decoration:none}
.addr-link:hover{color:var(--accentA);text-decoration:underline}

/* ── Miner lookup ── */
.lookup-row{display:flex;gap:2px;margin-bottom:14px}
.lookup-row input{flex:1;padding:8px 12px;background:rgba(6,10,28,0.90);border:1px solid rgba(0,229,255,0.26);
  border-radius:8px 0 0 8px;color:#c8e8ff;font-family:'Courier New',monospace;font-size:12px;outline:none}
.lookup-row input:focus{border-color:rgba(0,229,255,0.6);box-shadow:0 0 12px rgba(0,229,255,0.2)}
.lookup-row input::placeholder{color:#4a6080}
.lookup-row button{padding:8px 16px;background:rgba(0,229,255,0.12);border:1px solid rgba(0,229,255,0.35);
  border-radius:0 8px 8px 0;color:var(--accentA);font-size:11px;text-transform:uppercase;letter-spacing:0.8px;cursor:pointer}
.lookup-row button:hover{background:rgba(0,229,255,0.22);box-shadow:0 0 12px rgba(0,229,255,0.3)}
#miner-info{display:none}
.miner-stats-row{display:grid;grid-template-columns:repeat(3,1fr);gap:10px;margin-bottom:14px}
.miner-stat{background:rgba(6,10,28,0.90);border:1px solid rgba(0,229,255,0.26);border-radius:10px;padding:10px 14px}

/* ── Footer ── */
.status-footer{background:rgba(4,8,22,0.95);border-top:1px solid rgba(0,229,255,0.15);padding:6px 20px;
  display:flex;align-items:center;gap:18px;font-size:11px;color:#5a7a9a;position:fixed;bottom:0;left:0;right:0;z-index:10;
  box-shadow:0 -4px 20px rgba(0,0,0,0.4)}
.sf-item{display:flex;align-items:center;gap:5px}
.sf-dot{width:7px;height:7px;border-radius:50%;display:inline-block}
.sf-dot.ok{background:#39ff14;box-shadow:0 0 6px rgba(57,255,20,0.5)}
.sf-dot.err{background:#ff2a55;box-shadow:0 0 6px rgba(255,42,85,0.5)}
.footer-spacer{height:2.5rem}

/* ── Responsive ── */
@media(max-width:1200px){
  .cards{grid-template-columns:repeat(8,minmax(0,1fr))}
  .gauge-card{grid-column:span 4}
  .charts{grid-template-columns:1fr}
}
@media(max-width:800px){
  .cards{grid-template-columns:repeat(4,minmax(0,1fr))}
  .gauge-card{grid-column:span 4}
  .top{flex-direction:column;align-items:flex-start}
}
</style>
</head>
<body>
<div id="bgOverlay"></div>
<main>
  <!-- ── Header ── -->
  <section class="top">
    <div class="title">
      <h1 id="pool-name">TAZ Mining Pool</h1>
      <div class="subtitle">Tempest Dashboard</div>
    </div>
    <div class="top-right">
      <div class="status"><span class="dot" id="statusDot"></span><span id="statusText">Connected</span></div>
      <a href="/previews" class="nav-link">Themes</a>
      <a href="/" class="nav-link">Classic</a>
      <a href="/zallet" class="nav-link">Wallet</a>
    </div>
  </section>

  <!-- ── Cards Grid ── -->
  <section class="cards">
    <!-- Gauge: Pool Hashrate -->
    <article class="card gauge-card" style="--card-border:rgba(0,229,255,0.55);--card-glow:rgba(0,229,255,0.22)">
      <div class="label">POOL HASHRATE</div>
      <canvas id="gaugeHashrate"></canvas>
    </article>

    <!-- Gauge: 24h Luck -->
    <article class="card gauge-card" style="--card-border:rgba(255,220,40,0.55);--card-glow:rgba(255,200,20,0.22)">
      <div class="label">LUCK (24H)</div>
      <canvas id="gaugeLuck"></canvas>
    </article>

    <!-- Mini: Network Hashrate -->
    <article class="card mini-metric-card" style="--card-border:rgba(50,220,80,0.55);--card-glow:rgba(40,200,60,0.22)">
      <div class="label">NETWORK HASHRATE</div>
      <div class="mini-inline">
        <div class="value" id="mNetHash">--</div>
        <canvas id="sparkNetHash" class="mini-canvas"></canvas>
      </div>
      <div class="record-row"><span class="record-label">Peak:</span><span class="record-value" id="recNetHash">--</span></div>
    </article>

    <!-- Mini: Connected Miners -->
    <article class="card mini-metric-card" style="--card-border:rgba(160,100,255,0.55);--card-glow:rgba(140,80,255,0.22)">
      <div class="label">CONNECTED MINERS</div>
      <div class="mini-inline">
        <div class="value" id="mMiners">0</div>
        <canvas id="sparkMiners" class="mini-canvas"></canvas>
      </div>
      <div class="record-row"><span class="record-label">Peak:</span><span class="record-value" id="recMiners">0</span></div>
    </article>

    <!-- Mini: Blocks Found -->
    <article class="card mini-metric-card" style="--card-border:rgba(255,140,30,0.55);--card-glow:rgba(255,120,20,0.22)">
      <div class="label">BLOCKS FOUND</div>
      <div class="mini-inline">
        <div class="value" id="mBlocks">0</div>
        <canvas id="sparkBlocks" class="mini-canvas"></canvas>
      </div>
      <div class="record-row"><span class="record-label">Last:</span><span class="record-value" id="lastBlockTime">--</span></div>
    </article>

    <!-- Mini: Share Rate -->
    <article class="card mini-metric-card" style="--card-border:rgba(60,130,255,0.55);--card-glow:rgba(40,100,255,0.22)">
      <div class="label">SHARE RATE</div>
      <div class="mini-inline">
        <div class="value" id="mShareRate">0</div>
        <canvas id="sparkShares" class="mini-canvas"></canvas>
      </div>
      <div class="record-row"><span class="record-label">per min</span></div>
    </article>

    <!-- Immature Blocks -->
    <article class="card" style="--card-border:rgba(255,180,60,0.55);--card-glow:rgba(255,160,40,0.22)">
      <div class="label">IMMATURE BLOCKS</div>
      <div class="value value-inline"><span class="metric-led" id="ledImmature"></span><span id="mImmature">0</span></div>
    </article>

    <!-- Pending Payouts -->
    <article class="card" style="--card-border:rgba(220,180,255,0.55);--card-glow:rgba(200,150,255,0.22)">
      <div class="label">PENDING PAYOUTS</div>
      <div class="value value-inline"><span class="metric-led" id="ledPending"></span><span id="mPending">0</span></div>
    </article>

    <!-- Pool Fee -->
    <article class="card" style="--card-border:rgba(100,200,255,0.55);--card-glow:rgba(80,180,255,0.22)">
      <div class="label">POOL FEE</div>
      <div class="value" id="mFee">--</div>
    </article>

    <!-- Network Share 24h -->
    <article class="card" style="--card-border:rgba(255,60,180,0.55);--card-glow:rgba(255,40,160,0.22)">
      <div class="label">NET SHARE (24H)</div>
      <div class="value" id="mPoolPct" style="color:#ffe600;text-shadow:0 0 5px rgba(255,230,0,0.7),0 0 14px rgba(255,200,0,0.55)">--</div>
    </article>

    <!-- Total Shares -->
    <article class="card" style="--card-border:rgba(120,220,60,0.55);--card-glow:rgba(100,200,40,0.22)">
      <div class="label">TOTAL SHARES</div>
      <div class="value" id="mTotalShares">0</div>
    </article>

    <!-- Stratum Port -->
    <article class="card" style="--card-border:rgba(100,160,255,0.55);--card-glow:rgba(80,140,255,0.22)">
      <div class="label">STRATUM PORT</div>
      <div class="value" id="mPort">--</div>
    </article>
  </section>

  <!-- ── Chart Panels ── -->
  <section class="charts">
    <article class="panel">
      <h2>Hashrate History</h2>
      <div class="legend">
        <span><span class="sw" style="background:#00e5ff"></span>Pool</span>
        <span><span class="sw" style="background:#39ff14"></span>Network (scaled)</span>
      </div>
      <canvas id="chartHashrate"></canvas>
    </article>
    <article class="panel">
      <h2>Mining Activity</h2>
      <div class="legend">
        <span><span class="sw" style="background:#ffe600"></span>Shares/min</span>
        <span><span class="sw" style="background:#ff2bd6"></span>Miners</span>
      </div>
      <canvas id="chartMining"></canvas>
    </article>
  </section>

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
      <table id="workers-table"><thead><tr><th>Name</th><th>Last Seen</th></tr></thead><tbody></tbody></table>
    </div>
  </div>

  <!-- ── Tables ── -->
  <div class="section-title">Miners</div>
  <div class="table-wrap">
    <table id="miners-table">
      <thead><tr><th>Address</th><th>1m Avg</th><th>10m Avg</th><th>Workers</th><th>Shares</th><th>Pending</th></tr></thead>
      <tbody><tr><td colspan="6" style="color:#4a6080;font-style:italic">Loading...</td></tr></tbody>
    </table>
  </div>

  <div class="section-title">Recent Blocks</div>
  <div class="table-wrap">
    <table id="blocks-table">
      <thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead>
      <tbody><tr><td colspan="6" style="color:#4a6080;font-style:italic">Loading...</td></tr></tbody>
    </table>
  </div>

  <div class="section-title">Recent Payouts</div>
  <div class="table-wrap">
    <table id="payouts-table">
      <thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead>
      <tbody><tr><td colspan="4" style="color:#4a6080;font-style:italic">Loading...</td></tr></tbody>
    </table>
  </div>

  <div class="footer-spacer"></div>
</main>

<!-- ── Status Footer ── -->
<div class="status-footer">
  <div class="sf-item"><span class="sf-dot ok" id="sf-node-dot"></span><span>Node</span><span id="sf-node-text" style="color:#7a98b8">--</span></div>
  <div class="sf-item"><span class="sf-dot ok" id="sf-wallet-dot"></span><span>Wallet</span><span id="sf-wallet-text" style="color:#7a98b8">--</span></div>
  <div class="sf-item"><span>Last Template</span><span id="sf-template-text" style="color:#7a98b8">--</span></div>
  <div style="margin-left:auto" class="sf-item"><span id="sf-uptime">--</span></div>
</div>

<script>
/* ══════════════════════════════════════════════════════════════
 * CONSTANTS & STATE
 * ══════════════════════════════════════════════════════════════ */
const MAX_HIST = 120;
const REFRESH_STATS = 10000;
const REFRESH_TABLES = 30000;

const hist = { hashrate:[], netHash:[], miners:[], blocks:[], shares:[], labels:[] };
let prevShares = null;
const records = { netHash: 0, miners: 0 };
let dashStart = Date.now();

/* ══════════════════════════════════════════════════════════════
 * FORMATTERS
 * ══════════════════════════════════════════════════════════════ */
function fmtHash(h) {
  if (h == null || isNaN(h)) return '--';
  if (h >= 1e12) return (h/1e12).toFixed(2)+' TSol/s';
  if (h >= 1e9)  return (h/1e9).toFixed(2)+' GSol/s';
  if (h >= 1e6)  return (h/1e6).toFixed(2)+' MSol/s';
  if (h >= 1e3)  return (h/1e3).toFixed(2)+' KSol/s';
  return h.toFixed(1)+' Sol/s';
}
function fmtHashShort(h) {
  if (h == null || isNaN(h)) return '--';
  if (h >= 1e12) return (h/1e12).toFixed(1)+'T';
  if (h >= 1e9)  return (h/1e9).toFixed(1)+'G';
  if (h >= 1e6)  return (h/1e6).toFixed(1)+'M';
  if (h >= 1e3)  return (h/1e3).toFixed(1)+'K';
  return h.toFixed(0);
}
function fmtDur(ms) {
  const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);
  if(h>0) return h+'h '+m%60+'m';
  if(m>0) return m+'m '+s%60+'s';
  return s+'s';
}
function pushHist(arr,v){arr.push(v);if(arr.length>MAX_HIST)arr.shift()}

/* ══════════════════════════════════════════════════════════════
 * GAUGE RENDERING — Canvas semicircular gauge with needle
 * ══════════════════════════════════════════════════════════════ */
const gaugeState = {
  hashrate: { current:0, target:0, max:100 },
  luck:     { current:0, target:0, max:200 }
};
let lastGaugeTs = 0;
const GAUGE_TAU = 0.35;

function drawGauge(canvas, value, cfg) {
  if (!canvas) return;
  const W = canvas.clientWidth || 360;
  const H = canvas.clientHeight || 200;
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.floor(W * dpr);
  canvas.height = Math.floor(H * dpr);
  const ctx = canvas.getContext('2d');
  ctx.setTransform(dpr,0,0,dpr,0,0);
  ctx.clearRect(0,0,W,H);

  const minV = cfg.min || 0;
  const maxV = cfg.max || 100;
  const span = Math.max(1e-9, maxV - minV);
  const clamp = v => Math.max(minV, Math.min(maxV, v||0));
  const val = clamp(value);

  const pad = 4;
  const radius = Math.min((W - 2*pad)/2.16, (H - 2*pad)/1.6) * 1.05;
  const cx = W * 0.5;
  const cy = H * 0.52 + radius * 0.08;

  const deg2rad = d => d * Math.PI / 180;
  const startR = deg2rad(135);
  const endR = deg2rad(405);
  const v2a = v => startR + ((clamp(v) - minV)/span) * (endR - startR);

  // Arc ring with threshold coloring
  const arcR = radius * 0.90;
  const arcLW = Math.max(4, radius * 0.045);
  ctx.lineWidth = arcLW;
  ctx.lineCap = 'butt';
  const segs = 100;
  for (let s=0; s<segs; s++) {
    const t = (s+0.5)/segs;
    const vAt = minV + t * span;
    let r,g,b;
    const redEnd = ((cfg.red_max||0)-minV)/span;
    const yelEnd = ((cfg.yellow_max||0)-minV)/span;
    if (t <= redEnd) { r=255;g=58;b=58; }
    else if (t <= yelEnd) {
      const f=(t-redEnd)/Math.max(1e-9,yelEnd-redEnd);
      r=255;g=Math.round(58+f*118);b=Math.round(58-f*14);
    } else {
      const f=(t-yelEnd)/Math.max(1e-9,1-yelEnd);
      r=Math.round(252-f*201);g=Math.round(176+f*43);b=Math.round(44+f*63);
    }
    const a0 = startR + (s/segs)*(endR-startR);
    const a1 = startR + ((s+1)/segs)*(endR-startR);
    ctx.strokeStyle = `rgba(${r},${g},${b},0.85)`;
    ctx.beginPath();
    ctx.arc(cx,cy,arcR,a0,a1+0.01,false);
    ctx.stroke();
  }

  // Major ticks
  const majStep = cfg.major_step || (span/5);
  const tickOuter = radius * 0.87;
  const tickInner = radius * 0.76;
  const labelR = radius * 0.62;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  for (let v = minV; v <= maxV + 0.01; v += majStep) {
    const a = v2a(v);
    const cosA=Math.cos(a), sinA=Math.sin(a);
    // Tick color by zone
    let tc = 'rgba(236,241,247,0.90)';
    if (v <= (cfg.red_max||0)) tc = 'rgba(255,58,58,0.95)';
    else if (v <= (cfg.yellow_max||0)) tc = 'rgba(252,176,44,0.95)';
    else tc = 'rgba(51,219,107,0.95)';
    ctx.strokeStyle = tc;
    ctx.lineWidth = Math.max(2.5, radius*0.028);
    ctx.beginPath();
    ctx.moveTo(cx+tickOuter*cosA, cy+tickOuter*sinA);
    ctx.lineTo(cx+tickInner*cosA, cy+tickInner*sinA);
    ctx.stroke();
    // Label
    ctx.fillStyle = 'rgba(230,235,242,0.82)';
    ctx.font = `${Math.max(9,Math.round(radius*0.10))}px 'Avenir Next','Segoe UI',sans-serif`;
    const lbl = cfg.fmt_label ? cfg.fmt_label(v) : (v >= 1000 ? Math.round(v/1000)+'K' : Math.round(v).toString());
    ctx.fillText(lbl, cx+labelR*cosA, cy+labelR*sinA);
  }

  // Minor ticks
  const minStep = cfg.minor_step || (majStep/5);
  ctx.strokeStyle = 'rgba(180,190,210,0.35)';
  ctx.lineWidth = Math.max(0.8, radius*0.008);
  const minTickInner = radius * 0.82;
  for (let v = minV; v <= maxV + 0.01; v += minStep) {
    if (Math.abs(v % majStep) < minStep*0.5 || Math.abs(v % majStep - majStep) < minStep*0.5) continue;
    const a = v2a(v);
    ctx.beginPath();
    ctx.moveTo(cx+tickOuter*Math.cos(a), cy+tickOuter*Math.sin(a));
    ctx.lineTo(cx+minTickInner*Math.cos(a), cy+minTickInner*Math.sin(a));
    ctx.stroke();
  }

  // Title text
  ctx.fillStyle = 'rgba(232,236,241,0.85)';
  ctx.font = `700 ${Math.max(13,Math.round(radius*0.155))}px 'Avenir Next','Segoe UI',sans-serif`;
  ctx.textAlign = 'center';
  ctx.fillText(cfg.title||'', cx, cy - radius*0.34);

  // Needle
  const needleA = v2a(val);
  const nCos=Math.cos(needleA), nSin=Math.sin(needleA);
  const nLen = radius*0.82;
  const tailLen = radius*0.10;
  const baseHW = Math.max(3.5, radius*0.028);
  const tipX=cx+nLen*nCos, tipY=cy+nLen*nSin;
  const tailX=cx-tailLen*nCos, tailY=cy-tailLen*nSin;
  const perpX=-nSin, perpY=nCos;

  // Needle glow
  ctx.save();
  const glows = [
    {blur:50,color:'rgba(255,140,10,0.65)',fill:'rgba(255,120,10,0.06)'},
    {blur:28,color:'rgba(255,160,20,0.75)',fill:'rgba(255,130,10,0.10)'},
    {blur:12,color:'rgba(255,170,40,0.85)',fill:'rgba(255,140,20,0.14)'},
    {blur:4,color:'rgba(255,190,60,0.95)',fill:'rgba(255,160,30,0.18)'}
  ];
  for (const gl of glows) {
    ctx.shadowColor=gl.color;ctx.shadowBlur=gl.blur;ctx.fillStyle=gl.fill;
    ctx.beginPath();ctx.moveTo(tipX,tipY);
    ctx.lineTo(tailX+perpX*baseHW,tailY+perpY*baseHW);
    ctx.lineTo(tailX-perpX*baseHW,tailY-perpY*baseHW);
    ctx.closePath();ctx.fill();
  }
  ctx.restore();

  // Crisp needle
  const ng = ctx.createLinearGradient(tailX,tailY,tipX,tipY);
  ng.addColorStop(0,'#c85a00');ng.addColorStop(0.6,'#ff8a00');ng.addColorStop(1,'#ffc04d');
  ctx.fillStyle = ng;
  ctx.beginPath();ctx.moveTo(tipX,tipY);
  ctx.lineTo(tailX+perpX*baseHW,tailY+perpY*baseHW);
  ctx.lineTo(tailX-perpX*baseHW,tailY-perpY*baseHW);
  ctx.closePath();ctx.fill();

  // Center hub
  const hubG = ctx.createRadialGradient(cx-2,cy-2,2,cx,cy,radius*0.14);
  hubG.addColorStop(0,'rgba(167,174,180,0.98)');hubG.addColorStop(1,'rgba(38,44,51,0.98)');
  ctx.fillStyle = hubG;
  ctx.beginPath();ctx.arc(cx,cy,radius*0.14,0,Math.PI*2);ctx.fill();
  ctx.fillStyle = 'rgba(8,12,17,0.96)';
  ctx.beginPath();ctx.arc(cx,cy,radius*0.08,0,Math.PI*2);ctx.fill();

  // Value badge
  const bW=radius*0.84, bH=radius*0.40;
  const bX=cx-bW/2, bY=cy+radius*0.40;

  // Badge background
  const badgeFill = ctx.createLinearGradient(0,bY,0,bY+bH);
  badgeFill.addColorStop(0,'rgba(10,4,24,0.92)');badgeFill.addColorStop(1,'rgba(6,2,16,0.96)');
  roundRect(ctx,bX,bY,bW,bH,Math.max(6,radius*0.07));
  ctx.fillStyle = badgeFill;ctx.fill();
  ctx.strokeStyle = 'rgba(0,229,255,0.25)';ctx.lineWidth=1;ctx.stroke();

  // VFD value text with bloom
  const vText = cfg.fmt_value ? cfg.fmt_value(value) : value.toFixed(cfg.decimals||0);
  const vFont = `400 ${Math.max(16,Math.round(radius*0.36))}px 'Courier New',monospace`;
  ctx.font = vFont;ctx.textAlign='center';ctx.textBaseline='middle';
  const ledX = cx, ledY = bY + bH*0.5;

  // Multi-layer bloom
  const bloomLayers = [
    {fill:'rgba(100,200,255,0.35)',shadow:'rgba(0,229,255,0.70)',blur:Math.max(40,radius*0.5)},
    {fill:'rgba(120,210,255,0.50)',shadow:'rgba(0,229,255,0.80)',blur:Math.max(22,radius*0.3)},
    {fill:'rgba(150,220,255,0.65)',shadow:'rgba(0,229,255,0.90)',blur:Math.max(10,radius*0.15)},
    {fill:'rgba(180,235,255,0.80)',shadow:'rgba(0,229,255,0.95)',blur:Math.max(4,radius*0.06)}
  ];
  for (const l of bloomLayers) {
    ctx.fillStyle=l.fill;ctx.shadowColor=l.shadow;ctx.shadowBlur=l.blur;
    ctx.fillText(vText,ledX,ledY);
  }
  ctx.fillStyle='#c8e8ff';ctx.shadowBlur=0;ctx.fillText(vText,ledX,ledY);

  // Sub text
  if (cfg.sub_text) {
    ctx.font = `${Math.max(9,Math.round(radius*0.09))}px 'Avenir Next','Segoe UI',sans-serif`;
    ctx.fillStyle = 'rgba(156,182,212,0.70)';
    ctx.fillText(cfg.sub_text, cx, bY + bH + Math.max(10,radius*0.10));
  }
}

function roundRect(ctx,x,y,w,h,r) {
  const rr=Math.min(r,w/2,h/2);
  ctx.beginPath();ctx.moveTo(x+rr,y);ctx.arcTo(x+w,y,x+w,y+h,rr);ctx.arcTo(x+w,y+h,x,y+h,rr);
  ctx.arcTo(x,y+h,x,y,rr);ctx.arcTo(x,y,x+w,y,rr);ctx.closePath();
}

// Gauge animation loop
function gaugeLoop(ts) {
  if (!lastGaugeTs) lastGaugeTs = ts;
  const dt = Math.min((ts - lastGaugeTs)/1000, 0.25);
  lastGaugeTs = ts;
  const alpha = 1 - Math.exp(-dt/GAUGE_TAU);
  let redraw = false;
  for (const g of Object.values(gaugeState)) {
    const diff = g.target - g.current;
    if (Math.abs(diff) > 0.05) { g.current += diff * alpha; redraw = true; }
    else if (g.current !== g.target) { g.current = g.target; redraw = true; }
  }
  if (redraw) {
    const gs = gaugeState;
    drawGauge(document.getElementById('gaugeHashrate'), gs.hashrate.current, {
      min:0, max:gs.hashrate.max, red_max:gs.hashrate.max*0.15, yellow_max:gs.hashrate.max*0.4,
      major_step:gs.hashrate.max/5, minor_step:gs.hashrate.max/25,
      title:'SOL/s', fmt_value:v=>fmtHashShort(v), fmt_label:v=>fmtHashShort(v),
      sub_text: 'Pool Hashrate'
    });
    drawGauge(document.getElementById('gaugeLuck'), gs.luck.current, {
      min:0, max:200, red_max:150, yellow_max:100,
      major_step:50, minor_step:10,
      title:'LUCK', decimals:0, fmt_value:v=>Math.round(v)+'%', fmt_label:v=>v+'%',
      sub_text: '24-Hour Average'
    });
  }
  requestAnimationFrame(gaugeLoop);
}
// Initial draw at zero
drawGauge(document.getElementById('gaugeHashrate'), 0, {min:0,max:100,red_max:15,yellow_max:40,major_step:20,minor_step:5,title:'SOL/s',fmt_value:v=>fmtHashShort(v),fmt_label:v=>fmtHashShort(v),sub_text:'Pool Hashrate'});
drawGauge(document.getElementById('gaugeLuck'), 0, {min:0,max:200,red_max:150,yellow_max:100,major_step:50,minor_step:10,title:'LUCK',decimals:0,fmt_value:v=>Math.round(v)+'%',fmt_label:v=>v+'%',sub_text:'24-Hour Average'});
requestAnimationFrame(gaugeLoop);

/* ══════════════════════════════════════════════════════════════
 * SPARKLINE RENDERING — Inline mini-charts on canvas
 * ══════════════════════════════════════════════════════════════ */
function drawSparkline(canvas, data, color, fillAlpha) {
  if (!canvas || !data.length) return;
  const W = canvas.clientWidth || 120;
  const H = canvas.clientHeight || 60;
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.floor(W*dpr);
  canvas.height = Math.floor(H*dpr);
  const ctx = canvas.getContext('2d');
  ctx.setTransform(dpr,0,0,dpr,0,0);
  ctx.clearRect(0,0,W,H);

  const pad = 2;
  const pW = W-2*pad, pH = H-2*pad;
  let min = Infinity, max = -Infinity;
  for (const v of data) { if(v<min)min=v; if(v>max)max=v; }
  if (min===max) { min-=1; max+=1; }
  const range = max - min;

  // Draw gradient fill
  ctx.beginPath();
  for (let i=0; i<data.length; i++) {
    const x = pad + (i/(data.length-1))*pW;
    const y = pad + (1 - (data[i]-min)/range)*pH;
    if (i===0) ctx.moveTo(x,y); else ctx.lineTo(x,y);
  }
  const fillPath = new Path2D();
  fillPath.moveTo(pad, pad+(1-(data[0]-min)/range)*pH);
  for (let i=0; i<data.length; i++) {
    fillPath.lineTo(pad+(i/(data.length-1))*pW, pad+(1-(data[i]-min)/range)*pH);
  }
  fillPath.lineTo(pad+pW, pad+pH);
  fillPath.lineTo(pad, pad+pH);
  fillPath.closePath();
  const grad = ctx.createLinearGradient(0,pad,0,pad+pH);
  grad.addColorStop(0, color.replace(')',`,${fillAlpha||0.25})`).replace('rgb','rgba'));
  grad.addColorStop(1, 'rgba(0,0,0,0)');
  ctx.fillStyle = grad;
  ctx.fill(fillPath);

  // Draw line
  ctx.beginPath();
  for (let i=0; i<data.length; i++) {
    const x = pad + (i/(data.length-1))*pW;
    const y = pad + (1 - (data[i]-min)/range)*pH;
    if (i===0) ctx.moveTo(x,y); else ctx.lineTo(x,y);
  }
  ctx.strokeStyle = color;
  ctx.lineWidth = 1.5;
  ctx.shadowColor = color;
  ctx.shadowBlur = 8;
  ctx.stroke();
  ctx.shadowBlur = 0;

  // Current value dot
  if (data.length > 0) {
    const lastX = pad + pW;
    const lastY = pad + (1 - (data[data.length-1]-min)/range)*pH;
    ctx.beginPath();
    ctx.arc(lastX, lastY, 3, 0, Math.PI*2);
    ctx.fillStyle = color;
    ctx.shadowColor = color;
    ctx.shadowBlur = 10;
    ctx.fill();
    ctx.shadowBlur = 0;
  }
}

/* ══════════════════════════════════════════════════════════════
 * CHART RENDERING — Time-series line charts with gradient fill
 * ══════════════════════════════════════════════════════════════ */
function drawChart(canvas, datasets, labels) {
  if (!canvas) return;
  const W = canvas.clientWidth || 600;
  const H = canvas.clientHeight || 160;
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.floor(W*dpr);
  canvas.height = Math.floor(H*dpr);
  const ctx = canvas.getContext('2d');
  ctx.setTransform(dpr,0,0,dpr,0,0);
  ctx.clearRect(0,0,W,H);

  const padL=42, padR=12, padT=10, padB=24;
  const pW=W-padL-padR, pH=H-padT-padB;
  if (pW<=20||pH<=20) return;

  // Find global min/max across all datasets
  let gMin=Infinity, gMax=-Infinity;
  for (const ds of datasets) {
    for (const v of ds.data) { if(v<gMin)gMin=v; if(v>gMax)gMax=v; }
  }
  if (gMin===gMax) { gMin-=1; gMax+=1; }
  const range = gMax - gMin;

  // Grid lines
  ctx.strokeStyle = 'rgba(0,229,255,0.08)';
  ctx.lineWidth = 0.5;
  const gridSteps = 4;
  for (let i=0; i<=gridSteps; i++) {
    const y = padT + (i/gridSteps)*pH;
    ctx.beginPath();ctx.moveTo(padL,y);ctx.lineTo(padL+pW,y);ctx.stroke();
    // Y labels
    const val = gMax - (i/gridSteps)*range;
    ctx.fillStyle = 'rgba(156,182,212,0.5)';
    ctx.font = '9px sans-serif';
    ctx.textAlign = 'right';
    ctx.textBaseline = 'middle';
    ctx.fillText(fmtHashShort(val), padL-4, y);
  }

  // X labels
  if (labels.length > 1) {
    ctx.fillStyle = 'rgba(156,182,212,0.4)';
    ctx.font = '9px sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'top';
    const step = Math.max(1, Math.floor(labels.length/6));
    for (let i=0; i<labels.length; i+=step) {
      const x = padL + (i/(labels.length-1))*pW;
      ctx.fillText(labels[i], x, padT+pH+4);
    }
  }

  // Draw each dataset
  for (const ds of datasets) {
    if (!ds.data.length) continue;
    const n = ds.data.length;

    // Gradient fill
    const fillPath = new Path2D();
    fillPath.moveTo(padL, padT+(1-(ds.data[0]-gMin)/range)*pH);
    for (let i=0;i<n;i++) {
      fillPath.lineTo(padL+(i/(n-1))*pW, padT+(1-(ds.data[i]-gMin)/range)*pH);
    }
    fillPath.lineTo(padL+pW, padT+pH);
    fillPath.lineTo(padL, padT+pH);
    fillPath.closePath();
    const grad = ctx.createLinearGradient(0,padT,0,padT+pH);
    grad.addColorStop(0, ds.color.replace(')',',0.15)').replace('rgb','rgba'));
    grad.addColorStop(1, 'rgba(0,0,0,0)');
    ctx.fillStyle = grad;
    ctx.fill(fillPath);

    // Line
    ctx.beginPath();
    for (let i=0;i<n;i++) {
      const x = padL + (i/(n-1))*pW;
      const y = padT + (1 - (ds.data[i]-gMin)/range)*pH;
      if(i===0) ctx.moveTo(x,y); else ctx.lineTo(x,y);
    }
    ctx.strokeStyle = ds.color;
    ctx.lineWidth = 1.5;
    ctx.shadowColor = ds.color;
    ctx.shadowBlur = 6;
    ctx.stroke();
    ctx.shadowBlur = 0;
  }
}

/* ══════════════════════════════════════════════════════════════
 * STATUS LEDs
 * ══════════════════════════════════════════════════════════════ */
function setLed(id, state) {
  const el = document.getElementById(id);
  if (!el) return;
  const colors = {
    green:  {bg:'#39ff14',shadow:'0 0 0 4px rgba(57,255,20,0.22),0 0 12px rgba(57,255,20,0.6)'},
    yellow: {bg:'#ffe600',shadow:'0 0 0 4px rgba(255,230,0,0.22),0 0 12px rgba(255,230,0,0.58)'},
    red:    {bg:'#ff2a55',shadow:'0 0 0 4px rgba(255,42,85,0.24),0 0 12px rgba(255,42,85,0.58)'},
    off:    {bg:'#3a4a5a',shadow:'0 0 0 4px rgba(58,74,90,0.15),0 0 8px rgba(58,74,90,0.25)'}
  };
  const c = colors[state] || colors.off;
  el.style.background = c.bg;
  el.style.boxShadow = c.shadow;
}

function setConnected(ok) {
  const dot = document.getElementById('statusDot');
  const text = document.getElementById('statusText');
  if (ok) {
    dot.style.background = '#39ff14';
    dot.style.boxShadow = '0 0 0 7px rgba(57,255,20,0.22),0 0 16px rgba(57,255,20,0.55)';
    text.textContent = 'Connected';
  } else {
    dot.style.background = '#ff2a55';
    dot.style.boxShadow = '0 0 0 7px rgba(255,42,85,0.24),0 0 16px rgba(255,42,85,0.55)';
    text.textContent = 'Disconnected';
  }
}

/* ══════════════════════════════════════════════════════════════
 * DATA FETCHING
 * ══════════════════════════════════════════════════════════════ */
async function fetchStats() {
  try {
    const r = await fetch('/api/pool/stats');
    const d = await r.json();
    const now = Date.now();

    document.getElementById('pool-name').textContent = (d.name || 'TAZ Mining Pool').toUpperCase();

    // Update gauge targets
    const hr = d.hashrate_estimate || 0;
    gaugeState.hashrate.target = hr;
    // Auto-scale hashrate gauge max
    if (hr > 0) {
      let newMax = 100;
      if (hr <= 100) newMax = 100;
      else if (hr <= 1000) newMax = 1000;
      else if (hr <= 10000) newMax = 10000;
      else if (hr <= 100000) newMax = 100000;
      else if (hr <= 1e6) newMax = 1e6;
      else if (hr <= 1e9) newMax = 1e9;
      else newMax = 1e12;
      gaugeState.hashrate.max = newMax;
    }

    if (d.luck_percent != null) gaugeState.luck.target = d.luck_percent;

    // Metric cards
    document.getElementById('mNetHash').textContent = fmtHash(d.network_hashrate);
    document.getElementById('mMiners').textContent = d.connected_miners || 0;
    document.getElementById('mBlocks').textContent = d.total_blocks || 0;
    document.getElementById('mImmature').textContent = d.immature_blocks || 0;
    document.getElementById('mPending').textContent = d.pending_payout_blocks || 0;
    document.getElementById('mFee').textContent = (d.fee_percent||0) + '%';
    document.getElementById('mPort').textContent = d.stratum_port || '--';
    document.getElementById('mTotalShares').textContent = (d.total_shares||0).toLocaleString();

    const pctEl = document.getElementById('mPoolPct');
    if (d.pool_percent_24h != null) pctEl.textContent = d.pool_percent_24h.toFixed(2) + '%';
    else pctEl.textContent = '--';

    // Share rate
    const sharesPerMin = prevShares !== null ? Math.max(0, (d.total_shares - prevShares) * (60000/REFRESH_STATS)) : 0;
    prevShares = d.total_shares;
    document.getElementById('mShareRate').textContent = Math.round(sharesPerMin);

    // Records
    const nh = d.network_hashrate || 0;
    if (nh > records.netHash) { records.netHash = nh; document.getElementById('recNetHash').textContent = fmtHash(nh); }
    const mc = d.connected_miners || 0;
    if (mc > records.miners) { records.miners = mc; document.getElementById('recMiners').textContent = mc; }

    // LED states
    setLed('ledImmature', (d.immature_blocks||0) > 0 ? 'yellow' : 'green');
    setLed('ledPending', (d.pending_payout_blocks||0) > 0 ? 'yellow' : 'green');

    // Connection status
    const nodeOk = d.node_ok !== false;
    setConnected(nodeOk);
    const sfNodeDot = document.getElementById('sf-node-dot');
    sfNodeDot.className = 'sf-dot ' + (nodeOk ? 'ok' : 'err');
    document.getElementById('sf-node-text').textContent = nodeOk ? 'OK' : 'Stalled';
    const walletOk = d.wallet_ok !== false;
    const sfWalletDot = document.getElementById('sf-wallet-dot');
    sfWalletDot.className = 'sf-dot ' + (walletOk ? 'ok' : 'err');
    document.getElementById('sf-wallet-text').textContent = walletOk ? 'Online' : 'Offline';
    if (d.last_template_at) document.getElementById('sf-template-text').textContent = d.last_template_at.replace('T',' ').slice(0,19);
    document.getElementById('sf-uptime').textContent = 'Up ' + fmtDur(now - dashStart);

    // History
    pushHist(hist.hashrate, hr);
    pushHist(hist.netHash, nh);
    pushHist(hist.miners, mc);
    pushHist(hist.blocks, d.total_blocks||0);
    pushHist(hist.shares, Math.round(sharesPerMin));
    pushHist(hist.labels, now);

    // Update sparklines
    drawSparkline(document.getElementById('sparkNetHash'), hist.netHash, 'rgb(57,255,20)', 0.2);
    drawSparkline(document.getElementById('sparkMiners'), hist.miners, 'rgb(160,100,255)', 0.2);
    drawSparkline(document.getElementById('sparkBlocks'), hist.blocks, 'rgb(255,140,30)', 0.2);
    drawSparkline(document.getElementById('sparkShares'), hist.shares, 'rgb(60,130,255)', 0.2);

    // Update time-series charts
    const timeLabels = hist.labels.map(t => {
      const dt = new Date(t);
      return dt.getHours().toString().padStart(2,'0')+':'+dt.getMinutes().toString().padStart(2,'0')+':'+dt.getSeconds().toString().padStart(2,'0');
    });
    drawChart(document.getElementById('chartHashrate'), [
      {data:[...hist.hashrate], color:'rgb(0,229,255)'},
      {data:hist.netHash.map(v => v ? v / Math.max(1,(Math.max(...hist.netHash)/Math.max(1,Math.max(...hist.hashrate)||1))) : 0), color:'rgb(57,255,20)'}
    ], timeLabels);
    drawChart(document.getElementById('chartMining'), [
      {data:[...hist.shares], color:'rgb(255,230,0)'},
      {data:[...hist.miners], color:'rgb(255,43,214)'}
    ], timeLabels);

  } catch(e) {
    console.error('Stats fetch failed:', e);
    setConnected(false);
  }
}

async function fetchBlocks() {
  try {
    const r = await fetch('/api/blocks');
    const blocks = await r.json();
    const tbody = document.querySelector('#blocks-table tbody');
    if (!blocks.length) { tbody.innerHTML='<tr><td colspan="6" style="color:#4a6080;font-style:italic">No blocks yet</td></tr>'; return; }
    tbody.innerHTML = blocks.map(b => {
      let luckStr='--', luckColor='#5a7a9a';
      if (b.luck_percent!=null) {
        luckStr=b.luck_percent.toFixed(0)+'%';
        luckColor=b.luck_percent<=100?'#39ff14':b.luck_percent<=150?'#ffe600':'#ff2a55';
      }
      return '<tr>'+
        '<td style="color:#c8e8ff">'+b.height+'</td>'+
        '<td title="'+b.hash+'">'+b.hash.substring(0,16)+'...</td>'+
        '<td>'+b.reward_zec.toFixed(4)+' TAZ</td>'+
        '<td style="color:'+luckColor+'">'+luckStr+'</td>'+
        '<td class="status-'+b.status+'">'+b.status+'</td>'+
        '<td>'+b.found_at+'</td></tr>';
    }).join('');

    // Update last block time
    if (blocks.length > 0) {
      document.getElementById('lastBlockTime').textContent = blocks[0].found_at.replace('T',' ').slice(0,19);
    }
  } catch(e) { console.error('Blocks fetch failed:', e); }
}

async function fetchMiners() {
  try {
    const r = await fetch('/api/miners');
    const miners = await r.json();
    const tbody = document.querySelector('#miners-table tbody');
    if (!miners.length) { tbody.innerHTML='<tr><td colspan="6" style="color:#4a6080;font-style:italic">No miners yet</td></tr>'; return; }
    tbody.innerHTML = miners.map(m =>
      '<tr>'+
      '<td class="addr-cell" title="'+m.address+'"><span class="addr-link" onclick="doLookup(\''+m.address.replace(/'/g,"\\'")+'\')">' + m.address + '</span></td>'+
      '<td>'+fmtHash(m.hashrate_1m)+'</td>'+
      '<td>'+fmtHash(m.hashrate)+'</td>'+
      '<td>'+m.worker_count+'</td>'+
      '<td>'+m.share_count.toLocaleString()+'</td>'+
      '<td style="color:#ffe600">'+m.pending_zec.toFixed(8)+' TAZ</td>'+
      '</tr>'
    ).join('');
  } catch(e) { console.error('Miners fetch failed:', e); }
}

async function fetchPayouts() {
  try {
    const r = await fetch('/api/payouts');
    const payouts = await r.json();
    const tbody = document.querySelector('#payouts-table tbody');
    if (!payouts.length) { tbody.innerHTML='<tr><td colspan="4" style="color:#4a6080;font-style:italic">No payouts yet</td></tr>'; return; }
    tbody.innerHTML = payouts.map(p => {
      const txid = p.txid ? p.txid.substring(0,16)+'...' : '--';
      return '<tr>'+
        '<td class="addr-cell" title="'+p.miner_address+'" style="color:#c8e8ff">'+p.miner_address+'</td>'+
        '<td style="color:#39ff14">'+p.amount_zec.toFixed(8)+' TAZ</td>'+
        '<td title="'+(p.txid||'')+'">'+txid+'</td>'+
        '<td>'+p.created_at+'</td></tr>';
    }).join('');
  } catch(e) { console.error('Payouts fetch failed:', e); }
}

function doLookup(addr) { document.getElementById('miner-address').value=addr; lookupMiner(); }

async function lookupMiner() {
  const addr = document.getElementById('miner-address').value.trim();
  if (!addr) return;
  try {
    const r = await fetch('/api/miner/'+encodeURIComponent(addr));
    if (!r.ok) { alert('Miner not found'); return; }
    const data = await r.json();
    document.getElementById('miner-info').style.display = 'block';
    document.getElementById('miner-stats-grid').innerHTML =
      '<div class="miner-stat"><div class="label">Pending Balance</div><div class="value" style="font-size:18px;color:#ffe600">'+data.balance.pending_zec.toFixed(8)+' TAZ</div></div>'+
      '<div class="miner-stat"><div class="label">Total Paid</div><div class="value" style="font-size:18px;color:#39ff14">'+data.balance.paid_zec.toFixed(8)+' TAZ</div></div>'+
      '<div class="miner-stat"><div class="label">Workers</div><div class="value" style="font-size:18px">'+data.workers.length+'</div></div>';
    document.querySelector('#workers-table tbody').innerHTML = data.workers.map(w => '<tr><td>'+w.name+'</td><td>'+w.last_seen+'</td></tr>').join('');
  } catch(e) { console.error('Lookup failed:', e); }
}

/* ── Init ── */
document.addEventListener('DOMContentLoaded', () => {
  fetchStats(); fetchMiners(); fetchBlocks(); fetchPayouts();
  setInterval(fetchStats, REFRESH_STATS);
  setInterval(fetchMiners, REFRESH_STATS);
  setInterval(fetchBlocks, REFRESH_TABLES);
  setInterval(fetchPayouts, REFRESH_TABLES);
});
</script>
</body>
</html>
"##;
