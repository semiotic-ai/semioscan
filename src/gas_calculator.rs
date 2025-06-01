use std::sync::Arc;

use alloy_network::Network;
use alloy_primitives::{Address, U256};
use alloy_provider::RootProvider;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::GasCache;

#[derive(Debug, Clone)]
pub enum GasForTx {
    L1(L1Gas),
    L2(L2Gas),
}

impl From<(U256, U256)> for GasForTx {
    fn from((gas_used, effective_gas_price): (U256, U256)) -> Self {
        Self::L1(L1Gas::from((gas_used, effective_gas_price)))
    }
}

impl From<(U256, U256, U256)> for GasForTx {
    fn from((gas_used, effective_gas_price, l1_data_fee): (U256, U256, U256)) -> Self {
        Self::L2(L2Gas::from((gas_used, effective_gas_price, l1_data_fee)))
    }
}

#[derive(Debug, Clone)]
pub struct L1Gas {
    pub gas_used: U256,
    pub effective_gas_price: U256,
}

impl From<(U256, U256)> for L1Gas {
    fn from((gas_used, effective_gas_price): (U256, U256)) -> Self {
        Self {
            gas_used,
            effective_gas_price,
        }
    }
}

#[derive(Debug, Clone)]
pub struct L2Gas {
    pub gas_used: U256,
    pub effective_gas_price: U256,
    pub l1_data_fee: U256,
}

impl From<(U256, U256, U256)> for L2Gas {
    fn from((gas_used, effective_gas_price, l1_data_fee): (U256, U256, U256)) -> Self {
        Self {
            gas_used,
            effective_gas_price,
            l1_data_fee,
        }
    }
}

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

    pub fn add_l1_fee(&mut self, l1_fee: U256) {
        self.total_gas_cost = self.total_gas_cost.saturating_add(l1_fee);
    }

    /// Add a transaction to the gas cost result
    ///
    /// This will add the gas cost for the transaction to the total gas cost
    /// and increment the transaction count.
    ///
    /// If the transaction is an L2 transaction, it will add the L1 data fee to the total gas cost.
    ///
    /// If the transaction is an L1 transaction, it will add the gas cost for the transaction to the total gas cost
    /// and increment the transaction count.
    pub fn add_transaction(&mut self, gas: GasForTx) {
        match gas {
            GasForTx::L1(gas) => {
                let gas_cost = gas.gas_used.saturating_mul(gas.effective_gas_price);
                self.total_gas_cost = self.total_gas_cost.saturating_add(gas_cost);
                self.transaction_count += 1;
            }
            GasForTx::L2(gas) => {
                let l2_gas_cost = gas.gas_used.saturating_mul(gas.effective_gas_price);
                let l1_data_fee = gas.l1_data_fee;
                let total_gas_cost = l2_gas_cost.saturating_add(l1_data_fee);
                self.total_gas_cost = self.total_gas_cost.saturating_add(total_gas_cost);
                self.transaction_count += 1;
            }
        }
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
