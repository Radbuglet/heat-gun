use std::context::{infer_bundle, Bundle};

use hg_common::base::{
    net::transport::PeerId,
    rpc::{RpcKind, RpcKindServer, RpcServerCup, RpcServerSb},
};
use hg_ecs::{component, Obj};
use serde::{Deserialize, Serialize};

// === Rpc === //

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerRpcCatchup {
    pub name: String,
}

pub struct PlayerRpcKind;

impl RpcKind for PlayerRpcKind {
    const ID: &'static str = "player";

    type Catchup = PlayerRpcCatchup;
    type ServerBound = ();
    type ClientBound = ();
}

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
