#![feature(context_injection)]

use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

use anyhow::Context;
use base::net::{Transport, TransportEvent};
use hg_common::base::net::dev_cert::generate_dev_priv_key;
use quinn::crypto::rustls::QuicServerConfig;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

pub mod base;

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
    let mut transport = Transport::new(config, bind_addr);

    loop {
        let Some(ev) = transport.process_async().await else {
            continue;
        };

        match ev {
            TransportEvent::Connected { peer } => {
                tracing::info!("Connected: {peer:?}");
            }
            TransportEvent::Disconnected { peer, cause } => {
                tracing::info!("Disconnected: {peer:?}, {cause:?}");
            }
            TransportEvent::DataReceived { peer, packet, task } => {
                tracing::info!("Packet received: {peer:?}, {packet:?}");
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                    drop(task);
                });
            }
            TransportEvent::Shutdown { cause } => {
                tracing::error!("shutdown: {cause:?}");
            }
        }
    }
}
