use glam::Vec2;
use serde::{Deserialize, Serialize};

use hg_engine_common::rpc::{RpcKind, RpcNodeId};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayerPuppetRpcSb {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayerPuppetRpcCb {
    SetPos(Vec2),
}

pub struct PlayerPuppetRpcKind;

impl RpcKind for PlayerPuppetRpcKind {
    const ID: &'static str = "player_puppet";

    type Catchup = RpcNodeId;
    type ServerBound = PlayerPuppetRpcSb;
    type ClientBound = PlayerPuppetRpcCb;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayerOwnerRpcSb {
    SetPos(Vec2),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayerOwnerRpcCb {}

pub struct PlayerOwnerRpcKind;

impl RpcKind for PlayerOwnerRpcKind {
    const ID: &'static str = "player_owner";

    type Catchup = RpcNodeId;
    type ServerBound = PlayerOwnerRpcSb;
    type ClientBound = PlayerOwnerRpcCb;
}
