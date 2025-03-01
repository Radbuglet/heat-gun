use std::{borrow::Cow, num::NonZeroU64};

use serde::{Deserialize, Serialize};

use crate::base::net::RpcPacket;

// === Protocol === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RpcNodeId(pub NonZeroU64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RpcSbHeader {
    SendMessage(RpcNodeId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RpcCbHeader {
    SendMessage(RpcNodeId),
    CreateNode(RpcNodeId, Cow<'static, str>),
    DeleteNode(RpcNodeId),
}

// === RpcKind === //

pub trait RpcKind: Sized + 'static {
    const ID: &'static str;

    type Catchup: RpcPacket;
    type ServerBound: RpcPacket;
    type ClientBound: RpcPacket;
}
