use std::context::{infer_bundle, Bundle};

use glam::Vec2;
use hg_common::{
    base::{
        kinematic::Pos,
        rpc::{
            add_server_rpc_node, RpcGroup, RpcKindServer, RpcNodeServer, RpcPeer, RpcServerCup,
            RpcServerSb,
        },
    },
    game::player::{PlayerRpcCatchup, PlayerRpcKind},
};
use hg_ecs::{component, Entity, Obj};

use super::PlayerOwner;

// === Rpc === //

pub struct PlayerRpcKindServer;

impl RpcKindServer for PlayerRpcKindServer {
    type Kind = PlayerRpcKind;
    type Cx<'a> = infer_bundle!('a);
    type RpcRoot = PlayerStateServer;

    fn catchup(
        cx: Bundle<Self::Cx<'_>>,
        _peer: Obj<RpcPeer>,
        target: Obj<Self::RpcRoot>,
    ) -> RpcServerCup<Self> {
        let static ..cx;

        PlayerRpcCatchup {
            name: target.owner.sess.name().to_string(),
            pos: target.pos.0,
        }
    }

    fn process(
        cx: Bundle<Self::Cx<'_>>,
        _target: Obj<Self::RpcRoot>,
        _sender: Obj<RpcPeer>,
        packet: RpcServerSb<Self>,
    ) -> anyhow::Result<()> {
        let static ..cx;

        match packet {}
    }
}

// === Components === //

#[derive(Debug)]
pub struct PlayerStateServer {
    pub owner: Obj<PlayerOwner>,
    pub pos: Obj<Pos>,
    pub rpc: Obj<RpcNodeServer>,
}

component!(PlayerStateServer);

// === Prefabs === //

pub fn spawn_player(parent: Entity, owner: Obj<PlayerOwner>) -> Entity {
    let me = Entity::new(parent);

    let pos = me.add(Pos(Vec2::ZERO));
    let rpc = add_server_rpc_node::<PlayerRpcKindServer>(me);
    me.add(PlayerStateServer { pos, owner, rpc });
    Entity::service::<RpcGroup>().add_node(rpc);

    me
}
