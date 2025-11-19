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

use alloy_chains::NamedChain;
use alloy_network::Network;
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::config::SemioscanConfig;
use crate::gas::cache::GasCache;
use crate::retrieval::DecimalPrecision;
use crate::types::config::TransactionCount;
use crate::types::fees::L1DataFee;
use crate::types::gas::{GasAmount, GasPrice};
use crate::types::wei::WeiAmount;

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
    pub gas_used: GasAmount,
    /// Effective gas price paid per unit of gas (in wei)
    pub effective_gas_price: GasPrice,
}

impl From<(U256, U256)> for L1Gas {
    fn from((gas_used, effective_gas_price): (U256, U256)) -> Self {
        Self {
            gas_used: GasAmount::from(gas_used),
            effective_gas_price: GasPrice::from(effective_gas_price),
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
    pub gas_used: GasAmount,
    /// Effective L2 gas price paid per unit of gas (in wei)
    pub effective_gas_price: GasPrice,
    /// L1 data fee for posting transaction to L1 chain
    pub l1_data_fee: L1DataFee,
}

impl From<(U256, U256, U256)> for L2Gas {
    fn from((gas_used, effective_gas_price, l1_data_fee): (U256, U256, U256)) -> Self {
        Self {
            gas_used: GasAmount::from(gas_used),
            effective_gas_price: GasPrice::from(effective_gas_price),
            l1_data_fee: L1DataFee::new(l1_data_fee),
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
    /// Chain where the transactions occurred
    pub chain: NamedChain,
    /// Address that sent the transactions
    pub from: Address,
    /// Address that received the transactions
    pub to: Address,
    /// Total gas cost in wei (includes L1 data fees for L2 chains)
    pub total_gas_cost: WeiAmount,
    /// Number of transactions processed
    pub transaction_count: TransactionCount,
}

impl GasCostResult {
    pub fn new(chain: NamedChain, from: Address, to: Address) -> Self {
        Self {
            from,
            to,
            chain,
            total_gas_cost: WeiAmount::ZERO,
            transaction_count: TransactionCount::ZERO,
        }
    }

    pub fn add_l1_fee(&mut self, l1_fee: L1DataFee) {
        self.total_gas_cost = self.total_gas_cost + WeiAmount::from(l1_fee.as_u256());
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
                let gas_cost = gas.gas_used * gas.effective_gas_price;
                self.total_gas_cost = self.total_gas_cost + WeiAmount::from(gas_cost);
                self.transaction_count.increment();
            }
            GasForTx::L2(gas) => {
                let l2_gas_cost = gas.gas_used * gas.effective_gas_price;
                let l1_data_fee = gas.l1_data_fee.as_u256();
                let total_gas_cost = l2_gas_cost.saturating_add(l1_data_fee);
                self.total_gas_cost = self.total_gas_cost + WeiAmount::from(total_gas_cost);
                self.transaction_count.increment();
            }
        }
    }

    /// Merge another gas cost result into this one
    pub fn merge(&mut self, other: &Self) {
        self.total_gas_cost = self.total_gas_cost + other.total_gas_cost;
        self.transaction_count += other.transaction_count;
    }

    /// Get the total gas cost formatted as a string
    pub fn formatted_gas_cost(&self) -> String {
        self.format_gas_cost()
    }

    fn format_gas_cost(&self) -> String {
        let gas_cost = self.total_gas_cost.as_u256();

        let decimals = DecimalPrecision::NativeToken.decimals();

        let divisor = U256::from(10).pow(U256::from(decimals));

        let whole = gas_cost / divisor;
        let fractional = gas_cost % divisor;

        // Convert fractional part to string with leading zeros
        let fractional_str = format!("{:0width$}", fractional, width = decimals as usize);

        // Format with proper decimal places, ensuring we don't have trailing zeros
        format!("{}.{}", whole, fractional_str.trim_end_matches('0'))
    }
}

pub struct GasCostCalculator<N: Network, P: Provider<N>> {
    pub(crate) provider: P,
    pub(crate) gas_cache: Arc<Mutex<GasCache>>,
    pub(crate) config: SemioscanConfig,
    pub(crate) _phantom: std::marker::PhantomData<N>,
}

impl<N: Network, P: Provider<N>> GasCostCalculator<N, P> {
    /// Create a new gas cost calculator with default configuration
    pub fn new(provider: P) -> Self {
        Self::with_config(provider, SemioscanConfig::default())
    }

    /// Create a gas cost calculator with custom configuration
    pub fn with_config(provider: P, config: SemioscanConfig) -> Self {
        Self {
            provider,
            gas_cache: Arc::new(Mutex::new(GasCache::default())),
            config,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Create a gas cost calculator with custom cache and configuration
    pub fn with_cache_and_config(
        provider: P,
        gas_cache: Arc<Mutex<GasCache>>,
        config: SemioscanConfig,
    ) -> Self {
        Self {
            provider,
            gas_cache,
            config,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Create a gas cost calculator with custom cache (uses default config)
    pub fn with_cache(provider: P, gas_cache: Arc<Mutex<GasCache>>) -> Self {
        Self::with_cache_and_config(provider, gas_cache, SemioscanConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    #[test]
    fn test_gas_cost_result_add_transaction_l1() {
        let from = address!("1111111111111111111111111111111111111111");
        let to = address!("2222222222222222222222222222222222222222");
        let mut result = GasCostResult::new(NamedChain::Mainnet, from, to);

        // Add first transaction: 21000 gas at 50 gwei = 1,050,000,000,000,000 wei
        result.add_transaction(GasForTx::L1(L1Gas {
            gas_used: GasAmount::new(21000),
            effective_gas_price: GasPrice::from_gwei(50),
        }));

        assert_eq!(result.transaction_count, TransactionCount::new(1));
        assert_eq!(
            result.total_gas_cost,
            WeiAmount::from(1_050_000_000_000_000u64)
        );

        // Add second transaction: 100000 gas at 60 gwei = 6,000,000,000,000,000 wei
        result.add_transaction(GasForTx::L1(L1Gas {
            gas_used: GasAmount::new(100000),
            effective_gas_price: GasPrice::from_gwei(60),
        }));

        assert_eq!(result.transaction_count, TransactionCount::new(2));
        // Total: 1,050,000,000,000,000 + 6,000,000,000,000,000 = 7,050,000,000,000,000
        assert_eq!(
            result.total_gas_cost,
            WeiAmount::from(7_050_000_000_000_000u64)
        );
    }

    #[test]
    fn test_gas_cost_result_add_transaction_l2() {
        let from = address!("1111111111111111111111111111111111111111");
        let to = address!("2222222222222222222222222222222222222222");
        let mut result = GasCostResult::new(NamedChain::Arbitrum, from, to); // Arbitrum

        // Add L2 transaction: 150000 gas at 0.1 gwei + 0.005 ETH L1 data fee
        result.add_transaction(GasForTx::L2(L2Gas {
            gas_used: GasAmount::new(150000),
            effective_gas_price: GasPrice::new(100_000_000), // 0.1 gwei
            l1_data_fee: L1DataFee::new(U256::from(5_000_000_000_000_000u64)), // 0.005 ETH
        }));

        assert_eq!(result.transaction_count, TransactionCount::new(1));
        // L2 gas: 150000 * 100,000,000 = 15,000,000,000,000
        // L1 fee: 5,000,000,000,000,000
        // Total: 5,015,000,000,000,000
        assert_eq!(
            result.total_gas_cost,
            WeiAmount::from(5_015_000_000_000_000u64)
        );
    }

    #[test]
    fn test_gas_cost_result_merge() {
        let from = address!("1111111111111111111111111111111111111111");
        let to = address!("2222222222222222222222222222222222222222");

        let mut result1 = GasCostResult {
            chain: NamedChain::Mainnet,
            from,
            to,
            total_gas_cost: WeiAmount::from(1_000_000_000_000_000u64),
            transaction_count: TransactionCount::new(5),
        };

        let result2 = GasCostResult {
            chain: NamedChain::Mainnet,
            from,
            to,
            total_gas_cost: WeiAmount::from(500_000_000_000_000u64),
            transaction_count: TransactionCount::new(3),
        };

        result1.merge(&result2);

        // Test that merge adds both gas costs and transaction counts
        assert_eq!(
            result1.total_gas_cost,
            WeiAmount::from(1_500_000_000_000_000u64)
        );
        assert_eq!(result1.transaction_count, TransactionCount::new(8));
    }

    #[test]
    fn test_gas_cost_result_merge_with_zero() {
        let from = address!("1111111111111111111111111111111111111111");
        let to = address!("2222222222222222222222222222222222222222");

        let mut result = GasCostResult {
            chain: NamedChain::Mainnet,
            from,
            to,
            total_gas_cost: WeiAmount::from(1_000_000u64),
            transaction_count: TransactionCount::new(5),
        };

        let empty = GasCostResult::new(NamedChain::Mainnet, from, to);

        result.merge(&empty);

        // Merging with empty result should not change values
        assert_eq!(result.total_gas_cost, WeiAmount::from(1_000_000u64));
        assert_eq!(result.transaction_count, TransactionCount::new(5));
    }

    #[test]
    fn test_gas_cost_overflow_protection() {
        let from = address!("1111111111111111111111111111111111111111");
        let to = address!("2222222222222222222222222222222222222222");
        let mut result = GasCostResult::new(NamedChain::Mainnet, from, to);

        // Set to near-max value
        result.total_gas_cost = WeiAmount::from(U256::MAX - U256::from(1000u64));

        // Add transaction that would overflow - should saturate
        result.add_transaction(GasForTx::L1(L1Gas {
            gas_used: GasAmount::new(1000000),
            effective_gas_price: GasPrice::new(1000000),
        }));

        // Should saturate at U256::MAX, not wrap around
        assert_eq!(result.total_gas_cost, WeiAmount::from(U256::MAX));
        assert_eq!(result.transaction_count, TransactionCount::new(1));
    }

    #[test]
    fn test_gas_cost_merge_overflow_protection() {
        let from = address!("1111111111111111111111111111111111111111");
        let to = address!("2222222222222222222222222222222222222222");

        let mut result1 = GasCostResult {
            chain: NamedChain::Mainnet,
            from,
            to,
            total_gas_cost: WeiAmount::from(U256::MAX - U256::from(100u64)),
            transaction_count: TransactionCount::new(5),
        };

        let result2 = GasCostResult {
            chain: NamedChain::Mainnet,
            from,
            to,
            total_gas_cost: WeiAmount::from(500u64),
            transaction_count: TransactionCount::new(3),
        };

        result1.merge(&result2);

        // Should saturate at U256::MAX
        assert_eq!(result1.total_gas_cost, WeiAmount::from(U256::MAX));
        assert_eq!(result1.transaction_count, TransactionCount::new(8));
    }

    #[test]
    fn test_gas_cost_result_zero_transactions() {
        let from = address!("1111111111111111111111111111111111111111");
        let to = address!("2222222222222222222222222222222222222222");
        let result = GasCostResult::new(NamedChain::Mainnet, from, to);

        assert_eq!(result.total_gas_cost, WeiAmount::ZERO);
        assert_eq!(result.transaction_count, TransactionCount::ZERO);
        assert_eq!(result.chain, NamedChain::Mainnet);
        assert_eq!(result.from, from);
        assert_eq!(result.to, to);
    }

    #[test]
    fn test_add_l1_fee() {
        let from = address!("1111111111111111111111111111111111111111");
        let to = address!("2222222222222222222222222222222222222222");
        let mut result = GasCostResult::new(NamedChain::Arbitrum, from, to);

        result.add_l1_fee(L1DataFee::new(U256::from(1_000_000_000_000_000u64)));
        assert_eq!(
            result.total_gas_cost,
            WeiAmount::from(1_000_000_000_000_000u64)
        );

        result.add_l1_fee(L1DataFee::new(U256::from(500_000_000_000_000u64)));
        assert_eq!(
            result.total_gas_cost,
            WeiAmount::from(1_500_000_000_000_000u64)
        );
    }

    #[test]
    fn test_formatted_gas_cost() {
        let from = address!("1111111111111111111111111111111111111111");
        let to = address!("2222222222222222222222222222222222222222");

        let mut result = GasCostResult::new(NamedChain::Mainnet, from, to);
        result.total_gas_cost = WeiAmount::from(1_500_000_000_000_000_000u64); // 1.5 ETH

        let formatted = result.formatted_gas_cost();
        // Should format as "1.5" (trailing zeros removed)
        assert!(formatted.starts_with("1.5"));
    }
}
