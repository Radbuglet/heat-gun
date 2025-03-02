use hg_common::base::{mp::MpServerSession, rpc::RpcPeer};
use hg_ecs::{component, Obj};

use super::PlayerStateServer;

// === Components === //

#[derive(Debug)]
pub struct PlayerOwner {
    pub peer: Obj<RpcPeer>,
    pub sess: Obj<MpServerSession>,
    pub player: Obj<PlayerStateServer>,
}

component!(PlayerOwner);

impl PlayerOwner {
    pub fn downcast(peer: Obj<RpcPeer>) -> Obj<Self> {
        peer.entity().get()
    }
}
