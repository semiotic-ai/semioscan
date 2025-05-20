use alloy_primitives::{keccak256, Address, B256, U256};
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_types::Filter;
use alloy_sol_types::SolEvent;
use odos_sdk::OdosV2Router::SwapMulti;
use tracing::{error, info};

alloy_sol_types::sol! {
    event Transfer(address indexed from, address indexed to, uint256 value);
}

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

        // Calculate the Transfer event signature hash
        let transfer_signature = "Transfer(address,address,uint256)";
        let transfer_topic = B256::from_slice(&*keccak256(transfer_signature.as_bytes()));

        let mut current_block = from_block;
        while current_block <= to_block {
            let to_block = std::cmp::min(current_block + 1000, to_block);

            let filter = Filter::new()
                .from_block(current_block)
                .to_block(to_block)
                .address(contract_address)
                .topic2(to)
                .event_signature(vec![transfer_topic]);

            let logs = self.provider.get_logs(&filter).await?;

            for log in logs {
                match Transfer::decode_log(&log.into()) {
                    Ok(event) => {
                        if event.to == to {
                            result.amount = result.amount.saturating_add(event.value);
                        }
                    }
                    Err(e) => {
                        error!(error = ?e, "Failed to decode Transfer log");
                    }
                }
            }

            current_block = to_block + 1;
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

    /// Calculate the amount of a token received by a recipient
    /// for a given block range, using the SwapMulti event
    #[allow(clippy::too_many_arguments)]
    pub async fn calculate_swap_multi_amount_between_blocks(
        &self,
        chain_id: u64,
        router: Address,
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

        let contract_address = router;

        // Calculate the Transfer event signature hash
        let signature = "SwapMulti(address,uint256[],address[],uint256[],address[],uint32)";
        let topic = B256::from_slice(&*keccak256(signature.as_bytes()));

        let mut current_block = from_block;
        while current_block <= to_block {
            let to_block = std::cmp::min(current_block + 1000, to_block);

            let filter = Filter::new()
                .from_block(current_block)
                .to_block(to_block)
                .address(contract_address)
                .event_signature(vec![topic]);

            let logs = self.provider.get_logs(&filter).await?;

            for log in logs {
                match SwapMulti::decode_log(&log.into()) {
                    Ok(event) => {
                        if event.sender == from {
                            // Find the index of our target token in tokensOut
                            if let Some(index) = event
                                .tokensOut
                                .iter()
                                .filter(|&t| *t == token)
                                .position(|&t| t == token)
                            {
                                // Add the corresponding amount from amountsOut
                                if let Some(amount) = event.amountsOut.get(index) {
                                    result.amount = result.amount.saturating_add(*amount);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = ?e, "Failed to decode SwapMulti log");
                    }
                }
            }

            current_block = to_block + 1;
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
