use std::{net::SocketAddr, str::FromStr, sync::Arc};

use anyhow::Context;
use hg_common::base::net::dev_cert::generate_dev_priv_key;
use quinn::crypto::rustls::QuicServerConfig;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with_span_events(FmtSpan::CLOSE)
        .init();

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .ok()
        .context("failed to install crypto provider")?;

    // Setup crypto
    let (dev_key, dev_cert) = generate_dev_priv_key()?;
    let crypto = rustls::ServerConfig::builder()
        // Clients do not identify themselves through certificates.
        .with_no_client_auth()
        // Identify ourselves with the private key and self-signed certificate we just generated.
        .with_single_cert(vec![dev_cert], dev_key)
        .context("failed to create server TLS config")?;

    let crypto = QuicServerConfig::try_from(crypto).context("failed to create QUIC crypto")?;
    let crypto = Arc::new(crypto);

    // Setup server
    let bind_addr = SocketAddr::from_str("127.0.0.1:8080").unwrap();
    let config = quinn::ServerConfig::with_crypto(crypto);
    let endpoint = quinn::Endpoint::server(config, bind_addr)
        .with_context(|| format!("failed to create endpoint on `{bind_addr}`"))?;

    // Run server main loop
    tracing::info!("Listening on `{bind_addr}`!");

    while let Some(incoming) = endpoint.accept().await {
        let incoming = incoming.accept()?.await?;
        tracing::info!("Got connection from {}", incoming.remote_address());

        let (tx, rx) = incoming.open_bi().await?;
    }

    Ok(())
}
