use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::base::rpc::RpcKind;

// === Rpc === //

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerRpcCatchup {
    pub name: String,
    pub pos: Vec2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayerRpcSb {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayerRpcCb {}

pub struct PlayerRpcKind;

impl RpcKind for PlayerRpcKind {
    const ID: &'static str = "player";

    type Catchup = PlayerRpcCatchup;
    type ServerBound = PlayerRpcSb;
    type ClientBound = PlayerRpcCb;
}
