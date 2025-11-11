mod adapter;
mod api;
mod block_window;
#[cfg(feature = "cli")]
pub mod bootstrap; // CLI only
mod combined_retriever;
mod command;
mod event;
mod gas;
mod gas_cache;
mod gas_calculator;
pub mod price; // New trait-based architecture
mod price_cache;
#[cfg(feature = "odos-example")]
mod price_legacy; // Legacy Odos-specific price calculator (will be replaced by trait-based system)
mod spans;
mod tokens_to;
mod transfer;

pub use adapter::*;
pub use api::*;
pub use block_window::*;
pub use combined_retriever::*;
pub use command::*;
pub use event::*;
pub use gas::*;
pub use gas_cache::*;
pub use gas_calculator::*;
pub use price_cache::*;
#[cfg(feature = "odos-example")]
pub use price_legacy::*; // Only available with odos-example feature
pub use tokens_to::*;
pub use transfer::*;

// Re-export RouterType from odos-sdk for convenience
#[cfg(feature = "odos-example")]
pub use odos_sdk::RouterType;
