use std::{
    any::{type_name, TypeId},
    context::{pack, Bundle},
    fmt,
    marker::PhantomData,
    mem,
};

use anyhow::Context as _;
use bytes::Bytes;
use derive_where::derive_where;
use hg_ecs::{bind, component, entity::Component, AccessComp, Entity, Index, Obj, World, WORLD};
use hg_utils::hash::{hash_map, FxHashMap};

use crate::base::{
    net::{FrameEncoder, MultiPartDecoder, MultiPartSerializeExt as _, RpcPacket as _},
    rpc::RpcSbHeader,
};

use super::{RpcCbHeader, RpcKind, RpcNodeId};

// === RpcKind === //

pub type RpcClientCup<K> = <<K as RpcClientReplicator>::Kind as RpcKind>::Catchup;
pub type RpcClientCb<K> = <<K as RpcClientReplicator>::Kind as RpcKind>::ClientBound;
pub type RpcClientSb<K> = <<K as RpcClientReplicator>::Kind as RpcKind>::ServerBound;

pub struct RpcClientCreate<'t, R>
where
    R: RpcClientReplicator,
{
    _ty: PhantomData<(fn(&'t ()) -> &'t (), fn() -> R)>,
    client: Obj<RpcClient>,
    id: RpcNodeId,
}

impl<'t, R> RpcClientCreate<'t, R>
where
    R: RpcClientReplicator,
{
    pub fn client_ent(&self) -> Entity {
        self.client.entity()
    }

    pub fn client_obj(&self) -> Obj<RpcClient> {
        self.client
    }

    pub fn id(&self) -> RpcNodeId {
        self.id
    }

    pub fn finish(self, target: Obj<R>, cx: Bundle<&AccessComp<R>>) -> RpcClientFinished<'t, R> {
        let node = target.entity(pack!(cx));
        let client_handle = node.add(RpcClientNode {
            kind: TypeId::of::<R::Kind>(),
            vtable: <R as HasKindVtable>::VTABLE,
            node_id: self.id,
            userdata: Obj::raw(target),
            client: self.client,
        });

        RpcClientFinished {
            _ty: PhantomData,
            rpc: RpcClientHandle::wrap(client_handle),
        }
    }
}

pub struct RpcClientFinished<'t, R>
where
    R: RpcClientReplicator,
{
    _ty: PhantomData<fn(&'t ()) -> &'t ()>,
    rpc: RpcClientHandle<R::Kind>,
}

impl<'t, R> RpcClientFinished<'t, R>
where
    R: RpcClientReplicator,
{
    pub fn rpc(&self) -> RpcClientHandle<R::Kind> {
        self.rpc
    }
}

pub trait RpcClientReplicator: Component {
    type Kind: RpcKind;

    fn create<'t>(
        world: &mut World,
        req: RpcClientCreate<'t, Self>,
        packet: RpcClientCup<Self>,
    ) -> anyhow::Result<RpcClientFinished<'t, Self>>;

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

impl<K: RpcClientReplicator> HasKindVtable for K {
    const VTABLE: KindVtableRef = &KindVtable {
        create: |world, client, target, packet| {
            let node = K::create(
                world,
                RpcClientCreate {
                    _ty: PhantomData,
                    client,
                    id: target,
                },
                RpcClientCup::<K>::decode(&packet)?,
            )?;

            Ok(node.rpc().raw())
        },
        destroy: |world, target| {
            bind!(world);
            let userdata = target.userdata::<K>();
            K::destroy(userdata, &mut WORLD)
        },
        message: |world, target, packet| {
            bind!(world);
            let packet = RpcClientCb::<K>::decode(&packet)?;
            let userdata = target.userdata::<K>();
            K::process(userdata, &mut WORLD, packet)?;

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

    pub fn define<K: RpcClientReplicator>(&mut self) -> &mut Self {
        let kind_id = TypeId::of::<K::Kind>();
        let hash_map::Entry::Vacant(ty_entry) = self.kinds_by_type.entry(kind_id) else {
            panic!(
                "RPC kind {:?} registered more than once",
                type_name::<K::Kind>()
            );
        };

        let hash_map::Entry::Vacant(name_entry) =
            self.kinds_by_name.entry(<K::Kind as RpcKind>::ID)
        else {
            panic!(
                "RPC kind name {:?} registered more than once",
                <K::Kind as RpcKind>::ID,
            );
        };

        let vtable = <K as HasKindVtable>::VTABLE;
        ty_entry.insert(vtable);
        name_entry.insert(vtable);

        self
    }

    pub fn lookup_node(&self, id: RpcNodeId) -> Option<Obj<RpcClientNode>> {
        self.id_to_node.get(&id).copied()
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
                    .lookup_node(target_id)
                    .with_context(|| format!("node with ID {target_id:?} does not exist"))?;

                (target.vtable.message)(&mut WORLD, target, data)
            }
            RpcCbHeader::CreateNode(target_id, kind_name) => {
                let data = packet.expect().context("failed to parse RPC data")?;

                if self.lookup_node(target_id).is_some() {
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
                    .lookup_node(target_id)
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

// === RpcClientNode === //

#[derive(Debug)]
pub struct RpcClientNode {
    client: Obj<RpcClient>,
    kind: TypeId,
    node_id: RpcNodeId,
    vtable: KindVtableRef,
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
        assert_eq!(self.kind, TypeId::of::<K>());

        let mut encoder = FrameEncoder::new();
        encoder.encode_multi_part(packet);
        encoder.encode_multi_part(&RpcSbHeader::SendMessage(self.node_id));

        self.client.queued_packets.push(encoder);
    }

    pub fn userdata<K: RpcClientReplicator>(self: Obj<Self>) -> Obj<K> {
        debug_assert_eq!(self.kind, TypeId::of::<K::Kind>());
        Obj::from_raw(self.userdata)
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

    pub fn id(self) -> RpcNodeId {
        self.raw().node_id
    }

    pub fn client(self) -> Obj<RpcClient> {
        self.raw().client()
    }

    pub fn send(self, packet: &K::ServerBound) {
        self.raw().send::<K>(packet);
    }
}
