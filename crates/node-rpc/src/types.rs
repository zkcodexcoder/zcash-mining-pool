use serde::{Deserialize, Serialize};

/// A transaction included in a block template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTemplateTransaction {
    pub data: String,
    pub hash: String,
    pub fee: i64,
    pub sigops: u64,
    #[serde(default)]
    pub required: bool,
    /// Authorizing data digest (v5 transactions). Provided by zebrad.
    #[serde(default)]
    pub authdigest: Option<String>,
}

/// Response from the `getblocktemplate` RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTemplate {
    pub version: u32,
    pub previousblockhash: String,
    /// Light-client root commitment (NU5+).
    #[serde(default)]
    pub lightclientroothash: Option<String>,
    /// Block commitments hash (NU5+).
    #[serde(default)]
    pub blockcommitmentshash: Option<String>,
    /// Final sapling root hash.
    #[serde(default)]
    pub finalsaplingroothash: Option<String>,
    /// Default root hashes computed by the node.
    #[serde(default)]
    pub defaultroots: Option<DefaultRoots>,
    pub transactions: Vec<BlockTemplateTransaction>,
    /// Coinbase transaction fields.
    #[serde(default)]
    pub coinbasetxn: Option<CoinbaseTxn>,
    pub target: String,
    pub mintime: u64,
    pub curtime: u64,
    pub bits: String,
    pub height: u64,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub mutable: Vec<String>,
    #[serde(default)]
    pub noncerange: Option<String>,
    #[serde(default)]
    pub sigoplimit: Option<u64>,
    #[serde(default)]
    pub sizelimit: Option<u64>,
}

/// Default root hashes returned by getblocktemplate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultRoots {
    #[serde(default)]
    pub merkleroot: Option<String>,
    #[serde(default)]
    pub chainhistoryroot: Option<String>,
    #[serde(default)]
    pub authdataroot: Option<String>,
    #[serde(default)]
    pub blockcommitmentshash: Option<String>,
}

/// Coinbase transaction info from the block template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinbaseTxn {
    pub data: String,
    pub hash: String,
    #[serde(default)]
    pub fee: Option<i64>,
    #[serde(default)]
    pub sigops: Option<u64>,
    #[serde(default)]
    pub required: Option<bool>,
}

/// Response from `getblockcount`.
pub type BlockCount = u64;

/// Response from `getbestblockhash`.
pub type BestBlockHash = String;

/// Response from `getinfo` (subset of fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    #[serde(default)]
    pub version: Option<u64>,
    #[serde(default)]
    pub subversion: Option<String>,
    #[serde(default)]
    pub blocks: Option<u64>,
    #[serde(default)]
    pub connections: Option<u64>,
    #[serde(default)]
    pub testnet: Option<bool>,
}

/// Generic JSON-RPC request envelope.
#[derive(Debug, Serialize)]
pub struct JsonRpcRequest<'a> {
    pub jsonrpc: &'a str,
    pub id: u64,
    pub method: &'a str,
    pub params: serde_json::Value,
}

/// Generic JSON-RPC response envelope.
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse<T> {
    pub id: Option<u64>,
    pub result: Option<T>,
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error object.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RPC error {}: {}", self.code, self.message)
    }
}
