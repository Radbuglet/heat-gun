use std::context::{infer_bundle, Bundle};

use hg_common::{
    base::{
        net::transport::PeerId,
        rpc::{RpcKindServer, RpcNode, RpcServerCup, RpcServerSb},
    },
    game::player::{PlayerRpcCatchup, PlayerRpcKind},
};
use hg_ecs::{component, Entity, Obj};

use crate::base::net::NetManager;

// === Rpc === //

pub struct PlayerRpcKindServer;

impl RpcKindServer for PlayerRpcKindServer {
    type Kind = PlayerRpcKind;
    type Cx<'a> = infer_bundle!('a);
    type RpcRoot = PlayerServerState;

    fn catchup(
        cx: Bundle<Self::Cx<'_>>,
        peer: PeerId,
        target: Obj<Self::RpcRoot>,
    ) -> RpcServerCup<Self> {
        let static ..cx;

        let _ = peer;

        PlayerRpcCatchup {
            name: target.name.clone(),
        }
    }

    fn process(
        cx: Bundle<Self::Cx<'_>>,
        target: Obj<Self::RpcRoot>,
        sender: PeerId,
        packet: RpcServerSb<Self>,
    ) -> anyhow::Result<()> {
        todo!()
    }
}

// === Components === //

#[derive(Debug)]
pub struct PlayerServerState {
    name: String,
}

component!(PlayerServerState);

// === Prefabs === //

pub fn spawn_player(parent: Entity) -> Entity {
    let player = Entity::new(parent)
        .with(RpcNode::new::<PlayerRpcKind>())
        .with(PlayerServerState {
            name: "player_mc_playerface".to_string(),
        });

    Entity::service::<NetManager>().register(player.get());

    player
}
