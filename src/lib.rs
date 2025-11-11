mod adapter;
mod block_window;
mod combined_retriever;
mod config;
mod event;
mod gas;
mod gas_cache;
mod gas_calculator;
pub mod price; // New trait-based architecture
#[cfg(feature = "odos-example")]
mod price_cache; // Legacy price cache for Odos example
#[cfg(feature = "odos-example")]
mod price_calculator; // Generic price calculator using PriceSource trait
mod spans;
mod tokens_to;
mod transfer;

pub use adapter::*;
pub use block_window::*;
pub use combined_retriever::*;
pub use config::*;
pub use event::*;
pub use gas_cache::*;
pub use gas_calculator::*;
#[cfg(feature = "odos-example")]
pub use price_cache::*;
#[cfg(feature = "odos-example")]
pub use price_calculator::*; // Generic price calculator available with odos-example feature
pub use tokens_to::*;
pub use transfer::*;

// Re-export RouterType from odos-sdk for convenience
#[cfg(feature = "odos-example")]
pub use odos_sdk::RouterType;
