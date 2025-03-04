use std::{
    any::{type_name, Any, TypeId},
    context::{infer_bundle, pack, Bundle},
    fmt,
    marker::PhantomData,
    panic::Location,
    slice,
    sync::Arc,
};

use anyhow::Context;
use bytes::Bytes;
use derive_where::derive_where;
use hg_ecs::{component, entity::Component, Entity, Index, Obj};
use hg_utils::hash::{hash_map, FxHashMap};

use crate::{
    base::net::{MultiPartDecoder, RpcPacket},
    try_sync,
    utils::lang::MultiError,
};

use super::{RpcCbHeader, RpcKind, RpcNodeId};

// === RpcClient === //

pub trait RpcClientKind<K: RpcKind>: Component {}

#[derive(Debug)]
pub struct RpcClient {
    node_id_map: FxHashMap<RpcNodeId, Obj<RpcClientNode>>,
    kinds_by_name: FxHashMap<&'static str, TypeId>,
    kinds_by_ty: FxHashMap<TypeId, Arc<dyn KindStateErased>>,
    protocol_errors: Vec<anyhow::Error>,
    locked: bool,
}

#[derive_where(Debug)]
struct KindState<K: RpcKind> {
    register_loc: &'static Location<'static>,
    catchups: Vec<QueuedCatchup<K>>,
    messages: Vec<QueuedMessage<K>>,
    deletions: Vec<RpcClientHandle<K>>,
}

#[derive_where(Debug)]
struct QueuedCatchup<K: RpcKind> {
    rpc: RpcClientHandle<K>,
    packet: K::Catchup,
}

#[derive_where(Debug)]
struct QueuedMessage<K: RpcKind> {
    rpc: RpcClientHandle<K>,
    packet: K::ClientBound,
}

trait KindStateErased: 'static + fmt::Debug + Send + Sync {
    fn register_loc(&self) -> &'static Location<'static>;

    fn push_catchup(&mut self, node: Obj<RpcClientNode>, packet: Bytes) -> anyhow::Result<()>;

    fn push_message(&mut self, node: Obj<RpcClientNode>, packet: Bytes) -> anyhow::Result<()>;

    fn push_deletion(&mut self, node: Obj<RpcClientNode>);

    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;

    fn reset(&mut self);
}

impl<K: RpcKind> KindStateErased for KindState<K> {
    fn register_loc(&self) -> &'static Location<'static> {
        self.register_loc
    }

    fn push_catchup(&mut self, node: Obj<RpcClientNode>, packet: Bytes) -> anyhow::Result<()> {
        self.catchups.push(QueuedCatchup {
            rpc: RpcClientHandle::new(node),
            packet: <K::Catchup as RpcPacket>::decode(&packet)
                .context("failed to parse catchup packet")?,
        });

        Ok(())
    }

    fn push_message(&mut self, node: Obj<RpcClientNode>, packet: Bytes) -> anyhow::Result<()> {
        self.messages.push(QueuedMessage {
            rpc: RpcClientHandle::new(node),
            packet: <K::ClientBound as RpcPacket>::decode(&packet)
                .context("failed to parse message packet")?,
        });

        Ok(())
    }

    fn push_deletion(&mut self, node: Obj<RpcClientNode>) {
        self.deletions.push(RpcClientHandle::new(node));
    }

    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }

    fn reset(&mut self) {
        self.catchups.clear();
        self.messages.clear();
        self.deletions.clear();
    }
}

component!(RpcClient);

impl RpcClient {
    pub fn new() -> Self {
        Self {
            node_id_map: FxHashMap::default(),
            kinds_by_name: FxHashMap::default(),
            kinds_by_ty: FxHashMap::default(),
            protocol_errors: Vec::new(),
            locked: true,
        }
    }

    pub fn report_error(&mut self, err: anyhow::Error) {
        self.protocol_errors.push(err);
    }

    pub fn report_result<T>(&mut self, res: anyhow::Result<T>) -> Option<T> {
        match res {
            Ok(v) => Some(v),
            Err(err) => {
                self.report_error(err);
                None
            }
        }
    }

    #[track_caller]
    pub fn define<K: RpcKind>(&mut self) {
        // Bind the ID
        match self.kinds_by_name.entry(K::ID) {
            hash_map::Entry::Vacant(entry) => {
                entry.insert(TypeId::of::<K>());
            }
            hash_map::Entry::Occupied(entry) => {
                panic!(
                    "kind with ID {:?} was already registered (location: {})",
                    K::ID,
                    self.kinds_by_ty[entry.get()].register_loc()
                );
            }
        };

        // Bind the type
        self.kinds_by_ty.insert(
            TypeId::of::<K>(),
            Arc::new(KindState::<K> {
                register_loc: Location::caller(),
                catchups: Vec::new(),
                messages: Vec::new(),
                deletions: Vec::new(),
            }),
        );
    }

    pub fn reset(&mut self) -> anyhow::Result<()> {
        assert!(self.locked);

        for entry in self.kinds_by_ty.values_mut() {
            Arc::get_mut(entry)
                .expect("cannot reset RpcClient while it's being queried")
                .reset();
        }

        self.locked = false;

        MultiError::from_iter(self.protocol_errors.drain(..).map(Err)).map_err(anyhow::Error::new)
    }

    pub fn recv_packet(mut self: Obj<Self>, packet: Bytes) {
        let cx = pack!(@env => Bundle<infer_bundle!('_)>);

        let res: anyhow::Result<()> = try_sync! {
            let static ..cx;

            let mut packet = MultiPartDecoder::new(packet);

            let header = packet.expect_rich::<RpcCbHeader>()?;

            match header {
                RpcCbHeader::CreateNode(node_id, kind_name) => {
                    let me_obj = self;
                    let me_ent = self.entity();
                    let me = &mut *self;

                    let catchup = packet.expect()?;

                    // See if we know of this RPC kind
                    let kind_id = me
                        .kinds_by_name
                        .get(&*kind_name)
                        .copied()
                        .with_context(|| format!("unknown RPC node kind ID: {kind_name:?}"))?;

                    let kind_state = me.kinds_by_ty.get_mut(&kind_id).unwrap();
                    let kind_state = Arc::get_mut(kind_state).unwrap();

                    // Create new RPC node with the appropriate ID
                    let hash_map::Entry::Vacant(entry) = me.node_id_map.entry(node_id) else {
                        anyhow::bail!("duplicate creation of {node_id:?}");
                    };

                    let rpc = Entity::new(me_ent).add(RpcClientNode {
                        client: me_obj,
                        kind_id,
                        node_id,
                        userdata_ty: TypeId::of::<()>(),
                        userdata: Index::DANGLING,
                    });

                    entry.insert(rpc);

                    kind_state.push_catchup(rpc, catchup)?;
                }
                RpcCbHeader::DeleteNode(node_id) => {
                    let rpc = self.lookup_any_node(node_id).with_context(|| {
                        format!("failed to find deletion target node with ID {node_id:?}")
                    })?;

                    let kind_state = self.kinds_by_ty.get_mut(&rpc.kind_id).unwrap();
                    let kind_state = Arc::get_mut(kind_state).unwrap();

                    kind_state.push_deletion(rpc);
                }
                RpcCbHeader::SendMessage(node_id) => {
                    let message = packet.expect()?;
                    let rpc = self.lookup_any_node(node_id).with_context(|| {
                        format!("failed to find messaging target node with ID {node_id:?}")
                    })?;

                    let kind_state = self.kinds_by_ty.get_mut(&rpc.kind_id).unwrap();
                    let kind_state = Arc::get_mut(kind_state).unwrap();

                    kind_state.push_message(rpc, message)?;
                }
            }
        };

        self.report_result(res);
    }

    pub fn lock(&mut self) {
        assert!(!self.locked);
        self.locked = true;
    }

    pub fn lookup_any_node(&self, id: RpcNodeId) -> Option<Obj<RpcClientNode>> {
        self.node_id_map.get(&id).copied()
    }

    pub fn lookup_node<T, K>(&self, id: RpcNodeId) -> Option<Obj<T>>
    where
        T: RpcClientKind<K>,
        K: RpcKind,
    {
        self.lookup_any_node(id)?.opt_userdata()
    }

    pub fn query_create<K: RpcKind>(&self) -> ClientCreateQuery<K> {
        assert!(self.locked);

        ClientCreateQuery {
            inner: self
                .kinds_by_ty
                .get(&TypeId::of::<K>())
                .unwrap_or_else(|| panic!("RPC kind {:?} was never registered", type_name::<K>()))
                .clone()
                .as_any()
                .downcast::<KindState<K>>()
                .unwrap(),
        }
    }

    pub fn query_msg<K: RpcKind>(&self) {
        todo!()
    }

    pub fn query_delete<K: RpcKind>(&self) {
        todo!()
    }
}

// === RpcClientNode === //

#[derive(Debug)]
pub struct RpcClientNode {
    client: Obj<RpcClient>,
    node_id: RpcNodeId,
    kind_id: TypeId,
    userdata_ty: TypeId,
    userdata: Index,
}

component!(RpcClientNode);

impl RpcClientNode {
    pub fn client(&self) -> Obj<RpcClient> {
        self.client
    }

    pub fn node_id(&self) -> RpcNodeId {
        self.node_id
    }

    pub fn opt_userdata<T: Component>(&self) -> Option<Obj<T>> {
        (self.userdata_ty == TypeId::of::<T>()).then_some(Obj::from_raw(self.userdata))
    }

    pub fn userdata<T: Component>(&self) -> Obj<T> {
        self.opt_userdata().unwrap()
    }

    pub fn bind_userdata<T: Component>(&mut self, value: Obj<T>) {
        self.userdata_ty = TypeId::of::<T>();
        self.userdata = Obj::raw(value);
    }
}

#[derive_where(Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct RpcClientHandle<K: RpcKind> {
    _ty: PhantomData<fn() -> K>,
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
    pub const fn new(raw: Obj<RpcClientNode>) -> Self {
        Self {
            _ty: PhantomData,
            raw,
        }
    }

    pub const fn raw(self) -> Obj<RpcClientNode> {
        self.raw
    }

    pub fn client(self) -> Obj<RpcClient> {
        self.raw().client()
    }

    pub fn node_id(self) -> RpcNodeId {
        self.raw().node_id()
    }

    pub fn opt_userdata<T>(self) -> Option<Obj<T>>
    where
        T: RpcClientKind<K>,
    {
        self.raw.opt_userdata()
    }

    pub fn userdata<T>(self) -> Obj<T>
    where
        T: RpcClientKind<K>,
    {
        self.raw.userdata()
    }

    pub fn bind_userdata<T>(mut self, value: Obj<T>)
    where
        T: RpcClientKind<K>,
    {
        self.raw.bind_userdata(value)
    }
}

// === ClientCreateQuery === //

#[derive_where(Debug, Clone)]
pub struct ClientCreateQuery<K: RpcKind> {
    inner: Arc<KindState<K>>,
}

impl<K: RpcKind> ClientCreateQuery<K> {
    pub fn iter(&self) -> ClientCreateQueryIter<'_, K> {
        ClientCreateQueryIter {
            iter: self.inner.catchups.iter(),
        }
    }
}

impl<'a, K: RpcKind> IntoIterator for &'a ClientCreateQuery<K> {
    type Item = ClientCreateRequest<'a, K>;
    type IntoIter = ClientCreateQueryIter<'a, K>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive_where(Debug, Clone)]
pub struct ClientCreateQueryIter<'a, K: RpcKind> {
    iter: slice::Iter<'a, QueuedCatchup<K>>,
}

impl<'a, K: RpcKind> Iterator for ClientCreateQueryIter<'a, K> {
    type Item = ClientCreateRequest<'a, K>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|item| ClientCreateRequest {
            rpc: item.rpc,
            packet: &item.packet,
        })
    }
}

#[derive_where(Copy, Clone)]
pub struct ClientCreateRequest<'a, K: RpcKind> {
    rpc: RpcClientHandle<K>,
    packet: &'a K::Catchup,
}

impl<'a, K: RpcKind> ClientCreateRequest<'a, K> {
    pub fn packet(self) -> &'a K::Catchup {
        self.packet
    }

    pub fn client(self) -> Obj<RpcClient> {
        self.rpc.client()
    }

    pub fn rpc(self) -> RpcClientHandle<K> {
        self.rpc
    }

    pub fn opt_userdata<T>(self) -> Option<Obj<T>>
    where
        T: RpcClientKind<K>,
    {
        self.rpc().opt_userdata()
    }

    pub fn userdata<T>(self) -> Obj<T>
    where
        T: RpcClientKind<K>,
    {
        self.rpc().userdata()
    }

    pub fn bind_userdata<T>(self, value: Obj<T>)
    where
        T: RpcClientKind<K>,
    {
        self.rpc.bind_userdata(value);
    }
}
