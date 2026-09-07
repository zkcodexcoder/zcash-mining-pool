//! Exact, read-only funding evidence. Errors deliberately contain no RPC body,
//! wallet amount, address, credential or remote error text.
use crate::{RpcError, ZcashRpcClient};
use serde_json::Value;

const MAX_MONEY: i64 = 21_000_000 * 100_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FundingRpcError {
    #[error("funding wallet unavailable")]
    Unavailable,
    #[error("funding wallet evidence invalid")]
    InvalidEvidence,
    #[error("funding wallet dialect is not safely supported")]
    UnsupportedWallet,
}

/// Parse decimal ZEC without passing through f64, rounding, truncation or
/// saturation. Scientific JSON notation is exact too. Fractional zatoshis,
/// signs, whitespace and values above consensus MAX_MONEY are rejected.
pub fn exact_zatoshis(text: &str) -> Result<i64, FundingRpcError> {
    let bad = FundingRpcError::InvalidEvidence;
    if text.is_empty() || text.len() > 64 { return Err(bad); }
    let mut exponent_split = text.split(['e', 'E']);
    let decimal = exponent_split.next().ok_or(bad)?;
    let exponent = match exponent_split.next() {
        None => 0_i32,
        Some(e) => {
            if e.is_empty() || e.len() > 4 { return Err(bad); }
            e.parse::<i32>().map_err(|_| bad)?
        }
    };
    if exponent_split.next().is_some() || !(-32..=32).contains(&exponent) {
        return Err(bad);
    }
    let mut parts = decimal.split('.');
    let whole = parts.next().ok_or(bad)?;
    let fraction = parts.next();
    if parts.next().is_some() || whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.is_some_and(|f| f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()))
    { return Err(bad); }
    let fraction = fraction.unwrap_or("");
    let digits = format!("{whole}{fraction}");
    let significant = digits.trim_start_matches('0');
    if significant.is_empty() { return Ok(0); }
    let shift = 8_i32.checked_add(exponent).and_then(|v| v.checked_sub(fraction.len() as i32)).ok_or(bad)?;
    let integer = if shift < 0 {
        let drop = usize::try_from(-shift).map_err(|_| bad)?;
        if drop >= significant.len() || !significant[significant.len() - drop..].bytes().all(|b| b == b'0') {
            return Err(bad);
        }
        &significant[..significant.len() - drop]
    } else { significant };
    let mut zats = integer.parse::<i64>().map_err(|_| bad)?;
    if shift > 0 {
        for _ in 0..shift { zats = zats.checked_mul(10).ok_or(bad)?; }
    }
    if zats > MAX_MONEY { return Err(bad); }
    Ok(zats)
}

fn zallet_spendable(value: &Value) -> Result<i64, FundingRpcError> {
    let bad = FundingRpcError::InvalidEvidence;
    if value.get("minimum_confirmations").and_then(Value::as_u64) != Some(10) { return Err(bad); }
    let pools = value.get("pools").and_then(Value::as_object).ok_or(bad)?;
    let mut total = 0_i64;
    for (name, pool) in pools {
        if !matches!(name.as_str(), "transparent"|"sapling"|"orchard"|"ironwood") { return Err(bad); }
        let zats = pool.get("valueZat").and_then(Value::as_i64).filter(|n| (0..=MAX_MONEY).contains(n)).ok_or(bad)?;
        if name != "transparent" { total = total.checked_add(zats).filter(|n| *n <= MAX_MONEY).ok_or(bad)?; }
    }
    // Zero pools are omitted by the pinned RPC contract. Missing the pools
    // object itself, or a malformed present pool, is never a default zero.
    Ok(total)
}

fn derived_shielded_source(value: &Value, from: &str) -> Result<(), FundingRpcError> {
    let bad = FundingRpcError::InvalidEvidence;
    if from.is_empty() || from.len() > 2048 { return Err(bad); }
    let sources = value.as_array().filter(|a| a.len() <= 64).ok_or(bad)?;
    let mut matching_derived = 0_usize;
    for source in sources {
        let kind = source.get("source").and_then(Value::as_str).ok_or(bad)?;
        for (field, unified) in [("sapling",false),("unified",true)] {
            let Some(groups) = source.get(field) else { continue; };
            for group in groups.as_array().filter(|a|a.len() <= 1024).ok_or(bad)? {
                for address in group.get("addresses").and_then(Value::as_array).filter(|a|a.len()<=8192).ok_or(bad)? {
                    let address = if unified { address.get("address").and_then(Value::as_str) } else { address.as_str() }.ok_or(bad)?;
                    if address == from {
                        if kind != "mnemonic_seed" { return Err(bad); }
                        matching_derived += 1;
                    }
                }
            }
        }
    }
    if matching_derived == 1 { Ok(()) } else { Err(bad) }
}

fn source_account(value: &Value, from: &str) -> Result<String, FundingRpcError> {
    let bad = FundingRpcError::InvalidEvidence;
    let accounts = value.as_array().filter(|a| a.len()<=1024).ok_or(bad)?;
    let mut found = None;
    for account in accounts {
        let addresses = account.get("addresses").and_then(Value::as_array).filter(|a|a.len()<=8192).ok_or(bad)?;
        let matches = addresses.iter().any(|a| ["ua","sapling"].iter().any(|field| a.get(field).and_then(Value::as_str)==Some(from)));
        if matches {
            let uuid = account.get("account_uuid").and_then(Value::as_str).ok_or(bad)?;
            if uuid.len()!=36 || !uuid.bytes().enumerate().all(|(i,b)| {
                if [8,13,18,23].contains(&i) { b==b'-' } else { b.is_ascii_digit() || (b'a'..=b'f').contains(&b) }
            }) || found.is_some() { return Err(bad); }
            found=Some(uuid.to_string());
        }
    }
    found.ok_or(bad)
}

// Numeric parsing is tested independently, but this aggregate is NOT funding
// evidence: zecd 0.7.0 reports watch-only and mature coinbase under mine.trusted.
#[cfg(test)]
fn zecd_reported_trusted(value: &Value) -> Result<i64, FundingRpcError> {
    let trusted = value.get("mine").and_then(|v| v.get("trusted"))
        .and_then(Value::as_number).ok_or(FundingRpcError::InvalidEvidence)?;
    // arbitrary_precision preserves the original JSON decimal, including
    // numbers whose fractional-zatoshi tail f64 would silently discard.
    exact_zatoshis(&trusted.to_string())
}

fn method_missing(error: &RpcError) -> bool {
    matches!(error, RpcError::JsonRpc(e) if e.code == -32601)
}

fn wallet_ready(value: &Value) -> Result<(), FundingRpcError> {
    let bad = FundingRpcError::InvalidEvidence;
    if value.get("locked").and_then(Value::as_bool) != Some(false) { return Err(bad); }
    let tip = |name: &str| -> Result<(u64, String), FundingRpcError> {
        let tip = value.get(name).ok_or(bad)?;
        let height = tip.get("height").and_then(Value::as_u64).ok_or(bad)?;
        let hash = tip.get("blockhash").and_then(Value::as_str).ok_or(bad)?;
        if height == 0 || height > u32::MAX as u64 || hash.len() != 64
            || !hash.bytes().all(|b| b.is_ascii_hexdigit()) { return Err(bad); }
        Ok((height, hash.to_ascii_lowercase()))
    };
    let node = tip("node_tip")?;
    let wallet = tip("wallet_tip")?;
    if node != wallet || value.get("fully_synced_height").and_then(Value::as_u64) != Some(wallet.0) {
        return Err(bad);
    }
    if let Some(work) = value.get("sync_work_remaining").filter(|v| !v.is_null()) {
        if work.get("unscanned_blocks").and_then(Value::as_u64) != Some(0) { return Err(bad); }
        let progress = work.get("progress").ok_or(bad)?;
        let numerator = progress.get("numerator").and_then(Value::as_u64).ok_or(bad)?;
        let denominator = progress.get("denominator").and_then(Value::as_u64).ok_or(bad)?;
        if numerator != denominator { return Err(bad); }
    }
    Ok(())
}

fn canonical_wallet_tip(value: &Value, canonical_height: u64, canonical_hash: &str)
    -> Result<(), FundingRpcError>
{
    wallet_ready(value)?;
    let bad = FundingRpcError::InvalidEvidence;
    let height = value["wallet_tip"]["height"].as_u64().ok_or(bad)?;
    let wallet_hash = value["wallet_tip"]["blockhash"].as_str().ok_or(bad)?;
    if canonical_height > u32::MAX as u64 || canonical_height < height
        || canonical_height - height > 24 || canonical_hash.len() != 64
        || !canonical_hash.bytes().all(|b| b.is_ascii_hexdigit())
        || !canonical_hash.eq_ignore_ascii_case(wallet_hash)
    { return Err(bad); }
    Ok(())
}

fn unchanged_wallet_anchor(before: &Value, after: &Value) -> Result<(), FundingRpcError> {
    wallet_ready(before)?;
    wallet_ready(after)?;
    if before["wallet_tip"]["height"] != after["wallet_tip"]["height"]
        || !before["wallet_tip"]["blockhash"].as_str().unwrap()
            .eq_ignore_ascii_case(after["wallet_tip"]["blockhash"].as_str().unwrap())
    { return Err(FundingRpcError::InvalidEvidence); }
    Ok(())
}

fn signer_ready(value: &Value, required_until: i64) -> Result<(), FundingRpcError> {
    let bad = FundingRpcError::InvalidEvidence;
    // getwalletinfo's other fields are placeholders in the pinned Zallet;
    // never use its balances. The keystore-backed unlock field is implemented.
    // Require a recognizable response before accepting the documented omission
    // for an unencrypted keystore; malformed/empty data is not "unlocked".
    if required_until <= 0 || !value.is_object()
        || ["walletversion", "txcount", "keypoololdest", "keypoolsize"].iter()
            .any(|field| value.get(field).and_then(Value::as_u64).is_none())
    { return Err(bad); }
    if let Some(until) = value.get("unlocked_until") {
        let until = until.as_u64().ok_or(bad)?;
        if until < required_until as u64 { return Err(bad); }
    }
    Ok(())
}

impl ZcashRpcClient {
    /// Pinned Zallet sync readiness contract. This `locked` flag is a sync
    /// lock, not a signer lock; funding additionally checks signer readiness.
    pub async fn pps_wallet_ready(&self) -> Result<(), FundingRpcError> {
        let value = self.call_raw::<Value>("getwalletstatus", serde_json::json!([])).await
            .map_err(|_| FundingRpcError::Unavailable)?;
        wallet_ready(&value)
    }

    /// Bind the wallet's scanned funding anchor to the existing independently
    /// verified mining node, not merely to the wallet's own possibly-forked
    /// backend. This performs exact same-height hash comparison; hashes are
    /// never returned, formatted into errors or logged.
    pub async fn pps_wallet_ready_on_chain(&self, node: &ZcashRpcClient) -> Result<(), FundingRpcError> {
        let value = self.call_raw::<Value>("getwalletstatus", serde_json::json!([])).await
            .map_err(|_| FundingRpcError::Unavailable)?;
        wallet_ready(&value)?;
        let height = value["wallet_tip"]["height"].as_u64().ok_or(FundingRpcError::InvalidEvidence)?;
        let canonical_height = node.get_block_count().await.map_err(|_| FundingRpcError::Unavailable)?;
        if canonical_height < height || canonical_height - height > 24 { return Err(FundingRpcError::InvalidEvidence); }
        let canonical_hash = node.call_raw::<String>("getblockhash", serde_json::json!([height])).await
            .map_err(|_| FundingRpcError::Unavailable)?;
        canonical_wallet_tip(&value, canonical_height, &canonical_hash)
    }

    /// Read only: require signing authority to remain unlocked through the
    /// funding lease. Never unlock, export keys or retain wallet-info fields.
    pub async fn pps_wallet_signer_ready(&self, required_until: i64) -> Result<(), FundingRpcError> {
        let value = self.call_raw::<Value>("getwalletinfo", serde_json::json!([])).await
            .map_err(|_| FundingRpcError::Unavailable)?;
        signer_ready(&value, required_until)
    }

    /// Exact confirmed shielded funds belonging to the same derived account
    /// as the configured payout source. Pinned Zallet's totalbalance includes
    /// pending/watch-only amounts, so it is NEVER used here. Account/source
    /// metadata stays private to this call; no key operation is performed.
    pub async fn confirmed_spendable_zatoshis(&self, from: &str) -> Result<i64, FundingRpcError> {
        let classify = |e: RpcError| if method_missing(&e) { FundingRpcError::UnsupportedWallet } else { FundingRpcError::Unavailable };
        let sources = self.call_raw::<Value>("listaddresses", serde_json::json!([])).await.map_err(classify)?;
        derived_shielded_source(&sources, from)?;
        let accounts = self.call_raw::<Value>("z_listaccounts", serde_json::json!([true])).await.map_err(classify)?;
        let account = source_account(&accounts, from)?;
        let balance = self.call_raw::<Value>("z_getbalanceforaccount", serde_json::json!([account,10])).await.map_err(classify)?;
        zallet_spendable(&balance)
    }

    /// A balance must belong to one unchanged, fully scanned, canonical
    /// wallet snapshot. Reorg/tip movement during collection invalidates this
    /// attempt; the caller may obtain entirely new evidence on its next run.
    pub async fn confirmed_spendable_zatoshis_on_chain(&self, from: &str, node: &ZcashRpcClient)
        -> Result<i64, FundingRpcError>
    {
        let before = self.call_raw::<Value>("getwalletstatus", serde_json::json!([])).await
            .map_err(|_| FundingRpcError::Unavailable)?;
        wallet_ready(&before)?;
        let spendable = self.confirmed_spendable_zatoshis(from).await?;
        let after = self.call_raw::<Value>("getwalletstatus", serde_json::json!([])).await
            .map_err(|_| FundingRpcError::Unavailable)?;
        unchanged_wallet_anchor(&before, &after)?;
        let height = after["wallet_tip"]["height"].as_u64().ok_or(FundingRpcError::InvalidEvidence)?;
        let canonical_height = node.get_block_count().await.map_err(|_| FundingRpcError::Unavailable)?;
        if canonical_height < height || canonical_height - height > 24 { return Err(FundingRpcError::InvalidEvidence); }
        let hash = node.call_raw::<String>("getblockhash", serde_json::json!([height])).await
            .map_err(|_| FundingRpcError::Unavailable)?;
        canonical_wallet_tip(&after, canonical_height, &hash)?;
        Ok(spendable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Ephemeral loopback mock only: no wallet/node is contacted by these tests.
    async fn mock_rpc(responses: Vec<Value>) -> (ZcashRpcClient, tokio::task::JoinHandle<Vec<Value>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for response in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0_u8; 4096];
                loop {
                    let n = stream.read(&mut buffer).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    assert!(bytes.len() < 65_536);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = std::str::from_utf8(&bytes[..end]).unwrap();
                        let length: usize = headers.lines().find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse().unwrap())
                        }).unwrap();
                        if bytes.len() >= end + 4 + length {
                            requests.push(serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap());
                            break;
                        }
                    }
                }
                let body = response.to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).as_bytes()).await.unwrap();
            }
            requests
        });
        let http = reqwest::Client::builder().no_proxy().timeout(std::time::Duration::from_secs(2)).build().unwrap();
        (ZcashRpcClient::with_transport(&format!("http://{address}"), None, http), task)
    }

    #[test]
    fn exact_decimal_preserves_zatoshis_and_rejects_rounding() {
        for (text, zats) in [("0", 0), ("0.00000001", 1), ("1.00000001", 100_000_001),
            ("1e-8", 1), ("100000000e-16", 1), ("21e6", MAX_MONEY), ("10.000000000", 1_000_000_000)] {
            assert_eq!(exact_zatoshis(text).unwrap(), zats, "{text}");
        }
        for text in ["", "-0", "-1", "+1", " 1", "1 ", "NaN", "inf", ".1", "1.",
            "0.000000001", "10.000000000000001", "1e-9", "21000000.00000001", "1e100", "1e1e1"] {
            assert!(exact_zatoshis(text).is_err(), "{text}");
        }
    }

    #[test]
    fn both_dialects_are_strict_and_never_default_missing_fields_to_zero() {
        assert_eq!(zallet_spendable(&serde_json::json!({"minimum_confirmations":10,"pools":{
            "sapling":{"valueZat":1},"ironwood":{"valueZat":100000000},"transparent":{"valueZat":9900000000_i64}
        }})).unwrap(), 100_000_001);
        assert_eq!(zallet_spendable(&serde_json::json!({"minimum_confirmations":10,"pools":{}})).unwrap(),0);
        for bad in [serde_json::json!({"minimum_confirmations":1,"pools":{}}),
            serde_json::json!({"minimum_confirmations":10,"pools":{"orchard":{"valueZat":1.1}}}),
            serde_json::json!({"minimum_confirmations":10,"pools":{"orchard":{}}})] {
            assert!(zallet_spendable(&bad).is_err());
        }
        let precise: Value = serde_json::from_str(r#"{"mine":{"trusted":1.00000001,"immature":99}}"#).unwrap();
        assert_eq!(zecd_reported_trusted(&precise).unwrap(), 100_000_001);
        let fractional: Value = serde_json::from_str(r#"{"mine":{"trusted":10.000000000000001}}"#).unwrap();
        assert!(zecd_reported_trusted(&fractional).is_err());
        for v in [serde_json::json!({}), serde_json::json!({"private":null}), serde_json::json!({"private":1})] {
            assert!(zallet_spendable(&v).is_err());
        }
        for v in [serde_json::json!({}), serde_json::json!({"mine":{}}), serde_json::json!({"mine":{"trusted":"1"}})] {
            assert!(zecd_reported_trusted(&v).is_err());
        }
    }

    #[test]
    fn fallback_requires_exact_method_not_found_code() {
        for code in [-32603, -32602, -1, -5, -13, -28] {
            let e = RpcError::JsonRpc(crate::types::JsonRpcError { code, message: "suppressed".into(), data: None });
            assert!(!method_missing(&e));
        }
        assert!(method_missing(&RpcError::JsonRpc(crate::types::JsonRpcError {
            code: -32601, message: "suppressed".into(), data: None,
        })));
    }

    #[test]
    fn wallet_readiness_requires_sync_unlocked_and_exact_fully_scanned_tip() {
        let good=serde_json::json!({"locked":false,
            "node_tip":{"height":100,"blockhash":"a".repeat(64)},
            "wallet_tip":{"height":100,"blockhash":"a".repeat(64)},
            "fully_synced_height":100});
        assert!(wallet_ready(&good).is_ok());
        for key in ["locked","node_tip","wallet_tip","fully_synced_height"] {
            let mut bad=good.clone(); bad.as_object_mut().unwrap().remove(key);
            assert!(wallet_ready(&bad).is_err());
        }
        let mut bad=good.clone(); bad["locked"]=serde_json::json!(true);
        assert!(wallet_ready(&bad).is_err());
        let mut bad=good.clone(); bad["wallet_tip"]["height"]=serde_json::json!(99);
        assert!(wallet_ready(&bad).is_err());
        let mut bad=good.clone(); bad["wallet_tip"]["blockhash"]=serde_json::json!("b".repeat(64));
        assert!(wallet_ready(&bad).is_err());
        let mut bad=good.clone(); bad["fully_synced_height"]=serde_json::json!(99);
        assert!(wallet_ready(&bad).is_err());
        let mut bad=good.clone(); bad["sync_work_remaining"]=serde_json::json!({"unscanned_blocks":1,"progress":{"numerator":99,"denominator":100}});
        assert!(wallet_ready(&bad).is_err());
        let mut done=good; done["sync_work_remaining"]=serde_json::json!({"unscanned_blocks":0,"progress":{"numerator":100,"denominator":100}});
        assert!(wallet_ready(&done).is_ok());
    }

    #[test]
    fn wallet_anchor_rejects_advancing_fork_and_stale_or_ahead_tip() {
        let wallet=serde_json::json!({"locked":false,
            "node_tip":{"height":100,"blockhash":"a".repeat(64)},
            "wallet_tip":{"height":100,"blockhash":"a".repeat(64)},
            "fully_synced_height":100});
        assert!(canonical_wallet_tip(&wallet,100,&"a".repeat(64)).is_ok());
        assert!(canonical_wallet_tip(&wallet,124,&"a".repeat(64)).is_ok());
        assert!(canonical_wallet_tip(&wallet,125,&"a".repeat(64)).is_err());
        assert!(canonical_wallet_tip(&wallet,99,&"a".repeat(64)).is_err());
        assert!(canonical_wallet_tip(&wallet,100,&"b".repeat(64)).is_err());
        assert!(canonical_wallet_tip(&wallet,100,"").is_err());
        assert!(unchanged_wallet_anchor(&wallet,&wallet).is_ok());
        let mut changed=wallet.clone();
        changed["wallet_tip"]["blockhash"]=serde_json::json!("b".repeat(64));
        changed["node_tip"]["blockhash"]=serde_json::json!("b".repeat(64));
        assert!(wallet_ready(&changed).is_ok());
        assert!(unchanged_wallet_anchor(&wallet,&changed).is_err());
        changed=wallet.clone();
        for name in ["wallet_tip","node_tip"] { changed[name]["height"]=serde_json::json!(101); }
        changed["fully_synced_height"]=serde_json::json!(101);
        assert!(wallet_ready(&changed).is_ok());
        assert!(unchanged_wallet_anchor(&wallet,&changed).is_err());
    }

    #[test]
    fn signer_lock_is_independent_of_sync_lock_and_covers_entire_lease() {
        let mut info=serde_json::json!({"walletversion":0,"txcount":0,"keypoololdest":0,"keypoolsize":0});
        assert!(signer_ready(&info,160).is_ok()); // Pinned unencrypted contract.
        for until in [serde_json::json!(0),serde_json::json!(159),serde_json::json!(null),serde_json::json!("160")] {
            info["unlocked_until"]=until;
            assert!(signer_ready(&info,160).is_err());
        }
        info["unlocked_until"]=serde_json::json!(160);
        assert!(signer_ready(&info,160).is_ok());
        assert!(signer_ready(&serde_json::json!({}),160).is_err());
        assert!(signer_ready(&info,0).is_err());
    }

    #[tokio::test]
    async fn runtime_anchor_uses_exact_wallet_height_and_never_returns_hashes() {
        for canonical in ["a","b"] {
            let (wallet, wallet_requests)=mock_rpc(vec![serde_json::json!({"id":1,"error":null,"result":{
                "locked":false,"node_tip":{"height":100,"blockhash":"a".repeat(64)},
                "wallet_tip":{"height":100,"blockhash":"a".repeat(64)},"fully_synced_height":100}})]).await;
            let (node, node_requests)=mock_rpc(vec![
                serde_json::json!({"id":1,"error":null,"result":102}),
                serde_json::json!({"id":2,"error":null,"result":canonical.repeat(64)})]).await;
            let result=wallet.pps_wallet_ready_on_chain(&node).await;
            assert_eq!(result.is_ok(),canonical=="a");
            if let Err(error)=result { assert!(!error.to_string().contains(&canonical.repeat(64))); }
            assert_eq!(wallet_requests.await.unwrap()[0]["method"],"getwalletstatus");
            let requests=node_requests.await.unwrap();
            assert_eq!(requests[0]["method"],"getblockcount");
            assert_eq!(requests[1]["method"],"getblockhash");
            assert_eq!(requests[1]["params"],serde_json::json!([100]));
        }
    }

    #[tokio::test]
    async fn runtime_read_excludes_watch_only_and_rejects_unsupported_dialect() {
        let from="synthetic-shielded-source";
        let uuid="00000000-0000-4000-8000-000000000001";
        let (rpc, requests) = mock_rpc(vec![
            serde_json::json!({"id":1,"result":[{"source":"mnemonic_seed","sapling":[{"addresses":[from]}]}],"error":null}),
            serde_json::json!({"id":2,"result":[{"account_uuid":uuid,"addresses":[{"sapling":from}]}],"error":null}),
            serde_json::json!({"id":3,"result":{"minimum_confirmations":10,"pools":{"sapling":{"valueZat":100000001}}},"error":null})
        ]).await;
        assert_eq!(rpc.confirmed_spendable_zatoshis(from).await.unwrap(), 100_000_001);
        let requests = requests.await.unwrap();
        assert_eq!(requests[0]["method"], "listaddresses");
        assert_eq!(requests[1]["method"], "z_listaccounts");
        assert_eq!(requests[1]["params"], serde_json::json!([true]));
        assert_eq!(requests[2]["method"], "z_getbalanceforaccount");
        assert_eq!(requests[2]["params"], serde_json::json!([uuid,10]));

        let (rpc, requests) = mock_rpc(vec![
            serde_json::json!({"id":1,"result":null,"error":{"code":-32601,"message":"suppressed"}}),
        ]).await;
        assert_eq!(rpc.confirmed_spendable_zatoshis(from).await, Err(FundingRpcError::UnsupportedWallet));
        let requests = requests.await.unwrap();
        assert_eq!(requests.len(), 1);
    }

    #[tokio::test]
    async fn funding_balance_is_bracketed_by_an_unchanged_canonical_wallet_anchor() {
        let from="synthetic-shielded-source";
        let uuid="00000000-0000-4000-8000-000000000001";
        for changed in [false,true] {
            let status=|hash: &str| serde_json::json!({"locked":false,
                "node_tip":{"height":100,"blockhash":hash.repeat(64)},
                "wallet_tip":{"height":100,"blockhash":hash.repeat(64)},"fully_synced_height":100});
            let (wallet, requests)=mock_rpc(vec![
                serde_json::json!({"id":1,"result":status("a"),"error":null}),
                serde_json::json!({"id":2,"result":[{"source":"mnemonic_seed","sapling":[{"addresses":[from]}]}],"error":null}),
                serde_json::json!({"id":3,"result":[{"account_uuid":uuid,"addresses":[{"sapling":from}]}],"error":null}),
                serde_json::json!({"id":4,"result":{"minimum_confirmations":10,"pools":{"sapling":{"valueZat":100000001}}},"error":null}),
                serde_json::json!({"id":5,"result":status(if changed {"b"} else {"a"}),"error":null}),
            ]).await;
            let responses=if changed {vec![]} else {vec![
                serde_json::json!({"id":1,"result":100,"error":null}),
                serde_json::json!({"id":2,"result":"a".repeat(64),"error":null}),
            ]};
            let (node,node_requests)=mock_rpc(responses).await;
            let result=wallet.confirmed_spendable_zatoshis_on_chain(from,&node).await;
            if changed { assert_eq!(result,Err(FundingRpcError::InvalidEvidence)); }
            else { assert_eq!(result,Ok(100000001)); }
            let requests=requests.await.unwrap();
            assert_eq!(requests.iter().map(|v|v["method"].as_str().unwrap()).collect::<Vec<_>>(),
                vec!["getwalletstatus","listaddresses","z_listaccounts","z_getbalanceforaccount","getwalletstatus"]);
            assert_eq!(node_requests.await.unwrap().len(),if changed {0} else {2});
        }
    }

    #[tokio::test]
    async fn runtime_rpc_error_or_malformed_balance_is_not_a_dialect_probe() {
        for response in [
            serde_json::json!({"id":1,"result":null,"error":{"code":-13,"message":"private remote detail"}}),
            serde_json::json!({"id":1,"result":{"private":"1.000000001"},"error":null}),
            serde_json::json!({"id":1,"result":{},"error":null}),
        ] {
            let (rpc, requests) = mock_rpc(vec![response]).await;
            let err = rpc.confirmed_spendable_zatoshis("synthetic-shielded-source").await.unwrap_err();
            assert!(!err.to_string().contains("private remote detail"));
            assert_eq!(requests.await.unwrap().len(), 1);
        }
    }

    #[test]
    fn source_must_be_derived_shielded_and_resolve_one_account() {
        let from="synthetic-shielded-source";
        let derived=serde_json::json!([{"source":"mnemonic_seed","unified":[{"addresses":[{"address":from}]}]}]);
        assert!(derived_shielded_source(&derived,from).is_ok());
        let watched=serde_json::json!([{"source":"imported_watchonly","unified":[{"addresses":[{"address":from}]}]}]);
        assert!(derived_shielded_source(&watched,from).is_err());
        assert!(derived_shielded_source(&derived,"other-source").is_err());
        let uuid="00000000-0000-4000-8000-000000000001";
        let account=serde_json::json!({"account_uuid":uuid,"addresses":[{"ua":from}]});
        assert_eq!(source_account(&serde_json::json!([account.clone()]),from).unwrap(),uuid);
        assert!(source_account(&serde_json::json!([account.clone(),account]),from).is_err());
        assert!(source_account(&serde_json::json!([]),from).is_err());
    }
}
