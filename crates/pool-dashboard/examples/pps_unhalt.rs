//! Operator command: release a PPS financial halt after review (audit B10).
//!
//!   pps_unhalt <config.toml>                                   list active halts
//!   pps_unhalt <config.toml> <attempt-id> --reason "<why>"     check only
//!   pps_unhalt <config.toml> <attempt-id> --reason "<why>" --apply
//!
//! Run it from the pool directory so the database URL resolves. A release is
//! written only when all of these hold:
//! - the attempt carries an unreleased halt (a contract halt or an over-bound fee);
//! - the reconciler ran within the last 15 minutes and raised no alert other
//!   than ones naming this attempt;
//! - the PPS exact-accounting invariant passed on that run;
//! - for an over-bound fee, the fee allowance still covers it once settled.
//! The release is journaled (who, why, which reconciler run) and lifts the fence
//! on sends and new credits. It never moves money: a contract-halted attempt
//! stays held for the reversal tool; an over-bound fee settles on the next
//! reconciler pass at the fee actually paid. No credentials are printed.
use std::str::FromStr;
use std::time::Duration;

use anyhow::{bail, ensure, Context, Result};
use pool_db::PoolDb;
use serde::Deserialize;
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

const MAX_RECONCILER_AGE_SECONDS: i64 = 900;

#[derive(Deserialize)]
struct Config {
    database: Database,
}
#[derive(Deserialize)]
struct Database {
    url: String,
}

fn one_line(s: &str, max: usize) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(max).collect()
}

fn mentions(alert: &str, attempt: i64) -> bool {
    let id = attempt.to_string();
    alert.split(|c: char| !c.is_ascii_digit()).any(|token| token == id)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let config_path = args.get(1).context("usage: pps_unhalt <config.toml> [<attempt-id> --reason \"<why>\" [--operator <name>] [--apply]]")?;
    let mut attempt: Option<i64> = None;
    let mut reason: Option<String> = None;
    let mut operator: Option<String> = None;
    let mut apply = false;
    let mut rest = args[2..].iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--apply" => apply = true,
            "--reason" => reason = Some(rest.next().context("--reason needs a value")?.clone()),
            "--operator" => operator = Some(rest.next().context("--operator needs a value")?.clone()),
            other if attempt.is_none() && other.parse::<i64>().is_ok() => attempt = other.parse().ok(),
            other => bail!("unexpected argument {other}"),
        }
    }
    let config: Config = toml::from_str(&std::fs::read_to_string(config_path).context("read config")?)
        .context("parse config")?;
    let options = SqliteConnectOptions::from_str(&config.database.url)?
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(20));
    let db = PoolDb::new(SqlitePoolOptions::new().max_connections(1).connect_with(options).await?);
    db.run_migrations().await.map_err(|e| anyhow::anyhow!("migrations failed: {e}"))?;

    let halts = db.list_active_pps_halts().await.map_err(|e| anyhow::anyhow!("ledger read failed: {e}"))?;
    let Some(attempt) = attempt else {
        if halts.is_empty() {
            println!("no active PPS halt; the fence is {}", if db.pps_financial_halt_active().await? { "UP" } else { "down" });
        }
        for h in &halts {
            println!("attempt {} kind={} category={} excess_fee={} fee_bound={} txid={} status={}",
                h.attempt_id, h.kind.code(), h.category.map_or("-", |c| c.code()),
                h.excess_fee_zatoshis.map_or("-".to_string(), |v| v.to_string()), h.fee_upper_bound_zatoshis,
                h.expected_txid.as_deref().unwrap_or("-"), h.status);
        }
        return Ok(());
    };
    let halt = halts.iter().find(|h| h.attempt_id == attempt).context("that attempt carries no unreleased halt")?;
    println!("halt: attempt {} kind={} category={} excess_fee={} fee_bound={} txid={} status={}",
        halt.attempt_id, halt.kind.code(), halt.category.map_or("-", |c| c.code()),
        halt.excess_fee_zatoshis.map_or("-".to_string(), |v| v.to_string()), halt.fee_upper_bound_zatoshis,
        halt.expected_txid.as_deref().unwrap_or("-"), halt.status);
    let reason = reason.context("--reason is required")?;
    ensure!(!reason.trim().is_empty() && reason.len() <= 1024, "reason must be 1..=1024 bytes");
    let operator = operator.unwrap_or_else(|| format!("{}@{}",
        std::env::var("USER").unwrap_or_else(|_| "unknown".into()),
        std::fs::read_to_string("/etc/hostname").map(|h| h.trim().to_string()).unwrap_or_else(|_| "unknown".into())));

    // Reconciler evidence: a recent pass with nothing open besides this attempt.
    let health: Value = serde_json::from_str(&db.get_pool_status("reconciler_health").await?.map(|(value, _)| value).context("no reconciler_health in pool_status")?)
        .context("reconciler_health is not JSON")?;
    let last_run = health.get("last_run").and_then(Value::as_str).context("reconciler_health.last_run missing")?.to_string();
    let ran_at = chrono::DateTime::parse_from_rfc3339(&last_run).context("reconciler last_run is not RFC 3339")?;
    let age = chrono::Utc::now().timestamp() - ran_at.timestamp();
    println!("reconciler: last run {last_run} ({age} s ago)");
    ensure!((0..=MAX_RECONCILER_AGE_SECONDS).contains(&age), "reconciler pass is too old; wait for the next sweep");
    let alerts: Vec<String> = health.get("alerts").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
    let unrelated: Vec<&String> = alerts.iter().filter(|a| !mentions(a, attempt)).collect();
    for a in &alerts { println!("  alert: {}", one_line(a, 200)); }
    ensure!(unrelated.is_empty(), "{} reconciler alert(s) do not concern attempt {attempt}; resolve them first", unrelated.len());
    let pps_health: Value = serde_json::from_str(&db.get_pool_status("pps_health").await?.map(|(value, _)| value).context("no pps_health in pool_status")?)
        .context("pps_health is not JSON")?;
    ensure!(pps_health.get("invariant_ok").and_then(Value::as_bool) == Some(true), "PPS accounting invariant did not pass on the last reconciler run");
    println!("PPS invariant: ok");

    if let Some(excess) = halt.excess_fee_zatoshis {
        let s = db.pps_funding_snapshot().await.map_err(|e| anyhow::anyhow!("funding snapshot failed: {e}"))?;
        let committed = s.paid_fees_zatoshis.checked_add(s.reserved_fees_zatoshis).and_then(|v| v.checked_add(excess)).context("fee arithmetic overflow")?;
        println!("fee allowance: paid {} + reserved {} + this fee {} = {} of {}", s.paid_fees_zatoshis, s.reserved_fees_zatoshis, excess, committed, s.fee_allowance_zatoshis);
        ensure!(committed <= s.fee_allowance_zatoshis, "the fee allowance cannot absorb this fee; extend the budget first");
    }
    if !apply {
        println!("all checks passed; re-run with --apply to release the halt (operator {operator})");
        return Ok(());
    }
    let kind = db.release_pps_halt(attempt, &operator, &reason, &last_run).await
        .map_err(|e| anyhow::anyhow!("release refused: {e}"))?;
    println!("released {} on attempt {attempt}; fence now {}", kind.code(),
        if db.pps_financial_halt_active().await? { "UP (another halt remains)" } else { "down" });
    match kind {
        pool_db::pps_funding::PpsHaltKind::ContractHalt => println!("the attempt stays held: settle it with the reversal tool"),
        pool_db::pps_funding::PpsHaltKind::ExcessFee => println!("the attempt settles on the next reconciler pass at the fee actually paid"),
    }
    Ok(())
}
