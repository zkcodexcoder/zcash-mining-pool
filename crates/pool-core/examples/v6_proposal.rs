//! Offline validation for v6 coinbase-tag injection.
//!
//! Fetches a live template, injects the tag through the production code path,
//! assembles a full block (zeroed nonce/solution), and submits it to the node
//! as a `getblocktemplate` proposal. Proposal mode validates consensus rules
//! except PoW, so a `null` response proves the injected coinbase + recomputed
//! blockcommitments are consensus-valid WITHOUT touching the live pool.

use serde_json::json;

fn reverse_hex(h: &str) -> String {
    let mut b = hex::decode(h).expect("hex");
    b.reverse();
    hex::encode(b)
}

fn varint(n: usize) -> Vec<u8> {
    match n {
        0..=0xfc => vec![n as u8],
        0xfd..=0xffff => {
            let mut v = vec![0xfd];
            v.extend_from_slice(&(n as u16).to_le_bytes());
            v
        }
        _ => {
            let mut v = vec![0xfe];
            v.extend_from_slice(&(n as u32).to_le_bytes());
            v
        }
    }
}

#[tokio::main]
async fn main() {
    let rpc = node_rpc::ZcashRpcClient::new("http://operational-host.invalid:18232");
    let tpl: serde_json::Value = rpc
        .call_raw(
            "getblocktemplate",
            json!([{"capabilities": ["coinbasetxn"], "mode": "template"}]),
        )
        .await
        .expect("template");

    let cb_hex = tpl["coinbasetxn"]["data"].as_str().expect("coinbase data");
    let dr = &tpl["defaultroots"];
    let chain_history_root = dr["chainhistoryroot"].as_str().expect("chainhistoryroot");
    let tx_auth_digests: Vec<String> = tpl["transactions"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|t| t["authdigest"].as_str().expect("authdigest").to_string())
        .collect();

    println!(
        "template height={} txs={} coinbase v={}",
        tpl["height"],
        tx_auth_digests.len(),
        &cb_hex[..8]
    );

    // 1. Inject through the PRODUCTION path.
    let result = pool_core::coinbase::inject_coinbase_tag(
        cb_hex,
        b"zkclaudecoder",
        chain_history_root,
        &tx_auth_digests,
    )
    .expect("injection failed");
    let new_bc = result
        .new_block_commitments
        .clone()
        .expect("v6 must return blockcommitments");
    println!("injected ok; new blockcommitments={new_bc}");

    // 2. Assemble the block: header + txs.
    let mut block: Vec<u8> = Vec::new();
    let version = tpl["version"].as_u64().expect("version") as u32;
    block.extend_from_slice(&version.to_le_bytes());
    block.extend_from_slice(&hex::decode(reverse_hex(tpl["previousblockhash"].as_str().unwrap())).unwrap());
    block.extend_from_slice(&hex::decode(reverse_hex(dr["merkleroot"].as_str().unwrap())).unwrap());
    let mut bc_bytes = hex::decode(reverse_hex(&new_bc)).unwrap();
    if std::env::var_os("CORRUPT_BC").is_some() {
        bc_bytes[0] ^= 0xff; // negative control: break the commitment
    }
    block.extend_from_slice(&bc_bytes);
    let curtime = tpl["curtime"].as_u64().expect("curtime") as u32;
    block.extend_from_slice(&curtime.to_le_bytes());
    block.extend_from_slice(&hex::decode(reverse_hex(tpl["bits"].as_str().unwrap())).unwrap());
    block.extend_from_slice(&[0u8; 32]); // nonce
    block.extend_from_slice(&varint(1344)); // solution length (fd4005)
    block.extend_from_slice(&[0u8; 1344]); // zero solution (proposal mode skips PoW)

    let txs = tpl["transactions"].as_array().cloned().unwrap_or_default();
    block.extend_from_slice(&varint(1 + txs.len()));
    block.extend_from_slice(&hex::decode(&result.new_coinbase_hex).unwrap());
    for t in &txs {
        block.extend_from_slice(&hex::decode(t["data"].as_str().expect("tx data")).unwrap());
    }

    // 3. Submit as proposal.
    let resp: serde_json::Value = rpc
        .call_raw(
            "getblocktemplate",
            json!([{"mode": "proposal", "data": hex::encode(&block)}]),
        )
        .await
        .unwrap_or_else(|e| json!({"rpc_error": e.to_string()}));
    println!("PROPOSAL RESPONSE: {resp}");
    // null => valid; a string names the rejection reason.
}
