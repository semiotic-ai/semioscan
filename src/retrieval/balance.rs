// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Batch balance fetching utilities
//!
//! This module provides utilities for efficiently fetching multiple token balances
//! in a single batch operation. When used with Alloy's `CallBatchLayer`, the
//! parallel balance queries are automatically batched into a single Multicall3
//! RPC request.
//!
//! # Performance
//!
//! The batch fetcher uses `futures::join_all` to execute all balance queries
//! in parallel. When the provider is configured with `CallBatchLayer`:
//!
//! ```rust,ignore
//! use alloy_provider::{layers::CallBatchLayer, ProviderBuilder};
//! use std::time::Duration;
//!
//! let provider = ProviderBuilder::new()
//!     .layer(CallBatchLayer::new().wait(Duration::from_millis(10)))
//!     .connect_http(url);
//! ```
//!
//! All parallel `eth_call` requests are automatically batched into a single
//! Multicall3 contract call, reducing RPC overhead and rate limit consumption.
//!
//! # Example
//!
//! ```rust,ignore
//! use semioscan::retrieval::balance::batch_fetch_balances;
//! use alloy_primitives::Address;
//!
//! // Define balance queries as (token_address, holder_address) pairs
//! let queries = vec![
//!     (usdc_address, alice),
//!     (usdc_address, bob),
//!     (weth_address, alice),
//! ];
//!
//! let results = batch_fetch_balances(&provider, &queries).await;
//!
//! for result in results {
//!     match result {
//!         Ok((token, holder, balance)) => {
//!             println!("{holder} holds {balance} of token {token}");
//!         }
//!         Err((token, holder, e)) => {
//!             eprintln!("Failed to fetch balance for {holder} on {token}: {e}");
//!         }
//!     }
//! }
//! ```

use alloy_erc20_full::LazyToken;
use alloy_network::Network;
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use alloy_rpc_types::TransactionTrait;
use futures::future::join_all;
use tracing::{info, warn};

/// Query for a token balance: (token_address, holder_address)
pub type BalanceQuery = (Address, Address);

/// Result of a successful balance fetch: (token_address, holder_address, balance)
pub type BalanceResult = (Address, Address, U256);

/// Error from a failed balance fetch: (token_address, holder_address, error_message)
pub type BalanceError = (Address, Address, String);

/// Batch fetch token balances for multiple (token, holder) pairs.
///
/// This function executes all balance queries in parallel using `futures::join_all`.
/// When combined with Alloy's `CallBatchLayer`, the parallel `eth_call` requests
/// are automatically batched into a single Multicall3 RPC request.
///
/// # Arguments
///
/// * `provider` - The Alloy provider to use for RPC calls
/// * `queries` - Slice of (token_address, holder_address) pairs to query
///
/// # Returns
///
/// A vector of results, one for each query. Each result is either:
/// - `Ok((token, holder, balance))` on success
/// - `Err((token, holder, error_message))` on failure
///
/// # Performance
///
/// For N queries:
/// - Without `CallBatchLayer`: N separate RPC calls
/// - With `CallBatchLayer`: 1 batched RPC call (Multicall3)
///
/// # Example
///
/// ```rust,ignore
/// let queries = vec![
///     (usdc, alice),
///     (usdc, bob),
///     (weth, alice),
/// ];
///
/// let results = batch_fetch_balances(&provider, &queries).await;
/// ```
pub async fn batch_fetch_balances<N, P>(
    provider: &P,
    queries: &[BalanceQuery],
) -> Vec<Result<BalanceResult, BalanceError>>
where
    N: Network,
    P: Provider<N> + Clone,
    N::TransactionResponse:
        TransactionTrait + alloy_provider::network::eip2718::Typed2718 + Send + Sync + Clone,
    N::ReceiptResponse: Send + Sync + std::fmt::Debug + Clone,
{
    if queries.is_empty() {
        return vec![];
    }

    info!(count = queries.len(), "Batch fetching token balances");

    // Create futures for all balance fetches
    let fetch_futures: Vec<_> = queries
        .iter()
        .map(|&(token_address, holder_address)| {
            let provider = provider.clone();
            async move {
                let token = LazyToken::new(token_address, provider);
                match token.balance_of(holder_address).await {
                    Ok(balance) => Ok((token_address, holder_address, balance)),
                    Err(e) => {
                        warn!(
                            ?token_address,
                            ?holder_address,
                            error = ?e,
                            "Failed to fetch token balance"
                        );
                        Err((token_address, holder_address, e.to_string()))
                    }
                }
            }
        })
        .collect();

    // Execute all fetches in parallel
    // When CallBatchLayer is enabled, these will be automatically batched
    join_all(fetch_futures).await
}

/// Batch fetch ETH balances for multiple addresses.
///
/// This function executes all ETH balance queries in parallel.
/// When combined with Alloy's `CallBatchLayer`, the parallel requests
/// are automatically batched.
///
/// # Arguments
///
/// * `provider` - The Alloy provider to use for RPC calls
/// * `addresses` - Slice of addresses to query ETH balances for
///
/// # Returns
///
/// A vector of results, one for each address. Each result is either:
/// - `Ok((address, balance))` on success
/// - `Err((address, error_message))` on failure
pub async fn batch_fetch_eth_balances<N, P>(
    provider: &P,
    addresses: &[Address],
) -> Vec<Result<(Address, U256), (Address, String)>>
where
    N: Network,
    P: Provider<N> + Clone,
{
    if addresses.is_empty() {
        return vec![];
    }

    info!(count = addresses.len(), "Batch fetching ETH balances");

    let fetch_futures: Vec<_> = addresses
        .iter()
        .map(|&address| {
            let provider = provider.clone();
            async move {
                match provider.get_balance(address).await {
                    Ok(balance) => Ok((address, balance)),
                    Err(e) => {
                        warn!(
                            ?address,
                            error = ?e,
                            "Failed to fetch ETH balance"
                        );
                        Err((address, e.to_string()))
                    }
                }
            }
        })
        .collect();

    join_all(fetch_futures).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balance_query_type() {
        let token = Address::ZERO;
        let holder = Address::ZERO;
        let query: BalanceQuery = (token, holder);
        assert_eq!(query.0, token);
        assert_eq!(query.1, holder);
    }
}
