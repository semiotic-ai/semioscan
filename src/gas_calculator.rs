use std::sync::Arc;

use alloy_network::Network;
use alloy_primitives::{Address, U256};
use alloy_provider::RootProvider;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::GasCache;

#[derive(Default, Debug, Clone, Serialize)]
pub struct GasCostResult {
    pub chain_id: u64,
    pub from: Address,
    pub to: Address,
    pub total_gas_cost: U256,
    pub transaction_count: usize,
}

impl GasCostResult {
    pub fn new(chain_id: u64, from: Address, to: Address) -> Self {
        Self {
            from,
            to,
            chain_id,
            total_gas_cost: U256::ZERO,
            transaction_count: 0,
        }
    }

    pub fn add_transaction(&mut self, gas_used: U256, effective_gas_price: U256) {
        let gas_cost = gas_used.saturating_mul(effective_gas_price);
        self.total_gas_cost = self.total_gas_cost.saturating_add(gas_cost);
        self.transaction_count += 1;
    }

    /// Merge another gas cost result into this one
    pub fn merge(&mut self, other: &Self) {
        self.total_gas_cost = self.total_gas_cost.saturating_add(other.total_gas_cost);
        self.transaction_count += other.transaction_count;
    }

    /// Get the total gas cost formatted as a string
    pub fn formatted_gas_cost(&self) -> String {
        self.format_gas_cost()
    }

    fn format_gas_cost(&self) -> String {
        let gas_cost = self.total_gas_cost;

        const DECIMALS: u8 = 18; // All EVM chains use 18 decimals
        let divisor = U256::from(10).pow(U256::from(DECIMALS));

        let whole = gas_cost / divisor;
        let fractional = gas_cost % divisor;

        // Convert fractional part to string with leading zeros
        let fractional_str = format!("{:0width$}", fractional, width = DECIMALS as usize);

        // Format with proper decimal places, ensuring we don't have trailing zeros
        format!("{}.{}", whole, fractional_str.trim_end_matches('0'))
    }
}

// Maximum number of blocks to query in a single request
pub(crate) const MAX_BLOCK_RANGE: u64 = 500;

pub struct GasCostCalculator<N: Network> {
    pub(crate) provider: RootProvider<N>,
    pub(crate) gas_cache: Arc<Mutex<GasCache>>,
}

impl<N: Network> GasCostCalculator<N> {
    pub fn new(provider: RootProvider<N>) -> Self {
        Self {
            provider,
            gas_cache: Arc::new(Mutex::new(GasCache::default())),
        }
    }

    pub fn with_cache(provider: RootProvider<N>, gas_cache: Arc<Mutex<GasCache>>) -> Self {
        Self {
            provider,
            gas_cache,
        }
    }
}
