use std::{
    any::{type_name, TypeId},
    context::pack,
    fmt,
    marker::PhantomData,
    mem,
};

use anyhow::Context as _;
use bytes::Bytes;
use derive_where::derive_where;
use hg_ecs::{bind, component, entity::Component, AccessComp, Entity, Index, Obj, World, WORLD};
use hg_utils::hash::{hash_map, FxHashMap};
use thiserror::Error;

use crate::base::{
    net::{FrameEncoder, MultiPartDecoder, MultiPartSerializeExt as _, RpcPacket as _},
    rpc::RpcSbHeader,
};

use super::{RpcCbHeader, RpcKind, RpcNodeId};

// === RpcKind === //

pub type RpcClientCup<K> = <<K as RpcClientReplicator>::Kind as RpcKind>::Catchup;
pub type RpcClientCb<K> = <<K as RpcClientReplicator>::Kind as RpcKind>::ClientBound;
pub type RpcClientSb<K> = <<K as RpcClientReplicator>::Kind as RpcKind>::ServerBound;

pub trait RpcClientReplicator: Component {
    type Kind: RpcKind;

    fn create<'t>(
        world: &mut World,
        me: RpcClientHandle<Self::Kind>,
        packet: RpcClientCup<Self>,
    ) -> anyhow::Result<Obj<Self>>;

    fn process(self: Obj<Self>, world: &mut World, packet: RpcClientCb<Self>)
        -> anyhow::Result<()>;

    fn destroy(self: Obj<Self>, world: &mut World) -> anyhow::Result<()> {
        bind!(world, let cx: &AccessComp<Self>);
        self.entity(pack!(cx)).destroy();
        Ok(())
    }
}

type KindVtableRef = &'static KindVtable;

struct KindVtable {
    create: fn(&mut World, Obj<RpcClient>, RpcNodeId, Bytes) -> anyhow::Result<Obj<RpcClientNode>>,
    destroy: fn(&mut World, Obj<RpcClientNode>) -> anyhow::Result<()>,
    message: fn(&mut World, Obj<RpcClientNode>, Bytes) -> anyhow::Result<()>,
}

impl fmt::Debug for KindVtable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KindVtableInner").finish_non_exhaustive()
    }
}

trait HasKindVtable {
    const VTABLE: KindVtableRef;
}

impl<T> HasKindVtable for T
where
    T: RpcClientReplicator,
{
    const VTABLE: KindVtableRef = &KindVtable {
        create: |world, client, target, packet| {
            // Spawn an entity with a destruction guard to ensure that it gets destroyed on
            // replication failure.
            let mut guard =
                scopeguard::guard((world, Option::<Entity>::None), |(world, target)| {
                    let Some(target) = target else {
                        return;
                    };

                    bind!(world);
                    target.destroy();
                });

            let (world, destroy_target) = &mut *guard;
            bind!(world);

            let me = Entity::new(client.entity());
            *destroy_target = Some(me);

            // Spawn the RPC node
            let mut me = me.add(RpcClientNode {
                vtable: <T as HasKindVtable>::VTABLE,
                client,
                node_id: target,
                userdata_ty: TypeId::of::<T>(),
                userdata: Index::DANGLING,
            });

            let node = T::create(
                &mut WORLD,
                RpcClientHandle::wrap(me),
                RpcClientCup::<T>::decode(&packet)?,
            )?;

            me.userdata = Obj::raw(node);

            // Defuse the destruction guard
            *destroy_target = None;

            Ok(me)
        },
        destroy: |world, target| {
            bind!(world);
            let userdata = target.userdata::<T>();
            T::destroy(userdata, &mut WORLD)
        },
        message: |world, target, packet| {
            bind!(world);
            let packet = RpcClientCb::<T>::decode(&packet)?;
            let userdata = target.userdata::<T>();
            T::process(userdata, &mut WORLD, packet)?;

            Ok(())
        },
    };
}

// === RpcClient === //

#[derive(Debug, Default)]
pub struct RpcClient {
    kinds_by_type: FxHashMap<TypeId, KindVtableRef>,
    kinds_by_name: FxHashMap<&'static str, KindVtableRef>,
    id_to_node: FxHashMap<RpcNodeId, Obj<RpcClientNode>>,
    queued_packets: Vec<FrameEncoder>,
}

component!(RpcClient);

impl RpcClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn define<T>(&mut self) -> &mut Self
    where
        T: RpcClientReplicator,
    {
        let kind_id = TypeId::of::<T::Kind>();
        let hash_map::Entry::Vacant(ty_entry) = self.kinds_by_type.entry(kind_id) else {
            panic!(
                "RPC kind {:?} registered more than once",
                type_name::<T::Kind>()
            );
        };

        let hash_map::Entry::Vacant(name_entry) =
            self.kinds_by_name.entry(<T::Kind as RpcKind>::ID)
        else {
            panic!(
                "RPC kind name {:?} registered more than once",
                <T::Kind as RpcKind>::ID,
            );
        };

        let vtable = <T as HasKindVtable>::VTABLE;
        ty_entry.insert(vtable);
        name_entry.insert(vtable);

        self
    }

    pub fn lookup_any_node(&self, id: RpcNodeId) -> Option<Obj<RpcClientNode>> {
        self.id_to_node.get(&id).copied()
    }

    pub fn lookup_node<T>(&self, id: RpcNodeId) -> Result<Obj<T>, LookupNodeError>
    where
        T: RpcClientReplicator,
    {
        self.id_to_node
            .get(&id)
            .ok_or(LookupNodeError::Missing(id))?
            .opt_userdata()
            .ok_or(LookupNodeError::WrongType(id))
    }

    pub fn recv_packet(mut self: Obj<Self>, packet: Bytes) -> anyhow::Result<()> {
        let mut packet = MultiPartDecoder::new(packet);

        let header = packet
            .expect_rich::<RpcCbHeader>()
            .context("failed to parse RPC header")?;

        match header {
            RpcCbHeader::SendMessage(target_id) => {
                let data = packet.expect().context("failed to parse RPC data")?;

                let target = self
                    .lookup_any_node(target_id)
                    .with_context(|| format!("node with ID {target_id:?} does not exist"))?;

                (target.vtable.message)(&mut WORLD, target, data)
            }
            RpcCbHeader::CreateNode(target_id, kind_name) => {
                let data = packet.expect().context("failed to parse RPC data")?;

                if self.lookup_any_node(target_id).is_some() {
                    anyhow::bail!("node with ID {target_id:?} already exists");
                }

                let vtable = *self
                    .kinds_by_name
                    .get(&*kind_name)
                    .with_context(|| format!("kind with ID {kind_name:?} was never registered"))?;

                let node = (vtable.create)(&mut WORLD, self, target_id, data)?;

                self.id_to_node.insert(target_id, node);

                Ok(())
            }
            RpcCbHeader::DeleteNode(target_id) => {
                let target = self
                    .lookup_any_node(target_id)
                    .with_context(|| format!("node with ID {target_id:?} does not exist"))?;

                (target.vtable.destroy)(&mut WORLD, target)?;

                self.id_to_node.remove(&target_id);

                Ok(())
            }
        }
    }

    pub fn flush_sends(mut self: Obj<Self>) -> Vec<FrameEncoder> {
        mem::take(&mut self.queued_packets)
    }
}

#[derive(Debug, Clone, Error)]
pub enum LookupNodeError {
    #[error("node with ID {0:?} does not exist")]
    Missing(RpcNodeId),
    #[error("node with ID {0:?} has the wrong type")]
    WrongType(RpcNodeId),
}

// === RpcClientNode === //

#[derive(Debug)]
pub struct RpcClientNode {
    client: Obj<RpcClient>,
    node_id: RpcNodeId,
    vtable: KindVtableRef,
    userdata_ty: TypeId,
    userdata: Index,
}

component!(RpcClientNode);

impl RpcClientNode {
    pub fn id(&self) -> RpcNodeId {
        self.node_id
    }

    pub fn client(&self) -> Obj<RpcClient> {
        self.client
    }

    pub fn send<K: RpcKind>(mut self: Obj<Self>, packet: &K::ServerBound) {
        assert_eq!(self.userdata_ty, TypeId::of::<K>());

        let mut encoder = FrameEncoder::new();
        encoder.encode_multi_part(packet);
        encoder.encode_multi_part(&RpcSbHeader::SendMessage(self.node_id));

        self.client.queued_packets.push(encoder);
    }

    pub fn opt_userdata<T>(self: Obj<Self>) -> Option<Obj<T>>
    where
        T: RpcClientReplicator,
    {
        (self.userdata_ty == TypeId::of::<T>()).then_some(Obj::from_raw(self.userdata))
    }

    pub fn userdata<T>(self: Obj<Self>) -> Obj<T>
    where
        T: RpcClientReplicator,
    {
        self.opt_userdata().unwrap()
    }
}

// === RpcClientHandle === //

#[derive_where(Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct RpcClientHandle<K> {
    _ty: PhantomData<fn(K) -> K>,
    raw: Obj<RpcClientNode>,
}

impl<K: RpcKind> fmt::Debug for RpcClientHandle<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple(&format!("RpcClientHandle<{}>", type_name::<K>()))
            .field(&self.raw)
            .finish()
    }
}

impl<K: RpcKind> RpcClientHandle<K> {
    pub const DANGLING: Self = Self::wrap(Obj::DANGLING);

    pub const fn wrap(target: Obj<RpcClientNode>) -> Self {
        Self {
            _ty: PhantomData,
            raw: target,
        }
    }

    pub fn raw(self) -> Obj<RpcClientNode> {
        self.raw
    }

    pub fn entity(self) -> Entity {
        self.raw.entity()
    }

    pub fn id(self) -> RpcNodeId {
        self.raw().node_id
    }

    pub fn client(self) -> Obj<RpcClient> {
        self.raw().client()
    }

    pub fn send(self, packet: &K::ServerBound) {
        self.raw().send::<K>(packet);
    }

    pub fn userdata<T>(self) -> Obj<T>
    where
        T: RpcClientReplicator<Kind = K>,
    {
        self.raw().userdata()
    }
}
