//! Crisp server entrypoint: installs diagnostics and tracing, then runs the
//! HTTP service. All real work lives in the [`crisp_server`] library crate.

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    crisp_server::run().await
}
