use std::{borrow::Cow, num::NonZeroU64, u64};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{net::RpcPacket, utils::lang::NamedTypeId};

// === Errors === //

#[derive(Debug, Clone, Error)]
#[error(transparent)]
pub enum RpcNodeLookupError {
    NoSuchNode(NoSuchRpcNodeError),
    BadKind(BadRpcNodeKindError),
}

impl From<NoSuchRpcNodeError> for RpcNodeLookupError {
    fn from(value: NoSuchRpcNodeError) -> Self {
        Self::NoSuchNode(value)
    }
}

impl From<BadRpcNodeKindError> for RpcNodeLookupError {
    fn from(value: BadRpcNodeKindError) -> Self {
        Self::BadKind(value)
    }
}

#[derive(Debug, Clone, Error)]
#[error("RPC node {id:?} does not exist")]
pub struct NoSuchRpcNodeError {
    pub id: RpcNodeId,
}

#[derive(Debug, Clone, Error)]
#[error("expected RPC node {id:?} to have type {expected_ty}, got {actual_ty}")]
pub struct BadRpcNodeKindError {
    pub id: RpcNodeId,
    pub expected_ty: NamedTypeId,
    pub actual_ty: NamedTypeId,
}

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
    CreateNode(RpcNodeId, Cow<'static, str>),
    DeleteNode(RpcNodeId),
    SendMessage(RpcNodeId),
}

// === RpcKind === //

pub trait RpcKind: Sized + 'static {
    const ID: &'static str;

    type Catchup: RpcPacket;
    type ServerBound: RpcPacket;
    type ClientBound: RpcPacket;
}
