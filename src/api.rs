use alloy_chains::NamedChain;
use alloy_primitives::{address, Address};
use axum::{routing::get, Router};
use odos_sdk::RouterType;
use tokio::net::TcpListener;

use crate::{command::SemioscanHandle, get_lo_price, get_v2_price};

const V2_LIQUIDATOR_ADDRESS: Address = address!("498020622CA0d5De103b7E78E3eFe5819D0d28AB");
// TODO: support pre-v2 routers
const LO_LIQUIDATOR_ADDRESS: Address = address!("9aA30b2289020f9de59D39fBd7Bd5f3BE661a2a6");

/// Extension trait to provide liquidator addresses for router types on specific chains
///
/// Liquidator addresses are separate from router contract addresses and represent
/// the entity authorized to trigger liquidations. Currently only Arbitrum liquidator
/// addresses are known.
pub trait RouterTypeLiquidatorExt {
    /// Get the liquidator address for this router type on the given chain
    ///
    /// Returns Address::ZERO if liquidator address is unknown for the chain
    fn liquidator_address(&self, chain: NamedChain) -> Address;
}

impl RouterTypeLiquidatorExt for RouterType {
    fn liquidator_address(&self, chain: NamedChain) -> Address {
        // Only Arbitrum liquidator addresses are currently known
        if chain != NamedChain::Arbitrum {
            return Address::ZERO;
        }

        match self {
            RouterType::V2 => V2_LIQUIDATOR_ADDRESS,
            RouterType::LimitOrder => LO_LIQUIDATOR_ADDRESS,
            RouterType::V3 => Address::ZERO, // V3 liquidator address not yet known
        }
    }
}

/// Starts the API server.
pub async fn serve_api(listener: TcpListener, price_job: SemioscanHandle) -> anyhow::Result<()> {
    let app = Router::new()
        // Original path for backward compatibility
        .route("/api/v1/price", get(get_v2_price))
        // New explicit paths for different router types
        .route("/api/v1/price/v2", get(get_v2_price))
        .route("/api/v1/price/lo", get(get_lo_price))
        .with_state(price_job);

    let addr = listener.local_addr()?;

    tracing::info!(address = ?addr, "Starting server");

    axum::serve(listener, app).await?;

    Ok(())
}
