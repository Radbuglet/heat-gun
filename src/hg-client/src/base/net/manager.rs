use std::{net::SocketAddr, str::FromStr, sync::Arc};

use anyhow::Context;
use bytes::Bytes;
use hg_common::base::{
    net::{
        back_pressure::ErasedTaskGuard,
        backends::quic_client::QuicClientTransport,
        codec::FrameEncoder,
        dev_cert::fetch_dev_pub_cert,
        transport::{ClientTransport as _, ClientTransportEvent},
    },
    rpc::{RpcClient, RpcKind, RpcKindClient, RpcNodeClient},
};
use hg_ecs::{component, Entity, Obj};
use quinn::crypto::rustls::QuicClientConfig;

#[derive(Debug)]
pub struct NetManager {
    transport: QuicClientTransport,
    rpc: Obj<RpcClient>,
}

component!(NetManager);

impl NetManager {
    pub fn new(me: Entity) -> anyhow::Result<Obj<Self>> {
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

        let rpc = Entity::new(me).add(RpcClient::new());

        let mgr = me.add(Self { transport, rpc });

        Ok(mgr)
    }

    pub fn define<K: RpcKindClient>(mut self: Obj<Self>) {
        self.rpc.define::<K>();
    }

    pub fn send<K: RpcKind>(mut self: Obj<Self>, target: Obj<RpcNodeClient>, data: K::ServerBound) {
        let packet = self.rpc.send_packet::<K>(target, data);
        self.transport
            .send(packet.finish(), ErasedTaskGuard::noop());
    }

    pub fn process(mut self: Obj<Self>) {
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
