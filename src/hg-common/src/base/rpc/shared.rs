use std::{any::TypeId, borrow::Cow, num::NonZeroU64};

use derive_where::derive_where;
use hg_ecs::component;
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

#[derive(Debug, Copy, Clone)]
#[derive_where(Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct RpcKindId {
    ty: TypeId,
    #[derive_where(skip)]
    id: &'static str,
}

impl RpcKindId {
    pub fn of<K: RpcKind>() -> Self {
        Self {
            ty: TypeId::of::<K>(),
            id: K::ID,
        }
    }

    pub fn ty(self) -> TypeId {
        self.ty
    }

    pub fn id(self) -> &'static str {
        self.id
    }
}

// === RpcNode === //

#[derive(Debug)]
pub struct RpcNode {
    pub(crate) kind: RpcKindId,
    pub(crate) id: Option<RpcNodeId>,
}

component!(RpcNode);

impl RpcNode {
    pub fn new<K: RpcKind>() -> Self {
        Self::new_raw(RpcKindId::of::<K>())
    }

    pub fn new_raw(kind: RpcKindId) -> Self {
        Self { kind, id: None }
    }

    pub fn kind(&self) -> RpcKindId {
        self.kind
    }

    pub fn opt_id(&self) -> Option<RpcNodeId> {
        self.id
    }

    pub fn expect_id(&self) -> RpcNodeId {
        self.id.unwrap()
    }
}
