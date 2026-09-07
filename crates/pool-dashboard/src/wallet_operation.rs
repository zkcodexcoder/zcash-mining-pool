//! Normalize the two supported wallet operation-result shapes. A payout batch
//! can be finalized only with one unambiguous transaction ID; accepting the
//! first ID from a multi-transaction result could falsely settle the whole batch.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResultError {
    MissingResult,
    MissingTxid,
    MalformedTxid,
    MultipleTxids,
    ConflictingTxids,
}

impl std::fmt::Display for ResultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MissingResult => "missing or non-object result",
            Self::MissingTxid => "missing transaction ID",
            Self::MalformedTxid => "malformed transaction ID",
            Self::MultipleTxids => "multiple transaction IDs are unsupported",
            Self::ConflictingTxids => "conflicting transaction IDs",
        })
    }
}

impl std::error::Error for ResultError {}

fn valid_txid(value: &Value) -> Result<String, ResultError> {
    let txid = value.as_str().ok_or(ResultError::MalformedTxid)?;
    normalize_txid(txid)
}

/// Also guard legacy persisted IDs (the old wait path fabricated `unknown`).
pub(crate) fn normalize_txid(txid: &str) -> Result<String, ResultError> {
    if txid.len() != 64 || !txid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ResultError::MalformedTxid);
    }
    Ok(txid.to_ascii_lowercase())
}

/// `txid` and singleton `txids` are supported. If both are present, both must
/// be valid and identify the same transaction. Invalid success evidence is an
/// UNKNOWN outcome, not an operation failure: callers must retain reservations.
pub(crate) fn successful_txid(result: Option<&Value>) -> Result<String, ResultError> {
    let result = result
        .and_then(Value::as_object)
        .ok_or(ResultError::MissingResult)?;
    let single = result.get("txid").map(valid_txid).transpose()?;
    let array = result
        .get("txids")
        .map(|value| {
            let txids = value.as_array().ok_or(ResultError::MalformedTxid)?;
            match txids.len() {
                0 => Err(ResultError::MissingTxid),
                1 => valid_txid(&txids[0]),
                _ => Err(ResultError::MultipleTxids),
            }
        })
        .transpose()?;

    match (single, array) {
        (Some(a), Some(b)) if a != b => Err(ResultError::ConflictingTxids),
        (Some(txid), _) | (_, Some(txid)) => Ok(txid),
        (None, None) => Err(ResultError::MissingTxid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TXID: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    const OTHER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn accepts_both_single_transaction_dialects() {
        assert_eq!(
            successful_txid(Some(&json!({"txid": TXID}))),
            Ok(TXID.to_string())
        );
        assert_eq!(
            successful_txid(Some(&json!({"txids": [TXID]}))),
            Ok(TXID.to_string())
        );
    }

    #[test]
    fn matching_dual_representation_is_one_transaction() {
        let result = json!({"txid": TXID.to_ascii_uppercase(), "txids": [TXID]});
        assert_eq!(successful_txid(Some(&result)), Ok(TXID.to_string()));
    }

    #[test]
    fn conflicting_dual_representation_is_unknown() {
        assert_eq!(
            successful_txid(Some(&json!({"txid": TXID, "txids": [OTHER]}))),
            Err(ResultError::ConflictingTxids)
        );
    }

    #[test]
    fn rejects_multi_transaction_results_even_with_valid_single_field() {
        for result in [
            json!({"txids": [TXID, OTHER]}),
            json!({"txids": [TXID, TXID]}),
            json!({"txid": TXID, "txids": [TXID, OTHER]}),
        ] {
            assert_eq!(
                successful_txid(Some(&result)),
                Err(ResultError::MultipleTxids)
            );
        }
    }

    #[test]
    fn rejects_missing_results_and_ids() {
        assert_eq!(successful_txid(None), Err(ResultError::MissingResult));
        for result in [Value::Null, json!([]), json!("success")] {
            assert_eq!(
                successful_txid(Some(&result)),
                Err(ResultError::MissingResult)
            );
        }
        for result in [json!({}), json!({"txids": []})] {
            assert_eq!(
                successful_txid(Some(&result)),
                Err(ResultError::MissingTxid)
            );
        }
    }

    #[test]
    fn rejects_malformed_ids_in_either_dialect() {
        for invalid in [
            Value::Null,
            json!(1),
            json!(true),
            json!([]),
            json!({}),
            json!("unknown"),
            json!(""),
            json!("a".repeat(63)),
            json!("a".repeat(65)),
            json!("g".repeat(64)),
            json!("é".repeat(32)),
            json!(format!(" {TXID}")),
        ] {
            assert_eq!(
                successful_txid(Some(&json!({"txid": invalid}))),
                Err(ResultError::MalformedTxid)
            );
            assert_eq!(
                successful_txid(Some(&json!({"txids": [invalid]}))),
                Err(ResultError::MalformedTxid)
            );
        }
    }

    #[test]
    fn malformed_field_cannot_be_masked_by_other_valid_field() {
        for result in [
            json!({"txid": Value::Null, "txids": [TXID]}),
            json!({"txid": "unknown", "txids": [TXID]}),
            json!({"txid": TXID, "txids": Value::Null}),
            json!({"txid": TXID, "txids": TXID}),
            json!({"txid": TXID, "txids": ["unknown"]}),
        ] {
            assert_eq!(
                successful_txid(Some(&result)),
                Err(ResultError::MalformedTxid)
            );
        }
    }
}
