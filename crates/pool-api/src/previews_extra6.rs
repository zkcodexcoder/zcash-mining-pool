// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 24 — "Radar" — Minimalist dark with donut progress rings,
// clean sans-serif typography, soft shadows, modern dashboard look
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW24: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TAZ Mining Pool — Radar</title>
<style>
:root{--bg:#0f1117;--card:#1a1d27;--card2:#222635;--accent:#6366f1;--accent2:#818cf8;
  --green:#22c55e;--yellow:#eab308;--red:#ef4444;--cyan:#06b6d4;--pink:#ec4899;
  --ink:#e2e8f0;--muted:#64748b;--line:#2a2e3d}
*{margin:0;padding:0;box-sizing:border-box}
html,body{background:var(--bg);color:var(--ink);min-height:100vh;overflow-x:hidden}
body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;font-size:13px}

main{max-width:1440px;margin:0 auto;padding:20px}

/* ── Header ── */
.header{display:flex;align-items:center;gap:16px;margin-bottom:20px;padding:14px 20px;
  background:var(--card);border-radius:16px;border:1px solid var(--line)}
.header h1{font-size:18px;font-weight:700;color:var(--ink);letter-spacing:-0.3px}
.header .tag{background:var(--accent);color:white;padding:2px 8px;border-radius:6px;font-size:10px;font-weight:600}
.header .right{margin-left:auto;display:flex;align-items:center;gap:14px}
.header .dot{width:8px;height:8px;border-radius:50%;display:inline-block}
.header .dot.ok{background:var(--green);box-shadow:0 0 8px rgba(34,197,94,0.4)}
.header .dot.err{background:var(--red);box-shadow:0 0 8px rgba(239,68,68,0.4)}
.nav-link{color:var(--muted);font-size:12px;text-decoration:none;font-weight:500}
.nav-link:hover{color:var(--accent2)}

/* ── Grid ── */
.grid{display:grid;grid-template-columns:repeat(12,1fr);gap:12px;margin-bottom:20px}

/* ── Donut gauge cards ── */
.donut-card{grid-column:span 3;background:var(--card);border:1px solid var(--line);border-radius:16px;
  padding:16px;display:flex;flex-direction:column;align-items:center;gap:8px}
.donut-card canvas{width:160px;height:160px}
.donut-card .donut-label{font-size:11px;text-transform:uppercase;letter-spacing:1px;color:var(--muted);font-weight:600}
.donut-card .donut-sub{font-size:11px;color:var(--muted)}

/* ── Metric cards ── */
.metric-card{grid-column:span 2;background:var(--card);border:1px solid var(--line);border-radius:14px;
  padding:14px 16px;display:flex;flex-direction:column;gap:6px}
.metric-card .mc-label{font-size:11px;text-transform:uppercase;letter-spacing:0.5px;color:var(--muted);font-weight:600;
  display:flex;align-items:center;gap:6px}
.metric-card .mc-val{font-size:26px;font-weight:700;color:var(--ink);letter-spacing:-0.5px}
.metric-card .mc-sub{font-size:11px;color:var(--muted)}
.metric-card canvas{width:100%;height:40px;border-radius:6px;margin-top:4px}

/* ── Status indicator ── */
.si{width:8px;height:8px;border-radius:50%;display:inline-block}
.si-g{background:var(--green);box-shadow:0 0 6px rgba(34,197,94,0.4)}
.si-y{background:var(--yellow);box-shadow:0 0 6px rgba(234,179,8,0.4)}
.si-r{background:var(--red);box-shadow:0 0 6px rgba(239,68,68,0.4)}

/* ── Spark metric ── */
.spark-metric{grid-column:span 3;background:var(--card);border:1px solid var(--line);border-radius:14px;
  padding:14px 16px;display:flex;flex-direction:column;gap:6px}
.spark-metric .sm-row{display:flex;align-items:center;gap:12px}
.spark-metric .sm-val{font-size:22px;font-weight:700;color:var(--ink);white-space:nowrap}
.spark-metric canvas{flex:1;height:48px;border-radius:8px;background:var(--card2)}

/* ── Charts ── */
.chart-grid{display:grid;grid-template-columns:1fr 1fr;gap:12px;margin-bottom:20px}
.chart-panel{background:var(--card);border:1px solid var(--line);border-radius:16px;padding:16px}
.chart-panel .cp-header{display:flex;align-items:center;justify-content:space-between;margin-bottom:10px}
.chart-panel .cp-title{font-size:13px;font-weight:600;color:var(--ink)}
.chart-panel .cp-legend{display:flex;gap:12px;font-size:11px;color:var(--muted)}
.chart-panel .cp-legend .sw{width:10px;height:3px;display:inline-block;border-radius:1px;margin-right:4px;vertical-align:middle}
.chart-panel canvas{width:100%;height:150px;border-radius:10px;background:var(--card2)}

/* ── Tables ── */
.sec-title{font-size:12px;font-weight:600;color:var(--muted);text-transform:uppercase;letter-spacing:0.5px;padding:8px 0}
.table-wrap{border:1px solid var(--line);border-radius:14px;overflow:hidden;margin-bottom:20px}
table{width:100%;border-collapse:collapse;background:var(--card)}
th{font-size:11px;font-weight:600;color:var(--muted);padding:10px 14px;text-align:left;
  background:rgba(15,17,23,0.5);border-bottom:1px solid var(--line)}
td{font-size:12px;padding:8px 14px;border-bottom:1px solid rgba(42,46,61,0.5);color:var(--muted)}
tr:hover td{background:rgba(99,102,241,0.04)}
.status-confirmed{color:var(--green);font-weight:600}.status-pending{color:var(--yellow);font-weight:600}.status-orphaned{color:var(--red);font-weight:600}
.addr-cell{max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.addr-link{color:var(--ink);cursor:pointer;text-decoration:none;font-weight:500}.addr-link:hover{color:var(--accent2)}

/* ── Lookup ── */
.lookup-row{display:flex;gap:0;margin-bottom:20px}
.lookup-row input{flex:1;padding:10px 14px;background:var(--card);border:1px solid var(--line);
  border-radius:10px 0 0 10px;color:var(--ink);font-size:13px;outline:none;font-family:inherit}
.lookup-row input:focus{border-color:var(--accent)}
.lookup-row input::placeholder{color:var(--muted)}
.lookup-row button{padding:10px 20px;background:var(--accent);border:none;border-radius:0 10px 10px 0;
  color:white;font-size:12px;font-weight:600;cursor:pointer;font-family:inherit}
.lookup-row button:hover{background:var(--accent2)}
#miner-info{display:none}
.miner-stats-row{display:grid;grid-template-columns:repeat(3,1fr);gap:12px;margin-bottom:20px}
.miner-stat{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:14px 16px}

/* ── Footer ── */
.footer{background:var(--card);border-top:1px solid var(--line);padding:8px 20px;
  display:flex;align-items:center;gap:20px;font-size:11px;color:var(--muted);
  position:fixed;bottom:0;left:0;right:0;z-index:10}
.sf{display:flex;align-items:center;gap:6px}
.footer-spacer{height:3rem}

@media(max-width:1200px){.grid{grid-template-columns:repeat(6,1fr)}.donut-card{grid-column:span 3}.chart-grid{grid-template-columns:1fr}}
@media(max-width:800px){.grid{grid-template-columns:repeat(4,1fr)}.donut-card{grid-column:span 4}.spark-metric{grid-column:span 4}}
</style>
</head>
<body>
<main>
<div class="header">
  <h1 id="pool-name">TAZ Mining Pool</h1><span class="tag">Testnet</span>
  <div class="right">
    <span class="dot ok" id="hDot"></span><span style="font-size:11px;color:var(--muted)" id="hStat">Connected</span>
    <a href="/previews" class="nav-link">Themes</a><a href="/" class="nav-link">Classic</a><a href="/zallet" class="nav-link">Wallet</a>
  </div>
</div>

<div class="grid">
  <div class="donut-card"><div class="donut-label">Pool Hashrate</div><canvas id="donutHash"></canvas><div class="donut-sub" id="dHashSub">--</div></div>
  <div class="donut-card"><div class="donut-label">Luck (24h)</div><canvas id="donutLuck"></canvas><div class="donut-sub" id="dLuckSub">--</div></div>

  <div class="spark-metric"><div class="mc-label">Network Hashrate</div>
    <div class="sm-row"><div class="sm-val" id="vNH">--</div><canvas id="spNH"></canvas></div>
    <div class="mc-sub">Peak: <span id="recNH">--</span></div></div>
  <div class="spark-metric"><div class="mc-label">Connected Miners</div>
    <div class="sm-row"><div class="sm-val" id="vMn">0</div><canvas id="spMn"></canvas></div>
    <div class="mc-sub">Peak: <span id="recMn">0</span></div></div>

  <div class="metric-card"><div class="mc-label"><span class="si si-g" id="ldBl"></span>Blocks</div><div class="mc-val" id="vBl">0</div></div>
  <div class="metric-card"><div class="mc-label"><span class="si si-g" id="ldIm"></span>Immature</div><div class="mc-val" id="vIm">0</div></div>
  <div class="metric-card"><div class="mc-label"><span class="si si-g" id="ldPd"></span>Pending</div><div class="mc-val" id="vPd">0</div></div>
  <div class="metric-card"><div class="mc-label">Pool Fee</div><div class="mc-val" id="vFee">--</div></div>
  <div class="metric-card"><div class="mc-label">Net Share</div><div class="mc-val" id="vPct" style="color:var(--accent)">--</div></div>
  <div class="metric-card"><div class="mc-label">Share Rate</div><div class="mc-val" id="vSR">0</div><div class="mc-sub">/min</div></div>
  <div class="metric-card"><div class="mc-label">Total Shares</div><div class="mc-val" id="vTS" style="font-size:18px">0</div></div>
  <div class="metric-card"><div class="mc-label">Stratum</div><div class="mc-val" id="vPort">--</div></div>
</div>

<div class="chart-grid">
  <div class="chart-panel"><div class="cp-header"><span class="cp-title">Hashrate</span>
    <div class="cp-legend"><span><span class="sw" style="background:var(--accent)"></span>Pool</span>
      <span><span class="sw" style="background:var(--cyan)"></span>Network</span></div></div>
    <canvas id="chHash"></canvas></div>
  <div class="chart-panel"><div class="cp-header"><span class="cp-title">Activity</span>
    <div class="cp-legend"><span><span class="sw" style="background:var(--yellow)"></span>Shares/m</span>
      <span><span class="sw" style="background:var(--pink)"></span>Miners</span></div></div>
    <canvas id="chMine"></canvas></div>
</div>

<div class="sec-title">Miner Lookup</div>
<div class="lookup-row">
  <input type="text" id="miner-address" placeholder="Enter Zcash address..." onkeydown="if(event.key==='Enter')lookupMiner()">
  <button onclick="lookupMiner()">Search</button>
</div>
<div id="miner-info"><div class="miner-stats-row" id="miner-stats-grid"></div>
  <div class="sec-title">Workers</div>
  <div class="table-wrap"><table id="workers-table"><thead><tr><th>Name</th><th>Last Seen</th></tr></thead><tbody></tbody></table></div></div>

<div class="sec-title">Miners</div>
<div class="table-wrap"><table id="miners-table"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>Workers</th><th>Shares</th><th>Pending</th></tr></thead>
  <tbody><tr><td colspan="6" style="color:var(--muted)">Loading...</td></tr></tbody></table></div>

<div class="sec-title">Blocks</div>
<div class="table-wrap"><table id="blocks-table"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead>
  <tbody><tr><td colspan="6" style="color:var(--muted)">Loading...</td></tr></tbody></table></div>

<div class="sec-title">Payouts</div>
<div class="table-wrap"><table id="payouts-table"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead>
  <tbody><tr><td colspan="4" style="color:var(--muted)">Loading...</td></tr></tbody></table></div>

<div class="footer-spacer"></div>
</main>

<div class="footer">
  <div class="sf"><span class="dot ok" id="sfN"></span><span>Node</span><span id="sfNT">--</span></div>
  <div class="sf"><span class="dot ok" id="sfW"></span><span>Wallet</span><span id="sfWT">--</span></div>
  <div class="sf"><span>Template</span><span id="sfTm">--</span></div>
  <div style="margin-left:auto" class="sf"><span id="sfUp">--</span></div>
</div>

<script>
const MH=120,RS=10000,RT=30000;const H={hr:[],nh:[],mn:[],bl:[],sh:[],lb:[]};
let pSh=null;const rec={nh:0,mn:0};let t0=Date.now();
function fH(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}
function fHs(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(1)+'T';if(h>=1e9)return(h/1e9).toFixed(1)+'G';if(h>=1e6)return(h/1e6).toFixed(1)+'M';if(h>=1e3)return(h/1e3).toFixed(1)+'K';return h.toFixed(0)}
function fD(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);if(h>0)return h+'h '+m%60+'m';if(m>0)return m+'m '+s%60+'s';return s+'s'}
function push(a,v){a.push(v);if(a.length>MH)a.shift()}

/* ── Donut gauge ── */
function drawDonut(canvas,pct,text,color,trackColor) {
  if(!canvas)return;const S=canvas.clientWidth||160;const dpr=window.devicePixelRatio||1;
  canvas.width=Math.floor(S*dpr);canvas.height=Math.floor(S*dpr);
  const ctx=canvas.getContext('2d');ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,S,S);
  const cx=S/2,cy=S/2,r=S/2-14,lw=12;
  // Track
  ctx.lineWidth=lw;ctx.lineCap='round';
  ctx.strokeStyle=trackColor||'rgba(99,102,241,0.1)';
  ctx.beginPath();ctx.arc(cx,cy,r,-Math.PI*0.75,Math.PI*0.75);ctx.stroke();
  // Value arc
  if(pct>0.001){
    const endA=-Math.PI*0.75+Math.min(1,pct)*Math.PI*1.5;
    const grad=ctx.createConicGradient(-Math.PI*0.75,cx,cy);
    grad.addColorStop(0,color||'#6366f1');grad.addColorStop(0.5,color?color+'cc':'#818cf8');grad.addColorStop(1,color||'#6366f1');
    ctx.strokeStyle=grad;ctx.shadowColor=color||'#6366f1';ctx.shadowBlur=12;
    ctx.beginPath();ctx.arc(cx,cy,r,-Math.PI*0.75,endA);ctx.stroke();ctx.shadowBlur=0;
  }
  // Center text
  ctx.textAlign='center';ctx.textBaseline='middle';
  ctx.font=`700 ${Math.max(18,S*0.18)}px -apple-system,sans-serif`;
  ctx.fillStyle=color||'#e2e8f0';ctx.fillText(text,cx,cy-4);
  ctx.font=`500 ${Math.max(10,S*0.08)}px -apple-system,sans-serif`;
  ctx.fillStyle='#64748b';
}

/* ── Sparkline ── */
function drawSp(canvas,data,color){
  if(!canvas||!data.length)return;const W=canvas.clientWidth||120,Hh=canvas.clientHeight||48;
  const dpr=window.devicePixelRatio||1;canvas.width=Math.floor(W*dpr);canvas.height=Math.floor(Hh*dpr);
  const ctx=canvas.getContext('2d');ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,W,Hh);
  const p=3,pW=W-2*p,pH=Hh-2*p;
  let mn=Infinity,mx=-Infinity;for(const v of data){if(v<mn)mn=v;if(v>mx)mx=v}
  if(mn===mx){mn-=1;mx+=1}const rg=mx-mn;
  const fp=new Path2D();fp.moveTo(p,p+(1-(data[0]-mn)/rg)*pH);
  for(let i=0;i<data.length;i++)fp.lineTo(p+(i/(data.length-1))*pW,p+(1-(data[i]-mn)/rg)*pH);
  fp.lineTo(p+pW,p+pH);fp.lineTo(p,p+pH);fp.closePath();
  const g=ctx.createLinearGradient(0,p,0,p+pH);g.addColorStop(0,color+'22');g.addColorStop(1,'transparent');
  ctx.fillStyle=g;ctx.fill(fp);
  ctx.beginPath();for(let i=0;i<data.length;i++){const x=p+(i/(data.length-1))*pW,y=p+(1-(data[i]-mn)/rg)*pH;if(i===0)ctx.moveTo(x,y);else ctx.lineTo(x,y)}
  ctx.strokeStyle=color;ctx.lineWidth=2;ctx.stroke();
  const lx=p+pW,ly=p+(1-(data[data.length-1]-mn)/rg)*pH;
  ctx.beginPath();ctx.arc(lx,ly,3,0,Math.PI*2);ctx.fillStyle=color;ctx.fill();
}

/* ── Chart ── */
function drawCh(canvas,sets,labels){
  if(!canvas)return;const W=canvas.clientWidth||600,Hh=canvas.clientHeight||150;
  const dpr=window.devicePixelRatio||1;canvas.width=Math.floor(W*dpr);canvas.height=Math.floor(Hh*dpr);
  const ctx=canvas.getContext('2d');ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,W,Hh);
  const pL=42,pR=12,pT=10,pB=22,pW=W-pL-pR,pH=Hh-pT-pB;if(pW<=20||pH<=20)return;
  let gMn=Infinity,gMx=-Infinity;
  for(const s of sets)for(const v of s.data){if(v<gMn)gMn=v;if(v>gMx)gMx=v}
  if(gMn===gMx){gMn-=1;gMx+=1}const rg=gMx-gMn;
  ctx.strokeStyle='rgba(42,46,61,0.6)';ctx.lineWidth=0.5;
  for(let i=0;i<=4;i++){const y=pT+(i/4)*pH;ctx.beginPath();ctx.moveTo(pL,y);ctx.lineTo(pL+pW,y);ctx.stroke();
    ctx.fillStyle='#4a5568';ctx.font='10px sans-serif';ctx.textAlign='right';ctx.textBaseline='middle';
    ctx.fillText(fHs(gMx-(i/4)*rg),pL-4,y)}
  if(labels.length>1){ctx.fillStyle='#4a5568';ctx.font='9px sans-serif';ctx.textAlign='center';ctx.textBaseline='top';
    const step=Math.max(1,Math.floor(labels.length/6));
    for(let i=0;i<labels.length;i+=step)ctx.fillText(labels[i],pL+(i/(labels.length-1))*pW,pT+pH+4)}
  for(const s of sets){if(!s.data.length)continue;const n=s.data.length;
    const fp=new Path2D();fp.moveTo(pL,pT+(1-(s.data[0]-gMn)/rg)*pH);
    for(let i=0;i<n;i++)fp.lineTo(pL+(i/(n-1))*pW,pT+(1-(s.data[i]-gMn)/rg)*pH);
    fp.lineTo(pL+pW,pT+pH);fp.lineTo(pL,pT+pH);fp.closePath();
    const g=ctx.createLinearGradient(0,pT,0,pT+pH);g.addColorStop(0,s.color+'1a');g.addColorStop(1,'transparent');ctx.fillStyle=g;ctx.fill(fp);
    ctx.beginPath();for(let i=0;i<n;i++){const x=pL+(i/(n-1))*pW,y=pT+(1-(s.data[i]-gMn)/rg)*pH;if(i===0)ctx.moveTo(x,y);else ctx.lineTo(x,y)}
    ctx.strokeStyle=s.color;ctx.lineWidth=2;ctx.stroke()}
}

function sLed(id,s){const el=document.getElementById(id);if(!el)return;el.className='si si-'+(s==='green'?'g':s==='yellow'?'y':'r')}

async function fetchStats(){
  try{const r=await fetch('/api/pool/stats');const d=await r.json();const now=Date.now();
    document.getElementById('pool-name').textContent=d.name||'TAZ Mining Pool';
    const hr=d.hashrate_estimate||0;
    let mx=100;if(hr>100)mx=1000;if(hr>1000)mx=10000;if(hr>10000)mx=1e5;if(hr>1e5)mx=1e6;if(hr>1e6)mx=1e9;
    drawDonut(document.getElementById('donutHash'),hr/mx,fHs(hr),'#6366f1','rgba(99,102,241,0.1)');
    document.getElementById('dHashSub').textContent=fH(hr);
    const luck=d.luck_percent||0;
    const lColor=luck<=100?'#22c55e':luck<=150?'#eab308':'#ef4444';
    drawDonut(document.getElementById('donutLuck'),Math.min(luck/200,1),Math.round(luck)+'%',lColor,'rgba(99,102,241,0.1)');
    document.getElementById('dLuckSub').textContent='24-Hour Average';

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

    const ok=d.node_ok!==false;document.getElementById('hDot').className='dot '+(ok?'ok':'err');
    document.getElementById('hStat').textContent=ok?'Connected':'Stalled';
    document.getElementById('sfN').className='dot '+(ok?'ok':'err');document.getElementById('sfNT').textContent=ok?'OK':'Stalled';
    const wk=d.wallet_ok!==false;document.getElementById('sfW').className='dot '+(wk?'ok':'err');document.getElementById('sfWT').textContent=wk?'Online':'Offline';
    if(d.last_template_at)document.getElementById('sfTm').textContent=d.last_template_at.replace('T',' ').slice(0,19);
    document.getElementById('sfUp').textContent='Up '+fD(now-t0);

    push(H.hr,hr);push(H.nh,nh);push(H.mn,mc);push(H.bl,d.total_blocks||0);push(H.sh,Math.round(spm));push(H.lb,now);
    drawSp(document.getElementById('spNH'),H.nh,'#06b6d4');drawSp(document.getElementById('spMn'),H.mn,'#ec4899');
    const tl=H.lb.map(t=>{const d=new Date(t);return d.getHours().toString().padStart(2,'0')+':'+d.getMinutes().toString().padStart(2,'0')});
    drawCh(document.getElementById('chHash'),[{data:[...H.hr],color:'#6366f1'},{data:H.nh.map(v=>v?v/(Math.max(1,Math.max(...H.nh))/Math.max(1,Math.max(...H.hr)||1)):0),color:'#06b6d4'}],tl);
    drawCh(document.getElementById('chMine'),[{data:[...H.sh],color:'#eab308'},{data:[...H.mn],color:'#ec4899'}],tl);
  }catch(e){console.error(e)}}

async function fetchBlocks(){try{const r=await fetch('/api/blocks');const b=await r.json();const tb=document.querySelector('#blocks-table tbody');
  if(!b.length){tb.innerHTML='<tr><td colspan="6" style="color:var(--muted)">No blocks yet</td></tr>';return}
  tb.innerHTML=b.map(x=>{let ls='--',lc='var(--muted)';if(x.luck_percent!=null){ls=x.luck_percent.toFixed(0)+'%';lc=x.luck_percent<=100?'var(--green)':x.luck_percent<=150?'var(--yellow)':'var(--red)'}
    return'<tr><td style="color:var(--ink)">'+x.height+'</td><td title="'+x.hash+'">'+x.hash.substring(0,16)+'...</td><td>'+x.reward_zec.toFixed(4)+' TAZ</td><td style="color:'+lc+'">'+ls+'</td><td class="status-'+x.status+'">'+x.status+'</td><td>'+x.found_at+'</td></tr>'}).join('')}catch(e){}}

async function fetchMiners(){try{const r=await fetch('/api/miners');const m=await r.json();const tb=document.querySelector('#miners-table tbody');
  if(!m.length){tb.innerHTML='<tr><td colspan="6" style="color:var(--muted)">No miners yet</td></tr>';return}
  tb.innerHTML=m.map(x=>'<tr><td class="addr-cell" title="'+x.address+'"><span class="addr-link" onclick="doLookup(\''+x.address.replace(/'/g,"\\'")+'\')">' + x.address + '</span></td><td>'+fH(x.hashrate_1m)+'</td><td>'+fH(x.hashrate)+'</td><td>'+x.worker_count+'</td><td>'+x.share_count.toLocaleString()+'</td><td style="color:var(--accent)">'+x.pending_zec.toFixed(8)+'</td></tr>').join('')}catch(e){}}

async function fetchPayouts(){try{const r=await fetch('/api/payouts');const p=await r.json();const tb=document.querySelector('#payouts-table tbody');
  if(!p.length){tb.innerHTML='<tr><td colspan="4" style="color:var(--muted)">No payouts yet</td></tr>';return}
  tb.innerHTML=p.map(x=>'<tr><td class="addr-cell" title="'+x.miner_address+'" style="color:var(--ink)">'+x.miner_address+'</td><td style="color:var(--green)">'+x.amount_zec.toFixed(8)+' TAZ</td><td title="'+(x.txid||'')+'">'+((x.txid||'').substring(0,16)||'--')+'</td><td>'+x.created_at+'</td></tr>').join('')}catch(e){}}

function doLookup(a){document.getElementById('miner-address').value=a;lookupMiner()}
async function lookupMiner(){const a=document.getElementById('miner-address').value.trim();if(!a)return;
  try{const r=await fetch('/api/miner/'+encodeURIComponent(a));if(!r.ok){alert('Not found');return}const d=await r.json();
    document.getElementById('miner-info').style.display='block';
    document.getElementById('miner-stats-grid').innerHTML='<div class="miner-stat"><div class="mc-label">Pending</div><div class="mc-val" style="font-size:18px;color:var(--accent)">'+d.balance.pending_zec.toFixed(8)+'</div></div><div class="miner-stat"><div class="mc-label">Paid</div><div class="mc-val" style="font-size:18px;color:var(--green)">'+d.balance.paid_zec.toFixed(8)+'</div></div><div class="miner-stat"><div class="mc-label">Workers</div><div class="mc-val" style="font-size:18px">'+d.workers.length+'</div></div>';
    document.querySelector('#workers-table tbody').innerHTML=d.workers.map(w=>'<tr><td>'+w.name+'</td><td>'+w.last_seen+'</td></tr>').join('')}catch(e){}}

document.addEventListener('DOMContentLoaded',()=>{fetchStats();fetchMiners();fetchBlocks();fetchPayouts();
  setInterval(fetchStats,RS);setInterval(fetchMiners,RS);setInterval(fetchBlocks,RT);setInterval(fetchPayouts,RT)});
</script>
</body>
</html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 25 — "Plasma" — Vibrant purple/magenta with animated gradient
// borders, glassmorphism cards, neon accents, modern premium feel
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW25: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TAZ Mining Pool — Plasma</title>
<style>
:root{--bg:#0d0015;--panel:rgba(20,10,40,0.75);--accent:#a855f7;--accent2:#c084fc;--accent3:#e879f9;
  --cyan:#22d3ee;--green:#4ade80;--yellow:#fbbf24;--red:#f87171;
  --ink:#f0e6ff;--muted:#9584b0;--line:rgba(168,85,247,0.2)}
*{margin:0;padding:0;box-sizing:border-box}
html,body{background:var(--bg);color:var(--ink);min-height:100vh;overflow-x:hidden}
body{font-family:'Inter',-apple-system,BlinkMacSystemFont,sans-serif;font-size:13px;position:relative}
body::before{content:'';position:fixed;inset:0;z-index:0;pointer-events:none;
  background:radial-gradient(800px at 20% 20%,rgba(168,85,247,0.08),transparent),
    radial-gradient(600px at 80% 80%,rgba(236,72,153,0.06),transparent),
    radial-gradient(700px at 50% 50%,rgba(34,211,238,0.04),transparent)}

main{max-width:1440px;margin:0 auto;padding:20px;position:relative;z-index:1}

/* ── Animated gradient border utility ── */
@property --angle{syntax:'<angle>';initial-value:0deg;inherits:false}
@keyframes spin{to{--angle:360deg}}

/* ── Header ── */
.header{display:flex;align-items:center;gap:16px;margin-bottom:20px;padding:14px 20px;
  background:var(--panel);backdrop-filter:blur(12px);-webkit-backdrop-filter:blur(12px);
  border-radius:16px;border:1px solid var(--line);
  box-shadow:0 8px 32px rgba(168,85,247,0.08)}
.header h1{font-size:18px;font-weight:700;
  background:linear-gradient(135deg,var(--accent2),var(--accent3),var(--cyan));
  -webkit-background-clip:text;-webkit-text-fill-color:transparent;background-clip:text}
.header .tag{background:linear-gradient(135deg,var(--accent),var(--accent3));color:white;
  padding:3px 10px;border-radius:8px;font-size:10px;font-weight:700;letter-spacing:0.5px}
.header .right{margin-left:auto;display:flex;align-items:center;gap:14px}
.glow-dot{width:10px;height:10px;border-radius:50%;display:inline-block}
.glow-dot.ok{background:var(--green);box-shadow:0 0 12px rgba(74,222,128,0.5)}
.glow-dot.err{background:var(--red);box-shadow:0 0 12px rgba(248,113,113,0.5)}
.nav-link{color:var(--muted);font-size:12px;text-decoration:none;font-weight:500;transition:color 0.2s}
.nav-link:hover{color:var(--accent2)}

/* ── Grid ── */
.grid{display:grid;grid-template-columns:repeat(12,1fr);gap:14px;margin-bottom:20px}

/* ── Gauge cards ── */
.gauge-card{grid-column:span 3;background:var(--panel);backdrop-filter:blur(8px);
  border:1px solid var(--line);border-radius:20px;padding:16px;
  display:flex;flex-direction:column;align-items:center;gap:8px;
  box-shadow:0 4px 24px rgba(168,85,247,0.08);animation:spin 8s linear infinite;
  background-image:conic-gradient(from var(--angle,0deg),rgba(168,85,247,0.15),rgba(236,72,153,0.15),rgba(34,211,238,0.15),rgba(168,85,247,0.15));
  background-origin:border-box}
.gauge-card canvas{width:160px;height:160px}
.gauge-card .g-label{font-size:11px;text-transform:uppercase;letter-spacing:1px;color:var(--muted);font-weight:600}
.gauge-card .g-sub{font-size:11px;color:var(--muted)}

/* ── Cards ── */
.card{grid-column:span 2;background:var(--panel);backdrop-filter:blur(8px);
  border:1px solid var(--line);border-radius:16px;padding:14px 16px;
  display:flex;flex-direction:column;gap:6px;
  box-shadow:0 4px 16px rgba(168,85,247,0.06);transition:transform 0.2s,box-shadow 0.2s}
.card:hover{transform:translateY(-2px);box-shadow:0 8px 24px rgba(168,85,247,0.12)}
.card .label{font-size:10px;text-transform:uppercase;letter-spacing:0.8px;color:var(--muted);font-weight:600;
  display:flex;align-items:center;gap:6px}
.card .val{font-size:26px;font-weight:700;letter-spacing:-0.5px;
  background:linear-gradient(135deg,var(--accent2),var(--cyan));
  -webkit-background-clip:text;-webkit-text-fill-color:transparent;background-clip:text}
.card .sub{font-size:11px;color:var(--muted)}

/* ── Spark cards ── */
.spark-card{grid-column:span 3;background:var(--panel);backdrop-filter:blur(8px);
  border:1px solid var(--line);border-radius:16px;padding:14px 16px;
  display:flex;flex-direction:column;gap:6px;box-shadow:0 4px 16px rgba(168,85,247,0.06)}
.spark-card .sp-row{display:flex;align-items:center;gap:12px}
.spark-card .val{font-size:22px;font-weight:700;white-space:nowrap;
  background:linear-gradient(135deg,var(--accent2),var(--cyan));
  -webkit-background-clip:text;-webkit-text-fill-color:transparent;background-clip:text}
.spark-card canvas{flex:1;height:48px;border-radius:10px;
  background:rgba(168,85,247,0.05);border:1px solid rgba(168,85,247,0.1)}

/* ── LED ── */
.led{width:8px;height:8px;border-radius:50%;display:inline-block}
.led-g{background:var(--green);box-shadow:0 0 8px rgba(74,222,128,0.5)}
.led-y{background:var(--yellow);box-shadow:0 0 8px rgba(251,191,36,0.5)}
.led-r{background:var(--red);box-shadow:0 0 8px rgba(248,113,113,0.5)}

/* ── Charts ── */
.chart-grid{display:grid;grid-template-columns:1fr 1fr;gap:14px;margin-bottom:20px}
.chart-panel{background:var(--panel);backdrop-filter:blur(8px);border:1px solid var(--line);
  border-radius:20px;padding:18px;box-shadow:0 4px 24px rgba(168,85,247,0.06)}
.chart-panel .cp-header{display:flex;justify-content:space-between;align-items:center;margin-bottom:10px}
.chart-panel .cp-title{font-size:14px;font-weight:600;
  background:linear-gradient(135deg,var(--accent2),var(--accent3));
  -webkit-background-clip:text;-webkit-text-fill-color:transparent;background-clip:text}
.chart-panel .cp-legend{display:flex;gap:12px;font-size:11px;color:var(--muted)}
.chart-panel .cp-legend .sw{width:10px;height:3px;display:inline-block;border-radius:2px;margin-right:4px;vertical-align:middle}
.chart-panel canvas{width:100%;height:150px;border-radius:12px;
  background:rgba(168,85,247,0.04);border:1px solid rgba(168,85,247,0.08)}

/* ── Tables ── */
.sec-title{font-size:12px;font-weight:600;color:var(--muted);text-transform:uppercase;letter-spacing:0.8px;padding:8px 0}
.table-wrap{border:1px solid var(--line);border-radius:16px;overflow:hidden;margin-bottom:20px;
  box-shadow:0 4px 16px rgba(168,85,247,0.04)}
table{width:100%;border-collapse:collapse;background:var(--panel);backdrop-filter:blur(4px)}
th{font-size:11px;font-weight:600;color:var(--muted);padding:10px 14px;text-align:left;
  background:rgba(13,0,21,0.5);border-bottom:1px solid var(--line)}
td{font-size:12px;padding:8px 14px;border-bottom:1px solid rgba(168,85,247,0.08);color:var(--muted)}
tr:hover td{background:rgba(168,85,247,0.05)}
.status-confirmed{color:var(--green);font-weight:600}.status-pending{color:var(--yellow);font-weight:600}.status-orphaned{color:var(--red);font-weight:600}
.addr-cell{max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.addr-link{color:var(--ink);cursor:pointer;text-decoration:none;font-weight:500}.addr-link:hover{color:var(--accent2)}

/* ── Lookup ── */
.lookup-row{display:flex;gap:0;margin-bottom:20px}
.lookup-row input{flex:1;padding:12px 16px;background:var(--panel);border:1px solid var(--line);
  border-radius:12px 0 0 12px;color:var(--ink);font-size:13px;outline:none;font-family:inherit;
  backdrop-filter:blur(4px)}
.lookup-row input:focus{border-color:var(--accent)}
.lookup-row input::placeholder{color:rgba(149,132,176,0.5)}
.lookup-row button{padding:12px 24px;background:linear-gradient(135deg,var(--accent),var(--accent3));
  border:none;border-radius:0 12px 12px 0;color:white;font-size:12px;font-weight:700;cursor:pointer;font-family:inherit}
.lookup-row button:hover{filter:brightness(1.1)}
#miner-info{display:none}
.miner-stats-row{display:grid;grid-template-columns:repeat(3,1fr);gap:14px;margin-bottom:20px}
.miner-stat{background:var(--panel);border:1px solid var(--line);border-radius:14px;padding:14px 16px;backdrop-filter:blur(4px)}

/* ── Footer ── */
.footer{background:rgba(13,0,21,0.92);backdrop-filter:blur(8px);border-top:1px solid var(--line);
  padding:8px 20px;display:flex;align-items:center;gap:20px;font-size:11px;color:var(--muted);
  position:fixed;bottom:0;left:0;right:0;z-index:10}
.sf{display:flex;align-items:center;gap:6px}
.footer-spacer{height:3rem}

@media(max-width:1200px){.grid{grid-template-columns:repeat(6,1fr)}.gauge-card{grid-column:span 3}.chart-grid{grid-template-columns:1fr}}
@media(max-width:800px){.grid{grid-template-columns:repeat(4,1fr)}.gauge-card{grid-column:span 4}.spark-card{grid-column:span 4}}
</style>
</head>
<body>
<main>
<div class="header">
  <h1 id="pool-name">TAZ Mining Pool</h1><span class="tag">TESTNET</span>
  <div class="right">
    <span class="glow-dot ok" id="hDot"></span><span style="font-size:11px;color:var(--muted)" id="hStat">Connected</span>
    <a href="/previews" class="nav-link">Themes</a><a href="/" class="nav-link">Classic</a><a href="/zallet" class="nav-link">Wallet</a>
  </div>
</div>

<div class="grid">
  <div class="gauge-card"><div class="g-label">Pool Hashrate</div><canvas id="gHash"></canvas><div class="g-sub" id="gHashSub">--</div></div>
  <div class="gauge-card"><div class="g-label">Luck (24h)</div><canvas id="gLuck"></canvas><div class="g-sub" id="gLuckSub">--</div></div>

  <div class="spark-card"><div class="label">Network Hashrate</div>
    <div class="sp-row"><div class="val" id="vNH">--</div><canvas id="spNH"></canvas></div>
    <div class="sub" style="font-size:10px;color:var(--muted)">Peak: <span id="recNH">--</span></div></div>
  <div class="spark-card"><div class="label">Connected Miners</div>
    <div class="sp-row"><div class="val" id="vMn">0</div><canvas id="spMn"></canvas></div>
    <div class="sub" style="font-size:10px;color:var(--muted)">Peak: <span id="recMn">0</span></div></div>

  <div class="card"><div class="label"><span class="led led-g" id="ldBl"></span>Blocks</div><div class="val" id="vBl">0</div></div>
  <div class="card"><div class="label"><span class="led led-g" id="ldIm"></span>Immature</div><div class="val" id="vIm">0</div></div>
  <div class="card"><div class="label"><span class="led led-g" id="ldPd"></span>Pending</div><div class="val" id="vPd">0</div></div>
  <div class="card"><div class="label">Fee</div><div class="val" id="vFee">--</div></div>
  <div class="card"><div class="label">Net Share</div><div class="val" id="vPct">--</div></div>
  <div class="card"><div class="label">Share Rate</div><div class="val" id="vSR">0</div><div class="sub">/min</div></div>
  <div class="card"><div class="label">Shares</div><div class="val" id="vTS" style="font-size:16px">0</div></div>
  <div class="card"><div class="label">Stratum</div><div class="val" id="vPort">--</div></div>
</div>

<div class="chart-grid">
  <div class="chart-panel"><div class="cp-header"><span class="cp-title">Hashrate</span>
    <div class="cp-legend"><span><span class="sw" style="background:var(--accent)"></span>Pool</span>
      <span><span class="sw" style="background:var(--cyan)"></span>Network</span></div></div>
    <canvas id="chHash"></canvas></div>
  <div class="chart-panel"><div class="cp-header"><span class="cp-title">Activity</span>
    <div class="cp-legend"><span><span class="sw" style="background:var(--yellow)"></span>Shares/m</span>
      <span><span class="sw" style="background:var(--accent3)"></span>Miners</span></div></div>
    <canvas id="chMine"></canvas></div>
</div>

<div class="sec-title">Miner Lookup</div>
<div class="lookup-row">
  <input type="text" id="miner-address" placeholder="Enter Zcash address..." onkeydown="if(event.key==='Enter')lookupMiner()">
  <button onclick="lookupMiner()">Search</button>
</div>
<div id="miner-info"><div class="miner-stats-row" id="miner-stats-grid"></div>
  <div class="sec-title">Workers</div>
  <div class="table-wrap"><table id="workers-table"><thead><tr><th>Name</th><th>Last Seen</th></tr></thead><tbody></tbody></table></div></div>

<div class="sec-title">Miners</div>
<div class="table-wrap"><table id="miners-table"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>Workers</th><th>Shares</th><th>Pending</th></tr></thead>
  <tbody><tr><td colspan="6" style="color:var(--muted)">Loading...</td></tr></tbody></table></div>

<div class="sec-title">Blocks</div>
<div class="table-wrap"><table id="blocks-table"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead>
  <tbody><tr><td colspan="6" style="color:var(--muted)">Loading...</td></tr></tbody></table></div>

<div class="sec-title">Payouts</div>
<div class="table-wrap"><table id="payouts-table"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead>
  <tbody><tr><td colspan="4" style="color:var(--muted)">Loading...</td></tr></tbody></table></div>

<div class="footer-spacer"></div>
</main>

<div class="footer">
  <div class="sf"><span class="glow-dot ok" id="sfN"></span><span>Node</span><span id="sfNT">--</span></div>
  <div class="sf"><span class="glow-dot ok" id="sfW"></span><span>Wallet</span><span id="sfWT">--</span></div>
  <div class="sf"><span>Template</span><span id="sfTm">--</span></div>
  <div style="margin-left:auto" class="sf"><span id="sfUp">--</span></div>
</div>

<script>
const MH=120,RS=10000,RT=30000;const H={hr:[],nh:[],mn:[],bl:[],sh:[],lb:[]};
let pSh=null;const rec={nh:0,mn:0};let t0=Date.now();
function fH(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}
function fHs(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(1)+'T';if(h>=1e9)return(h/1e9).toFixed(1)+'G';if(h>=1e6)return(h/1e6).toFixed(1)+'M';if(h>=1e3)return(h/1e3).toFixed(1)+'K';return h.toFixed(0)}
function fD(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);if(h>0)return h+'h '+m%60+'m';if(m>0)return m+'m '+s%60+'s';return s+'s'}
function push(a,v){a.push(v);if(a.length>MH)a.shift()}

/* ── Donut with glow ── */
function drawDonut(canvas,pct,text,color) {
  if(!canvas)return;const S=canvas.clientWidth||160;const dpr=window.devicePixelRatio||1;
  canvas.width=Math.floor(S*dpr);canvas.height=Math.floor(S*dpr);
  const ctx=canvas.getContext('2d');ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,S,S);
  const cx=S/2,cy=S/2,r=S/2-16,lw=10;
  ctx.lineWidth=lw;ctx.lineCap='round';
  ctx.strokeStyle='rgba(168,85,247,0.08)';
  ctx.beginPath();ctx.arc(cx,cy,r,-Math.PI*0.75,Math.PI*0.75);ctx.stroke();
  if(pct>0.001){
    const endA=-Math.PI*0.75+Math.min(1,pct)*Math.PI*1.5;
    ctx.strokeStyle=color||'#a855f7';ctx.shadowColor=color||'#a855f7';ctx.shadowBlur=16;
    ctx.beginPath();ctx.arc(cx,cy,r,-Math.PI*0.75,endA);ctx.stroke();ctx.shadowBlur=0;
    // Outer glow ring
    ctx.lineWidth=2;ctx.strokeStyle=(color||'#a855f7')+'44';ctx.shadowColor=color||'#a855f7';ctx.shadowBlur=20;
    ctx.beginPath();ctx.arc(cx,cy,r+8,-Math.PI*0.75,endA);ctx.stroke();ctx.shadowBlur=0;
  }
  ctx.textAlign='center';ctx.textBaseline='middle';
  ctx.font=`700 ${Math.max(20,S*0.19)}px -apple-system,sans-serif`;
  ctx.fillStyle=color||'#f0e6ff';ctx.fillText(text,cx,cy-2);
}

/* ── Sparkline ── */
function drawSp(canvas,data,color){
  if(!canvas||!data.length)return;const W=canvas.clientWidth||120,Hh=canvas.clientHeight||48;
  const dpr=window.devicePixelRatio||1;canvas.width=Math.floor(W*dpr);canvas.height=Math.floor(Hh*dpr);
  const ctx=canvas.getContext('2d');ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,W,Hh);
  const p=3,pW=W-2*p,pH=Hh-2*p;
  let mn=Infinity,mx=-Infinity;for(const v of data){if(v<mn)mn=v;if(v>mx)mx=v}
  if(mn===mx){mn-=1;mx+=1}const rg=mx-mn;
  const fp=new Path2D();fp.moveTo(p,p+(1-(data[0]-mn)/rg)*pH);
  for(let i=0;i<data.length;i++)fp.lineTo(p+(i/(data.length-1))*pW,p+(1-(data[i]-mn)/rg)*pH);
  fp.lineTo(p+pW,p+pH);fp.lineTo(p,p+pH);fp.closePath();
  const g=ctx.createLinearGradient(0,p,0,p+pH);g.addColorStop(0,color+'33');g.addColorStop(1,'transparent');
  ctx.fillStyle=g;ctx.fill(fp);
  ctx.beginPath();for(let i=0;i<data.length;i++){const x=p+(i/(data.length-1))*pW,y=p+(1-(data[i]-mn)/rg)*pH;if(i===0)ctx.moveTo(x,y);else ctx.lineTo(x,y)}
  ctx.strokeStyle=color;ctx.lineWidth=2;ctx.shadowColor=color;ctx.shadowBlur=6;ctx.stroke();ctx.shadowBlur=0;
  const lx=p+pW,ly=p+(1-(data[data.length-1]-mn)/rg)*pH;
  ctx.beginPath();ctx.arc(lx,ly,3,0,Math.PI*2);ctx.fillStyle=color;ctx.shadowColor=color;ctx.shadowBlur=8;ctx.fill();ctx.shadowBlur=0;
}

/* ── Chart ── */
function drawCh(canvas,sets,labels){
  if(!canvas)return;const W=canvas.clientWidth||600,Hh=canvas.clientHeight||150;
  const dpr=window.devicePixelRatio||1;canvas.width=Math.floor(W*dpr);canvas.height=Math.floor(Hh*dpr);
  const ctx=canvas.getContext('2d');ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,W,Hh);
  const pL=42,pR=12,pT=10,pB=22,pW=W-pL-pR,pH=Hh-pT-pB;if(pW<=20||pH<=20)return;
  let gMn=Infinity,gMx=-Infinity;
  for(const s of sets)for(const v of s.data){if(v<gMn)gMn=v;if(v>gMx)gMx=v}
  if(gMn===gMx){gMn-=1;gMx+=1}const rg=gMx-gMn;
  ctx.strokeStyle='rgba(168,85,247,0.08)';ctx.lineWidth=0.5;
  for(let i=0;i<=4;i++){const y=pT+(i/4)*pH;ctx.beginPath();ctx.moveTo(pL,y);ctx.lineTo(pL+pW,y);ctx.stroke();
    ctx.fillStyle='#6b5a80';ctx.font='10px sans-serif';ctx.textAlign='right';ctx.textBaseline='middle';
    ctx.fillText(fHs(gMx-(i/4)*rg),pL-4,y)}
  if(labels.length>1){ctx.fillStyle='#6b5a80';ctx.font='9px sans-serif';ctx.textAlign='center';ctx.textBaseline='top';
    const step=Math.max(1,Math.floor(labels.length/6));
    for(let i=0;i<labels.length;i+=step)ctx.fillText(labels[i],pL+(i/(labels.length-1))*pW,pT+pH+4)}
  for(const s of sets){if(!s.data.length)continue;const n=s.data.length;
    const fp=new Path2D();fp.moveTo(pL,pT+(1-(s.data[0]-gMn)/rg)*pH);
    for(let i=0;i<n;i++)fp.lineTo(pL+(i/(n-1))*pW,pT+(1-(s.data[i]-gMn)/rg)*pH);
    fp.lineTo(pL+pW,pT+pH);fp.lineTo(pL,pT+pH);fp.closePath();
    const g=ctx.createLinearGradient(0,pT,0,pT+pH);g.addColorStop(0,s.color+'22');g.addColorStop(1,'transparent');ctx.fillStyle=g;ctx.fill(fp);
    ctx.beginPath();for(let i=0;i<n;i++){const x=pL+(i/(n-1))*pW,y=pT+(1-(s.data[i]-gMn)/rg)*pH;if(i===0)ctx.moveTo(x,y);else ctx.lineTo(x,y)}
    ctx.strokeStyle=s.color;ctx.lineWidth=2;ctx.shadowColor=s.color;ctx.shadowBlur=6;ctx.stroke();ctx.shadowBlur=0}
}

function sLed(id,s){const el=document.getElementById(id);if(!el)return;el.className='led led-'+(s==='green'?'g':s==='yellow'?'y':'r')}

async function fetchStats(){
  try{const r=await fetch('/api/pool/stats');const d=await r.json();const now=Date.now();
    document.getElementById('pool-name').textContent=d.name||'TAZ Mining Pool';
    const hr=d.hashrate_estimate||0;
    let mx=100;if(hr>100)mx=1000;if(hr>1000)mx=10000;if(hr>10000)mx=1e5;if(hr>1e5)mx=1e6;if(hr>1e6)mx=1e9;
    drawDonut(document.getElementById('gHash'),hr/mx,fHs(hr),'#a855f7');
    document.getElementById('gHashSub').textContent=fH(hr);
    const luck=d.luck_percent||0;
    const lc=luck<=100?'#4ade80':luck<=150?'#fbbf24':'#f87171';
    drawDonut(document.getElementById('gLuck'),Math.min(luck/200,1),Math.round(luck)+'%',lc);
    document.getElementById('gLuckSub').textContent='24h Average';

    document.getElementById('vNH').textContent=fH(d.network_hashrate);document.getElementById('vMn').textContent=d.connected_miners||0;
    document.getElementById('vBl').textContent=d.total_blocks||0;document.getElementById('vIm').textContent=d.immature_blocks||0;
    document.getElementById('vPd').textContent=d.pending_payout_blocks||0;document.getElementById('vFee').textContent=(d.fee_percent||0)+'%';
    document.getElementById('vPort').textContent=d.stratum_port||'--';document.getElementById('vTS').textContent=(d.total_shares||0).toLocaleString();
    const pe=document.getElementById('vPct');if(d.pool_percent_24h!=null)pe.textContent=d.pool_percent_24h.toFixed(2)+'%';
    const spm=pSh!==null?Math.max(0,(d.total_shares-pSh)*(60000/RS)):0;pSh=d.total_shares;
    document.getElementById('vSR').textContent=Math.round(spm);

    const nh=d.network_hashrate||0;if(nh>rec.nh){rec.nh=nh;document.getElementById('recNH').textContent=fH(nh)}
    const mc=d.connected_miners||0;if(mc>rec.mn){rec.mn=mc;document.getElementById('recMn').textContent=mc}
    sLed('ldIm',(d.immature_blocks||0)>0?'yellow':'green');sLed('ldPd',(d.pending_payout_blocks||0)>0?'yellow':'green');

    const ok=d.node_ok!==false;document.getElementById('hDot').className='glow-dot '+(ok?'ok':'err');
    document.getElementById('hStat').textContent=ok?'Connected':'Stalled';
    document.getElementById('sfN').className='glow-dot '+(ok?'ok':'err');document.getElementById('sfNT').textContent=ok?'OK':'Stalled';
    const wk=d.wallet_ok!==false;document.getElementById('sfW').className='glow-dot '+(wk?'ok':'err');document.getElementById('sfWT').textContent=wk?'Online':'Offline';
    if(d.last_template_at)document.getElementById('sfTm').textContent=d.last_template_at.replace('T',' ').slice(0,19);
    document.getElementById('sfUp').textContent='Up '+fD(now-t0);

    push(H.hr,hr);push(H.nh,nh);push(H.mn,mc);push(H.bl,d.total_blocks||0);push(H.sh,Math.round(spm));push(H.lb,now);
    drawSp(document.getElementById('spNH'),H.nh,'#22d3ee');drawSp(document.getElementById('spMn'),H.mn,'#e879f9');
    const tl=H.lb.map(t=>{const d=new Date(t);return d.getHours().toString().padStart(2,'0')+':'+d.getMinutes().toString().padStart(2,'0')});
    drawCh(document.getElementById('chHash'),[{data:[...H.hr],color:'#a855f7'},{data:H.nh.map(v=>v?v/(Math.max(1,Math.max(...H.nh))/Math.max(1,Math.max(...H.hr)||1)):0),color:'#22d3ee'}],tl);
    drawCh(document.getElementById('chMine'),[{data:[...H.sh],color:'#fbbf24'},{data:[...H.mn],color:'#e879f9'}],tl);
  }catch(e){console.error(e)}}

async function fetchBlocks(){try{const r=await fetch('/api/blocks');const b=await r.json();const tb=document.querySelector('#blocks-table tbody');
  if(!b.length){tb.innerHTML='<tr><td colspan="6" style="color:var(--muted)">No blocks</td></tr>';return}
  tb.innerHTML=b.map(x=>{let ls='--',lc='var(--muted)';if(x.luck_percent!=null){ls=x.luck_percent.toFixed(0)+'%';lc=x.luck_percent<=100?'var(--green)':x.luck_percent<=150?'var(--yellow)':'var(--red)'}
    return'<tr><td style="color:var(--ink)">'+x.height+'</td><td title="'+x.hash+'">'+x.hash.substring(0,16)+'...</td><td>'+x.reward_zec.toFixed(4)+' TAZ</td><td style="color:'+lc+'">'+ls+'</td><td class="status-'+x.status+'">'+x.status+'</td><td>'+x.found_at+'</td></tr>'}).join('')}catch(e){}}

async function fetchMiners(){try{const r=await fetch('/api/miners');const m=await r.json();const tb=document.querySelector('#miners-table tbody');
  if(!m.length){tb.innerHTML='<tr><td colspan="6" style="color:var(--muted)">No miners</td></tr>';return}
  tb.innerHTML=m.map(x=>'<tr><td class="addr-cell" title="'+x.address+'"><span class="addr-link" onclick="doLookup(\''+x.address.replace(/'/g,"\\'")+'\')">' + x.address + '</span></td><td>'+fH(x.hashrate_1m)+'</td><td>'+fH(x.hashrate)+'</td><td>'+x.worker_count+'</td><td>'+x.share_count.toLocaleString()+'</td><td style="color:var(--accent)">'+x.pending_zec.toFixed(8)+'</td></tr>').join('')}catch(e){}}

async function fetchPayouts(){try{const r=await fetch('/api/payouts');const p=await r.json();const tb=document.querySelector('#payouts-table tbody');
  if(!p.length){tb.innerHTML='<tr><td colspan="4" style="color:var(--muted)">No payouts</td></tr>';return}
  tb.innerHTML=p.map(x=>'<tr><td class="addr-cell" title="'+x.miner_address+'" style="color:var(--ink)">'+x.miner_address+'</td><td style="color:var(--green)">'+x.amount_zec.toFixed(8)+' TAZ</td><td title="'+(x.txid||'')+'">'+((x.txid||'').substring(0,16)||'--')+'</td><td>'+x.created_at+'</td></tr>').join('')}catch(e){}}

function doLookup(a){document.getElementById('miner-address').value=a;lookupMiner()}
async function lookupMiner(){const a=document.getElementById('miner-address').value.trim();if(!a)return;
  try{const r=await fetch('/api/miner/'+encodeURIComponent(a));if(!r.ok){alert('Not found');return}const d=await r.json();
    document.getElementById('miner-info').style.display='block';
    document.getElementById('miner-stats-grid').innerHTML='<div class="miner-stat"><div class="label">Pending</div><div class="val" style="font-size:18px">'+d.balance.pending_zec.toFixed(8)+'</div></div><div class="miner-stat"><div class="label">Paid</div><div class="val" style="font-size:18px;-webkit-text-fill-color:var(--green);color:var(--green)">'+d.balance.paid_zec.toFixed(8)+'</div></div><div class="miner-stat"><div class="label">Workers</div><div class="val" style="font-size:18px">'+d.workers.length+'</div></div>';
    document.querySelector('#workers-table tbody').innerHTML=d.workers.map(w=>'<tr><td>'+w.name+'</td><td>'+w.last_seen+'</td></tr>').join('')}catch(e){}}

document.addEventListener('DOMContentLoaded',()=>{fetchStats();fetchMiners();fetchBlocks();fetchPayouts();
  setInterval(fetchStats,RS);setInterval(fetchMiners,RS);setInterval(fetchBlocks,RT);setInterval(fetchPayouts,RT)});
</script>
</body>
</html>
"##;
