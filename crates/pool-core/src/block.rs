use std::sync::Arc;

use node_rpc::ZcashRpcClient;
use tracing::{error, info, warn};

pub struct BlockAssembler {
    rpc: Arc<ZcashRpcClient>,
}

impl BlockAssembler {
    pub fn new(rpc: Arc<ZcashRpcClient>) -> Self {
        Self { rpc }
    }

    pub async fn submit_block(&self, block_hex: &str) -> Result<(), BlockSubmitError> {
        info!(
            block_hex_len = block_hex.len(),
            block_bytes = block_hex.len() / 2,
            header_preview = &block_hex[..std::cmp::min(216, block_hex.len())],
            "Submitting solved block to network"
        );

        // First try proposal mode for detailed validation
        match self.rpc.validate_block_proposal(block_hex).await {
            Ok(result) => {
                if let Some(reject) = &result {
                    warn!(reason = %reject, "Block proposal rejected (pre-check)");
                } else {
                    info!("Block proposal validated OK");
                }
            }
            Err(e) => {
                // Proposal mode may not be supported; log and continue
                warn!(error = %e, "Block proposal validation failed (may not be supported)");
            }
        }

        match self.rpc.submit_block(block_hex).await {
            Ok(result) => {
                if let Some(reject_reason) = result {
                    if reject_reason.is_empty() || reject_reason == "null" {
                        info!("Block accepted by network!");
                        Ok(())
                    } else {
                        error!(reason = %reject_reason, "Block rejected by network");
                        Err(BlockSubmitError::Rejected(reject_reason))
                    }
                } else {
                    info!("Block accepted by network!");
                    Ok(())
                }
            }
            Err(e) => {
                error!(error = %e, "Block submission RPC failed");
                Err(BlockSubmitError::Rpc(e))
            }
        }
    }
}

/// Outcome of post-submit chain-inclusion verification (audit P11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InclusionCheck {
    /// getblockhash(height) returned our hash — the block is on the best chain.
    Verified,
    /// getblockhash(height) returned a different hash — our block lost the
    /// height race (or was accepted only as a side-chain block). Crediting
    /// would create phantom rewards that reverse_block_credits later claws
    /// back sloppily; the caller should skip record+distribute.
    Mismatch,
    /// The verification RPC itself failed. Inconclusive — the caller should
    /// proceed as before (credit) and rely on the maturity sweep, which
    /// re-checks the hash at +maturity_confirmations and orphans on mismatch.
    Unknown,
}

impl BlockAssembler {
    /// Verify that the block we just submitted is actually on zebra's best
    /// chain (audit P11). `submitblock` returning success only means the
    /// block passed validation — duplicates, stale parents, and side-chain
    /// blocks can all "succeed" without becoming the best block at `height`.
    ///
    /// `our_hash_hex` may be in either byte order; both forms are compared
    /// (consistent with check_block_maturity's tolerance).
    pub async fn verify_block_inclusion(&self, height: u64, our_hash_hex: &str) -> InclusionCheck {
        let our_le = our_hash_hex.to_lowercase();
        let our_be = {
            let bytes = hex::decode(&our_le).unwrap_or_default();
            hex::encode(bytes.into_iter().rev().collect::<Vec<u8>>())
        };

        // submitblock commits synchronously, so the first attempt should hit;
        // the retries paper over transient RPC failures only.
        let mut last_err: Option<node_rpc::RpcError> = None;
        for attempt in 0..3u8 {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            match self.rpc.get_block_hash(height).await {
                Ok(chain_hash) => {
                    let chain_hash = chain_hash.to_lowercase();
                    return if chain_hash == our_le || chain_hash == our_be {
                        InclusionCheck::Verified
                    } else {
                        warn!(
                            height,
                            ours = %our_be,
                            chain = %chain_hash,
                            "Submitted block is not the best block at its height"
                        );
                        InclusionCheck::Mismatch
                    };
                }
                Err(e) => last_err = Some(e),
            }
        }
        warn!(
            height,
            error = %last_err.map(|e| e.to_string()).unwrap_or_default(),
            "Could not verify block inclusion (RPC failed); proceeding on submitblock's word"
        );
        InclusionCheck::Unknown
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BlockSubmitError {
    #[error("Block rejected: {0}")]
    Rejected(String),
    #[error("RPC error: {0}")]
    Rpc(#[from] node_rpc::RpcError),
}
