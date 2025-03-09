use bytes::Bytes;
use hg_ecs::{
    bind, component,
    signal::{DeferSignal, DeferSignalReader},
    Entity, Obj, Query, World,
};
use hg_utils::hash::FxHashMap;

use crate::base::{
    mp::MpSbHello,
    net::{ErasedTaskGuard, PeerId, RpcPacket, ServerTransport, ServerTransportEvent},
    rpc::{RpcServerPeer, RpcServer, RpcServerFlushTransport},
    time::RunLoop,
};

// === MpServer === //

#[derive(Debug)]
pub struct MpServer {
    transport: Box<dyn ServerTransport>,
    sessions: FxHashMap<PeerId, Obj<MpServerSession>>,
    rpc: Obj<RpcServer>,
    on_join: DeferSignal<Obj<MpServerSession>>,
    on_quit: DeferSignal<Obj<MpServerSession>>,
}

component!(MpServer);

impl MpServer {
    pub fn new(transport: Box<dyn ServerTransport>, rpc: Obj<RpcServer>) -> Self {
        Self {
            transport,
            rpc,
            sessions: FxHashMap::default(),
            on_join: DeferSignal::new(),
            on_quit: DeferSignal::new(),
        }
    }

    pub fn on_join(&self) -> DeferSignalReader<Obj<MpServerSession>> {
        self.on_join.reader()
    }

    pub fn on_quit(&self) -> DeferSignalReader<Obj<MpServerSession>> {
        self.on_quit.reader()
    }

    pub fn process(mut self: Obj<Self>) {
        self.on_join.reset();
        self.on_quit.reset();
        self.rpc.flush(&mut ServerFlushTrans);

        while let Some(ev) = self.transport.process() {
            match ev {
                ServerTransportEvent::Connected { peer, task } => {
                    let sess = Entity::new(self.entity()).add(MpServerSession::new(self, peer));
                    self.sessions.insert(sess.peer, sess);

                    drop(task);
                }
                ServerTransportEvent::Disconnected { peer, cause: _ } => {
                    let sess = self.sessions.remove(&peer).unwrap();
                    if let SessionState::Play { peer, .. } = sess.state {
                        peer.disconnect();
                        self.on_quit.fire(sess);
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

        self.on_join.lock();
        self.on_quit.lock();
    }
}

struct ServerFlushTrans;

impl RpcServerFlushTransport for ServerFlushTrans {
    fn send_packet(&mut self, world: &mut World, target: Obj<RpcServerPeer>, packet: Bytes) {
        bind!(world);

        let mut target = target.entity().get::<MpServerSession>();
        target
            .manager
            .transport
            .peer_send(target.peer, packet, ErasedTaskGuard::noop());
    }
}

// === MpServerSession === //

#[derive(Debug)]
pub struct MpServerSession {
    manager: Obj<MpServer>,
    peer: PeerId,
    state: SessionState,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum SessionState {
    Login,
    Play { peer: Obj<RpcServerPeer>, name: String },
}

component!(MpServerSession);

impl MpServerSession {
    pub fn new(manager: Obj<MpServer>, peer: PeerId) -> Self {
        Self {
            manager,
            peer,
            state: SessionState::Login,
        }
    }

    pub fn downcast(peer: Obj<RpcServerPeer>) -> Obj<Self> {
        peer.entity().get()
    }

    pub fn peer(&self) -> Obj<RpcServerPeer> {
        let SessionState::Play { peer, .. } = self.state else {
            panic!("session has not yet transitioned to a play state");
        };

        peer
    }

    pub fn name(&self) -> &str {
        let SessionState::Play { ref name, .. } = self.state else {
            panic!("session has not yet transitioned to a play state");
        };

        name
    }

    pub fn process_recv(mut self: Obj<Self>, packet: Bytes) -> anyhow::Result<()> {
        match self.state {
            SessionState::Login => {
                let packet = MpSbHello::decode(&packet)?;
                tracing::info!("Peer {} logged in with {packet:?}", self.peer);
                let peer = self.manager.rpc.register_peer(self.entity());
                self.manager.on_join.fire(self);
                self.state = SessionState::Play {
                    peer,
                    name: packet.username.clone(),
                };
                Ok(())
            }
            SessionState::Play { peer, .. } => self.manager.rpc.recv_packet(peer, packet),
        }
    }
}

// === Systems === //

pub fn sys_update_mp_servers() {
    for server in Query::<Obj<MpServer>>::new() {
        server.process();
    }
}
