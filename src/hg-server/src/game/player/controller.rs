use glam::Vec2;
use hg_common::game::player::{
    PlayerOwnerRpcKind, PlayerOwnerRpcSb, PlayerPuppetRpcCb, PlayerPuppetRpcKind,
    PlayerPuppetRpcSb, PlayerRpcCatchup, PlayerRpcKind, PlayerRpcSb,
};
use hg_ecs::{bind, component, Entity, Obj, World};
use hg_engine_common::{
    kinematic::Pos,
    mp::MpServer,
    rpc::{spawn_server_rpc, RpcNodeId, RpcServerHandle, RpcServerPeer, RpcServerReplicator},
};

use super::PlayerOwner;

// === Components === //

#[derive(Debug)]
pub struct PlayerReplicator {
    pub owner: Obj<PlayerOwner>,
    pub pos: Obj<Pos>,
    pub rpc: RpcServerHandle<PlayerRpcKind>,
    pub rpc_owner: RpcServerHandle<PlayerOwnerRpcKind>,
    pub rpc_puppet: RpcServerHandle<PlayerPuppetRpcKind>,
}

component!(PlayerReplicator);

impl RpcServerReplicator<PlayerRpcKind> for PlayerReplicator {
    fn catchup(self: Obj<Self>, world: &mut World) -> PlayerRpcCatchup {
        bind!(world);

        PlayerRpcCatchup {
            name: self.owner.sess.name().to_string(),
            pos: self.pos.0,
        }
    }

    fn process(
        self: Obj<Self>,
        world: &mut World,
        _peer: Obj<RpcServerPeer>,
        packet: PlayerRpcSb,
    ) -> anyhow::Result<()> {
        bind!(world);

        match packet {}
    }
}

impl RpcServerReplicator<PlayerOwnerRpcKind> for PlayerReplicator {
    fn catchup(self: Obj<Self>, world: &mut World) -> RpcNodeId {
        bind!(world);
        self.rpc.id()
    }

    fn process(
        mut self: Obj<Self>,
        world: &mut World,
        _peer: Obj<RpcServerPeer>,
        packet: PlayerOwnerRpcSb,
    ) -> anyhow::Result<()> {
        bind!(world);

        match packet {
            PlayerOwnerRpcSb::SetPos(pos) => {
                self.pos.0 = pos;
                self.rpc_puppet.broadcast(&PlayerPuppetRpcCb::SetPos(pos));
            }
        }

        Ok(())
    }
}

impl RpcServerReplicator<PlayerPuppetRpcKind> for PlayerReplicator {
    fn catchup(self: Obj<Self>, world: &mut World) -> RpcNodeId {
        bind!(world);
        self.rpc.id()
    }

    fn process(
        self: Obj<Self>,
        world: &mut World,
        _peer: Obj<RpcServerPeer>,
        packet: PlayerPuppetRpcSb,
    ) -> anyhow::Result<()> {
        bind!(world);

        match packet {}
    }
}

// === Prefabs === //

pub fn spawn_player(parent: Entity, owner: Obj<PlayerOwner>) -> Entity {
    let me = Entity::new(parent);

    let pos = me.add(Pos(Vec2::new(
        fastrand::f32() * 500.,
        fastrand::f32() * 500.,
    )));

    let mut replicator = me.add(PlayerReplicator {
        pos,
        owner,
        rpc: RpcServerHandle::DANGLING,
        rpc_owner: RpcServerHandle::DANGLING,
        rpc_puppet: RpcServerHandle::DANGLING,
    });
    replicator.rpc = spawn_server_rpc(replicator);
    replicator.rpc_owner = spawn_server_rpc(replicator);
    replicator.rpc_puppet = spawn_server_rpc(replicator);

    let all_players = me.deep_get::<MpServer>().all_players();

    all_players.add_node(replicator.rpc.raw(), None);
    all_players.add_node(replicator.rpc_puppet.raw(), Some(owner.peer));
    replicator.rpc_owner.replicate(owner.peer);

    me
}
