// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Odos DEX price source implementation
//!
//! This module provides a reference implementation of [`PriceSource`]
//! for the Odos DEX aggregator. It demonstrates how to extract swap data from Odos router events.
//!
//! # Features
//!
//! This module is only available when the `odos-example` feature is enabled:
//!
//! ```toml
//! [dependencies]
//! semioscan = { version = "0.5", features = ["odos-example"] }
//! ```
//!
//! # Supported Events
//!
//! ## V2 Router
//! - **Swap** - Single-token swap event
//! - **SwapMulti** - Multi-token swap event
//!
//! ## V3 Router
//! - **Swap** - Single-token swap event (with referral/slippage data)
//! - **SwapMulti** - Multi-token swap event (with referral/slippage data)
//!
//! # Example Usage
//!
//! ```rust,ignore
//! use semioscan::price::odos::OdosPriceSource;
//! use alloy_chains::NamedChain;
//! use alloy_primitives::Address;
//! use odos_sdk::RouterType;
//!
//! // Chain-aware constructor (recommended)
//! let price_source = OdosPriceSource::for_chain(NamedChain::Arbitrum, RouterType::V2)?
//!     .with_liquidator_filter("0x123...".parse().unwrap());
//!
//! // Manual router address (fallback)
//! let router_address = "0xa669e7A0d4b3e4Fa48af2dE86BD4CD7126Be4e13".parse().unwrap();
//! let price_source = OdosPriceSource::new(router_address);
//!
//! // Use with PriceCalculator
//! let calculator = PriceCalculator::with_price_source(
//!     provider,
//!     Box::new(price_source),
//! );
//! ```

use super::{PriceSource, PriceSourceError, SwapData};
use alloy_chains::NamedChain;
use alloy_primitives::{Address, B256};
use alloy_rpc_types::Log;
use alloy_sol_types::SolEvent;
use odos_sdk::OdosV2Router::{Swap as SwapV2, SwapMulti as SwapMultiV2};
use odos_sdk::OdosV3Router::{Swap as SwapV3, SwapMulti as SwapMultiV3};
pub use odos_sdk::RouterType;
use odos_sdk::{get_v2_router_by_chain_id, get_v3_router_by_chain_id};

/// Errors from Odos price source operations
#[derive(Debug, thiserror::Error)]
pub enum OdosError {
    /// The specified chain does not have a router of the requested type
    #[error("Odos {router_type} router not available on chain {chain_name} (id: {chain_id})")]
    UnsupportedChain {
        /// The chain that was requested
        chain_name: &'static str,
        /// The chain ID
        chain_id: u64,
        /// The router type that was requested
        router_type: &'static str,
    },
    /// Router type does not emit swap events and cannot be used for price extraction
    #[error("{router_type} router not supported for price extraction (does not emit Swap/SwapMulti events)")]
    NonSwapRouterNotSupported {
        /// The router type that was rejected
        router_type: &'static str,
    },
}

impl OdosError {
    /// Create an error for an unsupported chain
    fn unsupported_chain(chain: NamedChain, router_type: RouterType) -> Self {
        Self::UnsupportedChain {
            chain_name: chain.as_str(),
            chain_id: chain as u64,
            router_type: router_type.as_str(),
        }
    }
}

/// Odos DEX price source implementation
///
/// Extracts swap data from Odos V2 router events (`Swap` and `SwapMulti`).
///
/// # Filtering
///
/// Optionally filter swaps by liquidator address using [`with_liquidator_filter`](OdosPriceSource::with_liquidator_filter).
/// This is useful when analyzing swaps from a specific address (e.g., your own liquidation bot).
///
/// # Event Handling
///
/// - **Single swaps** (`Swap` event): Direct token-to-token swaps
/// - **Multi swaps** (`SwapMulti` event): Complex multi-hop swaps with multiple input/output tokens
///
/// For `SwapMulti` events with multiple tokens, this implementation currently extracts
/// simple 1-to-1 token pairs. More complex multi-token handling can be added in the future.
#[derive(Debug)]
pub struct OdosPriceSource {
    router_address: Address,
    router_type: RouterType,
    liquidator_address: Option<Address>,
}

impl OdosPriceSource {
    /// Create a new Odos price source for the given router address
    ///
    /// Defaults to V2 router type for backward compatibility.
    /// Use [`for_chain`](Self::for_chain) for chain-aware construction with explicit router type.
    ///
    /// # Arguments
    ///
    /// * `router_address` - The Odos router contract address to scan for events
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let router = "0xa669e7A0d4b3e4Fa48af2dE86BD4CD7126Be4e13".parse().unwrap();
    /// let price_source = OdosPriceSource::new(router);
    /// ```
    pub fn new(router_address: Address) -> Self {
        Self {
            router_address,
            router_type: RouterType::V2,
            liquidator_address: None,
        }
    }

    /// Create an Odos price source for a specific chain and router type
    ///
    /// Automatically resolves the router address using odos-sdk's chain registry.
    /// This is the recommended constructor for most use cases.
    ///
    /// # Arguments
    ///
    /// * `chain` - The EVM chain (e.g., `NamedChain::Arbitrum`, `NamedChain::Mainnet`)
    /// * `router_type` - The router version (`RouterType::V2` or `RouterType::V3`)
    ///
    /// # Errors
    ///
    /// - Returns [`OdosError::NonSwapRouterNotSupported`] for router types that don't
    ///   emit `Swap/SwapMulti` events (e.g., `RouterType::LimitOrder`)
    /// - Returns [`OdosError::UnsupportedChain`] if Odos doesn't have the requested
    ///   router type deployed on the specified chain.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use semioscan::price::odos::{OdosPriceSource, RouterType};
    /// use alloy_chains::NamedChain;
    ///
    /// // V2 router on Arbitrum
    /// let source = OdosPriceSource::for_chain(NamedChain::Arbitrum, RouterType::V2)?;
    ///
    /// // V3 router on Mainnet
    /// let source = OdosPriceSource::for_chain(NamedChain::Mainnet, RouterType::V3)?;
    /// ```
    pub fn for_chain(chain: NamedChain, router_type: RouterType) -> Result<Self, OdosError> {
        // Only swap routers (V2/V3) emit Swap/SwapMulti events
        if !router_type.emits_swap_events() {
            return Err(OdosError::NonSwapRouterNotSupported {
                router_type: router_type.as_str(),
            });
        }

        let chain_id: u64 = chain.into();

        let router_address = match router_type {
            RouterType::V2 => get_v2_router_by_chain_id(chain_id),
            RouterType::V3 => get_v3_router_by_chain_id(chain_id),
            RouterType::LimitOrder => unreachable!("handled above"),
        }
        .ok_or_else(|| OdosError::unsupported_chain(chain, router_type))?;

        Ok(Self {
            router_address,
            router_type,
            liquidator_address: None,
        })
    }

    /// Create Odos price sources for all supported router types on a chain
    ///
    /// Returns price sources for V2 and V3 routers deployed on the specified chain.
    /// LimitOrder routers are excluded as they emit different events (`LimitOrderFilled`).
    ///
    /// # Arguments
    ///
    /// * `chain` - The EVM chain (e.g., `NamedChain::Arbitrum`, `NamedChain::Mainnet`)
    ///
    /// # Returns
    ///
    /// A vector of `OdosPriceSource` instances for V2 and V3 routers.
    /// The vector may be empty if the chain has no Odos V2/V3 routers deployed.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use semioscan::price::odos::OdosPriceSource;
    /// use alloy_chains::NamedChain;
    ///
    /// // Get all routers on Arbitrum (typically V2 and V3)
    /// let sources = OdosPriceSource::all_routers_for_chain(NamedChain::Arbitrum);
    /// println!("Found {} routers on Arbitrum", sources.len());
    ///
    /// // Use with multiple PriceCalculators or combine results
    /// for source in sources {
    ///     println!("Router: {:?}", source.router_address());
    /// }
    /// ```
    pub fn all_routers_for_chain(chain: NamedChain) -> Vec<Self> {
        RouterType::swap_routers()
            .into_iter()
            .filter_map(|router_type| Self::for_chain(chain, router_type).ok())
            .collect()
    }

    /// Add a filter to only include swaps from a specific liquidator address
    ///
    /// When set, only swaps where the sender matches this address will be included.
    ///
    /// # Arguments
    ///
    /// * `liquidator` - The address to filter by
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let price_source = OdosPriceSource::new(router)
    ///     .with_liquidator_filter("0x123...".parse().unwrap());
    /// ```
    pub fn with_liquidator_filter(mut self, liquidator: Address) -> Self {
        self.liquidator_address = Some(liquidator);
        self
    }
}

impl PriceSource for OdosPriceSource {
    fn router_address(&self) -> Address {
        self.router_address
    }

    fn event_topics(&self) -> Vec<B256> {
        match self.router_type {
            RouterType::V2 => {
                vec![SwapMultiV2::SIGNATURE_HASH, SwapV2::SIGNATURE_HASH]
            }
            RouterType::V3 => {
                vec![SwapMultiV3::SIGNATURE_HASH, SwapV3::SIGNATURE_HASH]
            }
            // LimitOrder is rejected in for_chain() and new() defaults to V2
            RouterType::LimitOrder => unreachable!("LimitOrder not supported for price extraction"),
        }
    }

    fn extract_swap_from_log(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
        if log.topics().is_empty() {
            return Ok(None);
        }

        let topic = log.topics()[0];

        match self.router_type {
            RouterType::V2 => {
                if topic == SwapMultiV2::SIGNATURE_HASH {
                    return self.extract_swap_multi_v2(log);
                }
                if topic == SwapV2::SIGNATURE_HASH {
                    return self.extract_swap_single_v2(log);
                }
            }
            RouterType::V3 => {
                if topic == SwapMultiV3::SIGNATURE_HASH {
                    return self.extract_swap_multi_v3(log);
                }
                if topic == SwapV3::SIGNATURE_HASH {
                    return self.extract_swap_single_v3(log);
                }
            }
            RouterType::LimitOrder => unreachable!("LimitOrder not supported for price extraction"),
        }

        Ok(None)
    }

    /// Determine if a swap should be included based on the liquidator filter
    ///
    /// # Arguments
    ///
    /// * `swap` - The swap data to check
    ///
    /// # Returns
    ///
    /// `true` if the swap should be included, `false` otherwise
    ///
    /// If the liquidator filter is set, only include swaps from that address. Otherwise accept all swaps.
    fn should_include_swap(&self, swap: &SwapData) -> bool {
        match self.liquidator_address {
            Some(liquidator) => swap.sender == Some(liquidator),
            None => true,
        }
    }
}

impl OdosPriceSource {
    // ===== V2 Event Extraction =====

    /// Extract swap data from a V2 SwapMulti event
    fn extract_swap_multi_v2(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
        let event = SwapMultiV2::decode_log(&log.clone().into())?;

        // Validate event data
        if event.tokensIn.is_empty() || event.tokensOut.is_empty() {
            return Err(PriceSourceError::empty_token_arrays());
        }

        if event.amountsIn.len() != event.tokensIn.len()
            || event.amountsOut.len() != event.tokensOut.len()
        {
            return Err(PriceSourceError::array_length_mismatch(
                event.tokensIn.len(),
                event.amountsIn.len(),
                event.tokensOut.len(),
                event.amountsOut.len(),
            ));
        }

        // Simple case: 1 input token + 1 output token
        if event.tokensIn.len() == 1 && event.tokensOut.len() == 1 {
            return Ok(Some(SwapData {
                token_in: event.tokensIn[0],
                token_in_amount: event.amountsIn[0],
                token_out: event.tokensOut[0],
                token_out_amount: event.amountsOut[0],
                sender: Some(event.sender),
            }));
        }

        // Skip complex multi-token swaps for now
        Ok(None)
    }

    /// Extract swap data from a V2 Swap event
    fn extract_swap_single_v2(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
        let event = SwapV2::decode_log(&log.clone().into())?;

        Ok(Some(SwapData {
            token_in: event.inputToken,
            token_in_amount: event.inputAmount,
            token_out: event.outputToken,
            token_out_amount: event.amountOut,
            sender: Some(event.sender),
        }))
    }

    // ===== V3 Event Extraction =====

    /// Extract swap data from a V3 SwapMulti event
    fn extract_swap_multi_v3(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
        let event = SwapMultiV3::decode_log(&log.clone().into())?;

        // Validate event data
        if event.tokensIn.is_empty() || event.tokensOut.is_empty() {
            return Err(PriceSourceError::empty_token_arrays());
        }

        if event.amountsIn.len() != event.tokensIn.len()
            || event.amountsOut.len() != event.tokensOut.len()
        {
            return Err(PriceSourceError::array_length_mismatch(
                event.tokensIn.len(),
                event.amountsIn.len(),
                event.tokensOut.len(),
                event.amountsOut.len(),
            ));
        }

        // Simple case: 1 input token + 1 output token
        if event.tokensIn.len() == 1 && event.tokensOut.len() == 1 {
            return Ok(Some(SwapData {
                token_in: event.tokensIn[0],
                token_in_amount: event.amountsIn[0],
                token_out: event.tokensOut[0],
                token_out_amount: event.amountsOut[0],
                sender: Some(event.sender),
            }));
        }

        // Skip complex multi-token swaps for now
        Ok(None)
    }

    /// Extract swap data from a V3 Swap event
    fn extract_swap_single_v3(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
        let event = SwapV3::decode_log(&log.clone().into())?;

        Ok(Some(SwapData {
            token_in: event.inputToken,
            token_in_amount: event.inputAmount,
            token_out: event.outputToken,
            token_out_amount: event.amountOut,
            sender: Some(event.sender),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_odos_price_source_creation() {
        let router: Address = "0xa669e7A0d4b3e4Fa48af2dE86BD4CD7126Be4e13"
            .parse()
            .unwrap();
        let price_source = OdosPriceSource::new(router);
        assert_eq!(price_source.router_address(), router);
    }

    #[test]
    fn test_for_chain_v2_arbitrum() {
        let source = OdosPriceSource::for_chain(NamedChain::Arbitrum, RouterType::V2)
            .expect("Arbitrum V2 should be supported");

        // Verify address matches known Arbitrum V2 router
        let expected: Address = "0xa669e7A0d4b3e4Fa48af2dE86BD4CD7126Be4e13"
            .parse()
            .unwrap();
        assert_eq!(source.router_address(), expected);
    }

    #[test]
    fn test_for_chain_v2_mainnet() {
        let source = OdosPriceSource::for_chain(NamedChain::Mainnet, RouterType::V2)
            .expect("Mainnet V2 should be supported");

        // Just verify it returns a valid address
        assert_ne!(source.router_address(), Address::ZERO);
    }

    #[test]
    fn test_for_chain_v3_mainnet() {
        let source = OdosPriceSource::for_chain(NamedChain::Mainnet, RouterType::V3)
            .expect("Mainnet V3 should be supported");

        assert_ne!(source.router_address(), Address::ZERO);
    }

    #[test]
    fn test_for_chain_unsupported() {
        // Use a chain that definitely doesn't have Odos deployed
        let result = OdosPriceSource::for_chain(NamedChain::Dev, RouterType::V2);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, OdosError::UnsupportedChain { .. }));
    }

    #[test]
    fn test_for_chain_non_swap_router_rejected() {
        // LimitOrder emits different events (LimitOrderFilled), not Swap/SwapMulti
        let result = OdosPriceSource::for_chain(NamedChain::Mainnet, RouterType::LimitOrder);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(
            err,
            OdosError::NonSwapRouterNotSupported { router_type: "LO" }
        ));
    }

    #[test]
    fn test_for_chain_with_liquidator_filter() {
        let liquidator: Address = "0x1234567890123456789012345678901234567890"
            .parse()
            .unwrap();

        let source = OdosPriceSource::for_chain(NamedChain::Arbitrum, RouterType::V2)
            .expect("Arbitrum V2 should be supported")
            .with_liquidator_filter(liquidator);

        // Verify filter is applied
        let swap = SwapData {
            token_in: Address::ZERO,
            token_in_amount: Default::default(),
            token_out: Address::ZERO,
            token_out_amount: Default::default(),
            sender: Some(liquidator),
        };
        assert!(source.should_include_swap(&swap));

        let other: Address = "0x9999999999999999999999999999999999999999"
            .parse()
            .unwrap();
        let swap_other = SwapData {
            sender: Some(other),
            ..swap
        };
        assert!(!source.should_include_swap(&swap_other));
    }

    #[test]
    fn test_liquidator_filter() {
        let router: Address = "0xa669e7A0d4b3e4Fa48af2dE86BD4CD7126Be4e13"
            .parse()
            .unwrap();
        let liquidator: Address = "0x1234567890123456789012345678901234567890"
            .parse()
            .unwrap();

        let price_source = OdosPriceSource::new(router).with_liquidator_filter(liquidator);

        // Test that swaps from liquidator are included
        let swap = SwapData {
            token_in: Address::ZERO,
            token_in_amount: Default::default(),
            token_out: Address::ZERO,
            token_out_amount: Default::default(),
            sender: Some(liquidator),
        };
        assert!(price_source.should_include_swap(&swap));

        // Test that swaps from other addresses are excluded
        let other_sender: Address = "0x9999999999999999999999999999999999999999"
            .parse()
            .unwrap();
        let swap_other = SwapData {
            sender: Some(other_sender),
            ..swap
        };
        assert!(!price_source.should_include_swap(&swap_other));
    }

    #[test]
    fn test_event_topics_v2() {
        let router: Address = "0xa669e7A0d4b3e4Fa48af2dE86BD4CD7126Be4e13"
            .parse()
            .unwrap();
        let price_source = OdosPriceSource::new(router);

        let topics = price_source.event_topics();
        assert_eq!(topics.len(), 2);
        assert_eq!(topics[0], SwapMultiV2::SIGNATURE_HASH);
        assert_eq!(topics[1], SwapV2::SIGNATURE_HASH);
    }

    #[test]
    fn test_event_topics_v3() {
        let source = OdosPriceSource::for_chain(NamedChain::Mainnet, RouterType::V3)
            .expect("Mainnet V3 should be supported");

        let topics = source.event_topics();
        assert_eq!(topics.len(), 2);
        assert_eq!(topics[0], SwapMultiV3::SIGNATURE_HASH);
        assert_eq!(topics[1], SwapV3::SIGNATURE_HASH);

        // V3 topics should differ from V2
        assert_ne!(SwapV2::SIGNATURE_HASH, SwapV3::SIGNATURE_HASH);
        assert_ne!(SwapMultiV2::SIGNATURE_HASH, SwapMultiV3::SIGNATURE_HASH);
    }

    #[test]
    fn test_all_routers_for_chain_mainnet() {
        let sources = OdosPriceSource::all_routers_for_chain(NamedChain::Mainnet);

        // Mainnet should have multiple routers (at least V2 and V3)
        assert!(sources.len() >= 2, "Expected at least 2 routers on Mainnet");

        // All addresses should be unique
        let mut addresses: Vec<_> = sources.iter().map(|s| s.router_address()).collect();
        addresses.sort();
        addresses.dedup();
        assert_eq!(
            addresses.len(),
            sources.len(),
            "Router addresses should be unique"
        );
    }

    #[test]
    fn test_all_routers_for_chain_unsupported() {
        let sources = OdosPriceSource::all_routers_for_chain(NamedChain::Dev);

        // Unsupported chain should return empty vec
        assert!(sources.is_empty());
    }
}
