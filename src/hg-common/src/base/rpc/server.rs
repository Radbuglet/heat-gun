use std::{
    any::{type_name, Any},
    context::{Bundle, BundleItemSet},
    fmt, mem,
    ops::Deref,
};

use derive_where::derive_where;
use hg_ecs::{component, entity::Component, Entity, Obj};
use hg_utils::hash::{hash_map, FxHashMap, FxHashSet};

use super::{RpcKind, RpcKindId, RpcNode, RpcNodeId, RpcPeer};

// === RpcKind === //

pub trait RpcKindServer: Sized + 'static {
    type Kind: RpcKind;

    type Cx<'a>: BundleItemSet;
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

#[derive(Copy, Clone)]
pub struct RpcKindIdServer(&'static RpcKindTableServer);

impl fmt::Debug for RpcKindIdServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RpcKindServerErased")
            .finish_non_exhaustive()
    }
}

impl RpcKindIdServer {
    pub fn of<K: RpcKindServer>() -> Self {
        trait Helper {
            const INNER: &'static RpcKindTableServer;
        }

        impl<K: RpcKindServer> Helper for K {
            const INNER: &'static RpcKindTableServer = &RpcKindTableServer {};
        }

        Self(<K as Helper>::INNER)
    }
}

impl Deref for RpcKindIdServer {
    type Target = RpcKindTableServer;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

pub struct RpcKindTableServer {}

// === RpcServer === //

#[derive(Debug)]
pub struct RpcServer {
    kinds: FxHashMap<RpcKindId, KindData>,
    id_to_node: FxHashMap<RpcNodeId, Obj<RpcNodeServer>>,
    id_gen: RpcNodeId,
}

#[derive(Debug)]
struct KindData {
    id: RpcKindIdServer,
    visibility_changes: FxHashMap<(Obj<RpcNodeServer>, RpcPeer), bool>,
    packet_queue: Box<dyn Any + Send + Sync>,
}

#[derive(Debug)]
#[derive_where(Default)]
struct PacketQueue<K: RpcKind>(Vec<(Obj<RpcNodeServer>, K::ClientBound)>);

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
            id: RpcKindIdServer::of::<K>(),
            visibility_changes: FxHashMap::default(),
            packet_queue: Box::new(PacketQueue::<K::Kind>::default()),
        });

        self
    }

    pub fn register(mut self: Obj<Self>, mut node: Obj<RpcNode>) {
        assert!(node.entity().try_get::<RpcNodeServer>().is_none());

        // Ensure that the node has a valid kind
        let kind = match self.kinds.get(&node.kind) {
            Some(KindData { id, .. }) => *id,
            None => panic!(
                "RPC node with kind {:?} was never registered",
                node.kind.id()
            ),
        };

        // Generate a unique node ID
        let next_id = self
            .id_gen
            .0
            .checked_add(1)
            .expect("too many nodes spawned");

        node.id = mem::replace(&mut self.id_gen, RpcNodeId(next_id));

        // Extend the node with server-specific state
        let server_node = node.entity().add(RpcNodeServer {
            server: self,
            kind,
            visible_to: FxHashSet::default(),
        });

        node.managed_state = Obj::raw(server_node);
    }

    fn unwrap_node(self: Obj<Self>, node: Obj<RpcNode>) -> Obj<RpcNodeServer> {
        let node = Obj::<RpcNodeServer>::from_raw(node.managed_state);
        assert_eq!(node.server, self);
        node
    }

    pub fn update_visibility(
        mut self: Obj<Self>,
        node: Obj<RpcNode>,
        peer: RpcPeer,
        visible: bool,
    ) {
        let kind = node.kind;
        let node = self.unwrap_node(node);
        self.kinds
            .get_mut(&kind)
            .unwrap()
            .visibility_changes
            .insert((node, peer), visible);
    }

    pub fn send_msg<K: RpcKind>(mut self: Obj<Self>, node: Obj<RpcNode>, message: K::ClientBound) {
        let kind = node.kind;
        let node = self.unwrap_node(node);
        self.kinds
            .get_mut(&kind)
            .unwrap()
            .packet_queue
            .downcast_mut::<PacketQueue<K>>()
            .unwrap()
            .0
            .push((node, message));
    }

    pub fn lookup(&self, id: RpcNodeId) -> Option<Entity> {
        Some(self.id_to_node.get(&id)?.entity())
    }

    pub fn flush(&mut self) {
        todo!();
    }
}

// === RpcNodeServer === //

#[derive(Debug)]
pub struct RpcNodeServer {
    server: Obj<RpcServer>,
    kind: RpcKindIdServer,
    visible_to: FxHashSet<RpcPeer>,
}

component!(RpcNodeServer);
