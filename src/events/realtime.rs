// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Real-time event streaming via WebSocket subscriptions.
//!
//! This module provides WebSocket-based event streaming as a complement to the
//! polling-based [`EventScanner`](super::scanner::EventScanner). Use this for
//! lower-latency, real-time analytics scenarios.
//!
//! # When to Use
//!
//! - **`EventScanner`**: Historical data, batch processing, guaranteed delivery
//! - **`RealtimeEventScanner`**: Live monitoring, real-time alerts, low latency
//!
//! # Provider Setup
//!
//! The `RealtimeEventScanner` requires a WebSocket-connected provider:
//!
//! ```rust,ignore
//! use alloy_provider::{ProviderBuilder, WsConnect};
//!
//! // Create WebSocket provider
//! let ws = WsConnect::new("wss://eth-mainnet.example.com/ws");
//! let provider = ProviderBuilder::new().connect_ws(ws).await?;
//!
//! // Use with RealtimeEventScanner
//! let scanner = RealtimeEventScanner::new(provider);
//! ```
//!
//! # Examples
//!
//! ## Subscribe to Transfer Events
//!
//! ```rust,ignore
//! use semioscan::events::realtime::RealtimeEventScanner;
//! use semioscan::events::filter::TransferFilterBuilder;
//! use futures::StreamExt;
//!
//! let scanner = RealtimeEventScanner::new(ws_provider);
//!
//! let filter = TransferFilterBuilder::new()
//!     .to_recipient(router_address)
//!     .build();
//!
//! let mut stream = scanner.subscribe_logs(filter).await?;
//!
//! while let Some(log) = stream.next().await {
//!     println!("Transfer: {:?}", log);
//! }
//! ```
//!
//! ## Subscribe to New Block Headers
//!
//! ```rust,ignore
//! use semioscan::events::realtime::RealtimeEventScanner;
//! use futures::StreamExt;
//!
//! let scanner = RealtimeEventScanner::new(ws_provider);
//! let mut stream = scanner.subscribe_blocks().await?;
//!
//! while let Some(header) = stream.next().await {
//!     println!("New block: {} ({})", header.number, header.hash);
//! }
//! ```

use alloy_primitives::BlockNumber;
use alloy_provider::Provider;
use alloy_rpc_types::{BlockNumberOrTag, Filter, Header, Log};
use futures::stream::{Stream, StreamExt};
use std::pin::Pin;
use tracing::{debug, info};

use crate::errors::{EventProcessingError, RpcError};

/// Real-time event scanner using WebSocket subscriptions.
///
/// This scanner provides real-time event streaming via WebSocket connections,
/// complementing the polling-based [`EventScanner`](super::scanner::EventScanner).
///
/// # Type Parameters
///
/// * `P` - Provider type that supports pub/sub subscriptions
///
/// # Thread Safety
///
/// The scanner itself is `Send + Sync` when the provider is. The returned
/// streams can be consumed from any async task.
///
/// # Examples
///
/// ```rust,ignore
/// use semioscan::events::realtime::RealtimeEventScanner;
/// use alloy_provider::{ProviderBuilder, WsConnect};
///
/// // WebSocket provider is required for subscriptions
/// let ws = WsConnect::new("wss://eth.example.com/ws");
/// let provider = ProviderBuilder::new().connect_ws(ws).await?;
///
/// let scanner = RealtimeEventScanner::new(provider);
/// ```
// This is a public API for library consumers - not used internally
#[allow(dead_code)]
pub struct RealtimeEventScanner<P> {
    provider: P,
}

#[allow(dead_code)]
impl<P> RealtimeEventScanner<P>
where
    P: Provider,
{
    /// Create a new real-time event scanner.
    ///
    /// # Arguments
    ///
    /// * `provider` - WebSocket-connected provider supporting pub/sub
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use semioscan::events::realtime::RealtimeEventScanner;
    /// use alloy_provider::{ProviderBuilder, WsConnect};
    ///
    /// let ws = WsConnect::new("wss://eth.example.com/ws");
    /// let provider = ProviderBuilder::new().connect_ws(ws).await?;
    /// let scanner = RealtimeEventScanner::new(provider);
    /// ```
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    /// Subscribe to new block headers.
    ///
    /// Returns a stream that yields each new block header as it's produced.
    /// Useful for tracking chain progress or triggering block-based logic.
    ///
    /// # Returns
    ///
    /// A boxed stream of block headers. The stream continues until the
    /// WebSocket connection is closed or an error occurs.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be established.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use semioscan::events::realtime::RealtimeEventScanner;
    /// use futures::StreamExt;
    ///
    /// let scanner = RealtimeEventScanner::new(ws_provider);
    /// let mut stream = scanner.subscribe_blocks().await?;
    ///
    /// while let Some(header) = stream.next().await {
    ///     println!("Block #{}: {}", header.number, header.hash);
    /// }
    /// ```
    pub async fn subscribe_blocks(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = Header> + Send + '_>>, EventProcessingError> {
        info!("Subscribing to new blocks");

        let subscription =
            self.provider.subscribe_blocks().await.map_err(|e| {
                EventProcessingError::Rpc(RpcError::subscription_failed("blocks", e))
            })?;

        let stream = subscription.into_stream();

        debug!("Block subscription established");

        Ok(Box::pin(stream))
    }

    /// Subscribe to logs matching a filter.
    ///
    /// Returns a stream that yields each log matching the filter as events
    /// are emitted on-chain. This is the real-time equivalent of
    /// [`EventScanner::scan`](super::scanner::EventScanner::scan).
    ///
    /// # Arguments
    ///
    /// * `filter` - Log filter specifying which events to subscribe to.
    ///   Use [`TransferFilterBuilder`](super::filter::TransferFilterBuilder)
    ///   for type-safe filter construction.
    ///
    /// # Returns
    ///
    /// A boxed stream of logs. The stream continues until the WebSocket
    /// connection is closed or an error occurs.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be established.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use semioscan::events::realtime::RealtimeEventScanner;
    /// use semioscan::events::filter::TransferFilterBuilder;
    /// use futures::StreamExt;
    ///
    /// let scanner = RealtimeEventScanner::new(ws_provider);
    ///
    /// // Subscribe to transfers to a specific address
    /// let filter = TransferFilterBuilder::new()
    ///     .to_recipient(router_address)
    ///     .from_block_tag(BlockNumberOrTag::Latest)
    ///     .build();
    ///
    /// let mut stream = scanner.subscribe_logs(filter).await?;
    ///
    /// while let Some(log) = stream.next().await {
    ///     // Process each transfer event as it occurs
    ///     process_transfer(&log)?;
    /// }
    /// ```
    pub async fn subscribe_logs(
        &self,
        filter: Filter,
    ) -> Result<Pin<Box<dyn Stream<Item = Log> + Send + '_>>, EventProcessingError> {
        info!(
            address = ?filter.address,
            topics = ?filter.topics,
            "Subscribing to logs"
        );

        let subscription = self
            .provider
            .subscribe_logs(&filter)
            .await
            .map_err(|e| EventProcessingError::Rpc(RpcError::subscription_failed("logs", e)))?;

        let stream = subscription.into_stream();

        debug!("Log subscription established");

        Ok(Box::pin(stream))
    }

    /// Subscribe to logs for a specific block range, then continue with live updates.
    ///
    /// This method first fetches historical logs for the specified range,
    /// then seamlessly transitions to live streaming. Useful for catching up
    /// on missed events before going real-time.
    ///
    /// # Arguments
    ///
    /// * `filter_template` - Base filter without block range (will be modified)
    /// * `from_block` - Start of historical range to catch up
    ///
    /// # Returns
    ///
    /// A stream that first yields historical logs, then continues with live logs.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use semioscan::events::realtime::RealtimeEventScanner;
    /// use futures::StreamExt;
    ///
    /// let scanner = RealtimeEventScanner::new(ws_provider);
    ///
    /// // Catch up from block 1000, then go live
    /// let filter = TransferFilterBuilder::new()
    ///     .to_recipient(router_address)
    ///     .build();
    ///
    /// let mut stream = scanner.subscribe_logs_with_catchup(filter, 1000).await?;
    ///
    /// while let Some(log) = stream.next().await {
    ///     // Receives historical logs first, then live updates
    ///     handle_log(log)?;
    /// }
    /// ```
    pub async fn subscribe_logs_with_catchup(
        &self,
        filter_template: Filter,
        from_block: BlockNumber,
    ) -> Result<Pin<Box<dyn Stream<Item = Log> + Send + '_>>, EventProcessingError> {
        info!(
            from_block = from_block,
            address = ?filter_template.address,
            "Subscribing to logs with historical catchup"
        );

        // First, fetch historical logs
        let historical_filter = filter_template
            .clone()
            .from_block(from_block)
            .to_block(BlockNumberOrTag::Latest);

        let historical_logs = self
            .provider
            .get_logs(&historical_filter)
            .await
            .map_err(|e| {
                EventProcessingError::Rpc(RpcError::get_logs_failed("historical catchup", e))
            })?;

        info!(
            historical_count = historical_logs.len(),
            "Fetched historical logs, switching to live subscription"
        );

        // Then subscribe to live logs
        let live_filter = filter_template.from_block(BlockNumberOrTag::Latest);

        let subscription = self
            .provider
            .subscribe_logs(&live_filter)
            .await
            .map_err(|e| EventProcessingError::Rpc(RpcError::subscription_failed("logs", e)))?;

        // Combine: emit historical first, then live
        let historical_stream = futures::stream::iter(historical_logs);
        let live_stream = subscription.into_stream();

        let combined = historical_stream.chain(live_stream);

        debug!("Log subscription with catchup established");

        Ok(Box::pin(combined))
    }

    /// Get a reference to the underlying provider.
    ///
    /// Useful for making additional RPC calls while maintaining the scanner.
    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Consume the scanner and return the underlying provider.
    pub fn into_provider(self) -> P {
        self.provider
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Integration tests for WebSocket subscriptions require a live
    // WebSocket endpoint. Unit tests focus on construction and type safety.

    #[test]
    fn test_scanner_construction_is_generic() {
        // This test verifies the type constraints compile correctly.
        // Actual WebSocket testing requires integration tests with a live node.
        fn _accepts_any_provider<P: Provider>(_scanner: RealtimeEventScanner<P>) {}
    }
}
