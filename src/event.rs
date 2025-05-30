use std::fmt::Debug;

use alloy_sol_types::sol;

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
