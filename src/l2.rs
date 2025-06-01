use alloy_chains::NamedChain;

pub trait L2 {
    /// Some chains have L1 data fees, such as OP Stack chains like Base and Optimism.
    /// Chains such as Arbitrum and Polygon do not have L1 data fees.
    fn has_l1_fees(&self) -> bool;
}

impl L2 for NamedChain {
    /// Note this implementation is no way near exhaustive.
    fn has_l1_fees(&self) -> bool {
        use NamedChain::*;
        matches!(self, Base | Optimism | Fraxtal | Mantle | Mode | Scroll)
    }
}
