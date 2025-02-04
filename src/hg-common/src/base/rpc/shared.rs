use std::{any::TypeId, fmt, num::NonZeroU64};

use derive_where::derive_where;
use hg_ecs::{component, Entity, Index};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

// === Newtypes === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct RpcPeer(pub Entity);

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct RpcNodeId(pub NonZeroU64);

// === RpcPacket === //

pub trait RpcPacket:
    'static + Sized + Send + Sync + Clone + fmt::Debug + Serialize + DeserializeOwned
{
}

impl<T> RpcPacket for T where
    T: 'static + Sized + Send + Sync + Clone + fmt::Debug + Serialize + DeserializeOwned
{
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
    pub(crate) id: RpcNodeId,
    pub(crate) managed_state: Index,
}

component!(RpcNode);

impl RpcNode {
    pub fn kind(&self) -> RpcKindId {
        self.kind
    }

    pub fn id(&self) -> RpcNodeId {
        self.id
    }
}
