use std::{
    any::type_name,
    context::{pack, Bundle, BundleItemSetFor},
    fmt,
};

use anyhow::Context as _;
use bytes::Bytes;
use hg_ecs::{bind, component, entity::Component, AccessComp, Index, Obj, World, WORLD};
use hg_utils::hash::{hash_map, FxHashMap};

use crate::base::{
    net::{
        codec::FrameEncoder,
        serialize::{MultiPartDecoder, MultiPartSerializeExt as _, RpcPacket as _},
    },
    rpc::{RpcNode, RpcSbHeader},
};

use super::{RpcCbHeader, RpcKind, RpcKindId, RpcNodeId};

// === RpcKind === //

pub type RpcClientCup<K> = <<K as RpcKindClient>::Kind as RpcKind>::Catchup;
pub type RpcClientCb<K> = <<K as RpcKindClient>::Kind as RpcKind>::ClientBound;
pub type RpcClientSb<K> = <<K as RpcKindClient>::Kind as RpcKind>::ServerBound;

pub trait RpcKindClient: Sized + 'static {
    type Kind: RpcKind;

    type Cx<'a>: BundleItemSetFor<'a>;
    type RpcRoot: Component;

    fn create(
        cx: Bundle<Self::Cx<'_>>,
        client: Obj<RpcClient>,
        id: RpcNodeId,
        packet: RpcClientCup<Self>,
    ) -> anyhow::Result<Obj<Self::RpcRoot>>;

    fn destroy(cx: Bundle<Self::Cx<'_>>, target: Obj<Self::RpcRoot>) -> anyhow::Result<()>;

    fn process(
        cx: Bundle<Self::Cx<'_>>,
        target: Obj<Self::RpcRoot>,
        packet: RpcClientCb<Self>,
    ) -> anyhow::Result<()>;
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

impl<K: RpcKindClient> HasKindVtable for K {
    const VTABLE: KindVtableRef = &KindVtable {
        create: |world, client, target, packet| {
            // Deserialize the packet
            let packet = RpcClientCup::<K>::decode(&packet)?;

            // Create the node
            let userdata = {
                bind!(world, let cx: _);
                K::create(cx, client, target, packet)?
            };

            // Create its book-keeping components.
            bind!(world, let cx: &AccessComp<K::RpcRoot>);
            let node = userdata.entity(pack!(cx));
            let kind_id = RpcKindId::of::<K::Kind>();
            node.add(RpcNode {
                kind: kind_id,
                id: Some(target),
            });
            let client_handle = node.add(RpcNodeClient {
                kind: kind_id,
                vtable: <K as HasKindVtable>::VTABLE,
                node_id: target,
                userdata: Obj::raw(userdata),
                client,
            });

            Ok(client_handle)
        },
        destroy: |world, target| {
            let userdata = {
                bind!(world, let cx: _);
                target.userdata::<K>(cx)
            };

            bind!(world, let cx: _);
            K::destroy(cx, userdata)
        },
        message: |world, target, packet| {
            // Deserialize the packet
            let packet = RpcClientCb::<K>::decode(&packet)?;

            // Fetch the RPC node userdata
            let userdata = {
                bind!(world, let cx: _);
                target.userdata::<K>(cx)
            };

            // Process the packet
            bind!(world, let cx: _);
            K::process(cx, userdata, packet)?;

            Ok(())
        },
    };
}

// === RpcClient === //

#[derive(Debug, Default)]
pub struct RpcClient {
    kinds_by_type: FxHashMap<RpcKindId, KindVtableRef>,
    kinds_by_name: FxHashMap<&'static str, KindVtableRef>,
    id_to_node: FxHashMap<RpcNodeId, Obj<RpcNodeClient>>,
}

component!(RpcClient);

impl RpcClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn define<K: RpcKindClient>(&mut self) -> &mut Self {
        let kind_id = RpcKindId::of::<K::Kind>();
        let hash_map::Entry::Vacant(ty_entry) = self.kinds_by_type.entry(kind_id) else {
            panic!(
                "RPC kind {:?} registered more than once",
                type_name::<K::Kind>()
            );
        };

        let hash_map::Entry::Vacant(name_entry) = self.kinds_by_name.entry(kind_id.id()) else {
            panic!("RPC kind name {:?} registered more than once", kind_id.id(),);
        };

        let vtable = <K as HasKindVtable>::VTABLE;
        ty_entry.insert(vtable);
        name_entry.insert(vtable);

        self
    }

    pub fn lookup_node(&self, id: RpcNodeId) -> Option<Obj<RpcNodeClient>> {
        self.id_to_node.get(&id).copied()
    }

    #[must_use]
    pub fn send_packet<K: RpcKind>(
        self: Obj<Self>,
        target: Obj<RpcNodeClient>,
        packet: K::ClientBound,
    ) -> FrameEncoder {
        assert_eq!(target.kind, RpcKindId::of::<K>());

        let mut encoder = FrameEncoder::new();
        encoder.data_mut().encode_multi_part(&packet);
        encoder
            .data_mut()
            .encode_multi_part(&RpcSbHeader::SendMessage(target.node_id));

        encoder
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
}

// === RpcNodeClient === //

#[derive(Debug)]
pub struct RpcNodeClient {
    kind: RpcKindId,
    vtable: KindVtableRef,
    node_id: RpcNodeId,
    userdata: Index,
    client: Obj<RpcClient>,
}

component!(RpcNodeClient);

impl RpcNodeClient {
    pub fn kind(&self) -> RpcKindId {
        self.kind
    }

    pub fn id(&self) -> RpcNodeId {
        self.node_id
    }

    pub fn client(&self) -> Obj<RpcClient> {
        self.client
    }

    fn userdata<K: RpcKindClient>(
        mut self: Obj<Self>,
        cx: Bundle<&AccessComp<K::RpcRoot>>,
    ) -> Obj<K::RpcRoot> {
        debug_assert_eq!(self.kind(), RpcKindId::of::<K::Kind>());

        if self.userdata == Index::DANGLING {
            self.userdata = Obj::raw(self.entity().get::<K::RpcRoot>(pack!(cx)));
        }
        Obj::from_raw(self.userdata)
    }
}
