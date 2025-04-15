use hg_ecs::{component, Obj};
use hg_engine_common::{mp::MpServerSession, rpc::RpcServerPeer};

use super::PlayerReplicator;

// === Components === //

#[derive(Debug)]
pub struct PlayerOwner {
    pub peer: Obj<RpcServerPeer>,
    pub sess: Obj<MpServerSession>,
    pub player: Obj<PlayerReplicator>,
}

component!(PlayerOwner);

impl PlayerOwner {
    pub fn downcast(peer: Obj<RpcServerPeer>) -> Obj<Self> {
        peer.entity().get()
    }
}
