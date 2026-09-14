//! Operator command: release a held testnet PPS payout whose zecd send failed
//! before building any transaction (for example over zecd's transaction size limit).
//!
//!   pps_release_failed_send <config.toml> <attempt-id>            check only
//!   pps_release_failed_send <config.toml> <attempt-id> --apply    release
//!
//! Run it from the pool directory so the database URL resolves. It releases only
//! when all of these hold:
//! - the ledger shows the attempt sealed and unsettled, with a recorded wallet
//!   operation and no observed transaction (re-checked inside the release);
//! - zecd reports that operation `failed` with no transaction id;
//! - the operation is at least 10 minutes old;
//! - no transaction in the node's mempool is known to the wallet.
//! The miners' claims return to pending and a later payout round pays them.
//! No credentials are printed.
use std::str::FromStr;
use std::time::Duration;

use anyhow::{bail, ensure, Context, Result};
use node_rpc::ZcashRpcClient;
use pool_db::PoolDb;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

const MIN_AGE_SECONDS: i64 = 600;

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

fn one_line(s: &str, max: usize) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(max).collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (config_path, attempt, apply) = match args.as_slice() {
        [_, c, a] => (c, a, false),
        [_, c, a, flag] if flag == "--apply" => (c, a, true),
        _ => bail!("usage: pps_release_failed_send <config.toml> <attempt-id> [--apply]"),
    };
    let attempt: i64 = attempt.parse().context("attempt id must be a number")?;
    let config: Config = toml::from_str(&std::fs::read_to_string(config_path).context("read config")?)
        .context("parse config")?;
    let wallet_url = config.payout.wallet_rpc_url.as_deref().context("no [payout] wallet_rpc_url")?;
    let wallet = client(wallet_url, &config.payout.wallet_rpc_user, &config.payout.wallet_rpc_password);
    let node = client(&config.node.rpc_url, &config.node.rpc_user, &config.node.rpc_password);
    let options = SqliteConnectOptions::from_str(&config.database.url)?
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(20));
    let db = PoolDb::new(SqlitePoolOptions::new().max_connections(1).connect_with(options).await?);

    let held = db
        .get_pps_conventional_attempt(attempt)
        .await
        .map_err(|e| anyhow::anyhow!("ledger read failed: {e}"))?
        .context("no conventional PPS attempt with that id")?;
    println!(
        "attempt {attempt}: reservation={} sealed={} operation_recorded={} transaction_observed={}",
        held.status, held.sealed, held.operation_id.is_some(), held.expected_txid.is_some()
    );
    let opid = held.operation_id.clone().context("no wallet operation recorded; nothing to release")?;
    ensure!(
        held.sealed && held.status == "reserved" && held.expected_txid.is_none(),
        "not a sealed send without an observed transaction; not releasable"
    );

    // 1. The wallet reports the operation failed, with no transaction id.
    let status = wallet
        .zecd_conventional_rpc("z_getoperationstatus", json!([[opid]]))
        .await
        .context("wallet operation status unavailable")?;
    let ops = status.as_array().context("unexpected operation status shape")?;
    ensure!(ops.len() == 1, "the wallet does not report exactly one operation with that id");
    let op = &ops[0];
    let wallet_status = op.get("status").and_then(Value::as_str).unwrap_or("");
    let txids = op.pointer("/result/txids").and_then(Value::as_array).map_or(0, Vec::len);
    let error = op.pointer("/error/message").and_then(Value::as_str).unwrap_or("");
    let created = op.get("creation_time").and_then(Value::as_i64).context("operation has no creation time")?;
    println!("wallet operation: status={wallet_status} txids={txids} error={}", one_line(error, 200));
    ensure!(
        wallet_status == "failed" && txids == 0,
        "the wallet operation did not fail without a transaction; not releasable"
    );

    // 2. Old enough that it cannot still change.
    let age = chrono::Utc::now().timestamp() - created;
    println!("operation age: {age} s");
    ensure!(age >= MIN_AGE_SECONDS, "operation is younger than {MIN_AGE_SECONDS} s; try again later");

    // 3. No mempool transaction belongs to the wallet.
    let mempool: Vec<String> = node
        .call_raw("getrawmempool", json!([]))
        .await
        .map_err(|e| anyhow::anyhow!("node mempool unavailable: {e}"))?;
    let mut wallet_known = 0;
    for txid in &mempool {
        if wallet.zecd_conventional_rpc("gettransaction", json!([txid])).await.is_ok() {
            wallet_known += 1;
        }
    }
    println!("node mempool: {} transactions, {} known to the wallet", mempool.len(), wallet_known);
    ensure!(wallet_known == 0, "a mempool transaction belongs to the wallet; not releasable");

    let evidence = json!({
        "wallet_status": wallet_status,
        "wallet_error": one_line(error, 1000),
        "operation_created_unix": created,
        "checked_at_unix": chrono::Utc::now().timestamp(),
        "mempool_transactions": mempool.len(),
        "mempool_wallet_transactions": wallet_known,
    })
    .to_string();
    if !apply {
        println!("all checks passed; re-run with --apply to release");
        return Ok(());
    }
    db.run_migrations().await.map_err(|e| anyhow::anyhow!("migrations failed: {e}"))?;
    let released = db
        .release_failed_conventional_send(attempt, &opid, &evidence)
        .await
        .map_err(|e| anyhow::anyhow!("release refused: {e}"))?;
    println!("released {released} zatoshis back to pending; a later payout round pays them");
    Ok(())
}
