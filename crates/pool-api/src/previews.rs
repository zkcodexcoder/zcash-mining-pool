use axum::response::Html;

pub async fn preview1() -> Html<String> { Html(PREVIEW1.to_string()) }
pub async fn preview2() -> Html<String> { Html(PREVIEW2.to_string()) }
pub async fn preview3() -> Html<String> { Html(PREVIEW3.to_string()) }
pub async fn preview4() -> Html<String> { Html(PREVIEW4.to_string()) }
pub async fn preview5() -> Html<String> { Html(PREVIEW5.to_string()) }
pub async fn preview6() -> Html<String> { Html(PREVIEW6.to_string()) }
pub async fn preview7() -> Html<String> { Html(PREVIEW7.to_string()) }
pub async fn preview8() -> Html<String> { Html(PREVIEW8.to_string()) }
pub async fn preview9() -> Html<String> { Html(PREVIEW9.to_string()) }
pub async fn preview10() -> Html<String> { Html(PREVIEW10.to_string()) }
pub async fn preview11() -> Html<String> { Html(PREVIEW11.to_string()) }
pub async fn preview12() -> Html<String> { Html(PREVIEW12.to_string()) }
pub async fn preview13() -> Html<String> { Html(PREVIEW13.to_string()) }
pub async fn preview14() -> Html<String> { Html(PREVIEW14.to_string()) }
pub async fn preview15() -> Html<String> { Html(PREVIEW15.to_string()) }
pub async fn preview16() -> Html<String> { Html(PREVIEW16.to_string()) }
pub async fn preview17() -> Html<String> { Html(PREVIEW17.to_string()) }
pub async fn preview18() -> Html<String> { Html(PREVIEW18.to_string()) }
pub async fn preview19() -> Html<String> { Html(PREVIEW19.to_string()) }
pub async fn preview20() -> Html<String> { Html(PREVIEW20.to_string()) }
pub async fn preview21() -> Html<String> { Html(PREVIEW21.to_string()) }
pub async fn gallery() -> Html<String> { Html(GALLERY.to_string()) }

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 1 — "Phosphor" — VFD green neon on deep black, scanlines, glow
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW1: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TAZ Mining Pool — Phosphor</title>
<style>
:root {
  --bg: #020a04;
  --panel: rgba(4, 18, 8, 0.85);
  --accent: #39ff14;
  --accent2: #00ff88;
  --dim: #1a5c2a;
  --dimmer: #0d3318;
  --ink: #d0ffd8;
  --muted: #4a8a5a;
  --line: rgba(57, 255, 20, 0.15);
  --glow: rgba(57, 255, 20, 0.22);
  --glow2: rgba(0, 255, 136, 0.12);
}
*{margin:0;padding:0;box-sizing:border-box}
html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:'Courier New','Lucida Console',monospace;font-size:13px;position:relative;overflow-x:hidden}
/* Scanline overlay */
body::after{content:'';position:fixed;inset:0;pointer-events:none;z-index:9999;
  background:repeating-linear-gradient(0deg,rgba(0,0,0,0.12) 0px,rgba(0,0,0,0.12) 1px,transparent 1px,transparent 3px);opacity:0.6}
/* Radial glow overlay */
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;
  background:radial-gradient(900px 500px at 10% -5%,rgba(57,255,20,0.08),transparent 60%),
             radial-gradient(700px 400px at 90% 105%,rgba(0,255,136,0.06),transparent 55%)}
main{max-width:1300px;margin:0 auto;padding:20px;position:relative;z-index:2}
/* ── Header ── */
.top{display:flex;justify-content:space-between;align-items:center;padding:14px 18px;border-radius:12px;margin-bottom:16px;
  background:radial-gradient(120% 160% at 0% 0%,rgba(57,255,20,0.06),transparent 50%),linear-gradient(155deg,rgba(4,18,8,0.9),rgba(3,12,6,0.85));
  border:1px solid var(--line);box-shadow:inset 0 0 0 1px rgba(57,255,20,0.04),0 0 20px var(--glow),0 0 30px var(--glow2)}
.top h1{font-size:20px;font-weight:700;color:var(--accent);letter-spacing:3px;text-transform:uppercase;
  text-shadow:0 0 10px rgba(57,255,20,0.5),0 0 25px rgba(57,255,20,0.3)}
.top .sub{color:var(--muted);font-size:11px;margin-top:2px}
.status{display:inline-flex;align-items:center;gap:8px;font-size:12px;
  border:1px solid rgba(57,255,20,0.25);padding:6px 12px;border-radius:999px;
  background:rgba(2,10,4,0.8);box-shadow:inset 0 0 10px rgba(57,255,20,0.1)}
.dot{width:8px;height:8px;border-radius:50%;background:var(--accent);
  box-shadow:0 0 0 5px rgba(57,255,20,0.15),0 0 12px rgba(57,255,20,0.5)}
.nav{display:flex;gap:12px;align-items:center}
.nav a{color:var(--dim);text-decoration:none;font-size:11px;letter-spacing:1px;text-transform:uppercase;
  padding:4px 10px;border:1px solid rgba(57,255,20,0.15);border-radius:4px;transition:all 0.2s}
.nav a:hover{color:var(--accent);border-color:rgba(57,255,20,0.4);box-shadow:0 0 8px rgba(57,255,20,0.2)}
/* ── Cards grid ── */
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:10px;margin-bottom:10px}
.cards6{grid-template-columns:repeat(6,1fr)}
.card{border-radius:10px;padding:12px 14px;position:relative;overflow:hidden;
  background:radial-gradient(120% 160% at 0% 0%,rgba(57,255,20,0.05),transparent 50%),linear-gradient(155deg,rgba(4,18,8,0.92),rgba(3,12,6,0.85));
  border:1px solid var(--line);box-shadow:inset 0 0 0 1px rgba(57,255,20,0.03),0 0 14px var(--glow)}
.card::before{content:'';position:absolute;top:0;left:0;right:0;height:1px;
  background:linear-gradient(90deg,transparent,rgba(57,255,20,0.3),transparent)}
.card .lbl{font-size:10px;text-transform:uppercase;letter-spacing:2px;color:var(--muted);margin-bottom:4px}
.card .val{font-size:26px;font-weight:700;color:var(--accent);line-height:1.1;
  text-shadow:0 0 6px rgba(57,255,20,0.6),0 0 16px rgba(57,255,20,0.35),0 0 30px rgba(57,255,20,0.2)}
.card .val.sm{font-size:16px}
/* ── Section titles ── */
.stitle{font-size:10px;text-transform:uppercase;letter-spacing:3px;color:var(--dim);
  margin:16px 0 8px;padding-left:10px;border-left:2px solid var(--accent);
  text-shadow:0 0 6px rgba(57,255,20,0.2)}
/* ── Tables ── */
table{width:100%;border-collapse:collapse;margin-bottom:8px}
th{font-size:10px;text-transform:uppercase;letter-spacing:1.5px;color:var(--dimmer);text-align:left;
  padding:6px 10px;border-bottom:1px solid var(--line);font-weight:400}
td{padding:5px 10px;border-bottom:1px solid rgba(57,255,20,0.06);color:var(--muted);font-size:12px;white-space:nowrap}
td.hi{color:var(--accent);text-shadow:0 0 4px rgba(57,255,20,0.3)}
td.addr{max-width:170px;overflow:hidden;text-overflow:ellipsis;cursor:pointer}
td.addr:hover{color:var(--accent);text-shadow:0 0 6px rgba(57,255,20,0.4)}
tr:hover td{background:rgba(57,255,20,0.03)}
.confirmed{color:var(--accent)}
.pending{color:var(--dim)}
.orphaned{color:#6b2020}
.luck-good{color:var(--accent)}
.luck-mid{color:#aaff33}
.luck-bad{color:#ff3333}
/* ── Footer ── */
.foot{margin-top:16px;padding:10px 0;border-top:1px solid var(--line);font-size:10px;
  color:var(--dimmer);display:flex;gap:24px;text-transform:uppercase;letter-spacing:1px}
.foot .on{color:var(--accent);text-shadow:0 0 4px rgba(57,255,20,0.4)}
.foot .off{color:#ff3333;text-shadow:0 0 4px rgba(255,51,51,0.4)}
@media(max-width:900px){.cards{grid-template-columns:1fr 1fr}.cards6{grid-template-columns:repeat(3,1fr)}}
@media(max-width:500px){.cards{grid-template-columns:1fr}.cards6{grid-template-columns:1fr 1fr}}
</style>
</head>
<body>
<main>
<div class="top">
  <div>
    <h1>TAZ Pool</h1>
    <div class="sub">Zcash Testnet &middot; Equihash(200,9) &middot; PPLNS</div>
  </div>
  <div style="display:flex;align-items:center;gap:14px">
    <div class="status"><span class="dot"></span><span id="f-status">Online</span></div>
    <div class="nav">
      <a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a>
      <a href="/zallet">Wallet</a>
      <a href="/previews">Themes</a><a href="/">V1</a>
    </div>
  </div>
</div>
<div class="cards">
  <div class="card"><div class="lbl">Pool Hashrate</div><div class="val" id="s-hash">--</div></div>
  <div class="card"><div class="lbl">Network Hashrate</div><div class="val" id="s-net">--</div></div>
  <div class="card"><div class="lbl">Blocks Found</div><div class="val" id="s-blocks">0</div></div>
  <div class="card"><div class="lbl">Miners Online</div><div class="val" id="s-miners">0</div></div>
</div>
<div class="cards cards6">
  <div class="card"><div class="lbl">Total Shares</div><div class="val sm" id="s-shares">0</div></div>
  <div class="card"><div class="lbl">Luck 24h</div><div class="val sm" id="s-luck">--</div></div>
  <div class="card"><div class="lbl">Net Share</div><div class="val sm" id="s-pct">--</div></div>
  <div class="card"><div class="lbl">Immature</div><div class="val sm" id="s-imm">0</div></div>
  <div class="card"><div class="lbl">Pool Fee</div><div class="val sm" id="s-fee">--</div></div>
  <div class="card"><div class="lbl">Stratum Port</div><div class="val sm" id="s-port">--</div></div>
</div>
<div class="stitle">Miners</div>
<table id="t-miners"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>Workers</th><th>Shares</th><th>Pending</th></tr></thead>
<tbody><tr><td colspan="6" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="stitle">Recent Blocks</div>
<table id="t-blocks"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead>
<tbody><tr><td colspan="6" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="stitle">Payouts</div>
<table id="t-payouts"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead>
<tbody><tr><td colspan="4" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="foot">
  <span>Node: <span id="f-node" class="on">--</span></span>
  <span>Wallet: <span id="f-wallet" class="on">--</span></span>
  <span>Uptime: <span id="f-up">--</span></span>
</div>
</main>
<script>
const started=Date.now();
function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}
function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h '+m%60+'m':m>0?m+'m '+s%60+'s':s+'s'}
function $(id){return document.getElementById(id)}
async function tick(){try{const d=await(await fetch('/api/pool/stats')).json();
$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);
$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;
$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;
$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;
if(d.luck_percent!=null){const l=$('s-luck');l.textContent=d.luck_percent.toFixed(0)+'%';l.className='val sm '+(d.luck_percent<=100?'luck-good':d.luck_percent<=150?'luck-mid':'luck-bad')}
if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';
$('f-node').textContent=d.node_ok?'OK':'DOWN';$('f-node').className=d.node_ok?'on':'off';
$('f-wallet').textContent=d.wallet_ok?'OK':'DOWN';$('f-wallet').className=d.wallet_ok?'on':'off';
$('f-up').textContent=fd(Date.now()-started);
$('f-status').textContent=d.node_ok?'Online':'Offline';
}catch(e){$('f-status').textContent='Error'}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const tb=document.querySelector('#t-miners tbody');
if(!m.length){tb.innerHTML='<tr><td colspan="6" style="color:var(--dimmer)">no miners</td></tr>';return}
tb.innerHTML=m.map(r=>'<tr><td class="addr hi" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td>'+r.pending_zec.toFixed(4)+' TAZ</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const tb=document.querySelector('#t-blocks tbody');
if(!b.length){tb.innerHTML='<tr><td colspan="6" style="color:var(--dimmer)">no blocks</td></tr>';return}
tb.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}
return'<tr><td class="hi">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,12)+'</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const tb=document.querySelector('#t-payouts tbody');
if(!p.length){tb.innerHTML='<tr><td colspan="4" style="color:var(--dimmer)">no payouts</td></tr>';return}
tb.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="hi">'+r.amount_zec.toFixed(8)+' TAZ</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,12):'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();
setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script>
</body>
</html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 2 — "Neon Pulse" — Cyan/magenta cyberpunk, glowing panels, starfield
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW2: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TAZ Mining Pool — Neon Pulse</title>
<style>
:root {
  --bg0: #040510;
  --bg1: #0b1433;
  --panel: rgba(6, 10, 28, 0.78);
  --line: rgba(0, 229, 255, 0.20);
  --ink: #e8f6ff;
  --muted: #7a9ab8;
  --accentA: #00e5ff;
  --accentB: #ff2bd6;
  --accentC: #39ff14;
  --glowA: rgba(0, 229, 255, 0.22);
  --glowB: rgba(255, 43, 214, 0.14);
}
*{margin:0;padding:0;box-sizing:border-box}
html,body{background:var(--bg0);color:var(--ink);min-height:100vh}
body{font-family:'Segoe UI','Helvetica Neue',Arial,sans-serif;font-size:13px;position:relative;overflow-x:hidden;isolation:isolate}
/* ── CSS starfield ── */
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-2;
  background-image:
    radial-gradient(1px 1px at 10% 20%,rgba(255,255,255,0.4),transparent),
    radial-gradient(1px 1px at 30% 65%,rgba(255,255,255,0.3),transparent),
    radial-gradient(1px 1px at 55% 15%,rgba(255,255,255,0.35),transparent),
    radial-gradient(1px 1px at 70% 80%,rgba(255,255,255,0.25),transparent),
    radial-gradient(1px 1px at 85% 40%,rgba(255,255,255,0.3),transparent),
    radial-gradient(1px 1px at 15% 90%,rgba(255,255,255,0.2),transparent),
    radial-gradient(1px 1px at 45% 45%,rgba(255,255,255,0.3),transparent),
    radial-gradient(1px 1px at 92% 10%,rgba(255,255,255,0.25),transparent),
    radial-gradient(1.5px 1.5px at 25% 50%,rgba(200,230,255,0.5),transparent),
    radial-gradient(1.5px 1.5px at 60% 35%,rgba(200,230,255,0.4),transparent),
    radial-gradient(1.5px 1.5px at 80% 70%,rgba(200,230,255,0.35),transparent),
    radial-gradient(1px 1px at 5% 55%,rgba(255,255,255,0.2),transparent),
    radial-gradient(1px 1px at 40% 85%,rgba(255,255,255,0.3),transparent),
    radial-gradient(1px 1px at 75% 25%,rgba(255,255,255,0.2),transparent),
    radial-gradient(1px 1px at 95% 90%,rgba(255,255,255,0.15),transparent),
    radial-gradient(1px 1px at 50% 5%,rgba(255,255,255,0.25),transparent)}
/* Gradient overlay */
body::after{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;
  background:radial-gradient(1100px 550px at 5% -8%,rgba(0,229,255,0.12),transparent 55%),
             radial-gradient(800px 450px at 100% -4%,rgba(255,43,214,0.10),transparent 50%),
             radial-gradient(700px 400px at 50% 110%,rgba(57,255,20,0.06),transparent 55%)}
main{max-width:1400px;margin:0 auto;padding:20px;position:relative;z-index:2}
/* ── Shared panel style ── */
.glow-panel{position:relative;overflow:hidden;border-radius:14px;
  background:radial-gradient(120% 160% at 0% 0%,rgba(0,229,255,0.06),transparent 55%),
             radial-gradient(140% 150% at 100% 0%,rgba(255,43,214,0.05),transparent 55%),
             linear-gradient(155deg,rgba(7,12,30,0.93),rgba(7,10,27,0.87));
  border:1px solid var(--line);
  box-shadow:inset 0 0 0 1px rgba(0,229,255,0.04),0 0 22px var(--glowA),0 0 32px var(--glowB)}
.glow-panel::before{content:'';position:absolute;inset:0;border-radius:inherit;padding:1px;
  background:linear-gradient(110deg,rgba(0,229,255,0.6),rgba(57,255,20,0.5),rgba(255,43,214,0.6),rgba(255,230,0,0.5));
  background-size:250% 250%;opacity:0.25;pointer-events:none;
  -webkit-mask:linear-gradient(#000 0 0) content-box,linear-gradient(#000 0 0);-webkit-mask-composite:xor;mask-composite:exclude}
.glow-panel::after{content:'';position:absolute;inset:0;border-radius:inherit;pointer-events:none;
  background:linear-gradient(180deg,rgba(255,255,255,0.05),transparent 30%);opacity:0.4}
/* ── Header bar ── */
.top{display:flex;justify-content:space-between;align-items:center;padding:14px 20px;margin-bottom:14px}
.top h1{font-size:22px;font-weight:700;color:#f5fbff;letter-spacing:1px;
  text-shadow:0 0 12px rgba(0,229,255,0.4),0 0 24px rgba(57,255,20,0.2);position:relative;z-index:2}
.top .sub{color:var(--muted);font-size:12px;margin-top:2px;position:relative;z-index:2;
  text-shadow:0 0 6px rgba(0,229,255,0.1)}
.status{display:inline-flex;align-items:center;gap:8px;font-size:12px;
  border:1px solid rgba(0,229,255,0.25);padding:7px 14px;border-radius:999px;
  background:rgba(2,6,23,0.8);box-shadow:inset 0 0 12px rgba(0,229,255,0.12),0 0 14px rgba(0,229,255,0.1);
  position:relative;z-index:2}
.dot{width:9px;height:9px;border-radius:50%;background:var(--accentC);
  box-shadow:0 0 0 5px rgba(57,255,20,0.15),0 0 12px rgba(57,255,20,0.5)}
.nav{display:flex;gap:10px;align-items:center;position:relative;z-index:2}
.nav a{color:var(--muted);text-decoration:none;font-size:11px;letter-spacing:1px;text-transform:uppercase;
  padding:5px 12px;border:1px solid rgba(0,229,255,0.2);border-radius:6px;transition:all 0.2s;
  background:rgba(2,6,23,0.5)}
.nav a:hover{color:var(--accentA);border-color:rgba(0,229,255,0.5);
  box-shadow:0 0 10px rgba(0,229,255,0.2);text-shadow:0 0 6px rgba(0,229,255,0.4)}
/* ── Metric cards ── */
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:10px;margin-bottom:10px}
.cards6{grid-template-columns:repeat(6,1fr)}
.card{border-radius:12px;padding:10px 14px;position:relative;overflow:hidden;
  background:radial-gradient(120% 160% at 0% 0%,rgba(0,229,255,0.05),transparent 50%),
             linear-gradient(155deg,rgba(7,12,30,0.92),rgba(7,10,27,0.86));
  border:2px solid rgba(100,180,255,0.25);
  box-shadow:inset 0 0 0 1px rgba(0,229,255,0.03),0 0 16px rgba(100,180,255,0.12),0 0 28px rgba(100,180,255,0.08)}
.card::before{content:'';position:absolute;inset:0;border-radius:inherit;pointer-events:none;
  background:linear-gradient(180deg,rgba(255,255,255,0.04),transparent 30%)}
.card .lbl{font-size:10px;text-transform:uppercase;letter-spacing:1.5px;color:#7a9ab8;
  text-shadow:0 0 6px rgba(0,229,255,0.12);position:relative;z-index:2}
.card .val{font-family:'Courier New',monospace;font-size:24px;font-weight:400;color:#c8e8ff;line-height:1.2;
  text-shadow:0 0 5px rgba(100,160,255,0.7),0 0 14px rgba(60,120,255,0.5),0 0 28px rgba(40,80,255,0.35);
  position:relative;z-index:2}
.card .val.sm{font-size:15px}
/* Per-card accent colors via inline style */
.card.accent-cyan{border-color:rgba(0,229,255,0.4);box-shadow:0 0 16px rgba(0,229,255,0.15),0 0 28px rgba(0,229,255,0.08)}
.card.accent-cyan .val{color:#70f7ff;text-shadow:0 0 5px rgba(0,229,255,0.7),0 0 14px rgba(0,229,255,0.5)}
.card.accent-mag{border-color:rgba(255,43,214,0.35);box-shadow:0 0 16px rgba(255,43,214,0.12),0 0 28px rgba(255,43,214,0.06)}
.card.accent-mag .val{color:#ff8de8;text-shadow:0 0 5px rgba(255,43,214,0.6),0 0 14px rgba(255,43,214,0.4)}
.card.accent-grn{border-color:rgba(57,255,20,0.35);box-shadow:0 0 16px rgba(57,255,20,0.12),0 0 28px rgba(57,255,20,0.06)}
.card.accent-grn .val{color:#80ff60;text-shadow:0 0 5px rgba(57,255,20,0.6),0 0 14px rgba(57,255,20,0.4)}
/* ── Sections ── */
.stitle{font-size:10px;text-transform:uppercase;letter-spacing:3px;color:#5a7a98;
  margin:16px 0 8px;padding-left:10px;border-left:2px solid var(--accentA);
  text-shadow:0 0 6px rgba(0,229,255,0.15)}
/* ── Tables ── */
table{width:100%;border-collapse:collapse;margin-bottom:8px}
th{font-size:10px;text-transform:uppercase;letter-spacing:1.5px;color:#3a5a78;text-align:left;
  padding:6px 10px;border-bottom:1px solid var(--line);font-weight:400}
td{padding:5px 10px;border-bottom:1px solid rgba(0,229,255,0.05);color:var(--muted);font-size:12px;white-space:nowrap}
td.hi{color:#c8e8ff;text-shadow:0 0 4px rgba(100,160,255,0.3)}
td.cyan{color:var(--accentA);text-shadow:0 0 4px rgba(0,229,255,0.4)}
td.addr{max-width:170px;overflow:hidden;text-overflow:ellipsis;cursor:pointer;font-family:'Courier New',monospace;font-size:11px}
td.addr:hover{color:var(--accentA);text-shadow:0 0 6px rgba(0,229,255,0.4)}
tr:hover td{background:rgba(0,229,255,0.02)}
.confirmed{color:var(--accentC);text-shadow:0 0 4px rgba(57,255,20,0.3)}
.pending{color:#ffe600;text-shadow:0 0 4px rgba(255,230,0,0.3)}
.orphaned{color:#ff2a55}
.luck-good{color:var(--accentC);text-shadow:0 0 4px rgba(57,255,20,0.3)}
.luck-mid{color:#ffe600;text-shadow:0 0 4px rgba(255,230,0,0.3)}
.luck-bad{color:#ff2a55;text-shadow:0 0 4px rgba(255,42,85,0.3)}
/* ── Footer ── */
.foot{margin-top:16px;padding:10px 18px;border-radius:10px;font-size:10px;color:#3a5a78;
  display:flex;gap:24px;text-transform:uppercase;letter-spacing:1px;
  background:rgba(6,10,28,0.5);border:1px solid rgba(0,229,255,0.08)}
.foot .on{color:var(--accentC);text-shadow:0 0 4px rgba(57,255,20,0.4)}
.foot .off{color:#ff2a55;text-shadow:0 0 4px rgba(255,42,85,0.4)}
@media(max-width:1100px){.cards{grid-template-columns:repeat(2,1fr)}.cards6{grid-template-columns:repeat(3,1fr)}}
@media(max-width:600px){.cards{grid-template-columns:1fr}.cards6{grid-template-columns:1fr 1fr}}
</style>
</head>
<body>
<main>
<div class="top glow-panel">
  <div>
    <h1>TAZ Mining Pool</h1>
    <div class="sub">Zcash Testnet &middot; Equihash(200,9) &middot; PPLNS</div>
  </div>
  <div style="display:flex;align-items:center;gap:14px">
    <div class="status"><span class="dot"></span><span id="f-status">Connected</span></div>
    <div class="nav">
      <a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a>
      <a href="/zallet">Wallet</a>
      <a href="/previews">Themes</a><a href="/">V1</a>
    </div>
  </div>
</div>
<div class="cards">
  <div class="card accent-cyan"><div class="lbl">Pool Hashrate</div><div class="val" id="s-hash">--</div></div>
  <div class="card"><div class="lbl">Network Hashrate</div><div class="val" id="s-net">--</div></div>
  <div class="card accent-grn"><div class="lbl">Blocks Found</div><div class="val" id="s-blocks">0</div></div>
  <div class="card accent-mag"><div class="lbl">Miners Online</div><div class="val" id="s-miners">0</div></div>
</div>
<div class="cards cards6">
  <div class="card"><div class="lbl">Total Shares</div><div class="val sm" id="s-shares">0</div></div>
  <div class="card"><div class="lbl">Luck 24h</div><div class="val sm" id="s-luck">--</div></div>
  <div class="card"><div class="lbl">Net Share</div><div class="val sm" id="s-pct">--</div></div>
  <div class="card"><div class="lbl">Immature</div><div class="val sm" id="s-imm">0</div></div>
  <div class="card"><div class="lbl">Pool Fee</div><div class="val sm" id="s-fee">--</div></div>
  <div class="card"><div class="lbl">Stratum</div><div class="val sm" id="s-port">--</div></div>
</div>
<div class="stitle">Miners</div>
<table id="t-miners"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>Workers</th><th>Shares</th><th>Pending</th></tr></thead>
<tbody><tr><td colspan="6" style="color:#3a5a78">loading...</td></tr></tbody></table>
<div class="stitle">Recent Blocks</div>
<table id="t-blocks"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead>
<tbody><tr><td colspan="6" style="color:#3a5a78">loading...</td></tr></tbody></table>
<div class="stitle">Payouts</div>
<table id="t-payouts"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead>
<tbody><tr><td colspan="4" style="color:#3a5a78">loading...</td></tr></tbody></table>
<div class="foot">
  <span>Node: <span id="f-node" class="on">--</span></span>
  <span>Wallet: <span id="f-wallet" class="on">--</span></span>
  <span>Uptime: <span id="f-up">--</span></span>
</div>
</main>
<script>
const started=Date.now();
function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}
function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h '+m%60+'m':m>0?m+'m '+s%60+'s':s+'s'}
function $(id){return document.getElementById(id)}
async function tick(){try{const d=await(await fetch('/api/pool/stats')).json();
$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);
$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;
$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;
$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;
if(d.luck_percent!=null){const l=$('s-luck');l.textContent=d.luck_percent.toFixed(0)+'%';l.className='val sm '+(d.luck_percent<=100?'luck-good':d.luck_percent<=150?'luck-mid':'luck-bad')}
if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';
$('f-node').textContent=d.node_ok?'OK':'DOWN';$('f-node').className=d.node_ok?'on':'off';
$('f-wallet').textContent=d.wallet_ok?'OK':'DOWN';$('f-wallet').className=d.wallet_ok?'on':'off';
$('f-up').textContent=fd(Date.now()-started);
$('f-status').textContent=d.node_ok?'Connected':'Offline';
}catch(e){$('f-status').textContent='Error'}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const tb=document.querySelector('#t-miners tbody');
if(!m.length){tb.innerHTML='<tr><td colspan="6" style="color:#3a5a78">no miners</td></tr>';return}
tb.innerHTML=m.map(r=>'<tr><td class="addr hi" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td class="cyan">'+r.pending_zec.toFixed(4)+' TAZ</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const tb=document.querySelector('#t-blocks tbody');
if(!b.length){tb.innerHTML='<tr><td colspan="6" style="color:#3a5a78">no blocks</td></tr>';return}
tb.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}
return'<tr><td class="cyan">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,12)+'</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const tb=document.querySelector('#t-payouts tbody');
if(!p.length){tb.innerHTML='<tr><td colspan="4" style="color:#3a5a78">no payouts</td></tr>';return}
tb.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="cyan">'+r.amount_zec.toFixed(8)+' TAZ</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,12):'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();
setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script>
</body>
</html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 3 — "Cipher Gold" — Warm amber/gold neon on obsidian, vault aesthetic
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW3: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TAZ Mining Pool — Cipher Gold</title>
<style>
:root {
  --bg: #06050a;
  --panel: rgba(12, 10, 6, 0.85);
  --accent: #ffe600;
  --accent2: #ff8c00;
  --dim: #8a7530;
  --dimmer: #4a3e18;
  --ink: #fff5d8;
  --muted: #9a8a5a;
  --line: rgba(255, 230, 0, 0.12);
  --glow: rgba(255, 230, 0, 0.16);
  --glow2: rgba(255, 140, 0, 0.10);
}
*{margin:0;padding:0;box-sizing:border-box}
html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:'Courier New','Lucida Console',monospace;font-size:13px;position:relative;overflow-x:hidden}
/* Warm radial glow */
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;
  background:radial-gradient(1000px 500px at 15% 0%,rgba(255,230,0,0.06),transparent 55%),
             radial-gradient(800px 400px at 85% 100%,rgba(255,140,0,0.05),transparent 50%)}
main{max-width:1300px;margin:0 auto;padding:20px;position:relative;z-index:2}
/* ── Header ── */
.top{display:flex;justify-content:space-between;align-items:center;padding:16px 20px;border-radius:12px;margin-bottom:16px;
  background:radial-gradient(120% 160% at 0% 0%,rgba(255,230,0,0.04),transparent 50%),
             radial-gradient(120% 160% at 100% 100%,rgba(255,140,0,0.03),transparent 50%),
             linear-gradient(155deg,rgba(12,10,6,0.92),rgba(8,7,4,0.88));
  border:1px solid var(--line);box-shadow:0 0 24px var(--glow),0 0 36px var(--glow2)}
.top::before{content:'';position:absolute;inset:0;border-radius:inherit;padding:1px;
  background:linear-gradient(110deg,rgba(255,230,0,0.5),rgba(255,140,0,0.4),rgba(255,230,0,0.3));
  background-size:200% 200%;opacity:0.2;pointer-events:none;
  -webkit-mask:linear-gradient(#000 0 0) content-box,linear-gradient(#000 0 0);-webkit-mask-composite:xor;mask-composite:exclude}
.top h1{font-size:18px;font-weight:700;color:var(--accent);letter-spacing:4px;text-transform:uppercase;
  text-shadow:0 0 10px rgba(255,230,0,0.5),0 0 22px rgba(255,230,0,0.25);position:relative;z-index:2}
.top .sub{color:var(--dim);font-size:11px;margin-top:3px;letter-spacing:1px;position:relative;z-index:2}
.top .tag{display:inline-block;font-size:9px;background:rgba(255,230,0,0.12);color:var(--accent);
  padding:2px 8px;letter-spacing:2px;border:1px solid rgba(255,230,0,0.2);border-radius:3px;margin-left:8px}
.nav{display:flex;gap:10px;align-items:center;position:relative;z-index:2}
.nav a{color:var(--dimmer);text-decoration:none;font-size:10px;letter-spacing:2px;text-transform:uppercase;
  padding:5px 12px;border:1px solid rgba(255,230,0,0.12);border-radius:4px;transition:all 0.2s}
.nav a:hover{color:var(--accent);border-color:rgba(255,230,0,0.35);box-shadow:0 0 8px rgba(255,230,0,0.15)}
/* ── Cards ── */
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:1px;background:rgba(255,230,0,0.06);
  border:1px solid rgba(255,230,0,0.08);border-radius:10px;overflow:hidden;margin-bottom:2px}
.cards6{grid-template-columns:repeat(6,1fr)}
.card{padding:14px 16px;position:relative;
  background:radial-gradient(100% 150% at 0% 0%,rgba(255,230,0,0.03),transparent 50%),
             linear-gradient(155deg,rgba(10,9,5,0.95),rgba(8,7,4,0.9))}
.card .lbl{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--dimmer);margin-bottom:5px}
.card .val{font-size:24px;font-weight:700;color:var(--accent);line-height:1;
  text-shadow:0 0 6px rgba(255,230,0,0.5),0 0 16px rgba(255,230,0,0.25),0 0 30px rgba(255,140,0,0.15)}
.card .val.sm{font-size:14px;color:var(--dim);text-shadow:0 0 4px rgba(255,230,0,0.25)}
/* ── Sections ── */
.stitle{font-size:9px;text-transform:uppercase;letter-spacing:3px;color:var(--dimmer);
  margin:18px 0 8px;display:flex;align-items:center;gap:8px}
.stitle::before{content:'';flex:0 0 20px;height:1px;background:linear-gradient(90deg,var(--accent),transparent)}
.stitle::after{content:'';flex:1;height:1px;background:linear-gradient(90deg,rgba(255,230,0,0.15),transparent)}
/* ── Tables ── */
table{width:100%;border-collapse:collapse;margin-bottom:8px}
th{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--dimmer);text-align:left;
  padding:6px 10px;border-bottom:1px solid rgba(255,230,0,0.08);font-weight:400}
td{padding:5px 10px;border-bottom:1px solid rgba(255,230,0,0.04);color:var(--muted);font-size:12px;white-space:nowrap}
td.hi{color:var(--accent);text-shadow:0 0 4px rgba(255,230,0,0.25)}
td.addr{max-width:170px;overflow:hidden;text-overflow:ellipsis;cursor:pointer}
td.addr:hover{color:var(--accent);text-shadow:0 0 6px rgba(255,230,0,0.3)}
tr:hover td{background:rgba(255,230,0,0.02)}
.confirmed{color:var(--accent)}
.pending{color:var(--dim)}
.orphaned{color:#6b2020}
.luck-good{color:var(--accent)}
.luck-mid{color:var(--accent2)}
.luck-bad{color:#ff2a55}
/* ── Footer ── */
.foot{margin-top:18px;padding:10px 0;border-top:1px solid var(--line);font-size:9px;
  color:var(--dimmer);display:flex;gap:24px;text-transform:uppercase;letter-spacing:2px}
.foot .on{color:var(--accent);text-shadow:0 0 4px rgba(255,230,0,0.3)}
.foot .off{color:#ff2a55}
@media(max-width:900px){.cards{grid-template-columns:repeat(2,1fr)}.cards6{grid-template-columns:repeat(3,1fr)}}
@media(max-width:500px){.cards{grid-template-columns:1fr}.cards6{grid-template-columns:1fr 1fr}}
</style>
</head>
<body>
<main>
<div class="top" style="position:relative;overflow:hidden">
  <div>
    <h1>TAZ Pool<span class="tag">TESTNET</span></h1>
    <div class="sub">Zcash &middot; Equihash(200,9) &middot; PPLNS</div>
  </div>
  <div class="nav">
    <a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a>
    <a href="/zallet">Wallet</a>
    <a href="/previews">Themes</a><a href="/">V1</a>
  </div>
</div>
<div class="cards">
  <div class="card"><div class="lbl">Pool Hashrate</div><div class="val" id="s-hash">--</div></div>
  <div class="card"><div class="lbl">Network</div><div class="val" id="s-net">--</div></div>
  <div class="card"><div class="lbl">Blocks</div><div class="val" id="s-blocks">0</div></div>
  <div class="card"><div class="lbl">Miners</div><div class="val" id="s-miners">0</div></div>
</div>
<div class="cards cards6">
  <div class="card"><div class="lbl">Shares</div><div class="val sm" id="s-shares">0</div></div>
  <div class="card"><div class="lbl">Luck 24h</div><div class="val sm" id="s-luck">--</div></div>
  <div class="card"><div class="lbl">Net Share</div><div class="val sm" id="s-pct">--</div></div>
  <div class="card"><div class="lbl">Immature</div><div class="val sm" id="s-imm">0</div></div>
  <div class="card"><div class="lbl">Fee</div><div class="val sm" id="s-fee">--</div></div>
  <div class="card"><div class="lbl">Stratum</div><div class="val sm" id="s-port">--</div></div>
</div>
<div class="stitle">Miners</div>
<table id="t-miners"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>W</th><th>Shares</th><th>Pending</th></tr></thead>
<tbody><tr><td colspan="6" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="stitle">Blocks</div>
<table id="t-blocks"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead>
<tbody><tr><td colspan="6" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="stitle">Payouts</div>
<table id="t-payouts"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead>
<tbody><tr><td colspan="4" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="foot">
  <span>Node <span id="f-node" class="on">--</span></span>
  <span>Wallet <span id="f-wallet" class="on">--</span></span>
  <span>Up <span id="f-up">--</span></span>
</div>
</main>
<script>
const started=Date.now();
function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}
function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h '+m%60+'m':m>0?m+'m '+s%60+'s':s+'s'}
function $(id){return document.getElementById(id)}
async function tick(){try{const d=await(await fetch('/api/pool/stats')).json();
$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);
$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;
$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;
$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;
if(d.luck_percent!=null){const l=$('s-luck');l.textContent=d.luck_percent.toFixed(0)+'%';l.className='val sm '+(d.luck_percent<=100?'luck-good':d.luck_percent<=150?'luck-mid':'luck-bad')}
if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';
$('f-node').textContent=d.node_ok?'OK':'DOWN';$('f-node').className=d.node_ok?'on':'off';
$('f-wallet').textContent=d.wallet_ok?'OK':'DOWN';$('f-wallet').className=d.wallet_ok?'on':'off';
$('f-up').textContent=fd(Date.now()-started)}catch(e){}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const tb=document.querySelector('#t-miners tbody');
if(!m.length){tb.innerHTML='<tr><td colspan="6" style="color:var(--dimmer)">none</td></tr>';return}
tb.innerHTML=m.map(r=>'<tr><td class="addr hi" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td class="hi">'+r.pending_zec.toFixed(4)+' TAZ</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const tb=document.querySelector('#t-blocks tbody');
if(!b.length){tb.innerHTML='<tr><td colspan="6" style="color:var(--dimmer)">none</td></tr>';return}
tb.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}
return'<tr><td class="hi">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,12)+'</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const tb=document.querySelector('#t-payouts tbody');
if(!p.length){tb.innerHTML='<tr><td colspan="4" style="color:var(--dimmer)">none</td></tr>';return}
tb.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="hi">'+r.amount_zec.toFixed(8)+' TAZ</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,12):'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();
setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script>
</body>
</html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 4 — "Ghost Protocol" — Ultra-minimal, ghostly cyan whispers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW4: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TAZ Mining Pool — Ghost Protocol</title>
<style>
:root {
  --bg: #030308;
  --ink: #8899aa;
  --accent: #00b8d4;
  --accent2: #00e5ff;
  --dim: #2a3a4a;
  --dimmer: #1a2530;
  --line: rgba(0, 184, 212, 0.08);
  --glow: rgba(0, 184, 212, 0.06);
}
*{margin:0;padding:0;box-sizing:border-box}
html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI','Inter',Roboto,sans-serif;font-size:13px;
  position:relative;overflow-x:hidden}
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;
  background:radial-gradient(900px 450px at 50% 0%,rgba(0,184,212,0.03),transparent 60%)}
main{max-width:1100px;margin:0 auto;padding:40px 28px;position:relative;z-index:2}
/* ── Header ── */
.top{display:flex;justify-content:space-between;align-items:baseline;margin-bottom:48px}
.top h1{font-size:12px;font-weight:400;letter-spacing:6px;text-transform:uppercase;color:var(--dim)}
.top h1 span{color:var(--accent);text-shadow:0 0 8px rgba(0,184,212,0.3)}
.nav{display:flex;gap:16px;align-items:center}
.nav a{color:var(--dimmer);text-decoration:none;font-size:10px;letter-spacing:2px;text-transform:uppercase;
  transition:color 0.3s}
.nav a:hover{color:var(--accent);text-shadow:0 0 8px rgba(0,184,212,0.3)}
/* ── Hero metrics ── */
.hero{display:grid;grid-template-columns:repeat(4,1fr);gap:40px;margin-bottom:48px;
  padding-bottom:32px;border-bottom:1px solid var(--line)}
.hero .lbl{font-size:9px;text-transform:uppercase;letter-spacing:3px;color:var(--dim);margin-bottom:8px}
.hero .val{font-family:'SF Mono','Fira Code','Courier New',monospace;font-size:28px;font-weight:200;
  color:#bbc8d4;line-height:1;text-shadow:0 0 12px rgba(0,184,212,0.08)}
/* ── Secondary metrics ── */
.meta{display:grid;grid-template-columns:repeat(6,1fr);gap:28px;margin-bottom:40px;
  padding-bottom:28px;border-bottom:1px solid var(--line)}
.meta .lbl{font-size:8px;text-transform:uppercase;letter-spacing:3px;color:var(--dimmer);margin-bottom:4px}
.meta .val{font-family:'SF Mono','Fira Code','Courier New',monospace;font-size:13px;color:var(--ink)}
/* ── Sections ── */
.stitle{font-size:8px;text-transform:uppercase;letter-spacing:4px;color:var(--dimmer);
  margin:0 0 12px}
/* ── Tables ── */
table{width:100%;border-collapse:collapse;margin-bottom:32px}
th{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--dimmer);text-align:left;
  padding:8px 0;border-bottom:1px solid var(--line);font-weight:400}
td{padding:7px 0;padding-right:12px;border-bottom:1px solid rgba(0,184,212,0.03);
  color:#556677;font-size:12px;white-space:nowrap}
td.hi{color:#99aabb}
td.accent{color:var(--accent);text-shadow:0 0 6px rgba(0,184,212,0.2)}
td.addr{max-width:180px;overflow:hidden;text-overflow:ellipsis;cursor:pointer;
  font-family:'SF Mono','Fira Code','Courier New',monospace;font-size:11px}
td.addr:hover{color:var(--accent);text-shadow:0 0 6px rgba(0,184,212,0.3)}
tr:hover td{color:#8899aa}
.confirmed{color:#5a8a7a}
.pending{color:var(--dim)}
.orphaned{color:#3a2020}
.luck-good{color:#5a8a7a}
.luck-mid{color:var(--ink)}
.luck-bad{color:#8a4040}
/* ── Footer ── */
.foot{margin-top:40px;font-size:9px;color:var(--dimmer);display:flex;gap:24px;
  text-transform:uppercase;letter-spacing:2px}
.foot .on{color:var(--accent);text-shadow:0 0 4px rgba(0,184,212,0.2)}
.foot .off{color:#8a4040}
@media(max-width:900px){.hero{grid-template-columns:1fr 1fr;gap:24px}.meta{grid-template-columns:repeat(3,1fr);gap:16px}}
@media(max-width:500px){.hero{grid-template-columns:1fr}.meta{grid-template-columns:1fr 1fr}}
</style>
</head>
<body>
<main>
<div class="top">
  <h1><span>TAZ</span> Mining Pool</h1>
  <div class="nav">
    <a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a>
    <a href="/zallet">Wallet</a>
    <a href="/previews">Themes</a><a href="/">V1</a>
  </div>
</div>
<div class="hero">
  <div><div class="lbl">Pool Hashrate</div><div class="val" id="s-hash">--</div></div>
  <div><div class="lbl">Network</div><div class="val" id="s-net">--</div></div>
  <div><div class="lbl">Blocks Found</div><div class="val" id="s-blocks">0</div></div>
  <div><div class="lbl">Miners</div><div class="val" id="s-miners">0</div></div>
</div>
<div class="meta">
  <div><div class="lbl">Shares</div><div class="val" id="s-shares">0</div></div>
  <div><div class="lbl">Luck 24h</div><div class="val" id="s-luck">--</div></div>
  <div><div class="lbl">Net Share</div><div class="val" id="s-pct">--</div></div>
  <div><div class="lbl">Immature</div><div class="val" id="s-imm">0</div></div>
  <div><div class="lbl">Fee</div><div class="val" id="s-fee">--</div></div>
  <div><div class="lbl">Stratum</div><div class="val" id="s-port">--</div></div>
</div>
<div class="stitle">Miners</div>
<table id="t-miners"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>Workers</th><th>Shares</th><th>Pending</th></tr></thead>
<tbody><tr><td colspan="6">loading...</td></tr></tbody></table>
<div class="stitle">Blocks</div>
<table id="t-blocks"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead>
<tbody><tr><td colspan="6">loading...</td></tr></tbody></table>
<div class="stitle">Payouts</div>
<table id="t-payouts"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead>
<tbody><tr><td colspan="4">loading...</td></tr></tbody></table>
<div class="foot">
  <span>Node <span id="f-node" class="on">--</span></span>
  <span>Wallet <span id="f-wallet" class="on">--</span></span>
  <span>Uptime <span id="f-up">--</span></span>
</div>
</main>
<script>
const started=Date.now();
function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' T';if(h>=1e9)return(h/1e9).toFixed(2)+' G';if(h>=1e6)return(h/1e6).toFixed(2)+' M';if(h>=1e3)return(h/1e3).toFixed(2)+' K';return h.toFixed(0)}
function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h '+m%60+'m':m>0?m+'m '+s%60+'s':s+'s'}
function $(id){return document.getElementById(id)}
async function tick(){try{const d=await(await fetch('/api/pool/stats')).json();
$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);
$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;
$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;
$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;
if(d.luck_percent!=null)$('s-luck').textContent=d.luck_percent.toFixed(0)+'%';
if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';
$('f-node').textContent=d.node_ok?'OK':'DOWN';$('f-node').className=d.node_ok?'on':'off';
$('f-wallet').textContent=d.wallet_ok?'OK':'DOWN';$('f-wallet').className=d.wallet_ok?'on':'off';
$('f-up').textContent=fd(Date.now()-started)}catch(e){}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const tb=document.querySelector('#t-miners tbody');
if(!m.length){tb.innerHTML='<tr><td colspan="6">none</td></tr>';return}
tb.innerHTML=m.map(r=>'<tr><td class="addr hi" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td class="accent">'+r.pending_zec.toFixed(4)+'</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const tb=document.querySelector('#t-blocks tbody');
if(!b.length){tb.innerHTML='<tr><td colspan="6">none</td></tr>';return}
tb.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}
return'<tr><td class="accent">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,12)+'</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const tb=document.querySelector('#t-payouts tbody');
if(!p.length){tb.innerHTML='<tr><td colspan="4">none</td></tr>';return}
tb.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="accent">'+r.amount_zec.toFixed(8)+'</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,12):'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();
setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script>
</body>
</html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 5 — "Plasma" — Multi-color neon, HUD grid, animated gradient borders
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW5: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TAZ Mining Pool — Plasma</title>
<style>
:root {
  --bg: #020208;
  --panel: rgba(4, 6, 18, 0.9);
  --cyan: #00e5ff;
  --mag: #ff2bd6;
  --grn: #39ff14;
  --ylw: #ffe600;
  --ink: #d0d8e8;
  --muted: #5a6a80;
  --dimmer: #2a3548;
  --line: rgba(0, 229, 255, 0.10);
}
*{margin:0;padding:0;box-sizing:border-box}
html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:'JetBrains Mono','Fira Code','Courier New',monospace;font-size:13px;
  position:relative;overflow-x:hidden}
/* Grid background */
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-2;
  background-image:
    linear-gradient(rgba(0,229,255,0.03) 1px,transparent 1px),
    linear-gradient(90deg,rgba(0,229,255,0.03) 1px,transparent 1px);
  background-size:60px 60px}
/* Gradient overlay */
body::after{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;
  background:radial-gradient(800px 400px at 20% -10%,rgba(0,229,255,0.08),transparent 50%),
             radial-gradient(600px 350px at 80% 110%,rgba(255,43,214,0.06),transparent 50%),
             radial-gradient(500px 300px at 50% 50%,rgba(57,255,20,0.03),transparent 50%)}
main{max-width:1400px;margin:0 auto;padding:20px;position:relative;z-index:2}
/* ── Animated gradient border mixin ── */
@keyframes borderGlow{0%{background-position:0% 50%}50%{background-position:100% 50%}100%{background-position:0% 50%}}
/* ── Header ── */
.top{position:relative;display:flex;justify-content:space-between;align-items:center;
  padding:16px 22px;border-radius:14px;margin-bottom:14px;overflow:hidden;
  background:linear-gradient(155deg,rgba(4,6,18,0.95),rgba(3,5,15,0.9));
  border:1px solid var(--line)}
.top::before{content:'';position:absolute;inset:0;border-radius:inherit;padding:1.5px;
  background:linear-gradient(110deg,var(--cyan),var(--grn),var(--mag),var(--ylw),var(--cyan));
  background-size:300% 300%;animation:borderGlow 8s ease infinite;opacity:0.35;pointer-events:none;
  -webkit-mask:linear-gradient(#000 0 0) content-box,linear-gradient(#000 0 0);-webkit-mask-composite:xor;mask-composite:exclude}
.top::after{content:'';position:absolute;inset:0;border-radius:inherit;pointer-events:none;
  box-shadow:inset 0 0 30px rgba(0,229,255,0.06),0 0 30px rgba(0,229,255,0.08),0 0 50px rgba(255,43,214,0.04)}
.top h1{font-size:20px;font-weight:700;letter-spacing:2px;text-transform:uppercase;position:relative;z-index:2;
  background:linear-gradient(90deg,var(--cyan),var(--grn));-webkit-background-clip:text;-webkit-text-fill-color:transparent;
  filter:drop-shadow(0 0 8px rgba(0,229,255,0.4))}
.top .sub{color:var(--muted);font-size:11px;margin-top:3px;position:relative;z-index:2}
.status{display:inline-flex;align-items:center;gap:8px;font-size:11px;
  border:1px solid rgba(0,229,255,0.15);padding:6px 14px;border-radius:999px;
  background:rgba(2,4,12,0.8);position:relative;z-index:2}
.dot{width:8px;height:8px;border-radius:50%;background:var(--grn);
  box-shadow:0 0 0 4px rgba(57,255,20,0.12),0 0 10px rgba(57,255,20,0.5);
  animation:pulse 2s ease-in-out infinite}
@keyframes pulse{0%,100%{box-shadow:0 0 0 4px rgba(57,255,20,0.12),0 0 10px rgba(57,255,20,0.5)}
  50%{box-shadow:0 0 0 6px rgba(57,255,20,0.2),0 0 16px rgba(57,255,20,0.7)}}
.nav{display:flex;gap:8px;align-items:center;position:relative;z-index:2}
.nav a{color:var(--muted);text-decoration:none;font-size:10px;letter-spacing:1.5px;text-transform:uppercase;
  padding:5px 14px;border-radius:6px;transition:all 0.3s;position:relative;overflow:hidden;
  border:1px solid rgba(0,229,255,0.1);background:rgba(2,4,12,0.5)}
.nav a:hover{color:var(--cyan);border-color:rgba(0,229,255,0.3);
  box-shadow:0 0 12px rgba(0,229,255,0.15),inset 0 0 12px rgba(0,229,255,0.05);
  text-shadow:0 0 6px rgba(0,229,255,0.4)}
/* ── Metric cards ── */
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:10px;margin-bottom:10px}
.cards6{grid-template-columns:repeat(6,1fr)}
.card{position:relative;overflow:hidden;border-radius:12px;padding:12px 16px;
  background:linear-gradient(155deg,rgba(4,6,18,0.95),rgba(3,5,15,0.88));
  border:1px solid var(--line)}
.card::before{content:'';position:absolute;inset:0;border-radius:inherit;padding:1px;
  background:linear-gradient(110deg,var(--card-c1,rgba(0,229,255,0.4)),var(--card-c2,rgba(100,180,255,0.3)));
  opacity:0.3;pointer-events:none;
  -webkit-mask:linear-gradient(#000 0 0) content-box,linear-gradient(#000 0 0);-webkit-mask-composite:xor;mask-composite:exclude}
.card::after{content:'';position:absolute;inset:0;border-radius:inherit;pointer-events:none;
  box-shadow:inset 0 0 20px var(--card-glow,rgba(0,229,255,0.04)),0 0 18px var(--card-glow,rgba(0,229,255,0.06))}
.card .lbl{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--muted);
  margin-bottom:4px;position:relative;z-index:2}
.card .val{font-size:22px;font-weight:700;line-height:1.2;position:relative;z-index:2;
  color:var(--card-text,#c8e8ff);
  text-shadow:0 0 5px var(--card-glow,rgba(100,160,255,0.5)),0 0 14px var(--card-glow,rgba(60,120,255,0.3))}
.card .val.sm{font-size:14px;font-weight:400}
/* Card color variants */
.c-cyan{--card-c1:rgba(0,229,255,0.5);--card-c2:rgba(0,180,220,0.3);--card-glow:rgba(0,229,255,0.08);--card-text:#70f7ff}
.c-mag{--card-c1:rgba(255,43,214,0.5);--card-c2:rgba(200,30,170,0.3);--card-glow:rgba(255,43,214,0.06);--card-text:#ff8de8}
.c-grn{--card-c1:rgba(57,255,20,0.5);--card-c2:rgba(40,200,15,0.3);--card-glow:rgba(57,255,20,0.06);--card-text:#80ff60}
.c-ylw{--card-c1:rgba(255,230,0,0.5);--card-c2:rgba(200,180,0,0.3);--card-glow:rgba(255,230,0,0.06);--card-text:#fff080}
/* ── Section titles ── */
.stitle{font-size:9px;text-transform:uppercase;letter-spacing:3px;color:var(--dimmer);
  margin:18px 0 10px;padding-left:12px;position:relative}
.stitle::before{content:'';position:absolute;left:0;top:50%;width:4px;height:4px;border-radius:50%;
  background:var(--cyan);box-shadow:0 0 6px var(--cyan);transform:translateY(-50%)}
/* ── Tables ── */
table{width:100%;border-collapse:collapse;margin-bottom:8px}
th{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--dimmer);text-align:left;
  padding:6px 10px;border-bottom:1px solid var(--line);font-weight:400}
td{padding:5px 10px;border-bottom:1px solid rgba(0,229,255,0.03);color:var(--muted);font-size:11px;white-space:nowrap}
td.hi{color:#c8e8ff}
td.glow-c{color:var(--cyan);text-shadow:0 0 4px rgba(0,229,255,0.3)}
td.glow-g{color:var(--grn);text-shadow:0 0 4px rgba(57,255,20,0.3)}
td.glow-m{color:var(--mag);text-shadow:0 0 4px rgba(255,43,214,0.3)}
td.addr{max-width:170px;overflow:hidden;text-overflow:ellipsis;cursor:pointer}
td.addr:hover{color:var(--cyan);text-shadow:0 0 6px rgba(0,229,255,0.4)}
tr:hover td{background:rgba(0,229,255,0.02)}
.confirmed{color:var(--grn);text-shadow:0 0 4px rgba(57,255,20,0.2)}
.pending{color:var(--ylw);text-shadow:0 0 4px rgba(255,230,0,0.2)}
.orphaned{color:#ff2a55}
.luck-good{color:var(--grn);text-shadow:0 0 4px rgba(57,255,20,0.2)}
.luck-mid{color:var(--ylw);text-shadow:0 0 4px rgba(255,230,0,0.2)}
.luck-bad{color:#ff2a55;text-shadow:0 0 4px rgba(255,42,85,0.2)}
/* ── Footer ── */
.foot{position:relative;overflow:hidden;margin-top:18px;padding:12px 18px;border-radius:10px;font-size:9px;
  color:var(--dimmer);display:flex;gap:24px;text-transform:uppercase;letter-spacing:1.5px;
  background:rgba(4,6,18,0.6);border:1px solid var(--line)}
.foot::before{content:'';position:absolute;top:0;left:0;right:0;height:1px;
  background:linear-gradient(90deg,transparent,var(--cyan),var(--mag),transparent)}
.foot .on{color:var(--grn);text-shadow:0 0 4px rgba(57,255,20,0.3)}
.foot .off{color:#ff2a55;text-shadow:0 0 4px rgba(255,42,85,0.3)}
@media(max-width:1100px){.cards{grid-template-columns:repeat(2,1fr)}.cards6{grid-template-columns:repeat(3,1fr)}}
@media(max-width:600px){.cards{grid-template-columns:1fr}.cards6{grid-template-columns:1fr 1fr}}
</style>
</head>
<body>
<main>
<div class="top">
  <div>
    <h1>TAZ Mining Pool</h1>
    <div class="sub">Zcash Testnet &middot; Equihash(200,9) &middot; PPLNS</div>
  </div>
  <div style="display:flex;align-items:center;gap:14px">
    <div class="status"><span class="dot"></span><span id="f-status">Online</span></div>
    <div class="nav">
      <a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a>
      <a href="/zallet">Wallet</a>
      <a href="/previews">Themes</a><a href="/">V1</a>
    </div>
  </div>
</div>
<div class="cards">
  <div class="card c-cyan"><div class="lbl">Pool Hashrate</div><div class="val" id="s-hash">--</div></div>
  <div class="card"><div class="lbl">Network Hashrate</div><div class="val" id="s-net">--</div></div>
  <div class="card c-grn"><div class="lbl">Blocks Found</div><div class="val" id="s-blocks">0</div></div>
  <div class="card c-mag"><div class="lbl">Miners Online</div><div class="val" id="s-miners">0</div></div>
</div>
<div class="cards cards6">
  <div class="card"><div class="lbl">Total Shares</div><div class="val sm" id="s-shares">0</div></div>
  <div class="card c-ylw"><div class="lbl">Luck 24h</div><div class="val sm" id="s-luck">--</div></div>
  <div class="card"><div class="lbl">Net Share</div><div class="val sm" id="s-pct">--</div></div>
  <div class="card"><div class="lbl">Immature</div><div class="val sm" id="s-imm">0</div></div>
  <div class="card"><div class="lbl">Pool Fee</div><div class="val sm" id="s-fee">--</div></div>
  <div class="card"><div class="lbl">Stratum</div><div class="val sm" id="s-port">--</div></div>
</div>
<div class="stitle">Miners</div>
<table id="t-miners"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>Workers</th><th>Shares</th><th>Pending</th></tr></thead>
<tbody><tr><td colspan="6" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="stitle">Recent Blocks</div>
<table id="t-blocks"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead>
<tbody><tr><td colspan="6" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="stitle">Payouts</div>
<table id="t-payouts"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead>
<tbody><tr><td colspan="4" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="foot">
  <span>Node: <span id="f-node" class="on">--</span></span>
  <span>Wallet: <span id="f-wallet" class="on">--</span></span>
  <span>Uptime: <span id="f-up">--</span></span>
</div>
</main>
<script>
const started=Date.now();
function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}
function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h '+m%60+'m':m>0?m+'m '+s%60+'s':s+'s'}
function $(id){return document.getElementById(id)}
async function tick(){try{const d=await(await fetch('/api/pool/stats')).json();
$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);
$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;
$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;
$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;
if(d.luck_percent!=null){const l=$('s-luck');l.textContent=d.luck_percent.toFixed(0)+'%';l.className='val sm '+(d.luck_percent<=100?'luck-good':d.luck_percent<=150?'luck-mid':'luck-bad')}
if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';
$('f-node').textContent=d.node_ok?'OK':'DOWN';$('f-node').className=d.node_ok?'on':'off';
$('f-wallet').textContent=d.wallet_ok?'OK':'DOWN';$('f-wallet').className=d.wallet_ok?'on':'off';
$('f-up').textContent=fd(Date.now()-started);
$('f-status').textContent=d.node_ok?'Online':'Offline';
}catch(e){$('f-status').textContent='Error'}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const tb=document.querySelector('#t-miners tbody');
if(!m.length){tb.innerHTML='<tr><td colspan="6" style="color:var(--dimmer)">no miners</td></tr>';return}
tb.innerHTML=m.map(r=>'<tr><td class="addr hi" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td class="glow-c">'+r.pending_zec.toFixed(4)+' TAZ</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const tb=document.querySelector('#t-blocks tbody');
if(!b.length){tb.innerHTML='<tr><td colspan="6" style="color:var(--dimmer)">no blocks</td></tr>';return}
tb.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}
return'<tr><td class="glow-c">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,12)+'</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const tb=document.querySelector('#t-payouts tbody');
if(!p.length){tb.innerHTML='<tr><td colspan="4" style="color:var(--dimmer)">no payouts</td></tr>';return}
tb.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="glow-c">'+r.amount_zec.toFixed(8)+' TAZ</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,12):'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();
setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script>
</body>
</html>
"##;

// Previews 6-10 and Gallery are in previews_extra.rs
include!("previews_extra.rs");
include!("previews_extra2.rs");
include!("previews_extra3.rs");
include!("previews_extra4.rs");
