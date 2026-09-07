//! Exact-fee PPS wallet path. No payload-bearing errors, logs, or sendmany fallback.
//! The supported wallet contract is Zallet e497c9a (PCZT 0.9.1).
use crate::ZcashRpcClient;
use base64::{engine::general_purpose::STANDARD, Engine};
use pczt::{
    roles::{
        signer::Signer,
        verifier::{OrchardError, SaplingError, Verifier},
    },
    Pczt,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, io::Cursor};
use zcash_address::{ConversionError, ToAddress, TryFromAddress, ZcashAddress};
use zcash_primitives::transaction::{
    sighash::{signature_hash, SignableInput},
    txid::TxIdDigester,
    Authorization, Authorized, Transaction, TxVersion,
};
use zcash_protocol::consensus::NetworkType;

const MAX_PCZT_BYTES: usize = 4 * 1024 * 1024;
const MAX_TX_BYTES: usize = 2 * 1024 * 1024;
const MAX_RECIPIENTS: usize = 100;
const MAX_ZATOSHIS: i64 = 21_000_000 * 100_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PcztError {
    #[error("PPS wallet capability unavailable")]
    CapabilityUnavailable,
    #[error("PPS wallet contract unsupported")]
    UnsupportedContract,
    #[error("PPS recipient unsupported")]
    UnsupportedRecipient,
    #[error("PPS proposal invalid")]
    InvalidProposal,
    #[error("PPS proposal effects changed")]
    EffectsChanged,
    #[error("PPS fee exceeds authorization")]
    FeeExceeded,
    #[error("PPS wallet operation unavailable")]
    RpcUnavailable,
    #[error("PPS extracted transaction invalid")]
    InvalidTransaction,
}

/// Deliberately opaque: a configuration value cannot attest to wallet support.
#[derive(Clone)]
pub struct VerifiedFeeContract {
    _verified: (),
}

struct RecipientScript(Vec<u8>);
impl TryFromAddress for RecipientScript {
    type Error = ();
    fn try_from_transparent_p2pkh(
        _: NetworkType,
        hash: [u8; 20],
    ) -> Result<Self, ConversionError<()>> {
        let mut script = vec![0x76, 0xa9, 0x14];
        script.extend_from_slice(&hash);
        script.extend_from_slice(&[0x88, 0xac]);
        Ok(Self(script))
    }
    fn try_from_transparent_p2sh(
        _: NetworkType,
        hash: [u8; 20],
    ) -> Result<Self, ConversionError<()>> {
        let mut script = vec![0xa9, 0x14];
        script.extend_from_slice(&hash);
        script.push(0x87);
        Ok(Self(script))
    }
}

fn recipient_script(network: &str, address: &str) -> Result<Vec<u8>, PcztError> {
    let network = match network {
        "mainnet" => NetworkType::Main,
        "testnet" => NetworkType::Test,
        _ => return Err(PcztError::UnsupportedRecipient),
    };
    if address.len() > 128 {
        return Err(PcztError::UnsupportedRecipient);
    }
    let parsed =
        ZcashAddress::try_from_encoded(address).map_err(|_| PcztError::UnsupportedRecipient)?;
    if parsed.encode() != address {
        return Err(PcztError::UnsupportedRecipient);
    }
    parsed
        .convert_if_network::<RecipientScript>(network)
        .map(|v| v.0)
        .map_err(|_| PcztError::UnsupportedRecipient)
}

/// Must also guard PPS share admission, before creating any miner liability.
pub fn validate_pps_recipient(network: &str, address: &str) -> Result<(), PcztError> {
    recipient_script(network, address).map(|_| ())
}

/// Positive and negative synthetic-address checks prove the wallet's actual
/// address decoder network without querying an account, seed, or real address.
pub async fn verify_pps_wallet_network(
    rpc: &ZcashRpcClient,
    network: &str,
) -> Result<(), PcztError> {
    let (wanted, other) = match network {
        "mainnet" => (NetworkType::Main, NetworkType::Test),
        "testnet" => (NetworkType::Test, NetworkType::Main),
        _ => return Err(PcztError::UnsupportedContract),
    };
    let expected = ZcashAddress::from_transparent_p2pkh(wanted, [0; 20]).encode();
    let opposite = ZcashAddress::from_transparent_p2pkh(other, [0; 20]).encode();
    let yes: Value = rpc
        .call_raw("validateaddress", json!([expected]))
        .await
        .map_err(|_| PcztError::CapabilityUnavailable)?;
    let no: Value = rpc
        .call_raw("validateaddress", json!([opposite]))
        .await
        .map_err(|_| PcztError::CapabilityUnavailable)?;
    if yes["isvalid"] != true
        || yes["address"] != expected
        || yes["isscript"] != false
        || yes["scriptPubKey"] != hex::encode(recipient_script(network, &expected)?)
        || no != json!({"isvalid":false})
    {
        return Err(PcztError::UnsupportedContract);
    }
    Ok(())
}

pub async fn verify_pps_wallet_capability(
    rpc: &ZcashRpcClient,
) -> Result<VerifiedFeeContract, PcztError> {
    let doc: Value = rpc
        .call_raw("rpc.discover", json!([]))
        .await
        .map_err(|_| PcztError::CapabilityUnavailable)?;
    validate_contract(&doc)?;
    Ok(VerifiedFeeContract { _verified: () })
}

fn validate_contract(doc: &Value) -> Result<(), PcztError> {
    let invalid = PcztError::UnsupportedContract;
    if doc["openrpc"] != "1.3.2"
        || doc["info"]["title"] != "Zallet"
        || doc["info"]["version"] != "0.1.0-beta.2"
    {
        return Err(invalid);
    }
    let methods = doc["methods"].as_array().ok_or(invalid)?;
    for (name, params, required, result_fields) in [
        (
            "pczt_create",
            &[
                "from",
                "amounts",
                "minconf",
                "privacy_policy",
                "fund_source",
            ][..],
            2,
            &["pczt", "privacy_policy"][..],
        ),
        (
            "pczt_inspect",
            &["pczt"][..],
            1,
            &["wallet_created", "fee_zat", "transparent"][..],
        ),
        (
            "pczt_prove",
            &["pczt"][..],
            1,
            &[
                "pczt",
                "sapling_proven",
                "orchard_proven",
                "ironwood_proven",
            ][..],
        ),
        (
            "pczt_sign",
            &["pczt", "privacy_policy", "strict"][..],
            1,
            &[
                "pczt",
                "unsigned_transparent",
                "unsigned_sapling",
                "unsigned_orchard",
                "unsigned_ironwood",
            ][..],
        ),
        (
            "pczt_extract",
            &["pczt"][..],
            1,
            &["hex", "txid", "stored"][..],
        ),
    ] {
        let matching: Vec<_> = methods.iter().filter(|v| v["name"] == name).collect();
        if matching.len() != 1 {
            return Err(invalid);
        }
        let method = matching[0];
        let actual = method["params"].as_array().ok_or(invalid)?;
        if actual.len() != params.len() || method["deprecated"] == true {
            return Err(invalid);
        }
        for (i, (actual, expected)) in actual.iter().zip(params).enumerate() {
            if actual.get("required").is_some_and(|v| !v.is_boolean()) {
                return Err(invalid);
            }
            if actual["name"] != *expected
                || actual["required"].as_bool().unwrap_or(false) != (i < required)
            {
                return Err(invalid);
            }
            let expected_kind = match *expected {
                "amounts" => "array",
                "minconf" => "integer",
                "strict" => "boolean",
                "fund_source" => "any",
                _ => "string",
            };
            if schema_kind(doc, &actual["schema"]) != Some(expected_kind) {
                return Err(invalid);
            }
        }
        let mut schema = &method["result"]["schema"];
        if let Some(reference) = schema["$ref"].as_str() {
            let key = reference
                .strip_prefix("#/components/schemas/")
                .ok_or(invalid)?;
            if key.is_empty() || key.contains('/') || key.contains('~') {
                return Err(invalid);
            }
            schema = &doc["components"]["schemas"][key];
        }
        let properties = schema["properties"].as_object().ok_or(invalid)?;
        let required = schema["required"].as_array().ok_or(invalid)?;
        if schema["type"] != "object"
            || result_fields
                .iter()
                .any(|f| !properties.contains_key(*f) || !required.iter().any(|v| v == f))
        {
            return Err(invalid);
        }
        for field in result_fields {
            let kind = match *field {
                "wallet_created" | "stored" | "sapling_proven" | "orchard_proven"
                | "ironwood_proven" => "boolean",
                "fee_zat" => "integer",
                "transparent" => "object",
                "unsigned_transparent"
                | "unsigned_sapling"
                | "unsigned_orchard"
                | "unsigned_ironwood" => "array",
                _ => "string",
            };
            if schema_kind(doc, &properties[*field]) != Some(kind) {
                return Err(invalid);
            }
        }
    }
    Ok(())
}

fn schema_kind<'a>(doc: &'a Value, schema: &'a Value) -> Option<&'a str> {
    if schema == &Value::Bool(true) {
        return Some("any");
    }
    let schema = if let Some(reference) = schema["$ref"].as_str() {
        let key = reference.strip_prefix("#/components/schemas/")?;
        if key.is_empty() || key.contains('/') || key.contains('~') {
            return None;
        }
        &doc["components"]["schemas"][key]
    } else {
        schema
    };
    schema["type"].as_str()
}

/// Runtime-private proposal. No Debug/Serialize, raw getters, or payload logging.
pub struct PpsProposal {
    pczt: String,
    effects: [u8; 32],
    fee: i64,
    branch: u32,
    expiry: u32,
    recipients: BTreeMap<Vec<u8>, u64>,
}

impl PpsProposal {
    pub fn proposal_id(&self) -> String {
        hex::encode(self.effects)
    }
    pub fn fee_zatoshis(&self) -> i64 {
        self.fee
    }
}

/// Fully validated fixed bytes; may only be sent to the pre-existing node RPC.
pub struct PpsSignedTransaction {
    hex: String,
    txid: String,
}
impl PpsSignedTransaction {
    pub fn txid(&self) -> &str {
        &self.txid
    }
    /// An error is ambiguous. The caller MUST retain the sealed reservation.
    pub async fn broadcast(&self, node: &ZcashRpcClient) -> Result<(), PcztError> {
        let returned: String = node
            .call_raw("sendrawtransaction", json!([self.hex]))
            .await
            .map_err(|_| PcztError::RpcUnavailable)?;
        if returned != self.txid {
            return Err(PcztError::InvalidTransaction);
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateResult {
    pczt: String,
    privacy_policy: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProveResult {
    pczt: String,
    sapling_proven: bool,
    orchard_proven: bool,
    ironwood_proven: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignResult {
    pczt: String,
    transparent_signed: usize,
    sapling_signed: usize,
    orchard_signed: usize,
    ironwood_signed: usize,
    unsigned_transparent: Vec<usize>,
    unsigned_sapling: Vec<usize>,
    unsigned_orchard: Vec<usize>,
    unsigned_ironwood: Vec<usize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtractResult {
    hex: String,
    txid: String,
    stored: bool,
}

fn parse_pczt(encoded: &str) -> Result<Pczt, PcztError> {
    if encoded.is_empty() || encoded.len() > MAX_PCZT_BYTES * 4 / 3 + 4 {
        return Err(PcztError::InvalidProposal);
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| PcztError::InvalidProposal)?;
    if bytes.len() > MAX_PCZT_BYTES || STANDARD.encode(&bytes) != encoded {
        return Err(PcztError::InvalidProposal);
    }
    Pczt::parse(&bytes).map_err(|_| PcztError::InvalidProposal)
}

fn zec_number(zatoshis: i64) -> Result<Value, PcztError> {
    if !(1..=MAX_ZATOSHIS).contains(&zatoshis) {
        return Err(PcztError::InvalidProposal);
    }
    // No floating point at any amount or fee boundary.
    serde_json::from_str(&format!(
        "{}.{:08}",
        zatoshis / 100_000_000,
        zatoshis % 100_000_000
    ))
    .map_err(|_| PcztError::InvalidProposal)
}

/// Input selection only; no proof, signature, extraction, or broadcast. The
/// caller must reserve this EXACT proposal's fee and recipients, then durably
/// seal the attempt before calling `sign_pps_proposal`.
pub async fn prepare_pps_proposal(
    rpc: &ZcashRpcClient,
    network: &str,
    from: &str,
    amounts: &[(String, i64)],
    max_fee_zatoshis: i64,
    tip_height: u32,
) -> Result<PpsProposal, PcztError> {
    let _contract = verify_pps_wallet_capability(rpc).await?;
    verify_pps_wallet_network(rpc, network).await?;
    if amounts.is_empty()
        || amounts.len() > MAX_RECIPIENTS
        || from.is_empty()
        || from.len() > 2048
        || max_fee_zatoshis <= 0
    {
        return Err(PcztError::InvalidProposal);
    }
    let mut recipients = BTreeMap::new();
    let mut request = Vec::new();
    for (address, amount) in amounts {
        let script = recipient_script(network, address)?;
        if recipients
            .insert(
                script,
                u64::try_from(*amount).map_err(|_| PcztError::InvalidProposal)?,
            )
            .is_some()
        {
            return Err(PcztError::InvalidProposal);
        }
        request.push(json!({"address":address, "amount":zec_number(*amount)?}));
    }
    // An address source is constrained by Zallet's spend_policy_for; account
    // UUID defaults to shielded-only. Transparent inputs are rejected below.
    let created: CreateResult = rpc
        .call_raw(
            "pczt_create",
            json!([from, request, 3, "AllowRevealedRecipients", null]),
        )
        .await
        .map_err(|_| PcztError::RpcUnavailable)?;
    if created.privacy_policy != "AllowRevealedRecipients" {
        return Err(PcztError::InvalidProposal);
    }
    let pczt = parse_pczt(&created.pczt)?;
    let inspect: Value = rpc
        .call_raw("pczt_inspect", json!([created.pczt]))
        .await
        .map_err(|_| PcztError::RpcUnavailable)?;
    let branch = *pczt.global().consensus_branch_id();
    let expiry = *pczt.global().expiry_height();
    let expected_branch = match network {
        "mainnet" => zcash_protocol::consensus::BranchId::for_height(
            &zcash_protocol::consensus::MAIN_NETWORK,
            (tip_height.saturating_add(1)).into(),
        ),
        "testnet" => zcash_protocol::consensus::BranchId::for_height(
            &zcash_protocol::consensus::TEST_NETWORK,
            (tip_height.saturating_add(1)).into(),
        ),
        _ => return Err(PcztError::InvalidProposal),
    };
    if branch != u32::from(expected_branch)
        || expiry <= tip_height
        || expiry > tip_height.saturating_add(100)
    {
        return Err(PcztError::InvalidProposal);
    }
    let fee = verify_proposal(&pczt, &recipients, max_fee_zatoshis)?;
    if inspect["wallet_created"] != true
        || inspect["fee_zat"].as_i64() != Some(fee)
        || inspect["tx_version"].as_u64() != Some(u64::from(*pczt.global().tx_version()))
        || inspect["consensus_branch_id"] != format!("{branch:08x}")
        || inspect["expiry_height"].as_u64() != Some(u64::from(expiry))
    {
        return Err(PcztError::InvalidProposal);
    }
    let effects = Signer::new(pczt)
        .map_err(|_| PcztError::InvalidProposal)?
        .shielded_sighash();
    Ok(PpsProposal {
        pczt: created.pczt,
        effects,
        fee,
        branch,
        expiry,
        recipients,
    })
}

fn verify_proposal(
    pczt: &Pczt,
    recipients: &BTreeMap<Vec<u8>, u64>,
    max_fee: i64,
) -> Result<i64, PcztError> {
    if !matches!(*pczt.global().tx_version(), 5 | 6)
        || !pczt.transparent().inputs().is_empty()
        || pczt.transparent().outputs().len() != recipients.len()
    {
        return Err(PcztError::InvalidProposal);
    }
    let mut actual = BTreeMap::new();
    let mut outgoing = 0i128;
    for output in pczt.transparent().outputs() {
        if actual
            .insert(output.script_pubkey().clone(), *output.value())
            .is_some()
        {
            return Err(PcztError::InvalidProposal);
        }
        outgoing = outgoing
            .checked_add(i128::from(*output.value()))
            .ok_or(PcztError::InvalidProposal)?;
    }
    if &actual != recipients {
        return Err(PcztError::InvalidProposal);
    }
    // Validate value commitments and ownership of EVERY nonzero shielded
    // output as change to a key that owns an input. No imported key/query is
    // needed: the input's PCZT viewing/proving data stays runtime-private.
    let mut net = 0i128;
    let mut funded = false;
    let verifier = Verifier::new(pczt.clone())
        .with_sapling::<(), _>(|bundle| {
            let mut views = Vec::new();
            let mut balance = 0i128;
            for spend in bundle.spends() {
                spend.verify_cv()?;
                spend.verify_nullifier(None)?;
                spend.verify_rk(None)?;
                let value = spend
                    .value()
                    .as_ref()
                    .ok_or(SaplingError::Custom(()))?
                    .inner();
                balance += i128::from(value);
                if value > 0 {
                    funded = true;
                    views.push(
                        spend
                            .proof_generation_key()
                            .as_ref()
                            .ok_or(SaplingError::Custom(()))?
                            .to_viewing_key(),
                    );
                }
            }
            for output in bundle.outputs() {
                output.verify_cv()?;
                output.verify_note_commitment()?;
                let value = output
                    .value()
                    .as_ref()
                    .ok_or(SaplingError::Custom(()))?
                    .inner();
                balance -= i128::from(value);
                if value > 0 {
                    let recipient = output
                        .recipient()
                        .as_ref()
                        .ok_or(SaplingError::Custom(()))?;
                    if !views.iter().any(|vk| {
                        vk.to_payment_address(*recipient.diversifier()).as_ref() == Some(recipient)
                    }) {
                        return Err(SaplingError::Custom(()));
                    }
                }
            }
            if balance != bundle.value_sum().to_raw() {
                return Err(SaplingError::Custom(()));
            }
            net += balance;
            Ok(())
        })
        .map_err(|_| PcztError::InvalidProposal)?;
    // Orchard and Ironwood use the same account key. Default wallet change may
    // migrate Orchard inputs to Ironwood, so ownership spans these two pools.
    let mut views = Vec::new();
    let mut collect_views = |bundle: &orchard::pczt::Bundle| -> Result<(), OrchardError<()>> {
        for action in bundle.actions() {
            let value = action
                .spend()
                .value()
                .as_ref()
                .ok_or(OrchardError::Custom(()))?
                .inner();
            if value > 0 {
                views.push(
                    action
                        .spend()
                        .fvk()
                        .as_ref()
                        .ok_or(OrchardError::Custom(()))?
                        .clone(),
                );
            }
        }
        Ok(())
    };
    Verifier::new(pczt.clone())
        .with_orchard(&mut collect_views)
        .map_err(|_| PcztError::InvalidProposal)?
        .with_ironwood(&mut collect_views)
        .map_err(|_| PcztError::InvalidProposal)?;
    let mut verify_orchard = |bundle: &orchard::pczt::Bundle| -> Result<(), OrchardError<()>> {
        let mut balance = 0i128;
        bundle.verify_cross_address_restriction()?;
        for action in bundle.actions() {
            action.verify_cv_net()?;
            action.spend().verify_nullifier(None)?;
            action.spend().verify_rk(None)?;
            action.output().verify_note_commitment(action.spend())?;
            let value = action
                .spend()
                .value()
                .as_ref()
                .ok_or(OrchardError::Custom(()))?
                .inner();
            balance += i128::from(value);
            if value > 0 {
                funded = true;
            }
        }
        for action in bundle.actions() {
            let output = action.output();
            let value = output
                .value()
                .as_ref()
                .ok_or(OrchardError::Custom(()))?
                .inner();
            balance -= i128::from(value);
            if value > 0 {
                let recipient = output
                    .recipient()
                    .as_ref()
                    .ok_or(OrchardError::Custom(()))?;
                if !views
                    .iter()
                    .any(|vk| vk.scope_for_address(recipient).is_some())
                {
                    return Err(OrchardError::Custom(()));
                }
            }
        }
        let declared =
            i128::from(i64::try_from(*bundle.value_sum()).map_err(|_| OrchardError::Custom(()))?);
        if balance != declared {
            return Err(OrchardError::Custom(()));
        }
        net += balance;
        Ok(())
    };
    verifier
        .with_orchard(&mut verify_orchard)
        .map_err(|_| PcztError::InvalidProposal)?
        .with_ironwood(&mut verify_orchard)
        .map_err(|_| PcztError::InvalidProposal)?;
    let fee = i64::try_from(net - outgoing).map_err(|_| PcztError::InvalidProposal)?;
    if !funded || fee <= 0 {
        return Err(PcztError::InvalidProposal);
    }
    if fee > max_fee {
        return Err(PcztError::FeeExceeded);
    }
    Ok(fee)
}

/// Proof creation has no signing/broadcast authority. Complete it before the
/// final funding check and durable no-refund fence.
pub async fn prove_pps_proposal(
    rpc: &ZcashRpcClient,
    mut proposal: PpsProposal,
) -> Result<PpsProposal, PcztError> {
    let proven: ProveResult = rpc
        .call_raw("pczt_prove", json!([proposal.pczt]))
        .await
        .map_err(|_| PcztError::RpcUnavailable)?;
    let _ = (
        proven.sapling_proven,
        proven.orchard_proven,
        proven.ironwood_proven,
    );
    verify_same_effects(&proven.pczt, &proposal)?;
    proposal.pczt = proven.pczt;
    Ok(proposal)
}

/// Caller must have durably sealed the exact proposal before this operation.
/// ANY error after entry must park its reservation: no automatic refund/replan.
pub async fn sign_pps_proposal(
    rpc: &ZcashRpcClient,
    proposal: PpsProposal,
) -> Result<PpsSignedTransaction, PcztError> {
    let signed: SignResult = rpc
        .call_raw(
            "pczt_sign",
            json!([proposal.pczt, "AllowRevealedRecipients", true]),
        )
        .await
        .map_err(|_| PcztError::RpcUnavailable)?;
    if signed.transparent_signed != 0
        || !signed.unsigned_transparent.is_empty()
        || !signed.unsigned_sapling.is_empty()
        || !signed.unsigned_orchard.is_empty()
        || !signed.unsigned_ironwood.is_empty()
        || (signed.sapling_signed == 0 && signed.orchard_signed == 0 && signed.ironwood_signed == 0)
    {
        return Err(PcztError::InvalidProposal);
    }
    verify_same_effects(&signed.pczt, &proposal)?;
    let extracted: ExtractResult = rpc
        .call_raw("pczt_extract", json!([signed.pczt]))
        .await
        .map_err(|_| PcztError::RpcUnavailable)?;
    verify_extracted(extracted, &proposal)
}

fn verify_same_effects(encoded: &str, proposal: &PpsProposal) -> Result<(), PcztError> {
    let pczt = parse_pczt(encoded)?;
    let effects = Signer::new(pczt)
        .map_err(|_| PcztError::InvalidProposal)?
        .shielded_sighash();
    if effects != proposal.effects {
        return Err(PcztError::EffectsChanged);
    }
    Ok(())
}

// Upstream signature hashing requires transparent prevout context. This exact
// empty context is valid ONLY because transparent inputs were rejected twice.
#[derive(Debug)]
struct NoInputs;
impl transparent::bundle::Authorization for NoInputs {
    type ScriptSig = ();
}
impl transparent::sighash::TransparentAuthorizingContext for NoInputs {
    fn input_amounts(&self) -> Vec<zcash_protocol::value::Zatoshis> {
        Vec::new()
    }
    fn input_scriptpubkeys(&self) -> Vec<transparent::address::Script> {
        Vec::new()
    }
}
struct NoInputsAuth;
impl Authorization for NoInputsAuth {
    type TransparentAuth = NoInputs;
    type SaplingAuth = <Authorized as Authorization>::SaplingAuth;
    type OrchardAuth = <Authorized as Authorization>::OrchardAuth;
}
impl transparent::bundle::MapAuth<transparent::bundle::Authorized, NoInputs> for NoInputs {
    fn map_script_sig(&self, _: transparent::address::Script) {}
    fn map_authorization(&self, _: transparent::bundle::Authorized) -> NoInputs {
        NoInputs
    }
}

fn verify_extracted(
    extracted: ExtractResult,
    proposal: &PpsProposal,
) -> Result<PpsSignedTransaction, PcztError> {
    if !extracted.stored
        || extracted.hex.len() > MAX_TX_BYTES * 2
        || extracted.hex.is_empty()
        || extracted.txid.len() != 64
    {
        return Err(PcztError::InvalidTransaction);
    }
    let raw = hex::decode(&extracted.hex).map_err(|_| PcztError::InvalidTransaction)?;
    if hex::encode(&raw) != extracted.hex {
        return Err(PcztError::InvalidTransaction);
    }
    let branch = zcash_protocol::consensus::BranchId::try_from(proposal.branch)
        .map_err(|_| PcztError::InvalidTransaction)?;
    let mut reader = Cursor::new(&raw);
    let tx = Transaction::read(&mut reader, branch).map_err(|_| PcztError::InvalidTransaction)?;
    if reader.position() != raw.len() as u64
        || !matches!(tx.version(), TxVersion::V5 | TxVersion::V6)
        || tx.consensus_branch_id() != branch
        || u32::from(tx.expiry_height()) != proposal.expiry
        || tx.txid().to_string() != extracted.txid
        || tx.sprout_bundle().is_some()
        || tx.transparent_bundle().is_some_and(|b| !b.vin.is_empty())
    {
        return Err(PcztError::InvalidTransaction);
    }
    let mut canonical = Vec::new();
    tx.write(&mut canonical)
        .map_err(|_| PcztError::InvalidTransaction)?;
    if canonical != raw {
        return Err(PcztError::InvalidTransaction);
    }
    let mut recipients = BTreeMap::new();
    if let Some(bundle) = tx.transparent_bundle() {
        for output in &bundle.vout {
            if recipients
                .insert(
                    output.script_pubkey().0 .0.clone(),
                    u64::from(output.value()),
                )
                .is_some()
            {
                return Err(PcztError::InvalidTransaction);
            }
        }
    }
    if recipients != proposal.recipients {
        return Err(PcztError::InvalidTransaction);
    }
    let fee = tx
        .fee_paid::<zcash_protocol::value::BalanceError, _>(|_| Ok(None))
        .map_err(|_| PcztError::InvalidTransaction)?
        .ok_or(PcztError::InvalidTransaction)?;
    if u64::from(fee) != proposal.fee as u64 {
        return Err(PcztError::FeeExceeded);
    }
    let data = tx
        .into_data()
        .map_authorization::<NoInputsAuth>(NoInputs, (), ());
    let effects = signature_hash(&data, &SignableInput::Shielded, &data.digest(TxIdDigester));
    if effects.as_ref() != &proposal.effects {
        return Err(PcztError::EffectsChanged);
    }
    // Exact bytes are retained privately, not a second input-selection request.
    Ok(PpsSignedTransaction {
        hex: extracted.hex,
        txid: extracted.txid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pczt::roles::{creator::Creator, io_finalizer::IoFinalizer};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use zcash_primitives::transaction::{
        builder::{BuildConfig, Builder, BundlePadding},
        fees::zip317,
    };
    use zcash_protocol::{
        consensus::{BranchId, TEST_NETWORK},
        memo::MemoBytes,
        value::Zatoshis,
    };

    async fn mock_rpc(
        results: Vec<Value>,
    ) -> (ZcashRpcClient, tokio::task::JoinHandle<Vec<Value>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for result in results {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                let request = loop {
                    let n = stream.read(&mut buffer).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    assert!(bytes.len() < MAX_PCZT_BYTES * 2);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = std::str::from_utf8(&bytes[..end]).unwrap();
                        let length: usize = headers
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse().unwrap())
                            })
                            .unwrap();
                        if bytes.len() >= end + 4 + length {
                            break serde_json::from_slice::<Value>(
                                &bytes[end + 4..end + 4 + length],
                            )
                            .unwrap();
                        }
                    }
                };
                let body = json!({"id":request["id"],"result":result,"error":null}).to_string();
                requests.push(request);
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
            }
            requests
        });
        let http = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap();
        (
            ZcashRpcClient::with_transport(&format!("http://{address}"), None, http),
            task,
        )
    }

    fn network_answers() -> Vec<Value> {
        let address = ZcashAddress::from_transparent_p2pkh(NetworkType::Test, [0; 20]).encode();
        vec![
            json!({"isvalid":true,"isscript":false,"address":address,
            "scriptPubKey":hex::encode(recipient_script("testnet",&address).unwrap())}),
            json!({"isvalid":false}),
        ]
    }

    #[test]
    fn recipients_are_canonical_network_matched_non_tex_transparent_only() {
        let main = ZcashAddress::from_transparent_p2pkh(NetworkType::Main, [1; 20]).encode();
        let test = ZcashAddress::from_transparent_p2sh(NetworkType::Test, [2; 20]).encode();
        assert!(validate_pps_recipient("mainnet", &main).is_ok());
        assert!(validate_pps_recipient("testnet", &test).is_ok());
        assert!(validate_pps_recipient("testnet", &main).is_err());
        assert!(validate_pps_recipient("mainnet", &test).is_err());
        for bad in ["", "ANY_TADDR", "not-an-address", &format!(" {main}")] {
            assert!(validate_pps_recipient("mainnet", bad).is_err());
        }
        let tex = ZcashAddress::from_tex(NetworkType::Main, [1; 20]).encode();
        assert!(validate_pps_recipient("mainnet", &tex).is_err());
        assert!(validate_pps_recipient("regtest", &test).is_err());
    }

    #[test]
    fn amounts_never_round_via_float_and_payload_errors_are_fixed() {
        assert_eq!(zec_number(1).unwrap().to_string(), "0.00000001");
        assert_eq!(zec_number(100_000_001).unwrap().to_string(), "1.00000001");
        for bad in [0, -1, i64::MAX] {
            assert!(zec_number(bad).is_err());
        }
        for bad in ["", "private-wallet-payload", "c2VjcmV0", "===="] {
            assert!(matches!(parse_pczt(bad), Err(PcztError::InvalidProposal)));
        }
        assert_eq!(
            PcztError::RpcUnavailable.to_string(),
            "PPS wallet operation unavailable"
        );
    }

    fn contract() -> Value {
        let specs = [
            (
                "pczt_create",
                vec![
                    "from",
                    "amounts",
                    "minconf",
                    "privacy_policy",
                    "fund_source",
                ],
                2,
                vec!["pczt", "privacy_policy"],
            ),
            (
                "pczt_inspect",
                vec!["pczt"],
                1,
                vec!["wallet_created", "fee_zat", "transparent"],
            ),
            (
                "pczt_prove",
                vec!["pczt"],
                1,
                vec![
                    "pczt",
                    "sapling_proven",
                    "orchard_proven",
                    "ironwood_proven",
                ],
            ),
            (
                "pczt_sign",
                vec!["pczt", "privacy_policy", "strict"],
                1,
                vec![
                    "pczt",
                    "unsigned_transparent",
                    "unsigned_sapling",
                    "unsigned_orchard",
                    "unsigned_ironwood",
                ],
            ),
            (
                "pczt_extract",
                vec!["pczt"],
                1,
                vec!["hex", "txid", "stored"],
            ),
        ];
        let methods:Vec<_> = specs.into_iter().map(|(name,params,required,fields)|{
            let props:serde_json::Map<_,_>=fields.iter().map(|f| {
                let kind=match *f {"wallet_created"|"stored"|"sapling_proven"|"orchard_proven"|"ironwood_proven"=>"boolean",
                    "fee_zat"=>"integer","transparent"=>"object",
                    "unsigned_transparent"|"unsigned_sapling"|"unsigned_orchard"|"unsigned_ironwood"=>"array",_=>"string"};
                (f.to_string(),json!({"type":kind}))
            }).collect();
            json!({"name":name,"params":params.iter().enumerate().map(|(i,p)|{
                let schema=match *p {"amounts"=>json!({"type":"array"}),"minconf"=>json!({"type":"integer"}),
                    "strict"=>json!({"type":"boolean"}),"fund_source"=>json!(true),_=>json!({"type":"string"})};
                json!({"name":p,"required":i<required,"schema":schema})}).collect::<Vec<_>>(),
                "result":{"schema":{"type":"object","properties":props,"required":fields}}})
        }).collect();
        json!({"openrpc":"1.3.2","info":{"title":"Zallet","version":"0.1.0-beta.2"},"methods":methods})
    }

    #[test]
    fn discovery_requires_every_exact_method_and_mandatory_result_field() {
        let good = contract();
        assert!(validate_contract(&good).is_ok());
        let mut bad = good.clone();
        bad["info"]["title"] = json!("zecd");
        assert!(validate_contract(&bad).is_err());
        let mut bad = good.clone();
        bad["methods"].as_array_mut().unwrap().pop();
        assert!(validate_contract(&bad).is_err());
        let mut bad = good.clone();
        bad["methods"]
            .as_array_mut()
            .unwrap()
            .push(good["methods"][0].clone());
        assert!(validate_contract(&bad).is_err());
        let mut bad = good.clone();
        bad["methods"][3]["params"][2]["name"] = json!("unsafe");
        assert!(validate_contract(&bad).is_err());
        let mut bad = good;
        bad["methods"][4]["result"]["schema"]["required"] = json!(["hex", "txid"]);
        assert!(validate_contract(&bad).is_err());
    }

    /// Synthetic note/key/tree, no real wallet. Adapted from pczt 0.9.1's public
    /// end_to_end builder test, using its REAL commitment and signer parsers.
    fn proposal_fixture(change_to_other_key: bool) -> (Pczt, BTreeMap<Vec<u8>, u64>) {
        proposal_fixture_version(change_to_other_key, false)
    }
    fn proposal_fixture_version(
        change_to_other_key: bool,
        v6: bool,
    ) -> (Pczt, BTreeMap<Vec<u8>, u64>) {
        use zcash_protocol::consensus::{NetworkUpgrade, Parameters};
        let key = orchard::keys::SpendingKey::from_bytes([7; 32]).unwrap();
        let fvk = orchard::keys::FullViewingKey::from(&key);
        let recipient = fvk.address_at(0u32, orchard::keys::Scope::External);
        let rho = orchard::note::Rho::from_bytes(&[0; 32]).unwrap();
        let rseed = orchard::note::RandomSeed::from_bytes([9; 32], &rho).unwrap();
        let note = orchard::Note::from_parts(
            recipient,
            orchard::value::NoteValue::from_raw(1_000_000),
            rho,
            rseed,
            if v6 {
                orchard::note::NoteVersion::V3
            } else {
                orchard::note::NoteVersion::V2
            },
        )
        .unwrap();
        let sibling = orchard::tree::MerkleHashOrchard::from_bytes(&[0; 32]).unwrap();
        let path = orchard::tree::MerklePath::from_parts(0, [sibling; 32]);
        let anchor = path.root(note.commitment().into());
        let height = if v6 {
            u32::from(
                TEST_NETWORK
                    .activation_height(NetworkUpgrade::Nu6_3)
                    .unwrap(),
            ) + 100
        } else {
            2_000_000
        };
        let mut builder = Builder::new(
            TEST_NETWORK,
            height.into(),
            BuildConfig::Standard {
                sapling_anchor: None,
                orchard_anchor: (!v6).then_some(anchor),
                ironwood_anchor: v6.then_some(anchor),
                orchard_padding: BundlePadding::DEFAULT,
                ironwood_padding: BundlePadding::DEFAULT,
            },
        );
        if v6 {
            builder
                .add_ironwood_spend::<zip317::FeeRule>(fvk.clone(), note, path)
                .unwrap();
        } else {
            builder
                .add_orchard_spend::<zip317::FeeRule>(fvk.clone(), note, path)
                .unwrap();
        }
        let change = if change_to_other_key {
            orchard::keys::FullViewingKey::from(
                &orchard::keys::SpendingKey::from_bytes([8; 32]).unwrap(),
            )
            .address_at(0u32, orchard::keys::Scope::Internal)
        } else {
            fvk.address_at(0u32, orchard::keys::Scope::Internal)
        };
        if v6 {
            builder
                .add_ironwood_output::<zip317::FeeRule>(
                    Some(fvk.to_ovk(orchard::keys::Scope::Internal)),
                    change,
                    Zatoshis::const_from_u64(885_000),
                    MemoBytes::empty(),
                )
                .unwrap();
        } else {
            builder
                .add_orchard_output::<zip317::FeeRule>(
                    Some(fvk.to_ovk(orchard::keys::Scope::Internal)),
                    change,
                    Zatoshis::const_from_u64(885_000),
                    MemoBytes::empty(),
                )
                .unwrap();
        }
        let address = transparent::address::TransparentAddress::PublicKeyHash([1; 20]);
        builder
            .add_transparent_output(&address, Zatoshis::const_from_u64(100_000))
            .unwrap();
        let parts = builder
            .build_for_pczt(rand::rngs::OsRng, &zip317::FeeRule::standard())
            .unwrap()
            .pczt_parts;
        let pczt = IoFinalizer::new(Creator::build_from_parts(parts).unwrap())
            .finalize_io()
            .unwrap();
        let encoded = ZcashAddress::from_transparent_p2pkh(NetworkType::Test, [1; 20]).encode();
        (
            pczt,
            BTreeMap::from([(recipient_script("testnet", &encoded).unwrap(), 100_000)]),
        )
    }

    #[test]
    fn real_pczt_parser_binds_exact_recipient_change_and_fee() {
        let (pczt, recipients) = proposal_fixture(false);
        assert_eq!(verify_proposal(&pczt, &recipients, 15_000), Ok(15_000));
        assert_eq!(
            verify_proposal(&pczt, &recipients, 14_999),
            Err(PcztError::FeeExceeded)
        );
        let mut wrong = recipients.clone();
        *wrong.values_mut().next().unwrap() += 1;
        assert_eq!(
            verify_proposal(&pczt, &wrong, 15_000),
            Err(PcztError::InvalidProposal)
        );
        let (diverted, recipients) = proposal_fixture(true);
        assert_eq!(
            verify_proposal(&diverted, &recipients, 15_000),
            Err(PcztError::InvalidProposal)
        );
        let (v6, recipients) = proposal_fixture_version(false, true);
        assert_eq!(*v6.global().tx_version(), 6);
        assert_eq!(verify_proposal(&v6, &recipients, 15_000), Ok(15_000));
        let (diverted, recipients) = proposal_fixture_version(true, true);
        assert_eq!(
            verify_proposal(&diverted, &recipients, 15_000),
            Err(PcztError::InvalidProposal)
        );
    }

    #[test]
    fn same_effect_check_rejects_changed_expiry_and_missing_fields() {
        let (pczt, recipients) = proposal_fixture(false);
        let effects = Signer::new(pczt.clone()).unwrap().shielded_sighash();
        let encoded = STANDARD.encode(pczt.clone().serialize().unwrap());
        let proposal = PpsProposal {
            pczt: encoded.clone(),
            effects,
            fee: 15_000,
            branch: *pczt.global().consensus_branch_id(),
            expiry: *pczt.global().expiry_height(),
            recipients,
        };
        assert!(verify_same_effects(&encoded, &proposal).is_ok());
        let empty = Creator::new(BranchId::Nu5.into(), 123, 1, Some([0; 32]), Some([0; 32]))
            .unwrap()
            .build()
            .unwrap();
        let changed = STANDARD.encode(empty.serialize().unwrap());
        assert_eq!(
            verify_same_effects(&changed, &proposal),
            Err(PcztError::EffectsChanged)
        );
        assert_eq!(
            verify_same_effects("garbage", &proposal),
            Err(PcztError::InvalidProposal)
        );
        assert!(matches!(
            verify_extracted(
                ExtractResult {
                    stored: false,
                    hex: "00".into(),
                    txid: "0".repeat(64)
                },
                &proposal
            ),
            Err(PcztError::InvalidTransaction)
        ));
    }

    #[tokio::test]
    async fn prepare_uses_exact_integer_amounts_and_never_signs_or_sends() {
        let (pczt, _) = proposal_fixture(false);
        let encoded = STANDARD.encode(pczt.clone().serialize().unwrap());
        let mut replies = vec![contract()];
        replies.extend(network_answers());
        replies.push(json!({"pczt":encoded,"privacy_policy":"AllowRevealedRecipients"}));
        replies.push(
            json!({"wallet_created":true,"fee_zat":15000,"tx_version":*pczt.global().tx_version(),
            "consensus_branch_id":format!("{:08x}",pczt.global().consensus_branch_id()),
            "expiry_height":*pczt.global().expiry_height()}),
        );
        let (rpc, requests) = mock_rpc(replies).await;
        let address = ZcashAddress::from_transparent_p2pkh(NetworkType::Test, [1; 20]).encode();
        let proposal = prepare_pps_proposal(
            &rpc,
            "testnet",
            "synthetic-source",
            &[(address, 100_000)],
            15_000,
            1_999_999,
        )
        .await
        .unwrap();
        assert_eq!(proposal.fee_zatoshis(), 15000);
        let calls = requests.await.unwrap();
        assert_eq!(
            calls
                .iter()
                .map(|v| v["method"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "rpc.discover",
                "validateaddress",
                "validateaddress",
                "pczt_create",
                "pczt_inspect"
            ]
        );
        assert_eq!(calls[3]["params"][1][0]["amount"].to_string(), "0.00100000");
        assert_eq!(calls[3]["params"][3], "AllowRevealedRecipients");
    }

    #[tokio::test]
    async fn network_proof_rejects_wrong_or_incomplete_decoder_and_unsupported_capability() {
        let (rpc, calls) = mock_rpc(vec![json!({"isvalid":true}), json!({"isvalid":false})]).await;
        assert_eq!(
            verify_pps_wallet_network(&rpc, "testnet").await,
            Err(PcztError::UnsupportedContract)
        );
        assert_eq!(calls.await.unwrap().len(), 2);
        let (rpc, calls) = mock_rpc(vec![json!({"info":{"title":"zecd"}})]).await;
        assert!(matches!(
            verify_pps_wallet_capability(&rpc).await,
            Err(PcztError::UnsupportedContract)
        ));
        assert_eq!(calls.await.unwrap().len(), 1);
    }

    #[test]
    #[ignore = "explicit CPU-bound synthetic proof/sign/extract rehearsal; no wallet or network"]
    fn real_ironwood_prove_sign_extract_and_exact_byte_binding() {
        use pczt::roles::{prover::Prover, tx_extractor::TransactionExtractor};
        let (pczt, recipients) = proposal_fixture_version(false, true);
        assert_eq!(verify_proposal(&pczt, &recipients, 15_000), Ok(15_000));
        let proposal = PpsProposal {
            pczt: STANDARD.encode(pczt.clone().serialize().unwrap()),
            effects: Signer::new(pczt.clone()).unwrap().shielded_sighash(),
            fee: 15_000,
            branch: *pczt.global().consensus_branch_id(),
            expiry: *pczt.global().expiry_height(),
            recipients,
        };
        let pk =
            orchard::circuit::ProvingKey::build(orchard::circuit::OrchardCircuitVersion::PostNu6_3);
        let proven = Prover::new(pczt)
            .create_ironwood_proof(&pk)
            .unwrap()
            .finish();
        let indexes: Vec<_> = proven
            .ironwood()
            .actions()
            .iter()
            .enumerate()
            .filter_map(|(i, a)| a.spend().spend_auth_sig().is_none().then_some(i))
            .collect();
        let mut signer = Signer::new(proven).unwrap();
        let ask = orchard::keys::SpendAuthorizingKey::from(
            &orchard::keys::SpendingKey::from_bytes([7; 32]).unwrap(),
        );
        for index in indexes {
            signer.sign_ironwood(index, &ask).unwrap();
        }
        let tx = TransactionExtractor::new(signer.finish())
            .extract()
            .unwrap();
        let mut raw = Vec::new();
        tx.write(&mut raw).unwrap();
        let txid = tx.txid().to_string();
        let hex = hex::encode(&raw);
        assert_eq!(
            verify_extracted(
                ExtractResult {
                    stored: true,
                    hex: hex.clone(),
                    txid: txid.clone()
                },
                &proposal
            )
            .unwrap()
            .txid(),
            txid
        );
        assert!(matches!(
            verify_extracted(
                ExtractResult {
                    stored: true,
                    hex: format!("{hex}00"),
                    txid: txid.clone()
                },
                &proposal
            ),
            Err(PcztError::InvalidTransaction)
        ));
        assert!(matches!(
            verify_extracted(
                ExtractResult {
                    stored: true,
                    hex,
                    txid: "0".repeat(64)
                },
                &proposal
            ),
            Err(PcztError::InvalidTransaction)
        ));
    }
}
