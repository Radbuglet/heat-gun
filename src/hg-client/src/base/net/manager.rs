use std::{net::SocketAddr, str::FromStr, sync::Arc};

use anyhow::Context;
use hg_common::base::net::dev_cert::fetch_dev_pub_cert;
use hg_ecs::component;
use quinn::crypto::rustls::QuicClientConfig;

use super::Transport;

#[derive(Debug)]
pub struct NetManager {
    pub transport: Transport,
}

component!(NetManager);

impl NetManager {
    pub fn new() -> anyhow::Result<Self> {
        let mut store = rustls::RootCertStore::empty();
        store.add(fetch_dev_pub_cert()?.context("no dev certificate found")?)?;
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(store)
            .with_no_client_auth();

        let config = Arc::new(QuicClientConfig::try_from(config)?);
        let config = quinn::ClientConfig::new(config);

        let transport = Transport::new(
            config,
            SocketAddr::from_str("127.0.0.1:8080").unwrap(),
            "localhost",
        );

        Ok(Self { transport })
    }
}
