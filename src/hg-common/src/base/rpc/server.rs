use std::{
    any::type_name,
    borrow::Cow,
    context::{pack, Bundle, BundleItemSetFor},
    fmt, mem,
    num::NonZeroU64,
};

use anyhow::Context;
use bytes::Bytes;
use hg_ecs::{bind, component, entity::Component, AccessComp, Index, Obj, World, WORLD};
use hg_utils::hash::{hash_map, FxHashMap, FxHashSet};

use crate::{
    base::net::{
        codec::FrameEncoder,
        serialize::{MultiPartDecoder, MultiPartSerializeExt as _, RpcPacket as _},
        transport::PeerId,
    },
    field,
    utils::lang::{steal_from_ecs, Steal},
};

use super::{RpcCbHeader, RpcKind, RpcKindId, RpcNode, RpcNodeId, RpcSbHeader};

// === RpcKind === //

pub type RpcServerCup<K> = <<K as RpcKindServer>::Kind as RpcKind>::Catchup;
pub type RpcServerCb<K> = <<K as RpcKindServer>::Kind as RpcKind>::ClientBound;
pub type RpcServerSb<K> = <<K as RpcKindServer>::Kind as RpcKind>::ServerBound;

pub trait RpcKindServer: Sized + 'static {
    type Kind: RpcKind;

    type Cx<'a>: BundleItemSetFor<'a>;
    type RpcRoot: Component;

    fn catchup(
        cx: Bundle<Self::Cx<'_>>,
        peer: PeerId,
        target: Obj<Self::RpcRoot>,
    ) -> RpcServerCup<Self>;

    fn process(
        cx: Bundle<Self::Cx<'_>>,
        target: Obj<Self::RpcRoot>,
        sender: PeerId,
        packet: RpcServerSb<Self>,
    ) -> anyhow::Result<()>;
}

type KindVtableRef = &'static KindVtable;

struct KindVtable {
    process_inbound: fn(&mut World, Obj<RpcNodeServer>, PeerId, Bytes) -> anyhow::Result<()>,
    drain_replications: fn(&mut World, &mut KindData),
}

impl fmt::Debug for KindVtable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KindVtableInner").finish_non_exhaustive()
    }
}

trait HasKindVtable {
    const VTABLE: KindVtableRef;
}

impl<K: RpcKindServer> HasKindVtable for K {
    const VTABLE: KindVtableRef = &KindVtable {
        process_inbound: |world, target, peer, packet| {
            // Deserialize the packet
            let packet = RpcServerSb::<K>::decode(&packet)?;

            // Fetch the RPC node userdata
            let userdata = {
                bind!(world, let cx: _);
                target.userdata::<K>(cx)
            };

            // Process the packet
            bind!(world, let cx: _);
            K::process(cx, userdata, peer, packet)?;

            Ok(())
        },
        drain_replications: |world, kind_data| {
            // Process replications
            for (target, peer) in kind_data.now_visible.drain() {
                // Fetch the RPC node userdata
                let (target_id, userdata) = {
                    bind!(world, let cx: _);

                    (target.node_id, target.userdata::<K>(cx))
                };

                // Produce the catchup structure
                let catchup = {
                    bind!(world, let cx: _);
                    K::catchup(cx, peer, userdata)
                };

                // Serialize the catchup structure
                let mut encoder = FrameEncoder::new();

                encoder.data_mut().encode_multi_part(&catchup);
                encoder
                    .data_mut()
                    .encode_multi_part(&RpcCbHeader::CreateNode(
                        target_id,
                        Cow::Borrowed(<K::Kind as RpcKind>::ID),
                    ));

                kind_data.packet_replicate_queue.push((peer, encoder));
            }

            // Process de-replications
            {
                bind!(world);

                for (target, peer) in kind_data.now_invisible.drain() {
                    let mut encoder = FrameEncoder::new();
                    let target_id = target.node_id;

                    encoder
                        .data_mut()
                        .encode_multi_part(&RpcCbHeader::DeleteNode(target_id));

                    kind_data.packet_de_replicate_queue.push((peer, encoder));
                }
            }
        },
    };
}

// === RpcServer === //

#[derive(Debug)]
pub struct RpcServer {
    kinds: Steal<FxHashMap<RpcKindId, KindData>>,
    id_to_node: FxHashMap<RpcNodeId, Obj<RpcNodeServer>>,
    id_gen: RpcNodeId,
}

#[derive(Debug)]
struct KindData {
    vtable: KindVtableRef,

    /// Fully-framed packets produced by explicit calls to `send_packet`.
    packet_send_queue: Vec<(Obj<RpcNodeServer>, FrameEncoder)>,

    /// The set of nodes now visible to given target peers. The `target`s' `visible_to` sets are
    /// guaranteed to not contain the peer already.
    now_visible: FxHashSet<(Obj<RpcNodeServer>, PeerId)>,

    /// The set of nodes now invisible to given target peers. The `target`s' `visible_to` sets are
    /// guaranteed to already contain the peer.
    now_invisible: FxHashSet<(Obj<RpcNodeServer>, PeerId)>,

    // TODO: document
    packet_replicate_queue: Vec<(PeerId, FrameEncoder)>,

    // TODO: document
    packet_de_replicate_queue: Vec<(PeerId, FrameEncoder)>,
}

component!(RpcServer);

impl Default for RpcServer {
    fn default() -> Self {
        Self::new()
    }
}

impl RpcServer {
    pub fn new() -> Self {
        Self {
            kinds: Steal::default(),
            id_to_node: FxHashMap::default(),
            id_gen: RpcNodeId(NonZeroU64::new(1).unwrap()),
        }
    }

    pub fn define<K: RpcKindServer>(&mut self) -> &mut Self {
        let hash_map::Entry::Vacant(entry) = self.kinds.entry(RpcKindId::of::<K::Kind>()) else {
            panic!(
                "RPC kind {:?} registered more than once",
                type_name::<K::Kind>()
            );
        };

        entry.insert(KindData {
            vtable: <K as HasKindVtable>::VTABLE,
            packet_send_queue: Vec::new(),
            now_visible: FxHashSet::default(),
            now_invisible: FxHashSet::default(),
            packet_replicate_queue: Vec::new(),
            packet_de_replicate_queue: Vec::new(),
        });

        self
    }

    pub fn register(mut self: Obj<Self>, mut node: Obj<RpcNode>) -> Obj<RpcNodeServer> {
        assert_eq!(node.opt_id(), None);

        // Ensure that the node has a valid kind
        let kind = node.kind();
        let vtable = self
            .kinds
            .get(&kind)
            .unwrap_or_else(|| panic!("RPC node with kind {:?} was never registered", kind.id()))
            .vtable;

        // Generate a unique node ID
        let next_id = self
            .id_gen
            .0
            .checked_add(1)
            .expect("too many nodes spawned");

        let node_id = mem::replace(&mut self.id_gen, RpcNodeId(next_id));
        node.id = Some(node_id);

        // Extend the node with server-specific state
        let server_node = node.entity().add(RpcNodeServer {
            kind: node.kind(),
            vtable,
            node_id,
            userdata: Index::DANGLING,
            server: self,
            visible_to: FxHashSet::default(),
        });

        // Register in the ID map
        self.id_to_node.insert(node_id, server_node);

        server_node
    }

    pub fn lookup_node(&self, id: RpcNodeId) -> Option<Obj<RpcNodeServer>> {
        self.id_to_node.get(&id).copied()
    }

    pub fn replicate(mut self: Obj<Self>, node: Obj<RpcNodeServer>, peer: PeerId) {
        let kind = self.kinds.get_mut(&node.kind).unwrap();

        kind.now_invisible.remove(&(node, peer));

        if !node.is_visible_to(peer) {
            kind.now_visible.insert((node, peer));
        }
    }

    pub fn de_replicate(mut self: Obj<Self>, node: Obj<RpcNodeServer>, peer: PeerId) {
        let kind = self.kinds.get_mut(&node.kind).unwrap();

        kind.now_visible.remove(&(node, peer));

        if node.is_visible_to(peer) {
            kind.now_invisible.insert((node, peer));
        }
    }

    pub fn send_packet<K: RpcKind>(
        mut self: Obj<Self>,
        target: Obj<RpcNodeServer>,
        packet: K::ClientBound,
    ) {
        assert_eq!(target.kind, RpcKindId::of::<K>());

        let mut encoder = FrameEncoder::new();
        encoder.data_mut().encode_multi_part(&packet);
        encoder
            .data_mut()
            .encode_multi_part(&RpcCbHeader::SendMessage(target.node_id));

        self.kinds
            .get_mut(&target.kind)
            .unwrap()
            .packet_send_queue
            .push((target, encoder));
    }

    pub fn recv_packet(self: Obj<Self>, sender: PeerId, packet: Bytes) -> anyhow::Result<()> {
        let mut packet = MultiPartDecoder::new(packet);

        let header = packet
            .expect_rich::<RpcSbHeader>()
            .context("failed to parse RPC header")?;

        let RpcSbHeader::SendMessage(target_id) = header;

        let data = packet.expect().context("failed to parse RPC data")?;

        let Some(target) = self.lookup_node(target_id) else {
            tracing::warn!("node with ID {target_id:?} does not exist");
            return Ok(());
        };

        if !target.is_visible_to(sender) {
            tracing::warn!("{target_id:?} is not visible to {sender:?}");
            return Ok(());
        }

        (target.vtable.process_inbound)(&mut WORLD, target, sender, data)
    }

    pub fn flush(self: Obj<Self>, trans: &mut impl RpcServerFlushTransport) {
        let mut guard = steal_from_ecs(self, field!(Self, kinds));
        let (world, kinds) = &mut *guard;

        for kind in kinds.values_mut() {
            // Process incoming packets and (de)replication requests.
            (kind.vtable.drain_replications)(world, kind);

            // Send out packets
            for (peer, packet) in kind.packet_replicate_queue.drain(..) {
                trans.send_packet_single(world, peer, packet);
            }

            for (target, packet) in kind.packet_send_queue.drain(..) {
                trans.send_packet_multi(world, target, packet);
            }

            for (peer, packet) in kind.packet_de_replicate_queue.drain(..) {
                trans.send_packet_single(world, peer, packet);
            }
        }
    }
}

pub trait RpcServerFlushTransport {
    fn send_packet_single(&mut self, world: &mut World, target: PeerId, packet: FrameEncoder);

    fn send_packet_multi(
        &mut self,
        world: &mut World,
        target: Obj<RpcNodeServer>,
        packet: FrameEncoder,
    );
}

// === RpcNodeServer === //

#[derive(Debug)]
pub struct RpcNodeServer {
    kind: RpcKindId,
    vtable: KindVtableRef,
    node_id: RpcNodeId,
    userdata: Index,
    server: Obj<RpcServer>,
    visible_to: FxHashSet<PeerId>,
}

component!(RpcNodeServer);

impl RpcNodeServer {
    pub fn kind(&self) -> RpcKindId {
        self.kind
    }

    pub fn id(&self) -> RpcNodeId {
        self.node_id
    }

    pub fn server(&self) -> Obj<RpcServer> {
        self.server
    }

    pub fn visible_to(&self) -> &FxHashSet<PeerId> {
        &self.visible_to
    }

    pub fn is_visible_to(&self, peer: PeerId) -> bool {
        self.visible_to.contains(&peer)
    }

    fn userdata<K: RpcKindServer>(
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
