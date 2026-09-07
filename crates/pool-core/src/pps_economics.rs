//! Network/height-bound miner-only subsidy retrieval for standard PPS.
//! This performs no wallet call or transaction. It is not an independent chain
//! agreement proof; the caller must also require a current trusted chain lease.
//! Cache only by immutable network AND job height, never just by current tip.

use node_rpc::ZcashRpcClient;
use rewards::PpsNetwork;
use serde_json::Value;

const MAX_MINER_SUBSIDY_ZATS: u64 = 1_250_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PpsEconomicsError {
    #[error("PPS economics RPC unavailable")]
    RpcUnavailable,
    #[error("PPS economics network identity mismatch")]
    NetworkMismatch,
    #[error("PPS economics requires a synchronized node")]
    NodeNotSynchronized,
    #[error("PPS economics job height is not the node's next block")]
    InvalidHeight,
    #[error("PPS network target does not match canonical header difficulty bits")]
    InvalidTarget,
    #[error("PPS economics response does not contain a valid miner subsidy")]
    InvalidSubsidy,
    #[error("PPS miner subsidy has invalid decimal precision or bounds")]
    InvalidDecimal,
}

/// Parse decimal coins as exact zatoshis, without any f64 conversion. Supports
/// exponent notation only when its value is exactly representable in zatoshis.
/// Trailing decimal zeros are harmless; a nonzero fractional zatoshi is not.
/// Reject signs on the coefficient, whitespace, NaN/Inf, huge numbers/exponents
/// and values outside the conservative miner-subsidy envelope.
pub fn decimal_coins_to_zatoshis(raw: &str) -> Result<u64, PpsEconomicsError> {
    let invalid = PpsEconomicsError::InvalidDecimal;
    if raw.is_empty() || raw.len() > 96 || !raw.is_ascii() {
        return Err(invalid);
    }
    let mut exponent_parts = raw.split(['e', 'E']);
    let coefficient = exponent_parts.next().ok_or(invalid)?;
    let exponent = match exponent_parts.next() {
        None => 0_i32,
        Some(value) => {
            let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid);
            }
            value.parse::<i32>().map_err(|_| invalid)?
        }
    };
    if exponent_parts.next().is_some() {
        return Err(invalid);
    }
    let mut decimal_parts = coefficient.split('.');
    let whole = decimal_parts.next().ok_or(invalid)?;
    let fraction = decimal_parts.next();
    if decimal_parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || (whole.len() > 1 && whole.starts_with('0'))
        || fraction.is_some_and(|v| v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(invalid);
    }
    let fraction = fraction.unwrap_or("");
    let digits = whole
        .bytes()
        .chain(fraction.bytes())
        .try_fold(0_u128, |n, b| {
            n.checked_mul(10)
                .and_then(|n| n.checked_add(u128::from(b - b'0')))
                .ok_or(invalid)
        })?;
    let power = 8_i32
        .checked_add(exponent)
        .and_then(|n| n.checked_sub(i32::try_from(fraction.len()).ok()?))
        .ok_or(invalid)?;
    let zats = if power >= 0 {
        let multiplier = 10_u128.checked_pow(power as u32).ok_or(invalid)?;
        digits.checked_mul(multiplier).ok_or(invalid)?
    } else {
        let magnitude = power.checked_neg().ok_or(invalid)? as u32;
        let divisor = 10_u128.checked_pow(magnitude).ok_or(invalid)?;
        if digits % divisor != 0 {
            return Err(invalid);
        }
        digits / divisor
    };
    let zats = u64::try_from(zats).map_err(|_| invalid)?;
    if zats == 0 || zats > MAX_MINER_SUBSIDY_ZATS {
        return Err(invalid);
    }
    Ok(zats)
}

fn validate_node(info: &Value, network: PpsNetwork, height: u64) -> Result<(), PpsEconomicsError> {
    let chain_ok = match (network, info.get("chain").and_then(Value::as_str)) {
        (PpsNetwork::Mainnet, Some("main" | "mainnet")) => true,
        (PpsNetwork::Testnet, Some("test" | "testnet")) => true,
        _ => false,
    };
    if !chain_ok {
        return Err(PpsEconomicsError::NetworkMismatch);
    }
    validate_local_sync(info)?;
    let tip = info
        .get("blocks")
        .and_then(Value::as_u64)
        .ok_or(PpsEconomicsError::InvalidHeight)?;
    if height == 0
        || height > u32::MAX as u64
        || tip > u32::MAX as u64
        || height != tip.saturating_add(1)
    {
        return Err(PpsEconomicsError::InvalidHeight);
    }
    Ok(())
}

/// Local readiness schema only, NEVER an independent canonical-chain proof.
/// Zakura v1.3.0 deliberately omits Bitcoin's initialblockdownload field:
/// https://github.com/zakura-core/zakura/blob/v1.3.0/crates/zakura-rpc/src/methods.rs
/// Its integrated header-chain publisher exposes the fully body-verified
/// frontier. Only an ABSENT IBD field may use that explicit alternative.
/// Callers must additionally require the current two-operator chain lease.
pub(crate) fn validate_local_sync(info: &Value) -> Result<(), PpsEconomicsError> {
    let invalid = PpsEconomicsError::NodeNotSynchronized;
    match info.get("initialblockdownload") {
        Some(Value::Bool(false)) => return Ok(()),
        Some(_) => return Err(invalid),
        None => {}
    }
    let blocks = info.get("blocks").and_then(Value::as_u64).ok_or(invalid)?;
    let headers = info.get("headers").and_then(Value::as_u64).ok_or(invalid)?;
    if blocks == 0 || headers > u32::MAX as u64 || headers < blocks || headers - blocks > 24 {
        return Err(invalid);
    }
    let hash = |value: Option<&Value>| -> Result<String, PpsEconomicsError> {
        let text = value.and_then(Value::as_str).ok_or(invalid)?;
        if text.len() != 64 || !text.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(invalid);
        }
        Ok(text.to_ascii_lowercase())
    };
    let best = hash(info.get("bestblockhash"))?;
    let chain = info
        .get("header_chain")
        .and_then(Value::as_object)
        .ok_or(invalid)?;
    if chain.get("mode").and_then(Value::as_str) != Some("integrated")
        || chain.contains_key("finality_warning")
    {
        return Err(invalid);
    }
    let verified = chain.get("verified_best").ok_or(invalid)?;
    let header = chain.get("header_best").ok_or(invalid)?;
    if verified.get("height").and_then(Value::as_u64) != Some(blocks)
        || header.get("height").and_then(Value::as_u64) != Some(headers)
        || hash(verified.get("hash"))? != best
    {
        return Err(invalid);
    }
    let header_hash = hash(header.get("hash"))?;
    if headers == blocks && header_hash != best {
        return Err(invalid);
    }
    let alarms = chain.get("alarms").ok_or(invalid)?;
    if alarms.get("resource_stalled").and_then(Value::as_bool) != Some(false)
        || alarms.get("header_best_body_unavailable") != Some(&Value::Null)
        || alarms.get("migrated_pin_refuted") != Some(&Value::Null)
    {
        return Err(invalid);
    }
    Ok(())
}

/// Bind the exact network target used for PPS probability to nBits that the
/// miner actually hashes. `bits_hex` is the RPC template's eight-character,
/// big-endian numeric form, NOT the byte-reversed Stratum wire form.
/// Reject negative, zero, overflowing and noncanonical compact targets.
pub fn validate_template_target(
    bits_hex: &str,
    network_target_be: &[u8; 32],
) -> Result<(), PpsEconomicsError> {
    let invalid = PpsEconomicsError::InvalidTarget;
    if bits_hex.len() != 8 || !bits_hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid);
    }
    let bits = u32::from_str_radix(bits_hex, 16).map_err(|_| invalid)?;
    if bits & 0x0080_0000 != 0 {
        return Err(invalid);
    }
    let size = (bits >> 24) as i32;
    let mantissa = bits & 0x007f_ffff;
    let mut target = [0u8; 32];
    for byte in 0..3 {
        let value = ((mantissa >> (8 * byte)) & 255) as u8;
        let position = size - 3 + byte;
        if position >= 32 {
            if value != 0 {
                return Err(invalid);
            }
        } else if position >= 0 {
            target[31 - position as usize] = value;
        }
    }
    let first = target.iter().position(|v| *v != 0).ok_or(invalid)?;
    let mut canonical_size = (32 - first) as u32;
    let mut canonical_mantissa = if canonical_size <= 3 {
        u32::from_be_bytes(target[28..32].try_into().map_err(|_| invalid)?)
            << (8 * (3 - canonical_size))
    } else {
        (u32::from(target[first]) << 16)
            | (u32::from(target[first + 1]) << 8)
            | u32::from(target[first + 2])
    };
    if canonical_mantissa & 0x0080_0000 != 0 {
        canonical_mantissa >>= 8;
        canonical_size += 1;
    }
    let canonical_bits = (canonical_size << 24) | canonical_mantissa;
    if bits != canonical_bits || target != *network_target_be {
        return Err(invalid);
    }
    Ok(())
}

pub fn validate_economics_response(
    info: &Value,
    subsidy: &Value,
    network: PpsNetwork,
    height: u64,
) -> Result<u64, PpsEconomicsError> {
    validate_node(info, network, height)?;
    if let Some(returned_height) = subsidy.get("height") {
        if returned_height.as_u64() != Some(height) {
            return Err(PpsEconomicsError::InvalidHeight);
        }
    }
    let amount = subsidy
        .get("miner")
        .and_then(Value::as_number)
        .ok_or(PpsEconomicsError::InvalidSubsidy)?;
    // arbitrary_precision is mandatory in Cargo features: Number's original
    // decimal value must not be rounded through f64 before this exact parser.
    decimal_coins_to_zatoshis(&amount.to_string())
}

pub async fn validated_miner_subsidy(
    rpc: &ZcashRpcClient,
    network: PpsNetwork,
    height: u64,
) -> Result<u64, PpsEconomicsError> {
    if height == 0 || height > u32::MAX as u64 {
        return Err(PpsEconomicsError::InvalidHeight);
    }
    let info: Value = rpc
        .call_raw("getblockchaininfo", serde_json::json!([]))
        .await
        .map_err(|_| PpsEconomicsError::RpcUnavailable)?;
    // Validate before requesting economics on a wrong or unsynchronized chain.
    validate_node(&info, network, height)?;
    let subsidy: Value = rpc
        .call_raw("getblocksubsidy", serde_json::json!([height]))
        .await
        .map_err(|_| PpsEconomicsError::RpcUnavailable)?;
    validate_economics_response(&info, &subsidy, network, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(chain: &str) -> Value {
        serde_json::json!({"chain":chain,"initialblockdownload":false,"blocks":4_000_000})
    }
    #[test]
    fn template_bits_bind_exact_canonical_target_and_endian() {
        let mut mainnet = [0u8; 32];
        mainnet[1] = 7;
        mainnet[2] = 255;
        mainnet[3] = 255;
        assert!(validate_template_target("1f07ffff", &mainnet).is_ok());
        assert!(validate_template_target("ffff071f", &mainnet).is_err());
        mainnet[31] = 1;
        assert!(validate_template_target("1f07ffff", &mainnet).is_err());
        let mut one = [0u8; 32];
        one[31] = 1;
        assert!(validate_template_target("01010000", &one).is_ok());
        for invalid in [
            "00000000",
            "01000001",
            "02000100",
            "01810000",
            "21010000",
            "ff010000",
            "1010000",
            "0x01010000",
        ] {
            assert!(
                validate_template_target(invalid, &one).is_err(),
                "{invalid}"
            );
        }
        let mut maximum_compact = [0u8; 32];
        maximum_compact[0] = 255;
        maximum_compact[1] = 255;
        assert!(validate_template_target("2100ffff", &maximum_compact).is_ok());
        let mut testnet = [0u8; 32];
        testnet[0] = 7;
        testnet[1] = 255;
        testnet[2] = 255;
        assert!(validate_template_target("2007ffff", &testnet).is_ok());
    }
    #[test]
    fn stale_job_height_cannot_refresh_historical_subsidy_forever() {
        let subsidy = serde_json::json!({"miner":1.25});
        for height in [1, 3_999_999, 4_000_000, 4_000_002] {
            assert_eq!(
                validate_economics_response(&node("test"), &subsidy, PpsNetwork::Testnet, height),
                Err(PpsEconomicsError::InvalidHeight)
            );
        }
        assert!(validate_economics_response(
            &node("test"),
            &subsidy,
            PpsNetwork::Testnet,
            4_000_001
        )
        .is_ok());
    }
    fn zakura_node() -> Value {
        let hash = "ab".repeat(32);
        serde_json::json!({"chain":"test","blocks":4_000_000,"headers":4_000_000,"bestblockhash":hash,
            "header_chain":{"mode":"integrated","verified_best":{"height":4_000_000,"hash":hash},
                "header_best":{"height":4_000_000,"hash":hash},
                "alarms":{"resource_stalled":false,"header_best_body_unavailable":null,"migrated_pin_refuted":null}}})
    }
    #[test]
    fn absent_ibd_requires_body_verified_zakura_frontiers_and_clear_alarms() {
        let base = zakura_node();
        assert!(validate_local_sync(&base).is_ok());
        assert!(validate_economics_response(
            &base,
            &serde_json::json!({"miner":1.25}),
            PpsNetwork::Testnet,
            4_000_001
        )
        .is_ok());
        for ibd in [
            serde_json::json!(true),
            Value::Null,
            serde_json::json!("false"),
            serde_json::json!(0),
        ] {
            let mut value = base.clone();
            value["initialblockdownload"] = ibd;
            assert!(validate_local_sync(&value).is_err());
        }
        for (path, replacement) in [
            ("/header_chain/mode", serde_json::json!("headers-only")),
            (
                "/header_chain/verified_best/height",
                serde_json::json!(3_999_999),
            ),
            (
                "/header_chain/verified_best/hash",
                serde_json::json!("cd".repeat(32)),
            ),
            (
                "/header_chain/header_best/hash",
                serde_json::json!("cd".repeat(32)),
            ),
            (
                "/header_chain/header_best/height",
                serde_json::json!(4_000_001),
            ),
            (
                "/header_chain/alarms/resource_stalled",
                serde_json::json!(true),
            ),
            (
                "/header_chain/alarms/header_best_body_unavailable",
                serde_json::json!({}),
            ),
            (
                "/header_chain/alarms/migrated_pin_refuted",
                serde_json::json!({}),
            ),
            ("/headers", serde_json::json!(3_999_999)),
            ("/bestblockhash", serde_json::json!("not-a-hash")),
        ] {
            let mut value = base.clone();
            *value.pointer_mut(path).unwrap() = replacement;
            assert!(validate_local_sync(&value).is_err(), "{path}");
        }
        let mut value = base.clone();
        value["header_chain"]["alarms"]
            .as_object_mut()
            .unwrap()
            .remove("migrated_pin_refuted");
        assert!(validate_local_sync(&value).is_err());
        let mut value = base.clone();
        value["header_chain"]["finality_warning"] = Value::Null;
        assert!(validate_local_sync(&value).is_err());
        for gap in [24, 25] {
            let mut value = base.clone();
            value["headers"] = serde_json::json!(4_000_000 + gap);
            value["header_chain"]["header_best"]["height"] = value["headers"].clone();
            assert_eq!(validate_local_sync(&value).is_ok(), gap == 24);
        }
    }
    #[test]
    fn exact_subsidy_and_halved_miner_portion() {
        for (raw, zats) in [
            ("1.25", 125_000_000),
            ("0.625", 62_500_000),
            ("12.5", 1_250_000_000),
            ("0.00000001", 1),
            ("1", 100_000_000),
            ("1.250000000", 125_000_000),
        ] {
            assert_eq!(decimal_coins_to_zatoshis(raw), Ok(zats));
        }
    }
    #[test]
    fn exact_scientific_notation_only() {
        for (raw, zats) in [
            ("1e-8", 1),
            ("1.0e-8", 1),
            ("125e-2", 125_000_000),
            ("1e+1", 1_000_000_000),
        ] {
            assert_eq!(decimal_coins_to_zatoshis(raw), Ok(zats));
        }
        for raw in [
            "1e-9",
            "1.1e-8",
            "1e10000000",
            "1e-2147483648",
            "1e2147483647",
            "1e1e1",
        ] {
            assert_eq!(
                decimal_coins_to_zatoshis(raw),
                Err(PpsEconomicsError::InvalidDecimal)
            );
        }
    }
    #[test]
    fn no_negative_zero_overflow_or_sub_zatoshi_rounding() {
        for raw in [
            "-1",
            "-0",
            "+1",
            "0",
            "0.000000001",
            "1.250000001",
            "12.50000001",
            "NaN",
            "Infinity",
            " 1",
            "1 ",
            "1.",
            ".1",
            "01",
            "1..0",
            "999999999999999999999999999999999999999999999999",
        ] {
            assert_eq!(
                decimal_coins_to_zatoshis(raw),
                Err(PpsEconomicsError::InvalidDecimal),
                "{raw}"
            );
        }
    }
    #[test]
    fn json_number_never_rounds_overprecision_through_float() {
        let value: Value = serde_json::from_str(r#"{"miner":1.2500000000000000001}"#).unwrap();
        assert_eq!(
            validate_economics_response(&node("test"), &value, PpsNetwork::Testnet, 4_000_001),
            Err(PpsEconomicsError::InvalidDecimal)
        );
        let value: Value = serde_json::from_str(r#"{"miner":1e-8}"#).unwrap();
        assert_eq!(
            validate_economics_response(&node("test"), &value, PpsNetwork::Testnet, 4_000_001),
            Ok(1)
        );
    }
    #[test]
    fn network_identity_and_explicit_sync_required() {
        let subsidy = serde_json::json!({"miner":1.25});
        for (label, network) in [
            ("main", PpsNetwork::Mainnet),
            ("mainnet", PpsNetwork::Mainnet),
            ("test", PpsNetwork::Testnet),
            ("testnet", PpsNetwork::Testnet),
        ] {
            assert!(
                validate_economics_response(&node(label), &subsidy, network, 4_000_001).is_ok()
            );
        }
        assert_eq!(
            validate_economics_response(&node("main"), &subsidy, PpsNetwork::Testnet, 4_000_001),
            Err(PpsEconomicsError::NetworkMismatch)
        );
        for state in [
            Value::Bool(true),
            Value::Null,
            Value::String("false".into()),
            serde_json::json!(0),
        ] {
            let mut info = node("test");
            info["initialblockdownload"] = state;
            assert_eq!(
                validate_economics_response(&info, &subsidy, PpsNetwork::Testnet, 4_000_001),
                Err(PpsEconomicsError::NodeNotSynchronized)
            );
        }
        let mut info = node("test");
        info.as_object_mut().unwrap().remove("initialblockdownload");
        assert_eq!(
            validate_economics_response(&info, &subsidy, PpsNetwork::Testnet, 4_000_001),
            Err(PpsEconomicsError::NodeNotSynchronized)
        );
    }
    #[test]
    fn subsidy_miner_field_not_total_or_string_and_height_binding() {
        for bad in [
            serde_json::json!({"total":1.25}),
            serde_json::json!({"miner":"1.25"}),
            serde_json::json!({"miner":null}),
            serde_json::json!({"miner":true}),
        ] {
            assert_eq!(
                validate_economics_response(&node("test"), &bad, PpsNetwork::Testnet, 4_000_001),
                Err(PpsEconomicsError::InvalidSubsidy)
            );
        }
        let subsidy = serde_json::json!({"miner":1.25,"height":4_000_000});
        assert_eq!(
            validate_economics_response(&node("test"), &subsidy, PpsNetwork::Testnet, 4_000_001),
            Err(PpsEconomicsError::InvalidHeight)
        );
        for h in [0, 4_000_002, u64::MAX] {
            assert_eq!(
                validate_economics_response(
                    &node("test"),
                    &serde_json::json!({"miner":1.25}),
                    PpsNetwork::Testnet,
                    h
                ),
                Err(PpsEconomicsError::InvalidHeight)
            );
        }
    }
}
