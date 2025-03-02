use bytes::Bytes;
use hg_common::base::{
    net::{
        quic_server::QuicServerTransport, ErasedTaskGuard, PeerId, ServerTransport as _,
        ServerTransportEvent,
    },
    rpc::{RpcGroup, RpcPeer, RpcServer, RpcServerFlushTransport},
    time::RunLoop,
};
use hg_ecs::{bind, component, Entity, Obj, World};
use hg_utils::hash::FxHashMap;

// === NetManager === //

#[derive(Debug)]
pub struct NetManager {
    transport: QuicServerTransport,
    rpc: Obj<RpcServer>,
    sessions: FxHashMap<PeerId, Obj<NetSession>>,
    group: Obj<RpcGroup>,
}

component!(NetManager);

impl NetManager {
    pub fn attach(me: Entity, transport: QuicServerTransport) -> Obj<Self> {
        let rpc = me.add(RpcServer::new());
        let group = me.add(RpcGroup::new());

        me.add(Self {
            transport,
            rpc,
            sessions: FxHashMap::default(),
            group,
        })
    }

    pub fn group(&self) -> Obj<RpcGroup> {
        self.group
    }

    pub fn process(mut self: Obj<Self>) {
        self.rpc.flush(&mut ServerFlushTrans);

        while let Some(ev) = self.transport.process() {
            match ev {
                ServerTransportEvent::Connected { peer, task } => {
                    let sess = Entity::new(self.entity()).add(NetSession::new(self, peer));
                    self.sessions.insert(sess.peer, sess);

                    drop(task);
                }
                ServerTransportEvent::Disconnected { peer, cause: _ } => {
                    let sess = self.sessions.remove(&peer).unwrap();
                    if let SessionState::Play(peer) = sess.state {
                        peer.disconnect();
                        self.group.remove_peer(peer);
                    }
                    sess.entity().destroy();
                }
                ServerTransportEvent::DataReceived { peer, packet, task } => {
                    let sess = self.sessions[&peer];
                    if let Err(err) = sess.process_recv(packet) {
                        tracing::error!("failed to process packet sent by peer {peer}: {err:?}");

                        self.transport
                            .peer_kick(peer, Bytes::from_static(b"protocol error"));
                    }

                    drop(task);
                }
                ServerTransportEvent::Shutdown { cause: _ } => {
                    Entity::service::<RunLoop>().request_exit();
                }
            }
        }
    }
}

struct ServerFlushTrans;

impl RpcServerFlushTransport for ServerFlushTrans {
    fn send_packet(&mut self, world: &mut World, target: Obj<RpcPeer>, packet: Bytes) {
        bind!(world);

        let mut target = target.entity().get::<NetSession>();
        target
            .manager
            .transport
            .peer_send(target.peer, packet, ErasedTaskGuard::noop());
    }
}

// === NetSession === //

#[derive(Debug)]
pub struct NetSession {
    manager: Obj<NetManager>,
    peer: PeerId,
    state: SessionState,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum SessionState {
    Login,
    Play(Obj<RpcPeer>),
}

component!(NetSession);

impl NetSession {
    pub fn new(manager: Obj<NetManager>, peer: PeerId) -> Self {
        Self {
            manager,
            peer,
            state: SessionState::Login,
        }
    }

    pub fn process_recv(mut self: Obj<Self>, packet: Bytes) -> anyhow::Result<()> {
        match self.state {
            SessionState::Login => {
                tracing::info!("Peer {} logged in with {packet:?}", self.peer);
                let peer = self.manager.rpc.register_peer(self.entity());
                self.manager.group.add_peer(peer);
                self.state = SessionState::Play(peer);
                Ok(())
            }
            SessionState::Play(peer) => self.manager.rpc.recv_packet(peer, packet),
        }
    }
}
