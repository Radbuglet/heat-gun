use std::{
    any::type_name,
    borrow::Cow,
    context::{infer_bundle, pack, Bundle, DerefCx},
    fmt,
    marker::PhantomData,
    mem,
    num::NonZeroU64,
};

use anyhow::Context;
use bytes::Bytes;
use derive_where::derive_where;
use hg_ecs::{
    bind, component, entity::Component, query::query_removed, AccessComp, AccessCompRef, Entity,
    Index, Obj, World, WORLD,
};
use hg_utils::hash::{FxHashMap, FxHashSet};

use crate::{
    net::{FrameEncoder, MultiPartDecoder, MultiPartSerializeExt as _, RpcPacket},
    utils::lang::NamedTypeId,
};

use super::{
    BadRpcNodeKindError, NoSuchRpcNodeError, RpcCbHeader, RpcKind, RpcNodeId, RpcNodeLookupError,
    RpcSbHeader,
};

// === RpcKind === //

pub trait RpcServerReplicator<K: RpcKind>: Component {
    fn catchup(self: Obj<Self>, world: &mut World) -> K::Catchup;

    fn process(
        self: Obj<Self>,
        world: &mut World,
        peer: Obj<RpcServerPeer>,
        packet: K::ServerBound,
    ) -> anyhow::Result<()>;
}

type KindVtableRef = &'static KindVtable;

struct KindVtable {
    produce_catchup: fn(&mut World, Obj<RpcServerNode>, &mut FrameEncoder),
    process_inbound:
        fn(&mut World, Obj<RpcServerNode>, Obj<RpcServerPeer>, Bytes) -> anyhow::Result<()>,
    kind_type_id: fn() -> NamedTypeId,
}

impl fmt::Debug for KindVtable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KindVtableInner").finish_non_exhaustive()
    }
}

trait HasKindVtable<K: RpcKind> {
    const VTABLE: KindVtableRef;
}

impl<T, K> HasKindVtable<K> for T
where
    T: RpcServerReplicator<K>,
    K: RpcKind,
{
    const VTABLE: KindVtableRef = &KindVtable {
        produce_catchup: |world, target, encoder| {
            bind!(world);

            let (target_id, userdata) = (target.node_id, target.userdata::<T>());
            let catchup = T::catchup(userdata, &mut WORLD);
            encoder.encode_multi_part(&catchup);
            encoder.encode_multi_part(&RpcCbHeader::CreateNode(target_id, Cow::Borrowed(K::ID)));
        },
        process_inbound: |world, target, peer, packet| {
            bind!(world);

            let packet = <K::ServerBound as RpcPacket>::decode(&packet)?;
            let userdata = target.userdata::<T>();
            T::process(userdata, &mut WORLD, peer, packet)?;

            Ok(())
        },
        kind_type_id: NamedTypeId::of::<K>,
    };
}

// === RpcServer === //

#[derive(Debug)]
pub struct RpcServer {
    id_to_node: FxHashMap<RpcNodeId, Obj<RpcServerNode>>,
    id_gen: RpcNodeId,
    action_queue: Vec<QueuedAction>,
    node_queues: FxHashSet<Obj<RpcNodeServerQueue>>,
}

#[derive(Debug)]
enum QueuedAction {
    ReplicateTo {
        queue: Obj<RpcNodeServerQueue>,
        peer: Obj<RpcServerPeer>,
        packet: FrameEncoder,
    },
    DestroyRemotely {
        queue: Obj<RpcNodeServerQueue>,
        peer: Obj<RpcServerPeer>,
    },
    Broadcast {
        queue: Obj<RpcNodeServerQueue>,
        packet: FrameEncoder,
    },
    DestroyNode {
        queue: Obj<RpcNodeServerQueue>,
    },
}

#[derive(Debug)]
pub struct RpcNodeServerQueue {
    node_id: RpcNodeId,
    visible_to: FxHashSet<Obj<RpcServerPeer>>,
}

component!(RpcServer, RpcNodeServerQueue);

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
            action_queue: Vec::new(),
            node_queues: FxHashSet::default(),
        }
    }

    pub fn register_node<T, K>(
        mut self: Obj<Self>,
        node: Entity,
        userdata: Obj<T>,
    ) -> Obj<RpcServerNode>
    where
        T: RpcServerReplicator<K>,
        K: RpcKind,
    {
        // Generate a unique node ID
        let next_id = self
            .id_gen
            .0
            .checked_add(1)
            .expect("too many nodes spawned");

        let node_id = mem::replace(&mut self.id_gen, RpcNodeId(next_id));

        // Create the queue node
        let queue = Entity::new(self.entity()).add(RpcNodeServerQueue {
            node_id,
            visible_to: FxHashSet::default(),
        });

        self.node_queues.insert(queue);

        // Extend the node with node state
        let server_node = node.add(RpcServerNode {
            server: self,
            node_id,
            vtable: <T as HasKindVtable<K>>::VTABLE,
            visible_to: FxHashSet::default(),
            queue,
            userdata_ty: NamedTypeId::of::<T>(),
            userdata: Obj::raw(userdata),
        });

        // Register in the ID map
        self.id_to_node.insert(node_id, server_node);

        server_node
    }

    pub fn register_peer(self: Obj<Self>, peer: Entity) -> Obj<RpcServerPeer> {
        peer.add(RpcServerPeer {
            server: self,
            vis_set: FxHashSet::default(),
            connected: true,
        })
    }

    pub fn lookup_any_node(&self, id: RpcNodeId) -> Result<Obj<RpcServerNode>, NoSuchRpcNodeError> {
        self.id_to_node
            .get(&id)
            .copied()
            .ok_or(NoSuchRpcNodeError { id })
    }

    pub fn lookup_node<T: Component>(&self, id: RpcNodeId) -> Result<Obj<T>, RpcNodeLookupError> {
        self.lookup_any_node(id)?.opt_userdata().map_err(Into::into)
    }

    pub fn recv_packet(
        self: Obj<Self>,
        sender: Obj<RpcServerPeer>,
        packet: Bytes,
    ) -> anyhow::Result<()> {
        let mut packet = MultiPartDecoder::new(packet);

        let header = packet
            .expect_rich::<RpcSbHeader>()
            .context("failed to parse RPC header")?;

        let RpcSbHeader::SendMessage(target_id) = header;

        let data = packet.expect().context("failed to parse RPC data")?;

        let Ok(target) = self.lookup_any_node(target_id) else {
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
        // Remove dead peers since we can't send packets to them anymore.
        for &(mut queue) in &self.node_queues {
            let cx = pack!(@env => Bundle<infer_bundle!('_)>);
            queue.visible_to.retain(|&peer| {
                let static ..cx;
                peer.is_connected()
            });
        }

        for action in mem::take(&mut self.action_queue) {
            match action {
                QueuedAction::ReplicateTo {
                    mut queue,
                    peer,
                    packet,
                } => {
                    if !peer.is_connected() {
                        continue;
                    }

                    let packet = target.complete_packet(packet);
                    target.send_packet(&mut WORLD, peer, packet);

                    queue.visible_to.insert(peer);
                }
                QueuedAction::DestroyRemotely { mut queue, peer } => {
                    if !peer.is_connected() {
                        continue;
                    }

                    let mut encoder = FrameEncoder::new();
                    encoder.encode_multi_part(&RpcCbHeader::DeleteNode(queue.node_id));

                    let packet = target.complete_packet(encoder);
                    target.send_packet(&mut WORLD, peer, packet);

                    queue.visible_to.remove(&peer);
                }
                QueuedAction::Broadcast { queue, packet } => {
                    let packet = target.complete_packet(packet);

                    // TODO: Don't clone
                    for peer in queue.visible_to.clone() {
                        target.send_packet(&mut WORLD, peer, packet.clone());
                    }
                }
                QueuedAction::DestroyNode { mut queue } => {
                    // Create a destruction packet
                    let mut encoder = FrameEncoder::new();
                    encoder.encode_multi_part(&RpcCbHeader::DeleteNode(queue.node_id));

                    // Broadcast it
                    let packet = target.complete_packet(encoder);

                    for peer in mem::take(&mut queue.visible_to) {
                        target.send_packet(&mut WORLD, peer, packet.clone());
                    }

                    // Destroy the unused queue
                    queue.entity().destroy();
                    self.node_queues.remove(&queue);
                }
            }
        }
    }
}

pub trait RpcServerFlushTransport {
    fn complete_packet(&mut self, encoder: FrameEncoder) -> Bytes {
        encoder.finish()
    }

    fn send_packet(&mut self, world: &mut World, target: Obj<RpcServerPeer>, packet: Bytes);
}

// === RpcServerNode === //

#[derive(Debug)]
pub struct RpcServerNode {
    server: Obj<RpcServer>,
    node_id: RpcNodeId,
    vtable: KindVtableRef,
    visible_to: FxHashSet<Obj<RpcServerPeer>>,
    queue: Obj<RpcNodeServerQueue>,
    userdata_ty: NamedTypeId,
    userdata: Index,
}

component!(RpcServerNode);

impl RpcServerNode {
    pub fn server(&self) -> Obj<RpcServer> {
        self.server
    }

    pub fn id(&self) -> RpcNodeId {
        self.node_id
    }

    pub fn visible_to(&self) -> &FxHashSet<Obj<RpcServerPeer>> {
        &self.visible_to
    }

    pub fn is_visible_to(&self, peer: Obj<RpcServerPeer>) -> bool {
        self.visible_to.contains(&peer)
    }

    pub fn replicate(mut self: Obj<Self>, mut peer: Obj<RpcServerPeer>) {
        if !peer.is_connected() {
            return;
        }

        if !self.visible_to.insert(peer) {
            return;
        }

        peer.vis_set.insert(self);

        let mut encoder = FrameEncoder::new();
        (self.vtable.produce_catchup)(&mut WORLD, self, &mut encoder);

        self.server.action_queue.push(QueuedAction::ReplicateTo {
            queue: self.queue,
            peer,
            packet: encoder,
        });
    }

    pub fn de_replicate(mut self: Obj<Self>, mut peer: Obj<RpcServerPeer>) {
        if !peer.is_connected() {
            return;
        }

        if !self.visible_to.remove(&peer) {
            return;
        }

        peer.vis_set.remove(&self);

        self.server
            .action_queue
            .push(QueuedAction::DestroyRemotely {
                queue: self.queue,
                peer,
            });
    }

    pub fn broadcast<K: RpcKind>(mut self: Obj<Self>, packet: &K::ClientBound) {
        assert_eq!((self.vtable.kind_type_id)(), NamedTypeId::of::<K>());

        let mut encoder = FrameEncoder::new();

        encoder.encode_multi_part(packet);
        encoder.encode_multi_part(&RpcCbHeader::SendMessage(self.node_id));

        self.server.action_queue.push(QueuedAction::Broadcast {
            queue: self.queue,
            packet: encoder,
        });
    }

    pub fn unregister(mut self: Obj<Self>) {
        // Queue up remote destruction
        self.server
            .action_queue
            .push(QueuedAction::DestroyNode { queue: self.queue });

        // Unregister the node from the server
        self.server.id_to_node.remove(&self.node_id);

        // Update peer visibility sets
        for mut peer in self.visible_to.drain() {
            peer.vis_set.remove(&self);
        }
    }

    pub fn opt_userdata<T: Component>(self: Obj<Self>) -> Result<Obj<T>, BadRpcNodeKindError> {
        if self.userdata_ty == NamedTypeId::of::<T>() {
            Ok(Obj::from_raw(self.userdata))
        } else {
            Err(BadRpcNodeKindError {
                id: self.node_id,
                expected_ty: NamedTypeId::of::<T>(),
                actual_ty: self.userdata_ty,
            })
        }
    }

    pub fn userdata<T: Component>(self: Obj<Self>) -> Obj<T> {
        self.opt_userdata().unwrap()
    }
}

// === RpcServerPeer === //

#[derive(Debug)]
pub struct RpcServerPeer {
    server: Obj<RpcServer>,
    vis_set: FxHashSet<Obj<RpcServerNode>>,
    connected: bool,
}

component!(RpcServerPeer);

impl RpcServerPeer {
    pub fn server(&self) -> Obj<RpcServer> {
        self.server
    }

    pub fn is_connected(self: Obj<Self>) -> bool {
        Obj::is_alive(self) && self.connected
    }

    pub fn disconnect(mut self: Obj<Self>) {
        if !self.connected {
            return;
        }

        self.connected = false;

        for mut replicated_to in self.vis_set.drain() {
            replicated_to.visible_to.remove(&self);
        }
    }
}

// === RpcServerHandle === //

#[derive_where(Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct RpcServerHandle<K> {
    _ty: PhantomData<fn(K) -> K>,
    raw: Obj<RpcServerNode>,
}

impl<K: RpcKind> fmt::Debug for RpcServerHandle<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple(&format!("RpcServerHandle<{}>", type_name::<K>()))
            .field(&self.raw)
            .finish()
    }
}

impl<K: RpcKind> RpcServerHandle<K> {
    pub const DANGLING: Self = Self::wrap(Obj::DANGLING);

    pub const fn wrap(target: Obj<RpcServerNode>) -> Self {
        Self {
            _ty: PhantomData,
            raw: target,
        }
    }

    pub fn raw(self) -> Obj<RpcServerNode> {
        self.raw
    }

    pub fn server(self) -> Obj<RpcServer> {
        self.raw().server()
    }

    pub fn id(self) -> RpcNodeId {
        self.raw().id()
    }

    pub fn visible_to<'a>(
        self,
        cx: Bundle<AccessCompRef<'a, RpcServerNode>>,
    ) -> &'a FxHashSet<Obj<RpcServerPeer>> {
        self.raw().deref_cx(cx).visible_to()
    }

    pub fn is_visible_to(self, peer: Obj<RpcServerPeer>) -> bool {
        self.raw().is_visible_to(peer)
    }

    pub fn replicate(self, peer: Obj<RpcServerPeer>) {
        self.raw().replicate(peer);
    }

    pub fn de_replicate(self, peer: Obj<RpcServerPeer>) {
        self.raw().de_replicate(peer);
    }

    pub fn broadcast(self, packet: &K::ClientBound) {
        self.raw().broadcast::<K>(packet);
    }
}

// === Systems === //

pub fn spawn_server_rpc<T, K>(target: Obj<T>, cx: Bundle<&AccessComp<T>>) -> RpcServerHandle<K>
where
    T: RpcServerReplicator<K>,
    K: RpcKind,
{
    let parent = target.entity(pack!(cx));
    let child = Entity::new(parent);
    let rpc = parent
        .deep_get::<RpcServer>()
        .register_node::<T, K>(child, target);

    RpcServerHandle::wrap(rpc)
}

pub fn sys_flush_rpc_server() {
    for node in query_removed::<RpcServerNode>() {
        node.unregister();
    }

    for peer in query_removed::<RpcServerPeer>() {
        peer.disconnect();
    }
}
