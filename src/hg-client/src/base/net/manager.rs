use std::{net::SocketAddr, str::FromStr, sync::Arc};

use anyhow::Context;
use bytes::Bytes;
use hg_common::base::{
    net::{
        fetch_dev_pub_cert, quic_client::QuicClientTransport, ClientTransport as _,
        ClientTransportEvent, ErasedTaskGuard, FrameEncoder,
    },
    rpc::RpcClient,
};
use hg_ecs::{component, Obj};
use quinn::crypto::rustls::QuicClientConfig;

#[derive(Debug)]
pub struct NetManager {
    transport: QuicClientTransport,
    rpc: Obj<RpcClient>,
}

component!(NetManager);

impl NetManager {
    pub fn new(rpc: Obj<RpcClient>) -> anyhow::Result<Self> {
        let mut store = rustls::RootCertStore::empty();
        store.add(fetch_dev_pub_cert()?.context("no dev certificate found")?)?;
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(store)
            .with_no_client_auth();

        let config = Arc::new(QuicClientConfig::try_from(config)?);
        let config = quinn::ClientConfig::new(config);

        let transport = QuicClientTransport::new(
            config,
            SocketAddr::from_str("127.0.0.1:8080").unwrap(),
            "localhost",
        );

        Ok(Self { transport, rpc })
    }

    pub fn process(mut self: Obj<Self>) {
        for packet in self.rpc.flush_sends() {
            self.transport
                .send(packet.finish(), ErasedTaskGuard::noop());
        }

        while let Some(ev) = self.transport.process() {
            match ev {
                ClientTransportEvent::Connected => {
                    // Send login packet
                    let mut encoder = FrameEncoder::new();
                    encoder.extend_from_slice(b"I want to log in, please!");
                    self.transport
                        .send(encoder.finish(), ErasedTaskGuard::noop());
                }
                ClientTransportEvent::Disconnected { cause } => todo!(),
                ClientTransportEvent::DataReceived { packet, task } => {
                    if let Err(err) = self.rpc.recv_packet(packet) {
                        tracing::error!("failed to process client-bound packet: {err:?}");
                        self.transport.disconnect(Bytes::new());
                    }
                    drop(task);
                }
            }
        }
    }
}
