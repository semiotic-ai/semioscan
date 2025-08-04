use std::collections::BTreeSet;

use alloy_chains::NamedChain;
use alloy_primitives::{keccak256, Address, U256};
use alloy_provider::Provider;
use alloy_rpc_types::Filter;
use alloy_sol_types::SolEvent;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info};

use crate::Transfer;

/// Extract tokens from a router contract.
///
/// Used for extracting router contract token balances by Likwid.
pub async fn extract_transferred_to_tokens<T: Provider>(
    provider: &T,
    chain: NamedChain,
    router: Address,
    start_block: u64,
    end_block: u64,
) -> anyhow::Result<BTreeSet<Address>> {
    info!(
        chain = %chain,
        router = %router,
        start_block = start_block,
        end_block = end_block,
        "Fetching Transfer logs"
    );

    const MAX_BLOCK_RANGE: u64 = 500;

    let mut current_block = start_block;

    // BTreeSet is used to deduplicate tokens while preserving their original order.
    let mut transferred_to_tokens = BTreeSet::new();

    while current_block <= end_block {
        let to_block = std::cmp::min(current_block + MAX_BLOCK_RANGE - 1, end_block);

        let filter = Filter::new()
            .from_block(current_block)
            .to_block(to_block)
            .event_signature(*keccak256(b"Transfer(address,address,uint256)"))
            .topic2(U256::from_be_bytes(router.into_word().into()));

        match provider.get_logs(&filter).await {
            Ok(logs) => {
                for log in logs {
                    let token_address = log.address();
                    match Transfer::decode_log(&log.into()) {
                        Ok(event) if event.to == router => {
                            debug!(extracted_token = ?token_address);
                            transferred_to_tokens.insert(token_address);
                        }
                        Err(e) => {
                            error!(?e, "Failed to decode Transfer log");
                            continue;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                error!(?e, %current_block, %to_block, "Error fetching logs in range");
            }
        }

        current_block = to_block + 1;

        // Add a small delay to avoid hitting rate limits on Sonic Alchemy endpoint
        if let (NamedChain::Sonic, true) = (chain, current_block <= to_block) {
            sleep(Duration::from_millis(250)).await;
        }
    }

    Ok(transferred_to_tokens)
}
