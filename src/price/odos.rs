//! Odos DEX price source implementation
//!
//! This module provides a reference implementation of [`PriceSource`](super::PriceSource)
//! for the Odos DEX aggregator. It demonstrates how to extract swap data from Odos router events.
//!
//! # Features
//!
//! This module is only available when the `odos-example` feature is enabled:
//!
//! ```toml
//! [dependencies]
//! semioscan = { version = "0.1", features = ["odos-example"] }
//! ```
//!
//! # Supported Events
//!
//! - **Swap** - Single-token swap event (V2 routers)
//! - **SwapMulti** - Multi-token swap event (V2 routers)
//!
//! # Example Usage
//!
//! ```rust,ignore
//! use semioscan::price::odos::OdosPriceSource;
//! use alloy_primitives::Address;
//!
//! // Create price source for Odos V2 router on Arbitrum
//! let router_address = "0xa669e7A0d4b3e4Fa48af2dE86BD4CD7126Be4e13".parse().unwrap();
//! let price_source = OdosPriceSource::new(router_address)
//!     .with_liquidator_filter("0x123...".parse().unwrap());
//!
//! // Use with PriceCalculator
//! let calculator = PriceCalculator::with_price_source(
//!     provider,
//!     Box::new(price_source),
//! );
//! ```

use super::{PriceSource, PriceSourceError, SwapData};
use alloy_primitives::{Address, B256};
use alloy_rpc_types::Log;
use alloy_sol_types::SolEvent;
use odos_sdk::OdosV2Router::{Swap, SwapMulti};

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
pub struct OdosPriceSource {
    router_address: Address,
    liquidator_address: Option<Address>,
}

impl OdosPriceSource {
    /// Create a new Odos price source for the given router address
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
            liquidator_address: None,
        }
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
        vec![SwapMulti::SIGNATURE_HASH, Swap::SIGNATURE_HASH]
    }

    fn extract_swap_from_log(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
        if log.topics().is_empty() {
            return Ok(None);
        }

        let topic = log.topics()[0];

        // Try SwapMulti event
        if topic == SwapMulti::SIGNATURE_HASH {
            return self.extract_swap_multi(log);
        }

        // Try Swap (single) event
        if topic == Swap::SIGNATURE_HASH {
            return self.extract_swap_single(log);
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
    /// Extract swap data from a SwapMulti event
    ///
    /// SwapMulti events can have multiple input and output tokens. This implementation
    /// currently handles the simple case of 1 input + 1 output token.
    ///
    /// Future enhancement: Handle complex multi-token swaps by returning Vec<SwapData>
    fn extract_swap_multi(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
        let event = SwapMulti::decode_log(&log.clone().into())
            .map_err(|e| PriceSourceError::DecodeError(e.to_string()))?;

        // Validate event data
        if event.tokensIn.is_empty() || event.tokensOut.is_empty() {
            return Err(PriceSourceError::InvalidSwapData(
                "SwapMulti event has empty token arrays".to_string(),
            ));
        }

        if event.amountsIn.len() != event.tokensIn.len()
            || event.amountsOut.len() != event.tokensOut.len()
        {
            return Err(PriceSourceError::InvalidSwapData(
                "Token and amount array lengths don't match".to_string(),
            ));
        }

        // Simple case: 1 input token + 1 output token
        // This is the most common pattern for liquidations
        if event.tokensIn.len() == 1 && event.tokensOut.len() == 1 {
            return Ok(Some(SwapData {
                token_in: event.tokensIn[0],
                token_in_amount: event.amountsIn[0],
                token_out: event.tokensOut[0],
                token_out_amount: event.amountsOut[0],
                sender: Some(event.sender),
            }));
        }

        // For now, skip complex multi-token swaps
        // Future: iterate through all token pairs or use more sophisticated matching
        Ok(None)
    }

    /// Extract swap data from a single Swap event
    fn extract_swap_single(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
        let event = Swap::decode_log(&log.clone().into())
            .map_err(|e| PriceSourceError::DecodeError(e.to_string()))?;

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
    fn test_event_topics() {
        let router: Address = "0xa669e7A0d4b3e4Fa48af2dE86BD4CD7126Be4e13"
            .parse()
            .unwrap();
        let price_source = OdosPriceSource::new(router);

        let topics = price_source.event_topics();
        assert_eq!(topics.len(), 2);
        assert_eq!(topics[0], SwapMulti::SIGNATURE_HASH);
        assert_eq!(topics[1], Swap::SIGNATURE_HASH);
    }
}
