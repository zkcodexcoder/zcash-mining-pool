use axum::response::Html;

pub async fn preview1() -> Html<String> { Html(PREVIEW1.to_string()) }
pub async fn preview2() -> Html<String> { Html(PREVIEW2.to_string()) }
pub async fn preview3() -> Html<String> { Html(PREVIEW3.to_string()) }
pub async fn preview4() -> Html<String> { Html(PREVIEW4.to_string()) }
pub async fn preview5() -> Html<String> { Html(PREVIEW5.to_string()) }

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 1 — "Terminal" — Green phosphor on black, command-line aesthetic
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW1: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TAZ Mining Pool — Terminal</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
@import url('https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;700&display=swap');
body{font-family:'JetBrains Mono',monospace;background:#0a0a0a;color:#33ff33;min-height:100vh;font-size:13px;line-height:1.6}
::selection{background:#33ff33;color:#0a0a0a}
.scanlines{position:fixed;top:0;left:0;right:0;bottom:0;pointer-events:none;z-index:999;background:repeating-linear-gradient(0deg,rgba(0,0,0,0.15) 0px,rgba(0,0,0,0.15) 1px,transparent 1px,transparent 2px)}
.shell{max-width:1100px;margin:0 auto;padding:1rem 1.5rem}
.prompt{color:#33ff33;opacity:0.5;margin-right:0.5rem;user-select:none}
.header{border-bottom:1px solid #1a3a1a;padding:1rem 0;margin-bottom:1rem}
.header .title{color:#33ff33;font-size:1.1rem;font-weight:700;letter-spacing:0.1em}
.header .subtitle{color:#1a6b1a;font-size:0.7rem;margin-top:0.25rem}
.header .nav{margin-top:0.5rem}
.header .nav a{color:#1a6b1a;text-decoration:none;margin-right:1.5rem;font-size:0.75rem}
.header .nav a:hover{color:#33ff33;text-decoration:underline}
.section{margin-bottom:1.5rem}
.section-head{color:#1a6b1a;font-size:0.7rem;text-transform:uppercase;letter-spacing:0.15em;margin-bottom:0.5rem;border-bottom:1px dashed #1a3a1a;padding-bottom:0.25rem}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:0}
.metric{padding:0.5rem 0}
.metric .key{color:#1a6b1a;font-size:0.65rem;text-transform:uppercase;letter-spacing:0.1em}
.metric .val{color:#33ff33;font-size:1.4rem;font-weight:700}
.metric .val.dim{color:#1a6b1a}
.metric .unit{color:#1a6b1a;font-size:0.7rem;margin-left:0.25rem}
table{width:100%;border-collapse:collapse}
th{color:#1a6b1a;font-size:0.6rem;text-transform:uppercase;letter-spacing:0.15em;text-align:left;padding:0.3rem 0.5rem;border-bottom:1px solid #1a3a1a}
td{padding:0.3rem 0.5rem;border-bottom:1px solid #0d1a0d;color:#33ff33;font-size:0.75rem;white-space:nowrap}
td.addr{max-width:180px;overflow:hidden;text-overflow:ellipsis;cursor:pointer}
td.addr:hover{color:#88ff88;text-decoration:underline}
tr:hover td{background:#0d1a0d}
.status-confirmed{color:#33ff33}
.status-pending{color:#aaff33}
.status-orphaned{color:#ff3333}
.bar{display:inline-block;height:8px;background:#33ff33;min-width:2px}
.luck-good{color:#33ff33}
.luck-mid{color:#aaff33}
.luck-bad{color:#ff3333}
.blink{animation:blink 1s step-end infinite}
@keyframes blink{50%{opacity:0}}
.footer{border-top:1px solid #1a3a1a;padding:0.75rem 0;margin-top:1rem;color:#1a6b1a;font-size:0.65rem;display:flex;gap:2rem}
.footer .ok{color:#33ff33}
.footer .err{color:#ff3333}
@media(max-width:700px){.grid{grid-template-columns:1fr 1fr}}
</style>
</head>
<body>
<div class="scanlines"></div>
<div class="shell">
<div class="header">
    <div class="title"><span class="prompt">$</span>TAZ_MINING_POOL<span class="blink">_</span></div>
    <div class="subtitle">Zcash Testnet — Equihash(200,9) — PPLNS</div>
    <div class="nav">
        <a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">[mine]</a>
        <a href="/zallet">[wallet]</a>
        <a href="/">[dashboard]</a>
    </div>
</div>

<div class="section">
    <div class="section-head">// system status</div>
    <div class="grid">
        <div class="metric"><div class="key">pool hashrate</div><div class="val" id="s-hash">--</div></div>
        <div class="metric"><div class="key">network hashrate</div><div class="val" id="s-net">--</div></div>
        <div class="metric"><div class="key">blocks found</div><div class="val" id="s-blocks">0</div></div>
        <div class="metric"><div class="key">connected miners</div><div class="val" id="s-miners">0</div></div>
    </div>
</div>

<div class="section">
    <div class="section-head">// metrics</div>
    <div class="grid">
        <div class="metric"><div class="key">total shares</div><div class="val" id="s-shares">0</div></div>
        <div class="metric"><div class="key">luck 24h</div><div class="val" id="s-luck">--</div></div>
        <div class="metric"><div class="key">network share</div><div class="val" id="s-pct">--</div></div>
        <div class="metric"><div class="key">immature / payout</div><div class="val"><span id="s-imm">0</span><span class="unit"> / </span><span id="s-pay">0</span></div></div>
        <div class="metric"><div class="key">pool fee</div><div class="val" id="s-fee">--</div></div>
        <div class="metric"><div class="key">stratum</div><div class="val" id="s-port">--</div></div>
    </div>
</div>

<div class="section">
    <div class="section-head">// miners</div>
    <table id="t-miners"><thead><tr><th>address</th><th>1m</th><th>10m</th><th>workers</th><th>shares</th><th>pending</th></tr></thead>
    <tbody><tr><td colspan="6" style="color:#1a6b1a">loading...</td></tr></tbody></table>
</div>

<div class="section">
    <div class="section-head">// recent blocks</div>
    <table id="t-blocks"><thead><tr><th>height</th><th>hash</th><th>reward</th><th>luck</th><th>status</th><th>found</th></tr></thead>
    <tbody><tr><td colspan="6" style="color:#1a6b1a">loading...</td></tr></tbody></table>
</div>

<div class="section">
    <div class="section-head">// payouts</div>
    <table id="t-payouts"><thead><tr><th>miner</th><th>amount</th><th>txid</th><th>date</th></tr></thead>
    <tbody><tr><td colspan="4" style="color:#1a6b1a">loading...</td></tr></tbody></table>
</div>

<div class="footer">
    <span>node: <span id="f-node" class="ok">--</span></span>
    <span>wallet: <span id="f-wallet" class="ok">--</span></span>
    <span>uptime: <span id="f-up">--</span></span>
</div>
</div>

<script>
const started=Date.now();
function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}
function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h '+m%60+'m':m>0?m+'m '+s%60+'s':s+'s'}
function $(id){return document.getElementById(id)}
async function tick(){
try{
const d=await(await fetch('/api/pool/stats')).json();
$('s-hash').textContent=fh(d.hashrate_estimate);
$('s-net').textContent=fh(d.network_hashrate);
$('s-blocks').textContent=d.total_blocks;
$('s-miners').textContent=d.connected_miners;
$('s-shares').textContent=d.total_shares.toLocaleString();
$('s-imm').textContent=d.immature_blocks;$('s-pay').textContent=d.pending_payout_blocks;
$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;
if(d.luck_percent!=null){const l=$('s-luck');l.textContent=d.luck_percent.toFixed(0)+'%';l.className='val '+(d.luck_percent<=100?'luck-good':d.luck_percent<=150?'luck-mid':'luck-bad')}
if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';
$('f-node').textContent=d.node_ok?'OK':'DOWN';$('f-node').className=d.node_ok?'ok':'err';
$('f-wallet').textContent=d.wallet_ok?'OK':'DOWN';$('f-wallet').className=d.wallet_ok?'ok':'err';
$('f-up').textContent=fd(Date.now()-started);
}catch(e){console.error(e)}
}
async function miners(){
try{
const m=await(await fetch('/api/miners')).json();const tb=document.querySelector('#t-miners tbody');
if(!m.length){tb.innerHTML='<tr><td colspan="6" style="color:#1a6b1a">no miners</td></tr>';return}
tb.innerHTML=m.map(r=>'<tr><td class="addr" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td>'+r.pending_zec.toFixed(4)+' TAZ</td></tr>').join('')
}catch(e){console.error(e)}
}
async function blocks(){
try{
const b=await(await fetch('/api/blocks')).json();const tb=document.querySelector('#t-blocks tbody');
if(!b.length){tb.innerHTML='<tr><td colspan="6" style="color:#1a6b1a">no blocks</td></tr>';return}
tb.innerHTML=b.slice(0,50).map(r=>{
let lc='luck-good',ls='--';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}
return'<tr><td>'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,12)+'…</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="status-'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'
}).join('')
}catch(e){console.error(e)}
}
async function payouts(){
try{
const p=await(await fetch('/api/payouts')).json();const tb=document.querySelector('#t-payouts tbody');
if(!p.length){tb.innerHTML='<tr><td colspan="4" style="color:#1a6b1a">no payouts</td></tr>';return}
tb.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td style="color:#33ff33">'+r.amount_zec.toFixed(8)+' TAZ</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,12)+'…':'--')+'</td><td>'+r.created_at+'</td></tr>').join('')
}catch(e){console.error(e)}
}
tick();miners();blocks();payouts();
setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script>
</body>
</html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 2 — "Brutalist" — Raw monochrome, thick borders, no decoration
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW2: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TAZ Mining Pool — Brutalist</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:'Courier New',monospace;background:#000;color:#fff;min-height:100vh;font-size:13px}
::selection{background:#fff;color:#000}
.wrap{max-width:1200px;margin:0 auto;padding:0.5rem}
.bar-top{background:#fff;color:#000;padding:0.5rem 1rem;font-weight:700;font-size:1rem;display:flex;justify-content:space-between;align-items:center;text-transform:uppercase;letter-spacing:0.2em}
.bar-top a{color:#000;text-decoration:none;font-size:0.65rem;letter-spacing:0.1em;border:2px solid #000;padding:0.15rem 0.5rem}
.bar-top a:hover{background:#000;color:#fff}
.bar-top .links{display:flex;gap:0.5rem}
.row{display:grid;grid-template-columns:repeat(4,1fr);margin-top:0.5rem;gap:0.5rem}
.cell{border:2px solid #333;padding:0.75rem}
.cell .label{font-size:0.55rem;text-transform:uppercase;letter-spacing:0.2em;color:#666;margin-bottom:0.25rem}
.cell .value{font-size:2rem;font-weight:700;line-height:1}
.cell .value.sm{font-size:1.2rem}
.row3{grid-template-columns:repeat(3,1fr)}
.row6{grid-template-columns:repeat(6,1fr)}
.section-label{font-size:0.6rem;text-transform:uppercase;letter-spacing:0.25em;color:#666;margin-top:1rem;margin-bottom:0.25rem;border-top:2px solid #333;padding-top:0.5rem}
table{width:100%;border-collapse:collapse;margin-bottom:0.5rem}
th{font-size:0.55rem;text-transform:uppercase;letter-spacing:0.15em;color:#666;text-align:left;padding:0.4rem 0.5rem;border-bottom:2px solid #333}
td{padding:0.35rem 0.5rem;border-bottom:1px solid #1a1a1a;font-size:0.75rem;white-space:nowrap}
td.addr{max-width:160px;overflow:hidden;text-overflow:ellipsis;cursor:pointer}
td.addr:hover{text-decoration:underline}
.confirmed{color:#fff}
.pending{color:#888}
.orphaned{color:#555;text-decoration:line-through}
.luck-good{color:#fff}
.luck-mid{color:#888}
.luck-bad{color:#555}
.foot{margin-top:1rem;padding:0.5rem 0;border-top:2px solid #333;font-size:0.6rem;color:#555;display:flex;gap:2rem;text-transform:uppercase;letter-spacing:0.1em}
@media(max-width:800px){.row{grid-template-columns:1fr 1fr}.row6{grid-template-columns:repeat(3,1fr)}}
@media(max-width:500px){.row{grid-template-columns:1fr}.row6{grid-template-columns:1fr 1fr}}
</style>
</head>
<body>
<div class="wrap">
<div class="bar-top">
    <span>TAZ POOL</span>
    <div class="links">
        <a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">MINE</a>
        <a href="/zallet">WALLET</a>
        <a href="/">V1</a>
    </div>
</div>

<div class="row">
    <div class="cell"><div class="label">Pool Hashrate</div><div class="value" id="s-hash">--</div></div>
    <div class="cell"><div class="label">Network</div><div class="value" id="s-net">--</div></div>
    <div class="cell"><div class="label">Blocks</div><div class="value" id="s-blocks">0</div></div>
    <div class="cell"><div class="label">Miners</div><div class="value" id="s-miners">0</div></div>
</div>
<div class="row row6">
    <div class="cell"><div class="label">Shares</div><div class="value sm" id="s-shares">0</div></div>
    <div class="cell"><div class="label">Luck 24h</div><div class="value sm" id="s-luck">--</div></div>
    <div class="cell"><div class="label">Net Share</div><div class="value sm" id="s-pct">--</div></div>
    <div class="cell"><div class="label">Immature</div><div class="value sm" id="s-imm">0</div></div>
    <div class="cell"><div class="label">Fee</div><div class="value sm" id="s-fee">--</div></div>
    <div class="cell"><div class="label">Port</div><div class="value sm" id="s-port">--</div></div>
</div>

<div class="section-label">Miners</div>
<table id="t-miners"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>W</th><th>Shares</th><th>Pending</th></tr></thead>
<tbody><tr><td colspan="6" style="color:#333">...</td></tr></tbody></table>

<div class="section-label">Blocks</div>
<table id="t-blocks"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead>
<tbody><tr><td colspan="6" style="color:#333">...</td></tr></tbody></table>

<div class="section-label">Payouts</div>
<table id="t-payouts"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead>
<tbody><tr><td colspan="4" style="color:#333">...</td></tr></tbody></table>

<div class="foot">
    <span>node: <span id="f-node">--</span></span>
    <span>wallet: <span id="f-wallet">--</span></span>
    <span>uptime: <span id="f-up">--</span></span>
</div>
</div>

<script>
const started=Date.now();
function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' T';if(h>=1e9)return(h/1e9).toFixed(2)+' G';if(h>=1e6)return(h/1e6).toFixed(2)+' M';if(h>=1e3)return(h/1e3).toFixed(2)+' K';return h.toFixed(0)}
function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h'+m%60+'m':m>0?m+'m'+s%60+'s':s+'s'}
function $(id){return document.getElementById(id)}
async function tick(){
try{const d=await(await fetch('/api/pool/stats')).json();
$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);
$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;
$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;
$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;
if(d.luck_percent!=null){$('s-luck').textContent=d.luck_percent.toFixed(0)+'%';$('s-luck').style.color=d.luck_percent<=100?'#fff':'#555'}
if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';
$('f-node').textContent=d.node_ok?'OK':'DOWN';$('f-wallet').textContent=d.wallet_ok?'OK':'DOWN';
$('f-up').textContent=fd(Date.now()-started);
}catch(e){}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const tb=document.querySelector('#t-miners tbody');if(!m.length){tb.innerHTML='<tr><td colspan="6" style="color:#333">none</td></tr>';return}tb.innerHTML=m.map(r=>'<tr><td class="addr" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td>'+r.pending_zec.toFixed(4)+'</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const tb=document.querySelector('#t-blocks tbody');if(!b.length){tb.innerHTML='<tr><td colspan="6" style="color:#333">none</td></tr>';return}tb.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}return'<tr><td>'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,10)+'…</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const tb=document.querySelector('#t-payouts tbody');if(!p.length){tb.innerHTML='<tr><td colspan="4" style="color:#333">none</td></tr>';return}tb.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td>'+r.amount_zec.toFixed(8)+'</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,10)+'…':'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script>
</body>
</html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 3 — "Cipher" — Amber/gold on deep black, encrypted data aesthetic
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW3: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TAZ Mining Pool — Cipher</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:'Courier New',monospace;background:#08080a;color:#c8a832;min-height:100vh;font-size:13px}
::selection{background:#c8a832;color:#08080a}
.wrap{max-width:1200px;margin:0 auto;padding:1rem 1.5rem}
.hdr{display:flex;align-items:center;justify-content:space-between;padding-bottom:0.75rem;border-bottom:1px solid #1a1608;margin-bottom:1rem}
.hdr h1{font-size:0.9rem;letter-spacing:0.3em;text-transform:uppercase;color:#c8a832}
.hdr .tag{font-size:0.55rem;background:#1a1608;color:#645420;padding:0.15rem 0.5rem;margin-left:0.75rem;letter-spacing:0.1em}
.hdr nav a{color:#645420;text-decoration:none;font-size:0.65rem;margin-left:1rem;letter-spacing:0.08em;text-transform:uppercase}
.hdr nav a:hover{color:#c8a832}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:1px;background:#1a1608;border:1px solid #1a1608;margin-bottom:1rem}
.card{background:#0c0c0e;padding:0.75rem 1rem}
.card .lbl{font-size:0.5rem;text-transform:uppercase;letter-spacing:0.2em;color:#645420;margin-bottom:0.3rem}
.card .num{font-size:1.8rem;font-weight:700;color:#c8a832;line-height:1}
.card .num.med{font-size:1.1rem}
.cards6{grid-template-columns:repeat(6,1fr)}
.sep{height:1px;background:#1a1608;margin:0.75rem 0}
.stitle{font-size:0.5rem;text-transform:uppercase;letter-spacing:0.25em;color:#645420;margin-bottom:0.5rem}
.stitle::before{content:'[ ';color:#322a10}
.stitle::after{content:' ]';color:#322a10}
table{width:100%;border-collapse:collapse}
th{font-size:0.5rem;text-transform:uppercase;letter-spacing:0.15em;color:#322a10;text-align:left;padding:0.35rem 0.5rem;border-bottom:1px solid #1a1608}
td{padding:0.3rem 0.5rem;border-bottom:1px solid #0e0e10;color:#967b28;font-size:0.75rem;white-space:nowrap}
td.hi{color:#c8a832}
td.addr{max-width:170px;overflow:hidden;text-overflow:ellipsis;cursor:pointer}
td.addr:hover{color:#c8a832;text-decoration:underline}
tr:hover td{background:#0e0e10}
.status-confirmed{color:#c8a832}
.status-pending{color:#645420}
.status-orphaned{color:#4a1a1a}
.luck-good{color:#c8a832}
.luck-mid{color:#967b28}
.luck-bad{color:#6b2020}
.foot{border-top:1px solid #1a1608;margin-top:1rem;padding-top:0.5rem;font-size:0.6rem;color:#322a10;display:flex;gap:2rem}
.foot .on{color:#645420}
.foot .off{color:#6b2020}
@media(max-width:800px){.cards{grid-template-columns:1fr 1fr}.cards6{grid-template-columns:repeat(3,1fr)}}
</style>
</head>
<body>
<div class="wrap">
<div class="hdr">
    <div style="display:flex;align-items:center"><h1>TAZ Pool</h1><span class="tag">testnet</span></div>
    <nav>
        <a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">mine</a>
        <a href="/zallet">wallet</a>
        <a href="/">v1</a>
    </nav>
</div>
<div class="cards">
    <div class="card"><div class="lbl">Pool Hashrate</div><div class="num" id="s-hash">--</div></div>
    <div class="card"><div class="lbl">Network</div><div class="num" id="s-net">--</div></div>
    <div class="card"><div class="lbl">Blocks</div><div class="num" id="s-blocks">0</div></div>
    <div class="card"><div class="lbl">Miners</div><div class="num" id="s-miners">0</div></div>
</div>
<div class="cards cards6">
    <div class="card"><div class="lbl">Shares</div><div class="num med" id="s-shares">0</div></div>
    <div class="card"><div class="lbl">Luck 24h</div><div class="num med" id="s-luck">--</div></div>
    <div class="card"><div class="lbl">Net Share</div><div class="num med" id="s-pct">--</div></div>
    <div class="card"><div class="lbl">Immature</div><div class="num med" id="s-imm">0</div></div>
    <div class="card"><div class="lbl">Fee</div><div class="num med" id="s-fee">--</div></div>
    <div class="card"><div class="lbl">Stratum</div><div class="num med" id="s-port">--</div></div>
</div>

<div class="sep"></div>
<div class="stitle">miners</div>
<table id="t-miners"><thead><tr><th>address</th><th>1m</th><th>10m</th><th>w</th><th>shares</th><th>pending</th></tr></thead><tbody><tr><td colspan="6" style="color:#322a10">...</td></tr></tbody></table>

<div class="sep"></div>
<div class="stitle">blocks</div>
<table id="t-blocks"><thead><tr><th>height</th><th>hash</th><th>reward</th><th>luck</th><th>status</th><th>found</th></tr></thead><tbody><tr><td colspan="6" style="color:#322a10">...</td></tr></tbody></table>

<div class="sep"></div>
<div class="stitle">payouts</div>
<table id="t-payouts"><thead><tr><th>miner</th><th>amount</th><th>txid</th><th>date</th></tr></thead><tbody><tr><td colspan="4" style="color:#322a10">...</td></tr></tbody></table>

<div class="foot">
    <span>node <span id="f-node" class="on">--</span></span>
    <span>wallet <span id="f-wallet" class="on">--</span></span>
    <span>up <span id="f-up">--</span></span>
</div>
</div>
<script>
const started=Date.now();
function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}
function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h '+m%60+'m':m>0?m+'m '+s%60+'s':s+'s'}
function $(id){return document.getElementById(id)}
async function tick(){try{const d=await(await fetch('/api/pool/stats')).json();$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;if(d.luck_percent!=null){const l=$('s-luck');l.textContent=d.luck_percent.toFixed(0)+'%';l.className='num med '+(d.luck_percent<=100?'luck-good':d.luck_percent<=150?'luck-mid':'luck-bad')}if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';$('f-node').textContent=d.node_ok?'ok':'down';$('f-node').className=d.node_ok?'on':'off';$('f-wallet').textContent=d.wallet_ok?'ok':'down';$('f-wallet').className=d.wallet_ok?'on':'off';$('f-up').textContent=fd(Date.now()-started)}catch(e){}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const tb=document.querySelector('#t-miners tbody');if(!m.length){tb.innerHTML='<tr><td colspan="6" style="color:#322a10">none</td></tr>';return}tb.innerHTML=m.map(r=>'<tr><td class="addr hi" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td>'+r.pending_zec.toFixed(4)+'</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const tb=document.querySelector('#t-blocks tbody');if(!b.length){tb.innerHTML='<tr><td colspan="6" style="color:#322a10">none</td></tr>';return}tb.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}return'<tr><td class="hi">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,10)+'…</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="status-'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const tb=document.querySelector('#t-payouts tbody');if(!p.length){tb.innerHTML='<tr><td colspan="4" style="color:#322a10">none</td></tr>';return}tb.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="hi">'+r.amount_zec.toFixed(8)+' TAZ</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,10)+'…':'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script>
</body>
</html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 4 — "Void" — Ultra-minimal, near-invisible borders, whisper-quiet
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW4: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TAZ Mining Pool — Void</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#060608;color:#888;min-height:100vh;font-size:13px}
::selection{background:#333;color:#fff}
.wrap{max-width:1100px;margin:0 auto;padding:2rem}
.hdr{display:flex;justify-content:space-between;align-items:baseline;margin-bottom:3rem}
.hdr h1{font-size:0.75rem;font-weight:400;letter-spacing:0.4em;text-transform:uppercase;color:#444}
.hdr nav a{color:#333;text-decoration:none;font-size:0.65rem;margin-left:1.5rem;letter-spacing:0.1em;text-transform:uppercase;transition:color 0.2s}
.hdr nav a:hover{color:#aaa}
.hero{display:grid;grid-template-columns:repeat(4,1fr);gap:3rem;margin-bottom:3rem}
.hero-item .lbl{font-size:0.55rem;text-transform:uppercase;letter-spacing:0.2em;color:#333;margin-bottom:0.5rem}
.hero-item .val{font-family:'JetBrains Mono','Courier New',monospace;font-size:2.2rem;font-weight:200;color:#ccc;line-height:1}
.meta{display:grid;grid-template-columns:repeat(6,1fr);gap:2rem;margin-bottom:3rem;padding-bottom:2rem;border-bottom:1px solid #111}
.meta-item .lbl{font-size:0.5rem;text-transform:uppercase;letter-spacing:0.2em;color:#333;margin-bottom:0.3rem}
.meta-item .val{font-family:'JetBrains Mono','Courier New',monospace;font-size:0.9rem;color:#666}
.stitle{font-size:0.5rem;text-transform:uppercase;letter-spacing:0.3em;color:#333;margin-bottom:1rem;margin-top:2rem}
table{width:100%;border-collapse:collapse;margin-bottom:2rem}
th{font-size:0.5rem;text-transform:uppercase;letter-spacing:0.15em;color:#333;text-align:left;padding:0.5rem 0;border-bottom:1px solid #111;font-weight:400}
td{padding:0.5rem 0;border-bottom:1px solid #0a0a0c;font-size:0.8rem;color:#555;padding-right:1rem}
td.hi{color:#aaa}
td.addr{max-width:180px;overflow:hidden;text-overflow:ellipsis;cursor:pointer;font-family:'JetBrains Mono','Courier New',monospace;font-size:0.7rem}
td.addr:hover{color:#ccc}
tr:hover td{color:#999}
.confirmed{color:#555}
.pending{color:#444}
.orphaned{color:#2a2a2a}
.luck-good{color:#888}
.luck-mid{color:#555}
.luck-bad{color:#333}
.foot{margin-top:3rem;font-size:0.55rem;color:#222;display:flex;gap:2rem;text-transform:uppercase;letter-spacing:0.15em}
@media(max-width:800px){.hero{grid-template-columns:1fr 1fr;gap:2rem}.meta{grid-template-columns:repeat(3,1fr)}}
@media(max-width:500px){.hero{grid-template-columns:1fr}.meta{grid-template-columns:1fr 1fr}}
</style>
</head>
<body>
<div class="wrap">
<div class="hdr">
    <h1>taz pool</h1>
    <nav>
        <a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">mine</a>
        <a href="/zallet">wallet</a>
        <a href="/">v1</a>
    </nav>
</div>
<div class="hero">
    <div class="hero-item"><div class="lbl">Hashrate</div><div class="val" id="s-hash">--</div></div>
    <div class="hero-item"><div class="lbl">Network</div><div class="val" id="s-net">--</div></div>
    <div class="hero-item"><div class="lbl">Blocks</div><div class="val" id="s-blocks">0</div></div>
    <div class="hero-item"><div class="lbl">Miners</div><div class="val" id="s-miners">0</div></div>
</div>
<div class="meta">
    <div class="meta-item"><div class="lbl">Shares</div><div class="val" id="s-shares">0</div></div>
    <div class="meta-item"><div class="lbl">Luck</div><div class="val" id="s-luck">--</div></div>
    <div class="meta-item"><div class="lbl">Net Share</div><div class="val" id="s-pct">--</div></div>
    <div class="meta-item"><div class="lbl">Immature</div><div class="val" id="s-imm">0</div></div>
    <div class="meta-item"><div class="lbl">Fee</div><div class="val" id="s-fee">--</div></div>
    <div class="meta-item"><div class="lbl">Stratum</div><div class="val" id="s-port">--</div></div>
</div>

<div class="stitle">miners</div>
<table id="t-miners"><thead><tr><th>address</th><th>1m</th><th>10m</th><th>workers</th><th>shares</th><th>pending</th></tr></thead><tbody><tr><td colspan="6">...</td></tr></tbody></table>

<div class="stitle">blocks</div>
<table id="t-blocks"><thead><tr><th>height</th><th>hash</th><th>reward</th><th>luck</th><th>status</th><th>found</th></tr></thead><tbody><tr><td colspan="6">...</td></tr></tbody></table>

<div class="stitle">payouts</div>
<table id="t-payouts"><thead><tr><th>miner</th><th>amount</th><th>txid</th><th>date</th></tr></thead><tbody><tr><td colspan="4">...</td></tr></tbody></table>

<div class="foot">
    <span>node <span id="f-node">--</span></span>
    <span>wallet <span id="f-wallet">--</span></span>
    <span>up <span id="f-up">--</span></span>
</div>
</div>
<script>
const started=Date.now();
function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' T';if(h>=1e9)return(h/1e9).toFixed(2)+' G';if(h>=1e6)return(h/1e6).toFixed(2)+' M';if(h>=1e3)return(h/1e3).toFixed(2)+' K';return h.toFixed(0)}
function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h '+m%60+'m':m>0?m+'m '+s%60+'s':s+'s'}
function $(id){return document.getElementById(id)}
async function tick(){try{const d=await(await fetch('/api/pool/stats')).json();$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;if(d.luck_percent!=null){$('s-luck').textContent=d.luck_percent.toFixed(0)+'%'}if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';$('f-node').textContent=d.node_ok?'ok':'down';$('f-wallet').textContent=d.wallet_ok?'ok':'down';$('f-up').textContent=fd(Date.now()-started)}catch(e){}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const tb=document.querySelector('#t-miners tbody');if(!m.length){tb.innerHTML='<tr><td colspan="6">none</td></tr>';return}tb.innerHTML=m.map(r=>'<tr><td class="addr" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td>'+r.pending_zec.toFixed(4)+'</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const tb=document.querySelector('#t-blocks tbody');if(!b.length){tb.innerHTML='<tr><td colspan="6">none</td></tr>';return}tb.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}return'<tr><td class="hi">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,10)+'…</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const tb=document.querySelector('#t-payouts tbody');if(!p.length){tb.innerHTML='<tr><td colspan="4">none</td></tr>';return}tb.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="hi">'+r.amount_zec.toFixed(8)+'</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,10)+'…':'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script>
</body>
</html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 5 — "Neon Grid" — Cyberpunk neon accents, dark panels, glowing edges
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW5: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TAZ Mining Pool — Neon Grid</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:'JetBrains Mono','Courier New',monospace;background:#05050a;color:#a0a0b0;min-height:100vh;font-size:13px}
::selection{background:#0ff;color:#000}
.wrap{max-width:1200px;margin:0 auto;padding:1rem 1.5rem}
.hdr{display:flex;align-items:center;justify-content:space-between;padding:0.75rem 0;margin-bottom:1rem;border-bottom:1px solid #0a0a18}
.hdr h1{font-size:1rem;font-weight:700;color:#0ff;letter-spacing:0.15em;text-transform:uppercase;text-shadow:0 0 10px rgba(0,255,255,0.3)}
.hdr .badge{font-size:0.5rem;color:#0a0a18;background:#0ff;padding:0.1rem 0.4rem;margin-left:0.5rem;font-weight:700;letter-spacing:0.1em}
.hdr nav a{color:#335;text-decoration:none;font-size:0.65rem;margin-left:1.25rem;text-transform:uppercase;letter-spacing:0.08em;transition:color 0.2s,text-shadow 0.2s}
.hdr nav a:hover{color:#0ff;text-shadow:0 0 8px rgba(0,255,255,0.4)}
.grid4{display:grid;grid-template-columns:repeat(4,1fr);gap:1px;margin-bottom:1px}
.grid6{display:grid;grid-template-columns:repeat(6,1fr);gap:1px;margin-bottom:1rem}
.panel{background:#080810;padding:0.75rem 1rem;border:1px solid #0a0a18;position:relative;overflow:hidden}
.panel::before{content:'';position:absolute;top:0;left:0;right:0;height:1px;background:linear-gradient(90deg,transparent,rgba(0,255,255,0.15),transparent)}
.panel .lbl{font-size:0.5rem;text-transform:uppercase;letter-spacing:0.2em;color:#335;margin-bottom:0.3rem}
.panel .val{font-size:1.6rem;font-weight:700;color:#e0e0f0;line-height:1}
.panel .val.accent{color:#0ff;text-shadow:0 0 8px rgba(0,255,255,0.2)}
.panel .val.sm{font-size:1rem}
.stitle{font-size:0.5rem;text-transform:uppercase;letter-spacing:0.25em;color:#335;margin:1rem 0 0.5rem;padding-left:0.25rem;border-left:2px solid #0ff}
table{width:100%;border-collapse:collapse;margin-bottom:0.5rem}
th{font-size:0.5rem;text-transform:uppercase;letter-spacing:0.12em;color:#224;text-align:left;padding:0.4rem 0.5rem;border-bottom:1px solid #0a0a18;font-weight:400}
td{padding:0.35rem 0.5rem;border-bottom:1px solid #08081a;color:#667;font-size:0.75rem;white-space:nowrap}
td.hi{color:#c0c0d0}
td.cyan{color:#0ff}
td.addr{max-width:170px;overflow:hidden;text-overflow:ellipsis;cursor:pointer}
td.addr:hover{color:#0ff;text-shadow:0 0 5px rgba(0,255,255,0.3)}
tr:hover td{background:#0a0a14}
.status-confirmed{color:#0f8}
.status-pending{color:#ff0}
.status-orphaned{color:#f33}
.luck-good{color:#0f8}
.luck-mid{color:#ff0}
.luck-bad{color:#f33}
.foot{margin-top:1.5rem;padding:0.5rem 0;border-top:1px solid #0a0a18;font-size:0.55rem;color:#224;display:flex;gap:2rem;text-transform:uppercase;letter-spacing:0.1em}
.foot .on{color:#0f8}
.foot .off{color:#f33}
@media(max-width:900px){.grid4{grid-template-columns:1fr 1fr}.grid6{grid-template-columns:repeat(3,1fr)}}
@media(max-width:500px){.grid4{grid-template-columns:1fr}.grid6{grid-template-columns:1fr 1fr}}
</style>
</head>
<body>
<div class="wrap">
<div class="hdr">
    <div style="display:flex;align-items:center"><h1>TAZ Pool</h1><span class="badge">TESTNET</span></div>
    <nav>
        <a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">mine</a>
        <a href="/zallet">wallet</a>
        <a href="/">v1</a>
    </nav>
</div>
<div class="grid4">
    <div class="panel"><div class="lbl">Pool Hashrate</div><div class="val accent" id="s-hash">--</div></div>
    <div class="panel"><div class="lbl">Network</div><div class="val" id="s-net">--</div></div>
    <div class="panel"><div class="lbl">Blocks Found</div><div class="val accent" id="s-blocks">0</div></div>
    <div class="panel"><div class="lbl">Miners Online</div><div class="val" id="s-miners">0</div></div>
</div>
<div class="grid6">
    <div class="panel"><div class="lbl">Shares</div><div class="val sm" id="s-shares">0</div></div>
    <div class="panel"><div class="lbl">Luck 24h</div><div class="val sm" id="s-luck">--</div></div>
    <div class="panel"><div class="lbl">Net Share</div><div class="val sm" id="s-pct">--</div></div>
    <div class="panel"><div class="lbl">Immature</div><div class="val sm" id="s-imm">0</div></div>
    <div class="panel"><div class="lbl">Fee</div><div class="val sm" id="s-fee">--</div></div>
    <div class="panel"><div class="lbl">Stratum</div><div class="val sm" id="s-port">--</div></div>
</div>

<div class="stitle">Miners</div>
<table id="t-miners"><thead><tr><th>address</th><th>1m</th><th>10m</th><th>w</th><th>shares</th><th>pending</th></tr></thead><tbody><tr><td colspan="6" style="color:#224">...</td></tr></tbody></table>

<div class="stitle">Recent Blocks</div>
<table id="t-blocks"><thead><tr><th>height</th><th>hash</th><th>reward</th><th>luck</th><th>status</th><th>found</th></tr></thead><tbody><tr><td colspan="6" style="color:#224">...</td></tr></tbody></table>

<div class="stitle">Payouts</div>
<table id="t-payouts"><thead><tr><th>miner</th><th>amount</th><th>txid</th><th>date</th></tr></thead><tbody><tr><td colspan="4" style="color:#224">...</td></tr></tbody></table>

<div class="foot">
    <span>node <span id="f-node" class="on">--</span></span>
    <span>wallet <span id="f-wallet" class="on">--</span></span>
    <span>up <span id="f-up">--</span></span>
</div>
</div>
<script>
const started=Date.now();
function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}
function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h '+m%60+'m':m>0?m+'m '+s%60+'s':s+'s'}
function $(id){return document.getElementById(id)}
async function tick(){try{const d=await(await fetch('/api/pool/stats')).json();$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;if(d.luck_percent!=null){const l=$('s-luck');l.textContent=d.luck_percent.toFixed(0)+'%';l.className='val sm '+(d.luck_percent<=100?'luck-good':d.luck_percent<=150?'luck-mid':'luck-bad')}if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';$('f-node').textContent=d.node_ok?'ok':'down';$('f-node').className=d.node_ok?'on':'off';$('f-wallet').textContent=d.wallet_ok?'ok':'down';$('f-wallet').className=d.wallet_ok?'on':'off';$('f-up').textContent=fd(Date.now()-started)}catch(e){}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const tb=document.querySelector('#t-miners tbody');if(!m.length){tb.innerHTML='<tr><td colspan="6" style="color:#224">none</td></tr>';return}tb.innerHTML=m.map(r=>'<tr><td class="addr hi" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td>'+r.pending_zec.toFixed(4)+'</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const tb=document.querySelector('#t-blocks tbody');if(!b.length){tb.innerHTML='<tr><td colspan="6" style="color:#224">none</td></tr>';return}tb.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}return'<tr><td class="cyan">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,10)+'…</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="status-'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const tb=document.querySelector('#t-payouts tbody');if(!p.length){tb.innerHTML='<tr><td colspan="4" style="color:#224">none</td></tr>';return}tb.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="cyan">'+r.amount_zec.toFixed(8)+' TAZ</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,10)+'…':'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script>
</body>
</html>
"##;
