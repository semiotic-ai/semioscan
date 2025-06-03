use alloy_primitives::{keccak256, Address, B256, U256};
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_types::Filter;
use alloy_sol_types::SolEvent;
use tokio::time::{sleep, Duration};
use tracing::{error, info};

use crate::{Transfer, TRANSFER_EVENT_SIGNATURE};

pub struct AmountResult {
    pub chain_id: u64,
    pub to: Address,
    pub token: Address,
    pub amount: U256,
}

pub struct AmountCalculator {
    provider: RootProvider,
}

impl AmountCalculator {
    pub fn new(provider: RootProvider) -> Self {
        Self { provider }
    }

    pub async fn calculate_transfer_amount_between_blocks(
        &self,
        chain_id: u64,
        from: Address,
        to: Address,
        token: Address,
        from_block: u64,
        to_block: u64,
    ) -> anyhow::Result<AmountResult> {
        let mut result = AmountResult {
            chain_id,
            to,
            token,
            amount: U256::ZERO,
        };

        let contract_address = token;

        let transfer_topic = B256::from_slice(&*keccak256(TRANSFER_EVENT_SIGNATURE.as_bytes()));

        let mut current_block = from_block;

        while current_block <= to_block {
            let end_chunk_block = std::cmp::min(current_block + 499, to_block);

            let filter = Filter::new()
                .from_block(current_block)
                .to_block(end_chunk_block)
                .address(contract_address)
                .event_signature(vec![transfer_topic])
                .topic1(from)
                .topic2(to);

            let logs = self.provider.get_logs(&filter).await?;

            for log in logs {
                match Transfer::decode_log(&log.into()) {
                    Ok(event) => {
                        info!(
                            chain_id = chain_id,
                            to = ?to,
                            token = ?token,
                            amount = ?event.value,
                            block = ?current_block,
                            current_total_amount = ?result.amount,
                            "Adding transfer amount to result"
                        );
                        result.amount = result.amount.saturating_add(event.value);
                    }
                    Err(e) => {
                        error!(error = ?e, "Failed to decode Transfer log");
                    }
                }
            }

            current_block = end_chunk_block + 1;

            // Add a small delay to avoid hitting rate limits on Sonic Alchemy endpoint
            if chain_id.eq(&146) && current_block <= to_block {
                sleep(Duration::from_millis(250)).await;
            }
        }

        info!(
            chain_id = chain_id,
            to = ?to,
            token = ?token,
            total_amount = ?result.amount,
            "Finished amount calculation"
        );

        Ok(result)
    }
}
