use std::{
    any::{type_name, Any},
    context::{infer_bundle, pack, Bundle},
    fmt,
    marker::PhantomData,
    mem,
    panic::Location,
    sync::Arc,
};

use anyhow::Context;
use bytes::Bytes;
use derive_where::derive_where;
use hg_ecs::{component, entity::Component, Entity, Index, Obj, Query};
use hg_utils::hash::{hash_map, FxHashMap};
use smallvec::SmallVec;

use crate::{
    base::{
        net::{FrameEncoder, MultiPartDecoder, MultiPartSerializeExt as _, RpcPacket},
        rpc::RpcSbHeader,
    },
    try_sync,
    utils::lang::{MultiError, NamedTypeId},
};

use super::{
    BadRpcNodeKindError, NoSuchRpcNodeError, RpcCbHeader, RpcKind, RpcNodeId, RpcNodeLookupError,
};

// === RpcClient === //

pub trait RpcClientKind<K: RpcKind>: Component {}

#[derive(Debug)]
pub struct RpcClient {
    node_id_map: FxHashMap<RpcNodeId, Obj<RpcClientNode>>,
    kinds_by_name: FxHashMap<&'static str, NamedTypeId>,
    kinds_by_ty: FxHashMap<NamedTypeId, Arc<dyn KindStateErased>>,
    send_queue: Vec<FrameEncoder>,
    protocol_errors: Vec<anyhow::Error>,
    frozen: bool,
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
            send_queue: Vec::new(),
            protocol_errors: Vec::new(),
            frozen: true,
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
                entry.insert(NamedTypeId::of::<K>());
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
            NamedTypeId::of::<K>(),
            Arc::new(KindState::<K> {
                register_loc: Location::caller(),
                catchups: Vec::new(),
                messages: Vec::new(),
                deletions: Vec::new(),
            }),
        );
    }

    pub fn reset(&mut self) -> anyhow::Result<()> {
        assert!(self.frozen);

        for entry in self.kinds_by_ty.values_mut() {
            Arc::get_mut(entry)
                .expect("cannot reset RpcClient while it's being queried")
                .reset();
        }

        self.frozen = false;

        MultiError::from_iter(self.protocol_errors.drain(..).map(Err)).map_err(anyhow::Error::new)
    }

    pub fn recv_packet(mut self: Obj<Self>, packet: Bytes) {
        let cx = pack!(@env => Bundle<infer_bundle!('_)>);
        let res = try_sync! {
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
                        userdata_ty: NamedTypeId::of::<()>(),
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

    #[must_use]
    pub fn flush_sends(&mut self) -> Vec<FrameEncoder> {
        mem::take(&mut self.send_queue)
    }

    pub fn freeze(&mut self) {
        assert!(!self.frozen);
        self.frozen = true;
    }

    pub fn lookup_any_node(&self, id: RpcNodeId) -> Result<Obj<RpcClientNode>, NoSuchRpcNodeError> {
        self.node_id_map
            .get(&id)
            .copied()
            .ok_or(NoSuchRpcNodeError { id })
    }

    pub fn lookup_node<T: Component>(&self, id: RpcNodeId) -> Result<Obj<T>, RpcNodeLookupError> {
        self.lookup_any_node(id)?.opt_userdata().map_err(Into::into)
    }

    pub fn query<K: RpcKind>(self: Obj<Self>) -> RpcClientQuery<K> {
        RpcClientQuery::new_from([self])
    }
}

// === RpcClientNode === //

#[derive(Debug)]
pub struct RpcClientNode {
    client: Obj<RpcClient>,
    node_id: RpcNodeId,
    kind_id: NamedTypeId,
    userdata_ty: NamedTypeId,
    userdata: Index,
}

component!(RpcClientNode);

impl RpcClientNode {
    pub fn client_ent(&self) -> Entity {
        self.client.entity()
    }

    pub fn client(&self) -> Obj<RpcClient> {
        self.client
    }

    pub fn node_id(&self) -> RpcNodeId {
        self.node_id
    }

    pub fn send<K: RpcKind>(mut self: Obj<Self>, packet: &K::ServerBound) {
        assert_eq!(self.kind_id, NamedTypeId::of::<K>());

        let mut encoder = FrameEncoder::new();

        encoder.encode_multi_part(packet);
        encoder.encode_multi_part(&RpcSbHeader::SendMessage(self.node_id));

        self.client.send_queue.push(encoder);
    }

    pub fn opt_userdata<T: Component>(&self) -> Result<Obj<T>, BadRpcNodeKindError> {
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

    pub fn userdata<T: Component>(&self) -> Obj<T> {
        self.opt_userdata().unwrap()
    }

    pub fn bind_userdata<T: Component>(&mut self, value: Obj<T>) {
        self.userdata_ty = NamedTypeId::of::<T>();
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

    pub fn client_ent(self) -> Entity {
        self.raw().client_ent()
    }

    pub fn client(self) -> Obj<RpcClient> {
        self.raw().client()
    }

    pub fn node_id(self) -> RpcNodeId {
        self.raw().node_id()
    }

    pub fn send(self, packet: &K::ServerBound) {
        self.raw().send::<K>(packet);
    }

    pub fn opt_userdata<T>(self) -> Result<Obj<T>, BadRpcNodeKindError>
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

// === RpcClientQuery === //

#[derive_where(Debug, Clone)]
pub struct RpcClientQuery<K: RpcKind> {
    inner: SmallVec<[Arc<KindState<K>>; 1]>,
}

impl<K: RpcKind> RpcClientQuery<K> {
    pub fn new() -> Self {
        Self::new_from(Query::new())
    }

    pub fn new_from(objs: impl IntoIterator<Item = Obj<RpcClient>>) -> Self {
        let cx = pack!(@env => Bundle<infer_bundle!('_)>);
        let inner = objs
            .into_iter()
            .map(|client| {
                let static ..cx;
                assert!(client.frozen);
                client
                    .kinds_by_ty
                    .get(&NamedTypeId::of::<K>())
                    .unwrap_or_else(|| {
                        panic!("RPC kind {:?} was never registered", type_name::<K>())
                    })
                    .clone()
                    .as_any()
                    .downcast::<KindState<K>>()
                    .unwrap()
            })
            .collect();

        Self { inner }
    }

    pub fn added<'a>(&'a self) -> impl Iterator<Item = ClientCreateRequest<'a, K>> + Clone + 'a {
        self.inner
            .iter()
            .flat_map(|v| v.catchups.iter())
            .map(|v| ClientCreateRequest {
                rpc: v.rpc,
                packet: &v.packet,
            })
    }

    pub fn msgs<'a>(&'a self) -> impl Iterator<Item = ClientMessageRequest<'a, K>> + Clone + 'a {
        self.inner
            .iter()
            .flat_map(|v| v.messages.iter())
            .map(|v| ClientMessageRequest {
                rpc: v.rpc,
                packet: &v.packet,
            })
    }

    pub fn removed<'a>(&'a self) -> impl Iterator<Item = RpcClientHandle<K>> + Clone + 'a {
        self.inner.iter().flat_map(|v| v.deletions.iter().copied())
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

    pub fn packet_target<T: Component>(self) -> Result<Obj<T>, RpcNodeLookupError>
    where
        K: RpcKind<Catchup = RpcNodeId>,
    {
        self.client().lookup_node(*self.packet())
    }

    pub fn rpc(self) -> RpcClientHandle<K> {
        self.rpc
    }

    pub fn client_ent(self) -> Entity {
        self.rpc.client_ent()
    }

    pub fn client(self) -> Obj<RpcClient> {
        self.rpc.client()
    }

    pub fn opt_userdata<T>(self) -> Result<Obj<T>, BadRpcNodeKindError>
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

#[derive_where(Copy, Clone)]
pub struct ClientMessageRequest<'a, K: RpcKind> {
    rpc: RpcClientHandle<K>,
    packet: &'a K::ClientBound,
}

impl<'a, K: RpcKind> ClientMessageRequest<'a, K> {
    pub fn packet(self) -> &'a K::ClientBound {
        self.packet
    }

    pub fn client(self) -> Obj<RpcClient> {
        self.rpc.client()
    }

    pub fn rpc(self) -> RpcClientHandle<K> {
        self.rpc
    }

    pub fn opt_userdata<T>(self) -> Result<Obj<T>, BadRpcNodeKindError>
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
}
