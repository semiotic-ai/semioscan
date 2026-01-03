// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Transport layer utilities for Alloy providers.
//!
//! This module provides Tower-based middleware layers for customizing
//! the RPC transport behavior of Alloy providers.
//!
//! # Rate Limiting
//!
//! The [`RateLimitLayer`] provides configurable rate limiting for RPC requests,
//! helping to stay within API rate limits for various RPC providers.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use semioscan::transport::RateLimitLayer;
//! use alloy_rpc_client::ClientBuilder;
//! use alloy_provider::ProviderBuilder;
//! use std::time::Duration;
//!
//! // Create a client with rate limiting (10 requests per second)
//! let client = ClientBuilder::default()
//!     .layer(RateLimitLayer::new(10, Duration::from_secs(1)))
//!     .http(rpc_url);
//!
//! let provider = ProviderBuilder::new()
//!     .connect_client(client);
//! ```
//!
//! ## With Chain-Specific Configuration
//!
//! ```rust,ignore
//! use semioscan::transport::RateLimitLayer;
//! use alloy_chains::NamedChain;
//! use std::time::Duration;
//!
//! // Create rate limiter based on chain requirements
//! let layer = match chain {
//!     NamedChain::Base => RateLimitLayer::new(4, Duration::from_secs(1)),
//!     NamedChain::Optimism => RateLimitLayer::new(10, Duration::from_secs(1)),
//!     _ => RateLimitLayer::new(25, Duration::from_secs(1)),
//! };
//! ```

mod logging;
mod rate_limit;

pub use logging::{LoggingLayer, LoggingService};
pub use rate_limit::{RateLimitLayer, RateLimitService};
