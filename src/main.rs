use std::process::ExitCode;

use semioscan::bootstrap::run;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<ExitCode, ExitCode> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(
            fmt::layer()
                .with_target(true)
                .json()
                .flatten_event(true)
                .with_ansi(true),
        )
        .init();

    if let Err(e) = run().await {
        tracing::error!("Semioscan error: {e}");
        return Err(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}
