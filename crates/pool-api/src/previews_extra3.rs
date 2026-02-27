// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 16 — "Matrix" — Falling green code rain, digital rain effect
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW16: &str = r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>TAZ Mining Pool — Matrix</title>
<style>
:root{--bg:#000800;--accent:#00ff41;--accent2:#00cc33;--ink:#c0ffc8;--muted:#408850;--dim:#205030;--dimmer:#102818;--line:rgba(0,255,65,0.1);--glow:rgba(0,255,65,0.1)}
*{margin:0;padding:0;box-sizing:border-box}html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:'Courier New','Lucida Console',monospace;font-size:13px;overflow-x:hidden}
#rain-canvas{position:fixed;inset:0;z-index:0;pointer-events:none;opacity:0.08}
body::after{content:'';position:fixed;inset:0;pointer-events:none;z-index:9999;
  background:repeating-linear-gradient(0deg,rgba(0,0,0,0.06) 0px,rgba(0,0,0,0.06) 1px,transparent 1px,transparent 2px);opacity:0.7}
main{max-width:1300px;margin:0 auto;padding:20px;position:relative;z-index:2}
.top{display:flex;justify-content:space-between;align-items:center;padding:14px 18px;margin-bottom:14px;
  background:rgba(0,12,4,0.92);border:1px solid var(--line);box-shadow:0 0 20px var(--glow)}
.top h1{font-size:18px;font-weight:700;color:var(--accent);letter-spacing:3px;text-transform:uppercase;
  text-shadow:0 0 8px rgba(0,255,65,0.6),0 0 20px rgba(0,255,65,0.3)}
.top .sub{color:var(--dim);font-size:10px;margin-top:2px;letter-spacing:1px}
.nav{display:flex;gap:8px}
.nav a{color:var(--dim);text-decoration:none;font-size:10px;letter-spacing:1.5px;text-transform:uppercase;
  padding:4px 12px;border:1px solid var(--line);transition:all 0.2s}
.nav a:hover{color:var(--accent);border-color:rgba(0,255,65,0.3);text-shadow:0 0 6px rgba(0,255,65,0.4)}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:2px;margin-bottom:2px}.cards6{grid-template-columns:repeat(6,1fr)}
.card{padding:12px 14px;background:rgba(0,12,4,0.9);border:1px solid var(--line)}
.card .lbl{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--dim);margin-bottom:4px}
.card .val{font-size:24px;font-weight:700;color:var(--accent);line-height:1;
  text-shadow:0 0 6px rgba(0,255,65,0.5),0 0 14px rgba(0,255,65,0.3)}
.card .val.sm{font-size:14px;color:var(--accent2);text-shadow:0 0 4px rgba(0,204,51,0.3)}
.stitle{font-size:9px;text-transform:uppercase;letter-spacing:3px;color:var(--dim);margin:16px 0 8px;padding-left:10px;border-left:2px solid var(--accent)}
table{width:100%;border-collapse:collapse;margin-bottom:8px;border:1px solid var(--line)}
th{font-size:9px;text-transform:uppercase;letter-spacing:1.5px;color:var(--dimmer);text-align:left;padding:6px 10px;border-bottom:1px solid var(--line);font-weight:400;background:rgba(0,12,4,0.5)}
td{padding:5px 10px;border-bottom:1px solid rgba(0,255,65,0.04);color:var(--muted);font-size:12px;white-space:nowrap}
td.hi{color:var(--accent);text-shadow:0 0 4px rgba(0,255,65,0.3)}td.addr{max-width:170px;overflow:hidden;text-overflow:ellipsis;cursor:pointer}td.addr:hover{color:var(--accent)}tr:hover td{background:rgba(0,255,65,0.02)}
.confirmed{color:var(--accent)}.pending{color:var(--accent2)}.orphaned{color:#553333}
.luck-good{color:var(--accent)}.luck-mid{color:var(--accent2)}.luck-bad{color:#ff3333}
.foot{margin-top:14px;padding:8px 14px;border:1px solid var(--line);font-size:9px;color:var(--dimmer);display:flex;gap:24px;text-transform:uppercase;letter-spacing:1.5px;background:rgba(0,12,4,0.7)}
.foot .on{color:var(--accent);text-shadow:0 0 4px rgba(0,255,65,0.4)}.foot .off{color:#ff3333}
@media(max-width:900px){.cards{grid-template-columns:1fr 1fr}.cards6{grid-template-columns:repeat(3,1fr)}}
</style></head><body>
<canvas id="rain-canvas"></canvas>
<main>
<div class="top"><div><h1>TAZ Pool</h1><div class="sub">Zcash Testnet &middot; Equihash(200,9) &middot; PPLNS</div></div>
<div class="nav"><a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a><a href="/zallet">Wallet</a><a href="/previews">Themes</a><a href="/">V1</a></div></div>
<div class="cards"><div class="card"><div class="lbl">Pool Hashrate</div><div class="val" id="s-hash">--</div></div><div class="card"><div class="lbl">Network</div><div class="val" id="s-net">--</div></div><div class="card"><div class="lbl">Blocks</div><div class="val" id="s-blocks">0</div></div><div class="card"><div class="lbl">Miners</div><div class="val" id="s-miners">0</div></div></div>
<div class="cards cards6"><div class="card"><div class="lbl">Shares</div><div class="val sm" id="s-shares">0</div></div><div class="card"><div class="lbl">Luck</div><div class="val sm" id="s-luck">--</div></div><div class="card"><div class="lbl">Net %</div><div class="val sm" id="s-pct">--</div></div><div class="card"><div class="lbl">Immature</div><div class="val sm" id="s-imm">0</div></div><div class="card"><div class="lbl">Fee</div><div class="val sm" id="s-fee">--</div></div><div class="card"><div class="lbl">Stratum</div><div class="val sm" id="s-port">--</div></div></div>
<div class="stitle">Miners</div><table id="t-miners"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>W</th><th>Shares</th><th>Pending</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="stitle">Blocks</div><table id="t-blocks"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="stitle">Payouts</div><table id="t-payouts"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead><tbody><tr><td colspan="4" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="foot"><span>Node: <span id="f-node" class="on">--</span></span><span>Wallet: <span id="f-wallet" class="on">--</span></span><span>Up: <span id="f-up">--</span></span></div>
</main><script>
// Matrix rain effect
const canvas=document.getElementById('rain-canvas'),ctx=canvas.getContext('2d');
function resize(){canvas.width=window.innerWidth;canvas.height=window.innerHeight}
resize();window.addEventListener('resize',resize);
const chars='01アイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモ';
const fontSize=14,columns=Math.floor(canvas.width/fontSize);
const drops=Array(columns).fill(1);
function drawRain(){ctx.fillStyle='rgba(0,8,0,0.05)';ctx.fillRect(0,0,canvas.width,canvas.height);
ctx.fillStyle='#00ff41';ctx.font=fontSize+'px monospace';
for(let i=0;i<drops.length;i++){const t=chars[Math.floor(Math.random()*chars.length)];
ctx.fillText(t,i*fontSize,drops[i]*fontSize);if(drops[i]*fontSize>canvas.height&&Math.random()>0.975)drops[i]=0;drops[i]++}}
setInterval(drawRain,50);
// Pool data
const S=Date.now();function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h '+m%60+'m':m>0?m+'m '+s%60+'s':s+'s'}function $(i){return document.getElementById(i)}
async function tick(){try{const d=await(await fetch('/api/pool/stats')).json();$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;if(d.luck_percent!=null){$('s-luck').textContent=d.luck_percent.toFixed(0)+'%';$('s-luck').className='val sm '+(d.luck_percent<=100?'luck-good':d.luck_percent<=150?'luck-mid':'luck-bad')}if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';$('f-node').textContent=d.node_ok?'OK':'DOWN';$('f-node').className=d.node_ok?'on':'off';$('f-wallet').textContent=d.wallet_ok?'OK':'DOWN';$('f-wallet').className=d.wallet_ok?'on':'off';$('f-up').textContent=fd(Date.now()-S)}catch(e){}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const t=document.querySelector('#t-miners tbody');if(!m.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=m.map(r=>'<tr><td class="addr hi" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td class="hi">'+r.pending_zec.toFixed(4)+' TAZ</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const t=document.querySelector('#t-blocks tbody');if(!b.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}return'<tr><td class="hi">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,12)+'</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const t=document.querySelector('#t-payouts tbody');if(!p.length){t.innerHTML='<tr><td colspan="4">none</td></tr>';return}t.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="hi">'+r.amount_zec.toFixed(8)+' TAZ</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,12):'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script></body></html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 17 — "Vapor" — Vaporwave pink/cyan, retro aesthetic, smooth gradients
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW17: &str = r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>TAZ Mining Pool — Vapor</title>
<style>
:root{--bg:#1a0a2e;--accent:#ff71ce;--accent2:#01cdfe;--accent3:#fffb96;--ink:#e8d8f0;--muted:#8a6aa0;--dim:#5a3a70;--dimmer:#3a1a50;--line:rgba(255,113,206,0.12)}
*{margin:0;padding:0;box-sizing:border-box}html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:'Segoe UI','Helvetica Neue',sans-serif;font-size:13px;overflow-x:hidden}
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;
  background:linear-gradient(180deg,rgba(26,10,46,1) 0%,rgba(20,10,50,1) 40%,rgba(10,20,60,0.9) 80%,rgba(5,15,40,1) 100%)}
body::after{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;
  background:radial-gradient(ellipse 900px 350px at 30% -5%,rgba(255,113,206,0.08),transparent 50%),
             radial-gradient(ellipse 700px 300px at 70% 110%,rgba(1,205,254,0.07),transparent 50%)}
main{max-width:1300px;margin:0 auto;padding:24px}
.top{display:flex;justify-content:space-between;align-items:center;padding:16px 22px;border-radius:16px;margin-bottom:16px;
  background:rgba(26,12,50,0.8);border:1px solid var(--line);
  box-shadow:0 4px 24px rgba(255,113,206,0.06);position:relative;overflow:hidden}
.top::before{content:'';position:absolute;inset:0;border-radius:inherit;padding:1.5px;
  background:linear-gradient(135deg,rgba(255,113,206,0.5),rgba(1,205,254,0.4),rgba(255,251,150,0.3));
  opacity:0.2;pointer-events:none;-webkit-mask:linear-gradient(#000 0 0) content-box,linear-gradient(#000 0 0);-webkit-mask-composite:xor;mask-composite:exclude}
.top h1{font-size:22px;font-weight:600;color:var(--accent);letter-spacing:2px;position:relative;z-index:2;
  text-shadow:0 0 10px rgba(255,113,206,0.4)}
.top .sub{color:var(--muted);font-size:11px;margin-top:2px;position:relative;z-index:2}
.nav{display:flex;gap:8px;position:relative;z-index:2}
.nav a{color:var(--dim);text-decoration:none;font-size:10px;letter-spacing:1.5px;text-transform:uppercase;
  padding:6px 14px;border:1px solid var(--line);border-radius:8px;transition:all 0.2s}
.nav a:hover{color:var(--accent);border-color:rgba(255,113,206,0.3)}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:12px;margin-bottom:12px}.cards6{grid-template-columns:repeat(6,1fr)}
.card{border-radius:14px;padding:14px 16px;background:rgba(26,12,50,0.75);border:1px solid var(--line);
  box-shadow:0 4px 20px rgba(0,0,0,0.2);position:relative;overflow:hidden}
.card .lbl{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--muted);margin-bottom:5px}
.card .val{font-family:'Courier New',monospace;font-size:24px;font-weight:600;color:var(--accent);line-height:1;
  text-shadow:0 0 6px rgba(255,113,206,0.4)}
.card .val.sm{font-size:14px;color:var(--accent2);text-shadow:0 0 4px rgba(1,205,254,0.3)}
.card.cyan .val{color:var(--accent2);text-shadow:0 0 6px rgba(1,205,254,0.4)}
.stitle{font-size:9px;text-transform:uppercase;letter-spacing:3px;color:var(--dim);margin:18px 0 8px;padding-left:10px;
  border-left:2px solid var(--accent)}
table{width:100%;border-collapse:separate;border-spacing:0;margin-bottom:10px;border-radius:10px;overflow:hidden;
  background:rgba(26,12,50,0.6);border:1px solid var(--line)}
th{font-size:9px;text-transform:uppercase;letter-spacing:1.5px;color:var(--dimmer);text-align:left;
  padding:7px 12px;border-bottom:1px solid var(--line);font-weight:400;background:rgba(26,12,50,0.4)}
td{padding:5px 12px;border-bottom:1px solid rgba(255,113,206,0.05);color:var(--muted);font-size:12px;white-space:nowrap}
td.hi{color:var(--accent);text-shadow:0 0 3px rgba(255,113,206,0.3)}
td.cyan{color:var(--accent2)}
td.addr{max-width:170px;overflow:hidden;text-overflow:ellipsis;cursor:pointer;font-family:monospace;font-size:11px}
td.addr:hover{color:var(--accent)}tr:hover td{background:rgba(255,113,206,0.02)}
.confirmed{color:var(--accent2)}.pending{color:var(--accent3)}.orphaned{color:#886644}
.luck-good{color:var(--accent2)}.luck-mid{color:var(--accent3)}.luck-bad{color:#ff5555}
.foot{margin-top:18px;padding:10px 16px;border-radius:10px;font-size:9px;color:var(--dim);display:flex;gap:24px;text-transform:uppercase;letter-spacing:1.5px;background:rgba(26,12,50,0.5);border:1px solid var(--line)}
.foot .on{color:var(--accent2)}.foot .off{color:#ff5555}
@media(max-width:900px){.cards{grid-template-columns:1fr 1fr}.cards6{grid-template-columns:repeat(3,1fr)}}
</style></head><body><main>
<div class="top"><div><h1>TAZ Mining Pool</h1><div class="sub">Zcash Testnet &middot; Equihash(200,9) &middot; PPLNS</div></div>
<div class="nav"><a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a><a href="/zallet">Wallet</a><a href="/previews">Themes</a><a href="/">V1</a></div></div>
<div class="cards"><div class="card"><div class="lbl">Pool Hashrate</div><div class="val" id="s-hash">--</div></div><div class="card cyan"><div class="lbl">Network</div><div class="val" id="s-net">--</div></div><div class="card"><div class="lbl">Blocks</div><div class="val" id="s-blocks">0</div></div><div class="card cyan"><div class="lbl">Miners</div><div class="val" id="s-miners">0</div></div></div>
<div class="cards cards6"><div class="card"><div class="lbl">Shares</div><div class="val sm" id="s-shares">0</div></div><div class="card"><div class="lbl">Luck 24h</div><div class="val sm" id="s-luck">--</div></div><div class="card"><div class="lbl">Net Share</div><div class="val sm" id="s-pct">--</div></div><div class="card"><div class="lbl">Immature</div><div class="val sm" id="s-imm">0</div></div><div class="card"><div class="lbl">Fee</div><div class="val sm" id="s-fee">--</div></div><div class="card"><div class="lbl">Stratum</div><div class="val sm" id="s-port">--</div></div></div>
<div class="stitle">Miners</div><table id="t-miners"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>W</th><th>Shares</th><th>Pending</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="stitle">Blocks</div><table id="t-blocks"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="stitle">Payouts</div><table id="t-payouts"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead><tbody><tr><td colspan="4" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="foot"><span>Node: <span id="f-node" class="on">--</span></span><span>Wallet: <span id="f-wallet" class="on">--</span></span><span>Up: <span id="f-up">--</span></span></div>
</main><script>
const S=Date.now();function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h '+m%60+'m':m>0?m+'m '+s%60+'s':s+'s'}function $(i){return document.getElementById(i)}
async function tick(){try{const d=await(await fetch('/api/pool/stats')).json();$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;if(d.luck_percent!=null){$('s-luck').textContent=d.luck_percent.toFixed(0)+'%';$('s-luck').className='val sm '+(d.luck_percent<=100?'luck-good':d.luck_percent<=150?'luck-mid':'luck-bad')}if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';$('f-node').textContent=d.node_ok?'OK':'DOWN';$('f-node').className=d.node_ok?'on':'off';$('f-wallet').textContent=d.wallet_ok?'OK':'DOWN';$('f-wallet').className=d.wallet_ok?'on':'off';$('f-up').textContent=fd(Date.now()-S)}catch(e){}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const t=document.querySelector('#t-miners tbody');if(!m.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=m.map(r=>'<tr><td class="addr hi" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td class="hi">'+r.pending_zec.toFixed(4)+' TAZ</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const t=document.querySelector('#t-blocks tbody');if(!b.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}return'<tr><td class="hi">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,12)+'</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const t=document.querySelector('#t-payouts tbody');if(!p.length){t.innerHTML='<tr><td colspan="4">none</td></tr>';return}t.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="hi">'+r.amount_zec.toFixed(8)+' TAZ</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,12)+'..':'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script></body></html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 18 — "Blueprint" — Technical drawing, blue lines on dark blue, CAD style
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW18: &str = r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>TAZ Mining Pool — Blueprint</title>
<style>
:root{--bg:#0a1628;--accent:#4488cc;--accent2:#6699dd;--ink:#8ab0d8;--muted:#5a7a9a;--dim:#3a5a7a;--dimmer:#1a3050;--line:rgba(68,136,204,0.15)}
*{margin:0;padding:0;box-sizing:border-box}html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:'Courier New','Lucida Console',monospace;font-size:12px;overflow-x:hidden}
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;
  background-image:linear-gradient(rgba(68,136,204,0.03) 1px,transparent 1px),linear-gradient(90deg,rgba(68,136,204,0.03) 1px,transparent 1px);
  background-size:40px 40px}
body::after{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;
  background:radial-gradient(800px 400px at 50% 50%,rgba(68,136,204,0.03),transparent 55%)}
main{max-width:1200px;margin:0 auto;padding:20px}
.top{display:flex;justify-content:space-between;align-items:center;padding:14px 18px;margin-bottom:14px;
  border:1px solid var(--line);background:rgba(10,22,40,0.9);position:relative}
.top::before{content:'';position:absolute;top:-1px;left:20px;right:20px;height:1px;background:var(--accent);opacity:0.3}
.top h1{font-size:16px;font-weight:400;color:var(--accent);letter-spacing:3px;text-transform:uppercase}
.top .sub{color:var(--dim);font-size:9px;margin-top:2px;letter-spacing:2px}
.nav{display:flex;gap:6px}
.nav a{color:var(--dim);text-decoration:none;font-size:9px;letter-spacing:2px;text-transform:uppercase;
  padding:4px 12px;border:1px dashed rgba(68,136,204,0.2);transition:all 0.2s}
.nav a:hover{color:var(--accent);border-style:solid;border-color:rgba(68,136,204,0.4)}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:1px;margin-bottom:1px;
  background:rgba(68,136,204,0.08)}.cards6{grid-template-columns:repeat(6,1fr)}
.card{padding:12px 14px;background:rgba(10,22,40,0.92);position:relative}
.card::after{content:'';position:absolute;top:4px;right:4px;width:6px;height:6px;border-top:1px solid rgba(68,136,204,0.2);border-right:1px solid rgba(68,136,204,0.2)}
.card .lbl{font-size:8px;text-transform:uppercase;letter-spacing:3px;color:var(--dim);margin-bottom:4px}
.card .val{font-size:22px;font-weight:400;color:var(--accent);line-height:1}
.card .val.sm{font-size:12px;color:var(--accent2)}
.stitle{font-size:8px;text-transform:uppercase;letter-spacing:4px;color:var(--dim);
  margin:16px 0 8px;display:flex;align-items:center;gap:8px}
.stitle::before,.stitle::after{content:'';flex:0 0 8px;height:1px;background:var(--accent);opacity:0.3}
table{width:100%;border-collapse:collapse;margin-bottom:8px;border:1px solid var(--line)}
th{font-size:8px;text-transform:uppercase;letter-spacing:2px;color:var(--dimmer);text-align:left;
  padding:6px 10px;border-bottom:1px solid var(--line);font-weight:400;background:rgba(10,22,40,0.5)}
td{padding:4px 10px;border-bottom:1px dashed rgba(68,136,204,0.06);color:var(--muted);font-size:11px;white-space:nowrap}
td.hi{color:var(--accent)}td.addr{max-width:170px;overflow:hidden;text-overflow:ellipsis;cursor:pointer}
td.addr:hover{color:var(--accent)}tr:hover td{background:rgba(68,136,204,0.03)}
.confirmed{color:var(--accent)}.pending{color:var(--accent2)}.orphaned{color:#886666}
.luck-good{color:var(--accent)}.luck-mid{color:var(--accent2)}.luck-bad{color:#cc5555}
.foot{margin-top:14px;padding:8px 14px;border:1px solid var(--line);font-size:8px;color:var(--dimmer);
  display:flex;gap:20px;text-transform:uppercase;letter-spacing:2px;background:rgba(10,22,40,0.7)}
.foot .on{color:var(--accent)}.foot .off{color:#cc5555}
@media(max-width:900px){.cards{grid-template-columns:1fr 1fr}.cards6{grid-template-columns:repeat(3,1fr)}}
</style></head><body><main>
<div class="top"><div><h1>TAZ Pool</h1><div class="sub">Zcash Testnet // Equihash(200,9) // PPLNS</div></div>
<div class="nav"><a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a><a href="/zallet">Wallet</a><a href="/previews">Themes</a><a href="/">V1</a></div></div>
<div class="cards"><div class="card"><div class="lbl">Pool Hashrate</div><div class="val" id="s-hash">--</div></div><div class="card"><div class="lbl">Network</div><div class="val" id="s-net">--</div></div><div class="card"><div class="lbl">Blocks</div><div class="val" id="s-blocks">0</div></div><div class="card"><div class="lbl">Miners</div><div class="val" id="s-miners">0</div></div></div>
<div class="cards cards6"><div class="card"><div class="lbl">Shares</div><div class="val sm" id="s-shares">0</div></div><div class="card"><div class="lbl">Luck</div><div class="val sm" id="s-luck">--</div></div><div class="card"><div class="lbl">Net %</div><div class="val sm" id="s-pct">--</div></div><div class="card"><div class="lbl">Immature</div><div class="val sm" id="s-imm">0</div></div><div class="card"><div class="lbl">Fee</div><div class="val sm" id="s-fee">--</div></div><div class="card"><div class="lbl">Port</div><div class="val sm" id="s-port">--</div></div></div>
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
// PREVIEW 19 — "Solarpunk" — Warm olive/green, organic, nature tech
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW19: &str = r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>TAZ Mining Pool — Solarpunk</title>
<style>
:root{--bg:#0c1208;--accent:#8fbc5a;--accent2:#c4d89a;--ink:#c8d8b0;--muted:#6a8050;--dim:#3a5028;--dimmer:#1e3014;--line:rgba(143,188,90,0.1)}
*{margin:0;padding:0;box-sizing:border-box}html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:'Inter',-apple-system,'Segoe UI',sans-serif;font-size:13px;overflow-x:hidden}
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;
  background:radial-gradient(ellipse 1000px 400px at 40% -10%,rgba(180,220,80,0.04),transparent 50%),
             radial-gradient(ellipse 600px 300px at 80% 110%,rgba(100,160,40,0.03),transparent 50%)}
main{max-width:1200px;margin:0 auto;padding:28px}
.top{display:flex;justify-content:space-between;align-items:center;padding:18px 24px;border-radius:16px;margin-bottom:18px;
  background:rgba(14,22,10,0.85);border:1px solid var(--line);box-shadow:0 4px 24px rgba(0,0,0,0.2)}
.top h1{font-size:18px;font-weight:500;color:var(--accent);letter-spacing:2px}
.top .sub{color:var(--muted);font-size:11px;margin-top:3px}
.nav{display:flex;gap:10px}
.nav a{color:var(--dim);text-decoration:none;font-size:10px;letter-spacing:1.5px;text-transform:uppercase;
  padding:6px 16px;border-radius:10px;border:1px solid var(--line);transition:all 0.2s}
.nav a:hover{color:var(--accent);border-color:rgba(143,188,90,0.3)}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:14px;margin-bottom:14px}.cards6{grid-template-columns:repeat(6,1fr)}
.card{border-radius:14px;padding:16px 18px;background:rgba(14,22,10,0.8);border:1px solid var(--line);
  box-shadow:0 4px 20px rgba(0,0,0,0.15)}
.card .lbl{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--muted);margin-bottom:6px}
.card .val{font-family:'SF Mono','Fira Code',monospace;font-size:26px;font-weight:400;color:var(--accent2);line-height:1}
.card .val.sm{font-size:14px;color:var(--accent)}
.stitle{font-size:9px;text-transform:uppercase;letter-spacing:3px;color:var(--dim);margin:22px 0 10px;
  display:flex;align-items:center;gap:8px}
.stitle::before{content:'';width:4px;height:4px;border-radius:50%;background:var(--accent)}
table{width:100%;border-collapse:separate;border-spacing:0;margin-bottom:12px;border-radius:12px;overflow:hidden;
  background:rgba(14,22,10,0.7);border:1px solid var(--line)}
th{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--dim);text-align:left;
  padding:8px 14px;border-bottom:1px solid var(--line);font-weight:400;background:rgba(14,22,10,0.4)}
td{padding:7px 14px;border-bottom:1px solid rgba(143,188,90,0.04);color:var(--muted);font-size:12px;white-space:nowrap}
td.hi{color:var(--accent2)}td.accent{color:var(--accent)}
td.addr{max-width:180px;overflow:hidden;text-overflow:ellipsis;cursor:pointer;font-family:monospace;font-size:11px}
td.addr:hover{color:var(--accent)}tr:hover td{background:rgba(143,188,90,0.02)}
.confirmed{color:var(--accent)}.pending{color:var(--accent2)}.orphaned{color:#886655}
.luck-good{color:var(--accent)}.luck-mid{color:var(--accent2)}.luck-bad{color:#cc6655}
.foot{margin-top:22px;padding:12px 18px;border-radius:12px;font-size:9px;color:var(--dim);
  display:flex;gap:28px;text-transform:uppercase;letter-spacing:2px;
  background:rgba(14,22,10,0.6);border:1px solid var(--line)}
.foot .on{color:var(--accent)}.foot .off{color:#cc6655}
@media(max-width:900px){.cards{grid-template-columns:1fr 1fr}.cards6{grid-template-columns:repeat(3,1fr)}}
</style></head><body><main>
<div class="top"><div><h1>TAZ Mining Pool</h1><div class="sub">Zcash Testnet &middot; Equihash(200,9) &middot; PPLNS</div></div>
<div class="nav"><a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a><a href="/zallet">Wallet</a><a href="/previews">Themes</a><a href="/">V1</a></div></div>
<div class="cards"><div class="card"><div class="lbl">Pool Hashrate</div><div class="val" id="s-hash">--</div></div><div class="card"><div class="lbl">Network</div><div class="val" id="s-net">--</div></div><div class="card"><div class="lbl">Blocks</div><div class="val" id="s-blocks">0</div></div><div class="card"><div class="lbl">Miners</div><div class="val" id="s-miners">0</div></div></div>
<div class="cards cards6"><div class="card"><div class="lbl">Shares</div><div class="val sm" id="s-shares">0</div></div><div class="card"><div class="lbl">Luck 24h</div><div class="val sm" id="s-luck">--</div></div><div class="card"><div class="lbl">Net Share</div><div class="val sm" id="s-pct">--</div></div><div class="card"><div class="lbl">Immature</div><div class="val sm" id="s-imm">0</div></div><div class="card"><div class="lbl">Fee</div><div class="val sm" id="s-fee">--</div></div><div class="card"><div class="lbl">Stratum</div><div class="val sm" id="s-port">--</div></div></div>
<div class="stitle">Miners</div><table id="t-miners"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>W</th><th>Shares</th><th>Pending</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="stitle">Blocks</div><table id="t-blocks"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="stitle">Payouts</div><table id="t-payouts"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead><tbody><tr><td colspan="4" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="foot"><span>Node: <span id="f-node" class="on">--</span></span><span>Wallet: <span id="f-wallet" class="on">--</span></span><span>Up: <span id="f-up">--</span></span></div>
</main><script>
const S=Date.now();function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h '+m%60+'m':m>0?m+'m '+s%60+'s':s+'s'}function $(i){return document.getElementById(i)}
async function tick(){try{const d=await(await fetch('/api/pool/stats')).json();$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;if(d.luck_percent!=null){$('s-luck').textContent=d.luck_percent.toFixed(0)+'%';$('s-luck').className='val sm '+(d.luck_percent<=100?'luck-good':d.luck_percent<=150?'luck-mid':'luck-bad')}if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';$('f-node').textContent=d.node_ok?'OK':'DOWN';$('f-node').className=d.node_ok?'on':'off';$('f-wallet').textContent=d.wallet_ok?'OK':'DOWN';$('f-wallet').className=d.wallet_ok?'on':'off';$('f-up').textContent=fd(Date.now()-S)}catch(e){}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const t=document.querySelector('#t-miners tbody');if(!m.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=m.map(r=>'<tr><td class="addr hi" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td class="accent">'+r.pending_zec.toFixed(4)+' TAZ</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const t=document.querySelector('#t-blocks tbody');if(!b.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}return'<tr><td class="accent">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,12)+'</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const t=document.querySelector('#t-payouts tbody');if(!p.length){t.innerHTML='<tr><td colspan="4">none</td></tr>';return}t.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="accent">'+r.amount_zec.toFixed(8)+' TAZ</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,12):'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script></body></html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 20 — "Crimson" — Deep red/black, luxurious dark, premium feel
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW20: &str = r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>TAZ Mining Pool — Crimson</title>
<style>
:root{--bg:#0a0406;--accent:#dc3545;--accent2:#e8606d;--accent3:#f0a0a8;--ink:#d8c0c4;--muted:#8a5a62;--dim:#5a2a34;--dimmer:#3a1218;--line:rgba(220,53,69,0.1)}
*{margin:0;padding:0;box-sizing:border-box}html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:'Inter',-apple-system,'Segoe UI',sans-serif;font-size:13px;overflow-x:hidden;
  -webkit-font-smoothing:antialiased}
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;
  background:radial-gradient(ellipse 800px 350px at 50% -10%,rgba(220,53,69,0.05),transparent 50%),
             radial-gradient(ellipse 600px 250px at 50% 110%,rgba(180,30,50,0.04),transparent 50%)}
main{max-width:1200px;margin:0 auto;padding:28px}
.top{display:flex;justify-content:space-between;align-items:center;padding:18px 24px;border-radius:14px;margin-bottom:18px;
  background:rgba(14,6,8,0.9);border:1px solid var(--line);box-shadow:0 4px 24px rgba(0,0,0,0.3);
  position:relative;overflow:hidden}
.top::before{content:'';position:absolute;inset:0;border-radius:inherit;padding:1px;
  background:linear-gradient(110deg,rgba(220,53,69,0.3),transparent 40%,transparent 60%,rgba(220,53,69,0.2));
  pointer-events:none;-webkit-mask:linear-gradient(#000 0 0) content-box,linear-gradient(#000 0 0);-webkit-mask-composite:xor;mask-composite:exclude}
.top h1{font-size:18px;font-weight:600;color:var(--accent3);letter-spacing:2px;position:relative;z-index:2}
.top .sub{color:var(--muted);font-size:11px;margin-top:3px;position:relative;z-index:2}
.nav{display:flex;gap:10px;position:relative;z-index:2}
.nav a{color:var(--dim);text-decoration:none;font-size:10px;letter-spacing:1.5px;text-transform:uppercase;
  padding:6px 16px;border-radius:8px;border:1px solid var(--line);transition:all 0.2s}
.nav a:hover{color:var(--accent);border-color:rgba(220,53,69,0.25)}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:14px;margin-bottom:14px}.cards6{grid-template-columns:repeat(6,1fr)}
.card{border-radius:12px;padding:16px 18px;background:rgba(14,6,8,0.85);border:1px solid var(--line);
  box-shadow:0 4px 20px rgba(0,0,0,0.2)}
.card .lbl{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--muted);margin-bottom:6px}
.card .val{font-family:'SF Mono','Fira Code',monospace;font-size:26px;font-weight:400;color:var(--accent3);line-height:1}
.card .val.sm{font-size:14px;color:var(--accent2)}
.stitle{font-size:9px;text-transform:uppercase;letter-spacing:3px;color:var(--dim);margin:22px 0 10px;
  display:flex;align-items:center;gap:8px}
.stitle::before{content:'';flex:0 0 16px;height:1px;background:linear-gradient(90deg,var(--accent),transparent)}
table{width:100%;border-collapse:separate;border-spacing:0;margin-bottom:12px;border-radius:10px;overflow:hidden;
  background:rgba(14,6,8,0.7);border:1px solid var(--line)}
th{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--dim);text-align:left;
  padding:8px 14px;border-bottom:1px solid var(--line);font-weight:400;background:rgba(14,6,8,0.4)}
td{padding:7px 14px;border-bottom:1px solid rgba(220,53,69,0.04);color:var(--muted);font-size:12px;white-space:nowrap}
td.hi{color:var(--accent3)}td.accent{color:var(--accent)}
td.addr{max-width:180px;overflow:hidden;text-overflow:ellipsis;cursor:pointer;font-family:monospace;font-size:11px}
td.addr:hover{color:var(--accent2)}tr:hover td{background:rgba(220,53,69,0.02)}
.confirmed{color:#5ab88a}.pending{color:var(--accent2)}.orphaned{color:var(--dim)}
.luck-good{color:#5ab88a}.luck-mid{color:var(--accent2)}.luck-bad{color:var(--accent)}
.foot{margin-top:22px;padding:12px 18px;border-radius:10px;font-size:9px;color:var(--dim);
  display:flex;gap:28px;text-transform:uppercase;letter-spacing:2px;
  background:rgba(14,6,8,0.6);border:1px solid var(--line)}
.foot .on{color:#5ab88a}.foot .off{color:var(--accent)}
@media(max-width:900px){.cards{grid-template-columns:1fr 1fr}.cards6{grid-template-columns:repeat(3,1fr)}}
</style></head><body><main>
<div class="top"><div><h1>TAZ Mining Pool</h1><div class="sub">Zcash Testnet &middot; Equihash(200,9) &middot; PPLNS</div></div>
<div class="nav"><a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a><a href="/zallet">Wallet</a><a href="/previews">Themes</a><a href="/">V1</a></div></div>
<div class="cards"><div class="card"><div class="lbl">Pool Hashrate</div><div class="val" id="s-hash">--</div></div><div class="card"><div class="lbl">Network</div><div class="val" id="s-net">--</div></div><div class="card"><div class="lbl">Blocks</div><div class="val" id="s-blocks">0</div></div><div class="card"><div class="lbl">Miners</div><div class="val" id="s-miners">0</div></div></div>
<div class="cards cards6"><div class="card"><div class="lbl">Shares</div><div class="val sm" id="s-shares">0</div></div><div class="card"><div class="lbl">Luck 24h</div><div class="val sm" id="s-luck">--</div></div><div class="card"><div class="lbl">Net Share</div><div class="val sm" id="s-pct">--</div></div><div class="card"><div class="lbl">Immature</div><div class="val sm" id="s-imm">0</div></div><div class="card"><div class="lbl">Fee</div><div class="val sm" id="s-fee">--</div></div><div class="card"><div class="lbl">Stratum</div><div class="val sm" id="s-port">--</div></div></div>
<div class="stitle">Miners</div><table id="t-miners"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>W</th><th>Shares</th><th>Pending</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="stitle">Blocks</div><table id="t-blocks"><thead><tr><th>Height</th><th>Hash</th><th>Reward</th><th>Luck</th><th>Status</th><th>Found</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="stitle">Payouts</div><table id="t-payouts"><thead><tr><th>Miner</th><th>Amount</th><th>TxID</th><th>Date</th></tr></thead><tbody><tr><td colspan="4" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
<div class="foot"><span>Node: <span id="f-node" class="on">--</span></span><span>Wallet: <span id="f-wallet" class="on">--</span></span><span>Up: <span id="f-up">--</span></span></div>
</main><script>
const S=Date.now();function fh(h){if(h==null||isNaN(h))return'--';if(h>=1e12)return(h/1e12).toFixed(2)+' TSol/s';if(h>=1e9)return(h/1e9).toFixed(2)+' GSol/s';if(h>=1e6)return(h/1e6).toFixed(2)+' MSol/s';if(h>=1e3)return(h/1e3).toFixed(2)+' KSol/s';return h.toFixed(1)+' Sol/s'}function fd(ms){const s=Math.floor(ms/1000),m=Math.floor(s/60),h=Math.floor(m/60);return h>0?h+'h '+m%60+'m':m>0?m+'m '+s%60+'s':s+'s'}function $(i){return document.getElementById(i)}
async function tick(){try{const d=await(await fetch('/api/pool/stats')).json();$('s-hash').textContent=fh(d.hashrate_estimate);$('s-net').textContent=fh(d.network_hashrate);$('s-blocks').textContent=d.total_blocks;$('s-miners').textContent=d.connected_miners;$('s-shares').textContent=d.total_shares.toLocaleString();$('s-imm').textContent=d.immature_blocks;$('s-fee').textContent=d.fee_percent+'%';$('s-port').textContent=d.stratum_port;if(d.luck_percent!=null){$('s-luck').textContent=d.luck_percent.toFixed(0)+'%';$('s-luck').className='val sm '+(d.luck_percent<=100?'luck-good':d.luck_percent<=150?'luck-mid':'luck-bad')}if(d.pool_percent_24h!=null)$('s-pct').textContent=d.pool_percent_24h.toFixed(2)+'%';$('f-node').textContent=d.node_ok?'OK':'DOWN';$('f-node').className=d.node_ok?'on':'off';$('f-wallet').textContent=d.wallet_ok?'OK':'DOWN';$('f-wallet').className=d.wallet_ok?'on':'off';$('f-up').textContent=fd(Date.now()-S)}catch(e){}}
async function miners(){try{const m=await(await fetch('/api/miners')).json();const t=document.querySelector('#t-miners tbody');if(!m.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=m.map(r=>'<tr><td class="addr hi" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td class="accent">'+r.pending_zec.toFixed(4)+' TAZ</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const t=document.querySelector('#t-blocks tbody');if(!b.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}return'<tr><td class="accent">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,12)+'</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const t=document.querySelector('#t-payouts tbody');if(!p.length){t.innerHTML='<tr><td colspan="4">none</td></tr>';return}t.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="accent">'+r.amount_zec.toFixed(8)+' TAZ</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,12):'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script></body></html>
"##;
