//! Semantic filter builders for blockchain events
//!
//! This module provides type-safe, self-documenting filter builders that replace
//! cryptic topic1/topic2 patterns with clear semantic methods.
//!
//! # Examples
//!
//! ```rust,ignore
//! use semioscan::events::filter::TransferFilterBuilder;
//! use alloy_primitives::address;
//!
//! // BEFORE: Cryptic
//! let filter = Filter::new()
//!     .topic1(from)  // What's topic1?
//!     .topic2(to);   // What's topic2?
//!
//! // AFTER: Self-documenting, idiomatic Rust
//! let filter = TransferFilterBuilder::new()
//!     .with_sender(from)
//!     .with_recipient(to)
//!     .with_token(token)
//!     .build();
//! ```
//!
//! # Usage with EventScanner
//!
//! When using filters with [`EventScanner`](crate::events::scanner::EventScanner),
//! the scanner manages block ranges and chunking. Build filters without block ranges:
//!
//! ```rust,ignore
//! use semioscan::events::{EventScanner, TransferFilterBuilder};
//!
//! let scanner = EventScanner::new(provider, config);
//! let filter = TransferFilterBuilder::new()
//!     .with_recipient(router)
//!     .build();
//!
//! // Scanner handles block range chunking
//! let logs = scanner.scan(chain, filter, start_block, end_block).await?;
//! ```

use alloy_primitives::{keccak256, Address, BlockNumber, U256};
use alloy_rpc_types::Filter;

/// Builder for ERC-20 Transfer event filters with semantic methods
///
/// Provides a type-safe, self-documenting API for constructing Transfer event filters.
/// Handles the complexities of topic encoding internally, exposing only domain concepts.
///
/// # Design Rationale
///
/// ERC-20 Transfer events have this structure:
/// ```solidity
/// event Transfer(address indexed from, address indexed to, uint256 value);
/// ```
///
/// Indexed parameters become topics in the log:
/// - topic0: Event signature hash (Transfer event signature)
/// - topic1: `from` address (sender)
/// - topic2: `to` address (recipient)
///
/// The builder hides this implementation detail, exposing:
/// - `from_sender(addr)` instead of `topic1(addr)`
/// - `to_recipient(addr)` instead of `topic2(addr)`
///
/// # Examples
///
/// ```rust,ignore
/// use semioscan::events::filter::TransferFilterBuilder;
/// use alloy_primitives::address;
///
/// // Find all transfers from a specific address
/// let filter = TransferFilterBuilder::new()
///     .from_sender(sender_addr)
///     .build();
///
/// // Find all transfers to a router contract
/// let filter = TransferFilterBuilder::new()
///     .to_recipient(router_addr)
///     .build();
///
/// // Find transfers of a specific token between two addresses
/// let filter = TransferFilterBuilder::new()
///     .for_token(usdc_addr)
///     .from_sender(router_addr)
///     .to_recipient(liquidator_addr)
///     .in_block_range(start_block, end_block)
///     .build();
/// ```
#[derive(Debug, Clone, Default)]
pub struct TransferFilterBuilder {
    from_block: Option<BlockNumber>,
    to_block: Option<BlockNumber>,
    token_address: Option<Address>,
    from_address: Option<Address>,
    to_address: Option<Address>,
}

impl TransferFilterBuilder {
    /// Create a new Transfer event filter builder
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let builder = TransferFilterBuilder::new();
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter for transfers of a specific token
    ///
    /// # Arguments
    ///
    /// * `token` - The ERC-20 token contract address
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use alloy_primitives::address;
    ///
    /// let usdc = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");
    /// let filter = TransferFilterBuilder::new()
    ///     .with_token(usdc)
    ///     .build();
    /// ```
    pub fn with_token(mut self, token: Address) -> Self {
        self.token_address = Some(token);
        self
    }

    /// Filter for transfers from a specific sender address
    ///
    /// Semantically replaces `.topic1(addr)` with a clear domain concept.
    ///
    /// # Arguments
    ///
    /// * `sender` - The address sending tokens (topic1 in ERC-20 Transfer)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Find all transfers FROM a router
    /// let filter = TransferFilterBuilder::new()
    ///     .with_sender(router_addr)
    ///     .build();
    /// ```
    pub fn with_sender(mut self, sender: Address) -> Self {
        self.from_address = Some(sender);
        self
    }

    /// Filter for transfers to a specific recipient address
    ///
    /// Semantically replaces `.topic2(addr)` with a clear domain concept.
    ///
    /// # Arguments
    ///
    /// * `recipient` - The address receiving tokens (topic2 in ERC-20 Transfer)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Find all transfers TO a router (token discovery)
    /// let filter = TransferFilterBuilder::new()
    ///     .with_recipient(router_addr)
    ///     .build();
    /// ```
    pub fn with_recipient(mut self, recipient: Address) -> Self {
        self.to_address = Some(recipient);
        self
    }

    /// Build the final Alloy Filter
    ///
    /// Constructs an `alloy_rpc_types::Filter` with Transfer event signature
    /// and the configured parameters.
    ///
    /// # Returns
    ///
    /// A configured `Filter` ready to use with `provider.get_logs()`
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let filter = TransferFilterBuilder::new()
    ///     .for_token(usdc)
    ///     .to_recipient(router)
    ///     .build();
    ///
    /// let logs = provider.get_logs(&filter).await?;
    /// ```
    pub fn build(self) -> Filter {
        let mut filter =
            Filter::new().event_signature(*keccak256(b"Transfer(address,address,uint256)"));

        // Add block range if specified
        if let Some(from) = self.from_block {
            filter = filter.from_block(from);
        }
        if let Some(to) = self.to_block {
            filter = filter.to_block(to);
        }

        // Add token address if specified
        if let Some(token) = self.token_address {
            filter = filter.address(token);
        }

        // Add from address (topic1) if specified
        if let Some(from) = self.from_address {
            filter = filter.topic1(from);
        }

        // Add to address (topic2) if specified
        // Handle the obscure U256::from_be_bytes conversion internally
        if let Some(to) = self.to_address {
            filter = filter.topic2(U256::from_be_bytes(to.into_word().into()));
        }

        filter
    }
}

/// Convenience function to create a Transfer filter for token discovery
///
/// Creates a filter that finds all Transfer events where the recipient is the specified address.
/// This is the most common pattern for discovering which tokens have been transferred to a contract.
///
/// **Note**: This function is intended for use with [`EventScanner`](crate::events::scanner::EventScanner),
/// which manages block range chunking. The filter returned does NOT include block ranges.
///
/// # Arguments
///
/// * `recipient` - The address to find transfers to
///
/// # Returns
///
/// A configured `Filter` ready to use with `EventScanner::scan()`
///
/// # Examples
///
/// ```rust,ignore
/// use semioscan::events::filter::transfer_filter_to_recipient;
/// use semioscan::events::EventScanner;
/// use alloy_primitives::address;
///
/// let router = address!("0x1234...");
/// let filter = transfer_filter_to_recipient(router);
///
/// // Use with scanner (recommended)
/// let scanner = EventScanner::new(provider, config);
/// let logs = scanner.scan(chain, filter, 1_000_000, 1_010_000).await?;
/// ```
#[allow(dead_code)] // Public API for external consumers, tested in integration tests
pub fn transfer_filter_to_recipient(recipient: Address) -> Filter {
    TransferFilterBuilder::new()
        .with_recipient(recipient)
        .build()
}

/// Convenience function to create a Transfer filter for specific token transfers
///
/// Creates a filter for Transfer events of a specific token between two addresses.
///
/// **Note**: This function is intended for use with [`EventScanner`](crate::events::scanner::EventScanner),
/// which manages block range chunking. The filter returned does NOT include block ranges.
///
/// # Arguments
///
/// * `token` - The ERC-20 token contract address
/// * `from` - The sender address
/// * `to` - The recipient address
///
/// # Returns
///
/// A configured `Filter` ready to use with `EventScanner::scan()`
///
/// # Examples
///
/// ```rust,ignore
/// use semioscan::events::filter::transfer_filter_from_to;
/// use semioscan::events::EventScanner;
/// use alloy_primitives::address;
///
/// let usdc = address!("0xA0b8...");
/// let filter = transfer_filter_from_to(usdc, router, liquidator);
///
/// // Use with scanner (recommended)
/// let scanner = EventScanner::new(provider, config);
/// let logs = scanner.scan(chain, filter, 1_000_000, 1_010_000).await?;
/// ```
#[allow(dead_code)] // Public API for external consumers, tested in integration tests
pub fn transfer_filter_from_to(token: Address, from: Address, to: Address) -> Filter {
    TransferFilterBuilder::new()
        .with_token(token)
        .with_sender(from)
        .with_recipient(to)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    #[test]
    fn test_builder_with_all_fields() {
        let token = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");
        let from = address!("1111111111111111111111111111111111111111");
        let to = address!("2222222222222222222222222222222222222222");

        let filter = TransferFilterBuilder::new()
            .with_token(token)
            .with_sender(from)
            .with_recipient(to)
            .build();

        // Filter should be valid even without block range (scanner adds it)
        assert_eq!(filter.get_from_block(), None);
        assert_eq!(filter.get_to_block(), None);
    }

    #[test]
    fn test_builder_with_recipient_only() {
        let recipient = address!("1111111111111111111111111111111111111111");

        let filter = TransferFilterBuilder::new()
            .with_recipient(recipient)
            .build();

        // No block range - scanner will add it
        assert_eq!(filter.get_from_block(), None);
        assert_eq!(filter.get_to_block(), None);
    }

    #[test]
    fn test_transfer_filter_to_recipient_convenience() {
        let router = address!("1111111111111111111111111111111111111111");
        let filter = transfer_filter_to_recipient(router);

        // No block range - for use with EventScanner
        assert_eq!(filter.get_from_block(), None);
        assert_eq!(filter.get_to_block(), None);
    }

    #[test]
    fn test_transfer_filter_from_to_convenience() {
        let token = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");
        let from = address!("1111111111111111111111111111111111111111");
        let to = address!("2222222222222222222222222222222222222222");

        let filter = transfer_filter_from_to(token, from, to);

        // No block range - for use with EventScanner
        assert_eq!(filter.get_from_block(), None);
        assert_eq!(filter.get_to_block(), None);
    }

    #[test]
    fn test_default_builder() {
        let filter = TransferFilterBuilder::new().build();

        // Should create a valid filter even with no parameters
        assert_eq!(filter.get_from_block(), None);
        assert_eq!(filter.get_to_block(), None);
    }

    #[test]
    fn test_builder_partial_fields() {
        // Test that we can build with some fields set
        let token = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");
        let filter = TransferFilterBuilder::new().with_token(token).build();

        assert_eq!(filter.get_from_block(), None);
        assert_eq!(filter.get_to_block(), None);
    }

    #[test]
    fn test_builder_method_chaining() {
        // Ensure builder pattern works fluently
        let filter = TransferFilterBuilder::new()
            .with_token(Address::ZERO)
            .with_sender(Address::ZERO)
            .with_recipient(Address::ZERO)
            .build();

        // No block range - scanner handles this
        assert_eq!(filter.get_from_block(), None);
        assert_eq!(filter.get_to_block(), None);
    }

    // Integration tests for public API usage
    mod integration {
        use super::*;

        #[test]
        fn test_public_api_token_discovery_pattern() {
            // This tests the most common usage pattern: discovering tokens sent to a router
            let router = address!("0x1234567890abcdef1234567890abcdef12345678");

            // Method 1: Using builder directly
            let filter1 = TransferFilterBuilder::new().with_recipient(router).build();

            // Method 2: Using convenience function
            let filter2 = transfer_filter_to_recipient(router);

            // Both methods should produce equivalent filters
            assert_eq!(filter1.get_from_block(), filter2.get_from_block());
            assert_eq!(filter1.get_to_block(), filter2.get_to_block());
        }

        #[test]
        fn test_public_api_specific_transfer_pattern() {
            // This tests filtering for specific token transfers between addresses
            let usdc = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");
            let router = address!("1111111111111111111111111111111111111111");
            let liquidator = address!("2222222222222222222222222222222222222222");

            // Method 1: Using builder directly
            let filter1 = TransferFilterBuilder::new()
                .with_token(usdc)
                .with_sender(router)
                .with_recipient(liquidator)
                .build();

            // Method 2: Using convenience function
            let filter2 = transfer_filter_from_to(usdc, router, liquidator);

            // Both methods should produce equivalent filters
            assert_eq!(filter1.get_from_block(), filter2.get_from_block());
            assert_eq!(filter1.get_to_block(), filter2.get_to_block());
        }

        #[test]
        fn test_builder_idiomatic_naming() {
            // Verify that idiomatic with_* methods work as expected
            let token = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");
            let from = address!("1111111111111111111111111111111111111111");
            let to = address!("2222222222222222222222222222222222222222");

            // Should compile and chain fluently
            let _filter = TransferFilterBuilder::new()
                .with_token(token)
                .with_sender(from)
                .with_recipient(to)
                .build();

            // The fact that this compiles and doesn't need #[allow] attributes
            // proves we're following Rust conventions
        }

        #[test]
        fn test_partial_filter_construction() {
            // Users should be able to build filters with just some fields
            let router = address!("1111111111111111111111111111111111111111");

            // Just recipient (token discovery)
            let _filter1 = TransferFilterBuilder::new().with_recipient(router).build();

            // Just sender
            let _filter2 = TransferFilterBuilder::new().with_sender(router).build();

            // Just token
            let token = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");
            let _filter3 = TransferFilterBuilder::new().with_token(token).build();

            // All combinations should be valid
        }

        #[test]
        fn test_convenience_functions_no_block_ranges() {
            // Convenience functions should NOT include block ranges
            // (scanner manages those)
            let router = address!("1111111111111111111111111111111111111111");
            let token = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");

            let filter1 = transfer_filter_to_recipient(router);
            assert_eq!(filter1.get_from_block(), None);
            assert_eq!(filter1.get_to_block(), None);

            let filter2 = transfer_filter_from_to(token, router, router);
            assert_eq!(filter2.get_from_block(), None);
            assert_eq!(filter2.get_to_block(), None);
        }
    }
}
