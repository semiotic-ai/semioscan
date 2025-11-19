//! OpenTelemetry span creation helpers for semioscan operations.
//!
//! This module provides span creation functions following an orthogonal design pattern
//! where telemetry concerns are separated from business logic. Instead of using
//! `#[instrument]` attributes directly on functions, each instrumented operation has
//! a corresponding span helper function in this module.
//!
//! Usage pattern:
//! ```rust,ignore
//! pub async fn my_operation(&self, param: Type) -> Result<T> {
//!     let span = spans::my_operation(param_value);
//!     let _guard = span.enter();
//!     // Business logic here
//! }
//! ```

use alloy_chains::NamedChain;
use alloy_primitives::{Address, BlockNumber, TxHash};
use chrono::NaiveDate;
use tracing::{Level, Span};

/// Create span for processing a single log entry for combined data extraction.
///
/// Parent: process_block_range_for_combined_data span
/// Children: RPC calls for transaction and receipt retrieval
#[inline]
pub(crate) fn process_log_for_combined_data(tx_hash: TxHash) -> Span {
    tracing::trace_span!("semioscan.process_log_for_combined_data", tx_hash = %tx_hash,)
}

/// Create span for processing a block range to collect combined transfer and gas data.
///
/// Parent: calculate_combined_data_with_adapter span
/// Children: process_log_for_combined_data spans (one per log)
#[inline]
pub(crate) fn process_block_range_for_combined_data(
    chain: NamedChain,
    from_address: Address,
    to_address: Address,
    token_address: Address,
    from_block: BlockNumber,
    to_block: BlockNumber,
) -> Span {
    tracing::debug_span!(
        "semioscan.process_block_range_for_combined_data",
        chain_id = %chain,
        from_address = %from_address,
        to_address = %to_address,
        token_address = %token_address,
        from_block = from_block,
        to_block = to_block,
    )
}

/// Create span for calculating combined transfer amount and gas cost data.
///
/// This is the main public API entry point for combined data retrieval.
///
/// Parent: None (root span for this operation)
/// Children: process_block_range_for_combined_data span
#[inline]
pub(crate) fn calculate_combined_data_with_adapter(
    chain: NamedChain,
    from_address: Address,
    to_address: Address,
    token_address: Address,
    from_block: BlockNumber,
    to_block: BlockNumber,
) -> Span {
    tracing::span!(
        Level::INFO,
        "semioscan.calculate_combined_data_with_adapter",
        chain_id = %chain,
        from_address = %from_address,
        to_address = %to_address,
        token_address = %token_address,
        from_block = from_block,
        to_block = to_block,
    )
}

/// Create span for processing a transfer event log to extract gas information.
///
/// Parent: Gas calculator operation span
/// Children: RPC calls for transaction and receipt retrieval
#[inline]
pub(crate) fn process_event_log(tx_hash: TxHash) -> Span {
    tracing::span!(
        Level::INFO,
        "semioscan.process_event_log",
        tx_hash = %tx_hash,
    )
}

/// Create span for fetching block timestamp.
///
/// Parent: Block window calculation span
/// Children: RPC call to get block
#[inline]
pub(crate) fn get_block_timestamp(block_number: BlockNumber) -> Span {
    tracing::debug_span!("semioscan.get_block_timestamp", block_number = block_number,)
}

/// Create span for finding first block at or after a target timestamp.
///
/// Parent: get_daily_window span
/// Children: get_block_timestamp spans (during binary search)
#[inline]
pub(crate) fn find_first_block_at_or_after(target_ts: u64, latest_block: BlockNumber) -> Span {
    tracing::debug_span!(
        "semioscan.find_first_block_at_or_after",
        target_ts = target_ts,
        latest_block = latest_block,
    )
}

/// Create span for finding last block at or before a target timestamp.
///
/// Parent: get_daily_window span
/// Children: get_block_timestamp spans (during binary search)
#[inline]
pub(crate) fn find_last_block_at_or_before(target_ts: u64, latest_block: BlockNumber) -> Span {
    tracing::debug_span!(
        "semioscan.find_last_block_at_or_before",
        target_ts = target_ts,
        latest_block = latest_block,
    )
}

/// Create span for calculating daily block window for a specific date.
///
/// This is the main public API for block window calculations.
///
/// Parent: None (root span for this operation)
/// Children: find_first_block_at_or_after, find_last_block_at_or_before spans
#[inline]
pub(crate) fn get_daily_window(chain: NamedChain, date: NaiveDate) -> Span {
    tracing::info_span!(
        "semioscan.get_daily_window",
        chain_id = %chain,
        date = %date,
    )
}

/// Create span for processing logs in a block range for gas calculation.
///
/// Parent: calculate_gas_cost_with_adapter span
/// Children: RPC calls for fetching logs and processing individual log entries
#[inline]
pub(crate) fn process_logs_in_range(
    event_type: crate::EventType,
    chain: NamedChain,
    topic1: Address,
    topic2: Address,
    token: Address,
    from_block: BlockNumber,
    to_block: BlockNumber,
) -> Span {
    tracing::debug_span!(
        "semioscan.process_logs_in_range",
        event_type = event_type.name(),
        chain_id = %chain,
        topic1 = %topic1,
        topic2 = %topic2,
        token = %token,
        from_block = from_block,
        to_block = to_block,
        block_count = to_block.saturating_sub(from_block) + 1,
    )
}

/// Create span for gas cost calculation with caching and gap detection.
///
/// This is the main entry point for gas cost calculations.
///
/// Parent: None (root span for this operation)
/// Children: process_logs_in_range spans (one per gap)
#[inline]
pub(crate) fn calculate_gas_cost_with_adapter(
    event_type: crate::EventType,
    chain: NamedChain,
    topic1: Address,
    topic2: Address,
    start_block: BlockNumber,
    end_block: BlockNumber,
) -> Span {
    tracing::info_span!(
        "semioscan.calculate_gas_cost_with_adapter",
        event_type = event_type.name(),
        chain_id = %chain,
        topic1 = %topic1,
        topic2 = %topic2,
        start_block = start_block,
        end_block = end_block,
        block_count = end_block.saturating_sub(start_block) + 1,
    )
}
