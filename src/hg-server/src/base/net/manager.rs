use bytes::Bytes;
use hg_common::base::{
    net::{back_pressure::ErasedTaskGuard, codec::FrameEncoder, transport::PeerId},
    rpc::{RpcKind, RpcKindServer, RpcNode, RpcNodeServer, RpcServer, RpcServerFlushTransport},
    time::RunLoop,
};
use hg_ecs::{
    bind, component,
    signal::{SimpleSignal, SimpleSignalReader},
    Entity, Obj, World,
};
use hg_utils::hash::FxHashMap;

use super::{Transport, TransportEvent};

// === NetManager === //

#[derive(Debug)]
pub struct NetManager {
    transport: Transport,
    rpc: Obj<RpcServer>,
    sessions: FxHashMap<PeerId, Obj<NetSession>>,
    on_join: SimpleSignal<PeerId>,
}

component!(NetManager);

impl NetManager {
    pub fn new(me: Entity, transport: Transport) -> Obj<NetManager> {
        me.add(Self {
            transport,
            rpc: Entity::new(me).add(RpcServer::new()),
            sessions: FxHashMap::default(),
            on_join: SimpleSignal::new(),
        })
    }

    pub fn define<K: RpcKindServer>(&mut self) {
        self.rpc.define::<K>();
    }

    pub fn register(&mut self, node: Obj<RpcNode>) -> Obj<RpcNodeServer> {
        self.rpc.register(node)
    }

    pub fn replicate(&mut self, node: Obj<RpcNodeServer>, peer: PeerId) {
        self.rpc.replicate(node, peer);
    }

    pub fn de_replicate(&mut self, node: Obj<RpcNodeServer>, peer: PeerId) {
        self.rpc.de_replicate(node, peer);
    }

    pub fn send<K: RpcKind>(&mut self, target: Obj<RpcNodeServer>, packet: K::ClientBound) {
        self.rpc.send_packet::<K>(target, packet);
    }

    pub fn on_join(&self) -> SimpleSignalReader<PeerId> {
        self.on_join.reader()
    }

    pub fn process(mut self: Obj<Self>) {
        self.on_join.reset();

        self.rpc.flush(&mut ServerFlushTrans(self));

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
                    if let Err(err) = sess.process_recv(packet) {
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

        self.on_join.lock();
    }
}

struct ServerFlushTrans(Obj<NetManager>);

impl RpcServerFlushTransport for ServerFlushTrans {
    fn send_packet_single(&mut self, world: &mut World, target: PeerId, packet: FrameEncoder) {
        bind!(world);

        self.0.transport.write_handle_ref().peer_send(
            target,
            packet.finish(),
            ErasedTaskGuard::noop(),
        );
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
            self.0.transport.write_handle_ref().peer_send(
                peer,
                packet.clone(),
                ErasedTaskGuard::noop(),
            );
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
