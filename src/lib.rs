mod adapter;
#[cfg(all(feature = "api-server", feature = "odos-example"))]
mod api;
mod block_window;
#[cfg(feature = "cli")]
pub mod bootstrap; // CLI only
mod combined_retriever;
mod command;
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
pub mod provider; // Provider abstraction for blockchain access
mod spans;
mod tokens_to;
mod transfer;

pub use adapter::*;
#[cfg(all(feature = "api-server", feature = "odos-example"))]
pub use api::*;
pub use block_window::*;
pub use combined_retriever::*;
pub use command::*;
pub use config::*;
pub use event::*;
pub use gas::*;
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
