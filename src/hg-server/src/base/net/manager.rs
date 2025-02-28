use bytes::Bytes;
use hg_common::base::{net::transport::PeerId, rpc::RpcServer, time::RunLoop};
use hg_ecs::{component, Entity, Obj};
use hg_utils::hash::FxHashMap;

use super::{Transport, TransportEvent};

#[derive(Debug)]
pub struct NetManager {
    transport: Transport,
    rpc: Obj<RpcServer>,
    sessions: FxHashMap<PeerId, Obj<NetSession>>,
}

component!(NetManager);

impl NetManager {
    pub fn new(me: Entity, transport: Transport) -> Obj<NetManager> {
        me.add(Self {
            transport,
            rpc: Entity::new(me).add(RpcServer::new()),
            sessions: FxHashMap::default(),
        })
    }

    pub fn rpc(&self) -> Obj<RpcServer> {
        self.rpc
    }

    pub fn process(mut self: Obj<Self>) {
        while let Some(ev) = self.transport.process() {
            match ev {
                TransportEvent::Connected { peer, task } => {
                    let sess = Entity::new(self.entity()).add(NetSession::new(self, peer));
                    self.sessions.insert(sess.peer, sess);

                    drop(task);
                }
                TransportEvent::Disconnected { peer, cause: _ } => {
                    self.sessions.remove(&peer);
                }
                TransportEvent::DataReceived { peer, packet, task } => {
                    let sess = self.sessions[&peer];
                    if let Err(err) = self.process_packet(sess, packet) {
                        tracing::error!("failed to process packet sent by peer {peer}: {err:?}");

                        self.transport
                            .write_handle_ref()
                            .peer_kick(peer, Bytes::from_static(b"protocol error"));
                    }

                    drop(task);
                }
                TransportEvent::Shutdown { cause: _ } => {
                    Entity::service::<RunLoop>().request_exit();
                }
            }
        }
    }

    fn process_packet(self: Obj<Self>, sess: Obj<NetSession>, packet: Bytes) -> anyhow::Result<()> {
        match sess.state {
            SessionState::Play => self.rpc.recv_packet(sess.peer, packet),
        }
    }
}

#[derive(Debug)]
pub struct NetSession {
    manager: Obj<NetManager>,
    peer: PeerId,
    state: SessionState,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum SessionState {
    Play,
}

component!(NetSession);

impl NetSession {
    pub fn new(manager: Obj<NetManager>, peer: PeerId) -> Self {
        Self {
            manager,
            peer,
            state: SessionState::Play,
        }
    }
}
