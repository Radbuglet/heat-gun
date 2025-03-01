use std::{
    borrow::Cow,
    context::{infer_bundle, pack, Bundle, BundleItemSetFor},
    fmt, mem,
    num::NonZeroU64,
};

use anyhow::Context;
use bytes::Bytes;
use hg_ecs::{
    bind, component, entity::Component, query::query_removed, AccessComp, Entity, Index, Obj,
    World, WORLD,
};
use hg_utils::hash::{FxHashMap, FxHashSet};

use crate::base::net::{
    FrameEncoder, MultiPartDecoder, MultiPartSerializeExt as _, RpcPacket as _,
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
        peer: Obj<RpcPeer>,
        target: Obj<Self::RpcRoot>,
    ) -> RpcServerCup<Self>;

    fn process(
        cx: Bundle<Self::Cx<'_>>,
        target: Obj<Self::RpcRoot>,
        sender: Obj<RpcPeer>,
        packet: RpcServerSb<Self>,
    ) -> anyhow::Result<()>;
}

type KindVtableRef = &'static KindVtable;

struct KindVtable {
    produce_catchup: fn(&mut World, Obj<RpcNodeServer>, Obj<RpcPeer>, &mut FrameEncoder),
    process_inbound: fn(&mut World, Obj<RpcNodeServer>, Obj<RpcPeer>, Bytes) -> anyhow::Result<()>,
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
        produce_catchup: |world, target, peer, encoder| {
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
            encoder.encode_multi_part(&catchup);
            encoder.encode_multi_part(&RpcCbHeader::CreateNode(
                target_id,
                Cow::Borrowed(<K::Kind as RpcKind>::ID),
            ));
        },
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
    };
}

// === RpcServer === //

#[derive(Debug)]
pub struct RpcServer {
    id_to_node: FxHashMap<RpcNodeId, Obj<RpcNodeServer>>,
    id_gen: RpcNodeId,
    dirty_queues: FxHashSet<Obj<RpcNodeServerQueue>>,
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
            id_to_node: FxHashMap::default(),
            id_gen: RpcNodeId(NonZeroU64::new(1).unwrap()),
            dirty_queues: FxHashSet::default(),
        }
    }

    pub fn register_node<K: RpcKindServer>(
        mut self: Obj<Self>,
        node: Entity,
    ) -> (Obj<RpcNode>, Obj<RpcNodeServer>) {
        // Generate a unique node ID
        let next_id = self
            .id_gen
            .0
            .checked_add(1)
            .expect("too many nodes spawned");

        let node_id = mem::replace(&mut self.id_gen, RpcNodeId(next_id));

        // Create the queue node
        let queue = Entity::new(self.entity()).add(RpcNodeServerQueue {
            server: self,
            node_id,
            visible_to: FxHashSet::default(),
            action_queue: Vec::new(),
            destroyed: false,
            marked_dirty: false,
        });

        // Extend the node with node state
        let generic_node = node.add(RpcNode {
            kind: RpcKindId::of::<K::Kind>(),
            id: node_id,
        });
        let server_node = node.add(RpcNodeServer {
            kind: RpcKindId::of::<K::Kind>(),
            vtable: <K as HasKindVtable>::VTABLE,
            node_id,
            userdata: Index::DANGLING,
            server: self,
            visible_to: FxHashSet::default(),
            queue,
        });

        // Register in the ID map
        self.id_to_node.insert(node_id, server_node);

        (generic_node, server_node)
    }

    pub fn register_peer(self: Obj<Self>, peer: Entity) -> Obj<RpcPeer> {
        peer.add(RpcPeer {
            server: self,
            vis_set: FxHashSet::default(),
            alive: true,
        })
    }

    pub fn lookup_node(&self, id: RpcNodeId) -> Option<Obj<RpcNodeServer>> {
        self.id_to_node.get(&id).copied()
    }

    pub fn recv_packet(self: Obj<Self>, sender: Obj<RpcPeer>, packet: Bytes) -> anyhow::Result<()> {
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

    pub fn flush(mut self: Obj<Self>, target: &mut (impl ?Sized + RpcServerFlushTransport)) {
        for mut queue in mem::take(&mut self.dirty_queues) {
            queue.marked_dirty = false;

            // Remove dead peers since we can't send packets to them anymore.
            let cx = pack!(@env => Bundle<infer_bundle!('_)>);
            queue.visible_to.retain(|&peer| {
                let static ..cx;
                peer.is_alive()
            });

            // Send out packets
            for action in mem::take(&mut queue.action_queue) {
                match action {
                    QueuedAction::ReplicateTo(peer, packet) => {
                        if !peer.is_alive() {
                            continue;
                        }

                        let packet = target.terminate_packet(packet);
                        target.send_packet(&mut WORLD, peer, packet);

                        queue.visible_to.insert(peer);
                    }
                    QueuedAction::DestroyRemotely(peer) => {
                        if !peer.is_alive() {
                            continue;
                        }

                        let mut encoder = FrameEncoder::new();
                        encoder.encode_multi_part(&RpcCbHeader::DeleteNode(queue.node_id));

                        let packet = target.terminate_packet(encoder);
                        target.send_packet(&mut WORLD, peer, packet);

                        queue.visible_to.remove(&peer);
                    }
                    QueuedAction::Broadcast(packet) => {
                        let packet = target.terminate_packet(packet);

                        // TODO: Don't clone
                        for peer in queue.visible_to.clone() {
                            target.send_packet(&mut WORLD, peer, packet.clone());
                        }
                    }
                }
            }

            // Process queue destruction
            if !queue.destroyed {
                continue;
            }

            // Encode a deletion packet
            let mut encoder = FrameEncoder::new();
            encoder.encode_multi_part(&RpcCbHeader::DeleteNode(queue.node_id));

            // Broadcast it
            let packet = target.terminate_packet(encoder);

            for peer in mem::take(&mut queue.visible_to) {
                target.send_packet(&mut WORLD, peer, packet.clone());
            }

            // Destroy the unused queue
            queue.entity().destroy();
        }
    }
}

pub trait RpcServerFlushTransport {
    fn terminate_packet(&mut self, encoder: FrameEncoder) -> Bytes {
        encoder.finish()
    }

    fn send_packet(&mut self, world: &mut World, target: Obj<RpcPeer>, packet: Bytes);
}

// === RpcNodeServer === //

#[derive(Debug)]
pub struct RpcNodeServer {
    kind: RpcKindId,
    vtable: KindVtableRef,
    node_id: RpcNodeId,
    userdata: Index,
    server: Obj<RpcServer>,
    visible_to: FxHashSet<Obj<RpcPeer>>,
    queue: Obj<RpcNodeServerQueue>,
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

    pub fn visible_to(&self) -> &FxHashSet<Obj<RpcPeer>> {
        &self.visible_to
    }

    pub fn is_visible_to(&self, peer: Obj<RpcPeer>) -> bool {
        self.visible_to.contains(&peer)
    }

    pub fn replicate(mut self: Obj<Self>, mut peer: Obj<RpcPeer>) {
        if !self.visible_to.insert(peer) {
            return;
        }

        peer.vis_set.insert(self);

        let mut encoder = FrameEncoder::new();
        (self.vtable.produce_catchup)(&mut WORLD, self, peer, &mut encoder);

        self.queue.mark_dirty();
        self.queue
            .action_queue
            .push(QueuedAction::ReplicateTo(peer, encoder));
    }

    pub fn de_replicate(mut self: Obj<Self>, mut peer: Obj<RpcPeer>) {
        if !self.visible_to.remove(&peer) {
            return;
        }

        peer.vis_set.remove(&self);

        self.queue.mark_dirty();
        self.queue
            .action_queue
            .push(QueuedAction::DestroyRemotely(peer));
    }

    pub fn broadcast<K: RpcKind>(mut self: Obj<Self>, packet: &K::ClientBound) {
        assert_eq!(self.kind, RpcKindId::of::<K>());

        let mut encoder = FrameEncoder::new();

        encoder.encode_multi_part(packet);
        encoder.encode_multi_part(&RpcCbHeader::SendMessage(self.node_id));

        self.queue
            .action_queue
            .push(QueuedAction::Broadcast(encoder));
    }

    pub fn unregister(mut self: Obj<Self>) {
        // Queue up remote destruction
        self.queue.destroyed = true;
        self.queue.mark_dirty();

        // Unregister the node from the server
        self.server.id_to_node.remove(&self.node_id);

        // Update peer visibility sets
        for mut peer in self.visible_to.drain() {
            peer.vis_set.remove(&self);
        }
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

#[derive(Debug)]
pub struct RpcNodeServerQueue {
    server: Obj<RpcServer>,
    node_id: RpcNodeId,
    visible_to: FxHashSet<Obj<RpcPeer>>,
    action_queue: Vec<QueuedAction>,
    destroyed: bool,
    marked_dirty: bool,
}

#[derive(Debug)]
enum QueuedAction {
    ReplicateTo(Obj<RpcPeer>, FrameEncoder),
    DestroyRemotely(Obj<RpcPeer>),
    Broadcast(FrameEncoder),
}

component!(RpcNodeServerQueue);

impl RpcNodeServerQueue {
    fn mark_dirty(mut self: Obj<Self>) {
        if self.marked_dirty {
            return;
        }

        self.marked_dirty = true;
        self.server.dirty_queues.insert(self);
    }
}

// === RpcPeer === //

#[derive(Debug)]
pub struct RpcPeer {
    server: Obj<RpcServer>,
    vis_set: FxHashSet<Obj<RpcNodeServer>>,
    alive: bool,
}

component!(RpcPeer);

impl RpcPeer {
    pub fn server(&self) -> Obj<RpcServer> {
        self.server
    }

    pub fn unregister(mut self: Obj<Self>) {
        if !self.alive {
            return;
        }

        self.alive = false;

        for mut replicated_to in self.vis_set.drain() {
            replicated_to.visible_to.remove(&self);
        }
    }

    pub fn is_alive(self: Obj<Self>) -> bool {
        Obj::is_alive(self) && self.alive
    }
}

// === Systems === //

pub fn add_server_rpc_node<K: RpcKindServer>(target: Entity) -> (Obj<RpcNode>, Obj<RpcNodeServer>) {
    target.deep_get::<RpcServer>().register_node::<K>(target)
}

pub fn sys_flush_server_rpc() {
    for node in query_removed::<RpcNodeServer>() {
        node.unregister();
    }

    for peer in query_removed::<RpcPeer>() {
        peer.unregister();
    }
}
