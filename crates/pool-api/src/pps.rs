//! Live PPS ledger dashboard, served at /pps with data from /api/pps.
//!
//! Read-only. Every figure is pulled live from the pool database each request
//! (pps_meta, pps_funding_policy, pps_accounts, pps_payouts) plus the
//! credit-health record in pool_status, so the page reflects the real engine
//! state. The page polls /api/pps on an interval; no caching.
use crate::handlers::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, Json};
use serde::Serialize;
use sqlx::Row;

#[derive(Serialize)]
struct MinerRow {
    address: String,
    pending: i64,
    paying: i64,
    paid: i64,
}

#[derive(Serialize)]
struct PayoutRow {
    txid: String,
    amount: i64,
    created_at: String,
}

#[derive(Serialize)]
pub struct PpsData {
    network: String,
    epoch: String,
    fee_percent: f64,
    gross_whole_zat: i64,
    gross_fraction: i64,
    cap_zat: i64,
    fee_cap_zat: i64,
    total_cap_zat: i64,
    reserve_floor_zat: i64,
    event_count: i64,
    pending_zat: i64,
    paying_zat: i64,
    paid_zat: i64,
    payout_count: i64,
    payout_total_zat: i64,
    max_payout_zat: i64,
    settle_maturity: i64,
    health: serde_json::Value,
    miners: Vec<MinerRow>,
    recent_payouts: Vec<PayoutRow>,
}

/// Live JSON for the /pps dashboard. Read-only; no money path.
pub async fn get_pps_data(State(state): State<AppState>) -> Result<Json<PpsData>, StatusCode> {
    let db = state.db.inner();
    let oops = |_| StatusCode::INTERNAL_SERVER_ERROR;

    let meta = sqlx::query(
        "SELECT active_epoch, cap_zats, gross_whole, gross_fraction, event_count \
         FROM pps_meta WHERE singleton=1",
    )
    .fetch_one(db)
    .await
    .map_err(oops)?;

    let policy = sqlx::query(
        "SELECT credit_cap, total_cap, fee_cap, reserve_floor \
         FROM pps_funding_policy WHERE singleton=1",
    )
    .fetch_one(db)
    .await
    .map_err(oops)?;

    let totals = sqlx::query(
        "SELECT COALESCE(SUM(pending),0) p, COALESCE(SUM(paying),0) g, COALESCE(SUM(paid),0) d \
         FROM pps_accounts",
    )
    .fetch_one(db)
    .await
    .map_err(oops)?;

    let payouts = sqlx::query("SELECT COUNT(*) n, COALESCE(SUM(amount),0) t FROM pps_payouts")
        .fetch_one(db)
        .await
        .map_err(oops)?;

    let miner_rows = sqlx::query(
        "SELECT m.address a, ac.pending p, ac.paying g, ac.paid d \
         FROM pps_accounts ac JOIN miners m ON m.id=ac.miner_id \
         WHERE ac.pending>0 OR ac.paying>0 OR ac.paid>0 \
         ORDER BY ac.pending DESC LIMIT 25",
    )
    .fetch_all(db)
    .await
    .map_err(oops)?;

    let payout_rows = sqlx::query(
        "SELECT txid, amount, created_at FROM pps_payouts ORDER BY rowid DESC LIMIT 8",
    )
    .fetch_all(db)
    .await
    .map_err(oops)?;

    // Audit B23: decode and re-validate the sampler heartbeat server-side (schema,
    // sample age, lease deadlines) so a dead sampler reads Unknown/stale instead of
    // leaving a frozen READY on the page.
    let raw_health: Option<String> = sqlx::query_scalar::<_, String>(
        "SELECT value FROM pool_status WHERE key='pps_credit_health'",
    )
    .fetch_optional(db)
    .await
    .map_err(oops)?;
    let health: serde_json::Value = serde_json::to_value(crate::credit_health::present(
        true,
        raw_health.as_deref(),
        chrono::Utc::now().timestamp(),
    ))
    .unwrap_or(serde_json::Value::Null);

    let miners = miner_rows
        .iter()
        .map(|r| MinerRow {
            address: r.get("a"),
            pending: r.get("p"),
            paying: r.get("g"),
            paid: r.get("d"),
        })
        .collect();

    let recent_payouts = payout_rows
        .iter()
        .map(|r| PayoutRow {
            txid: r.get("txid"),
            amount: r.get("amount"),
            created_at: r.get("created_at"),
        })
        .collect();

    Ok(Json(PpsData {
        network: state.network.clone(),
        epoch: meta.get("active_epoch"),
        fee_percent: state.pool_fee,
        gross_whole_zat: meta.get("gross_whole"),
        gross_fraction: meta.get("gross_fraction"),
        cap_zat: meta.get("cap_zats"),
        fee_cap_zat: policy.get("fee_cap"),
        total_cap_zat: policy.get("total_cap"),
        reserve_floor_zat: policy.get("reserve_floor"),
        event_count: meta.get("event_count"),
        pending_zat: totals.get("p"),
        paying_zat: totals.get("g"),
        paid_zat: totals.get("d"),
        payout_count: payouts.get("n"),
        payout_total_zat: payouts.get("t"),
        // Config values (not stored per-row in the DB); kept in sync with
        // [pps].max_payout_zatoshis and PPS_SETTLE_MATURITY.
        max_payout_zat: 950_000_000,
        settle_maturity: 10,
        health,
        miners,
        recent_payouts,
    }))
}

/// The /pps dashboard page. Static shell; it fetches /api/pps and renders live.
pub async fn pps_page(State(_state): State<AppState>) -> Html<&'static str> {
    Html(PPS_HTML)
}

const PPS_HTML: &str = r####"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>PPS Ledger Console</title>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Fraunces:opsz,wght@9..144,500;9..144,600&family=IBM+Plex+Mono:wght@400;500;600&family=IBM+Plex+Sans:wght@400;500;600&display=swap">
<style>
  :root {
    --paper:#f4f6f8; --panel:#fff; --sunk:#eef1f4;
    --ink:#1a1f2b; --ink-2:#4c5461; --ink-3:#7b8494; --rule:#dfe3e8;
    --bronze:#a06c1a; --teal:#2a8a86;
    --good:#2f7d4f; --good-soft:#e2f0e8; --owed:#b07714; --owed-soft:#f6ecd8;
    --crit:#ad3a30; --crit-soft:#f7e4e1; --flow-track:#e7eaee;
  }
  @media (prefers-color-scheme:dark){:root:not([data-theme="light"]){
    --paper:#12151b; --panel:#1a1e26; --sunk:#14171d;
    --ink:#e9ecf1; --ink-2:#adb5c2; --ink-3:#767f8e; --rule:#2a2f39;
    --bronze:#d59b3e; --teal:#57b4af;
    --good:#63bd8a; --good-soft:#16301f80; --owed:#d69a3c; --owed-soft:#33281180;
    --crit:#e07a6f; --crit-soft:#3a201c80; --flow-track:#262b34;
  }}
  :root[data-theme="dark"]{
    --paper:#12151b; --panel:#1a1e26; --sunk:#14171d;
    --ink:#e9ecf1; --ink-2:#adb5c2; --ink-3:#767f8e; --rule:#2a2f39;
    --bronze:#d59b3e; --teal:#57b4af;
    --good:#63bd8a; --good-soft:#16301f80; --owed:#d69a3c; --owed-soft:#33281180;
    --crit:#e07a6f; --crit-soft:#3a201c80; --flow-track:#262b34;
  }
  *{box-sizing:border-box}
  body{margin:0;background:var(--paper);color:var(--ink);font-family:"IBM Plex Sans",system-ui,sans-serif;font-size:15px;line-height:1.5}
  .mono{font-family:"IBM Plex Mono",ui-monospace,monospace;font-variant-numeric:tabular-nums}
  h1,h2,h3{font-family:"Fraunces",Georgia,serif;font-weight:600;text-wrap:balance;margin:0}
  a{color:var(--bronze)}
  .wrap{max-width:1080px;margin:0 auto;padding:2rem 1.5rem 4rem}
  header{display:flex;flex-wrap:wrap;align-items:baseline;gap:.5rem 1.5rem;padding-bottom:1rem;border-bottom:2px solid var(--ink)}
  header h1{font-size:clamp(1.7rem,3.5vw,2.3rem);letter-spacing:-.01em}
  .spacer{flex:1}
  .eyebrow{font-size:.72rem;letter-spacing:.13em;text-transform:uppercase;color:var(--ink-3);font-weight:600}
  .pill{display:inline-flex;align-items:center;gap:.4rem;font-size:.78rem;font-weight:600;letter-spacing:.03em;padding:.28rem .7rem;border-radius:999px;color:var(--ink-3);background:var(--sunk)}
  .pill.ready{color:var(--good);background:var(--good-soft)}
  .pill.paused{color:var(--crit);background:var(--crit-soft)}
  .pill.warn{color:var(--owed);background:var(--owed-soft)}
  .pill.degraded{color:var(--owed);background:var(--owed-soft)}
  .health-wrap{display:flex;flex-direction:column;align-items:flex-end;gap:.25rem}
  .health-reason{font-family:"IBM Plex Mono",monospace;font-size:.72rem;color:var(--ink-3);letter-spacing:.02em}
  .health-reason:empty{display:none}
  .pill .dot{width:7px;height:7px;border-radius:50%;background:currentColor}
  .meta-row{display:flex;flex-wrap:wrap;gap:.4rem 2rem;margin-top:.8rem;color:var(--ink-2);font-size:.86rem}
  .meta-row b{color:var(--ink);font-weight:500}
  .live{color:var(--ink-3);font-size:.8rem;display:inline-flex;align-items:center;gap:.4rem}
  .live .rec{width:7px;height:7px;border-radius:50%;background:var(--good);animation:pulse 2s infinite}
  @keyframes pulse{0%,100%{opacity:1}50%{opacity:.35}}
  @media (prefers-reduced-motion:reduce){.live .rec{animation:none}}
  section{margin-top:2rem}
  .sec-head{display:flex;align-items:baseline;gap:.8rem;margin-bottom:.9rem}
  .sec-head h2{font-size:1.15rem}
  .sec-head .note{color:var(--ink-3);font-size:.82rem}
  .panel,.cap,.flow{background:var(--panel);border:1px solid var(--rule);border-radius:10px;padding:1.25rem 1.4rem}
  .cap .top{display:flex;flex-wrap:wrap;align-items:flex-end;justify-content:space-between;gap:.5rem}
  .cap .big{font-family:"Fraunces",serif;font-size:2.1rem;font-weight:600;line-height:1}
  .cap .big small{font-size:.9rem;color:var(--ink-3);font-family:"IBM Plex Mono",monospace;font-weight:400}
  .cap .pct{font-family:"IBM Plex Mono",monospace;font-size:1.5rem;font-weight:600;color:var(--owed)}
  .meter{height:14px;border-radius:7px;background:var(--flow-track);margin:.9rem 0 .5rem;overflow:hidden;display:flex}
  .meter .fill{background:linear-gradient(90deg,var(--bronze),var(--owed))}
  .meter .fee{background:var(--teal);opacity:.55}
  .legend{display:flex;flex-wrap:wrap;gap:.3rem 1.4rem;font-size:.82rem;color:var(--ink-2)}
  .legend span::before{content:"";display:inline-block;width:9px;height:9px;border-radius:2px;margin-right:.4rem;vertical-align:.02em}
  .lg-used::before{background:var(--owed)}.lg-fee::before{background:var(--teal)}.lg-free::before{background:var(--flow-track)}
  .warn-line{margin-top:.7rem;font-size:.84rem;color:var(--owed)}
  .warn-line[hidden]{display:none}
  .flowbar{display:flex;height:46px;border-radius:7px;overflow:hidden;margin:.4rem 0 .9rem}
  .flowbar>div{display:flex;align-items:center;justify-content:center;color:#fff;font-family:"IBM Plex Mono",monospace;font-weight:600;font-size:.8rem;min-width:2px}
  .flowbar .paid{background:var(--good)}.flowbar .paying{background:var(--teal)}.flowbar .owed{background:var(--owed)}
  .flow-keys{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:.9rem}
  .flow-key{border-left:3px solid var(--rule);padding-left:.7rem}
  .flow-key.k-paid{border-color:var(--good)}.flow-key.k-paying{border-color:var(--teal)}.flow-key.k-owed{border-color:var(--owed)}
  .flow-key .v{font-family:"Fraunces",serif;font-size:1.35rem;font-weight:600}
  .flow-key .k{font-size:.78rem;color:var(--ink-2)}
  .conserve{margin-top:.9rem;font-size:.82rem;color:var(--ink-3)}
  .conserve .mono{color:var(--ink-2)}
  .tiles{display:grid;grid-template-columns:repeat(auto-fit,minmax(155px,1fr));gap:1px;background:var(--rule);border:1px solid var(--rule);border-radius:10px;overflow:hidden}
  .tile{background:var(--panel);padding:.95rem 1.05rem}
  .tile .n{font-family:"IBM Plex Mono",monospace;font-size:1.5rem;font-weight:600;line-height:1.05}
  .tile .l{font-size:.76rem;color:var(--ink-2);margin-top:.3rem}
  .tile .s{font-size:.72rem;color:var(--ink-3);margin-top:.15rem}
  .cols{display:grid;grid-template-columns:1.35fr 1fr;gap:1.5rem;align-items:start}
  @media (max-width:800px){.cols{grid-template-columns:1fr}}
  .pipe{display:flex;flex-direction:column}
  .step{display:grid;grid-template-columns:1.6rem 1fr;gap:.8rem;padding:.55rem 0}
  .step .rail{position:relative;display:flex;justify-content:center}
  .step .rail::before{content:"";position:absolute;top:1.4rem;bottom:-.55rem;width:2px;background:var(--rule)}
  .step:last-child .rail::before{display:none}
  .step .marker{width:1.6rem;height:1.6rem;border-radius:50%;background:var(--sunk);border:1.5px solid var(--rule);color:var(--bronze);display:flex;align-items:center;justify-content:center;font-family:"IBM Plex Mono",monospace;font-size:.8rem;font-weight:600;z-index:1}
  .step h3{font-size:.98rem;font-family:"IBM Plex Sans",sans-serif;font-weight:600}
  .step p{margin:.2rem 0 0;font-size:.86rem;color:var(--ink-2)}
  .step .tag{font-family:"IBM Plex Mono",monospace;font-size:.74rem;color:var(--ink-3)}
  .formula{margin-top:1.1rem;background:var(--sunk);border-radius:8px;padding:.9rem 1rem}
  .formula .eq{font-family:"IBM Plex Mono",monospace;font-size:.82rem;line-height:1.7;overflow-x:auto}
  .formula .eq b{color:var(--bronze)}
  .formula p{margin:.6rem 0 0;font-size:.83rem;color:var(--ink-2)}
  .gates{display:flex;flex-direction:column;gap:.7rem}
  .gate{display:flex;align-items:center;gap:.7rem}
  .gate .st{width:9px;height:9px;border-radius:50%;flex:none;background:var(--ink-3)}
  .gate .st.ok{background:var(--good)}.gate .st.warn{background:var(--owed)}.gate .st.bad{background:var(--crit)}
  .gate .gname{font-weight:500}
  .gate .gdesc{color:var(--ink-3);font-size:.8rem}
  .gate .gval{margin-left:auto;font-family:"IBM Plex Mono",monospace;font-size:.82rem;color:var(--ink-2);white-space:nowrap}
  .policy{display:grid;grid-template-columns:1fr auto;gap:.35rem 1rem;margin-top:1.1rem;font-size:.85rem}
  .policy dt{color:var(--ink-2)}
  .policy dd{margin:0;font-family:"IBM Plex Mono",monospace;text-align:right;color:var(--ink)}
  .tablewrap{overflow-x:auto}
  table{width:100%;border-collapse:collapse;font-size:.86rem}
  th,td{text-align:left;padding:.55rem .7rem;border-bottom:1px solid var(--rule)}
  th{font-size:.72rem;letter-spacing:.06em;text-transform:uppercase;color:var(--ink-3);font-weight:600}
  td.num,th.num{text-align:right;font-family:"IBM Plex Mono",monospace;font-variant-numeric:tabular-nums;white-space:nowrap}
  td .addr{font-family:"IBM Plex Mono",monospace;font-size:.8rem}
  tr:last-child td{border-bottom:none}
  .throttle{margin-top:1rem;background:var(--sunk);border-radius:8px;padding:.85rem 1rem;font-size:.85rem;color:var(--ink-2)}
  .throttle b{color:var(--ink)}
  footer{margin-top:2.5rem;padding-top:1rem;border-top:1px solid var(--rule);color:var(--ink-3);font-size:.8rem}
  .good-txt{color:var(--good)}.owed-txt{color:var(--owed)}.crit-txt{color:var(--crit)}
  .navlinks{margin-top:.6rem;font-size:.85rem}
  .navlinks a{margin-right:1rem;text-decoration:none}
</style>
</head>
<body>
<div class="wrap">
  <header>
    <div>
      <p class="eyebrow">Zec Miner · <span id="network">testnet</span></p>
      <h1>PPS Ledger Console</h1>
    </div>
    <div class="spacer"></div>
    <div class="health-wrap">
      <span class="pill" id="health-pill"><span class="dot"></span><span id="health-txt">loading…</span></span>
      <span class="health-reason" id="health-reason"></span>
    </div>
  </header>
  <div class="meta-row">
    <span>Reward mode <b>Pay-Per-Share</b></span>
    <span>Epoch <b class="mono" id="epoch">—</b></span>
    <span>Pool fee <b id="fee">—</b></span>
    <span class="live"><span class="rec"></span><span id="updated">connecting…</span></span>
  </div>
  <div class="navlinks"><a href="/">← Pool</a><a href="/network">Network</a></div>

  <section>
    <div class="sec-head"><h2>Liability cap</h2><span class="note">outstanding promises (owed + in flight) against the pool's variance capital — capacity returns as payouts settle</span></div>
    <div class="cap">
      <div class="top"><div class="big mono"><span id="outstanding">—</span> <small>/ <span id="cap">—</span> TAZ outstanding</small></div><div class="pct" id="cap-pct">—</div></div>
      <div class="meter" role="img" aria-label="liability cap usage"><div class="fill" id="m-used" style="width:0"></div><div class="fee" id="m-fee" style="width:0"></div></div>
      <div class="legend"><span class="lg-used">outstanding to miners · <span id="lg-used">—</span> TAZ</span><span class="lg-fee">tx-fee allowance · <span id="lg-fee">—</span> TAZ</span><span class="lg-free">headroom · <span id="lg-free">—</span> TAZ</span></div>
      <div class="legend" style="margin-top:.35rem"><span>lifetime credited · <span id="gross" class="mono">—</span> TAZ</span></div>
      <div class="warn-line" id="budget-warn" hidden>▲ Budget low — <span id="headroom">—</span> TAZ of liability headroom remains. Crediting pauses only if outstanding reaches the cap; capacity comes back as payouts settle.</div>
    </div>
  </section>

  <section>
    <div class="sec-head"><h2>Where the money is</h2><span class="note">every credited share is conserved across exactly three states</span></div>
    <div class="flow">
      <div class="flowbar" role="img" aria-label="money split"><div class="owed" id="fb-owed" style="width:0"></div><div class="paid" id="fb-paid" style="width:0"></div><div class="paying" id="fb-paying" style="width:0"></div></div>
      <div class="flow-keys">
        <div class="flow-key k-owed"><div class="v mono" id="k-owed">—</div><div class="k">TAZ owed to miners (pending)</div></div>
        <div class="flow-key k-paying"><div class="v mono" id="k-paying">—</div><div class="k">TAZ in flight (a payout sent, awaiting maturity)</div></div>
        <div class="flow-key k-paid"><div class="v mono" id="k-paid">—</div><div class="k">TAZ paid out on-chain (<span id="k-paidn">—</span> payouts)</div></div>
      </div>
      <div class="conserve">Books reconcile exactly: <span class="mono" id="conserve">—</span> credited = the sum of all <span class="mono" id="events2">—</span> priced share-events.</div>
    </div>
  </section>

  <section>
    <div class="tiles">
      <div class="tile"><div class="n" id="events">—</div><div class="l">shares priced &amp; credited</div><div class="s">this epoch</div></div>
      <div class="tile"><div class="n" id="avg">—</div><div class="l">avg TAZ per share</div><div class="s">= subsidy × your diff share</div></div>
      <div class="tile"><div class="n" id="poutn">—</div><div class="l">payouts settled</div><div class="s"><span id="poutt">—</span> TAZ total</div></div>
      <div class="tile"><div class="n" id="minern">—</div><div class="l">active miners</div><div class="s">both paid to shielded</div></div>
      <div class="tile"><div class="n" id="fee2">—</div><div class="l">pool fee</div><div class="s">deducted per share</div></div>
    </div>
  </section>

  <div class="cols">
    <section style="margin-top:2rem">
      <div class="sec-head"><h2>How a share becomes money</h2></div>
      <div class="panel">
        <div class="pipe">
          <div class="step"><div class="rail"><div class="marker">1</div></div><div><h3>Share arrives</h3><p>A miner submits valid proof-of-work against the pool's fixed difficulty‑1000 target.</p><span class="tag">stratum · fixed share target</span></div></div>
          <div class="step"><div class="rail"><div class="marker">2</div></div><div><h3>Priced instantly</h3><p>Quoted against the current block subsidy and network difficulty, minus the fee. A share of difficulty D at network difficulty N is worth <span class="mono">D/N × subsidy</span> — its exact expected value as a block.</p><span class="tag">rewards::pps · exact big-integer math</span></div></div>
          <div class="step"><div class="rail"><div class="marker">3</div></div><div><h3>Credited to the ledger</h3><p>Added to the miner's <b>pending</b> balance in one atomic transaction, counting against the lifetime cap. De-duplicated by proof hash — credited exactly once.</p><span class="tag">pps_events → pps_accounts.pending</span></div></div>
          <div class="step"><div class="rail"><div class="marker">4</div></div><div><h3>Admission gate</h3><p>Every valid share is priced and credited. A live <b>funding lease</b> — fresh proof the wallet holds enough mature shielded notes to cover what is owed — must pass before any payout is sent. While it is stale, shares still credit and payouts wait.</p><span class="tag">credits never wait · sends re-prove funding</span></div></div>
          <div class="step"><div class="rail"><div class="marker">5</div></div><div><h3>Paid out</h3><p>A batch moves <b>pending → paying</b>, is sealed durably, sent via one <span class="mono">z_sendmany</span>, then settles to <b>paid</b> only after 10 confirmations. The durable seal makes a double-pay impossible.</p><span class="tag">reserve · seal · send · settle</span></div></div>
        </div>
        <div class="formula">
          <div class="eq"><b>price</b> = ⌊ (network_target+1) · subsidy · (10000−fee_bps) · 10¹²<br>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;÷ ( (share_target+1) · 10000 ) ⌋ &nbsp;<span style="color:var(--ink-3)">sub-zatoshi</span></div>
          <p>Priced on raw 32-byte targets, floored once — the pool can never over-pay a share, and bears the block-finding variance itself. That's PPS: steady, predictable miner income; the pool takes the luck risk.</p>
        </div>
      </div>
    </section>

    <section style="margin-top:2rem">
      <div class="sec-head"><h2>Health gates</h2></div>
      <div class="panel">
        <div class="gates" id="gates"></div>
        <div class="sec-head" style="margin:1.4rem 0 .6rem"><h2 style="font-size:.98rem">Epoch policy</h2></div>
        <dl class="policy">
          <dt>Credit cap (max liability)</dt><dd><span id="p-cap">—</span> TAZ</dd>
          <dt>Tx-fee allowance</dt><dd><span id="p-fee">—</span> TAZ</dd>
          <dt>Total exposure</dt><dd><span id="p-total">—</span> TAZ</dd>
          <dt>Reserve floor</dt><dd><span id="p-reserve">—</span> TAZ</dd>
          <dt>Max per payout</dt><dd><span id="p-maxpay">—</span> TAZ</dd>
          <dt>Settlement maturity</dt><dd><span id="p-mat">—</span> conf</dd>
        </dl>
      </div>
    </section>
  </div>

  <section>
    <div class="sec-head"><h2>Miners</h2><span class="note">mining to shielded addresses</span></div>
    <div class="panel" style="padding:.4rem .6rem"><div class="tablewrap"><table>
      <thead><tr><th>Address</th><th class="num">Owed</th><th class="num">In flight</th><th class="num">Paid</th></tr></thead>
      <tbody id="miners"></tbody>
    </table></div></div>
  </section>

  <section>
    <div class="sec-head"><h2>Recent payouts</h2><span class="note">live from the pps_payouts ledger</span></div>
    <div class="panel" style="padding:.4rem .6rem"><div class="tablewrap"><table>
      <thead><tr><th>Time (UTC)</th><th>Txid</th><th class="num">Amount</th></tr></thead>
      <tbody id="payouts"></tbody>
    </table></div></div>
    <div class="throttle"><b>Why payouts are gradual.</b> Each batch is capped and settles only after 10 confirmations (~12.5 min), and a new payout can't start until the prior one settles — so payouts arrive one at a time. Raising the per-payout cap and/or lowering the maturity clears a backlog faster.</div>
  </section>

  <footer>Live view of the testnet PPS engine, refreshed from <span class="mono">pool.db</span> every 15 s. PPS is testnet-only; mainnet runs PPLNS.</footer>
</div>

<script>
const $ = id => document.getElementById(id);
const TAZ = z => (Number(z)/1e8);
const f2 = n => n.toLocaleString('en-US',{minimumFractionDigits:2,maximumFractionDigits:2});
const shorttx = t => t ? t.slice(0,16) : '—';
const shortaddr = a => a && a.length>18 ? a.slice(0,8)+'…'+a.slice(-6) : (a||'—');

// Plain-language reasons for the health category shown under the pill.
const REASON = {
  ok:'', funding_missing:'no funding lease yet — shares still credited, payouts held until proven',
  funding_expired:'funding lease lapsed — shares still credited, payouts held until re-proven',
  generation_changed:'a payout just moved funds — re-proving funding (~12s), shares still credited',
  funding_insufficient:'wallet below required cover — shares still credited, payouts held',
  invalid_evidence:'funding evidence rejected — shares still credited, payouts held',
  fee_capacity_exhausted:'payout fee budget spent — shares still credited, payouts held',
  financial_halt:'operator halt — shares still credited, no sends',
  chain_invalid:'chain agreement unproven (node vs reference explorers) — warning only; shares still credited, payouts continue',
  cap_exhausted:'credit cap exhausted — valid shares are being REJECTED',
  current_quote_insufficient:'next share would exceed the cap — valid shares are being REJECTED',
  accounting_invalid:'ledger unreadable — valid shares are being REJECTED',
  concurrent_change:'state changed mid-sample — re-sampling',
  missing:'no health sample published yet', malformed:'health sample unreadable',
  stale:'health sample is stale (sampler not running?)', unknown:'sampler could not determine state',
};
function setPill(state, category){
  const p=$('health-pill'), t=$('health-txt'), r=$('health-reason');
  p.className='pill';
  if(state==='ready'){p.classList.add('ready');t.textContent='ADMISSION READY';}
  else if(state==='degraded'){p.classList.add('degraded');t.textContent='CREDITING · PAYOUTS MAY HOLD';}
  else if(state==='paused'){p.classList.add('paused');t.textContent='ADMISSION PAUSED';}
  else {t.textContent='TELEMETRY UNKNOWN';}
  r.textContent = REASON[category] ?? (category||'');
}
function gate(name,desc,cls,val){
  return `<div class="gate"><span class="st ${cls}"></span><div><div class="gname">${name}</div><div class="gdesc">${desc}</div></div><span class="gval">${val}</span></div>`;
}

async function render(){
  let d;
  try { const r = await fetch('/api/pps',{cache:'no-store'}); if(!r.ok) throw 0; d = await r.json(); }
  catch(e){ $('updated').textContent='offline — retrying'; return; }

  const gross = TAZ(d.gross_whole_zat), cap = TAZ(d.cap_zat), feecap = TAZ(d.fee_cap_zat);
  const total = TAZ(d.total_cap_zat), reserve = TAZ(d.reserve_floor_zat);
  const pending = TAZ(d.pending_zat), paying = TAZ(d.paying_zat), paid = TAZ(d.paid_zat);
  // Refill model: the cap bounds OUTSTANDING liability (owed + in flight), not
  // lifetime credits. Capacity returns as payouts settle.
  const outstanding = pending + paying;
  const capPct = cap>0 ? outstanding/cap*100 : 0;
  const headroom = Math.max(cap-outstanding,0);

  $('network').textContent = d.network;
  $('epoch').textContent = d.epoch;
  $('fee').textContent = d.fee_percent.toFixed(1)+'%';
  $('fee2').textContent = d.fee_percent.toFixed(1)+'%';

  $('outstanding').textContent = f2(outstanding);
  $('gross').textContent = f2(gross);
  $('cap').textContent = f2(cap);
  $('cap-pct').textContent = capPct.toFixed(1)+'%';
  // meter: scale outstanding+fee against total exposure so the fee band reads true
  $('m-used').style.width = (total>0?outstanding/total*100:0)+'%';
  $('m-fee').style.width = (total>0?feecap/total*100:0)+'%';
  $('lg-used').textContent = f2(outstanding);
  $('lg-fee').textContent = f2(feecap);
  $('lg-free').textContent = f2(headroom);
  const low = d.health && d.health.budget_low;
  $('budget-warn').hidden = !low;
  $('headroom').textContent = f2(headroom);
  $('cap-pct').style.color = capPct>=90 ? 'var(--crit)' : (capPct>=75 ? 'var(--owed)' : 'var(--good)');

  const g = Math.max(gross,1e-9);
  $('fb-owed').style.width = pending/g*100+'%';
  $('fb-paid').style.width = paid/g*100+'%';
  $('fb-paying').style.width = paying/g*100+'%';
  $('fb-owed').textContent = 'owed '+Math.round(pending/g*100)+'%';
  $('fb-paid').textContent = paid/g>0.04 ? 'paid' : '';
  $('k-owed').textContent = f2(pending);
  $('k-paying').textContent = f2(paying);
  $('k-paid').textContent = f2(paid);
  $('k-paidn').textContent = d.payout_count;
  // Audit B23: actually compare. Whole-zatoshi account balances may trail the
  // gross total by each account's sub-zatoshi fraction carry, never more.
  const diffZat = Math.abs((d.pending_zat + d.paying_zat + d.paid_zat) - d.gross_whole_zat);
  $('conserve').textContent = f2(pending)+' + '+f2(paying)+' + '+f2(paid)+' = '+f2(pending+paying+paid)+' TAZ '
    + (diffZat <= 1000 ? '✓' : '✗ off by ' + diffZat + ' zat');
  $('events2').textContent = d.event_count.toLocaleString('en-US');

  $('events').textContent = d.event_count.toLocaleString('en-US');
  $('avg').textContent = d.event_count>0 ? (gross/d.event_count).toFixed(4) : '—';
  $('poutn').textContent = d.payout_count;
  $('poutt').textContent = f2(TAZ(d.payout_total_zat));
  $('minern').textContent = d.miners.length;

  // policy
  $('p-cap').textContent = f2(cap); $('p-fee').textContent = f2(feecap);
  $('p-total').textContent = f2(total); $('p-reserve').textContent = f2(reserve);
  $('p-maxpay').textContent = f2(TAZ(d.max_payout_zat)); $('p-mat').textContent = d.settle_maturity;

  // health gates
  const h = d.health || {};
  const now = Math.floor(Date.now()/1000);
  const secs = (end)=> end? Math.max(end-now,0)+' s':'—';
  setPill(h.state, h.category);
  if (Number.isSafeInteger(h.sampled_at_unix)) {
    const age = Math.max(now - h.sampled_at_unix, 0);
    $('health-reason').textContent += ($('health-reason').textContent ? ' · ' : '') + 'sampled ' + age + ' s ago';
  }
  // Admission: ready and degraded both mean valid shares ARE being credited;
  // only paused means they are being rejected.
  const admCls = h.state==='ready'?'ok':(h.state==='degraded'?'warn':(h.state==='paused'?'bad':''));
  const admVal = {ready:'CREDITING', degraded:'CREDITING (degraded)', paused:'REJECTING', unknown:'UNKNOWN'}[h.state] || '—';
  // Funding gates sends, not credits: stale is a warning, never a rejection.
  const fundVal = h.funding_expiry_valid ? 'valid · '+secs(h.funding_expires_at_unix) : 'stale — re-proving';
  // Price telemetry: when the last share was priced (idle is normal between
  // shares and after a new tip), and whether that price fit under the cap.
  const pricedAgo = h.quote_checked_at_unix ? Math.max(now-h.quote_checked_at_unix,0) : null;
  const priceCls = h.current_quote_fits===false ? 'bad' : (pricedAgo!==null && pricedAgo<=30 ? 'ok' : '');
  const priceVal = h.current_quote_fits===false ? 'does NOT fit cap'
    : pricedAgo===null ? 'none priced yet'
    : (h.current_quote_fits ? 'fits · ' : 'idle · ') + 'last priced '+pricedAgo+' s ago';
  $('gates').innerHTML =
    gate('Credit admission','valid shares priced &amp; credited',admCls,admVal)+
    gate('Funding lease','wallet cover — gates payouts, not credits',h.funding_expiry_valid?'ok':'warn',fundVal)+
    gate('Chain agreement','node + references agree — warning only',h.chain_expiry_valid?'ok':'warn',h.chain_expiry_valid?'valid · '+secs(h.chain_expires_at_unix):'UNPROVEN — warning only')+
    gate('Price quote','last priced share vs the cap',priceCls,priceVal)+
    gate('Budget','credit cap headroom',low?'warn':'ok',low?'LOW':'ok');

  // miners
  $('miners').innerHTML = d.miners.map(m=>
    `<tr><td><span class="addr">${shortaddr(m.address)}</span></td><td class="num">${f2(TAZ(m.pending))}</td><td class="num">${f2(TAZ(m.paying))}</td><td class="num">${f2(TAZ(m.paid))}</td></tr>`
  ).join('') || '<tr><td colspan="4" style="color:var(--ink-3)">no miners with a balance</td></tr>';

  // payouts
  $('payouts').innerHTML = d.recent_payouts.map(p=>
    `<tr><td class="mono">${(p.created_at||'').replace('T',' ').slice(5,19)}</td><td><span class="addr">${shorttx(p.txid)}</span></td><td class="num">${f2(TAZ(p.amount))}</td></tr>`
  ).join('') || '<tr><td colspan="3" style="color:var(--ink-3)">no payouts yet</td></tr>';

  $('updated').textContent = 'updated '+new Date().toLocaleTimeString();
}
render();
setInterval(render, 15000);
</script>
</body>
</html>
"####;
