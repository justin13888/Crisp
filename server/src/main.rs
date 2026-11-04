//! Crisp server entrypoint: installs diagnostics and tracing, then runs the
//! HTTP service. All real work lives in the [`crisp_server`] library crate.

use crisp_server::config::LogFormat;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let logs = tracing_subscriber::fmt().with_env_filter(filter);
    match LogFormat::from_lookup(|key| std::env::var(key).ok())? {
        LogFormat::Text => logs.init(),
        LogFormat::Json => logs.json().with_current_span(true).init(),
    }

    crisp_server::run().await
}
