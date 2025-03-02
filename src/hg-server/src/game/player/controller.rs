use glam::Vec2;
use hg_common::{
    base::{
        kinematic::Pos,
        rpc::{
            register_server_rpc, RpcGroup, RpcNodeServer, RpcPeer, RpcServerCup,
            RpcServerReplicator, RpcServerSb,
        },
    },
    game::player::{PlayerRpcCatchup, PlayerRpcKind},
};
use hg_ecs::{bind, component, Entity, Obj, World};

use super::PlayerOwner;

// === Components === //

#[derive(Debug)]
pub struct PlayerReplicator {
    pub owner: Obj<PlayerOwner>,
    pub pos: Obj<Pos>,
    pub rpc: Obj<RpcNodeServer>,
}

component!(PlayerReplicator);

impl RpcServerReplicator for PlayerReplicator {
    type Kind = PlayerRpcKind;

    fn catchup(self: Obj<Self>, world: &mut World, _peer: Obj<RpcPeer>) -> RpcServerCup<Self> {
        bind!(world);

        PlayerRpcCatchup {
            name: self.owner.sess.name().to_string(),
            pos: self.pos.0,
        }
    }

    fn process(
        self: Obj<Self>,
        world: &mut World,
        _peer: Obj<RpcPeer>,
        packet: RpcServerSb<Self>,
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
        rpc: Obj::DANGLING,
    });
    let rpc = register_server_rpc(replicator);
    replicator.rpc = rpc;

    Entity::service::<RpcGroup>().add_node(rpc);

    me
}
