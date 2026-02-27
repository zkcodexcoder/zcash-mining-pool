use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::types::*;

/// Default timeout for RPC calls so a hung node cannot stall the pool.
const RPC_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON-RPC error: {0}")]
    JsonRpc(JsonRpcError),
    #[error("Null result from RPC")]
    NullResult,
}

/// Client for communicating with a Zcash node (zcashd or zebrad) via JSON-RPC.
pub struct ZcashRpcClient {
    http: reqwest::Client,
    url: String,
    auth: Option<(String, String)>,
    next_id: AtomicU64,
}

impl ZcashRpcClient {
    fn default_http_client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(RPC_TIMEOUT)
            .build()
            .expect("reqwest client build")
    }

    pub fn new(url: &str) -> Self {
        Self {
            http: Self::default_http_client(),
            url: url.to_string(),
            auth: None,
            next_id: AtomicU64::new(1),
        }
    }

    pub fn with_auth(url: &str, user: &str, password: &str) -> Self {
        Self {
            http: Self::default_http_client(),
            url: url.to_string(),
            auth: Some((user.to_string(), password.to_string())),
            next_id: AtomicU64::new(1),
        }
    }

    /// Generic RPC call returning a typed result. Public for health checks.
    pub async fn call_raw<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T, RpcError> {
        self.call(method, params).await
    }

    async fn call<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T, RpcError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = JsonRpcRequest {
            jsonrpc: "1.0",
            id,
            method,
            params,
        };

        let mut req = self.http.post(&self.url).json(&request);
        if let Some((ref user, ref pass)) = self.auth {
            req = req.basic_auth(user, Some(pass));
        }

        let http_resp = req.send().await?;
        let body = http_resp.text().await?;

        let resp: JsonRpcResponse<T> =
            serde_json::from_str(&body).map_err(|_| RpcError::NullResult)?;

        if let Some(err) = resp.error {
            return Err(RpcError::JsonRpc(err));
        }

        resp.result.ok_or(RpcError::NullResult)
    }

    /// Fetch a block template for mining.
    pub async fn get_block_template(&self) -> Result<BlockTemplate, RpcError> {
        let params = serde_json::json!([{
            "capabilities": ["coinbasetxn", "workid"],
            "mode": "template"
        }]);
        self.call("getblocktemplate", params).await
    }

    /// Submit a solved block to the network.
    /// Returns Ok(None) on success, Ok(Some(reason)) on rejection.
    pub async fn submit_block(&self, block_hex: &str) -> Result<Option<String>, RpcError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = JsonRpcRequest {
            jsonrpc: "1.0",
            id,
            method: "submitblock",
            params: serde_json::json!([block_hex]),
        };

        let mut req = self.http.post(&self.url).json(&request);
        if let Some((ref user, ref pass)) = self.auth {
            req = req.basic_auth(user, Some(pass));
        }

        let body = req.send().await?.text().await?;

        tracing::info!(raw_response = %body, "submitblock response");

        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|_| RpcError::NullResult)?;

        if let Some(err) = parsed.get("error").and_then(|e| {
            if e.is_null() { None } else { Some(e.clone()) }
        }) {
            return Err(RpcError::JsonRpc(JsonRpcError {
                code: err.get("code").and_then(|c| c.as_i64()).unwrap_or(-1),
                message: err.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string(),
                data: None,
            }));
        }

        match parsed.get("result") {
            Some(serde_json::Value::Null) | None => Ok(None),
            Some(serde_json::Value::String(s)) if s.is_empty() => Ok(None),
            Some(serde_json::Value::String(s)) => Ok(Some(s.clone())),
            Some(other) => Ok(Some(other.to_string())),
        }
    }

    /// Validate a block using proposal mode (returns detailed rejection reason).
    pub async fn validate_block_proposal(&self, block_hex: &str) -> Result<Option<String>, RpcError> {
        let params = serde_json::json!([{
            "mode": "proposal",
            "data": block_hex
        }]);
        self.call("getblocktemplate", params).await
    }

    /// Get the current block count (height of the best chain).
    pub async fn get_block_count(&self) -> Result<BlockCount, RpcError> {
        self.call("getblockcount", serde_json::json!([])).await
    }

    /// Get the hash of the best (tip) block.
    pub async fn get_best_block_hash(&self) -> Result<BestBlockHash, RpcError> {
        self.call("getbestblockhash", serde_json::json!([])).await
    }

    /// Get general node info.
    pub async fn get_info(&self) -> Result<NodeInfo, RpcError> {
        self.call("getinfo", serde_json::json!([])).await
    }

    /// Get the block hash at a given height.
    pub async fn get_block_hash(&self, height: u64) -> Result<String, RpcError> {
        self.call("getblockhash", serde_json::json!([height])).await
    }

    /// Get a block by hash or height with the specified verbosity level.
    /// verbosity=0: hex-encoded data, 1: JSON object, 2: JSON with full tx objects.
    pub async fn get_block(&self, hash_or_height: &str, verbosity: u8) -> Result<serde_json::Value, RpcError> {
        self.call("getblock", serde_json::json!([hash_or_height, verbosity])).await
    }

    /// Get a raw transaction by txid.
    /// verbose=0: hex string, verbose=1: JSON object.
    pub async fn get_raw_transaction(&self, txid: &str, verbose: u8) -> Result<serde_json::Value, RpcError> {
        self.call("getrawtransaction", serde_json::json!([txid, verbose])).await
    }

    /// Get estimated network solutions per second.
    /// `blocks`: number of recent blocks to average over (default 120).
    pub async fn get_network_sol_ps(&self, blocks: Option<u32>) -> Result<f64, RpcError> {
        let b = blocks.unwrap_or(120);
        self.call("getnetworksolps", serde_json::json!([b])).await
    }

    /// Shield transparent coinbase UTXOs to a shielded address.
    /// Returns an operation ID (opid) that can be polled with `z_getoperationstatus`.
    pub async fn z_shield_coinbase(
        &self,
        from_address: &str,
        to_address: &str,
        limit: Option<u32>,
    ) -> Result<serde_json::Value, RpcError> {
        let lim = limit.unwrap_or(0); // 0 = no limit, shield all mature coinbase UTXOs
        self.call("z_shieldcoinbase", serde_json::json!([
            from_address,
            to_address,
            null,               // fee (default ZIP 317)
            lim,                // limit UTXOs per tx
            null,               // memo
            "AllowRevealedSenders"
        ])).await
    }

    /// Send ZEC from `from_address` to multiple recipients.
    /// `amounts` is a list of (address, amount_in_zatoshis) pairs.
    /// Returns an operation ID (opid) that can be polled with `z_getoperationstatus`.
    pub async fn z_sendmany(
        &self,
        from_address: &str,
        amounts: &[(&str, f64)],
    ) -> Result<String, RpcError> {
        let recipients: Vec<serde_json::Value> = amounts
            .iter()
            .map(|(addr, amount)| {
                serde_json::json!({
                    "address": addr,
                    "amount": amount
                })
            })
            .collect();
        self.call("z_sendmany", serde_json::json!([
            from_address,
            recipients,
            1,                          // minconf
            null,                       // fee (use default ZIP 317)
            "AllowRevealedRecipients"   // privacyPolicy (allows shielded->transparent)
        ])).await
    }

    /// Check the status of one or more async operations.
    /// Returns a JSON array of operation status objects.
    pub async fn z_get_operation_status(
        &self,
        opids: &[&str],
    ) -> Result<Vec<serde_json::Value>, RpcError> {
        self.call("z_getoperationstatus", serde_json::json!([opids])).await
    }

    /// Get total wallet balance. Used for Zallet health check.
    /// Returns Ok(()) if RPC responds; balance value can be extracted from a raw call if needed.
    pub async fn z_get_total_balance(&self) -> Result<serde_json::Value, RpcError> {
        self.call(
            "z_gettotalbalance",
            serde_json::json!([0u32, true]),
        )
        .await
    }
}
