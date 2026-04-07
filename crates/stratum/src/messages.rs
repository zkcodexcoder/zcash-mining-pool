use serde::{Deserialize, Serialize};

/// A raw JSON-RPC message as received over the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
}

/// Parsed client (miner) request.
#[derive(Debug, Clone)]
pub enum ClientRequest {
    Subscribe {
        id: serde_json::Value,
        user_agent: String,
        session_id: Option<String>,
        host: Option<String>,
        port: Option<u16>,
    },
    Authorize {
        id: serde_json::Value,
        worker_name: String,
        worker_password: String,
    },
    Submit {
        id: serde_json::Value,
        worker_name: String,
        job_id: String,
        time: String,
        nonce_2: String,
        equihash_solution: String,
    },
    SuggestTarget {
        id: serde_json::Value,
        target: String,
    },
    ExtranonceSubscribe {
        id: serde_json::Value,
    },
    Unknown {
        id: serde_json::Value,
        method: String,
    },
}

/// Server notification or response to send to a miner.
#[derive(Debug, Clone)]
pub enum ServerMessage {
    SubscribeResult {
        id: serde_json::Value,
        session_id: String,
        nonce_1: String,
        nonce2_size: usize,
    },
    AuthorizeResult {
        id: serde_json::Value,
        authorized: bool,
        error: Option<StratumError>,
    },
    SetTarget {
        target: String,
    },
    SetDifficulty {
        difficulty: f64,
    },
    Notify {
        job_id: String,
        version: String,
        prev_hash: String,
        merkle_root: String,
        reserved: String,
        time: String,
        bits: String,
        clean_jobs: bool,
    },
    SubmitResult {
        id: serde_json::Value,
        accepted: bool,
        error: Option<StratumError>,
    },
    Reconnect {
        host: Option<String>,
        port: Option<u16>,
        wait_time: Option<u32>,
    },
}

/// Stratum error codes per ZIP 301.
#[derive(Debug, Clone)]
pub struct StratumError {
    pub code: i32,
    pub message: String,
    pub traceback: Option<serde_json::Value>,
}

impl std::fmt::Display for StratumError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl StratumError {
    pub fn other(msg: &str) -> Self {
        Self { code: 20, message: msg.to_string(), traceback: None }
    }
    pub fn job_not_found() -> Self {
        Self { code: 21, message: "Job not found".to_string(), traceback: None }
    }
    pub fn duplicate_share() -> Self {
        Self { code: 22, message: "Duplicate share".to_string(), traceback: None }
    }
    pub fn low_difficulty() -> Self {
        Self { code: 23, message: "Low difficulty share".to_string(), traceback: None }
    }
    pub fn unauthorized() -> Self {
        Self { code: 24, message: "Unauthorized worker".to_string(), traceback: None }
    }
    pub fn not_subscribed() -> Self {
        Self { code: 25, message: "Not subscribed".to_string(), traceback: None }
    }
}

impl ClientRequest {
    /// Parse a raw JSON-RPC message into a typed client request.
    pub fn parse(raw: &RawMessage) -> Option<Self> {
        let method = raw.method.as_deref()?;
        let id = raw.id.clone().unwrap_or(serde_json::Value::Null);
        let params = raw.params.clone().unwrap_or(serde_json::Value::Array(vec![]));
        let arr = params.as_array();

        match method {
            "mining.subscribe" => {
                let empty = vec![];
                let arr = arr.unwrap_or(&empty);
                Some(ClientRequest::Subscribe {
                    id,
                    user_agent: arr.first().and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    session_id: arr.get(1).and_then(|v| v.as_str()).map(|s| s.to_string()),
                    host: arr.get(2).and_then(|v| v.as_str()).map(|s| s.to_string()),
                    port: arr.get(3).and_then(|v| v.as_u64()).map(|v| v as u16),
                })
            }
            "mining.authorize" => {
                let arr = arr?;
                Some(ClientRequest::Authorize {
                    id,
                    worker_name: arr.first().and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    worker_password: arr.get(1).and_then(|v| v.as_str()).unwrap_or("").to_string(),
                })
            }
            "mining.submit" => {
                let arr = arr?;
                Some(ClientRequest::Submit {
                    id,
                    worker_name: arr.first().and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    job_id: arr.get(1).and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    time: arr.get(2).and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    nonce_2: arr.get(3).and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    equihash_solution: arr.get(4).and_then(|v| v.as_str()).unwrap_or("").to_string(),
                })
            }
            "mining.suggest_target" => {
                let arr = arr?;
                Some(ClientRequest::SuggestTarget {
                    id,
                    target: arr.first().and_then(|v| v.as_str()).unwrap_or("").to_string(),
                })
            }
            "mining.extranonce.subscribe" => {
                Some(ClientRequest::ExtranonceSubscribe { id })
            }
            _ => Some(ClientRequest::Unknown { id, method: method.to_string() }),
        }
    }
}

impl ServerMessage {
    /// Serialize a server message to a JSON line (without trailing newline).
    pub fn to_json(&self) -> String {
        let raw = match self {
            ServerMessage::SubscribeResult { id, session_id, nonce_1, nonce2_size } => {
                serde_json::json!({
                    "id": id,
                    "result": [["mining.notify", session_id], nonce_1, nonce2_size],
                    "error": null
                })
            }
            ServerMessage::AuthorizeResult { id, authorized, error } => {
                if *authorized {
                    serde_json::json!({
                        "id": id,
                        "result": true,
                        "error": null
                    })
                } else {
                    let err = error.as_ref().map(|e| {
                        serde_json::json!([e.code, e.message, e.traceback])
                    });
                    serde_json::json!({
                        "id": id,
                        "result": null,
                        "error": err
                    })
                }
            }
            ServerMessage::SetTarget { target } => {
                serde_json::json!({
                    "id": null,
                    "method": "mining.set_target",
                    "params": [target]
                })
            }
            ServerMessage::SetDifficulty { difficulty } => {
                serde_json::json!({
                    "id": null,
                    "method": "mining.set_difficulty",
                    "params": [difficulty]
                })
            }
            ServerMessage::Notify {
                job_id, version, prev_hash, merkle_root,
                reserved, time, bits, clean_jobs,
            } => {
                serde_json::json!({
                    "id": null,
                    "method": "mining.notify",
                    "params": [job_id, version, prev_hash, merkle_root, reserved, time, bits, clean_jobs]
                })
            }
            ServerMessage::SubmitResult { id, accepted, error } => {
                if *accepted {
                    serde_json::json!({
                        "id": id,
                        "result": true,
                        "error": null
                    })
                } else {
                    let err = error.as_ref().map(|e| {
                        serde_json::json!([e.code, e.message, e.traceback])
                    });
                    serde_json::json!({
                        "id": id,
                        "result": null,
                        "error": err
                    })
                }
            }
            ServerMessage::Reconnect { host, port, wait_time } => {
                let params: Vec<serde_json::Value> = match (host, port, wait_time) {
                    (Some(h), Some(p), Some(w)) => {
                        vec![serde_json::json!(h), serde_json::json!(p), serde_json::json!(w)]
                    }
                    _ => vec![],
                };
                serde_json::json!({
                    "id": null,
                    "method": "client.reconnect",
                    "params": params
                })
            }
        };
        serde_json::to_string(&raw).expect("failed to serialize server message")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_subscribe() {
        let raw = RawMessage {
            id: Some(serde_json::json!(1)),
            method: Some("mining.subscribe".to_string()),
            params: Some(serde_json::json!(["MagicBean/1.0.0", null, "pool.example.com", 3337])),
            result: None,
            error: None,
        };
        let req = ClientRequest::parse(&raw).unwrap();
        match req {
            ClientRequest::Subscribe { user_agent, host, port, .. } => {
                assert_eq!(user_agent, "MagicBean/1.0.0");
                assert_eq!(host, Some("pool.example.com".to_string()));
                assert_eq!(port, Some(3337));
            }
            _ => panic!("Expected Subscribe"),
        }
    }

    #[test]
    fn parse_submit() {
        let raw = RawMessage {
            id: Some(serde_json::json!(4)),
            method: Some("mining.submit".to_string()),
            params: Some(serde_json::json!(["worker1", "job42", "12345678", "aabbccdd", "0100...solution"])),
            result: None,
            error: None,
        };
        let req = ClientRequest::parse(&raw).unwrap();
        match req {
            ClientRequest::Submit { worker_name, job_id, nonce_2, .. } => {
                assert_eq!(worker_name, "worker1");
                assert_eq!(job_id, "job42");
                assert_eq!(nonce_2, "aabbccdd");
            }
            _ => panic!("Expected Submit"),
        }
    }

    #[test]
    fn serialize_notify() {
        let msg = ServerMessage::Notify {
            job_id: "42".to_string(),
            version: "04000000".to_string(),
            prev_hash: "aa".repeat(32),
            merkle_root: "bb".repeat(32),
            reserved: "00".repeat(32),
            time: "12345678".to_string(),
            bits: "deadbeef".to_string(),
            clean_jobs: true,
        };
        let json = msg.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["method"], "mining.notify");
        assert_eq!(parsed["params"][0], "42");
        assert_eq!(parsed["params"][7], true);
    }

    #[test]
    fn serialize_subscribe_result() {
        let msg = ServerMessage::SubscribeResult {
            id: serde_json::json!(1),
            session_id: "abc123".to_string(),
            nonce_1: "01020304".to_string(),
            nonce2_size: 28,
        };
        let json = msg.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["result"][0][0], "mining.notify");
        assert_eq!(parsed["result"][0][1], "abc123");
        assert_eq!(parsed["result"][1], "01020304");
        assert_eq!(parsed["result"][2], 28);
    }
}
