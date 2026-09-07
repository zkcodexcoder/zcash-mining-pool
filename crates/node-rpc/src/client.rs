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
    /// The node answered, but the response didn't match our expected schema.
    /// Carries the body head so the NEXT network-upgrade schema break is
    /// diagnosable from the log line (audit #16: the NU6.3 template break
    /// collapsed to an information-free NullResult for a week).
    #[error("RPC response parse error: {message}; body: {body_snippet}")]
    Parse { message: String, body_snippet: String },
}

impl RpcError {
    /// True only when the node itself authoritatively said "no such
    /// transaction/item" (JSON-RPC error code -5, RPC_INVALID_ADDRESS_OR_KEY).
    /// Transport failures (HTTP, timeouts), parse issues, and every other RPC
    /// error mean "unknown", NOT "doesn't exist" — money-path callers
    /// (reconciler) must never treat them as proof of absence, or a node
    /// outage converts live payouts into refunds/failures (double-pay class).
    pub fn is_definitely_not_found(&self) -> bool {
        matches!(self, RpcError::JsonRpc(e) if e.code == -5)
    }
}

/// Client for communicating with a Zcash node (zcashd or zebrad) via JSON-RPC.
pub struct ZcashRpcClient {
    http: reqwest::Client,
    url: String,
    auth: Option<(String, String)>,
    next_id: AtomicU64,
}

impl ZcashRpcClient {
    /// Construct a client with an explicitly bounded transport. This permits
    /// owner-run read-only probes to disable redirects/proxies without changing
    /// the established runtime defaults used by `new` and `with_auth`.
    pub fn with_transport(
        url: &str,
        auth: Option<(String, String)>,
        http: reqwest::Client,
    ) -> Self {
        Self { http, url: url.to_string(), auth, next_id: AtomicU64::new(1) }
    }

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

    /// Isolated zecd funding reads only. Unlike legacy call_raw this bounds
    /// the streamed body before parsing and never retains remote error text.
    /// No existing RPC caller or mainnet wallet path uses this helper.
    pub(crate) async fn zecd_funding_read(
        &self,
        method: &'static str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, crate::zecd_funding::ZecdFundingError> {
        use crate::zecd_funding::ZecdFundingError as E;
        let limit = match method {
            "listunspent" => crate::zecd_funding::MAX_RPC_BODY_BYTES,
            "getnetworkinfo" | "getwalletinfo" | "getblockchaininfo" | "getaddressinfo"
            | "getbalance" | "getblockcount" | "getblockhash" => 128 * 1024,
            _ => return Err(E::InvalidEvidence),
        };
        self.zecd_bounded_rpc(method, params, limit, false).await
    }

    /// Isolated conventional testnet payout transport. This is deliberately
    /// separate from the funding read allowlist: z_sendmany is a wallet write
    /// and must only follow the typed durable reservation and one-shot seal.
    /// It sends exactly once, never redirects/retries, and suppresses remote
    /// error content. Mainnet and existing generic RPC callers are unchanged.
    pub async fn zecd_conventional_rpc(
        &self, method: &'static str, params: serde_json::Value,
    ) -> Result<serde_json::Value, crate::zecd_funding::ZecdFundingError> {
        if !matches!(method, "z_sendmany" | "z_getoperationstatus" | "getrawtransaction"
            | "getblock" | "getblockhash" | "gettransaction" | "getwalletinfo")
        { return Err(crate::zecd_funding::ZecdFundingError::InvalidEvidence); }
        self.zecd_bounded_rpc(method, params, crate::zecd_funding::MAX_RPC_BODY_BYTES, false).await
    }

    pub(crate) async fn zecd_signer_read(
        &self, method: &'static str, params: serde_json::Value,
    ) -> Result<serde_json::Value, crate::zecd_funding::ZecdFundingError> {
        if !matches!(method, "z_getoperationstatus" | "getrawtransaction" | "getblock" | "getblockhash" | "gettransaction") {
            return Err(crate::zecd_funding::ZecdFundingError::InvalidEvidence);
        }
        self.zecd_bounded_rpc(method, params, crate::zecd_funding::MAX_RPC_BODY_BYTES, false).await
    }

    /// Bounded signer-receipt discovery only. None means an explicit -5 error
    /// for this exact raw lookup, never a transport failure or unconfirmed tx.
    /// Actor probes and all ordinary RPC error semantics remain separate.
    pub(crate) async fn zecd_signer_raw_lookup(
        &self, txid: &str,
    ) -> Result<Option<serde_json::Value>, crate::zecd_funding::ZecdFundingError> {
        if txid.len()!=64 || !txid.bytes().all(|b|b.is_ascii_hexdigit()) {
            return Err(crate::zecd_funding::ZecdFundingError::InvalidEvidence);
        }
        self.zecd_bounded_rpc_outcome("getrawtransaction", serde_json::json!([txid,1]),
            crate::zecd_funding::MAX_RPC_BODY_BYTES, false, true).await
    }

    /// A fixed absent-tx lookup bypasses the read-side cache and reaches the
    /// wallet actor in pinned zecd. Only -5 is the successful missing response;
    /// a dead actor (-1), missing RPC, upstream error or unexpected result fails.
    pub(crate) async fn zecd_actor_live(&self) -> Result<(), crate::zecd_funding::ZecdFundingError> {
        use crate::zecd_funding::ZecdFundingError as E;
        match self.zecd_bounded_rpc("getrawtransaction",
            serde_json::json!(["0".repeat(64), 0]), 128 * 1024, true).await
        {
            Err(E::ActorProbeMissing) => Ok(()),
            _ => Err(E::NotReady),
        }
    }

    async fn zecd_bounded_rpc(
        &self, method: &'static str, params: serde_json::Value, limit: usize, actor_probe: bool,
    ) -> Result<serde_json::Value, crate::zecd_funding::ZecdFundingError> {
        self.zecd_bounded_rpc_outcome(method, params, limit, actor_probe, false).await?
            .ok_or(crate::zecd_funding::ZecdFundingError::InvalidEvidence)
    }

    async fn zecd_bounded_rpc_outcome(
        &self, method: &'static str, params: serde_json::Value, limit: usize,
        actor_probe: bool, raw_absence: bool,
    ) -> Result<Option<serde_json::Value>, crate::zecd_funding::ZecdFundingError> {
        use crate::zecd_funding::ZecdFundingError as E;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = JsonRpcRequest { jsonrpc: "2.0", id, method, params };
        // Keep this isolated path on the exact configured URL: no proxy
        // environment or HTTP redirect may send wallet-source data elsewhere.
        // Existing caller transports and all legacy/mainnet RPCs are unchanged.
        let http = reqwest::Client::builder().no_proxy()
            .redirect(reqwest::redirect::Policy::none()).timeout(RPC_TIMEOUT)
            .build().map_err(|_| E::Unavailable)?;
        let mut req = http.post(&self.url).json(&request).timeout(RPC_TIMEOUT);
        if let Some((user, pass)) = &self.auth { req = req.basic_auth(user, Some(pass)); }
        let mut response = req.send().await.map_err(|_| E::Unavailable)?;
        let http_success = response.status().is_success();
        // Pinned zecd follows Bitcoin Core: a single JSON-RPC error uses HTTP
        // 500. Parse only that bounded error envelope, never treat a 500 result
        // as success. Redirects, authentication and all other statuses reject.
        if !http_success && response.status() != reqwest::StatusCode::INTERNAL_SERVER_ERROR {
            return Err(E::Unavailable);
        }
        if response.content_length().is_some_and(|n| n > limit as u64) {
            return Err(E::ResponseTooLarge);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| E::Unavailable)? {
            if chunk.len() > limit.saturating_sub(body.len()) { return Err(E::ResponseTooLarge); }
            body.extend_from_slice(&chunk);
        }
        let envelope: serde_json::Value = serde_json::from_slice(&body).map_err(|_| E::InvalidEvidence)?;
        if !envelope.is_object() || envelope.get("id").and_then(serde_json::Value::as_u64) != Some(id) {
            return Err(E::InvalidEvidence);
        }
        if let Some(error) = envelope.get("error").filter(|v| !v.is_null()) {
            // JSON-RPC 2 permits an error response to omit result. Only this
            // fixed discovery lookup admits absent-or-null result with -5;
            // no other method or error gains that interpretation.
            if raw_absence && error.is_object()
                && error.get("code").and_then(serde_json::Value::as_i64)==Some(-5)
                && error.get("message").and_then(serde_json::Value::as_str).is_some()
                && envelope.get("result").is_none_or(serde_json::Value::is_null)
            { return Ok(None); }
            if envelope.get("result") != Some(&serde_json::Value::Null) {
                return Err(E::InvalidEvidence);
            }
            if actor_probe && error.get("code").and_then(serde_json::Value::as_i64) == Some(-5) {
                return Err(E::ActorProbeMissing);
            }
            return Err(if error.get("code").and_then(serde_json::Value::as_i64) == Some(-32601) {
                E::UnsupportedRpc
            } else { E::Unavailable });
        }
        if !http_success { return Err(E::Unavailable); }
        envelope.get("result").filter(|v| !v.is_null()).cloned().map(Some).ok_or(E::InvalidEvidence)
    }

    async fn call<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T, RpcError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
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
            serde_json::from_str(&body).map_err(|e| RpcError::Parse {
                message: e.to_string(),
                body_snippet: body.chars().take(300).collect(),
            })?;

        if let Some(err) = resp.error {
            return Err(RpcError::JsonRpc(err));
        }

        resp.result.ok_or(RpcError::NullResult)
    }

    /// Fetch a block template for mining.
    pub async fn get_block_template(&self) -> Result<BlockTemplate, RpcError> {
        let params = serde_json::json!([{
            "capabilities": ["coinbasetxn", "workid", "longpoll"],
            "mode": "template"
        }]);
        self.call("getblocktemplate", params).await
    }

    /// Long-polling variant of `get_block_template`. The node blocks until
    /// the template identified by `longpollid` is no longer current (new
    /// block, mempool change, etc.) and then returns the new template.
    /// Uses `timeout` as a hard cap on the HTTP request so the pool can
    /// fall back to regular polling if the node hangs.
    pub async fn get_block_template_longpoll(
        &self,
        longpollid: &str,
        timeout: Duration,
    ) -> Result<BlockTemplate, RpcError> {
        let params = serde_json::json!([{
            "capabilities": ["coinbasetxn", "workid", "longpoll"],
            "mode": "template",
            "longpollid": longpollid,
        }]);
        self.call_with_timeout("getblocktemplate", params, timeout).await
    }

    /// Same as `call` but overrides the per-request HTTP timeout. Used for
    /// long-polling requests that are expected to take longer than
    /// `RPC_TIMEOUT`.
    async fn call_with_timeout<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> Result<T, RpcError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method,
            params,
        };
        let mut req = self.http.post(&self.url).json(&request).timeout(timeout);
        if let Some((ref user, ref pass)) = self.auth {
            req = req.basic_auth(user, Some(pass));
        }
        let http_resp = req.send().await?;
        let body = http_resp.text().await?;
        let resp: JsonRpcResponse<T> =
            serde_json::from_str(&body).map_err(|e| RpcError::Parse {
                message: e.to_string(),
                body_snippet: body.chars().take(300).collect(),
            })?;
        if let Some(err) = resp.error {
            return Err(RpcError::JsonRpc(err));
        }
        resp.result.ok_or(RpcError::NullResult)
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
            serde_json::from_str(&body).map_err(|e| RpcError::Parse {
                message: e.to_string(),
                body_snippet: body.chars().take(300).collect(),
            })?;

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

    /// Get mining info (includes network difficulty, hashrate, chain height, etc.).
    pub async fn get_mining_info(&self) -> Result<serde_json::Value, RpcError> {
        self.call("getmininginfo", serde_json::json!([])).await
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
        match self.call("z_shieldcoinbase", serde_json::json!([
            from_address,
            to_address,
            null,               // fee (default ZIP 317)
            lim,                // limit UTXOs per tx
            null,               // memo
            "AllowRevealedSenders"
        ])).await {
            Ok(v) => Ok(v),
            // zecd rejects the extended zcashd parameter list (it sweeps all
            // mature coinbase in one op, ZIP-317 fee always); retry minimal
            // (from, to). A real failure (e.g. -6 nothing-to-shield) surfaces
            // from this second call with its proper error.
            Err(RpcError::JsonRpc(_)) => {
                self.call(
                    "z_shieldcoinbase",
                    serde_json::json!([from_address, to_address]),
                )
                .await
            }
            Err(e) => Err(e),
        }
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

    /// Wallet balances in a dialect-neutral form: (transparent+shielded spendable,
    /// pending, immature) in ZEC as f64. Probes `z_gettotalbalance` (zallet/zcashd
    /// dialect: string fields, minconf param) first, falling back to `getbalances`
    /// (zecd/Bitcoin-Core dialect: numeric `mine.*` fields, ZIP-315 confirmations
    /// policy). Both payout-wallet dialects thus work with no config switch.
    pub async fn wallet_balances(&self, minconf: u32) -> Result<WalletBalances, RpcError> {
        match self
            .call::<serde_json::Value>("z_gettotalbalance", serde_json::json!([minconf, true]))
            .await
        {
            Ok(v) => {
                let s = |k: &str| {
                    v.get(k)
                        .and_then(|x| x.as_str())
                        .and_then(|x| x.parse::<f64>().ok())
                        .unwrap_or(0.0)
                };
                Ok(WalletBalances {
                    spendable: s("private"),
                    transparent: s("transparent"),
                    pending: 0.0,
                    immature: 0.0,
                })
            }
            // Method not found (-32601) or zcashd's unknown-method (-32602/-1):
            // assume the getbalances dialect.
            Err(RpcError::JsonRpc(_)) => {
                let v = self
                    .call::<serde_json::Value>("getbalances", serde_json::json!([]))
                    .await?;
                let mine = v.get("mine").cloned().unwrap_or_default();
                let n = |k: &str| mine.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
                Ok(WalletBalances {
                    spendable: n("trusted"),
                    transparent: n("coinbase"),
                    pending: n("untrusted_pending"),
                    immature: n("immature"),
                })
            }
            Err(e) => Err(e),
        }
    }
}

/// Dialect-neutral wallet balance snapshot (ZEC as f64), from either
/// `z_gettotalbalance` (zallet) or `getbalances` (zecd).
#[derive(Debug, Clone, Copy, Default)]
pub struct WalletBalances {
    /// Spendable shielded balance (zallet: `private` at the given minconf;
    /// zecd: `mine.trusted` under its ZIP-315 confirmations policy).
    pub spendable: f64,
    /// Transparent funds (zallet: `transparent`; zecd: `mine.coinbase`, the
    /// mature-coinbase-awaiting-shielding portion relevant to the pool).
    pub transparent: f64,
    /// Incoming below the confirmations policy (zecd only; zallet folds this
    /// into the minconf semantics).
    pub pending: f64,
    /// Immature coinbase (zecd only).
    pub immature: f64,
}
