use std::{
    any::type_name,
    context::{pack, Bundle, BundleItemSetFor},
    fmt, mem,
    num::NonZeroU64,
};

use bytes::Bytes;
use hg_ecs::{bind, component, entity::Component, AccessComp, Index, Obj, World};
use hg_utils::hash::{hash_map, FxHashMap, FxHashSet};

use crate::{
    base::net::{codec::FrameEncoder, serialize::MultiPartSerializeExt as _, transport::PeerId},
    field,
    utils::lang::{steal_from_ecs, Steal},
};

use super::{RpcCbHeader, RpcKind, RpcKindId, RpcNode, RpcNodeId, RpcPacket as _};

// === RpcKind === //

type ServerSb<K> = <<K as RpcKindServer>::Kind as RpcKind>::ServerBound;

pub trait RpcKindServer: Sized + 'static {
    type Kind: RpcKind;

    type Cx<'a>: BundleItemSetFor<'a>;
    type RpcRoot: Component;

    fn catchup(
        cx: Bundle<Self::Cx<'_>>,
        peer: PeerId,
        target: Obj<Self::RpcRoot>,
    ) -> <Self::Kind as RpcKind>::Catchup;

    fn process(
        cx: Bundle<Self::Cx<'_>>,
        target: Obj<Self::RpcRoot>,
        sender: PeerId,
        packet: <Self::Kind as RpcKind>::ServerBound,
    ) -> anyhow::Result<()>;
}

type KindVtableRef = &'static KindVtable;

struct KindVtable {
    drain_on_flush: fn(&mut World, &mut KindData),
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
        drain_on_flush: |world, kind_data| {
            // Process received packets
            // TODO: Should this be allowed to reentrantly send new packets?
            for (target, sender, packet) in kind_data.packet_recv_queue.drain(..) {
                // Deserialize the packet
                let Ok(packet) = ServerSb::<K>::decode(&packet) else {
                    // TODO: Better error handling
                    tracing::warn!("failed to decode packet");
                    continue;
                };

                // Fetch the RPC node userdata
                let userdata = {
                    bind!(world, let cx: _);
                    target.userdata::<K>(cx)
                };

                // Process the packet
                let res = {
                    let mut reborrow = world.reborrow();
                    let cx = reborrow.bundle();
                    K::process(cx, userdata, sender, packet)
                };

                if res.is_err() {
                    // TODO: Better error handling
                    tracing::error!("failed to process packet: {res:?}");
                    continue;
                }
            }

            // Process replications
            for (target, peer) in kind_data.now_visible.drain() {
                // Fetch the RPC node userdata
                let (target_id, userdata) = {
                    bind!(world, let cx: _);

                    (target.node_id, target.userdata::<K>(cx))
                };

                // Produce the catchup structure
                let res = {
                    let mut reborrow = world.reborrow();
                    let cx = reborrow.bundle();
                    K::catchup(cx, peer, userdata)
                };

                // Serialize the catchup structure
                let mut encoder = FrameEncoder::new();

                encoder.data_mut().encode_multi_part(|packet| {
                    res.encode(packet);
                });

                encoder
                    .data_mut()
                    .encode_multi_part(|packet| RpcCbHeader::CreateNode(target_id).encode(packet));

                kind_data.packet_replicate_queue.push((peer, encoder));
            }

            // Process de-replications
            {
                bind!(world);

                for (target, peer) in kind_data.now_invisible.drain() {
                    let mut encoder = FrameEncoder::new();
                    let target_id = target.node_id;

                    encoder.data_mut().encode_multi_part(|packet| {
                        RpcCbHeader::DeleteNode(target_id).encode(packet)
                    });

                    kind_data.packet_de_replicate_queue.push((peer, encoder));
                }
            }
        },
    };
}

// === RpcServer === //

#[derive(Debug)]
pub struct RpcServer {
    /// The set of `RpcKind`s registered with this RPC server and queues for operations on nodes
    /// of that kind.
    kinds: Steal<FxHashMap<RpcKindId, KindData>>,

    /// A map from node ID to node handle.
    id_to_node: FxHashMap<RpcNodeId, Obj<RpcNodeServer>>,

    /// The next ID to be assigned to a newly-registered `RpcNode`.
    id_gen: RpcNodeId,
}

#[derive(Debug)]
struct KindData {
    vtable: &'static KindVtable,

    /// Queues packets received from some unspecified transport. The RPC multi-part packet has been
    /// parsed and stripped, leaving just the raw message packet in its place.
    packet_recv_queue: Vec<(Obj<RpcNodeServer>, PeerId, Bytes)>,

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
            packet_recv_queue: Vec::new(),
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
        assert!(
            self.kinds.contains_key(&node.kind()),
            "RPC node with kind {:?} was never registered",
            node.kind.id()
        );

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

        let target_id = target.node_id;

        let mut encoder = FrameEncoder::new();

        encoder.data_mut().encode_multi_part(|out| {
            packet.encode(out);
        });

        encoder.data_mut().encode_multi_part(|packet| {
            RpcCbHeader::SendMessage(target_id).encode(packet);
        });

        self.kinds
            .get_mut(&target.kind)
            .unwrap()
            .packet_send_queue
            .push((target, encoder));
    }

    pub fn recv_packet(mut self: Obj<Self>, target_id: RpcNodeId, sender: PeerId, data: Bytes) {
        let Some(target) = self.lookup_node(target_id) else {
            tracing::warn!("node with ID {target_id:?} does not exist");
            return;
        };

        if !target.is_visible_to(sender) {
            tracing::warn!("{target_id:?} is not visible to {sender:?}");
            return;
        }

        self.kinds
            .get_mut(&target.kind)
            .unwrap()
            .packet_recv_queue
            .push((target, sender, data));
    }

    pub fn flush(self: Obj<Self>, trans: &mut impl RpcServerFlushTransport) {
        let mut guard = steal_from_ecs(self, field!(Self, kinds));
        let (world, kinds) = &mut *guard;

        for kind in kinds.values_mut() {
            // Process incoming packets and (de)replication requests.
            (kind.vtable.drain_on_flush)(world, kind);

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
