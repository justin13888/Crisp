//! `crisp-mcp`: serves the Crisp authoring tools over MCP on stdio.
//!
//! stdout carries the protocol, so every log line goes to stderr.

use std::sync::Arc;

use crisp_mcp::clock::SystemClock;
use crisp_mcp::config::Config;
use crisp_mcp::gateway::{Gateway, HttpGateway, NoServer};
use crisp_mcp::library::FsLibrary;
use crisp_mcp::signer::Signer;
use crisp_mcp::tools::{Authoring, CrispTools};
use eyre::WrapErr as _;
use rmcp::ServiceExt as _;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let config = Config::from_env()?;
    let filter = EnvFilter::try_from_env("CRISP_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let logs = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false);
    if config.json_logs {
        logs.json().init();
    } else {
        logs.init();
    }

    let signer = Arc::new(Signer::new(config.key.clone()));
    let gateway: Arc<dyn Gateway> = match config.server.clone() {
        Some(server) => Arc::new(HttpGateway::new(server, Arc::clone(&signer))?),
        None => Arc::new(NoServer),
    };
    let authoring = Authoring::new(
        Arc::new(FsLibrary::new(config.library_dir.clone())),
        gateway,
        signer,
        config.display_name.clone(),
        Arc::new(SystemClock),
    );
    info!(
        library = %config.library_dir.display(),
        key = config.key.is_some(),
        server = config.server.as_ref().map(|s| s.url.as_str()),
        version = env!("CARGO_PKG_VERSION"),
        "crisp-mcp starting on stdio"
    );
    let service = CrispTools::new(authoring)
        .serve(rmcp::transport::stdio())
        .await
        .wrap_err("could not start the MCP session")?;
    service.waiting().await.wrap_err("the MCP session failed")?;
    info!("crisp-mcp stopped");
    Ok(())
}
