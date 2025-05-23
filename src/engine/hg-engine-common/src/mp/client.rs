use bytes::Bytes;
use hg_ecs::{component, Obj, Query};

use crate::{
    net::{ClientTransport, ClientTransportEvent, ErasedTaskGuard, FrameEncoder},
    rpc::RpcClient,
};

use super::MpSbHello;

// === MpClient === //

#[derive(Debug)]
pub struct MpClient {
    transport: Box<dyn ClientTransport>,
    rpc: Obj<RpcClient>,
}

component!(MpClient);

impl MpClient {
    pub fn new(transport: Box<dyn ClientTransport>, rpc: Obj<RpcClient>) -> Self {
        Self { transport, rpc }
    }

    pub fn process(mut self: Obj<Self>) {
        if let Err(err) = self.rpc.reset() {
            tracing::error!("protocol error ocurred: {err:?}");
            self.transport.disconnect(Bytes::new());
        }

        for packet in self.rpc.flush_sends() {
            self.transport
                .send(packet.finish(), ErasedTaskGuard::noop());
        }

        while let Some(ev) = self.transport.process() {
            match ev {
                ClientTransportEvent::Connected => {
                    // Send login packet
                    self.transport.send(
                        FrameEncoder::single(&MpSbHello {
                            username: "player_mc_playerface".to_string(),
                        }),
                        ErasedTaskGuard::noop(),
                    );
                }
                ClientTransportEvent::Disconnected { cause: _ } => todo!(),
                ClientTransportEvent::DataReceived { packet, task } => {
                    self.rpc.recv_packet(packet);
                    drop(task);
                }
            }
        }

        self.rpc.freeze();
    }
}

// === Systems === //

pub fn sys_update_mp_clients() {
    for client in Query::<Obj<MpClient>>::new() {
        client.process();
    }
}
