use hg_common::base::{
    net::{serialize::decode_multi_part, transport::PeerId},
    rpc::RpcServer,
};
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
    pub fn process(mut self: Obj<Self>) {
        while let Some(ev) = self.transport.process_non_blocking() {
            match ev {
                TransportEvent::Connected { peer, task } => {
                    let sess = Entity::new(self.entity()).add(NetSession::new(self, peer));
                    self.sessions.insert(sess.peer, sess);

                    drop(task);
                }
                TransportEvent::Disconnected { peer, cause } => {}
                TransportEvent::DataReceived { peer, packet, task } => {
                    let peer = self.sessions[&peer];

                    let packet = decode_multi_part(&packet);
                }
                TransportEvent::Shutdown { cause } => {}
            }
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
}
