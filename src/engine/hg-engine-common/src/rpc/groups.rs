use std::context::{infer_bundle, pack, Bundle};

use hg_ecs::{bind, component, query::query_removed, Obj};
use hg_utils::hash::FxHashSet;

use crate::{
    field,
    utils::lang::{steal_from_ecs, Steal},
};

use super::{RpcServerNode, RpcServerPeer};

#[derive(Debug, Default)]
pub struct RpcGroup {
    inner: Steal<RpcGroupInner>,
}

#[derive(Debug, Default)]
struct RpcGroupInner {
    nodes: FxHashSet<Obj<RpcGroupFollower>>,
    peers: FxHashSet<Obj<RpcServerPeer>>,
}

component!(RpcGroup);

impl RpcGroup {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(
        mut self: Obj<Self>,
        target: Obj<RpcServerNode>,
        excluded: Option<Obj<RpcServerPeer>>,
    ) -> Obj<RpcGroupFollower> {
        let follower = target.entity().add(RpcGroupFollower {
            rpc: target,
            group: self,
            excluded,
        });

        self.inner.nodes.insert(follower);

        let mut guard = steal_from_ecs(self, field!(Self, inner));
        let (world, inner) = &mut *guard;

        bind!(world, let cx: infer_bundle!('_));

        inner.peers.retain(|&peer| {
            let static ..cx;

            if !peer.is_connected() {
                return false;
            }

            if Some(peer) != excluded {
                target.replicate(peer);
            }

            true
        });

        follower
    }

    pub fn add_peer(mut self: Obj<Self>, peer: Obj<RpcServerPeer>) {
        if !peer.is_connected() {
            return;
        }

        self.inner.peers.insert(peer);

        let mut guard = steal_from_ecs(self, field!(Self, inner));
        let (world, inner) = &mut *guard;
        bind!(world);

        for &node in &inner.nodes {
            if node.excluded != Some(peer) {
                node.rpc.replicate(peer);
            }
        }
    }

    pub fn remove_peer(mut self: Obj<Self>, peer: Obj<RpcServerPeer>) {
        if !self.inner.peers.remove(&peer) {
            return;
        }

        if !peer.is_connected() {
            return;
        }

        let mut guard = steal_from_ecs(self, field!(Self, inner));
        let (world, inner) = &mut *guard;
        bind!(world);

        for &node in &inner.nodes {
            if node.excluded != Some(peer) {
                node.rpc.de_replicate(peer);
            }
        }
    }
}

#[derive(Debug)]
pub struct RpcGroupFollower {
    rpc: Obj<RpcServerNode>,
    group: Obj<RpcGroup>,
    excluded: Option<Obj<RpcServerPeer>>,
}

component!(RpcGroupFollower);

impl RpcGroupFollower {
    pub fn unregister(self: Obj<Self>) {
        let mut guard = steal_from_ecs(self.group, field!(RpcGroup, inner));
        let (world, inner) = &mut *guard;
        bind!(world);

        // Remove node from set
        inner.nodes.remove(&self);

        // Replicate deletion
        let cx = pack!(@env => Bundle<infer_bundle!('_)>);

        inner.peers.retain(|&peer| {
            let static ..cx;

            if !peer.is_connected() {
                return false;
            }

            self.rpc.de_replicate(peer);
            true
        });
    }
}

pub fn sys_flush_rpc_groups() {
    for node in query_removed::<RpcGroupFollower>() {
        node.unregister();
    }
}
