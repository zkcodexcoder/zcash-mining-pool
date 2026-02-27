// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 6 — "Hologram" — Electric blue, frosted glass, depth layers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW6: &str = r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>TAZ Mining Pool — Hologram</title>
<style>
:root{--bg:#060818;--glass:rgba(10,18,40,0.55);--border:rgba(80,140,255,0.18);--accent:#4da6ff;--accent2:#80c0ff;--ink:#c8d8f0;--muted:#5a7090;--dim:#2a3a58;--glow:rgba(77,166,255,0.12)}
*{margin:0;padding:0;box-sizing:border-box}
html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:'Inter',-apple-system,'Segoe UI',sans-serif;font-size:13px;overflow-x:hidden}
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;
  background:radial-gradient(ellipse 1000px 500px at 30% -5%,rgba(77,166,255,0.08),transparent 60%),
             radial-gradient(ellipse 800px 400px at 70% 105%,rgba(120,80,255,0.06),transparent 55%)}
main{max-width:1300px;margin:0 auto;padding:24px}
.top{display:flex;justify-content:space-between;align-items:center;padding:16px 22px;border-radius:16px;margin-bottom:16px;
  background:var(--glass);backdrop-filter:blur(20px);-webkit-backdrop-filter:blur(20px);
  border:1px solid var(--border);box-shadow:0 8px 32px rgba(0,0,0,0.3),inset 0 1px 0 rgba(255,255,255,0.04)}
.top h1{font-size:20px;font-weight:600;color:#e0eeff;letter-spacing:1px}
.top .sub{color:var(--muted);font-size:11px;margin-top:2px}
.dot{width:8px;height:8px;border-radius:50%;background:#4da6ff;box-shadow:0 0 8px rgba(77,166,255,0.6)}
.status{display:flex;align-items:center;gap:8px;font-size:11px;color:var(--muted);
  padding:6px 14px;border-radius:999px;background:rgba(10,18,40,0.6);border:1px solid var(--border)}
.nav{display:flex;gap:8px}
.nav a{color:var(--muted);text-decoration:none;font-size:10px;letter-spacing:1px;text-transform:uppercase;
  padding:6px 14px;border-radius:8px;border:1px solid var(--border);background:rgba(10,18,40,0.4);
  backdrop-filter:blur(8px);transition:all 0.2s}
.nav a:hover{color:var(--accent);border-color:rgba(77,166,255,0.35);box-shadow:0 0 12px var(--glow)}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:12px;margin-bottom:12px}
.cards6{grid-template-columns:repeat(6,1fr)}
.card{border-radius:14px;padding:14px 16px;background:var(--glass);backdrop-filter:blur(16px);-webkit-backdrop-filter:blur(16px);
  border:1px solid var(--border);box-shadow:0 4px 20px rgba(0,0,0,0.2),inset 0 1px 0 rgba(255,255,255,0.03);
  position:relative;overflow:hidden}
.card::after{content:'';position:absolute;top:0;left:0;right:0;height:1px;
  background:linear-gradient(90deg,transparent,rgba(77,166,255,0.2),transparent)}
.card .lbl{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--muted);margin-bottom:6px}
.card .val{font-size:24px;font-weight:600;color:#e0eeff;line-height:1}
.card .val.sm{font-size:14px;font-weight:400;color:var(--accent2)}
.stitle{font-size:9px;text-transform:uppercase;letter-spacing:3px;color:var(--dim);margin:20px 0 10px}
table{width:100%;border-collapse:collapse;margin-bottom:12px;background:var(--glass);
  backdrop-filter:blur(12px);border-radius:10px;overflow:hidden;border:1px solid var(--border)}
th{font-size:9px;text-transform:uppercase;letter-spacing:1.5px;color:var(--dim);text-align:left;
  padding:8px 12px;border-bottom:1px solid var(--border);font-weight:400;background:rgba(10,18,40,0.3)}
td{padding:6px 12px;border-bottom:1px solid rgba(80,140,255,0.06);color:var(--muted);font-size:12px;white-space:nowrap}
td.hi{color:#c8d8f0}td.accent{color:var(--accent)}
td.addr{max-width:170px;overflow:hidden;text-overflow:ellipsis;cursor:pointer;font-family:monospace;font-size:11px}
td.addr:hover{color:var(--accent)}
tr:hover td{background:rgba(77,166,255,0.03)}
.confirmed{color:#4da6ff}.pending{color:#ffa033}.orphaned{color:#ff4466}
.luck-good{color:#4da6ff}.luck-mid{color:#ffa033}.luck-bad{color:#ff4466}
.foot{margin-top:20px;padding:10px 16px;border-radius:10px;font-size:9px;color:var(--dim);
  display:flex;gap:24px;text-transform:uppercase;letter-spacing:1.5px;
  background:var(--glass);border:1px solid var(--border)}
.foot .on{color:var(--accent)}.foot .off{color:#ff4466}
@media(max-width:1000px){.cards{grid-template-columns:1fr 1fr}.cards6{grid-template-columns:repeat(3,1fr)}}
@media(max-width:600px){.cards{grid-template-columns:1fr}.cards6{grid-template-columns:1fr 1fr}}
</style></head><body><main>
<div class="top">
  <div><h1>TAZ Mining Pool</h1><div class="sub">Zcash Testnet &middot; Equihash(200,9) &middot; PPLNS</div></div>
  <div style="display:flex;align-items:center;gap:14px">
    <div class="status"><span class="dot"></span><span id="f-status">Online</span></div>
    <div class="nav"><a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a><a href="/zallet">Wallet</a><a href="/previews">Themes</a><a href="/">V1</a></div>
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
<table id="t-miners"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>W</th><th>Shares</th><th>Pending</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dim)">loading...</td></tr></tbody></table>
<div class="stitle">Blocks</div>
<table id="t-blocks"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dim)">loading...</td></tr></tbody></table>
<div class="stitle">Payouts</div>
<table id="t-payouts"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead><tbody><tr><td colspan="4" style="color:var(--dim)">loading...</td></tr></tbody></table>
<div class="foot"><span>Node: <span id="f-node" class="on">--</span></span><span>Wallet: <span id="f-wallet" class="on">--</span></span><span>Up: <span id="f-up">--</span></span></div>
</main>
<script>
const S=Date.now();function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h '+m%60+'m':m>0?m+'m '+s%60+'s':s+'s'}function $(i){return document.getElementById(i)}
async function tick(){try{const d=await(await fetch('/api/pool/stats')).json();$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;if(d.luck_percent!=null){$('s-luck').textContent=d.luck_percent.toFixed(0)+'%';$('s-luck').className='val sm '+(d.luck_percent<=100?'luck-good':d.luck_percent<=150?'luck-mid':'luck-bad')}if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';$('f-node').textContent=d.node_ok?'OK':'DOWN';$('f-node').className=d.node_ok?'on':'off';$('f-wallet').textContent=d.wallet_ok?'OK':'DOWN';$('f-wallet').className=d.wallet_ok?'on':'off';$('f-up').textContent=fd(Date.now()-S);$('f-status').textContent=d.node_ok?'Online':'Offline'}catch(e){}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const t=document.querySelector('#t-miners tbody');if(!m.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=m.map(r=>'<tr><td class="addr hi" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td class="accent">'+r.pending_zec.toFixed(4)+' TAZ</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const t=document.querySelector('#t-blocks tbody');if(!b.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}return'<tr><td class="accent">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,12)+'</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const t=document.querySelector('#t-payouts tbody');if(!p.length){t.innerHTML='<tr><td colspan="4">none</td></tr>';return}t.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="accent">'+r.amount_zec.toFixed(8)+' TAZ</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,12):'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script></body></html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 7 — "Ember" — Deep red/orange volcanic, glowing coals
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW7: &str = r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>TAZ Mining Pool — Ember</title>
<style>
:root{--bg:#0a0404;--accent:#ff6633;--accent2:#ff9944;--dim:#6a3020;--dimmer:#3a1810;--ink:#f0d0c0;--muted:#8a5540;--line:rgba(255,102,51,0.12);--glow:rgba(255,102,51,0.15)}
*{margin:0;padding:0;box-sizing:border-box}html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:'Courier New',monospace;font-size:13px;overflow-x:hidden}
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;background:radial-gradient(900px 450px at 50% 100%,rgba(255,60,20,0.08),transparent 55%),radial-gradient(600px 300px at 20% -5%,rgba(255,140,60,0.05),transparent 50%)}
main{max-width:1300px;margin:0 auto;padding:20px}
.top{display:flex;justify-content:space-between;align-items:center;padding:14px 20px;border-radius:12px;margin-bottom:14px;background:radial-gradient(120% 160% at 50% 100%,rgba(255,60,20,0.06),transparent 50%),linear-gradient(155deg,rgba(18,6,4,0.92),rgba(12,4,2,0.88));border:1px solid var(--line);box-shadow:0 0 20px var(--glow);position:relative;overflow:hidden}
.top::before{content:'';position:absolute;inset:0;border-radius:inherit;padding:1px;background:linear-gradient(110deg,rgba(255,102,51,0.4),rgba(255,153,68,0.3),rgba(255,60,20,0.4));opacity:0.2;pointer-events:none;-webkit-mask:linear-gradient(#000 0 0) content-box,linear-gradient(#000 0 0);-webkit-mask-composite:xor;mask-composite:exclude}
.top h1{font-size:20px;font-weight:700;color:var(--accent);letter-spacing:2px;text-transform:uppercase;text-shadow:0 0 10px rgba(255,102,51,0.5),0 0 20px rgba(255,60,20,0.3);position:relative;z-index:2}
.top .sub{color:var(--dim);font-size:11px;margin-top:2px;position:relative;z-index:2}
.nav{display:flex;gap:8px;position:relative;z-index:2}
.nav a{color:var(--dimmer);text-decoration:none;font-size:10px;letter-spacing:1.5px;text-transform:uppercase;padding:5px 12px;border:1px solid var(--line);border-radius:4px;transition:all 0.2s}
.nav a:hover{color:var(--accent);border-color:rgba(255,102,51,0.3);box-shadow:0 0 8px var(--glow)}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:10px;margin-bottom:10px}.cards6{grid-template-columns:repeat(6,1fr)}
.card{border-radius:10px;padding:12px 14px;position:relative;overflow:hidden;background:radial-gradient(100% 150% at 50% 100%,rgba(255,60,20,0.04),transparent 50%),linear-gradient(155deg,rgba(18,6,4,0.92),rgba(12,4,2,0.88));border:1px solid var(--line);box-shadow:0 0 12px var(--glow)}
.card .lbl{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--dim);margin-bottom:4px}
.card .val{font-size:24px;font-weight:700;color:var(--accent);line-height:1;text-shadow:0 0 6px rgba(255,102,51,0.5),0 0 14px rgba(255,60,20,0.3)}
.card .val.sm{font-size:14px;color:var(--accent2);text-shadow:0 0 4px rgba(255,153,68,0.3)}
.stitle{font-size:9px;text-transform:uppercase;letter-spacing:3px;color:var(--dim);margin:16px 0 8px;padding-left:10px;border-left:2px solid var(--accent)}
table{width:100%;border-collapse:collapse;margin-bottom:8px}
th{font-size:9px;text-transform:uppercase;letter-spacing:1.5px;color:var(--dimmer);text-align:left;padding:6px 10px;border-bottom:1px solid var(--line);font-weight:400}
td{padding:5px 10px;border-bottom:1px solid rgba(255,102,51,0.05);color:var(--muted);font-size:12px;white-space:nowrap}
td.hi{color:var(--accent);text-shadow:0 0 4px rgba(255,102,51,0.3)}td.addr{max-width:170px;overflow:hidden;text-overflow:ellipsis;cursor:pointer}td.addr:hover{color:var(--accent)}tr:hover td{background:rgba(255,60,20,0.03)}
.confirmed{color:var(--accent)}.pending{color:var(--accent2)}.orphaned{color:#993333}.luck-good{color:var(--accent)}.luck-mid{color:var(--accent2)}.luck-bad{color:#ff2233}
.foot{margin-top:16px;padding:10px 0;border-top:1px solid var(--line);font-size:9px;color:var(--dimmer);display:flex;gap:24px;text-transform:uppercase;letter-spacing:1.5px}.foot .on{color:var(--accent)}.foot .off{color:#ff2233}
@media(max-width:900px){.cards{grid-template-columns:1fr 1fr}.cards6{grid-template-columns:repeat(3,1fr)}}
</style></head><body><main>
<div class="top"><div><h1>TAZ Pool</h1><div class="sub">Zcash Testnet &middot; Equihash(200,9) &middot; PPLNS</div></div><div class="nav"><a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a><a href="/zallet">Wallet</a><a href="/previews">Themes</a><a href="/">V1</a></div></div>
<div class="cards"><div class="card"><div class="lbl">Pool Hashrate</div><div class="val" id="s-hash">--</div></div><div class="card"><div class="lbl">Network</div><div class="val" id="s-net">--</div></div><div class="card"><div class="lbl">Blocks</div><div class="val" id="s-blocks">0</div></div><div class="card"><div class="lbl">Miners</div><div class="val" id="s-miners">0</div></div></div>
<div class="cards cards6"><div class="card"><div class="lbl">Shares</div><div class="val sm" id="s-shares">0</div></div><div class="card"><div class="lbl">Luck</div><div class="val sm" id="s-luck">--</div></div><div class="card"><div class="lbl">Net %</div><div class="val sm" id="s-pct">--</div></div><div class="card"><div class="lbl">Immature</div><div class="val sm" id="s-imm">0</div></div><div class="card"><div class="lbl">Fee</div><div class="val sm" id="s-fee">--</div></div><div class="card"><div class="lbl">Stratum</div><div class="val sm" id="s-port">--</div></div></div>
<div class="stitle">Miners</div><table id="t-miners"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>W</th><th>Shares</th><th>Pending</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="stitle">Blocks</div><table id="t-blocks"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="stitle">Payouts</div><table id="t-payouts"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead><tbody><tr><td colspan="4" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="foot"><span>Node: <span id="f-node" class="on">--</span></span><span>Wallet: <span id="f-wallet" class="on">--</span></span><span>Up: <span id="f-up">--</span></span></div>
</main><script>
const S=Date.now();function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h '+m%60+'m':m>0?m+'m '+s%60+'s':s+'s'}function $(i){return document.getElementById(i)}
async function tick(){try{const d=await(await fetch('/api/pool/stats')).json();$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;if(d.luck_percent!=null){$('s-luck').textContent=d.luck_percent.toFixed(0)+'%';$('s-luck').className='val sm '+(d.luck_percent<=100?'luck-good':d.luck_percent<=150?'luck-mid':'luck-bad')}if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';$('f-node').textContent=d.node_ok?'OK':'DOWN';$('f-node').className=d.node_ok?'on':'off';$('f-wallet').textContent=d.wallet_ok?'OK':'DOWN';$('f-wallet').className=d.wallet_ok?'on':'off';$('f-up').textContent=fd(Date.now()-S)}catch(e){}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const t=document.querySelector('#t-miners tbody');if(!m.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=m.map(r=>'<tr><td class="addr hi" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td class="hi">'+r.pending_zec.toFixed(4)+' TAZ</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const t=document.querySelector('#t-blocks tbody');if(!b.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}return'<tr><td class="hi">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,12)+'</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const t=document.querySelector('#t-payouts tbody');if(!p.length){t.innerHTML='<tr><td colspan="4">none</td></tr>';return}t.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="hi">'+r.amount_zec.toFixed(8)+' TAZ</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,12):'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script></body></html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 8 — "Synthwave" — 80s retro purple/pink, grid horizon
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW8: &str = r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>TAZ Mining Pool — Synthwave</title>
<style>
:root{--bg:#0d0520;--accent:#ff44cc;--accent2:#aa55ff;--ink:#e0d0f0;--muted:#7a5a8a;--dim:#3a2550;--line:rgba(255,68,204,0.12);--glow:rgba(255,68,204,0.12)}
*{margin:0;padding:0;box-sizing:border-box}html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:'Segoe UI','Helvetica Neue',sans-serif;font-size:13px;overflow-x:hidden}
body::before{content:'';position:fixed;bottom:0;left:0;right:0;height:50vh;pointer-events:none;z-index:-1;background:linear-gradient(0deg,rgba(255,68,100,0.08),rgba(170,85,255,0.06) 40%,transparent 80%)}
body::after{content:'';position:fixed;bottom:0;left:-50%;right:-50%;height:40vh;pointer-events:none;z-index:-1;background-image:linear-gradient(rgba(255,68,204,0.06) 1px,transparent 1px),linear-gradient(90deg,rgba(255,68,204,0.06) 1px,transparent 1px);background-size:80px 80px;transform:perspective(500px) rotateX(60deg);transform-origin:bottom center}
main{max-width:1300px;margin:0 auto;padding:20px;position:relative;z-index:2}
.top{display:flex;justify-content:space-between;align-items:center;padding:16px 22px;border-radius:14px;margin-bottom:16px;background:linear-gradient(135deg,rgba(15,8,35,0.92),rgba(20,10,40,0.88));border:1px solid var(--line);box-shadow:0 0 24px var(--glow);position:relative;overflow:hidden}
.top::before{content:'';position:absolute;inset:0;border-radius:inherit;padding:1.5px;background:linear-gradient(110deg,var(--accent),var(--accent2),#ff8844);opacity:0.25;pointer-events:none;-webkit-mask:linear-gradient(#000 0 0) content-box,linear-gradient(#000 0 0);-webkit-mask-composite:xor;mask-composite:exclude}
.top h1{font-size:22px;font-weight:700;position:relative;z-index:2;background:linear-gradient(90deg,var(--accent),var(--accent2));-webkit-background-clip:text;-webkit-text-fill-color:transparent;filter:drop-shadow(0 0 8px rgba(255,68,204,0.4))}
.top .sub{color:var(--muted);font-size:11px;margin-top:2px;position:relative;z-index:2}
.nav{display:flex;gap:8px;position:relative;z-index:2}
.nav a{color:var(--dim);text-decoration:none;font-size:10px;letter-spacing:1.5px;text-transform:uppercase;padding:5px 14px;border:1px solid var(--line);border-radius:6px;transition:all 0.2s}
.nav a:hover{color:var(--accent);border-color:rgba(255,68,204,0.3);box-shadow:0 0 10px var(--glow)}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:10px;margin-bottom:10px}.cards6{grid-template-columns:repeat(6,1fr)}
.card{border-radius:12px;padding:12px 16px;position:relative;overflow:hidden;background:linear-gradient(155deg,rgba(15,8,35,0.93),rgba(20,10,40,0.87));border:1px solid var(--line);box-shadow:0 0 14px var(--glow)}
.card .lbl{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--muted);margin-bottom:5px;position:relative;z-index:2}
.card .val{font-family:'Courier New',monospace;font-size:24px;font-weight:700;line-height:1;position:relative;z-index:2;background:linear-gradient(90deg,var(--accent),var(--accent2));-webkit-background-clip:text;-webkit-text-fill-color:transparent;filter:drop-shadow(0 0 6px rgba(255,68,204,0.4))}
.card .val.sm{font-size:14px;filter:drop-shadow(0 0 3px rgba(170,85,255,0.3))}
.stitle{font-size:9px;text-transform:uppercase;letter-spacing:3px;color:var(--dim);margin:18px 0 8px;padding-left:10px;border-left:2px solid var(--accent)}
table{width:100%;border-collapse:collapse;margin-bottom:8px}
th{font-size:9px;text-transform:uppercase;letter-spacing:1.5px;color:var(--dim);text-align:left;padding:6px 10px;border-bottom:1px solid var(--line);font-weight:400}
td{padding:5px 10px;border-bottom:1px solid rgba(255,68,204,0.04);color:var(--muted);font-size:12px;white-space:nowrap}
td.hi{color:var(--accent);text-shadow:0 0 4px rgba(255,68,204,0.3)}td.addr{max-width:170px;overflow:hidden;text-overflow:ellipsis;cursor:pointer;font-family:monospace;font-size:11px}td.addr:hover{color:var(--accent)}tr:hover td{background:rgba(255,68,204,0.02)}
.confirmed{color:var(--accent)}.pending{color:var(--accent2)}.orphaned{color:#993333}.luck-good{color:var(--accent)}.luck-mid{color:var(--accent2)}.luck-bad{color:#ff3344}
.foot{margin-top:16px;padding:10px 0;border-top:1px solid var(--line);font-size:9px;color:var(--dim);display:flex;gap:24px;text-transform:uppercase;letter-spacing:1.5px}.foot .on{color:var(--accent)}.foot .off{color:#ff3344}
@media(max-width:900px){.cards{grid-template-columns:1fr 1fr}.cards6{grid-template-columns:repeat(3,1fr)}}
</style></head><body><main>
<div class="top"><div><h1>TAZ Mining Pool</h1><div class="sub">Zcash Testnet &middot; Equihash(200,9) &middot; PPLNS</div></div><div class="nav"><a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a><a href="/zallet">Wallet</a><a href="/previews">Themes</a><a href="/">V1</a></div></div>
<div class="cards"><div class="card"><div class="lbl">Pool Hashrate</div><div class="val" id="s-hash">--</div></div><div class="card"><div class="lbl">Network</div><div class="val" id="s-net">--</div></div><div class="card"><div class="lbl">Blocks</div><div class="val" id="s-blocks">0</div></div><div class="card"><div class="lbl">Miners</div><div class="val" id="s-miners">0</div></div></div>
<div class="cards cards6"><div class="card"><div class="lbl">Shares</div><div class="val sm" id="s-shares">0</div></div><div class="card"><div class="lbl">Luck</div><div class="val sm" id="s-luck">--</div></div><div class="card"><div class="lbl">Net %</div><div class="val sm" id="s-pct">--</div></div><div class="card"><div class="lbl">Immature</div><div class="val sm" id="s-imm">0</div></div><div class="card"><div class="lbl">Fee</div><div class="val sm" id="s-fee">--</div></div><div class="card"><div class="lbl">Stratum</div><div class="val sm" id="s-port">--</div></div></div>
<div class="stitle">Miners</div><table id="t-miners"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>W</th><th>Shares</th><th>Pending</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dim)">loading...</td></tr></tbody></table>
<div class="stitle">Blocks</div><table id="t-blocks"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dim)">loading...</td></tr></tbody></table>
<div class="stitle">Payouts</div><table id="t-payouts"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead><tbody><tr><td colspan="4" style="color:var(--dim)">loading...</td></tr></tbody></table>
<div class="foot"><span>Node: <span id="f-node" class="on">--</span></span><span>Wallet: <span id="f-wallet" class="on">--</span></span><span>Up: <span id="f-up">--</span></span></div>
</main><script>
const S=Date.now();function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h '+m%60+'m':m>0?m+'m '+s%60+'s':s+'s'}function $(i){return document.getElementById(i)}
async function tick(){try{const d=await(await fetch('/api/pool/stats')).json();$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;if(d.luck_percent!=null){$('s-luck').textContent=d.luck_percent.toFixed(0)+'%';$('s-luck').className='val sm '+(d.luck_percent<=100?'luck-good':d.luck_percent<=150?'luck-mid':'luck-bad')}if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';$('f-node').textContent=d.node_ok?'OK':'DOWN';$('f-node').className=d.node_ok?'on':'off';$('f-wallet').textContent=d.wallet_ok?'OK':'DOWN';$('f-wallet').className=d.wallet_ok?'on':'off';$('f-up').textContent=fd(Date.now()-S)}catch(e){}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const t=document.querySelector('#t-miners tbody');if(!m.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=m.map(r=>'<tr><td class="addr hi" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td class="hi">'+r.pending_zec.toFixed(4)+' TAZ</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const t=document.querySelector('#t-blocks tbody');if(!b.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}return'<tr><td class="hi">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,12)+'</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const t=document.querySelector('#t-payouts tbody');if(!p.length){t.innerHTML='<tr><td colspan="4">none</td></tr>';return}t.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="hi">'+r.amount_zec.toFixed(8)+' TAZ</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,12):'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script></body></html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 9 — "Signal" — Military green, HUD/radar aesthetic
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW9: &str = r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>TAZ Mining Pool — Signal</title>
<style>
:root{--bg:#0a0e08;--accent:#88cc44;--accent2:#aadd66;--dim:#4a5a30;--dimmer:#2a3518;--ink:#c0d8a0;--muted:#6a7a50;--line:rgba(136,204,68,0.12)}
*{margin:0;padding:0;box-sizing:border-box}html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:'Courier New',monospace;font-size:12px;text-transform:uppercase;letter-spacing:0.5px;overflow-x:hidden}
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;background:radial-gradient(600px 600px at 50% 50%,rgba(136,204,68,0.04),transparent 60%)}
main{max-width:1200px;margin:0 auto;padding:16px}
.top{display:flex;justify-content:space-between;align-items:center;padding:10px 16px;margin-bottom:12px;border:1px solid var(--line);background:rgba(12,18,10,0.9)}
.top h1{font-size:14px;font-weight:700;color:var(--accent);letter-spacing:3px}
.top .sub{color:var(--dim);font-size:9px;margin-top:2px;letter-spacing:1px}
.nav{display:flex;gap:6px}
.nav a{color:var(--dimmer);text-decoration:none;font-size:9px;letter-spacing:1.5px;padding:4px 10px;border:1px solid var(--line);transition:all 0.2s}
.nav a:hover{color:var(--accent);border-color:rgba(136,204,68,0.3)}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:1px;background:rgba(136,204,68,0.06);border:1px solid var(--line);margin-bottom:1px}.cards6{grid-template-columns:repeat(6,1fr)}
.card{padding:10px 12px;background:rgba(12,18,10,0.9)}
.card .lbl{font-size:8px;letter-spacing:2px;color:var(--dim);margin-bottom:3px}
.card .val{font-size:20px;font-weight:700;color:var(--accent);line-height:1}.card .val.sm{font-size:12px;color:var(--accent2)}
.stitle{font-size:8px;letter-spacing:3px;color:var(--dim);margin:14px 0 6px;display:flex;align-items:center;gap:6px}.stitle::before{content:'>';color:var(--accent)}
table{width:100%;border-collapse:collapse;margin-bottom:6px;border:1px solid var(--line)}
th{font-size:8px;letter-spacing:2px;color:var(--dimmer);text-align:left;padding:5px 8px;border-bottom:1px solid var(--line);font-weight:400;background:rgba(12,18,10,0.5)}
td{padding:4px 8px;border-bottom:1px solid rgba(136,204,68,0.04);color:var(--muted);font-size:11px;white-space:nowrap}
td.hi{color:var(--accent)}td.addr{max-width:160px;overflow:hidden;text-overflow:ellipsis;cursor:pointer}td.addr:hover{color:var(--accent)}tr:hover td{background:rgba(136,204,68,0.03)}
.confirmed{color:var(--accent)}.pending{color:var(--accent2)}.orphaned{color:#884444}.luck-good{color:var(--accent)}.luck-mid{color:var(--accent2)}.luck-bad{color:#cc4444}
.foot{margin-top:12px;padding:8px 0;border-top:1px solid var(--line);font-size:8px;color:var(--dimmer);display:flex;gap:20px;letter-spacing:2px}.foot .on{color:var(--accent)}.foot .off{color:#cc4444}
@media(max-width:800px){.cards{grid-template-columns:1fr 1fr}.cards6{grid-template-columns:repeat(3,1fr)}}
</style></head><body><main>
<div class="top"><div><h1>TAZ Pool</h1><div class="sub">Zcash Testnet // Equihash(200,9) // PPLNS</div></div><div class="nav"><a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a><a href="/zallet">Wallet</a><a href="/previews">Themes</a><a href="/">V1</a></div></div>
<div class="cards"><div class="card"><div class="lbl">Pool Hash</div><div class="val" id="s-hash">--</div></div><div class="card"><div class="lbl">Net Hash</div><div class="val" id="s-net">--</div></div><div class="card"><div class="lbl">Blocks</div><div class="val" id="s-blocks">0</div></div><div class="card"><div class="lbl">Miners</div><div class="val" id="s-miners">0</div></div></div>
<div class="cards cards6"><div class="card"><div class="lbl">Shares</div><div class="val sm" id="s-shares">0</div></div><div class="card"><div class="lbl">Luck</div><div class="val sm" id="s-luck">--</div></div><div class="card"><div class="lbl">Net %</div><div class="val sm" id="s-pct">--</div></div><div class="card"><div class="lbl">Immature</div><div class="val sm" id="s-imm">0</div></div><div class="card"><div class="lbl">Fee</div><div class="val sm" id="s-fee">--</div></div><div class="card"><div class="lbl">Port</div><div class="val sm" id="s-port">--</div></div></div>
<div class="stitle">Miners</div><table id="t-miners"><thead><tr><th>Addr</th><th>1m</th><th>10m</th><th>W</th><th>Shares</th><th>Pend</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dimmer)">standby...</td></tr></tbody></table>
<div class="stitle">Blocks</div><table id="t-blocks"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Time</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dimmer)">standby...</td></tr></tbody></table>
<div class="stitle">Payouts</div><table id="t-payouts"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead><tbody><tr><td colspan="4" style="color:var(--dimmer)">standby...</td></tr></tbody></table>
<div class="foot"><span>Node: <span id="f-node" class="on">--</span></span><span>Wallet: <span id="f-wallet" class="on">--</span></span><span>Up: <span id="f-up">--</span></span></div>
</main><script>
const S=Date.now();function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' T';if(h>=1e9)return(h/1e9).toFixed(2)+' G';if(h>=1e6)return(h/1e6).toFixed(2)+' M';if(h>=1e3)return(h/1e3).toFixed(2)+' K';return h.toFixed(0)}function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h'+m%60+'m':m>0?m+'m'+s%60+'s':s+'s'}function $(i){return document.getElementById(i)}
async function tick(){try{const d=await(await fetch('/api/pool/stats')).json();$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;if(d.luck_percent!=null){$('s-luck').textContent=d.luck_percent.toFixed(0)+'%';$('s-luck').className='val sm '+(d.luck_percent<=100?'luck-good':d.luck_percent<=150?'luck-mid':'luck-bad')}if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';$('f-node').textContent=d.node_ok?'OK':'DOWN';$('f-node').className=d.node_ok?'on':'off';$('f-wallet').textContent=d.wallet_ok?'OK':'DOWN';$('f-wallet').className=d.wallet_ok?'on':'off';$('f-up').textContent=fd(Date.now()-S)}catch(e){}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const t=document.querySelector('#t-miners tbody');if(!m.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=m.map(r=>'<tr><td class="addr hi" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td class="hi">'+r.pending_zec.toFixed(4)+'</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const t=document.querySelector('#t-blocks tbody');if(!b.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}return'<tr><td class="hi">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,10)+'</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const t=document.querySelector('#t-payouts tbody');if(!p.length){t.innerHTML='<tr><td colspan="4">none</td></tr>';return}t.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="hi">'+r.amount_zec.toFixed(8)+'</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,10):'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script></body></html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 10 — "Frost" — Ice white/blue, glass morphism
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW10: &str = r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>TAZ Mining Pool — Frost</title>
<style>
:root{--bg:#0a1420;--glass:rgba(16,28,48,0.5);--accent:#88ccff;--accent2:#aaddff;--ink:#c0d4e8;--muted:#6080a0;--dim:#304060;--dimmer:#1a2840;--line:rgba(136,204,255,0.1)}
*{margin:0;padding:0;box-sizing:border-box}html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:-apple-system,BlinkMacSystemFont,'Inter','Segoe UI',sans-serif;font-size:13px;overflow-x:hidden}
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;background:radial-gradient(1000px 500px at 50% -10%,rgba(136,204,255,0.06),transparent 55%),radial-gradient(800px 400px at 50% 110%,rgba(100,160,255,0.04),transparent 50%)}
main{max-width:1200px;margin:0 auto;padding:28px}
.top{display:flex;justify-content:space-between;align-items:center;padding:18px 24px;border-radius:16px;margin-bottom:18px;background:var(--glass);backdrop-filter:blur(24px);-webkit-backdrop-filter:blur(24px);border:1px solid var(--line);box-shadow:0 8px 40px rgba(0,0,0,0.25),inset 0 1px 0 rgba(255,255,255,0.05)}
.top h1{font-size:18px;font-weight:600;color:var(--accent2);letter-spacing:2px}
.top .sub{color:var(--muted);font-size:11px;margin-top:3px}
.nav{display:flex;gap:10px}
.nav a{color:var(--dim);text-decoration:none;font-size:10px;letter-spacing:1.5px;text-transform:uppercase;padding:6px 16px;border-radius:10px;border:1px solid var(--line);background:rgba(16,28,48,0.3);backdrop-filter:blur(8px);transition:all 0.2s}
.nav a:hover{color:var(--accent);border-color:rgba(136,204,255,0.25);box-shadow:0 0 12px rgba(136,204,255,0.06)}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:14px;margin-bottom:14px}.cards6{grid-template-columns:repeat(6,1fr)}
.card{border-radius:14px;padding:16px 18px;background:var(--glass);backdrop-filter:blur(16px);-webkit-backdrop-filter:blur(16px);border:1px solid var(--line);box-shadow:0 4px 24px rgba(0,0,0,0.15),inset 0 1px 0 rgba(255,255,255,0.03)}
.card .lbl{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--muted);margin-bottom:6px}
.card .val{font-family:'SF Mono','Fira Code',monospace;font-size:26px;font-weight:300;color:var(--accent2);line-height:1}.card .val.sm{font-size:14px;color:var(--accent)}
.stitle{font-size:9px;text-transform:uppercase;letter-spacing:3px;color:var(--dim);margin:22px 0 10px}
table{width:100%;border-collapse:separate;border-spacing:0;margin-bottom:14px;border-radius:12px;overflow:hidden;border:1px solid var(--line);background:var(--glass);backdrop-filter:blur(12px)}
th{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--dim);text-align:left;padding:8px 14px;border-bottom:1px solid var(--line);font-weight:400;background:rgba(16,28,48,0.3)}
td{padding:7px 14px;border-bottom:1px solid rgba(136,204,255,0.04);color:var(--muted);font-size:12px;white-space:nowrap}
td.hi{color:var(--accent2)}td.accent{color:var(--accent)}td.addr{max-width:180px;overflow:hidden;text-overflow:ellipsis;cursor:pointer;font-family:monospace;font-size:11px}td.addr:hover{color:var(--accent)}tr:hover td{background:rgba(136,204,255,0.03)}
.confirmed{color:var(--accent)}.pending{color:#88aacc}.orphaned{color:#886666}.luck-good{color:var(--accent)}.luck-mid{color:#ccaa66}.luck-bad{color:#cc6666}
.foot{margin-top:20px;padding:12px 18px;border-radius:12px;font-size:9px;color:var(--dim);display:flex;gap:24px;text-transform:uppercase;letter-spacing:2px;background:var(--glass);border:1px solid var(--line)}.foot .on{color:var(--accent)}.foot .off{color:#cc6666}
@media(max-width:1000px){.cards{grid-template-columns:1fr 1fr}.cards6{grid-template-columns:repeat(3,1fr)}}@media(max-width:600px){.cards{grid-template-columns:1fr}.cards6{grid-template-columns:1fr 1fr}}
</style></head><body><main>
<div class="top"><div><h1>TAZ Mining Pool</h1><div class="sub">Zcash Testnet &middot; Equihash(200,9) &middot; PPLNS</div></div><div class="nav"><a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a><a href="/zallet">Wallet</a><a href="/previews">Themes</a><a href="/">V1</a></div></div>
<div class="cards"><div class="card"><div class="lbl">Pool Hashrate</div><div class="val" id="s-hash">--</div></div><div class="card"><div class="lbl">Network</div><div class="val" id="s-net">--</div></div><div class="card"><div class="lbl">Blocks</div><div class="val" id="s-blocks">0</div></div><div class="card"><div class="lbl">Miners</div><div class="val" id="s-miners">0</div></div></div>
<div class="cards cards6"><div class="card"><div class="lbl">Shares</div><div class="val sm" id="s-shares">0</div></div><div class="card"><div class="lbl">Luck 24h</div><div class="val sm" id="s-luck">--</div></div><div class="card"><div class="lbl">Net Share</div><div class="val sm" id="s-pct">--</div></div><div class="card"><div class="lbl">Immature</div><div class="val sm" id="s-imm">0</div></div><div class="card"><div class="lbl">Fee</div><div class="val sm" id="s-fee">--</div></div><div class="card"><div class="lbl">Stratum</div><div class="val sm" id="s-port">--</div></div></div>
<div class="stitle">Miners</div><table id="t-miners"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>W</th><th>Shares</th><th>Pending</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dim)">loading...</td></tr></tbody></table>
<div class="stitle">Blocks</div><table id="t-blocks"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dim)">loading...</td></tr></tbody></table>
<div class="stitle">Payouts</div><table id="t-payouts"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead><tbody><tr><td colspan="4" style="color:var(--dim)">loading...</td></tr></tbody></table>
<div class="foot"><span>Node: <span id="f-node" class="on">--</span></span><span>Wallet: <span id="f-wallet" class="on">--</span></span><span>Up: <span id="f-up">--</span></span></div>
</main><script>
const S=Date.now();function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h '+m%60+'m':m>0?m+'m '+s%60+'s':s+'s'}function $(i){return document.getElementById(i)}
async function tick(){try{const d=await(await fetch('/api/pool/stats')).json();$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;if(d.luck_percent!=null)$('s-luck').textContent=d.luck_percent.toFixed(0)+'%';if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';$('f-node').textContent=d.node_ok?'OK':'DOWN';$('f-node').className=d.node_ok?'on':'off';$('f-wallet').textContent=d.wallet_ok?'OK':'DOWN';$('f-wallet').className=d.wallet_ok?'on':'off';$('f-up').textContent=fd(Date.now()-S)}catch(e){}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const t=document.querySelector('#t-miners tbody');if(!m.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=m.map(r=>'<tr><td class="addr hi" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td class="accent">'+r.pending_zec.toFixed(4)+' TAZ</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const t=document.querySelector('#t-blocks tbody');if(!b.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}return'<tr><td class="accent">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,12)+'</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const t=document.querySelector('#t-payouts tbody');if(!p.length){t.innerHTML='<tr><td colspan="4">none</td></tr>';return}t.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="accent">'+r.amount_zec.toFixed(8)+' TAZ</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,12):'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script></body></html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// GALLERY — Preview selector page with live iframe thumbnails
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const GALLERY: &str = r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>TAZ Mining Pool — Theme Gallery</title>
<style>
:root{--bg:#040510;--panel:rgba(6,10,28,0.8);--accent:#00e5ff;--accent2:#ff2bd6;--ink:#e0e8f0;--muted:#5a7a98;--dim:#2a3a50;--line:rgba(0,229,255,0.12)}
*{margin:0;padding:0;box-sizing:border-box}
html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:'Segoe UI','Helvetica Neue',sans-serif;font-size:14px;overflow-x:hidden}
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;
  background:radial-gradient(900px 450px at 30% -5%,rgba(0,229,255,0.06),transparent 55%),
             radial-gradient(700px 350px at 70% 105%,rgba(255,43,214,0.05),transparent 50%)}
main{max-width:1400px;margin:0 auto;padding:28px}
.back{display:inline-block;color:var(--muted);text-decoration:none;font-size:11px;margin-bottom:16px;
  text-transform:uppercase;letter-spacing:1.5px;transition:color 0.2s}
.back:hover{color:var(--accent)}
h1{font-size:24px;font-weight:600;color:#f0f8ff;text-align:center;margin-bottom:6px;
  text-shadow:0 0 12px rgba(0,229,255,0.3)}
.subtitle{text-align:center;color:var(--muted);font-size:13px;margin-bottom:28px}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(380px,1fr));gap:20px}
.thumb{display:block;position:relative;border-radius:14px;overflow:hidden;text-decoration:none;
  background:var(--panel);border:1px solid var(--line);
  box-shadow:0 4px 24px rgba(0,0,0,0.3);transition:transform 0.2s,box-shadow 0.2s;cursor:pointer}
.thumb:hover{transform:translateY(-4px);box-shadow:0 8px 36px rgba(0,229,255,0.12),0 4px 24px rgba(0,0,0,0.4)}
.iframe-wrap{width:100%;height:280px;overflow:hidden;border-radius:14px 14px 0 0;position:relative}
.iframe-wrap iframe{border:none;pointer-events:none;
  transform:scale(0.5);transform-origin:0 0;width:200%;height:560px}
.thumb-info{padding:12px 16px;display:flex;justify-content:space-between;align-items:center;
  border-top:1px solid var(--line);background:rgba(4,8,20,0.6)}
.thumb-name{font-size:14px;font-weight:600;color:#c0d8f0}
.thumb-num{font-size:11px;color:var(--dim);text-transform:uppercase;letter-spacing:2px}
.thumb-desc{font-size:11px;color:var(--muted);margin-top:2px}
@media(max-width:800px){.grid{grid-template-columns:1fr}}
</style></head><body><main>
<a href="/" class="back">&larr; Back to Dashboard</a>
<h1>Theme Gallery</h1>
<p class="subtitle">Click any theme to view it full-screen. All themes show live pool data.</p>
<div class="grid">
  <a href="/preview1" class="thumb"><div class="iframe-wrap"><iframe src="/preview1" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Phosphor</div><div class="thumb-desc">VFD green neon, scanlines, terminal glow</div></div><div class="thumb-num">#1</div></div></a>
  <a href="/preview2" class="thumb"><div class="iframe-wrap"><iframe src="/preview2" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Neon Pulse</div><div class="thumb-desc">Cyan/magenta cyberpunk, starfield, gradient borders</div></div><div class="thumb-num">#2</div></div></a>
  <a href="/preview3" class="thumb"><div class="iframe-wrap"><iframe src="/preview3" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Cipher Gold</div><div class="thumb-desc">Warm amber/gold neon on obsidian, vault aesthetic</div></div><div class="thumb-num">#3</div></div></a>
  <a href="/preview4" class="thumb"><div class="iframe-wrap"><iframe src="/preview4" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Ghost Protocol</div><div class="thumb-desc">Ultra-minimal, ghostly cyan whispers, spacious</div></div><div class="thumb-num">#4</div></div></a>
  <a href="/preview5" class="thumb"><div class="iframe-wrap"><iframe src="/preview5" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Plasma</div><div class="thumb-desc">Multi-color neon, animated gradient borders, HUD grid</div></div><div class="thumb-num">#5</div></div></a>
  <a href="/preview6" class="thumb"><div class="iframe-wrap"><iframe src="/preview6" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Hologram</div><div class="thumb-desc">Electric blue, frosted glass panels, depth layers</div></div><div class="thumb-num">#6</div></div></a>
  <a href="/preview7" class="thumb"><div class="iframe-wrap"><iframe src="/preview7" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Ember</div><div class="thumb-desc">Deep red/orange volcanic, glowing coals</div></div><div class="thumb-num">#7</div></div></a>
  <a href="/preview8" class="thumb"><div class="iframe-wrap"><iframe src="/preview8" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Synthwave</div><div class="thumb-desc">80s retro sunset, purple/pink, grid horizon</div></div><div class="thumb-num">#8</div></div></a>
  <a href="/preview9" class="thumb"><div class="iframe-wrap"><iframe src="/preview9" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Signal</div><div class="thumb-desc">Military green monochrome, HUD/radar aesthetic</div></div><div class="thumb-num">#9</div></div></a>
  <a href="/preview10" class="thumb"><div class="iframe-wrap"><iframe src="/preview10" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Frost</div><div class="thumb-desc">Ice blue glass morphism, clinical precision</div></div><div class="thumb-num">#10</div></div></a>
  <a href="/preview11" class="thumb"><div class="iframe-wrap"><iframe src="/preview11" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Obsidian</div><div class="thumb-desc">Pure monochrome, extreme minimalism, no color</div></div><div class="thumb-num">#11</div></div></a>
  <a href="/preview12" class="thumb"><div class="iframe-wrap"><iframe src="/preview12" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Aurora</div><div class="thumb-desc">Northern lights teal/green/purple shimmer</div></div><div class="thumb-num">#12</div></div></a>
  <a href="/preview13" class="thumb"><div class="iframe-wrap"><iframe src="/preview13" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Zcash</div><div class="thumb-desc">Official brand gold + electric blue, Z logo</div></div><div class="thumb-num">#13</div></div></a>
  <a href="/preview14" class="thumb"><div class="iframe-wrap"><iframe src="/preview14" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Midnight</div><div class="thumb-desc">Deep navy, refined corporate, subtle blue accents</div></div><div class="thumb-num">#14</div></div></a>
  <a href="/preview15" class="thumb"><div class="iframe-wrap"><iframe src="/preview15" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Toxic</div><div class="thumb-desc">Radioactive green/yellow, bold, high contrast</div></div><div class="thumb-num">#15</div></div></a>
  <a href="/preview16" class="thumb"><div class="iframe-wrap"><iframe src="/preview16" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Matrix</div><div class="thumb-desc">Falling code rain, digital rain canvas effect</div></div><div class="thumb-num">#16</div></div></a>
  <a href="/preview17" class="thumb"><div class="iframe-wrap"><iframe src="/preview17" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Vapor</div><div class="thumb-desc">Vaporwave pink/cyan, retro aesthetic</div></div><div class="thumb-num">#17</div></div></a>
  <a href="/preview18" class="thumb"><div class="iframe-wrap"><iframe src="/preview18" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Blueprint</div><div class="thumb-desc">Technical drawing, CAD-style blue lines on dark</div></div><div class="thumb-num">#18</div></div></a>
  <a href="/preview19" class="thumb"><div class="iframe-wrap"><iframe src="/preview19" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Solarpunk</div><div class="thumb-desc">Warm olive/green, organic nature tech</div></div><div class="thumb-num">#19</div></div></a>
  <a href="/preview20" class="thumb"><div class="iframe-wrap"><iframe src="/preview20" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Crimson</div><div class="thumb-desc">Deep red/black, luxurious dark premium feel</div></div><div class="thumb-num">#20</div></div></a>
  <a href="/preview21" class="thumb" style="border-color:rgba(0,229,255,0.6);box-shadow:0 0 20px rgba(0,229,255,0.3)"><div class="iframe-wrap"><iframe src="/preview21" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Tempest</div><div class="thumb-desc">Premium: Canvas gauges, sparklines, VFD glow, LEDs</div></div><div class="thumb-num">#21</div></div></a>
  <a href="/preview22" class="thumb" style="border-color:rgba(244,183,40,0.6);box-shadow:0 0 20px rgba(244,183,40,0.3)"><div class="iframe-wrap"><iframe src="/preview22" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Nexus</div><div class="thumb-desc">Premium: Ring gauges, gold/amber warm glow</div></div><div class="thumb-num">#22</div></div></a>
  <a href="/preview23" class="thumb" style="border-color:rgba(0,255,65,0.6);box-shadow:0 0 20px rgba(0,255,65,0.3)"><div class="iframe-wrap"><iframe src="/preview23" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Cipher</div><div class="thumb-desc">Premium: Hacker terminal, bar gauges, scanlines</div></div><div class="thumb-num">#23</div></div></a>
  <a href="/preview24" class="thumb" style="border-color:rgba(99,102,241,0.6);box-shadow:0 0 20px rgba(99,102,241,0.3)"><div class="iframe-wrap"><iframe src="/preview24" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Radar</div><div class="thumb-desc">Premium: Donut gauges, clean modern, indigo accent</div></div><div class="thumb-num">#24</div></div></a>
  <a href="/preview25" class="thumb" style="border-color:rgba(168,85,247,0.6);box-shadow:0 0 20px rgba(168,85,247,0.3)"><div class="iframe-wrap"><iframe src="/preview25" loading="lazy"></iframe></div><div class="thumb-info"><div><div class="thumb-name">Plasma</div><div class="thumb-desc">Premium: Glassmorphism, gradient text, purple glow</div></div><div class="thumb-num">#25</div></div></a>
</div>
</main></body></html>
"##;
