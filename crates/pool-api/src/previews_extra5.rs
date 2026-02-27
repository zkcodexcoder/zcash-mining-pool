// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 22 — "Nexus" — Gold/amber premium with full circular gauges,
// animated ring progress, and warm glow aesthetic
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW22: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TAZ Mining Pool — Nexus</title>
<style>
:root{--bg:#0a0806;--panel:rgba(18,14,8,0.92);--accent:#f4b728;--accent2:#ff8c00;--accent3:#ffcc44;
  --dim:#3d2e10;--ink:#ffe8c8;--muted:#9a8660;--line:rgba(244,183,40,0.15);--glow:rgba(244,183,40,0.18)}
*{margin:0;padding:0;box-sizing:border-box}
html,body{background:var(--bg);color:var(--ink);min-height:100vh;overflow-x:hidden}
body{font-family:'JetBrains Mono','Fira Code','Courier New',monospace;font-size:13px;position:relative}
body::before{content:'';position:fixed;inset:0;z-index:0;pointer-events:none;
  background:radial-gradient(ellipse at 20% 0%,rgba(244,183,40,0.06),transparent 60%),
    radial-gradient(ellipse at 80% 100%,rgba(255,140,0,0.04),transparent 60%)}

main{max-width:1440px;margin:0 auto;padding:16px 20px;position:relative;z-index:1}

/* ── Header ── */
.header{display:flex;align-items:center;gap:12px;padding:10px 16px;margin-bottom:14px;
  background:var(--panel);border:1px solid var(--line);border-radius:12px;
  box-shadow:0 0 20px rgba(244,183,40,0.08)}
.header h1{font-size:16px;font-weight:700;color:var(--accent);letter-spacing:1px;
  text-shadow:0 0 20px rgba(244,183,40,0.4)}
.badge{background:#1a1400;border:1px solid var(--dim);padding:2px 8px;font-size:9px;color:var(--muted);
  text-transform:uppercase;letter-spacing:1px;border-radius:4px}
.header-right{margin-left:auto;display:flex;align-items:center;gap:12px}
.status-dot{width:8px;height:8px;border-radius:50%;display:inline-block}
.status-dot.ok{background:#48bb78;box-shadow:0 0 8px rgba(72,187,120,0.5)}
.status-dot.err{background:#fc8181;box-shadow:0 0 8px rgba(252,129,129,0.5)}
.nav-link{color:var(--muted);font-size:10px;text-decoration:none;text-transform:uppercase;letter-spacing:1px}
.nav-link:hover{color:var(--accent)}

/* ── Grid ── */
.grid{display:grid;grid-template-columns:repeat(12,1fr);gap:10px;margin-bottom:14px}

/* ── Gauge cards ── */
.gauge-card{grid-column:span 3;background:var(--panel);border:1px solid var(--line);border-radius:12px;
  padding:10px;display:flex;flex-direction:column;align-items:center;gap:4px;
  box-shadow:0 0 16px rgba(244,183,40,0.06)}
.gauge-card .label{font-size:10px;text-transform:uppercase;letter-spacing:1px;color:var(--muted);width:100%}
.gauge-card canvas{width:100%;max-width:260px;height:auto;aspect-ratio:1/0.85}

/* ── Metric cards ── */
.card{grid-column:span 2;background:var(--panel);border:1px solid var(--line);border-radius:12px;
  padding:10px 14px;display:flex;flex-direction:column;gap:4px;box-shadow:0 0 12px rgba(244,183,40,0.04)}
.card .label{font-size:9px;text-transform:uppercase;letter-spacing:1px;color:var(--muted)}
.card .val{font-size:22px;font-weight:700;color:var(--accent);line-height:1;
  text-shadow:0 0 12px rgba(244,183,40,0.4)}
.card .sub{font-size:9px;color:var(--muted);margin-top:2px}

/* ── Sparkline cards ── */
.spark-card{grid-column:span 3;background:var(--panel);border:1px solid var(--line);border-radius:12px;
  padding:10px 14px;display:flex;flex-direction:column;gap:4px;box-shadow:0 0 12px rgba(244,183,40,0.04)}
.spark-card .spark-row{display:flex;align-items:center;gap:10px}
.spark-card .val{font-size:20px;font-weight:700;color:var(--accent);white-space:nowrap;
  text-shadow:0 0 10px rgba(244,183,40,0.3)}
.spark-card canvas{flex:1;height:48px;border-radius:6px;
  background:rgba(244,183,40,0.03);border:1px solid rgba(244,183,40,0.08)}

/* ── LED indicator ── */
.led{width:8px;height:8px;border-radius:50%;display:inline-block}
.led-green{background:#48bb78;box-shadow:0 0 6px rgba(72,187,120,0.6)}
.led-yellow{background:#f4b728;box-shadow:0 0 6px rgba(244,183,40,0.6)}
.led-red{background:#fc8181;box-shadow:0 0 6px rgba(252,129,129,0.6)}

/* ── Charts ── */
.chart-row{display:grid;grid-template-columns:1fr 1fr;gap:10px;margin-bottom:14px}
.chart-panel{background:var(--panel);border:1px solid var(--line);border-radius:12px;padding:12px}
.chart-panel .panel-title{font-size:10px;text-transform:uppercase;letter-spacing:1px;color:var(--muted);margin-bottom:6px;
  display:flex;align-items:center;justify-content:space-between}
.chart-panel .legend{display:flex;gap:10px;font-size:9px;color:var(--muted)}
.chart-panel .legend .sw{width:8px;height:3px;display:inline-block;border-radius:1px;margin-right:4px;vertical-align:middle}
.chart-panel canvas{width:100%;height:140px;border-radius:8px;background:rgba(244,183,40,0.02);border:1px solid rgba(244,183,40,0.06)}

/* ── Tables ── */
.section-title{font-size:10px;text-transform:uppercase;letter-spacing:1px;color:var(--muted);padding:6px 0}
.table-wrap{border:1px solid var(--line);border-radius:10px;overflow:hidden;margin-bottom:14px}
table{width:100%;border-collapse:collapse;background:var(--panel)}
th{font-size:9px;text-transform:uppercase;letter-spacing:0.8px;color:var(--dim);padding:8px 12px;text-align:left;
  background:rgba(10,8,4,0.95);border-bottom:1px solid var(--line)}
td{font-size:11px;padding:6px 12px;border-bottom:1px solid rgba(244,183,40,0.06);color:var(--muted)}
tr:hover td{background:rgba(244,183,40,0.03)}
.status-confirmed{color:#48bb78}.status-pending{color:#f4b728}.status-orphaned{color:#fc8181}
.addr-cell{max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.addr-link{color:var(--ink);cursor:pointer;text-decoration:none}.addr-link:hover{color:var(--accent)}

/* ── Miner lookup ── */
.lookup-row{display:flex;gap:2px;margin-bottom:14px}
.lookup-row input{flex:1;padding:8px 12px;background:rgba(18,14,8,0.92);border:1px solid var(--line);
  border-radius:8px 0 0 8px;color:var(--ink);font-family:inherit;font-size:12px;outline:none}
.lookup-row input:focus{border-color:var(--accent)}
.lookup-row input::placeholder{color:var(--dim)}
.lookup-row button{padding:8px 16px;background:rgba(244,183,40,0.12);border:1px solid rgba(244,183,40,0.3);
  border-radius:0 8px 8px 0;color:var(--accent);font-size:10px;text-transform:uppercase;cursor:pointer;font-family:inherit}
.lookup-row button:hover{background:rgba(244,183,40,0.22)}
#miner-info{display:none}
.miner-stats-row{display:grid;grid-template-columns:repeat(3,1fr);gap:10px;margin-bottom:14px}
.miner-stat{background:var(--panel);border:1px solid var(--line);border-radius:10px;padding:10px 14px}

/* ── Footer ── */
.footer{background:rgba(10,8,4,0.95);border-top:1px solid var(--line);padding:6px 20px;
  display:flex;align-items:center;gap:16px;font-size:10px;color:var(--dim);
  position:fixed;bottom:0;left:0;right:0;z-index:10}
.sf-item{display:flex;align-items:center;gap:5px}
.footer-spacer{height:2.5rem}

@media(max-width:1200px){.grid{grid-template-columns:repeat(6,1fr)}.gauge-card{grid-column:span 3}.chart-row{grid-template-columns:1fr}}
@media(max-width:800px){.grid{grid-template-columns:repeat(4,1fr)}.gauge-card{grid-column:span 4}.spark-card{grid-column:span 4}}
</style>
</head>
<body>
<main>
<div class="header">
  <h1 id="pool-name">TAZ MINING POOL</h1><span class="badge">Testnet</span>
  <div class="header-right">
    <span class="status-dot ok" id="hDot"></span><span style="font-size:10px;color:var(--muted)" id="hStatus">Connected</span>
    <a href="/previews" class="nav-link">Themes</a><a href="/" class="nav-link">Classic</a><a href="/zallet" class="nav-link">Wallet</a>
  </div>
</div>

<div class="grid">
  <!-- Gauge: Hashrate -->
  <div class="gauge-card"><div class="label">Pool Hashrate</div><canvas id="gaugeHash"></canvas></div>
  <!-- Gauge: Luck -->
  <div class="gauge-card"><div class="label">Luck (24h)</div><canvas id="gaugeLuck"></canvas></div>

  <!-- Sparkline: Network Hash -->
  <div class="spark-card"><div class="label">Network Hashrate</div>
    <div class="spark-row"><div class="val" id="vNetHash">--</div><canvas id="spNetHash"></canvas></div>
    <div class="sub" style="font-size:9px;color:var(--muted)">Peak: <span id="recNetHash">--</span></div></div>

  <!-- Sparkline: Miners -->
  <div class="spark-card"><div class="label">Connected Miners</div>
    <div class="spark-row"><div class="val" id="vMiners">0</div><canvas id="spMiners"></canvas></div>
    <div class="sub" style="font-size:9px;color:var(--muted)">Peak: <span id="recMiners">0</span></div></div>

  <!-- Cards row -->
  <div class="card"><div class="label">Blocks Found</div><div class="val" id="vBlocks">0</div></div>
  <div class="card"><div class="label">Immature <span class="led led-green" id="ledImm"></span></div><div class="val" id="vImm">0</div></div>
  <div class="card"><div class="label">Pending Payouts <span class="led led-green" id="ledPend"></span></div><div class="val" id="vPend">0</div></div>
  <div class="card"><div class="label">Pool Fee</div><div class="val" id="vFee">--</div></div>
  <div class="card"><div class="label">Net Share (24h)</div><div class="val" id="vPct">--</div></div>
  <div class="card"><div class="label">Share Rate</div><div class="val" id="vShareRate">0</div><div class="sub">/min</div></div>
  <div class="card"><div class="label">Total Shares</div><div class="val" id="vShares" style="font-size:16px">0</div></div>
  <div class="card"><div class="label">Stratum Port</div><div class="val" id="vPort">--</div></div>
</div>

<div class="chart-row">
  <div class="chart-panel">
    <div class="panel-title"><span>Hashrate History</span>
      <div class="legend"><span><span class="sw" style="background:var(--accent)"></span>Pool</span>
        <span><span class="sw" style="background:#48bb78"></span>Network</span></div></div>
    <canvas id="chartHash"></canvas></div>
  <div class="chart-panel">
    <div class="panel-title"><span>Mining Activity</span>
      <div class="legend"><span><span class="sw" style="background:var(--accent2)"></span>Shares/m</span>
        <span><span class="sw" style="background:#a78bfa"></span>Miners</span></div></div>
    <canvas id="chartMining"></canvas></div>
</div>

<div class="section-title">Miner Lookup</div>
<div class="lookup-row">
  <input type="text" id="miner-address" placeholder="Enter Zcash address..." onkeydown="if(event.key==='Enter')lookupMiner()">
  <button onclick="lookupMiner()">Lookup</button>
</div>
<div id="miner-info"><div class="miner-stats-row" id="miner-stats-grid"></div>
  <div class="section-title">Workers</div>
  <div class="table-wrap"><table id="workers-table"><thead><tr><th>Name</th><th>Last Seen</th></tr></thead><tbody></tbody></table></div></div>

<div class="section-title">Miners</div>
<div class="table-wrap"><table id="miners-table"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>Workers</th><th>Shares</th><th>Pending</th></tr></thead>
  <tbody><tr><td colspan="6" style="color:var(--dim)">Loading...</td></tr></tbody></table></div>

<div class="section-title">Recent Blocks</div>
<div class="table-wrap"><table id="blocks-table"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead>
  <tbody><tr><td colspan="6" style="color:var(--dim)">Loading...</td></tr></tbody></table></div>

<div class="section-title">Recent Payouts</div>
<div class="table-wrap"><table id="payouts-table"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead>
  <tbody><tr><td colspan="4" style="color:var(--dim)">Loading...</td></tr></tbody></table></div>

<div class="footer-spacer"></div>
</main>

<div class="footer">
  <div class="sf-item"><span class="status-dot ok" id="sfNode"></span><span>Node</span><span id="sfNodeT" style="color:var(--muted)">--</span></div>
  <div class="sf-item"><span class="status-dot ok" id="sfWallet"></span><span>Wallet</span><span id="sfWalletT" style="color:var(--muted)">--</span></div>
  <div class="sf-item"><span>Template</span><span id="sfTemplate" style="color:var(--muted)">--</span></div>
  <div style="margin-left:auto" class="sf-item"><span id="sfUptime">--</span></div>
</div>

<script>
const MH=120,RS=10000,RT=30000;
const H={hr:[],nh:[],mn:[],bl:[],sh:[],lb:[]};
let pSh=null;const rec={nh:0,mn:0};let t0=Date.now();

function fH(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}
function fHs(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(1)+'T';if(h>=1e9)return(h/1e9).toFixed(1)+'G';if(h>=1e6)return(h/1e6).toFixed(1)+'M';if(h>=1e3)return(h/1e3).toFixed(1)+'K';return h.toFixed(0)}
function fD(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);if(h>0)return h+'h '+m%60+'m';if(m>0)return m+'m '+s%60+'s';return s+'s'}
function push(a,v){a.push(v);if(a.length>MH)a.shift()}

/* ── Ring Gauge ── */
function drawRing(canvas, pct, val, cfg) {
  if(!canvas)return;
  const W=canvas.clientWidth||260,Hh=canvas.clientHeight||220;
  const dpr=window.devicePixelRatio||1;
  canvas.width=Math.floor(W*dpr);canvas.height=Math.floor(Hh*dpr);
  const ctx=canvas.getContext('2d');ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,W,Hh);

  const cx=W/2, cy=Hh*0.48;
  const r=Math.min(cx-8,cy-8)*0.9;
  const lw=Math.max(8,r*0.12);
  const startA=-Math.PI*0.75, endA=Math.PI*0.75, span=endA-startA;
  const valA=startA+Math.min(1,Math.max(0,pct))*span;

  // Background track
  ctx.lineWidth=lw;ctx.lineCap='round';
  ctx.strokeStyle='rgba(244,183,40,0.08)';
  ctx.beginPath();ctx.arc(cx,cy,r,startA,endA);ctx.stroke();

  // Value arc with gradient
  if(pct>0.001){
    const grad=ctx.createLinearGradient(cx-r,cy,cx+r,cy);
    grad.addColorStop(0,cfg.colorStart||'#ff4444');
    grad.addColorStop(0.4,cfg.colorMid||'#f4b728');
    grad.addColorStop(1,cfg.colorEnd||'#48bb78');
    ctx.strokeStyle=grad;
    ctx.shadowColor=cfg.glowColor||'rgba(244,183,40,0.5)';ctx.shadowBlur=16;
    ctx.beginPath();ctx.arc(cx,cy,r,startA,valA);ctx.stroke();
    ctx.shadowBlur=0;
  }

  // Ticks
  const tR=r+lw/2+4, tR2=tR+6;
  ctx.strokeStyle='rgba(244,183,40,0.25)';ctx.lineWidth=1;
  for(let i=0;i<=10;i++){
    const a=startA+i/10*span;
    ctx.beginPath();ctx.moveTo(cx+tR*Math.cos(a),cy+tR*Math.sin(a));
    ctx.lineTo(cx+tR2*Math.cos(a),cy+tR2*Math.sin(a));ctx.stroke();
  }

  // Center value
  ctx.textAlign='center';ctx.textBaseline='middle';
  ctx.font=`700 ${Math.max(20,r*0.38)}px 'JetBrains Mono','Fira Code','Courier New',monospace`;
  ctx.fillStyle='#f4b728';
  ctx.shadowColor='rgba(244,183,40,0.5)';ctx.shadowBlur=20;
  ctx.fillText(val,cx,cy);ctx.shadowBlur=0;

  // Sub label
  if(cfg.unit){
    ctx.font=`${Math.max(10,r*0.12)}px 'JetBrains Mono',monospace`;
    ctx.fillStyle='rgba(154,134,96,0.8)';
    ctx.fillText(cfg.unit,cx,cy+r*0.32);
  }

  // Bottom text
  if(cfg.subText){
    ctx.font=`${Math.max(9,r*0.10)}px sans-serif`;
    ctx.fillStyle='rgba(154,134,96,0.6)';
    ctx.fillText(cfg.subText,cx,cy+r*0.85);
  }
}

/* ── Sparkline ── */
function drawSp(canvas,data,color){
  if(!canvas||!data.length)return;
  const W=canvas.clientWidth||120,Hh=canvas.clientHeight||48;
  const dpr=window.devicePixelRatio||1;
  canvas.width=Math.floor(W*dpr);canvas.height=Math.floor(Hh*dpr);
  const ctx=canvas.getContext('2d');ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,W,Hh);
  const p=2,pW=W-2*p,pH=Hh-2*p;
  let mn=Infinity,mx=-Infinity;for(const v of data){if(v<mn)mn=v;if(v>mx)mx=v}
  if(mn===mx){mn-=1;mx+=1}const rg=mx-mn;
  // Fill
  const fp=new Path2D();fp.moveTo(p,p+(1-(data[0]-mn)/rg)*pH);
  for(let i=0;i<data.length;i++)fp.lineTo(p+(i/(data.length-1))*pW,p+(1-(data[i]-mn)/rg)*pH);
  fp.lineTo(p+pW,p+pH);fp.lineTo(p,p+pH);fp.closePath();
  const g=ctx.createLinearGradient(0,p,0,p+pH);
  g.addColorStop(0,color+'33');g.addColorStop(1,'transparent');ctx.fillStyle=g;ctx.fill(fp);
  // Line
  ctx.beginPath();for(let i=0;i<data.length;i++){const x=p+(i/(data.length-1))*pW,y=p+(1-(data[i]-mn)/rg)*pH;if(i===0)ctx.moveTo(x,y);else ctx.lineTo(x,y)}
  ctx.strokeStyle=color;ctx.lineWidth=1.5;ctx.shadowColor=color;ctx.shadowBlur=6;ctx.stroke();ctx.shadowBlur=0;
  // Dot
  const lx=p+pW,ly=p+(1-(data[data.length-1]-mn)/rg)*pH;
  ctx.beginPath();ctx.arc(lx,ly,2.5,0,Math.PI*2);ctx.fillStyle=color;ctx.shadowColor=color;ctx.shadowBlur=8;ctx.fill();ctx.shadowBlur=0;
}

/* ── Chart ── */
function drawCh(canvas,sets,labels){
  if(!canvas)return;
  const W=canvas.clientWidth||600,Hh=canvas.clientHeight||140;
  const dpr=window.devicePixelRatio||1;
  canvas.width=Math.floor(W*dpr);canvas.height=Math.floor(Hh*dpr);
  const ctx=canvas.getContext('2d');ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,W,Hh);
  const pL=40,pR=10,pT=8,pB=20,pW=W-pL-pR,pH=Hh-pT-pB;
  if(pW<=20||pH<=20)return;
  let gMn=Infinity,gMx=-Infinity;
  for(const s of sets)for(const v of s.data){if(v<gMn)gMn=v;if(v>gMx)gMx=v}
  if(gMn===gMx){gMn-=1;gMx+=1}const rg=gMx-gMn;
  // Grid
  ctx.strokeStyle='rgba(244,183,40,0.06)';ctx.lineWidth=0.5;
  for(let i=0;i<=4;i++){const y=pT+(i/4)*pH;ctx.beginPath();ctx.moveTo(pL,y);ctx.lineTo(pL+pW,y);ctx.stroke();
    ctx.fillStyle='rgba(154,134,96,0.4)';ctx.font='9px monospace';ctx.textAlign='right';ctx.textBaseline='middle';
    ctx.fillText(fHs(gMx-(i/4)*rg),pL-4,y)}
  // X labels
  if(labels.length>1){ctx.fillStyle='rgba(154,134,96,0.3)';ctx.font='8px sans-serif';ctx.textAlign='center';ctx.textBaseline='top';
    const step=Math.max(1,Math.floor(labels.length/6));
    for(let i=0;i<labels.length;i+=step){ctx.fillText(labels[i],pL+(i/(labels.length-1))*pW,pT+pH+3)}}
  // Lines
  for(const s of sets){if(!s.data.length)continue;const n=s.data.length;
    const fp=new Path2D();fp.moveTo(pL,pT+(1-(s.data[0]-gMn)/rg)*pH);
    for(let i=0;i<n;i++)fp.lineTo(pL+(i/(n-1))*pW,pT+(1-(s.data[i]-gMn)/rg)*pH);
    fp.lineTo(pL+pW,pT+pH);fp.lineTo(pL,pT+pH);fp.closePath();
    const g=ctx.createLinearGradient(0,pT,0,pT+pH);g.addColorStop(0,s.color+'22');g.addColorStop(1,'transparent');ctx.fillStyle=g;ctx.fill(fp);
    ctx.beginPath();for(let i=0;i<n;i++){const x=pL+(i/(n-1))*pW,y=pT+(1-(s.data[i]-gMn)/rg)*pH;if(i===0)ctx.moveTo(x,y);else ctx.lineTo(x,y)}
    ctx.strokeStyle=s.color;ctx.lineWidth=1.5;ctx.shadowColor=s.color;ctx.shadowBlur=4;ctx.stroke();ctx.shadowBlur=0}
}

function setLed(id,state){const el=document.getElementById(id);if(!el)return;
  el.className='led led-'+state}

/* ── Data ── */
async function fetchStats(){
  try{const r=await fetch('/api/pool/stats');const d=await r.json();const now=Date.now();
    document.getElementById('pool-name').textContent=(d.name||'TAZ Mining Pool').toUpperCase();
    const hr=d.hashrate_estimate||0;
    // Auto-scale max
    let mx=100;if(hr>100)mx=1000;if(hr>1000)mx=10000;if(hr>10000)mx=1e5;if(hr>1e5)mx=1e6;if(hr>1e6)mx=1e9;
    drawRing(document.getElementById('gaugeHash'),hr/mx,fHs(hr),{unit:'Sol/s',colorStart:'#ff4444',colorMid:'#f4b728',colorEnd:'#48bb78',glowColor:'rgba(244,183,40,0.5)',subText:'Pool Hashrate'});
    const luck=d.luck_percent||0;
    drawRing(document.getElementById('gaugeLuck'),Math.min(luck/200,1),Math.round(luck)+'%',{unit:'Luck',colorStart:'#48bb78',colorMid:'#f4b728',colorEnd:'#ff4444',glowColor:'rgba(244,183,40,0.5)',subText:'24-Hour Average'});

    document.getElementById('vNetHash').textContent=fH(d.network_hashrate);
    document.getElementById('vMiners').textContent=d.connected_miners||0;
    document.getElementById('vBlocks').textContent=d.total_blocks||0;
    document.getElementById('vImm').textContent=d.immature_blocks||0;
    document.getElementById('vPend').textContent=d.pending_payout_blocks||0;
    document.getElementById('vFee').textContent=(d.fee_percent||0)+'%';
    document.getElementById('vPort').textContent=d.stratum_port||'--';
    document.getElementById('vShares').textContent=(d.total_shares||0).toLocaleString();
    const pctEl=document.getElementById('vPct');
    if(d.pool_percent_24h!=null)pctEl.textContent=d.pool_percent_24h.toFixed(2)+'%';
    const spm=pSh!==null?Math.max(0,(d.total_shares-pSh)*(60000/RS)):0;pSh=d.total_shares;
    document.getElementById('vShareRate').textContent=Math.round(spm);

    const nh=d.network_hashrate||0;if(nh>rec.nh){rec.nh=nh;document.getElementById('recNetHash').textContent=fH(nh)}
    const mc=d.connected_miners||0;if(mc>rec.mn){rec.mn=mc;document.getElementById('recMiners').textContent=mc}

    setLed('ledImm',(d.immature_blocks||0)>0?'yellow':'green');
    setLed('ledPend',(d.pending_payout_blocks||0)>0?'yellow':'green');

    const nodeOk=d.node_ok!==false;const hDot=document.getElementById('hDot');
    hDot.className='status-dot '+(nodeOk?'ok':'err');
    document.getElementById('hStatus').textContent=nodeOk?'Connected':'Stalled';
    document.getElementById('sfNode').className='status-dot '+(nodeOk?'ok':'err');
    document.getElementById('sfNodeT').textContent=nodeOk?'OK':'Stalled';
    const wOk=d.wallet_ok!==false;document.getElementById('sfWallet').className='status-dot '+(wOk?'ok':'err');
    document.getElementById('sfWalletT').textContent=wOk?'Online':'Offline';
    if(d.last_template_at)document.getElementById('sfTemplate').textContent=d.last_template_at.replace('T',' ').slice(0,19);
    document.getElementById('sfUptime').textContent='Up '+fD(now-t0);

    push(H.hr,hr);push(H.nh,nh);push(H.mn,mc);push(H.bl,d.total_blocks||0);push(H.sh,Math.round(spm));push(H.lb,now);
    drawSp(document.getElementById('spNetHash'),H.nh,'#48bb78');
    drawSp(document.getElementById('spMiners'),H.mn,'#a78bfa');
    const tl=H.lb.map(t=>{const d=new Date(t);return d.getHours().toString().padStart(2,'0')+':'+d.getMinutes().toString().padStart(2,'0')});
    drawCh(document.getElementById('chartHash'),[{data:[...H.hr],color:'#f4b728'},{data:H.nh.map(v=>v?v/(Math.max(1,Math.max(...H.nh))/Math.max(1,Math.max(...H.hr)||1)):0),color:'#48bb78'}],tl);
    drawCh(document.getElementById('chartMining'),[{data:[...H.sh],color:'#ff8c00'},{data:[...H.mn],color:'#a78bfa'}],tl);
  }catch(e){console.error(e)}}

async function fetchBlocks(){try{const r=await fetch('/api/blocks');const b=await r.json();const tb=document.querySelector('#blocks-table tbody');
  if(!b.length){tb.innerHTML='<tr><td colspan="6" style="color:var(--dim)">No blocks yet</td></tr>';return}
  tb.innerHTML=b.map(x=>{let ls='--',lc='var(--muted)';if(x.luck_percent!=null){ls=x.luck_percent.toFixed(0)+'%';lc=x.luck_percent<=100?'#48bb78':x.luck_percent<=150?'#f4b728':'#fc8181'}
    return'<tr><td style="color:var(--ink)">'+x.height+'</td><td title="'+x.hash+'">'+x.hash.substring(0,16)+'...</td><td>'+x.reward_zec.toFixed(4)+' TAZ</td><td style="color:'+lc+'">'+ls+'</td><td class="status-'+x.status+'">'+x.status+'</td><td>'+x.found_at+'</td></tr>'}).join('')}catch(e){console.error(e)}}

async function fetchMiners(){try{const r=await fetch('/api/miners');const m=await r.json();const tb=document.querySelector('#miners-table tbody');
  if(!m.length){tb.innerHTML='<tr><td colspan="6" style="color:var(--dim)">No miners yet</td></tr>';return}
  tb.innerHTML=m.map(x=>'<tr><td class="addr-cell" title="'+x.address+'"><span class="addr-link" onclick="doLookup(\''+x.address.replace(/'/g,"\\'")+'\')">' + x.address + '</span></td><td>'+fH(x.hashrate_1m)+'</td><td>'+fH(x.hashrate)+'</td><td>'+x.worker_count+'</td><td>'+x.share_count.toLocaleString()+'</td><td style="color:var(--accent)">'+x.pending_zec.toFixed(8)+'</td></tr>').join('')}catch(e){console.error(e)}}

async function fetchPayouts(){try{const r=await fetch('/api/payouts');const p=await r.json();const tb=document.querySelector('#payouts-table tbody');
  if(!p.length){tb.innerHTML='<tr><td colspan="4" style="color:var(--dim)">No payouts yet</td></tr>';return}
  tb.innerHTML=p.map(x=>'<tr><td class="addr-cell" title="'+x.miner_address+'" style="color:var(--ink)">'+x.miner_address+'</td><td style="color:#48bb78">'+x.amount_zec.toFixed(8)+' TAZ</td><td title="'+(x.txid||'')+'">'+((x.txid||'').substring(0,16)||'--')+'</td><td>'+x.created_at+'</td></tr>').join('')}catch(e){console.error(e)}}

function doLookup(a){document.getElementById('miner-address').value=a;lookupMiner()}
async function lookupMiner(){const a=document.getElementById('miner-address').value.trim();if(!a)return;
  try{const r=await fetch('/api/miner/'+encodeURIComponent(a));if(!r.ok){alert('Not found');return}const d=await r.json();
    document.getElementById('miner-info').style.display='block';
    document.getElementById('miner-stats-grid').innerHTML='<div class="miner-stat"><div class="label">Pending</div><div class="val" style="font-size:16px">'+d.balance.pending_zec.toFixed(8)+'</div></div><div class="miner-stat"><div class="label">Total Paid</div><div class="val" style="font-size:16px;color:#48bb78">'+d.balance.paid_zec.toFixed(8)+'</div></div><div class="miner-stat"><div class="label">Workers</div><div class="val" style="font-size:16px">'+d.workers.length+'</div></div>';
    document.querySelector('#workers-table tbody').innerHTML=d.workers.map(w=>'<tr><td>'+w.name+'</td><td>'+w.last_seen+'</td></tr>').join('')}catch(e){console.error(e)}}

document.addEventListener('DOMContentLoaded',()=>{fetchStats();fetchMiners();fetchBlocks();fetchPayouts();
  setInterval(fetchStats,RS);setInterval(fetchMiners,RS);setInterval(fetchBlocks,RT);setInterval(fetchPayouts,RT)});
</script>
</body>
</html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 23 — "Cipher" — Hacker terminal green with ASCII-art header,
// dot-matrix style values, bar gauges, and terminal-inspired tables
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW23: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TAZ Mining Pool — Cipher</title>
<style>
:root{--bg:#0a0f0a;--panel:rgba(8,16,8,0.92);--accent:#00ff41;--accent2:#33ff77;--dim:#0a3a0a;
  --ink:#b8ffb8;--muted:#4a7a4a;--line:rgba(0,255,65,0.12);--glow:rgba(0,255,65,0.15)}
*{margin:0;padding:0;box-sizing:border-box}
html,body{background:var(--bg);color:var(--ink);min-height:100vh;overflow-x:hidden}
body{font-family:'Courier New','Lucida Console',monospace;font-size:13px;position:relative}
body::before{content:'';position:fixed;inset:0;z-index:0;pointer-events:none;
  background:repeating-linear-gradient(0deg,transparent,transparent 2px,rgba(0,255,65,0.015) 2px,rgba(0,255,65,0.015) 4px)}
body::after{content:'';position:fixed;inset:0;z-index:0;pointer-events:none;
  background:radial-gradient(ellipse at 50% 0%,rgba(0,255,65,0.05),transparent 70%)}

main{max-width:1440px;margin:0 auto;padding:16px 20px;position:relative;z-index:1}

/* ── ASCII Header ── */
.ascii-header{background:var(--panel);border:1px solid var(--line);border-radius:0;padding:10px 16px;margin-bottom:12px;
  display:flex;align-items:center;gap:16px;font-family:'Courier New',monospace}
.ascii-title{color:var(--accent);font-size:14px;font-weight:700;letter-spacing:2px;
  text-shadow:0 0 10px rgba(0,255,65,0.5),0 0 20px rgba(0,255,65,0.3)}
.ascii-header .right{margin-left:auto;display:flex;align-items:center;gap:12px}
.cursor-blink{animation:blink 1s step-end infinite;color:var(--accent)}
@keyframes blink{0%,100%{opacity:1}50%{opacity:0}}
.term-link{color:var(--muted);font-size:11px;text-decoration:none;letter-spacing:1px}
.term-link:hover{color:var(--accent)}
.online-dot{width:8px;height:8px;border-radius:50%;background:var(--accent);display:inline-block;
  box-shadow:0 0 8px rgba(0,255,65,0.6)}

/* ── Grid ── */
.grid{display:grid;grid-template-columns:repeat(12,1fr);gap:8px;margin-bottom:12px}

/* ── Bar gauge cards ── */
.bar-gauge{grid-column:span 6;background:var(--panel);border:1px solid var(--line);padding:12px 16px;
  display:flex;flex-direction:column;gap:6px}
.bar-gauge .label{font-size:10px;text-transform:uppercase;letter-spacing:2px;color:var(--muted)}
.bar-gauge .gauge-row{display:flex;align-items:center;gap:12px}
.bar-gauge .val{font-size:24px;font-weight:700;color:var(--accent);white-space:nowrap;min-width:120px;
  text-shadow:0 0 8px rgba(0,255,65,0.4)}
.bar-track{flex:1;height:20px;background:rgba(0,255,65,0.05);border:1px solid var(--line);position:relative;overflow:hidden}
.bar-fill{height:100%;transition:width 0.5s ease;position:relative}
.bar-fill::after{content:'';position:absolute;inset:0;
  background:repeating-linear-gradient(90deg,transparent,transparent 3px,rgba(0,0,0,0.2) 3px,rgba(0,0,0,0.2) 4px)}
.bar-pct{position:absolute;right:4px;top:50%;transform:translateY(-50%);font-size:10px;color:var(--bg);font-weight:700;z-index:2}

/* ── Stat cards ── */
.card{grid-column:span 2;background:var(--panel);border:1px solid var(--line);padding:8px 12px;
  display:flex;flex-direction:column;gap:2px}
.card .label{font-size:9px;text-transform:uppercase;letter-spacing:1.5px;color:var(--muted)}
.card .val{font-size:20px;font-weight:700;color:var(--accent);
  text-shadow:0 0 6px rgba(0,255,65,0.3)}
.card .sub{font-size:9px;color:var(--muted)}
.card .led{width:6px;height:6px;border-radius:50%;display:inline-block;margin-right:4px}
.led-g{background:var(--accent);box-shadow:0 0 4px rgba(0,255,65,0.5)}
.led-y{background:#ffaa00;box-shadow:0 0 4px rgba(255,170,0,0.5)}
.led-r{background:#ff4444;box-shadow:0 0 4px rgba(255,68,68,0.5)}

/* ── Spark cards ── */
.spark-card{grid-column:span 3;background:var(--panel);border:1px solid var(--line);padding:8px 12px;
  display:flex;flex-direction:column;gap:3px}
.spark-card .spark-row{display:flex;align-items:center;gap:8px}
.spark-card .val{font-size:18px;font-weight:700;color:var(--accent);white-space:nowrap;
  text-shadow:0 0 6px rgba(0,255,65,0.3)}
.spark-card canvas{flex:1;height:40px;background:rgba(0,255,65,0.02);border:1px solid var(--line)}

/* ── Charts ── */
.chart-row{display:grid;grid-template-columns:1fr 1fr;gap:8px;margin-bottom:12px}
.chart-box{background:var(--panel);border:1px solid var(--line);padding:10px 14px}
.chart-box .title{font-size:10px;text-transform:uppercase;letter-spacing:2px;color:var(--muted);margin-bottom:6px;
  display:flex;justify-content:space-between}
.chart-box .legend{display:flex;gap:10px;font-size:9px;color:var(--muted)}
.chart-box .legend .sw{width:8px;height:3px;display:inline-block;margin-right:3px;vertical-align:middle}
.chart-box canvas{width:100%;height:130px;background:rgba(0,255,65,0.02);border:1px solid var(--line)}

/* ── Terminal Tables ── */
.section-title{font-size:10px;letter-spacing:2px;color:var(--muted);padding:6px 0}
.section-title::before{content:'> ';color:var(--accent)}
.table-wrap{border:1px solid var(--line);margin-bottom:12px;overflow:hidden}
table{width:100%;border-collapse:collapse;background:var(--panel)}
th{font-size:9px;text-transform:uppercase;letter-spacing:1.5px;color:var(--dim);padding:6px 12px;text-align:left;
  background:rgba(4,8,4,0.95);border-bottom:1px solid var(--line)}
td{font-size:11px;padding:5px 12px;border-bottom:1px solid rgba(0,255,65,0.05);color:var(--muted)}
tr:hover td{background:rgba(0,255,65,0.03)}
.status-confirmed{color:var(--accent)}.status-pending{color:#ffaa00}.status-orphaned{color:#ff4444}
.addr-cell{max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.addr-link{color:var(--ink);cursor:pointer;text-decoration:none}.addr-link:hover{color:var(--accent)}

/* ── Lookup ── */
.lookup-row{display:flex;gap:0;margin-bottom:12px}
.lookup-row input{flex:1;padding:8px 12px;background:var(--panel);border:1px solid var(--line);
  color:var(--ink);font-family:inherit;font-size:12px;outline:none}
.lookup-row input:focus{border-color:var(--accent);box-shadow:0 0 8px rgba(0,255,65,0.2)}
.lookup-row input::placeholder{color:var(--dim)}
.lookup-row button{padding:8px 16px;background:rgba(0,255,65,0.1);border:1px solid var(--line);
  color:var(--accent);font-family:inherit;font-size:10px;text-transform:uppercase;cursor:pointer;letter-spacing:1px}
.lookup-row button:hover{background:rgba(0,255,65,0.2)}
#miner-info{display:none}
.miner-stats-row{display:grid;grid-template-columns:repeat(3,1fr);gap:8px;margin-bottom:12px}
.miner-stat{background:var(--panel);border:1px solid var(--line);padding:8px 12px}

/* ── Footer ── */
.footer{background:rgba(4,8,4,0.95);border-top:1px solid var(--line);padding:5px 20px;
  display:flex;align-items:center;gap:14px;font-size:10px;color:var(--muted);
  position:fixed;bottom:0;left:0;right:0;z-index:10}
.sf{display:flex;align-items:center;gap:4px}
.footer-spacer{height:2rem}

@media(max-width:1200px){.grid{grid-template-columns:repeat(6,1fr)}.bar-gauge{grid-column:span 6}.chart-row{grid-template-columns:1fr}}
@media(max-width:800px){.grid{grid-template-columns:repeat(4,1fr)}.bar-gauge{grid-column:span 4}.spark-card{grid-column:span 4}}
</style>
</head>
<body>
<main>
<div class="ascii-header">
  <div class="ascii-title" id="pool-name">TAZ_MINING_POOL</div>
  <span class="cursor-blink">_</span>
  <div class="right">
    <span class="online-dot" id="hDot"></span><span style="font-size:10px;color:var(--muted)" id="hStat">ONLINE</span>
    <a href="/previews" class="term-link">[themes]</a><a href="/" class="term-link">[classic]</a><a href="/zallet" class="term-link">[wallet]</a>
  </div>
</div>

<div class="grid">
  <!-- Bar Gauges -->
  <div class="bar-gauge"><div class="label">Pool Hashrate</div>
    <div class="gauge-row"><div class="val" id="vHash">--</div>
      <div class="bar-track"><div class="bar-fill" id="barHash" style="width:0%;background:linear-gradient(90deg,#ff4444,#ffaa00,#00ff41)"></div><span class="bar-pct" id="barHashPct">0%</span></div></div></div>

  <div class="bar-gauge"><div class="label">Luck (24h)</div>
    <div class="gauge-row"><div class="val" id="vLuck">--</div>
      <div class="bar-track"><div class="bar-fill" id="barLuck" style="width:0%;background:linear-gradient(90deg,#00ff41,#ffaa00,#ff4444)"></div><span class="bar-pct" id="barLuckPct">0%</span></div></div></div>

  <!-- Sparkline cards -->
  <div class="spark-card"><div class="label">Net Hashrate</div>
    <div class="spark-row"><div class="val" id="vNH">--</div><canvas id="spNH"></canvas></div>
    <div class="sub" style="font-size:9px;color:var(--muted)">peak: <span id="recNH">--</span></div></div>
  <div class="spark-card"><div class="label">Miners</div>
    <div class="spark-row"><div class="val" id="vMn">0</div><canvas id="spMn"></canvas></div>
    <div class="sub" style="font-size:9px;color:var(--muted)">peak: <span id="recMn">0</span></div></div>
  <div class="spark-card"><div class="label">Share Rate</div>
    <div class="spark-row"><div class="val" id="vSR">0</div><canvas id="spSR"></canvas></div>
    <div class="sub" style="font-size:9px;color:var(--muted)">/min</div></div>
  <div class="spark-card"><div class="label">Blocks</div>
    <div class="spark-row"><div class="val" id="vBl">0</div><canvas id="spBl"></canvas></div></div>

  <!-- Stat cards -->
  <div class="card"><div class="label"><span class="led led-g" id="ldIm"></span>Immature</div><div class="val" id="vIm">0</div></div>
  <div class="card"><div class="label"><span class="led led-g" id="ldPd"></span>Payout Q</div><div class="val" id="vPd">0</div></div>
  <div class="card"><div class="label">Pool Fee</div><div class="val" id="vFee">--</div></div>
  <div class="card"><div class="label">Net Share</div><div class="val" id="vPct">--</div></div>
  <div class="card"><div class="label">Total Shares</div><div class="val" id="vTS" style="font-size:14px">0</div></div>
  <div class="card"><div class="label">Port</div><div class="val" id="vPort">--</div></div>
</div>

<div class="chart-row">
  <div class="chart-box"><div class="title"><span>hashrate_history</span>
    <div class="legend"><span><span class="sw" style="background:var(--accent)"></span>pool</span>
      <span><span class="sw" style="background:#33ff77"></span>network</span></div></div>
    <canvas id="chHash"></canvas></div>
  <div class="chart-box"><div class="title"><span>mining_activity</span>
    <div class="legend"><span><span class="sw" style="background:#ffaa00"></span>shares/m</span>
      <span><span class="sw" style="background:#00aaff"></span>miners</span></div></div>
    <canvas id="chMine"></canvas></div>
</div>

<div class="section-title">miner_lookup</div>
<div class="lookup-row">
  <input type="text" id="miner-address" placeholder="$ enter address..." onkeydown="if(event.key==='Enter')lookupMiner()">
  <button onclick="lookupMiner()">[search]</button>
</div>
<div id="miner-info"><div class="miner-stats-row" id="miner-stats-grid"></div>
  <div class="section-title">workers</div>
  <div class="table-wrap"><table id="workers-table"><thead><tr><th>name</th><th>last_seen</th></tr></thead><tbody></tbody></table></div></div>

<div class="section-title">connected_miners</div>
<div class="table-wrap"><table id="miners-table"><thead><tr><th>address</th><th>1m</th><th>10m</th><th>wkrs</th><th>shares</th><th>pending</th></tr></thead>
  <tbody><tr><td colspan="6" style="color:var(--dim)">loading...</td></tr></tbody></table></div>

<div class="section-title">recent_blocks</div>
<div class="table-wrap"><table id="blocks-table"><thead><tr><th>height</th><th>hash</th><th>reward</th><th>luck</th><th>status</th><th>found</th></tr></thead>
  <tbody><tr><td colspan="6" style="color:var(--dim)">loading...</td></tr></tbody></table></div>

<div class="section-title">recent_payouts</div>
<div class="table-wrap"><table id="payouts-table"><thead><tr><th>miner</th><th>amount</th><th>txid</th><th>date</th></tr></thead>
  <tbody><tr><td colspan="4" style="color:var(--dim)">loading...</td></tr></tbody></table></div>

<div class="footer-spacer"></div>
</main>

<div class="footer">
  <div class="sf"><span class="online-dot" id="sfN" style="width:6px;height:6px"></span><span>node</span><span id="sfNT" style="color:var(--muted)">--</span></div>
  <div class="sf"><span class="online-dot" id="sfW" style="width:6px;height:6px"></span><span>wallet</span><span id="sfWT" style="color:var(--muted)">--</span></div>
  <div class="sf"><span>template</span><span id="sfTmpl" style="color:var(--muted)">--</span></div>
  <div style="margin-left:auto" class="sf"><span id="sfUp">--</span></div>
</div>

<script>
const MH=120,RS=10000,RT=30000;
const H={hr:[],nh:[],mn:[],bl:[],sh:[],lb:[]};
let pSh=null;const rec={nh:0,mn:0};let t0=Date.now();

function fH(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}
function fHs(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(1)+'T';if(h>=1e9)return(h/1e9).toFixed(1)+'G';if(h>=1e6)return(h/1e6).toFixed(1)+'M';if(h>=1e3)return(h/1e3).toFixed(1)+'K';return h.toFixed(0)}
function fD(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);if(h>0)return h+'h '+m%60+'m';if(m>0)return m+'m '+s%60+'s';return s+'s'}
function push(a,v){a.push(v);if(a.length>MH)a.shift()}

/* ── Sparkline ── */
function drawSp(canvas,data,color){
  if(!canvas||!data.length)return;
  const W=canvas.clientWidth||120,Hh=canvas.clientHeight||40;
  const dpr=window.devicePixelRatio||1;
  canvas.width=Math.floor(W*dpr);canvas.height=Math.floor(Hh*dpr);
  const ctx=canvas.getContext('2d');ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,W,Hh);
  const p=2,pW=W-2*p,pH=Hh-2*p;
  let mn=Infinity,mx=-Infinity;for(const v of data){if(v<mn)mn=v;if(v>mx)mx=v}
  if(mn===mx){mn-=1;mx+=1}const rg=mx-mn;
  const fp=new Path2D();fp.moveTo(p,p+(1-(data[0]-mn)/rg)*pH);
  for(let i=0;i<data.length;i++)fp.lineTo(p+(i/(data.length-1))*pW,p+(1-(data[i]-mn)/rg)*pH);
  fp.lineTo(p+pW,p+pH);fp.lineTo(p,p+pH);fp.closePath();
  const g=ctx.createLinearGradient(0,p,0,p+pH);g.addColorStop(0,color+'33');g.addColorStop(1,'transparent');ctx.fillStyle=g;ctx.fill(fp);
  ctx.beginPath();for(let i=0;i<data.length;i++){const x=p+(i/(data.length-1))*pW,y=p+(1-(data[i]-mn)/rg)*pH;if(i===0)ctx.moveTo(x,y);else ctx.lineTo(x,y)}
  ctx.strokeStyle=color;ctx.lineWidth=1.5;ctx.shadowColor=color;ctx.shadowBlur=4;ctx.stroke();ctx.shadowBlur=0;
  const lx=p+pW,ly=p+(1-(data[data.length-1]-mn)/rg)*pH;
  ctx.beginPath();ctx.arc(lx,ly,2,0,Math.PI*2);ctx.fillStyle=color;ctx.fill();
}

/* ── Chart ── */
function drawCh(canvas,sets,labels){
  if(!canvas)return;
  const W=canvas.clientWidth||600,Hh=canvas.clientHeight||130;
  const dpr=window.devicePixelRatio||1;
  canvas.width=Math.floor(W*dpr);canvas.height=Math.floor(Hh*dpr);
  const ctx=canvas.getContext('2d');ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,W,Hh);
  const pL=40,pR=10,pT=6,pB=18,pW=W-pL-pR,pH=Hh-pT-pB;if(pW<=20||pH<=20)return;
  let gMn=Infinity,gMx=-Infinity;
  for(const s of sets)for(const v of s.data){if(v<gMn)gMn=v;if(v>gMx)gMx=v}
  if(gMn===gMx){gMn-=1;gMx+=1}const rg=gMx-gMn;
  ctx.strokeStyle='rgba(0,255,65,0.06)';ctx.lineWidth=0.5;
  for(let i=0;i<=4;i++){const y=pT+(i/4)*pH;ctx.beginPath();ctx.moveTo(pL,y);ctx.lineTo(pL+pW,y);ctx.stroke();
    ctx.fillStyle='rgba(74,122,74,0.5)';ctx.font='8px monospace';ctx.textAlign='right';ctx.textBaseline='middle';
    ctx.fillText(fHs(gMx-(i/4)*rg),pL-4,y)}
  if(labels.length>1){ctx.fillStyle='rgba(74,122,74,0.3)';ctx.font='8px monospace';ctx.textAlign='center';ctx.textBaseline='top';
    const step=Math.max(1,Math.floor(labels.length/6));
    for(let i=0;i<labels.length;i+=step)ctx.fillText(labels[i],pL+(i/(labels.length-1))*pW,pT+pH+2)}
  for(const s of sets){if(!s.data.length)continue;const n=s.data.length;
    const fp=new Path2D();fp.moveTo(pL,pT+(1-(s.data[0]-gMn)/rg)*pH);
    for(let i=0;i<n;i++)fp.lineTo(pL+(i/(n-1))*pW,pT+(1-(s.data[i]-gMn)/rg)*pH);
    fp.lineTo(pL+pW,pT+pH);fp.lineTo(pL,pT+pH);fp.closePath();
    const g=ctx.createLinearGradient(0,pT,0,pT+pH);g.addColorStop(0,s.color+'1a');g.addColorStop(1,'transparent');ctx.fillStyle=g;ctx.fill(fp);
    ctx.beginPath();for(let i=0;i<n;i++){const x=pL+(i/(n-1))*pW,y=pT+(1-(s.data[i]-gMn)/rg)*pH;if(i===0)ctx.moveTo(x,y);else ctx.lineTo(x,y)}
    ctx.strokeStyle=s.color;ctx.lineWidth=1.5;ctx.shadowColor=s.color;ctx.shadowBlur=4;ctx.stroke();ctx.shadowBlur=0}
}

function sLed(id,s){const el=document.getElementById(id);if(!el)return;
  el.className='led led-'+(s==='green'?'g':s==='yellow'?'y':'r')}

async function fetchStats(){
  try{const r=await fetch('/api/pool/stats');const d=await r.json();const now=Date.now();
    document.getElementById('pool-name').textContent=(d.name||'TAZ_MINING_POOL').toUpperCase().replace(/ /g,'_');
    const hr=d.hashrate_estimate||0;
    document.getElementById('vHash').textContent=fH(hr);
    let mx=100;if(hr>100)mx=1000;if(hr>1000)mx=10000;if(hr>10000)mx=1e5;if(hr>1e5)mx=1e6;if(hr>1e6)mx=1e9;
    const hPct=Math.min(100,hr/mx*100);
    document.getElementById('barHash').style.width=hPct+'%';
    document.getElementById('barHashPct').textContent=Math.round(hPct)+'%';

    const luck=d.luck_percent||0;
    document.getElementById('vLuck').textContent=Math.round(luck)+'%';
    const lPct=Math.min(100,luck/200*100);
    document.getElementById('barLuck').style.width=lPct+'%';
    document.getElementById('barLuckPct').textContent=Math.round(luck)+'%';

    document.getElementById('vNH').textContent=fH(d.network_hashrate);
    document.getElementById('vMn').textContent=d.connected_miners||0;
    document.getElementById('vBl').textContent=d.total_blocks||0;
    document.getElementById('vIm').textContent=d.immature_blocks||0;
    document.getElementById('vPd').textContent=d.pending_payout_blocks||0;
    document.getElementById('vFee').textContent=(d.fee_percent||0)+'%';
    document.getElementById('vPort').textContent=d.stratum_port||'--';
    document.getElementById('vTS').textContent=(d.total_shares||0).toLocaleString();
    const pe=document.getElementById('vPct');if(d.pool_percent_24h!=null)pe.textContent=d.pool_percent_24h.toFixed(2)+'%';
    const spm=pSh!==null?Math.max(0,(d.total_shares-pSh)*(60000/RS)):0;pSh=d.total_shares;
    document.getElementById('vSR').textContent=Math.round(spm);

    const nh=d.network_hashrate||0;if(nh>rec.nh){rec.nh=nh;document.getElementById('recNH').textContent=fH(nh)}
    const mc=d.connected_miners||0;if(mc>rec.mn){rec.mn=mc;document.getElementById('recMn').textContent=mc}
    sLed('ldIm',(d.immature_blocks||0)>0?'yellow':'green');sLed('ldPd',(d.pending_payout_blocks||0)>0?'yellow':'green');

    const ok=d.node_ok!==false;const hd=document.getElementById('hDot');
    hd.style.background=ok?'var(--accent)':'#ff4444';hd.style.boxShadow=ok?'0 0 8px rgba(0,255,65,0.6)':'0 0 8px rgba(255,68,68,0.6)';
    document.getElementById('hStat').textContent=ok?'ONLINE':'OFFLINE';
    document.getElementById('sfN').style.background=ok?'var(--accent)':'#ff4444';
    document.getElementById('sfNT').textContent=ok?'ok':'stalled';
    const wk=d.wallet_ok!==false;document.getElementById('sfW').style.background=wk?'var(--accent)':'#ff4444';
    document.getElementById('sfWT').textContent=wk?'online':'offline';
    if(d.last_template_at)document.getElementById('sfTmpl').textContent=d.last_template_at.replace('T',' ').slice(0,19);
    document.getElementById('sfUp').textContent='uptime: '+fD(now-t0);

    push(H.hr,hr);push(H.nh,nh);push(H.mn,mc);push(H.bl,d.total_blocks||0);push(H.sh,Math.round(spm));push(H.lb,now);
    drawSp(document.getElementById('spNH'),H.nh,'#33ff77');drawSp(document.getElementById('spMn'),H.mn,'#00aaff');
    drawSp(document.getElementById('spSR'),H.sh,'#ffaa00');drawSp(document.getElementById('spBl'),H.bl,'#00ff41');
    const tl=H.lb.map(t=>{const d=new Date(t);return d.getHours().toString().padStart(2,'0')+':'+d.getMinutes().toString().padStart(2,'0')});
    drawCh(document.getElementById('chHash'),[{data:[...H.hr],color:'#00ff41'},{data:H.nh.map(v=>v?v/(Math.max(1,Math.max(...H.nh))/Math.max(1,Math.max(...H.hr)||1)):0),color:'#33ff77'}],tl);
    drawCh(document.getElementById('chMine'),[{data:[...H.sh],color:'#ffaa00'},{data:[...H.mn],color:'#00aaff'}],tl);
  }catch(e){console.error(e)}}

async function fetchBlocks(){try{const r=await fetch('/api/blocks');const b=await r.json();const tb=document.querySelector('#blocks-table tbody');
  if(!b.length){tb.innerHTML='<tr><td colspan="6" style="color:var(--dim)">no blocks</td></tr>';return}
  tb.innerHTML=b.map(x=>{let ls='--',lc='var(--muted)';if(x.luck_percent!=null){ls=x.luck_percent.toFixed(0)+'%';lc=x.luck_percent<=100?'var(--accent)':x.luck_percent<=150?'#ffaa00':'#ff4444'}
    return'<tr><td style="color:var(--ink)">'+x.height+'</td><td title="'+x.hash+'">'+x.hash.substring(0,16)+'...</td><td>'+x.reward_zec.toFixed(4)+'</td><td style="color:'+lc+'">'+ls+'</td><td class="status-'+x.status+'">'+x.status+'</td><td>'+x.found_at+'</td></tr>'}).join('')}catch(e){}}

async function fetchMiners(){try{const r=await fetch('/api/miners');const m=await r.json();const tb=document.querySelector('#miners-table tbody');
  if(!m.length){tb.innerHTML='<tr><td colspan="6" style="color:var(--dim)">no miners</td></tr>';return}
  tb.innerHTML=m.map(x=>'<tr><td class="addr-cell" title="'+x.address+'"><span class="addr-link" onclick="doLookup(\''+x.address.replace(/'/g,"\\'")+'\')">' + x.address + '</span></td><td>'+fH(x.hashrate_1m)+'</td><td>'+fH(x.hashrate)+'</td><td>'+x.worker_count+'</td><td>'+x.share_count.toLocaleString()+'</td><td style="color:var(--accent)">'+x.pending_zec.toFixed(8)+'</td></tr>').join('')}catch(e){}}

async function fetchPayouts(){try{const r=await fetch('/api/payouts');const p=await r.json();const tb=document.querySelector('#payouts-table tbody');
  if(!p.length){tb.innerHTML='<tr><td colspan="4" style="color:var(--dim)">no payouts</td></tr>';return}
  tb.innerHTML=p.map(x=>'<tr><td class="addr-cell" title="'+x.miner_address+'" style="color:var(--ink)">'+x.miner_address+'</td><td style="color:var(--accent)">'+x.amount_zec.toFixed(8)+'</td><td title="'+(x.txid||'')+'">'+((x.txid||'').substring(0,16)||'--')+'</td><td>'+x.created_at+'</td></tr>').join('')}catch(e){}}

function doLookup(a){document.getElementById('miner-address').value=a;lookupMiner()}
async function lookupMiner(){const a=document.getElementById('miner-address').value.trim();if(!a)return;
  try{const r=await fetch('/api/miner/'+encodeURIComponent(a));if(!r.ok){alert('not found');return}const d=await r.json();
    document.getElementById('miner-info').style.display='block';
    document.getElementById('miner-stats-grid').innerHTML='<div class="miner-stat"><div class="label">pending</div><div class="val" style="font-size:16px">'+d.balance.pending_zec.toFixed(8)+'</div></div><div class="miner-stat"><div class="label">paid</div><div class="val" style="font-size:16px;color:#33ff77">'+d.balance.paid_zec.toFixed(8)+'</div></div><div class="miner-stat"><div class="label">workers</div><div class="val" style="font-size:16px">'+d.workers.length+'</div></div>';
    document.querySelector('#workers-table tbody').innerHTML=d.workers.map(w=>'<tr><td>'+w.name+'</td><td>'+w.last_seen+'</td></tr>').join('')}catch(e){}}

document.addEventListener('DOMContentLoaded',()=>{fetchStats();fetchMiners();fetchBlocks();fetchPayouts();
  setInterval(fetchStats,RS);setInterval(fetchMiners,RS);setInterval(fetchBlocks,RT);setInterval(fetchPayouts,RT)});
</script>
</body>
</html>
"##;
