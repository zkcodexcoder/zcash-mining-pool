//! Pure, testnet-only checks for the pinned zecd 0.7.0 conventional-fee route.
//!
//! No RPCs or key operations. A validated profile is NOT evidence that a running
//! wallet has that configuration: the caller must independently pin and verify
//! its effective configuration. Reserve the entire ceiling before requesting a
//! send. Post-send verification cannot retroactively enforce a spending limit.
//! Raw transaction checks do not verify proofs, signatures, chain inclusion, or
//! ownership of encrypted change. Those require the pinned wallet and canonical
//! node checks at the integration boundary; this is not the mainnet PCZT path.
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
};
use zcash_address::{ConversionError, TryFromAddress, ZcashAddress};
use zcash_primitives::transaction::{
    fees::{zip317, FeeRule},
    Transaction, TxVersion,
};
use zcash_protocol::consensus::{BranchId, NetworkType, TEST_NETWORK};

const MAX_MONEY: i64 = 21_000_000 * 100_000_000;
const MAX_RAW_BYTES: usize = 2_000_000;
// V5/V6 Sapling spend: 96 effect bytes + 192 proof + 64 authorization.
// Sapling outputs require 948 bytes; Orchard/Ironwood actions at least 884.
// Shared headers, anchors, binding signatures, and proofs only increase size.
const MIN_SHIELDED_ACTION_BYTES: usize = 352;

#[derive(Clone, Copy)]
enum FeeProfile {
    Restricted { action_limit: usize },
    ConsensusSize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConventionalError {
    #[error("testnet conventional profile unsupported")]
    Profile,
    #[error("testnet conventional payout unsupported")]
    Payout,
    #[error("testnet conventional transaction invalid")]
    Transaction,
    #[error("testnet conventional fee invalid")]
    Fee,
    #[error("testnet conventional recipients mismatch")]
    Recipient,
    /// Inadequate pinned-wallet evidence, not a proven raw financial violation.
    /// The caller must HOLD the reservation, never release it or infer a fee halt.
    #[error("testnet conventional wallet history unavailable or inconsistent")]
    WalletHistory,
}

/// Structural validation only; never use configuration-supplied values as a
/// substitute for independently verified runtime profile evidence.
#[derive(Clone, Copy)]
pub struct TestnetConventionalProfile {
    kind: FeeProfile,
    max_recipients: usize,
}
impl TestnetConventionalProfile {
    pub fn validate(
        network: &str,
        action_limit: usize,
        cache_proving_key: bool,
        sapling_enabled: bool,
        max_recipients: usize,
    ) -> Result<Self, ConventionalError> {
        // Deliberately bounded subset of zecd's configurable usize range.
        if network != "testnet"
            || !cache_proving_key
            || sapling_enabled
            || !(1..=50).contains(&action_limit)
            || !(1..=100).contains(&max_recipients)
        {
            return Err(ConventionalError::Profile);
        }
        Ok(Self {
            kind: FeeProfile::Restricted { action_limit },
            max_recipients,
        })
    }

    /// Conservative whole-transaction bound for the pinned zecd 0.7.0
    /// shielded-source, non-TEX, single-step standard ZIP-317 send path.
    ///
    /// This does not depend on zecd's optional Orchard action cap or disabled
    /// Sapling. V5/V6 consensus limits an includable transaction to at most
    /// 2,000,000 bytes. Every Sapling spend costs at least 352 serialized bytes;
    /// every Sapling output and Orchard/Ironwood action costs more. Thus
    /// max(Sapling spends, Sapling outputs) + Orchard + Ironwood is bounded by
    /// floor(2,000,000 / 352). At most N standard P2PKH/P2SH outputs add N.
    ///
    /// The integration must pin the wallet's standard-fee implementation and
    /// reserve this ceiling BEFORE sending. It is not a limit on arbitrary fee
    /// overpayment, unrelated wallet sends, or more than one transaction.
    /// Oversized or ambiguous sends remain reserved until conclusively resolved.
    pub fn consensus_size_bound(
        network: &str,
        max_recipients: usize,
    ) -> Result<Self, ConventionalError> {
        if network != "testnet" || !(1..=100).contains(&max_recipients) {
            return Err(ConventionalError::Profile);
        }
        Ok(Self {
            kind: FeeProfile::ConsensusSize,
            max_recipients,
        })
    }

    pub fn identifier(&self) -> &'static str {
        match self.kind {
            FeeProfile::Restricted { .. } => "restricted-family-v1",
            FeeProfile::ConsensusSize => "consensus-size-v1",
        }
    }

    pub fn fee_ceiling_zatoshis(&self, recipients: usize) -> Result<i64, ConventionalError> {
        if recipients == 0 || recipients > self.max_recipients {
            return Err(ConventionalError::Payout);
        }
        // The pinned actor caps combined real family spends and real outputs
        // separately at A. Each bundle requires at most spends+outputs actions,
        // plus at most two padding actions. Count BOTH bundles, including the
        // post-NU6.3 disjoint-spend/output Orchard case. Standard t outputs add
        // at most N logical actions. This is intentionally a conservative bound.
        let shielded_actions = match self.kind {
            FeeProfile::Restricted { action_limit } => {
                action_limit.checked_mul(2).and_then(|n| n.checked_add(4))
            }
            FeeProfile::ConsensusSize => Some(MAX_RAW_BYTES / MIN_SHIELDED_ACTION_BYTES),
        };
        let actions = shielded_actions
            .and_then(|n| n.checked_add(recipients))
            .ok_or(ConventionalError::Fee)?;
        i64::try_from(actions.max(2))
            .ok()
            .and_then(|n| n.checked_mul(5_000))
            .ok_or(ConventionalError::Fee)
    }
}

struct RecipientScript(Vec<u8>);
impl TryFromAddress for RecipientScript {
    type Error = ();
    fn try_from_transparent_p2pkh(
        _: NetworkType,
        hash: [u8; 20],
    ) -> Result<Self, ConversionError<()>> {
        let mut bytes = vec![0x76, 0xa9, 0x14];
        bytes.extend(hash);
        bytes.extend([0x88, 0xac]);
        Ok(Self(bytes))
    }
    fn try_from_transparent_p2sh(
        _: NetworkType,
        hash: [u8; 20],
    ) -> Result<Self, ConversionError<()>> {
        let mut bytes = vec![0xa9, 0x14];
        bytes.extend(hash);
        bytes.push(0x87);
        Ok(Self(bytes))
    }
}
struct ShieldedSource;
impl TryFromAddress for ShieldedSource {
    type Error = ();
    fn try_from_sapling(_: NetworkType, _: [u8; 43]) -> Result<Self, ConversionError<()>> {
        Ok(Self)
    }
    fn try_from_unified(
        _: NetworkType,
        address: zcash_address::unified::Address,
    ) -> Result<Self, ConversionError<()>> {
        use zcash_address::unified::{Container, Receiver};
        if address
            .items_as_parsed()
            .iter()
            .any(|r| matches!(r, Receiver::Sapling(_) | Receiver::Orchard(_)))
        {
            Ok(Self)
        } else {
            Err(ConversionError::User(()))
        }
    }
}
fn canonical(address: &str) -> Result<ZcashAddress, ConventionalError> {
    if address.is_empty() || address.len() > 2048 {
        return Err(ConventionalError::Payout);
    }
    let parsed = ZcashAddress::try_from_encoded(address).map_err(|_| ConventionalError::Payout)?;
    if parsed.encode() != address {
        return Err(ConventionalError::Payout);
    }
    Ok(parsed)
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum RecipientReceiver {
    Transparent(Vec<u8>),
    Sapling([u8; 43]),
    Orchard([u8; 43]),
}

struct RecipientAddress {
    receivers: BTreeSet<RecipientReceiver>,
    unified: bool,
    unknown: bool,
}
impl RecipientAddress {
    fn bare(receiver: RecipientReceiver) -> Self {
        Self {
            receivers: BTreeSet::from([receiver]),
            unified: false,
            unknown: false,
        }
    }
    fn has_shielded(&self) -> bool {
        self.receivers
            .iter()
            .any(|r| !matches!(r, RecipientReceiver::Transparent(_)))
    }
    fn payable(&self) -> BTreeSet<RecipientReceiver> {
        self.receivers
            .iter()
            .filter(|r| {
                // A UA is always a shielded destination on this route. Never fall
                // back to its transparent component, even if that component exists.
                !self.unified || !matches!(r, RecipientReceiver::Transparent(_))
            })
            .cloned()
            .collect()
    }
}
impl TryFromAddress for RecipientAddress {
    type Error = ();
    fn try_from_transparent_p2pkh(
        n: NetworkType,
        hash: [u8; 20],
    ) -> Result<Self, ConversionError<()>> {
        Ok(Self::bare(RecipientReceiver::Transparent(
            RecipientScript::try_from_transparent_p2pkh(n, hash)?.0,
        )))
    }
    fn try_from_transparent_p2sh(
        n: NetworkType,
        hash: [u8; 20],
    ) -> Result<Self, ConversionError<()>> {
        Ok(Self::bare(RecipientReceiver::Transparent(
            RecipientScript::try_from_transparent_p2sh(n, hash)?.0,
        )))
    }
    fn try_from_sapling(_: NetworkType, receiver: [u8; 43]) -> Result<Self, ConversionError<()>> {
        Ok(Self::bare(RecipientReceiver::Sapling(receiver)))
    }
    fn try_from_unified(
        n: NetworkType,
        address: zcash_address::unified::Address,
    ) -> Result<Self, ConversionError<()>> {
        use zcash_address::unified::{Container, Receiver};
        let mut decoded = Self {
            receivers: BTreeSet::new(),
            unified: true,
            unknown: false,
        };
        for item in address.items_as_parsed() {
            let receiver = match item {
                Receiver::P2pkh(hash) => RecipientReceiver::Transparent(
                    RecipientScript::try_from_transparent_p2pkh(n, *hash)?.0,
                ),
                Receiver::P2sh(hash) => RecipientReceiver::Transparent(
                    RecipientScript::try_from_transparent_p2sh(n, *hash)?.0,
                ),
                Receiver::Sapling(raw) => RecipientReceiver::Sapling(*raw),
                Receiver::Orchard(raw) => RecipientReceiver::Orchard(*raw),
                _ => {
                    decoded.unknown = true;
                    continue;
                }
            };
            decoded.receivers.insert(receiver);
        }
        if !decoded.has_shielded() {
            return Err(ConversionError::User(()));
        }
        Ok(decoded)
    }
}
/// Audit B6: stable keys for every receiver inside a testnet payout recipient, so
/// the payout selector can skip a recipient that overlaps one already selected
/// (for example a wallet's bare t-address and its UA) instead of letting the batch
/// builder reject the whole round.
pub fn recipient_receiver_keys(address: &str) -> Result<Vec<Vec<u8>>, ConventionalError> {
    Ok(decode_recipient(address)?
        .receivers
        .into_iter()
        .map(|receiver| match receiver {
            RecipientReceiver::Transparent(script) => [&[0u8][..], &script[..]].concat(),
            RecipientReceiver::Sapling(raw) => [&[1u8][..], &raw[..]].concat(),
            RecipientReceiver::Orchard(raw) => [&[2u8][..], &raw[..]].concat(),
        })
        .collect())
}

fn decode_recipient(address: &str) -> Result<RecipientAddress, ConventionalError> {
    let decoded = canonical(address)?
        .convert_if_network::<RecipientAddress>(NetworkType::Test)
        .map_err(|_| ConventionalError::Payout)?;
    // Address-container checksums and network encodings are not curve-point
    // validity. Reject an unpayable known receiver before any accounting credit.
    for receiver in &decoded.receivers {
        let valid = match receiver {
            RecipientReceiver::Transparent(_) => true,
            RecipientReceiver::Sapling(raw) => sapling::PaymentAddress::from_bytes(raw).is_some(),
            RecipientReceiver::Orchard(raw) => {
                bool::from(orchard::Address::from_raw_address_bytes(raw).is_some())
            }
        };
        if !valid {
            return Err(ConventionalError::Payout);
        }
    }
    Ok(decoded)
}

/// Admit canonical testnet bare transparent/Sapling addresses and UAs with a
/// known shielded receiver. TEX, unknown-only UAs and cross-network inputs fail.
/// This is structural address admission, not wallet ownership or spend authority.
pub fn validate_testnet_recipient(address: &str) -> Result<(), ConventionalError> {
    decode_recipient(address).map(|_| ())
}

/// Runtime-private: no Debug/Serialize implementation exposes payment details.
pub struct ConventionalPayoutExpectation {
    // Exact visible raw outputs. A UA never contributes a transparent output.
    recipients: BTreeMap<Vec<u8>, u64>,
    recipient_sets: Vec<(BTreeSet<RecipientReceiver>, u64)>,
    wallet_history: bool,
    target_height: u32,
    branch: BranchId,
    ceiling: i64,
    allow_sapling: bool,
}
impl ConventionalPayoutExpectation {
    /// `target_height` is the independently verified node tip + 1, captured and
    /// persisted with this attempt. Never replace it with a later recovery tip.
    /// This pure function checks source KIND only, not wallet ownership.
    pub fn new(
        profile: &TestnetConventionalProfile,
        source: &str,
        amounts: &[(String, i64)],
        target_height: u32,
    ) -> Result<Self, ConventionalError> {
        canonical(source)?
            .convert_if_network::<ShieldedSource>(NetworkType::Test)
            .map_err(|_| ConventionalError::Payout)?;
        if target_height == 0 || target_height > u32::MAX - 100 {
            return Err(ConventionalError::Payout);
        }
        let ceiling = profile.fee_ceiling_zatoshis(amounts.len())?;
        let branch = BranchId::for_height(&TEST_NETWORK, target_height.into());
        if !matches!(
            TxVersion::suggested_for_branch(branch),
            TxVersion::V5 | TxVersion::V6
        ) {
            return Err(ConventionalError::Payout);
        }
        let mut recipients = BTreeMap::new();
        let mut recipient_sets = Vec::new();
        let mut identities = BTreeSet::new();
        let mut wallet_history = false;
        let mut total = 0i64;
        for (address, amount) in amounts {
            if *amount <= 0 || *amount > MAX_MONEY {
                return Err(ConventionalError::Payout);
            }
            let decoded = decode_recipient(address)?;
            // Reject aliases and overlapping UA components, even components
            // that cannot be selected for payment on this shielded-only UA path.
            for receiver in &decoded.receivers {
                if !identities.insert(receiver.clone()) {
                    return Err(ConventionalError::Payout);
                }
            }
            wallet_history |= decoded.has_shielded();
            let payable = decoded.payable();
            if matches!(profile.kind, FeeProfile::Restricted { .. })
                && payable
                    .iter()
                    .all(|r| matches!(r, RecipientReceiver::Sapling(_)))
            {
                return Err(ConventionalError::Payout);
            }
            for receiver in &payable {
                if let RecipientReceiver::Transparent(script) = receiver {
                    recipients.insert(script.clone(), *amount as u64);
                }
            }
            recipient_sets.push((payable, *amount as u64));
            total = total
                .checked_add(*amount)
                .filter(|n| *n <= MAX_MONEY)
                .ok_or(ConventionalError::Payout)?;
        }
        if total.checked_add(ceiling).is_none_or(|n| n > MAX_MONEY) {
            return Err(ConventionalError::Payout);
        }
        Ok(Self {
            recipients,
            recipient_sets,
            wallet_history,
            target_height,
            branch,
            ceiling,
            allow_sapling: matches!(profile.kind, FeeProfile::ConsensusSize),
        })
    }
    pub fn fee_ceiling_zatoshis(&self) -> i64 {
        self.ceiling
    }
    pub fn requires_wallet_history(&self) -> bool {
        self.wallet_history
    }
}

/// Metadata-only result. Presence does not establish signature/proof validity or
/// canonical-chain inclusion, and cannot authorize releasing another attempt.
pub struct VerifiedConventionalPayout {
    txid: String,
    fee: i64,
}
impl VerifiedConventionalPayout {
    pub fn txid(&self) -> &str {
        &self.txid
    }
    pub fn actual_fee_zatoshis(&self) -> i64 {
        self.fee
    }
    pub fn conventional_fee_zatoshis(&self) -> i64 {
        self.fee
    }
}

pub fn verify_conventional_payout(
    raw_hex: &str,
    expected_txid: &str,
    expected: &ConventionalPayoutExpectation,
) -> Result<VerifiedConventionalPayout, ConventionalError> {
    let (_, verified) = inspect_conventional_raw(raw_hex, expected_txid, expected)?;
    if expected.requires_wallet_history() {
        return Err(ConventionalError::WalletHistory);
    }
    Ok(verified)
}

fn inspect_conventional_raw(
    raw_hex: &str,
    expected_txid: &str,
    expected: &ConventionalPayoutExpectation,
) -> Result<(Transaction, VerifiedConventionalPayout), ConventionalError> {
    let invalid = ConventionalError::Transaction;
    if raw_hex.is_empty()
        || raw_hex.len() > MAX_RAW_BYTES * 2
        || expected_txid.len() != 64
        || !expected_txid
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid);
    }
    let raw = hex::decode(raw_hex).map_err(|_| invalid)?;
    if hex::encode(&raw) != raw_hex {
        return Err(invalid);
    }
    let mut reader = Cursor::new(&raw);
    let tx = Transaction::read(&mut reader, expected.branch).map_err(|_| invalid)?;
    if reader.position() != raw.len() as u64
        || tx.txid().to_string() != expected_txid
        || tx.version() != TxVersion::suggested_for_branch(expected.branch)
        || tx.consensus_branch_id() != expected.branch
        || tx.lock_time() != 0
        || u32::from(tx.expiry_height()) < expected.target_height
        || u32::from(tx.expiry_height()) > expected.target_height + 100
        || tx.sprout_bundle().is_some()
        || (!expected.allow_sapling && tx.sapling_bundle().is_some())
        || tx.transparent_bundle().is_some_and(|b| !b.vin.is_empty())
        || (tx.sapling_bundle().is_none()
            && tx.orchard_bundle().is_none()
            && tx.ironwood_bundle().is_none())
    {
        return Err(invalid);
    }
    let mut rewritten = Vec::new();
    tx.write(&mut rewritten).map_err(|_| invalid)?;
    if rewritten != raw {
        return Err(invalid);
    }
    let mut actual = BTreeMap::new();
    let mut output_sizes = Vec::new();
    if let Some(bundle) = tx.transparent_bundle() {
        for output in &bundle.vout {
            let mut bytes = Vec::new();
            output.write(&mut bytes).map_err(|_| invalid)?;
            output_sizes.push(bytes.len());
            if actual
                .insert(
                    output.script_pubkey().0 .0.clone(),
                    u64::from(output.value()),
                )
                .is_some()
            {
                return Err(ConventionalError::Recipient);
            }
        }
    }
    // No extra transparent change, duplicate output, missing output or altered amount.
    if actual != expected.recipients {
        return Err(ConventionalError::Recipient);
    }
    let conventional = zip317::FeeRule::standard()
        .fee_required(
            &TEST_NETWORK,
            expected.target_height.into(),
            std::iter::empty::<zcash_primitives::transaction::fees::transparent::InputSize>(),
            output_sizes,
            tx.sapling_bundle().map_or(0, |b| b.shielded_spends().len()),
            tx.sapling_bundle()
                .map_or(0, |b| b.shielded_outputs().len()),
            tx.orchard_bundle().map_or(0, |b| b.actions().len()),
            tx.ironwood_bundle().map_or(0, |b| b.actions().len()),
        )
        .map_err(|_| ConventionalError::Fee)?;
    let actual_fee = tx
        .fee_paid::<zcash_protocol::value::BalanceError, _>(|_| Ok(None))
        .map_err(|_| ConventionalError::Fee)?
        .ok_or(ConventionalError::Fee)?;
    let fee = i64::try_from(u64::from(actual_fee)).map_err(|_| ConventionalError::Fee)?;
    if actual_fee != conventional || fee <= 0 || fee > expected.ceiling {
        return Err(ConventionalError::Fee);
    }
    Ok((
        tx,
        VerifiedConventionalPayout {
            txid: expected_txid.to_owned(),
            fee,
        },
    ))
}

fn canonical_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn negative_zatoshis(value: Option<&Value>) -> Result<i64, ConventionalError> {
    let bad = ConventionalError::WalletHistory;
    let text = value.and_then(Value::as_number).ok_or(bad)?.to_string();
    let positive = text.strip_prefix('-').ok_or(bad)?;
    let amount = crate::funding::exact_zatoshis(positive).map_err(|_| bad)?;
    if amount <= 0 {
        return Err(bad);
    }
    Ok(amount)
}

fn history_receiver(address: &str, pool: &str) -> Result<RecipientReceiver, ConventionalError> {
    let bad = ConventionalError::WalletHistory;
    let decoded = decode_recipient(address).map_err(|_| bad)?;
    // Pinned zecd normalizes outgoing history to the SINGLE paid receiver.
    // The original multi-receiver UA is not recoverable from ciphertext.
    if decoded.unknown || decoded.receivers.len() != 1 {
        return Err(bad);
    }
    let receiver = decoded.receivers.into_iter().next().ok_or(bad)?;
    if !matches!(
        (&receiver, pool, decoded.unified),
        (RecipientReceiver::Transparent(_), "transparent", false)
            | (RecipientReceiver::Sapling(_), "sapling", false)
            | (RecipientReceiver::Orchard(_), "orchard" | "ironwood", true)
    ) {
        return Err(bad);
    }
    Ok(receiver)
}

/// Verify raw consensus-size/header/ZIP-317/transparent-output constraints and
/// bind shielded recipients to the pinned sender wallet's mined `gettransaction`
/// history. A matching history is TRUSTED WALLET EVIDENCE, not independent
/// cryptographic proof of ciphertext contents, signatures, proofs or inclusion.
/// Caller must authenticate/pin that wallet, establish completed enhancement,
/// and compare its block hash and confirmations with the canonical node before
/// settlement. No viewing keys are exported or required by this verifier.
/// Missing, malformed, inconsistent or incomplete history yields WalletHistory:
/// HOLD the attempt; do not treat a wallet assertion as a proven raw fee breach.
pub fn verify_conventional_payout_with_wallet(
    raw_hex: &str,
    expected_txid: &str,
    expected: &ConventionalPayoutExpectation,
    wallet_tx: &Value,
) -> Result<VerifiedConventionalPayout, ConventionalError> {
    // Check independent raw invariants first: absent history must not hide an
    // actual raw fee violation. Wallet fee discrepancies alone are not Fee.
    let (tx, verified) = inspect_conventional_raw(raw_hex, expected_txid, expected)?;
    let bad = ConventionalError::WalletHistory;
    let wallet = wallet_tx.as_object().ok_or(bad)?;
    if wallet.get("txid").and_then(Value::as_str) != Some(expected_txid)
        || wallet.get("hex").and_then(Value::as_str) != Some(raw_hex)
        || wallet
            .get("confirmations")
            .and_then(Value::as_u64)
            .filter(|n| *n >= 1)
            .is_none()
        || !wallet
            .get("blockhash")
            .and_then(Value::as_str)
            .is_some_and(canonical_hash)
        || negative_zatoshis(wallet.get("fee"))? != verified.fee
    {
        return Err(bad);
    }
    let details = wallet.get("details").and_then(Value::as_array).ok_or(bad)?;
    if details.len() != expected.recipient_sets.len() {
        return Err(bad);
    }
    let mut paid = BTreeSet::new();
    let mut indices = BTreeSet::new();
    for detail in details {
        let detail = detail.as_object().ok_or(bad)?;
        if detail.get("category").and_then(Value::as_str) != Some("send")
            || detail.get("abandoned").and_then(Value::as_bool) != Some(false)
            || negative_zatoshis(detail.get("fee"))? != verified.fee
        {
            return Err(bad);
        }
        let amount = negative_zatoshis(detail.get("amount"))? as u64;
        let pool = detail.get("pool").and_then(Value::as_str).ok_or(bad)?;
        let index = detail
            .get("vout")
            .and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or(bad)?;
        let count = match pool {
            "transparent" => tx.transparent_bundle().map_or(0, |b| b.vout.len()),
            "sapling" => tx
                .sapling_bundle()
                .map_or(0, |b| b.shielded_outputs().len()),
            "orchard" => tx.orchard_bundle().map_or(0, |b| b.actions().len()),
            "ironwood" => tx.ironwood_bundle().map_or(0, |b| b.actions().len()),
            _ => return Err(bad),
        };
        if index >= count || !indices.insert((pool, index)) {
            return Err(bad);
        }
        let receiver = history_receiver(
            detail.get("address").and_then(Value::as_str).ok_or(bad)?,
            pool,
        )?;
        // Bind transparent history to the EXACT indexed raw output, not merely
        // another output with the same amount in this transaction.
        if let RecipientReceiver::Transparent(script) = &receiver {
            let output = &tx.transparent_bundle().ok_or(bad)?.vout[index];
            if output.script_pubkey().0 .0 != *script || u64::from(output.value()) != amount {
                return Err(bad);
            }
        }
        let matched = expected
            .recipient_sets
            .iter()
            .enumerate()
            .find(|(_, (receivers, value))| *value == amount && receivers.contains(&receiver))
            .map(|(i, _)| i)
            .ok_or(bad)?;
        if !paid.insert(matched) {
            return Err(bad);
        }
    }
    if paid.len() != expected.recipient_sets.len() {
        return Err(bad);
    }
    Ok(verified)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use zcash_address::ToAddress;
    fn profile() -> TestnetConventionalProfile {
        TestnetConventionalProfile::validate("testnet", 50, true, false, 100).unwrap()
    }
    fn size_profile() -> TestnetConventionalProfile {
        TestnetConventionalProfile::consensus_size_bound("testnet", 100).unwrap()
    }
    fn source() -> String {
        ZcashAddress::from_sapling(NetworkType::Test, [1; 43]).encode()
    }
    fn recipient() -> String {
        ZcashAddress::from_transparent_p2pkh(NetworkType::Test, [2; 20]).encode()
    }
    fn expected() -> ConventionalPayoutExpectation {
        ConventionalPayoutExpectation::new(
            &profile(),
            &source(),
            &[(recipient(), 100_000)],
            2_000_000,
        )
        .unwrap()
    }
    /// Shared synthetic parser fixture only; never a production wallet proof.
    pub(crate) fn signer_receipt_fixture() -> (String, String, u32, String, String) {
        let (raw, txid, height) = raw_fixture(false, 2, 0, 15_000, 100_000);
        (raw, txid, height, source(), recipient())
    }
    fn unified_address(
        network: NetworkType,
        sapling: Option<[u8; 43]>,
        orchard: Option<[u8; 43]>,
    ) -> String {
        use zcash_address::unified::{Address, Encoding, Receiver};
        let mut receivers = Vec::new();
        if let Some(raw) = sapling {
            receivers.push(Receiver::Sapling(raw));
        }
        if let Some(raw) = orchard {
            receivers.push(Receiver::Orchard(raw));
        }
        ZcashAddress::from_unified(network, Address::try_from_items(receivers).unwrap()).encode()
    }
    fn fixture_sapling(tag: u8) -> [u8; 43] {
        sapling::zip32::ExtendedSpendingKey::master(&[tag; 32])
            .default_address()
            .1
            .to_bytes()
    }
    fn fixture_orchard(tag: u8) -> [u8; 43] {
        let sk = orchard::keys::SpendingKey::from_bytes([tag; 32]).unwrap();
        orchard::keys::FullViewingKey::from(&sk)
            .address_at(0u32, orchard::keys::Scope::External)
            .to_raw_address_bytes()
    }
    fn fixture_wallet(raw: &str, txid: &str, receiver: &str, pool: &str, fee: i64) -> Value {
        fn negative(amount: i64) -> Value {
            serde_json::from_str(&format!(
                "-{}.{:08}",
                amount / 100_000_000,
                amount % 100_000_000
            ))
            .unwrap()
        }
        serde_json::json!({
            "txid": txid, "hex": raw, "confirmations": 10, "blockhash": "a".repeat(64),
            "fee": negative(fee),
            "details": [{ "category": "send", "address": receiver,
                "amount": negative(100_000), "vout": 0, "pool": pool,
                "abandoned": false, "fee": negative(fee) }]
        })
    }
    /// Public synthetic parser fixture only: fixed 100,000-zatoshi shielded
    /// payment, dummy proofs/ciphertexts, and an asserted wallet view. Never a
    /// wallet proof. Return raw, txid, height, source, original UA, wallet view.
    pub(crate) fn shielded_receipt_fixture(
        pool: &str,
    ) -> (String, String, u32, String, String, Value) {
        use zcash_primitives::transaction::Authorized;
        let (v6, orchard, ironwood) = match pool {
            "orchard" => (false, 2, 0),
            "ironwood" => (true, 0, 2),
            "sapling" => (false, 0, 0),
            _ => panic!("unsupported synthetic pool"),
        };
        let fee = 10_000;
        let (base, _, height) = raw_fixture(v6, orchard, ironwood, fee, 0);
        let branch = BranchId::for_height(&TEST_NETWORK, height.into());
        let tx = Transaction::read(Cursor::new(hex::decode(base).unwrap()), branch).unwrap();
        let tx = tx
            .into_data()
            .map_bundles::<Authorized>(|_| None, |b| b, |b| b)
            .freeze()
            .unwrap();
        let mut bytes = Vec::new();
        tx.write(&mut bytes).unwrap();
        let (raw, txid) = if pool == "sapling" {
            add_sapling(&hex::encode(bytes), height, 1, 1, fee)
        } else {
            (hex::encode(bytes), tx.txid().to_string())
        };
        let ua = unified_address(
            NetworkType::Test,
            Some(fixture_sapling(4)),
            Some(fixture_orchard(5)),
        );
        let receiver = if pool == "sapling" {
            ZcashAddress::from_sapling(NetworkType::Test, fixture_sapling(4)).encode()
        } else {
            unified_address(NetworkType::Test, None, Some(fixture_orchard(5)))
        };
        let wallet = fixture_wallet(&raw, &txid, &receiver, pool, fee);
        (raw, txid, height, source(), ua, wallet)
    }
    // Structurally valid synthetic transactions with intentionally dummy proof
    // and signature bytes. They exercise the real upstream parser/serializer,
    // NOT consensus acceptance. Canonical node acceptance remains mandatory.
    fn raw_fixture(
        v6: bool,
        orchard_count: usize,
        ironwood_count: usize,
        fee: i64,
        output_amount: u64,
    ) -> (String, String, u32) {
        use orchard::{
            bundle::{Authorized as OrchardAuth, BundleVersion},
            note::{ExtractedNoteCommitment, Nullifier, TransmittedNoteCiphertext},
        };
        use zcash_primitives::transaction::{Authorized, TransactionData};
        use zcash_protocol::{
            consensus::{NetworkUpgrade, Parameters},
            value::{ZatBalance, Zatoshis},
        };
        let sk = orchard::keys::SpendingKey::from_bytes([7; 32]).unwrap();
        let fvk = orchard::keys::FullViewingKey::from(&sk);
        let addr = fvk.address_at(0u32, orchard::keys::Scope::External);
        let ask = orchard::keys::SpendAuthorizingKey::from(&sk);
        let randomized = ask.randomize(&Default::default());
        let action = orchard::Action::from_parts(
            Nullifier::from_bytes(&[0; 32]).unwrap(),
            (&randomized).into(),
            ExtractedNoteCommitment::from_bytes(&[0; 32]).unwrap(),
            TransmittedNoteCiphertext {
                epk_bytes: addr.to_raw_address_bytes()[11..].try_into().unwrap(),
                enc_ciphertext: [0; 580],
                out_ciphertext: [0; 80],
            },
            orchard::value::ValueCommitment::from_bytes(&[0; 32]).unwrap(),
            [0; 64].into(),
        )
        .unwrap();
        let bundle = |count: usize, version: BundleVersion, balance: i64| {
            (count > 0).then(|| {
                orchard::Bundle::try_from_parts(
                    (action.clone(), vec![action.clone(); count - 1]).into(),
                    version.default_flags(),
                    ZatBalance::from_i64(balance).unwrap(),
                    orchard::Anchor::from_bytes([0; 32]).unwrap(),
                    OrchardAuth::from_parts(
                        orchard::Proof::new(vec![0; orchard::Proof::expected_proof_size(count)]),
                        [0; 64].into(),
                    ),
                    version,
                )
                .unwrap()
            })
        };
        let height = if v6 {
            u32::from(
                TEST_NETWORK
                    .activation_height(NetworkUpgrade::Nu6_3)
                    .unwrap(),
            ) + 100
        } else {
            2_000_000
        };
        let transparent = Some(transparent::bundle::Bundle {
            vin: vec![],
            vout: vec![transparent::bundle::TxOut::new(
                Zatoshis::from_u64(output_amount).unwrap(),
                transparent::address::TransparentAddress::PublicKeyHash([2; 20])
                    .script()
                    .into(),
            )],
            authorization: transparent::bundle::Authorized,
        });
        let total = output_amount as i64 + fee;
        let orchard = bundle(
            orchard_count,
            if v6 {
                BundleVersion::orchard_v3()
            } else {
                BundleVersion::orchard_v2()
            },
            if orchard_count > 0 { total } else { 0 },
        );
        let ironwood = bundle(
            ironwood_count,
            BundleVersion::ironwood_v3(),
            if orchard_count == 0 { total } else { 0 },
        );
        let data = if v6 {
            TransactionData::<Authorized>::from_parts_v6(
                BranchId::for_height(&TEST_NETWORK, height.into()),
                0,
                (height + 40).into(),
                transparent,
                None,
                orchard,
                ironwood,
            )
        } else {
            TransactionData::<Authorized>::from_parts(
                TxVersion::V5,
                BranchId::for_height(&TEST_NETWORK, height.into()),
                0,
                (height + 40).into(),
                transparent,
                None,
                None,
                orchard,
            )
        };
        let tx = data.freeze().unwrap();
        let mut bytes = Vec::new();
        tx.write(&mut bytes).unwrap();
        (hex::encode(bytes), tx.txid().to_string(), height)
    }

    // Synthetic Sapling effect data, with dummy proofs/signatures, serialized
    // according to the upstream V5/V6 format. The valid public point is cv from
    // sapling-crypto 0.7.0 src/test_vectors/note_encryption.rs, first vector.
    // No wallet, private key, signing, or consensus-valid transaction is used.
    fn add_sapling(
        raw: &str,
        height: u32,
        spends: usize,
        outputs: usize,
        balance: i64,
    ) -> (String, String) {
        assert!(spends + outputs > 0);
        let point = hex::decode("a9cb0d137232ff8448d0f078b6814c66cb331b0f2d3d8a085bedba815f00a8db")
            .unwrap();
        let branch = BranchId::for_height(&TEST_NETWORK, height.into());
        let bytes = hex::decode(raw).unwrap();
        let base = Transaction::read(Cursor::new(&bytes), branch).unwrap();
        assert!(base.sapling_bundle().is_none());
        let mut prefix = Vec::new();
        if base.version() == TxVersion::V6 {
            base.write_v6_header(&mut prefix).unwrap();
        } else {
            base.write_v5_header(&mut prefix).unwrap();
        }
        base.write_transparent(&mut prefix).unwrap();
        assert_eq!(&bytes[..prefix.len()], prefix);
        assert_eq!(&bytes[prefix.len()..prefix.len() + 2], &[0, 0]);
        let mut sapling = Vec::new();
        let compact_size = |out: &mut Vec<u8>, count: usize| {
            if count < 253 {
                out.push(count as u8);
            } else {
                out.push(253);
                out.extend(u16::try_from(count).unwrap().to_le_bytes());
            }
        };
        compact_size(&mut sapling, spends);
        for _ in 0..spends {
            sapling.extend(&point); // value commitment
            sapling.extend([0; 32]); // nullifier
            sapling.extend(&point); // randomized verification key
        }
        compact_size(&mut sapling, outputs);
        for _ in 0..outputs {
            sapling.extend(&point);
            sapling.extend([0; 32]); // canonical note commitment
            sapling.extend(&point); // ephemeral key
            sapling.extend([0; 580]);
            sapling.extend([0; 80]);
        }
        sapling.extend(balance.to_le_bytes());
        if spends > 0 {
            sapling.extend([0; 32]); // shared anchor
        }
        sapling.extend(vec![0; spends * 192]);
        sapling.extend(vec![0; spends * 64]);
        sapling.extend(vec![0; outputs * 192]);
        sapling.extend([0; 64]); // binding signature
        let end = prefix.len() + 2;
        prefix.extend(sapling);
        prefix.extend(&bytes[end..]);
        let tx = Transaction::read(Cursor::new(&prefix), branch).unwrap();
        let mut canonical = Vec::new();
        tx.write(&mut canonical).unwrap();
        assert_eq!(prefix, canonical);
        (hex::encode(prefix), tx.txid().to_string())
    }

    #[test]
    fn shielded_history_matches_multi_receiver_ua_and_bare_sapling() {
        for pool in ["orchard", "ironwood", "sapling"] {
            let (raw, txid, height, source, ua, wallet) = shielded_receipt_fixture(pool);
            let expected = ConventionalPayoutExpectation::new(
                &size_profile(),
                &source,
                &[(ua.clone(), 100_000)],
                height,
            )
            .unwrap();
            assert!(expected.requires_wallet_history());
            assert_eq!(
                verify_conventional_payout(&raw, &txid, &expected).err(),
                Some(ConventionalError::WalletHistory)
            );
            assert_eq!(
                verify_conventional_payout_with_wallet(&raw, &txid, &expected, &wallet)
                    .unwrap()
                    .actual_fee_zatoshis(),
                10_000
            );
            assert_ne!(wallet["details"][0]["address"].as_str(), Some(ua.as_str()));
            // A bare Sapling recipient also matches that exact Sapling receiver.
            if pool == "sapling" {
                let bare = wallet["details"][0]["address"].as_str().unwrap().to_owned();
                let e = ConventionalPayoutExpectation::new(
                    &size_profile(),
                    &source,
                    &[(bare, 100_000)],
                    height,
                )
                .unwrap();
                assert!(verify_conventional_payout_with_wallet(&raw, &txid, &e, &wallet).is_ok());
            }
            // The pinned wallet must report its normalized receiver, not its
            // cached original multi-receiver UA, on this history contract.
            let mut full = wallet.clone();
            full["details"][0]["address"] = Value::String(ua);
            assert_eq!(
                verify_conventional_payout_with_wallet(&raw, &txid, &expected, &full).err(),
                Some(ConventionalError::WalletHistory)
            );
        }
        assert!(!expected().requires_wallet_history());
    }

    #[test]
    fn shielded_recipient_admission_rejects_aliases_network_tex_and_unknown_only() {
        use zcash_address::unified::{Address, Encoding, Receiver};
        let ua = unified_address(
            NetworkType::Test,
            Some(fixture_sapling(4)),
            Some(fixture_orchard(5)),
        );
        let bare = ZcashAddress::from_sapling(NetworkType::Test, fixture_sapling(4)).encode();
        for valid in [&ua, &bare, &recipient()] {
            assert!(validate_testnet_recipient(valid).is_ok());
        }
        let unknown = Address::try_from_items(vec![Receiver::Unknown {
            typecode: 0x10,
            data: vec![1; 43],
        }])
        .unwrap();
        let known_and_unknown = Address::try_from_items(vec![
            Receiver::Orchard(fixture_orchard(5)),
            Receiver::Unknown {
                typecode: 0x10,
                data: vec![1; 43],
            },
        ])
        .unwrap();
        assert!(validate_testnet_recipient(
            &ZcashAddress::from_unified(NetworkType::Test, known_and_unknown).encode()
        )
        .is_ok());
        for invalid in [
            "".to_owned(),
            "private-payload".to_owned(),
            format!(" {ua}"),
            ua.to_uppercase(),
            ZcashAddress::from_unified(NetworkType::Test, unknown).encode(),
            ZcashAddress::from_sapling(NetworkType::Main, fixture_sapling(4)).encode(),
            unified_address(
                NetworkType::Main,
                Some(fixture_sapling(4)),
                Some(fixture_orchard(5)),
            ),
            ZcashAddress::from_sapling(NetworkType::Test, [0; 43]).encode(),
            unified_address(NetworkType::Test, None, Some([0xff; 43])),
            unified_address(NetworkType::Test, Some([0; 43]), Some(fixture_orchard(5))),
            unified_address(
                NetworkType::Test,
                Some(fixture_sapling(4)),
                Some([0xff; 43]),
            ),
            ZcashAddress::from_tex(NetworkType::Test, [4; 20]).encode(),
            ZcashAddress::from_transparent_p2pkh(NetworkType::Main, [2; 20]).encode(),
        ] {
            assert!(validate_testnet_recipient(&invalid).is_err());
        }
        for alias in [
            ua.clone(),
            bare,
            unified_address(
                NetworkType::Test,
                Some(fixture_sapling(4)),
                Some(fixture_orchard(6)),
            ),
            unified_address(
                NetworkType::Test,
                Some(fixture_sapling(6)),
                Some(fixture_orchard(5)),
            ),
        ] {
            assert!(ConventionalPayoutExpectation::new(
                &size_profile(),
                &source(),
                &[(ua.clone(), 1), (alias, 2)],
                2_000_000
            )
            .is_err());
        }
        // Even a non-payable transparent UA component may not alias another
        // expected recipient. A future policy must not silently activate it.
        let mixed = Address::try_from_items(vec![
            Receiver::P2pkh([2; 20]),
            Receiver::Orchard(fixture_orchard(5)),
        ])
        .unwrap();
        let mixed = ZcashAddress::from_unified(NetworkType::Test, mixed).encode();
        assert!(ConventionalPayoutExpectation::new(
            &size_profile(),
            &source(),
            &[(mixed, 1), (recipient(), 2)],
            2_000_000
        )
        .is_err());
    }

    #[test]
    fn wallet_evidence_malformed_incomplete_or_mismatching_always_holds() {
        let (raw, txid, height, source, ua, wallet) = shielded_receipt_fixture("orchard");
        let expected =
            ConventionalPayoutExpectation::new(&size_profile(), &source, &[(ua, 100_000)], height)
                .unwrap();
        let holds = |value: &Value| {
            assert_eq!(
                verify_conventional_payout_with_wallet(&raw, &txid, &expected, value).err(),
                Some(ConventionalError::WalletHistory)
            );
        };
        holds(&Value::Null);
        holds(&serde_json::json!({}));
        for field in [
            "hex",
            "txid",
            "fee",
            "details",
            "confirmations",
            "blockhash",
        ] {
            let mut changed = wallet.clone();
            changed.as_object_mut().unwrap().remove(field);
            holds(&changed);
        }
        for (field, value) in [
            ("hex", Value::String(format!("{raw}00"))),
            ("txid", Value::String("b".repeat(64))),
            ("confirmations", serde_json::json!(0)),
            ("confirmations", serde_json::json!(-1)),
            ("confirmations", serde_json::json!(1.5)),
            ("blockhash", Value::String("A".repeat(64))),
            ("blockhash", Value::Null),
            ("details", serde_json::json!([])),
            ("details", serde_json::json!({})),
        ] {
            let mut changed = wallet.clone();
            changed[field] = value;
            holds(&changed);
        }
        for field in [
            "address",
            "category",
            "pool",
            "vout",
            "amount",
            "fee",
            "abandoned",
        ] {
            let mut changed = wallet.clone();
            changed["details"][0].as_object_mut().unwrap().remove(field);
            holds(&changed);
        }
        for (field, value) in [
            ("category", serde_json::json!("receive")),
            ("category", serde_json::json!("generate")),
            ("abandoned", serde_json::json!(true)),
            ("pool", serde_json::json!("ironwood")),
            ("pool", serde_json::json!("sapling")),
            ("pool", serde_json::json!("unknown")),
            ("vout", serde_json::json!(2)),
            ("vout", serde_json::json!(-1)),
            ("vout", serde_json::json!(1.5)),
            (
                "address",
                Value::String(unified_address(
                    NetworkType::Test,
                    None,
                    Some(fixture_orchard(6)),
                )),
            ),
            (
                "address",
                Value::String(unified_address(
                    NetworkType::Main,
                    None,
                    Some(fixture_orchard(5)),
                )),
            ),
            ("address", Value::String(recipient())),
        ] {
            let mut changed = wallet.clone();
            changed["details"][0][field] = value;
            holds(&changed);
        }
        for text in [
            "0.001",
            "-0.00100001",
            "-0.001000000000000001",
            "-0.00099999",
            "0",
            "-0.0",
            "-21000001",
            "-1e100",
        ] {
            let mut changed = wallet.clone();
            changed["details"][0]["amount"] = serde_json::from_str(text).unwrap();
            holds(&changed);
        }
        for field in ["amount", "fee"] {
            let mut changed = wallet.clone();
            changed["details"][0][field] = serde_json::json!("-0.001");
            holds(&changed);
        }
        for path in [0, 1] {
            let mut changed = wallet.clone();
            if path == 0 {
                changed["fee"] = serde_json::from_str("-0.00010001").unwrap();
            } else {
                changed["details"][0]["fee"] = serde_json::from_str("-0.00010001").unwrap();
            }
            holds(&changed);
        }
        let mut extra = wallet.clone();
        extra["details"]
            .as_array_mut()
            .unwrap()
            .push(wallet["details"][0].clone());
        holds(&extra);
        // Scientific decimal notation retains exact zatoshis; never uses f64.
        let mut exact = wallet.clone();
        exact["details"][0]["amount"] = serde_json::from_str("-1e-3").unwrap();
        assert!(verify_conventional_payout_with_wallet(&raw, &txid, &expected, &exact).is_ok());
    }

    #[test]
    fn multiple_sends_require_bijective_receivers_and_unique_pool_indices() {
        let (raw, txid, height, source, ua, mut wallet) = shielded_receipt_fixture("orchard");
        let ua2 = unified_address(NetworkType::Test, None, Some(fixture_orchard(6)));
        let expected = ConventionalPayoutExpectation::new(
            &size_profile(),
            &source,
            &[(ua, 100_000), (ua2.clone(), 100_000)],
            height,
        )
        .unwrap();
        let mut other = wallet["details"][0].clone();
        other["address"] = Value::String(ua2);
        other["vout"] = serde_json::json!(1);
        wallet["details"].as_array_mut().unwrap().push(other);
        assert!(verify_conventional_payout_with_wallet(&raw, &txid, &expected, &wallet).is_ok());
        for same_receiver in [false, true] {
            let mut changed = wallet.clone();
            if same_receiver {
                changed["details"][1]["address"] = changed["details"][0]["address"].clone();
            } else {
                changed["details"][1]["vout"] = serde_json::json!(0);
            }
            assert_eq!(
                verify_conventional_payout_with_wallet(&raw, &txid, &expected, &changed).err(),
                Some(ConventionalError::WalletHistory)
            );
        }
    }

    #[test]
    fn mixed_transparent_and_shielded_sends_bind_raw_outputs_without_ua_fallback() {
        use zcash_address::unified::{Address, Encoding, Receiver};
        let (raw, txid, height) = raw_fixture(false, 2, 0, 15_000, 100_000);
        let ua = unified_address(
            NetworkType::Test,
            Some(fixture_sapling(4)),
            Some(fixture_orchard(5)),
        );
        let receiver = unified_address(NetworkType::Test, None, Some(fixture_orchard(5)));
        let expected = ConventionalPayoutExpectation::new(
            &size_profile(),
            &source(),
            &[(recipient(), 100_000), (ua, 100_000)],
            height,
        )
        .unwrap();
        let mut wallet = fixture_wallet(&raw, &txid, &recipient(), "transparent", 15_000);
        let shielded = fixture_wallet(&raw, &txid, &receiver, "orchard", 15_000);
        wallet["details"]
            .as_array_mut()
            .unwrap()
            .push(shielded["details"][0].clone());
        // Index zero in DIFFERENT pools is legitimate; indices are not global.
        assert!(verify_conventional_payout_with_wallet(&raw, &txid, &expected, &wallet).is_ok());
        let mut wrong = wallet.clone();
        wrong["details"][0]["address"] = Value::String(
            ZcashAddress::from_transparent_p2pkh(NetworkType::Test, [9; 20]).encode(),
        );
        assert_eq!(
            verify_conventional_payout_with_wallet(&raw, &txid, &expected, &wrong).err(),
            Some(ConventionalError::WalletHistory)
        );
        // Raw transparent recipients remain independently checkable, and a UA's
        // transparent component does not authorize even an exactly matching raw
        // transparent payment: this path requires its shielded component.
        let mixed = Address::try_from_items(vec![
            Receiver::P2pkh([2; 20]),
            Receiver::Orchard(fixture_orchard(5)),
        ])
        .unwrap();
        let mixed = ZcashAddress::from_unified(NetworkType::Test, mixed).encode();
        let no_fallback = ConventionalPayoutExpectation::new(
            &size_profile(),
            &source(),
            &[(mixed, 100_000)],
            height,
        )
        .unwrap();
        assert_eq!(
            verify_conventional_payout_with_wallet(&raw, &txid, &no_fallback, &wallet).err(),
            Some(ConventionalError::Recipient)
        );
        // The optional wallet-aware API is compatible with an entirely visible
        // payout too, but its raw-only predecessor needs no wallet assertion.
        let transparent = ConventionalPayoutExpectation::new(
            &size_profile(),
            &source(),
            &[(recipient(), 100_000)],
            height,
        )
        .unwrap();
        let visible_wallet = fixture_wallet(&raw, &txid, &recipient(), "transparent", 15_000);
        assert!(
            verify_conventional_payout_with_wallet(&raw, &txid, &transparent, &visible_wallet)
                .is_ok()
        );
        assert!(verify_conventional_payout(&raw, &txid, &transparent).is_ok());
    }

    #[test]
    fn raw_fee_violation_is_distinct_from_wallet_history_hold() {
        let (raw, txid, height, source, ua, wallet) = shielded_receipt_fixture("orchard");
        let expected =
            ConventionalPayoutExpectation::new(&size_profile(), &source, &[(ua, 100_000)], height)
                .unwrap();
        assert_eq!(
            verify_conventional_payout_with_wallet(&raw, &txid, &expected, &Value::Null).err(),
            Some(ConventionalError::WalletHistory)
        );
        let branch = BranchId::for_height(&TEST_NETWORK, height.into());
        let mut bytes = hex::decode(raw).unwrap();
        let tx = Transaction::read(Cursor::new(&bytes), branch).unwrap();
        let mut header = Vec::new();
        tx.write_v5_header(&mut header).unwrap();
        tx.write_transparent(&mut header).unwrap();
        // Empty Sapling counts, Orchard action count, actions, then flags and
        // public value balance. Fixture action effect serialization is 820 B.
        let balance_offset = header.len() + 2 + 1 + 2 * 820 + 1;
        assert_eq!(
            i64::from_le_bytes(
                bytes[balance_offset..balance_offset + 8]
                    .try_into()
                    .unwrap()
            ),
            10_000
        );
        bytes[balance_offset..balance_offset + 8].copy_from_slice(&10_001i64.to_le_bytes());
        let wrong = Transaction::read(Cursor::new(&bytes), branch).unwrap();
        assert_eq!(
            verify_conventional_payout_with_wallet(
                &hex::encode(bytes),
                &wrong.txid().to_string(),
                &expected,
                &wallet
            )
            .err(),
            Some(ConventionalError::Fee)
        );
    }

    #[test]
    fn consensus_size_profile_is_explicit_and_bounded_for_every_recipient_count() {
        assert_eq!(size_profile().identifier(), "consensus-size-v1");
        assert_eq!(profile().identifier(), "restricted-family-v1");
        for network in ["mainnet", "regtest", "test", "Testnet", ""] {
            assert!(TestnetConventionalProfile::consensus_size_bound(network, 100).is_err());
        }
        for n in [0, 101, usize::MAX] {
            assert!(TestnetConventionalProfile::consensus_size_bound("testnet", n).is_err());
            assert!(size_profile().fee_ceiling_zatoshis(n).is_err());
        }
        assert_eq!(MAX_RAW_BYTES / MIN_SHIELDED_ACTION_BYTES, 5681);
        for n in 1..=100 {
            assert_eq!(
                size_profile().fee_ceiling_zatoshis(n),
                Ok(5000 * (n as i64 + 5681))
            );
            // Every lower-cost script remains bounded by the 34-byte P2PKH
            // standard output. The floor's equality case is all Sapling spends.
            for output_bytes in [32, 34] {
                let fee = zip317::FeeRule::standard()
                    .fee_required(
                        &TEST_NETWORK,
                        2_000_000.into(),
                        std::iter::empty::<
                            zcash_primitives::transaction::fees::transparent::InputSize,
                        >(),
                        vec![output_bytes; n],
                        5681,
                        0,
                        0,
                        0,
                    )
                    .unwrap();
                assert!(u64::from(fee) <= size_profile().fee_ceiling_zatoshis(n).unwrap() as u64);
            }
        }
        assert_eq!(size_profile().fee_ceiling_zatoshis(100), Ok(28_905_000));
        let one = TestnetConventionalProfile::consensus_size_bound("testnet", 1).unwrap();
        assert!(one.fee_ceiling_zatoshis(2).is_err());
    }

    #[test]
    fn consensus_size_parser_checks_sapling_and_mixed_public_balances() {
        for (v6, o, i, spends, outputs) in [
            (false, 0, 0, 1, 0),
            (false, 0, 0, 3, 2),
            (true, 0, 0, 1, 3),
            (false, 2, 0, 2, 3),
            (true, 2, 3, 3, 2),
        ] {
            let fee = 5000 * (1 + spends.max(outputs) + o + i).max(2) as i64;
            // In mixed cases Sapling funds the transparent payout AND sends
            // 42 zatoshis into another shielded pool. This tests signed public
            // value-balance summation, not just one positive pool balance.
            let base_fee = if o + i > 0 { -100_042 } else { 0 };
            let (raw, _, height) = raw_fixture(v6, o, i, base_fee, 100_000);
            let expected = ConventionalPayoutExpectation::new(
                &size_profile(),
                &source(),
                &[(recipient(), 100_000)],
                height,
            )
            .unwrap();
            let sapling_balance = 100_000 + fee + if o + i > 0 { 42 } else { 0 };
            let (raw, txid) = add_sapling(&raw, height, spends, outputs, sapling_balance);
            let checked = verify_conventional_payout(&raw, &txid, &expected).unwrap();
            assert_eq!(checked.actual_fee_zatoshis(), fee);
            let restricted = ConventionalPayoutExpectation::new(
                &profile(),
                &source(),
                &[(recipient(), 100_000)],
                height,
            )
            .unwrap();
            assert!(matches!(
                verify_conventional_payout(&raw, &txid, &restricted),
                Err(ConventionalError::Transaction)
            ));
            for incorrect_fee in [fee - 1, fee + 1] {
                let (base, _, _) = raw_fixture(v6, o, i, base_fee, 100_000);
                let balance = 100_000 + incorrect_fee + if o + i > 0 { 42 } else { 0 };
                let (wrong, wrong_txid) = add_sapling(&base, height, spends, outputs, balance);
                assert!(matches!(
                    verify_conventional_payout(&wrong, &wrong_txid, &expected),
                    Err(ConventionalError::Fee)
                ));
            }
        }
    }

    #[test]
    fn consensus_size_parser_enforces_actual_serialized_size_limit() {
        for spends in [5681, 5682] {
            let fee = 5000 * (spends as i64 + 1);
            let (raw, _, height) = raw_fixture(false, 0, 0, 0, 100_000);
            let (raw, txid) = add_sapling(&raw, height, spends, 0, 100_000 + fee);
            let expected = ConventionalPayoutExpectation::new(
                &size_profile(),
                &source(),
                &[(recipient(), 100_000)],
                height,
            )
            .unwrap();
            if spends == 5681 {
                assert!(raw.len() / 2 < MAX_RAW_BYTES);
                assert_eq!(
                    verify_conventional_payout(&raw, &txid, &expected)
                        .unwrap()
                        .actual_fee_zatoshis(),
                    fee
                );
            } else {
                assert!(raw.len() / 2 > MAX_RAW_BYTES);
                assert!(matches!(
                    verify_conventional_payout(&raw, &txid, &expected),
                    Err(ConventionalError::Transaction)
                ));
            }
        }
    }

    #[test]
    fn wider_profile_still_rejects_wrong_header_trailing_and_unsupported_recipients() {
        use zcash_primitives::transaction::{Authorized, TransactionData};
        for v6 in [false, true] {
            let (base, _, height) = raw_fixture(v6, 0, 0, 0, 100_000);
            let (raw, txid) = add_sapling(&base, height, 1, 0, 110_000);
            let expected = ConventionalPayoutExpectation::new(
                &size_profile(),
                &source(),
                &[(recipient(), 100_000)],
                height,
            )
            .unwrap();
            // Expiry endpoints are inclusive, but a subsequent recovery tip
            // must not expand this stored attempt's fixed expiry window.
            for expiry in [height - 1, height, height + 100, height + 101] {
                let mut bytes = hex::decode(&raw).unwrap();
                bytes[16..20].copy_from_slice(&expiry.to_le_bytes());
                let tx = Transaction::read(Cursor::new(&bytes), expected.branch).unwrap();
                let result = verify_conventional_payout(
                    &hex::encode(bytes),
                    &tx.txid().to_string(),
                    &expected,
                );
                assert_eq!(result.is_ok(), (height..=height + 100).contains(&expiry));
            }
            let mut bytes = hex::decode(&raw).unwrap();
            bytes[12..16].copy_from_slice(&1u32.to_le_bytes());
            let tx = Transaction::read(Cursor::new(&bytes), expected.branch).unwrap();
            assert!(matches!(
                verify_conventional_payout(&hex::encode(bytes), &tx.txid().to_string(), &expected),
                Err(ConventionalError::Transaction)
            ));
            assert!(matches!(
                verify_conventional_payout(&format!("{raw}00"), &txid, &expected),
                Err(ConventionalError::Transaction)
            ));
            assert!(matches!(
                verify_conventional_payout(&raw, &"0".repeat(64), &expected),
                Err(ConventionalError::Transaction)
            ));
            if v6 {
                // Same correct NU6.3 branch and expiry, but the wrong version.
                let parsed =
                    Transaction::read(Cursor::new(hex::decode(&raw).unwrap()), expected.branch)
                        .unwrap();
                let wrong = TransactionData::<Authorized>::from_parts(
                    TxVersion::V5,
                    expected.branch,
                    0,
                    (height + 40).into(),
                    parsed.transparent_bundle().cloned(),
                    None,
                    parsed.sapling_bundle().cloned(),
                    None,
                )
                .freeze()
                .unwrap();
                let mut bytes = Vec::new();
                wrong.write(&mut bytes).unwrap();
                assert!(matches!(
                    verify_conventional_payout(
                        &hex::encode(bytes),
                        &wrong.txid().to_string(),
                        &expected
                    ),
                    Err(ConventionalError::Transaction)
                ));
            }
        }
        for bad in [
            ZcashAddress::from_tex(NetworkType::Test, [2; 20]).encode(),
            ZcashAddress::from_transparent_p2pkh(NetworkType::Main, [2; 20]).encode(),
        ] {
            assert!(ConventionalPayoutExpectation::new(
                &size_profile(),
                &source(),
                &[(bad, 1)],
                2_000_000
            )
            .is_err());
        }
        assert!(ConventionalPayoutExpectation::new(
            &size_profile(),
            &recipient(),
            &[(recipient(), 1)],
            2_000_000
        )
        .is_err());
    }
    #[test]
    fn upstream_raw_parser_binds_both_bundle_fees_exact_outputs_and_txid() {
        for (v6, o, i, fee) in [
            (false, 2, 0, 15_000),
            (true, 0, 2, 15_000),
            (true, 2, 3, 30_000),
        ] {
            let (raw, txid, height) = raw_fixture(v6, o, i, fee, 100_000);
            let e = ConventionalPayoutExpectation::new(
                &profile(),
                &source(),
                &[(recipient(), 100_000)],
                height,
            )
            .unwrap();
            let result = verify_conventional_payout(&raw, &txid, &e).unwrap();
            assert_eq!(result.txid(), txid);
            assert_eq!(result.actual_fee_zatoshis(), fee);
            assert_eq!(result.conventional_fee_zatoshis(), fee);
            assert!(matches!(
                verify_conventional_payout(&format!("{raw}00"), &txid, &e),
                Err(ConventionalError::Transaction)
            ));
            assert!(matches!(
                verify_conventional_payout(&raw, &"0".repeat(64), &e),
                Err(ConventionalError::Transaction)
            ));
            let wrong = ConventionalPayoutExpectation::new(
                &profile(),
                &source(),
                &[(recipient(), 99_999)],
                height,
            )
            .unwrap();
            assert!(matches!(
                verify_conventional_payout(&raw, &txid, &wrong),
                Err(ConventionalError::Recipient)
            ));
        }
        for fee in [14_999, 15_001] {
            let (raw, txid, _) = raw_fixture(false, 2, 0, fee, 100_000);
            assert!(matches!(
                verify_conventional_payout(&raw, &txid, &expected()),
                Err(ConventionalError::Fee)
            ));
        }
        let (raw, txid, _) = raw_fixture(false, 110, 0, 555_000, 100_000);
        assert!(matches!(
            verify_conventional_payout(&raw, &txid, &expected()),
            Err(ConventionalError::Fee)
        ));
        let (raw, txid, _) = raw_fixture(false, 2, 0, 15_000, 100_000);
        let wrong_branch = ConventionalPayoutExpectation::new(
            &profile(),
            &source(),
            &[(recipient(), 100_000)],
            4_100_000,
        )
        .unwrap();
        assert!(matches!(
            verify_conventional_payout(&raw, &txid, &wrong_branch),
            Err(ConventionalError::Transaction)
        ));
    }
    #[test]
    fn transparent_inputs_duplicate_outputs_and_extra_change_are_rejected() {
        use zcash_primitives::transaction::Authorized;
        let (raw, _, _) = raw_fixture(false, 2, 0, 15_000, 100_000);
        for alteration in 0..3 {
            let bytes = hex::decode(&raw).unwrap();
            let tx = Transaction::read(Cursor::new(bytes), expected().branch).unwrap();
            let tx = tx
                .into_data()
                .map_bundles::<Authorized>(
                    |bundle| {
                        let mut bundle = bundle.unwrap();
                        match alteration {
                            0 => bundle.vin.push(transparent::bundle::TxIn::from_parts(
                                transparent::bundle::OutPoint::new([3; 32], 0),
                                transparent::address::TransparentAddress::PublicKeyHash([4; 20])
                                    .script()
                                    .into(),
                                u32::MAX,
                            )),
                            1 => bundle.vout.push(bundle.vout[0].clone()),
                            _ => bundle.vout.push(transparent::bundle::TxOut::new(
                                zcash_protocol::value::Zatoshis::const_from_u64(1),
                                transparent::address::TransparentAddress::PublicKeyHash([5; 20])
                                    .script()
                                    .into(),
                            )),
                        }
                        Some(bundle)
                    },
                    |bundle| bundle,
                    |bundle| bundle,
                )
                .freeze()
                .unwrap();
            let mut bytes = Vec::new();
            tx.write(&mut bytes).unwrap();
            for profile in [profile(), size_profile()] {
                let e = ConventionalPayoutExpectation::new(
                    &profile,
                    &source(),
                    &[(recipient(), 100_000)],
                    2_000_000,
                )
                .unwrap();
                let result =
                    verify_conventional_payout(&hex::encode(&bytes), &tx.txid().to_string(), &e);
                assert_eq!(
                    result.err(),
                    Some(if alteration == 0 {
                        ConventionalError::Transaction
                    } else {
                        ConventionalError::Recipient
                    })
                );
            }
        }
    }
    #[test]
    fn profiles_are_testnet_only_fixed_bounded_cached_family_path() {
        assert!(TestnetConventionalProfile::validate("mainnet", 50, true, false, 100).is_err());
        assert!(TestnetConventionalProfile::validate("testnet", 50, false, false, 100).is_err());
        assert!(TestnetConventionalProfile::validate("testnet", 50, true, true, 100).is_err());
        for limit in [0, 51, usize::MAX] {
            assert!(
                TestnetConventionalProfile::validate("testnet", limit, true, false, 100).is_err()
            );
        }
        for n in [0, 101, usize::MAX] {
            assert!(TestnetConventionalProfile::validate("testnet", 50, true, false, n).is_err());
        }
        assert_eq!(profile().fee_ceiling_zatoshis(100), Ok(1_020_000));
        assert!(profile().fee_ceiling_zatoshis(0).is_err());
        assert!(profile().fee_ceiling_zatoshis(101).is_err());
    }
    #[test]
    fn payout_rejects_unsupported_network_source_recipient_and_amounts() {
        for bad in [recipient(), "ANY_TADDR".into(), format!(" {}", source())] {
            assert!(ConventionalPayoutExpectation::new(
                &profile(),
                &bad,
                &[(recipient(), 1)],
                2_000_000
            )
            .is_err());
        }
        let main = ZcashAddress::from_transparent_p2pkh(NetworkType::Main, [2; 20]).encode();
        let tex = ZcashAddress::from_tex(NetworkType::Test, [2; 20]).encode();
        for bad in [main, tex, source()] {
            assert!(ConventionalPayoutExpectation::new(
                &profile(),
                &source(),
                &[(bad, 1)],
                2_000_000
            )
            .is_err());
        }
        for bad in [-1, 0, MAX_MONEY, i64::MAX] {
            assert!(ConventionalPayoutExpectation::new(
                &profile(),
                &source(),
                &[(recipient(), bad)],
                2_000_000
            )
            .is_err());
        }
        assert!(ConventionalPayoutExpectation::new(
            &profile(),
            &source(),
            &[(recipient(), 1), (recipient(), 2)],
            2_000_000
        )
        .is_err());
        assert!(
            ConventionalPayoutExpectation::new(&profile(), &source(), &[(recipient(), 1)], 1)
                .is_err()
        );
        assert!(ConventionalPayoutExpectation::new(
            &profile(),
            &source(),
            &[(recipient(), 1)],
            u32::MAX
        )
        .is_err());
    }
    #[test]
    fn fee_bound_covers_both_family_bundles_padding_and_standard_outputs() {
        // Exhaustive reduced shape space including disjoint post-NU6.3 Orchard
        // actions and independent two-action padding. No signing/proving needed.
        for a in 1..=8 {
            for os in 0..=a {
                for is in 0..=a - os {
                    for oo in 0..=a {
                        for io in 0..=a - oo {
                            for n in [1, 3, 100] {
                                // Use the upstream transaction builder's exact
                                // post-NU6.3 count/padding rule, not a guessed max.
                                let o = orchard::builder::BundleType::DEFAULT
                                    .num_actions(
                                        orchard::bundle::BundleVersion::orchard_v3()
                                            .default_flags(),
                                        os,
                                        oo,
                                    )
                                    .unwrap();
                                let i = orchard::builder::BundleType::DEFAULT
                                    .num_actions(
                                        orchard::bundle::BundleVersion::ironwood_v3()
                                            .default_flags(),
                                        is,
                                        io,
                                    )
                                    .unwrap();
                                let fee=zip317::FeeRule::standard().fee_required(&TEST_NETWORK,2_000_000.into(),
                    std::iter::empty::<zcash_primitives::transaction::fees::transparent::InputSize>(),
                    vec![34;n],0,0,o,i).unwrap();
                                let p = TestnetConventionalProfile::validate(
                                    "testnet", a, true, false, 100,
                                )
                                .unwrap();
                                assert!(
                                    u64::from(fee) <= p.fee_ceiling_zatoshis(n).unwrap() as u64
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn malformed_raw_and_payload_errors_fail_closed_without_echo() {
        for raw in ["", "00", "private-payload", "0x00"] {
            assert!(matches!(
                verify_conventional_payout(raw, &"0".repeat(64), &expected()),
                Err(ConventionalError::Transaction)
            ));
        }
        assert_eq!(
            ConventionalError::Transaction.to_string(),
            "testnet conventional transaction invalid"
        );
    }
}
