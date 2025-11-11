//! Gas cost calculation for blockchain transactions
//!
//! This module provides tools for calculating total gas costs for transactions between
//! two addresses over a given block range. It handles both L1 (Ethereum) and L2 (Optimism Stack)
//! chains correctly, including L1 data fees for L2 transactions.
//!
//! # Examples
//!
//! ```rust,ignore
//! use semioscan::GasCalculator;
//! use alloy_provider::ProviderBuilder;
//!
//! let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
//! let calculator = GasCalculator::new(provider.clone());
//!
//! let result = calculator
//!     .get_gas_cost(chain_id, from_addr, to_addr, start_block, end_block)
//!     .await?;
//!
//! println!("Total gas cost: {} wei", result.total_gas_cost);
//! println!("Transactions: {}", result.transaction_count);
//! ```

use std::sync::Arc;

use alloy_network::Network;
use alloy_primitives::{Address, U256};
use alloy_provider::RootProvider;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::{GasCache, SemioscanConfig};

/// Gas data for a single transaction
///
/// This enum represents gas costs for either L1 or L2 transactions. L2 transactions
/// include additional L1 data fees that are automatically included in calculations.
#[derive(Debug, Clone)]
pub enum GasForTx {
    /// L1 (Ethereum) transaction gas data
    L1(L1Gas),
    /// L2 (Optimism Stack) transaction gas data with L1 data fee
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

/// Gas data for L1 (Ethereum) transactions
///
/// L1 transactions have a simple gas cost calculation:
/// `total_cost = gas_used * effective_gas_price`
#[derive(Debug, Clone)]
pub struct L1Gas {
    /// Amount of gas consumed by the transaction
    pub gas_used: U256,
    /// Effective gas price paid per unit of gas (in wei)
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

/// Gas data for L2 (Optimism Stack) transactions
///
/// L2 transactions have an additional L1 data fee component:
/// `total_cost = (gas_used * effective_gas_price) + l1_data_fee`
///
/// The L1 data fee covers the cost of posting transaction data to the L1 chain.
#[derive(Debug, Clone)]
pub struct L2Gas {
    /// Amount of L2 gas consumed by the transaction
    pub gas_used: U256,
    /// Effective L2 gas price paid per unit of gas (in wei)
    pub effective_gas_price: U256,
    /// L1 data fee for posting transaction to L1 chain (in wei)
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

/// Result of gas cost calculation over a block range
///
/// Contains the total gas costs paid for all transactions from one address to another,
/// along with the number of transactions processed.
///
/// # Units
///
/// All gas costs are in wei (the smallest unit of native chain currency).
///
/// # L2 Handling
///
/// For L2 chains (Arbitrum, Base, Optimism, etc.), the `total_gas_cost` automatically
/// includes both L2 execution gas and L1 data fees.
#[derive(Default, Debug, Clone, Serialize)]
pub struct GasCostResult {
    /// Chain ID where the transactions occurred
    pub chain_id: u64,
    /// Address that sent the transactions
    pub from: Address,
    /// Address that received the transactions
    pub to: Address,
    /// Total gas cost in wei (includes L1 data fees for L2 chains)
    pub total_gas_cost: U256,
    /// Number of transactions processed
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

// Maximum number of blocks to query in a single request (legacy default, now deprecated)
// Replaced by SemioscanConfig.max_block_range - use config.get_max_block_range(chain) instead
#[deprecated(
    since = "0.2.0",
    note = "Use SemioscanConfig.get_max_block_range(chain) instead"
)]
#[allow(dead_code)]
pub(crate) const MAX_BLOCK_RANGE: u64 = 500;

pub struct GasCostCalculator<N: Network> {
    pub(crate) provider: RootProvider<N>,
    pub(crate) gas_cache: Arc<Mutex<GasCache>>,
    pub(crate) config: SemioscanConfig,
}

impl<N: Network> GasCostCalculator<N> {
    /// Create a new gas cost calculator with default configuration
    pub fn new(provider: RootProvider<N>) -> Self {
        Self::with_config(provider, SemioscanConfig::default())
    }

    /// Create a gas cost calculator with custom configuration
    pub fn with_config(provider: RootProvider<N>, config: SemioscanConfig) -> Self {
        Self {
            provider,
            gas_cache: Arc::new(Mutex::new(GasCache::default())),
            config,
        }
    }

    /// Create a gas cost calculator with custom cache and configuration
    pub fn with_cache_and_config(
        provider: RootProvider<N>,
        gas_cache: Arc<Mutex<GasCache>>,
        config: SemioscanConfig,
    ) -> Self {
        Self {
            provider,
            gas_cache,
            config,
        }
    }

    /// Create a gas cost calculator with custom cache (uses default config)
    pub fn with_cache(provider: RootProvider<N>, gas_cache: Arc<Mutex<GasCache>>) -> Self {
        Self::with_cache_and_config(provider, gas_cache, SemioscanConfig::default())
    }
}
