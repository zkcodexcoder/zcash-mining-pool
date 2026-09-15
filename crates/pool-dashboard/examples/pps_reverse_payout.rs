//! Operator command: reverse a PPS payout whose transaction can never confirm
//! (audit B2 reversal / B4). Two cases, decided from the ledger:
//! - a SETTLED payout (paid, pps_payouts rows): the amounts return from paid to
//!   pending and are journaled in pps_payout_reversals;
//! - a SENT, unsettled payout (sealed with an observed transaction, still
//!   paying): the claims return to pending and the reservation is released,
//!   journaled in pps_released_sends.
//!
//!   pps_reverse_payout <config.toml> <attempt-id>                      check only
//!   pps_reverse_payout <config.toml> <attempt-id> --apply [--operator x]
//!
//! Run it from the pool directory so the database URL resolves. It acts only when:
//! - the node no longer has the transaction (not found) or shows it unconfirmed;
//! - the transaction is not in the node's mempool;
//! - the payout wallet reports it CONFLICTED (negative confirmations), i.e. its
//!   notes were spent elsewhere and it can never confirm;
//! - the attempt was last updated at least an hour ago.
//! A later payout round pays the returned balances. No credentials are printed.
use std::str::FromStr;
use std::time::Duration;

use anyhow::{bail, ensure, Context, Result};
use node_rpc::ZcashRpcClient;
use pool_db::PoolDb;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

const MIN_AGE_SECONDS: i64 = 3600;

#[derive(Deserialize)]
struct Config {
    database: Database,
    node: Node,
    payout: Payout,
}
#[derive(Deserialize)]
struct Database {
    url: String,
}
#[derive(Deserialize)]
struct Node {
    rpc_url: String,
    rpc_user: Option<String>,
    rpc_password: Option<String>,
}
#[derive(Deserialize)]
struct Payout {
    wallet_rpc_url: Option<String>,
    wallet_rpc_user: Option<String>,
    wallet_rpc_password: Option<String>,
}

fn client(url: &str, user: &Option<String>, password: &Option<String>) -> ZcashRpcClient {
    match (user, password) {
        (Some(u), Some(p)) => ZcashRpcClient::with_auth(url, u, p),
        _ => ZcashRpcClient::new(url),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let config_path = args.get(1).context("usage: pps_reverse_payout <config.toml> <attempt-id> [--operator <name>] [--apply]")?;
    let attempt: i64 = args.get(2).context("attempt id required")?.parse().context("attempt id must be a number")?;
    let mut operator: Option<String> = None;
    let mut apply = false;
    let mut rest = args[3..].iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--apply" => apply = true,
            "--operator" => operator = Some(rest.next().context("--operator needs a value")?.clone()),
            other => bail!("unexpected argument {other}"),
        }
    }
    let operator = operator.unwrap_or_else(|| format!("{}@{}",
        std::env::var("USER").unwrap_or_else(|_| "unknown".into()),
        std::fs::read_to_string("/etc/hostname").map(|h| h.trim().to_string()).unwrap_or_else(|_| "unknown".into())));
    let config: Config = toml::from_str(&std::fs::read_to_string(config_path).context("read config")?)
        .context("parse config")?;
    let wallet_url = config.payout.wallet_rpc_url.as_deref().context("no [payout] wallet_rpc_url")?;
    let wallet = client(wallet_url, &config.payout.wallet_rpc_user, &config.payout.wallet_rpc_password);
    let node = client(&config.node.rpc_url, &config.node.rpc_user, &config.node.rpc_password);
    let options = SqliteConnectOptions::from_str(&config.database.url)?
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(20));
    let db = PoolDb::new(SqlitePoolOptions::new().max_connections(1).connect_with(options).await?);

    let held = db.get_pps_conventional_attempt(attempt).await
        .map_err(|e| anyhow::anyhow!("ledger read failed: {e}"))?
        .context("no conventional PPS attempt with that id")?;
    let (status, _created_at, updated_at) = db.get_pps_payout_attempt_status(attempt).await
        .map_err(|e| anyhow::anyhow!("ledger read failed: {e}"))?
        .context("no payout attempt with that id")?;
    let settled = db.get_pps_settled_payouts(attempt).await.map_err(|e| anyhow::anyhow!("ledger read failed: {e}"))?;
    println!("attempt {attempt}: status={status} reservation={} sealed={} transaction_observed={} settled_rows={} updated_at={updated_at}",
        held.status, held.sealed, held.expected_txid.is_some(), settled.len());
    let (case, txid, total): (&str, String, i64) = if !settled.is_empty() {
        ensure!(status == "confirmed", "settled rows exist but the attempt is not confirmed; review by hand");
        ensure!(settled.iter().all(|p| !p.reversed), "this payout was already reversed");
        let txid = settled[0].txid.clone();
        ensure!(settled.iter().all(|p| p.txid == txid), "settled rows disagree on the transaction; review by hand");
        ("settled", txid, settled.iter().map(|p| p.amount_zatoshis).sum())
    } else {
        ensure!(held.sealed && held.status == "reserved", "not a settled payout and not a sealed send; nothing to reverse");
        let txid = held.expected_txid.clone().context("sealed send without an observed transaction: use pps_release_failed_send")?;
        ("sent-unsettled", txid, 0)
    };
    println!("case: {case}; transaction {txid}");
    for p in &settled {
        println!("  miner {} amount {} zatoshis", p.miner_id, p.amount_zatoshis);
    }

    // 1. The node no longer carries the transaction (or shows it unconfirmed).
    let node_view = match node.get_raw_transaction(&txid, 1).await {
        Ok(raw) => {
            let confirmations = raw.get("confirmations").and_then(Value::as_i64).unwrap_or(0);
            ensure!(confirmations < 1, "the node still shows the transaction with {confirmations} confirmations; not reversible");
            "unmined".to_string()
        }
        Err(e) if e.is_definitely_not_found() => "absent".to_string(),
        Err(e) => bail!("node transaction lookup unavailable: {e}"),
    };
    let mempool: Vec<String> = node.call_raw("getrawmempool", json!([])).await
        .map_err(|e| anyhow::anyhow!("node mempool unavailable: {e}"))?;
    ensure!(!mempool.iter().any(|t| *t == txid), "the transaction is in the node's mempool; not reversible");
    let height: u64 = node.call_raw("getblockcount", json!([])).await.map_err(|e| anyhow::anyhow!("node height unavailable: {e}"))?;
    println!("node: transaction {node_view}; mempool {} transactions; height {height}", mempool.len());

    // 2. The wallet says the transaction conflicted: its notes were spent elsewhere.
    let wallet_tx = wallet.zecd_conventional_rpc("gettransaction", json!([txid])).await
        .map_err(|e| anyhow::anyhow!("the wallet cannot report the transaction: {e}"))?;
    let wallet_confirmations = wallet_tx.get("confirmations").and_then(Value::as_i64).context("wallet transaction has no confirmations field")?;
    println!("wallet: confirmations {wallet_confirmations}");
    ensure!(wallet_confirmations < 0, "the wallet does not report the transaction conflicted; not reversible");

    // 3. Old enough that nothing is still in flight.
    let updated = chrono::NaiveDateTime::parse_from_str(&updated_at, "%Y-%m-%d %H:%M:%S").context("attempt timestamp unparsable")?;
    let age = chrono::Utc::now().timestamp() - updated.and_utc().timestamp();
    println!("attempt age: {age} s");
    ensure!(age >= MIN_AGE_SECONDS, "attempt updated less than {MIN_AGE_SECONDS} s ago; try again later");

    let evidence = json!({
        "txid": txid, "node": node_view, "node_height": height, "mempool_transactions": mempool.len(),
        "wallet_confirmations": wallet_confirmations, "attempt_age_seconds": age,
        "checked_at_unix": chrono::Utc::now().timestamp(),
    }).to_string();
    if !apply {
        println!("all checks passed; re-run with --apply to reverse (operator {operator})");
        return Ok(());
    }
    db.run_migrations().await.map_err(|e| anyhow::anyhow!("migrations failed: {e}"))?;
    let returned = if case == "settled" {
        db.reverse_conflicted_pps_payout(attempt, &txid, &operator, &evidence).await
            .map_err(|e| anyhow::anyhow!("reversal refused: {e}"))?
    } else {
        db.release_conflicted_conventional_send(attempt, &txid, &evidence).await
            .map_err(|e| anyhow::anyhow!("release refused: {e}"))?
    };
    println!("returned {returned} zatoshis to pending{}; a later payout round pays them",
        if total > 0 { format!(" (settled total was {total})") } else { String::new() });
    Ok(())
}
