use std::{
    any::type_name,
    context::{pack, Bundle, BundleItemSetFor},
    fmt, mem,
};

use bytes::Bytes;
use hg_ecs::{bind, component, entity::Component, AccessComp, Index, Obj, World};
use hg_utils::hash::{hash_map, FxHashMap, FxHashSet};

use crate::{
    base::net::{codec::FrameEncoder, serialize::MultiPartSerializeExt as _},
    field,
    utils::lang::{steal_from_ecs, Steal},
};

use super::{RpcKind, RpcKindId, RpcNode, RpcNodeId, RpcPacket as _, RpcPeer};

// === RpcKind === //

type ServerSb<K> = <<K as RpcKindServer>::Kind as RpcKind>::ServerBound;

pub trait RpcKindServer: Sized + 'static {
    type Kind: RpcKind;

    type Cx<'a>: BundleItemSetFor<'a>;
    type RpcRoot: Component;

    fn catchup(
        cx: Bundle<Self::Cx<'_>>,
        peer: RpcPeer,
        target: Obj<Self::RpcRoot>,
    ) -> <Self::Kind as RpcKind>::Catchup;

    fn process(
        cx: Bundle<Self::Cx<'_>>,
        target: Obj<Self::RpcRoot>,
        sender: RpcPeer,
        packet: <Self::Kind as RpcKind>::ServerBound,
    ) -> anyhow::Result<()>;
}

type KindVtableRef = &'static KindVtable;

struct KindVtable {
    process_recv: fn(&mut World, &mut KindData),
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
        process_recv: |world, kind_data| {
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
    vtable: &'static KindVtable,
    packet_recv_queue: Vec<(Obj<RpcNodeServer>, RpcPeer, Bytes)>,
    packet_send_queue: Vec<(Obj<RpcNodeServer>, FrameEncoder)>,
}

component!(RpcServer);

impl RpcServer {
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

    pub fn replicate(mut self: Obj<Self>, node: Obj<RpcNodeServer>, peer: RpcPeer) {
        todo!();
    }

    pub fn de_replicate(mut self: Obj<Self>, node: Obj<RpcNodeServer>, peer: RpcPeer) {
        todo!()
    }

    pub fn send_packet<K: RpcKind>(
        mut self: Obj<Self>,
        target: Obj<RpcNodeServer>,
        packet: K::ClientBound,
    ) {
        assert_eq!(target.kind, RpcKindId::of::<K>());

        let mut encoder = FrameEncoder::new();
        encoder.data_mut().encode_multi_part(|out| {
            packet.encode(out);
        });

        self.kinds
            .get_mut(&target.kind)
            .unwrap()
            .packet_send_queue
            .push((target, encoder));
    }

    pub fn recv_packet(mut self: Obj<Self>, target_id: RpcNodeId, sender: RpcPeer, data: Bytes) {
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

    pub fn flush(mut self: Obj<Self>, trans: &mut impl RpcServerFlushTransport) {
        let mut guard = steal_from_ecs(self, field!(Self, kinds));
        let (world, kinds) = &mut *guard;

        for kind in kinds.values_mut() {
            // Process received packets
            (kind.vtable.process_recv)(world, kind);

            // Send out queued regular packets
            for (target, mut packet) in kind.packet_send_queue.drain(..) {
                packet.data_mut().encode_multi_part(|packet| {
                    // TODO: Encode header
                });

                trans.send_packet(world, target, packet);
            }

            // Send out queued catchup packets
            // TODO
        }
    }
}

pub trait RpcServerFlushTransport {
    fn send_packet(&mut self, world: &mut World, target: Obj<RpcNodeServer>, packet: FrameEncoder);
}

// === RpcNodeServer === //

#[derive(Debug)]
pub struct RpcNodeServer {
    kind: RpcKindId,
    node_id: RpcNodeId,
    userdata: Index,
    server: Obj<RpcServer>,
    visible_to: FxHashSet<RpcPeer>,
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

    pub fn visible_to(&self) -> &FxHashSet<RpcPeer> {
        &self.visible_to
    }

    pub fn is_visible_to(&self, peer: RpcPeer) -> bool {
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
