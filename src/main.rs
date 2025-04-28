use std::process::ExitCode;

use semioscan::bootstrap::run;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt::init();
    if let Err(e) = run().await {
        tracing::error!("Clearing Job error: {e}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
