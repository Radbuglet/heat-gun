use std::{any::TypeId, fmt, num::NonZeroU64};

use anyhow::Context;
use bytes::BytesMut;
use derive_where::derive_where;
use hg_ecs::{component, Entity};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::utils::lang::ExtendMutAdapter;

// === Newtypes === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct RpcPeer(pub Entity);

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RpcNodeId(pub NonZeroU64);

// === RpcPacket === //

pub trait RpcPacket:
    'static + Sized + Send + Sync + Clone + fmt::Debug + Serialize + DeserializeOwned
{
    fn decode(data: &[u8]) -> anyhow::Result<Self>;

    fn encode(&self, out: &mut BytesMut);
}

impl<T> RpcPacket for T
where
    T: 'static + Sized + Send + Sync + Clone + fmt::Debug + Serialize + DeserializeOwned,
{
    fn decode(data: &[u8]) -> anyhow::Result<Self> {
        postcard::from_bytes(data).context("failed to deserialize packet")
    }

    fn encode(&self, out: &mut BytesMut) {
        postcard::to_extend(self, ExtendMutAdapter(out)).unwrap();
    }
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
