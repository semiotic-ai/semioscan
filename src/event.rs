//! Canonical ERC-20 event definitions for blockchain event decoding
//!
//! This module provides strongly-typed event definitions for the standard ERC-20
//! token events: Transfer and Approval. These events are universal across all
//! ERC-20 tokens and follow the ERC-20 specification.
//!
//! # Event Signatures
//!
//! - **Transfer**: `Transfer(address,address,uint256)`
//! - **Approval**: `Approval(address,address,uint256)`
//!
//! # Example: Decoding Transfer events
//!
//! ```rust,ignore
//! use semioscan::Transfer;
//! use alloy_sol_types::SolEvent;
//! use alloy_rpc_types::Log;
//!
//! // Fetch logs from RPC
//! let logs: Vec<Log> = provider.get_logs(&filter).await?;
//!
//! for log in logs {
//!     match Transfer::decode_log(&log.inner) {
//!         Ok(event) => {
//!             println!("Transfer: {} -> {}, amount: {}",
//!                 event.from, event.to, event.value);
//!         }
//!         Err(e) => eprintln!("Failed to decode: {}", e),
//!     }
//! }
//! ```
//!
//! # Example: Decoding Approval events
//!
//! ```rust,ignore
//! use semioscan::Approval;
//! use alloy_sol_types::SolEvent;
//!
//! match Approval::decode_log(&log.inner) {
//!     Ok(event) => {
//!         println!("Approval: {} approved {} to spend {}",
//!             event.owner, event.spender, event.value);
//!     }
//!     Err(e) => eprintln!("Failed to decode: {}", e),
//! }
//! ```
//!
//! # Example: Using event signatures for filters
//!
//! ```rust
//! use semioscan::TRANSFER_EVENT_SIGNATURE;
//! use alloy_primitives::keccak256;
//!
//! // Create event topic hash for RPC filters
//! let transfer_topic = keccak256(TRANSFER_EVENT_SIGNATURE.as_bytes());
//! ```

use std::fmt::Debug;

use alloy_sol_types::sol;

/// The canonical ERC-20 Transfer event signature
///
/// This constant contains the string representation of the Transfer event
/// signature as defined in the ERC-20 specification. Use this with `keccak256`
/// to create event topic filters for RPC queries.
///
/// # Example
///
/// ```rust
/// use semioscan::TRANSFER_EVENT_SIGNATURE;
/// use alloy_primitives::keccak256;
///
/// let topic = keccak256(TRANSFER_EVENT_SIGNATURE.as_bytes());
/// // Use topic in RPC filter to query Transfer events
/// ```
pub const TRANSFER_EVENT_SIGNATURE: &str = "Transfer(address,address,uint256)";

sol! {
    /// ERC-20 Transfer event
    ///
    /// Emitted when tokens are transferred from one address to another.
    /// This includes:
    /// - Regular transfers between users
    /// - Minting (from = 0x0)
    /// - Burning (to = 0x0)
    ///
    /// # Fields
    ///
    /// - `from`: Address tokens are transferred from (indexed)
    /// - `to`: Address tokens are transferred to (indexed)
    /// - `value`: Amount of tokens transferred (raw, not adjusted for decimals)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use semioscan::Transfer;
    /// use alloy_sol_types::SolEvent;
    ///
    /// let event = Transfer::decode_log(&log.inner)?;
    /// println!("Transfer of {} from {} to {}", event.value, event.from, event.to);
    /// ```
    event Transfer(address indexed from, address indexed to, uint256 value);
}

impl Debug for Transfer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Transfer(from: {}, to: {}, value: {})",
            self.from, self.to, self.value
        )
    }
}

/// The canonical ERC-20 Approval event signature
///
/// This constant contains the string representation of the Approval event
/// signature as defined in the ERC-20 specification. Use this with `keccak256`
/// to create event topic filters for RPC queries.
///
/// # Example
///
/// ```rust
/// use semioscan::APPROVAL_EVENT_SIGNATURE;
/// use alloy_primitives::keccak256;
///
/// let topic = keccak256(APPROVAL_EVENT_SIGNATURE.as_bytes());
/// // Use topic in RPC filter to query Approval events
/// ```
pub const APPROVAL_EVENT_SIGNATURE: &str = "Approval(address,address,uint256)";

sol! {
    /// ERC-20 Approval event
    ///
    /// Emitted when an owner approves a spender to transfer tokens on their behalf.
    /// This is used for delegated transfers, commonly seen in DEX interactions.
    ///
    /// # Fields
    ///
    /// - `owner`: Address that owns the tokens and grants approval (indexed)
    /// - `spender`: Address that is approved to spend the tokens (indexed)
    /// - `value`: Maximum amount the spender is approved to transfer (raw, not adjusted for decimals)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use semioscan::Approval;
    /// use alloy_sol_types::SolEvent;
    ///
    /// let event = Approval::decode_log(&log.inner)?;
    /// println!("{} approved {} to spend up to {}", event.owner, event.spender, event.value);
    /// ```
    event Approval(address indexed owner, address indexed spender, uint256 value);
}

impl Debug for Approval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Approval(owner: {}, spender: {}, value: {})",
            self.owner, self.spender, self.value
        )
    }
}
