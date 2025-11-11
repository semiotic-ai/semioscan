//! This module contains the canonical Transfer and Approval event definitions.
//! It is used to decode events from the blockchain.

use std::fmt::Debug;

use alloy_sol_types::sol;

/// The canonical Transfer event signature
pub const TRANSFER_EVENT_SIGNATURE: &str = "Transfer(address,address,uint256)";

sol! {
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

/// The canonical Approval event signature
pub const APPROVAL_EVENT_SIGNATURE: &str = "Approval(address,address,uint256)";

sol! {
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
