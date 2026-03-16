use blake2b_simd::Params;
use sha2::{Digest, Sha256};
use tracing::debug;

/// Result of injecting a coinbase tag.
pub struct InjectionResult {
    /// The modified coinbase transaction hex.
    pub new_coinbase_hex: String,
    /// For v4: new txid (SHA256d of full tx). None for v5.
    pub new_txid: Option<String>,
    /// For v5: new hashBlockCommitments. None for v4.
    pub new_block_commitments: Option<String>,
}

/// Maximum scriptSig length we allow after tag injection.
const MAX_SCRIPT_SIG_LEN: usize = 100;

/// Inject a tag into the coinbase transaction's scriptSig.
///
/// For v5 transactions, recomputes the auth digest chain and returns
/// a new `hashBlockCommitments`. The `tx_auth_digests` parameter must
/// contain the auth digests (hex) of all non-coinbase transactions in
/// block order (from zebrad's `authdigest` field on each template tx).
///
/// For v4, returns a new txid.
///
/// `chain_history_root_hex` is needed for v5 block commitments computation.
pub fn inject_coinbase_tag(
    coinbase_hex: &str,
    tag: &[u8],
    chain_history_root_hex: &str,
    tx_auth_digests: &[String],
) -> Result<InjectionResult, String> {
    let data = hex::decode(coinbase_hex)
        .map_err(|e| format!("Invalid coinbase hex: {e}"))?;

    if data.len() < 4 {
        return Err("Coinbase too short".into());
    }

    // Detect version: v5 = 05000080, v4 = 04000080
    let is_v5 = data[0..4] == [0x05, 0x00, 0x00, 0x80];
    let is_v4 = data[0..4] == [0x04, 0x00, 0x00, 0x80];

    if !is_v5 && !is_v4 {
        return Err(format!(
            "Unknown tx version: {:02x}{:02x}{:02x}{:02x}",
            data[0], data[1], data[2], data[3]
        ));
    }

    // Skip header to reach tx_in_count
    // v5: version(4) + version_group_id(4) + consensus_branch_id(4) + lock_time(4) + expiry_height(4) = 20
    // v4: version(4) + version_group_id(4) = 8
    let header_len = if is_v5 { 20 } else { 8 };

    if data.len() < header_len + 1 {
        return Err("Coinbase too short for header".into());
    }

    // Read tx_in_count (should be 1 for coinbase)
    let (tx_in_count, cs_len) = read_compact_size(&data, header_len)?;
    if tx_in_count != 1 {
        return Err(format!("Expected 1 txin in coinbase, got {tx_in_count}"));
    }

    // Skip prevout (32-byte hash + 4-byte index = 36 bytes)
    let prevout_offset = header_len + cs_len;
    let script_sig_len_offset = prevout_offset + 36;

    if data.len() < script_sig_len_offset + 1 {
        return Err("Coinbase too short for prevout".into());
    }

    // Read scriptSig length
    let (script_sig_len, _sig_cs_len) = read_compact_size(&data, script_sig_len_offset)?;
    let script_sig_offset = script_sig_len_offset + _sig_cs_len;
    let script_sig_end = script_sig_offset + script_sig_len;

    if data.len() < script_sig_end {
        return Err("Coinbase too short for scriptSig".into());
    }

    // Check if tag already present
    let existing_sig = &data[script_sig_offset..script_sig_end];
    if existing_sig.windows(tag.len()).any(|w| w == tag) {
        debug!("Coinbase tag already present, skipping injection");
        return Ok(InjectionResult {
            new_coinbase_hex: coinbase_hex.to_string(),
            new_txid: None,
            new_block_commitments: None,
        });
    }

    // Check length limit
    let new_script_sig_len = script_sig_len + tag.len();
    if new_script_sig_len > MAX_SCRIPT_SIG_LEN {
        return Err(format!(
            "scriptSig would be {} bytes (max {})",
            new_script_sig_len, MAX_SCRIPT_SIG_LEN
        ));
    }

    // Build new scriptSig: original + tag bytes
    let mut new_script_sig = data[script_sig_offset..script_sig_end].to_vec();
    new_script_sig.extend_from_slice(tag);

    // New compactSize for scriptSig length (always 1 byte since max is 100)
    let new_cs = compact_size_byte(new_script_sig_len)?;

    // Reconstruct the transaction:
    // [everything before scriptSig length] + [new_cs] + [new_scriptSig] + [rest after old scriptSig]
    let mut new_data = Vec::with_capacity(data.len() + tag.len());
    new_data.extend_from_slice(&data[..script_sig_len_offset]);
    new_data.push(new_cs);
    new_data.extend_from_slice(&new_script_sig);
    new_data.extend_from_slice(&data[script_sig_end..]);

    if is_v5 {
        // Extract consensus_branch_id from coinbase bytes 8-11
        let consensus_branch_id = &data[8..12];

        // Compute new coinbase auth_digest
        let coinbase_auth = compute_coinbase_auth_digest(consensus_branch_id, new_cs, &new_script_sig);

        // Parse non-coinbase tx auth digests from template
        let mut leaves = Vec::with_capacity(1 + tx_auth_digests.len());
        leaves.push(coinbase_auth);
        for (i, ad_hex) in tx_auth_digests.iter().enumerate() {
            let ad_bytes = hex::decode(ad_hex)
                .map_err(|e| format!("Invalid authdigest hex for tx {i}: {e}"))?;
            if ad_bytes.len() != 32 {
                return Err(format!("authdigest for tx {i} is {} bytes, expected 32", ad_bytes.len()));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&ad_bytes);
            leaves.push(arr);
        }

        // Build auth data merkle root
        let auth_data_root = auth_data_merkle_root(&leaves);

        // block_commitments = BLAKE2b("ZcashBlockCommit", chain_history_root || auth_data_root || [0; 32])
        let chain_history_root = hex::decode(chain_history_root_hex)
            .map_err(|e| format!("Invalid chain_history_root hex: {e}"))?;
        if chain_history_root.len() != 32 {
            return Err(format!(
                "chain_history_root must be 32 bytes, got {}",
                chain_history_root.len()
            ));
        }

        let mut commit_input = [0u8; 96];
        commit_input[..32].copy_from_slice(&chain_history_root);
        commit_input[32..64].copy_from_slice(&auth_data_root);
        // commit_input[64..96] already zero (terminator)
        let block_commitments = blake2b_256(b"ZcashBlockCommit", &commit_input);

        Ok(InjectionResult {
            new_coinbase_hex: hex::encode(&new_data),
            new_txid: None,
            new_block_commitments: Some(hex::encode(block_commitments)),
        })
    } else {
        // v4: txid = SHA256d of the full serialized tx (displayed in reverse byte order)
        let txid_bytes = sha256d(&new_data);
        let txid_hex = hex::encode(txid_bytes.iter().rev().copied().collect::<Vec<u8>>());

        Ok(InjectionResult {
            new_coinbase_hex: hex::encode(&new_data),
            new_txid: Some(txid_hex),
            new_block_commitments: None,
        })
    }
}

/// Compute the auth_digest of a coinbase transaction after scriptSig modification.
///
/// coinbase has no sapling/orchard data, so those auth digests are [0; 32].
fn compute_coinbase_auth_digest(
    consensus_branch_id: &[u8],
    script_sig_cs: u8,
    new_script_sig: &[u8],
) -> [u8; 32] {
    // transparent_scripts_digest = BLAKE2b("ZTxAuthTransHash", compactSize(len) || scriptSig)
    let mut scripts_input = Vec::with_capacity(1 + new_script_sig.len());
    scripts_input.push(script_sig_cs);
    scripts_input.extend_from_slice(new_script_sig);
    let transparent_scripts_digest = blake2b_256(b"ZTxAuthTransHash", &scripts_input);

    // auth_digest = BLAKE2b("ZTxAuthHash_" || branch_id, transparent || sapling([0;32]) || orchard([0;32]))
    let mut auth_perso = [0u8; 16];
    auth_perso[..12].copy_from_slice(b"ZTxAuthHash_");
    auth_perso[12..16].copy_from_slice(consensus_branch_id);

    let mut auth_input = [0u8; 96];
    auth_input[..32].copy_from_slice(&transparent_scripts_digest);
    // auth_input[32..96] already zero (sapling + orchard auth digests for coinbase)
    blake2b_256(&auth_perso, &auth_input)
}

/// Compute the auth data merkle root from a list of transaction auth digests.
///
/// Uses a perfect binary tree padded with [0; 32] to the next power of 2.
/// Hash function: BLAKE2b-256 with personalization "ZcashAuthDatHash".
fn auth_data_merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return [0u8; 32];
    }

    // Pad to next power of 2 (minimum 2) with zero leaves
    let n = leaves.len().next_power_of_two().max(2);
    let mut current: Vec<[u8; 32]> = Vec::with_capacity(n);
    current.extend_from_slice(leaves);
    current.resize(n, [0u8; 32]);

    // Build tree bottom-up
    while current.len() > 1 {
        let mut next = Vec::with_capacity(current.len() / 2);
        for pair in current.chunks(2) {
            let mut input = [0u8; 64];
            input[..32].copy_from_slice(&pair[0]);
            input[32..64].copy_from_slice(&pair[1]);
            next.push(blake2b_256(b"ZcashAuthDatHash", &input));
        }
        current = next;
    }

    current[0]
}

/// Read a compactSize integer at the given offset. Returns (value, bytes_consumed).
fn read_compact_size(data: &[u8], offset: usize) -> Result<(usize, usize), String> {
    if offset >= data.len() {
        return Err("read_compact_size: offset beyond data".into());
    }
    match data[offset] {
        0..=252 => Ok((data[offset] as usize, 1)),
        0xFD => {
            if offset + 3 > data.len() {
                return Err("Truncated compactSize (FD)".into());
            }
            let val = u16::from_le_bytes([data[offset + 1], data[offset + 2]]) as usize;
            Ok((val, 3))
        }
        0xFE => {
            if offset + 5 > data.len() {
                return Err("Truncated compactSize (FE)".into());
            }
            let val = u32::from_le_bytes([
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
            ]) as usize;
            Ok((val, 5))
        }
        0xFF => {
            if offset + 9 > data.len() {
                return Err("Truncated compactSize (FF)".into());
            }
            let val = u64::from_le_bytes(data[offset + 1..offset + 9].try_into().unwrap()) as usize;
            Ok((val, 9))
        }
    }
}

/// Encode a value as a 1-byte compactSize. Errors if value >= 253.
fn compact_size_byte(n: usize) -> Result<u8, String> {
    if n >= 253 {
        return Err(format!("compact_size_byte: value {n} too large for 1 byte"));
    }
    Ok(n as u8)
}

/// BLAKE2b-256 with a 16-byte personalization string.
fn blake2b_256(personalization: &[u8], data: &[u8]) -> [u8; 32] {
    let hash = Params::new()
        .hash_length(32)
        .personal(personalization)
        .hash(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(hash.as_bytes());
    out
}

fn sha256d(data: &[u8]) -> [u8; 32] {
    let first = Sha256::digest(data);
    let second = Sha256::digest(first);
    let mut out = [0u8; 32];
    out.copy_from_slice(&second);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_compact_size() {
        assert_eq!(read_compact_size(&[42], 0).unwrap(), (42, 1));
        assert_eq!(read_compact_size(&[252], 0).unwrap(), (252, 1));
        let mut data = vec![0xFD, 0x2C, 0x01];
        assert_eq!(read_compact_size(&data, 0).unwrap(), (300, 3));
        data.insert(0, 0xFF);
        assert_eq!(read_compact_size(&data, 1).unwrap(), (300, 3));
    }

    #[test]
    fn test_compact_size_byte() {
        assert_eq!(compact_size_byte(0).unwrap(), 0);
        assert_eq!(compact_size_byte(100).unwrap(), 100);
        assert_eq!(compact_size_byte(252).unwrap(), 252);
        assert!(compact_size_byte(253).is_err());
    }

    #[test]
    fn test_auth_data_merkle_root_single() {
        // 1 leaf → padded to 2 with [0;32], root = hash(leaf || zeros)
        let leaf = [0xAA; 32];
        let root = auth_data_merkle_root(&[leaf]);

        let mut expected_input = [0u8; 64];
        expected_input[..32].copy_from_slice(&leaf);
        let expected = blake2b_256(b"ZcashAuthDatHash", &expected_input);
        assert_eq!(root, expected);
    }

    #[test]
    fn test_auth_data_merkle_root_two() {
        // 2 leaves → already power of 2, root = hash(leaf0 || leaf1)
        let leaf0 = [0xAA; 32];
        let leaf1 = [0xBB; 32];
        let root = auth_data_merkle_root(&[leaf0, leaf1]);

        let mut expected_input = [0u8; 64];
        expected_input[..32].copy_from_slice(&leaf0);
        expected_input[32..64].copy_from_slice(&leaf1);
        let expected = blake2b_256(b"ZcashAuthDatHash", &expected_input);
        assert_eq!(root, expected);
    }

    #[test]
    fn test_auth_data_merkle_root_three() {
        // 3 leaves → padded to 4: hash(hash(l0||l1) || hash(l2||zeros))
        let l0 = [0xAA; 32];
        let l1 = [0xBB; 32];
        let l2 = [0xCC; 32];
        let root = auth_data_merkle_root(&[l0, l1, l2]);

        let mut input01 = [0u8; 64];
        input01[..32].copy_from_slice(&l0);
        input01[32..64].copy_from_slice(&l1);
        let h01 = blake2b_256(b"ZcashAuthDatHash", &input01);

        let mut input2z = [0u8; 64];
        input2z[..32].copy_from_slice(&l2);
        let h2z = blake2b_256(b"ZcashAuthDatHash", &input2z);

        let mut input_top = [0u8; 64];
        input_top[..32].copy_from_slice(&h01);
        input_top[32..64].copy_from_slice(&h2z);
        let expected = blake2b_256(b"ZcashAuthDatHash", &input_top);

        assert_eq!(root, expected);
    }
}
