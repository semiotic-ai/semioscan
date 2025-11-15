//! Event processing for ERC-20 transfers and approvals.
//!
//! This module handles:
//! - Transfer and Approval event definitions
//! - Transfer amount extraction and accumulation
//! - Token discovery via event scanning

pub mod definitions;
pub mod discovery;
pub mod transfers;

// Re-export public types
pub use definitions::*;
pub use discovery::*;
pub use transfers::*;
