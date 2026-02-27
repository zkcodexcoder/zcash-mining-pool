// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 11 — "Obsidian" — Dark monochrome, no color, extreme minimalism
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW11: &str = r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>TAZ Mining Pool — Obsidian</title>
<style>
:root{--bg:#0a0a0a;--panel:#111;--accent:#e0e0e0;--accent2:#999;--ink:#b0b0b0;--muted:#666;--dim:#444;--dimmer:#2a2a2a;--line:rgba(255,255,255,0.06)}
*{margin:0;padding:0;box-sizing:border-box}html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;font-size:13px;overflow-x:hidden}
main{max-width:1200px;margin:0 auto;padding:24px}
.top{display:flex;justify-content:space-between;align-items:center;padding:16px 0;margin-bottom:20px;border-bottom:1px solid var(--line)}
.top h1{font-size:16px;font-weight:500;color:var(--accent);letter-spacing:4px;text-transform:uppercase}
.top .sub{color:var(--dim);font-size:10px;margin-top:3px;letter-spacing:1px}
.nav{display:flex;gap:12px}
.nav a{color:var(--dim);text-decoration:none;font-size:10px;letter-spacing:1.5px;text-transform:uppercase;
  padding:6px 14px;border:1px solid var(--line);border-radius:3px;transition:all 0.15s}
.nav a:hover{color:var(--accent);border-color:rgba(255,255,255,0.15)}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:1px;background:var(--line);margin-bottom:1px}.cards6{grid-template-columns:repeat(6,1fr)}
.card{padding:16px 18px;background:var(--panel)}
.card .lbl{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--dim);margin-bottom:6px}
.card .val{font-family:'SF Mono','Fira Code',monospace;font-size:28px;font-weight:200;color:var(--accent);line-height:1}
.card .val.sm{font-size:14px;font-weight:400;color:var(--accent2)}
.stitle{font-size:9px;text-transform:uppercase;letter-spacing:3px;color:var(--dim);margin:24px 0 8px}
table{width:100%;border-collapse:collapse;margin-bottom:10px}
th{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--dimmer);text-align:left;
  padding:8px 12px;border-bottom:1px solid var(--line);font-weight:400}
td{padding:6px 12px;border-bottom:1px solid var(--line);color:var(--muted);font-size:12px;white-space:nowrap}
td.hi{color:var(--accent)}td.addr{max-width:180px;overflow:hidden;text-overflow:ellipsis;cursor:pointer;font-family:monospace;font-size:11px}
td.addr:hover{color:var(--accent)}tr:hover td{background:rgba(255,255,255,0.02)}
.confirmed{color:#a0a0a0}.pending{color:var(--dim)}.orphaned{color:#555}
.luck-good{color:#a0a0a0}.luck-mid{color:var(--muted)}.luck-bad{color:#888}
.foot{margin-top:24px;padding:12px 0;border-top:1px solid var(--line);font-size:9px;color:var(--dimmer);
  display:flex;gap:28px;text-transform:uppercase;letter-spacing:2px}
.foot .on{color:var(--accent2)}.foot .off{color:#666}
@media(max-width:900px){.cards{grid-template-columns:1fr 1fr}.cards6{grid-template-columns:repeat(3,1fr)}}
</style></head><body><main>
<div class="top"><div><h1>TAZ Pool</h1><div class="sub">Zcash Testnet &middot; Equihash(200,9) &middot; PPLNS</div></div>
<div class="nav"><a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a><a href="/zallet">Wallet</a><a href="/previews">Themes</a><a href="/">V1</a></div></div>
<div class="cards"><div class="card"><div class="lbl">Pool Hashrate</div><div class="val" id="s-hash">--</div></div><div class="card"><div class="lbl">Network</div><div class="val" id="s-net">--</div></div><div class="card"><div class="lbl">Blocks</div><div class="val" id="s-blocks">0</div></div><div class="card"><div class="lbl">Miners</div><div class="val" id="s-miners">0</div></div></div>
<div class="cards cards6"><div class="card"><div class="lbl">Shares</div><div class="val sm" id="s-shares">0</div></div><div class="card"><div class="lbl">Luck 24h</div><div class="val sm" id="s-luck">--</div></div><div class="card"><div class="lbl">Net Share</div><div class="val sm" id="s-pct">--</div></div><div class="card"><div class="lbl">Immature</div><div class="val sm" id="s-imm">0</div></div><div class="card"><div class="lbl">Fee</div><div class="val sm" id="s-fee">--</div></div><div class="card"><div class="lbl">Stratum</div><div class="val sm" id="s-port">--</div></div></div>
<div class="stitle">Miners</div><table id="t-miners"><thead><tr><th>Address</th><th>1m</th><th>10m</th><th>Workers</th><th>Shares</th><th>Pending</th></tr></thead><tbody><tr><td colspan="6" style="color:var(--dimmer)">loading...</td></tr></tbody></table>
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
// PREVIEW 12 — "Aurora" — Northern lights gradient, teal/green/purple shimmer
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW12: &str = r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>TAZ Mining Pool — Aurora</title>
<style>
:root{--bg:#050d12;--accent:#00ddb0;--accent2:#7b68ee;--accent3:#00bbcc;--ink:#c8e8e0;--muted:#5a8a7a;--dim:#2a5a4a;--dimmer:#1a3a30;--line:rgba(0,221,176,0.12);--glow:rgba(0,221,176,0.08)}
*{margin:0;padding:0;box-sizing:border-box}html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:'Inter',-apple-system,'Segoe UI',sans-serif;font-size:13px;overflow-x:hidden}
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;
  background:radial-gradient(ellipse 1200px 400px at 20% -5%,rgba(0,221,176,0.07),transparent 55%),
             radial-gradient(ellipse 900px 350px at 60% -8%,rgba(123,104,238,0.06),transparent 50%),
             radial-gradient(ellipse 700px 300px at 85% 110%,rgba(0,187,204,0.05),transparent 50%)}
@keyframes aurora{0%,100%{opacity:0.6}50%{opacity:1}}
main{max-width:1300px;margin:0 auto;padding:24px}
.top{display:flex;justify-content:space-between;align-items:center;padding:16px 22px;border-radius:14px;margin-bottom:16px;
  background:rgba(6,16,22,0.85);border:1px solid var(--line);box-shadow:0 0 24px var(--glow);position:relative;overflow:hidden}
.top::before{content:'';position:absolute;top:0;left:0;right:0;height:2px;
  background:linear-gradient(90deg,transparent,var(--accent),var(--accent2),var(--accent3),transparent);animation:aurora 4s ease-in-out infinite}
.top h1{font-size:20px;font-weight:600;color:var(--accent);letter-spacing:2px;position:relative;z-index:2;
  text-shadow:0 0 12px rgba(0,221,176,0.4)}
.top .sub{color:var(--muted);font-size:11px;margin-top:2px;position:relative;z-index:2}
.nav{display:flex;gap:8px;position:relative;z-index:2}
.nav a{color:var(--dim);text-decoration:none;font-size:10px;letter-spacing:1.5px;text-transform:uppercase;
  padding:5px 14px;border:1px solid var(--line);border-radius:6px;transition:all 0.2s}
.nav a:hover{color:var(--accent);border-color:rgba(0,221,176,0.3)}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:12px;margin-bottom:12px}.cards6{grid-template-columns:repeat(6,1fr)}
.card{border-radius:12px;padding:14px 16px;background:rgba(6,16,22,0.8);border:1px solid var(--line);
  box-shadow:0 4px 20px rgba(0,0,0,0.2);position:relative;overflow:hidden}
.card::before{content:'';position:absolute;top:0;left:0;right:0;height:1px;
  background:linear-gradient(90deg,transparent,rgba(0,221,176,0.3),rgba(123,104,238,0.2),transparent)}
.card .lbl{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--muted);margin-bottom:5px}
.card .val{font-family:'SF Mono','Fira Code',monospace;font-size:24px;font-weight:500;line-height:1;
  background:linear-gradient(135deg,var(--accent),var(--accent3));-webkit-background-clip:text;-webkit-text-fill-color:transparent;
  filter:drop-shadow(0 0 6px rgba(0,221,176,0.4))}
.card .val.sm{font-size:14px;filter:drop-shadow(0 0 3px rgba(0,221,176,0.3))}
.stitle{font-size:9px;text-transform:uppercase;letter-spacing:3px;color:var(--dim);margin:18px 0 8px;padding-left:10px;border-left:2px solid var(--accent)}
table{width:100%;border-collapse:collapse;margin-bottom:10px}
th{font-size:9px;text-transform:uppercase;letter-spacing:1.5px;color:var(--dimmer);text-align:left;padding:7px 12px;border-bottom:1px solid var(--line);font-weight:400}
td{padding:5px 12px;border-bottom:1px solid rgba(0,221,176,0.05);color:var(--muted);font-size:12px;white-space:nowrap}
td.hi{color:var(--accent);text-shadow:0 0 4px rgba(0,221,176,0.3)}
td.addr{max-width:170px;overflow:hidden;text-overflow:ellipsis;cursor:pointer;font-family:monospace;font-size:11px}
td.addr:hover{color:var(--accent)}tr:hover td{background:rgba(0,221,176,0.02)}
.confirmed{color:var(--accent)}.pending{color:var(--accent2)}.orphaned{color:#884444}
.luck-good{color:var(--accent)}.luck-mid{color:var(--accent2)}.luck-bad{color:#dd4444}
.foot{margin-top:18px;padding:10px 16px;border-radius:10px;font-size:9px;color:var(--dim);display:flex;gap:24px;text-transform:uppercase;letter-spacing:1.5px;background:rgba(6,16,22,0.6);border:1px solid var(--line)}
.foot .on{color:var(--accent)}.foot .off{color:#dd4444}
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
async function miners(){try{const m=await(await fetch('/api/miners')).json();const t=document.querySelector('#t-miners tbody');if(!m.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=m.map(r=>'<tr><td class="addr hi" title="'+r.address+'">'+r.address+'</td><td>'+fh(r.hashrate_1m)+'</td><td>'+fh(r.hashrate)+'</td><td>'+r.worker_count+'</td><td>'+r.share_count.toLocaleString()+'</td><td class="hi">'+r.pending_zec.toFixed(4)+' TAZ</td></tr>').join('')}catch(e){}}
async function blocks(){try{const b=await(await fetch('/api/blocks')).json();const t=document.querySelector('#t-blocks tbody');if(!b.length){t.innerHTML='<tr><td colspan="6">none</td></tr>';return}t.innerHTML=b.slice(0,50).map(r=>{let ls='--',lc='luck-good';if(r.luck_percent!=null){ls=r.luck_percent.toFixed(0)+'%';lc=r.luck_percent<=100?'luck-good':r.luck_percent<=150?'luck-mid':'luck-bad'}return'<tr><td class="hi">'+r.height+'</td><td title="'+r.hash+'">'+r.hash.substring(0,12)+'</td><td>'+r.reward_zec.toFixed(4)+'</td><td class="'+lc+'">'+ls+'</td><td class="'+r.status+'">'+r.status+'</td><td>'+r.found_at+'</td></tr>'}).join('')}catch(e){}}
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const t=document.querySelector('#t-payouts tbody');if(!p.length){t.innerHTML='<tr><td colspan="4">none</td></tr>';return}t.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="hi">'+r.amount_zec.toFixed(8)+' TAZ</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,12):'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script></body></html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 13 — "Zcash" — Official Zcash brand colors (electric blue + yellow)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW13: &str = r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>TAZ Mining Pool — Zcash</title>
<style>
:root{--bg:#0a0e16;--accent:#f4b728;--accent2:#ecb22e;--accent3:#4a9eff;--ink:#d8dce8;--muted:#7a88a0;--dim:#3a4560;--dimmer:#222838;--line:rgba(244,183,40,0.12);--glow:rgba(244,183,40,0.08)}
*{margin:0;padding:0;box-sizing:border-box}html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:'Inter',-apple-system,'Segoe UI',sans-serif;font-size:13px;overflow-x:hidden}
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;
  background:radial-gradient(900px 400px at 20% -5%,rgba(244,183,40,0.06),transparent 55%),
             radial-gradient(700px 350px at 80% 110%,rgba(74,158,255,0.05),transparent 50%)}
main{max-width:1300px;margin:0 auto;padding:24px}
.top{display:flex;justify-content:space-between;align-items:center;padding:16px 22px;border-radius:14px;margin-bottom:16px;
  background:rgba(12,16,24,0.9);border:1px solid var(--line);box-shadow:0 0 20px var(--glow);position:relative;overflow:hidden}
.top::before{content:'';position:absolute;inset:0;border-radius:inherit;padding:1.5px;
  background:linear-gradient(110deg,rgba(244,183,40,0.5),rgba(74,158,255,0.4),rgba(244,183,40,0.5));
  opacity:0.2;pointer-events:none;-webkit-mask:linear-gradient(#000 0 0) content-box,linear-gradient(#000 0 0);-webkit-mask-composite:xor;mask-composite:exclude}
.top h1{font-size:20px;font-weight:700;color:var(--accent);letter-spacing:2px;position:relative;z-index:2;
  text-shadow:0 0 10px rgba(244,183,40,0.4),0 0 20px rgba(244,183,40,0.2)}
.zcash-logo{display:inline-block;width:28px;height:28px;border-radius:50%;background:var(--accent);color:var(--bg);
  font-weight:800;font-size:16px;text-align:center;line-height:28px;margin-right:10px;vertical-align:middle;
  box-shadow:0 0 12px rgba(244,183,40,0.4)}
.top .sub{color:var(--muted);font-size:11px;margin-top:2px;position:relative;z-index:2}
.nav{display:flex;gap:8px;position:relative;z-index:2}
.nav a{color:var(--dim);text-decoration:none;font-size:10px;letter-spacing:1.5px;text-transform:uppercase;
  padding:5px 14px;border:1px solid var(--line);border-radius:6px;transition:all 0.2s}
.nav a:hover{color:var(--accent);border-color:rgba(244,183,40,0.3);box-shadow:0 0 8px var(--glow)}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:12px;margin-bottom:12px}.cards6{grid-template-columns:repeat(6,1fr)}
.card{border-radius:12px;padding:14px 16px;background:rgba(12,16,24,0.85);border:1px solid var(--line);
  box-shadow:0 4px 20px rgba(0,0,0,0.2);position:relative;overflow:hidden}
.card .lbl{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--muted);margin-bottom:5px}
.card .val{font-family:'SF Mono','Fira Code',monospace;font-size:24px;font-weight:600;color:var(--accent);line-height:1;
  text-shadow:0 0 6px rgba(244,183,40,0.4),0 0 14px rgba(244,183,40,0.2)}
.card .val.sm{font-size:14px;color:var(--accent2);text-shadow:0 0 4px rgba(236,178,46,0.3)}
.card.blue .val{color:var(--accent3);text-shadow:0 0 6px rgba(74,158,255,0.4)}
.stitle{font-size:9px;text-transform:uppercase;letter-spacing:3px;color:var(--dim);margin:18px 0 8px;
  padding-left:10px;border-left:2px solid var(--accent)}
table{width:100%;border-collapse:collapse;margin-bottom:10px}
th{font-size:9px;text-transform:uppercase;letter-spacing:1.5px;color:var(--dimmer);text-align:left;
  padding:7px 12px;border-bottom:1px solid var(--line);font-weight:400}
td{padding:5px 12px;border-bottom:1px solid rgba(244,183,40,0.05);color:var(--muted);font-size:12px;white-space:nowrap}
td.hi{color:var(--accent);text-shadow:0 0 4px rgba(244,183,40,0.3)}
td.blue{color:var(--accent3);text-shadow:0 0 4px rgba(74,158,255,0.3)}
td.addr{max-width:170px;overflow:hidden;text-overflow:ellipsis;cursor:pointer;font-family:monospace;font-size:11px}
td.addr:hover{color:var(--accent)}tr:hover td{background:rgba(244,183,40,0.02)}
.confirmed{color:#48bb78}.pending{color:var(--accent)}.orphaned{color:#dd4444}
.luck-good{color:#48bb78}.luck-mid{color:var(--accent)}.luck-bad{color:#dd4444}
.foot{margin-top:18px;padding:10px 16px;border-radius:10px;font-size:9px;color:var(--dim);
  display:flex;gap:24px;text-transform:uppercase;letter-spacing:1.5px;
  background:rgba(12,16,24,0.6);border:1px solid var(--line)}
.foot .on{color:#48bb78}.foot .off{color:#dd4444}
@media(max-width:900px){.cards{grid-template-columns:1fr 1fr}.cards6{grid-template-columns:repeat(3,1fr)}}
</style></head><body><main>
<div class="top"><div><h1><span class="zcash-logo">Z</span>TAZ Mining Pool</h1><div class="sub">Zcash Testnet &middot; Equihash(200,9) &middot; PPLNS</div></div>
<div class="nav"><a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a><a href="/zallet">Wallet</a><a href="/previews">Themes</a><a href="/">V1</a></div></div>
<div class="cards"><div class="card"><div class="lbl">Pool Hashrate</div><div class="val" id="s-hash">--</div></div><div class="card blue"><div class="lbl">Network</div><div class="val" id="s-net">--</div></div><div class="card"><div class="lbl">Blocks</div><div class="val" id="s-blocks">0</div></div><div class="card blue"><div class="lbl">Miners</div><div class="val" id="s-miners">0</div></div></div>
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
async function payouts(){try{const p=await(await fetch('/api/payouts')).json();const t=document.querySelector('#t-payouts tbody');if(!p.length){t.innerHTML='<tr><td colspan="4">none</td></tr>';return}t.innerHTML=p.map(r=>'<tr><td class="addr" title="'+r.miner_address+'">'+r.miner_address+'</td><td class="hi">'+r.amount_zec.toFixed(8)+' TAZ</td><td title="'+(r.txid||'')+'">'+( r.txid?r.txid.substring(0,12):'--')+'</td><td>'+r.created_at+'</td></tr>').join('')}catch(e){}}
tick();miners();blocks();payouts();setInterval(tick,10000);setInterval(miners,10000);setInterval(blocks,30000);setInterval(payouts,30000);
</script></body></html>
"##;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PREVIEW 14 — "Midnight" — Deep navy, refined corporate, subtle blue accents
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW14: &str = r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>TAZ Mining Pool — Midnight</title>
<style>
:root{--bg:#0c1020;--panel:rgba(14,18,38,0.9);--accent:#6c8fff;--accent2:#4a6ad8;--ink:#b0b8d0;--muted:#5a6888;--dim:#3a4260;--dimmer:#1e2440;--line:rgba(108,143,255,0.1)}
*{margin:0;padding:0;box-sizing:border-box}html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:-apple-system,BlinkMacSystemFont,'Inter','Segoe UI',sans-serif;font-size:13px;overflow-x:hidden;
  -webkit-font-smoothing:antialiased;-moz-osx-font-smoothing:grayscale}
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;
  background:radial-gradient(800px 400px at 50% 0%,rgba(108,143,255,0.04),transparent 55%)}
main{max-width:1200px;margin:0 auto;padding:28px}
.top{display:flex;justify-content:space-between;align-items:center;padding:18px 24px;border-radius:12px;margin-bottom:18px;
  background:var(--panel);border:1px solid var(--line);box-shadow:0 4px 24px rgba(0,0,0,0.3)}
.top h1{font-size:18px;font-weight:600;color:#d0d8f0;letter-spacing:1px}
.top .sub{color:var(--muted);font-size:11px;margin-top:3px}
.nav{display:flex;gap:8px}
.nav a{color:var(--muted);text-decoration:none;font-size:10px;letter-spacing:1px;text-transform:uppercase;
  padding:6px 16px;border-radius:8px;border:1px solid var(--line);background:rgba(14,18,38,0.5);transition:all 0.2s}
.nav a:hover{color:var(--accent);border-color:rgba(108,143,255,0.25)}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:14px;margin-bottom:14px}.cards6{grid-template-columns:repeat(6,1fr)}
.card{border-radius:10px;padding:16px 18px;background:var(--panel);border:1px solid var(--line);
  box-shadow:0 2px 16px rgba(0,0,0,0.15)}
.card .lbl{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--muted);margin-bottom:6px}
.card .val{font-family:'SF Mono','Fira Code','JetBrains Mono',monospace;font-size:26px;font-weight:400;color:#c0ccf0;line-height:1}
.card .val.sm{font-size:14px;color:var(--accent)}
.stitle{font-size:9px;text-transform:uppercase;letter-spacing:3px;color:var(--dim);margin:22px 0 10px;
  display:flex;align-items:center;gap:8px}
.stitle::before{content:'';flex:0 0 12px;height:2px;background:var(--accent);border-radius:1px}
table{width:100%;border-collapse:separate;border-spacing:0;margin-bottom:12px;border-radius:10px;overflow:hidden;
  background:var(--panel);border:1px solid var(--line)}
th{font-size:9px;text-transform:uppercase;letter-spacing:2px;color:var(--dim);text-align:left;
  padding:8px 14px;border-bottom:1px solid var(--line);font-weight:400;background:rgba(14,18,38,0.4)}
td{padding:7px 14px;border-bottom:1px solid rgba(108,143,255,0.04);color:var(--muted);font-size:12px;white-space:nowrap}
td.hi{color:#c0ccf0}td.accent{color:var(--accent)}
td.addr{max-width:180px;overflow:hidden;text-overflow:ellipsis;cursor:pointer;font-family:monospace;font-size:11px}
td.addr:hover{color:var(--accent)}tr:hover td{background:rgba(108,143,255,0.02)}
.confirmed{color:#5ab88a}.pending{color:var(--accent)}.orphaned{color:#a05555}
.luck-good{color:#5ab88a}.luck-mid{color:#d4aa55}.luck-bad{color:#c05555}
.foot{margin-top:22px;padding:12px 18px;border-radius:10px;font-size:9px;color:var(--dim);
  display:flex;gap:28px;text-transform:uppercase;letter-spacing:2px;
  background:var(--panel);border:1px solid var(--line)}
.foot .on{color:#5ab88a}.foot .off{color:#c05555}
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
// PREVIEW 15 — "Toxic" — Radioactive green/yellow, high contrast, bold
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const PREVIEW15: &str = r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>TAZ Mining Pool — Toxic</title>
<style>
:root{--bg:#060804;--accent:#ccff00;--accent2:#88dd00;--ink:#d0e8a0;--muted:#6a8040;--dim:#3a5020;--dimmer:#1e2a10;--line:rgba(204,255,0,0.1);--glow:rgba(204,255,0,0.1)}
*{margin:0;padding:0;box-sizing:border-box}html,body{background:var(--bg);color:var(--ink);min-height:100vh}
body{font-family:'JetBrains Mono','Fira Code','Courier New',monospace;font-size:12px;overflow-x:hidden}
body::before{content:'';position:fixed;inset:0;pointer-events:none;z-index:-1;
  background:radial-gradient(700px 350px at 50% 50%,rgba(204,255,0,0.04),transparent 55%)}
body::after{content:'';position:fixed;inset:0;pointer-events:none;z-index:9999;
  background:repeating-linear-gradient(0deg,transparent 0px,transparent 2px,rgba(0,0,0,0.08) 2px,rgba(0,0,0,0.08) 4px);opacity:0.5}
main{max-width:1200px;margin:0 auto;padding:16px}
.top{display:flex;justify-content:space-between;align-items:center;padding:12px 16px;margin-bottom:12px;
  border:2px solid rgba(204,255,0,0.2);background:rgba(8,12,4,0.95);
  box-shadow:0 0 20px var(--glow),inset 0 0 30px rgba(204,255,0,0.02)}
.top h1{font-size:18px;font-weight:900;color:var(--accent);letter-spacing:4px;text-transform:uppercase;
  text-shadow:0 0 12px rgba(204,255,0,0.6),0 0 30px rgba(204,255,0,0.3)}
.top .sub{color:var(--dim);font-size:10px;margin-top:2px;letter-spacing:2px}
.nav{display:flex;gap:6px}
.nav a{color:var(--dim);text-decoration:none;font-size:9px;letter-spacing:2px;text-transform:uppercase;
  padding:4px 12px;border:2px solid rgba(204,255,0,0.15);transition:all 0.15s;font-weight:700}
.nav a:hover{color:var(--accent);border-color:rgba(204,255,0,0.4);box-shadow:0 0 10px var(--glow);
  text-shadow:0 0 6px rgba(204,255,0,0.4)}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:2px;margin-bottom:2px}.cards6{grid-template-columns:repeat(6,1fr)}
.card{padding:12px 14px;background:rgba(8,12,4,0.9);border:1px solid var(--line);
  box-shadow:inset 0 0 20px rgba(204,255,0,0.02)}
.card .lbl{font-size:8px;text-transform:uppercase;letter-spacing:3px;color:var(--dim);margin-bottom:4px;font-weight:700}
.card .val{font-size:26px;font-weight:900;color:var(--accent);line-height:1;
  text-shadow:0 0 8px rgba(204,255,0,0.5),0 0 20px rgba(204,255,0,0.3)}
.card .val.sm{font-size:13px;font-weight:700;color:var(--accent2);text-shadow:0 0 4px rgba(136,221,0,0.3)}
.stitle{font-size:8px;text-transform:uppercase;letter-spacing:4px;color:var(--dim);
  margin:14px 0 6px;font-weight:900;display:flex;align-items:center;gap:6px}
.stitle::before{content:'[';color:var(--accent)}.stitle::after{content:']';color:var(--accent)}
table{width:100%;border-collapse:collapse;margin-bottom:6px;border:1px solid var(--line)}
th{font-size:8px;text-transform:uppercase;letter-spacing:2px;color:var(--dimmer);text-align:left;
  padding:5px 10px;border-bottom:1px solid var(--line);font-weight:900;background:rgba(8,12,4,0.5)}
td{padding:4px 10px;border-bottom:1px solid rgba(204,255,0,0.04);color:var(--muted);font-size:11px;white-space:nowrap}
td.hi{color:var(--accent);text-shadow:0 0 4px rgba(204,255,0,0.3)}
td.addr{max-width:160px;overflow:hidden;text-overflow:ellipsis;cursor:pointer}
td.addr:hover{color:var(--accent)}tr:hover td{background:rgba(204,255,0,0.03)}
.confirmed{color:var(--accent)}.pending{color:var(--accent2)}.orphaned{color:#884422}
.luck-good{color:var(--accent)}.luck-mid{color:var(--accent2)}.luck-bad{color:#ff4422}
.foot{margin-top:14px;padding:8px 16px;border:1px solid var(--line);font-size:8px;color:var(--dimmer);
  display:flex;gap:20px;text-transform:uppercase;letter-spacing:2px;font-weight:700;background:rgba(8,12,4,0.7)}
.foot .on{color:var(--accent);text-shadow:0 0 4px rgba(204,255,0,0.4)}.foot .off{color:#ff4422}
@media(max-width:900px){.cards{grid-template-columns:1fr 1fr}.cards6{grid-template-columns:repeat(3,1fr)}}
</style></head><body><main>
<div class="top"><div><h1>TAZ Pool</h1><div class="sub">Zcash Testnet // Equihash // PPLNS</div></div>
<div class="nav"><a href="http://pool.tazminer.com:3000" target="_blank" rel="noopener">Mine</a><a href="/zallet">Wallet</a><a href="/previews">Themes</a><a href="/">V1</a></div></div>
<div class="cards"><div class="card"><div class="lbl">Pool Hash</div><div class="val" id="s-hash">--</div></div><div class="card"><div class="lbl">Net Hash</div><div class="val" id="s-net">--</div></div><div class="card"><div class="lbl">Blocks</div><div class="val" id="s-blocks">0</div></div><div class="card"><div class="lbl">Miners</div><div class="val" id="s-miners">0</div></div></div>
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
