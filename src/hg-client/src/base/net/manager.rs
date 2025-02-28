use std::{net::SocketAddr, str::FromStr, sync::Arc};

use anyhow::Context;
use bytes::Bytes;
use hg_common::base::{
    net::{back_pressure::ErasedTaskGuard, codec::FrameEncoder, dev_cert::fetch_dev_pub_cert},
    rpc::{RpcClient, RpcKindClient},
};
use hg_ecs::{component, Entity, Obj};
use quinn::crypto::rustls::QuicClientConfig;

use super::{Transport, TransportEvent};

#[derive(Debug)]
pub struct NetManager {
    transport: Transport,
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

        let transport = Transport::new(
            config,
            SocketAddr::from_str("127.0.0.1:8080").unwrap(),
            "localhost",
        );

        let rpc = Entity::new(me).add(RpcClient::new());

        let mgr = me.add(Self { transport, rpc });

        Ok(mgr)
    }

    pub fn define_rpc<K: RpcKindClient>(mut self: Obj<Self>) {
        self.rpc.define::<K>();
    }

    pub fn process(mut self: Obj<Self>) {
        while let Some(ev) = self.transport.process() {
            match ev {
                TransportEvent::Connected => {
                    // Send login packet
                    let mut encoder = FrameEncoder::new();
                    encoder
                        .data_mut()
                        .extend_from_slice(b"I want to log in, please!");

                    let data = encoder.finish();
                    self.transport.send(data, ErasedTaskGuard::noop());
                }
                TransportEvent::Disconnected { cause } => todo!(),
                TransportEvent::DataReceived { packet, task } => {
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
