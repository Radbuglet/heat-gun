use bytes::Bytes;
use hg_common::base::{
    net::{
        quic_server::QuicServerTransport, ErasedTaskGuard, FrameEncoder, PeerId,
        ServerTransport as _, ServerTransportEvent,
    },
    rpc::{RpcNodeServer, RpcServer, RpcServerFlushTransport},
    time::RunLoop,
};
use hg_ecs::{
    bind, component,
    signal::{SimpleSignal, SimpleSignalReader},
    Entity, Obj, World,
};
use hg_utils::hash::FxHashMap;

// === NetManager === //

#[derive(Debug)]
pub struct NetManager {
    transport: QuicServerTransport,
    rpc: Obj<RpcServer>,
    sessions: FxHashMap<PeerId, Obj<NetSession>>,
    on_join: SimpleSignal<PeerId>,
}

component!(NetManager);

impl NetManager {
    pub fn new(transport: QuicServerTransport, rpc: Obj<RpcServer>) -> Self {
        Self {
            transport,
            rpc,
            sessions: FxHashMap::default(),
            on_join: SimpleSignal::new(),
        }
    }

    pub fn on_join(&self) -> SimpleSignalReader<PeerId> {
        self.on_join.reader()
    }

    pub fn process(mut self: Obj<Self>) {
        self.on_join.reset();

        self.rpc.flush(&mut ServerFlushTrans(self));

        while let Some(ev) = self.transport.process() {
            match ev {
                ServerTransportEvent::Connected { peer, task } => {
                    let sess = Entity::new(self.entity()).add(NetSession::new(self, peer));
                    self.sessions.insert(sess.peer, sess);

                    drop(task);
                }
                ServerTransportEvent::Disconnected { peer, cause: _ } => {
                    self.sessions.remove(&peer);
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
    }
}

struct ServerFlushTrans(Obj<NetManager>);

impl RpcServerFlushTransport for ServerFlushTrans {
    fn send_packet_single(&mut self, world: &mut World, target: PeerId, packet: FrameEncoder) {
        bind!(world);

        self.0
            .transport
            .peer_send(target, packet.finish(), ErasedTaskGuard::noop());
    }

    fn send_packet_multi(
        &mut self,
        world: &mut hg_ecs::World,
        target: Obj<RpcNodeServer>,
        packet: FrameEncoder,
    ) {
        bind!(world);

        let packet = packet.finish();

        for &peer in target.visible_to() {
            self.0
                .transport
                .peer_send(peer, packet.clone(), ErasedTaskGuard::noop());
        }
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
    Play,
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
                self.state = SessionState::Play;
                self.manager.on_join.fire(self.peer);
                Ok(())
            }
            SessionState::Play => self.manager.rpc.recv_packet(self.peer, packet),
        }
    }
}
