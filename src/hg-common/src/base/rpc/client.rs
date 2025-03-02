use std::{
    any::{type_name, TypeId},
    context::pack,
    fmt, mem,
};

use anyhow::Context as _;
use bytes::Bytes;
use hg_ecs::{bind, component, entity::Component, AccessComp, Index, Obj, World, WORLD};
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

pub trait RpcClientReplicator: Component {
    type Kind: RpcKind;

    fn create(
        world: &mut World,
        client: Obj<RpcClient>,
        id: RpcNodeId,
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
    create: fn(&mut World, Obj<RpcClient>, RpcNodeId, Bytes) -> anyhow::Result<Obj<RpcNodeClient>>,
    destroy: fn(&mut World, Obj<RpcNodeClient>) -> anyhow::Result<()>,
    message: fn(&mut World, Obj<RpcNodeClient>, Bytes) -> anyhow::Result<()>,
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
            // Deserialize the packet
            let packet = RpcClientCup::<K>::decode(&packet)?;

            // Create the node
            let userdata = {
                bind!(world);
                K::create(&mut WORLD, client, target, packet)?
            };

            // Create its book-keeping components.
            bind!(world, let cx: &AccessComp<K>);
            let node = userdata.entity(pack!(cx));
            let client_handle = node.add(RpcNodeClient {
                kind: TypeId::of::<K::Kind>(),
                vtable: <K as HasKindVtable>::VTABLE,
                node_id: target,
                userdata: Obj::raw(userdata),
                client,
            });

            Ok(client_handle)
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
    id_to_node: FxHashMap<RpcNodeId, Obj<RpcNodeClient>>,
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

    pub fn lookup_node(&self, id: RpcNodeId) -> Option<Obj<RpcNodeClient>> {
        self.id_to_node.get(&id).copied()
    }

    pub fn send_packet<K: RpcKind>(
        mut self: Obj<Self>,
        target: Obj<RpcNodeClient>,
        packet: K::ServerBound,
    ) {
        assert_eq!(target.kind, TypeId::of::<K>());

        let mut encoder = FrameEncoder::new();
        encoder.encode_multi_part(&packet);
        encoder.encode_multi_part(&RpcSbHeader::SendMessage(target.node_id));

        self.queued_packets.push(encoder);
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

// === RpcNodeClient === //

#[derive(Debug)]
pub struct RpcNodeClient {
    kind: TypeId,
    vtable: KindVtableRef,
    node_id: RpcNodeId,
    userdata: Index,
    client: Obj<RpcClient>,
}

component!(RpcNodeClient);

impl RpcNodeClient {
    pub fn id(&self) -> RpcNodeId {
        self.node_id
    }

    pub fn client(&self) -> Obj<RpcClient> {
        self.client
    }

    pub fn userdata<K: RpcClientReplicator>(self: Obj<Self>) -> Obj<K> {
        debug_assert_eq!(self.kind, TypeId::of::<K::Kind>());
        Obj::from_raw(self.userdata)
    }
}
