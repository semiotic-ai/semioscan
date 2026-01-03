// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Event processing for ERC-20 transfers and approvals.
//!
//! This module handles:
//! - Transfer and Approval event definitions
//! - Transfer amount extraction and accumulation
//! - Token discovery via event scanning
//! - Semantic filter builders for type-safe event filtering
//! - Generic event scanning with chunking and rate limiting
//! - Real-time event streaming via WebSocket subscriptions

pub mod definitions;
pub mod discovery;
pub mod filter;
pub mod realtime;
pub mod scanner;
pub mod transfers;

// Re-export public types
pub use definitions::{Approval, Transfer};
pub use discovery::{extract_transferred_to_tokens, extract_transferred_to_tokens_with_config};
pub use transfers::{AmountCalculator, AmountResult};

// Public API exports for external consumers (not used internally, which is expected for a library)
// These are tested in filter::tests::integration module
#[allow(unused_imports)]
pub use filter::{transfer_filter_from_to, transfer_filter_to_recipient, TransferFilterBuilder};
#[allow(unused_imports)]
pub use realtime::RealtimeEventScanner;
#[allow(unused_imports)]
pub use scanner::EventScanner;
