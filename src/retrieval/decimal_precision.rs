// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Decimal precision constants for blockchain values

/// Decimal precision for blockchain values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecimalPrecision {
    /// USDC and most stablecoins use 6 decimals
    Usdc,
    /// BSC Binance-Peg USDC uses 18 decimals (non-standard)
    BinancePegUsdc,
    /// Native tokens (ETH, BNB, MATIC, etc.) and gas costs use 18 decimals
    NativeToken,
}

impl DecimalPrecision {
    /// Get the number of decimals as a u8
    pub fn decimals(self) -> u8 {
        match self {
            DecimalPrecision::Usdc => 6,
            DecimalPrecision::BinancePegUsdc => 18,
            DecimalPrecision::NativeToken => 18,
        }
    }
}
