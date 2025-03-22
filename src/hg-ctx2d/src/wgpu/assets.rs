use std::{
    any::Any,
    borrow::Borrow,
    fmt, hash,
    ops::Deref,
    sync::{
        atomic::{AtomicU64, Ordering::*},
        Arc, OnceLock, RwLock, Weak,
    },
};

use derive_where::derive_where;
use hg_utils::{
    hash::{fx_hash_one, hash_map, FxHashMap},
    impl_tuples,
    mem::MappedArc,
};

// === AssetManager === //

#[derive(Debug, Clone, Default)]
pub struct AssetManager(Arc<AssetManagerInner>);

#[derive(Debug, Default)]
struct AssetManagerInner {
    asset_map: RwLock<FxHashMap<AssetKeyErased, Weak<AssetEntry>>>,
    id_gen: AtomicU64,
}

#[derive(Debug)]
struct AssetKeyErased {
    hash: u64,
    id: u64,
    func: usize,
    value: Box<dyn AssetKeyOwned>,
}

type AssetEntryValue<T> = OnceLock<AssetEntryValueInner<T>>;

struct AssetEntryValueInner<T> {
    _keep_alive: Vec<AssetKeepAlive>,
    value: T,
}

struct AssetEntry<T: ?Sized = dyn Any + Send + Sync> {
    manager: Weak<AssetManagerInner>,
    hash: u64,
    id: u64,
    value: T,
}

impl<T: ?Sized> Drop for AssetEntry<T> {
    fn drop(&mut self) {
        let Some(manager) = self.manager.upgrade() else {
            return;
        };

        let mut map = manager.asset_map.write().unwrap();

        let hash_map::RawEntryMut::Occupied(entry) = map
            .raw_entry_mut()
            .from_hash(self.hash, |v| v.id == self.id)
        else {
            return;
        };

        // N.B. `AssetKeyErased` contains user a controlled destructor which may recursively drop
        // other `AssetEntry` arcs. We avoid a dead-lock by dropping the map before the entry.
        let kv = entry.remove_entry();
        drop(map);
        drop(manager);
        drop(kv);
    }
}

impl AssetManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.0.asset_map.read().unwrap().len()
    }

    pub fn load_untracked<C, K, O>(
        &self,
        context: C,
        key: K,
        loader: fn(&mut AssetManagerTracked, C, K) -> O,
    ) -> Asset<O>
    where
        K: AssetKey,
        O: 'static + Send + Sync,
    {
        let entry = self.load_entry::<K, O>(&key, loader as usize);

        Asset(MappedArc::new(entry, |entry| {
            &entry
                .value
                .downcast_ref::<AssetEntryValue<O>>()
                .unwrap()
                .get_or_init(|| {
                    let mut tracked = AssetManagerTracked::new(self.clone());
                    let out = loader(&mut tracked, context, key);

                    AssetEntryValueInner {
                        _keep_alive: tracked.into_keep_alive(),
                        value: out,
                    }
                })
                .value
        }))
    }

    fn load_entry<K, O>(&self, key: &K, func: usize) -> Arc<AssetEntry>
    where
        K: AssetKey,
        O: 'static + Send + Sync,
    {
        fn check_key<K: AssetKey>(
            hash: u64,
            func: usize,
            key: &K,
            candidate: &AssetKeyErased,
        ) -> bool {
            candidate.hash == hash
                && candidate.func == func
                && candidate
                    .value
                    .as_any()
                    .downcast_ref::<K::Owned>()
                    .is_some_and(|v| key.matches(v))
        }

        let hash = fx_hash_one((func, &key));

        let assets = self.0.asset_map.read().unwrap();
        if let Some((_k, v)) = assets
            .raw_entry()
            .from_hash(hash, |candidate| check_key(hash, func, key, candidate))
        {
            if let Some(v) = v.upgrade() {
                return v;
            }
        }

        drop(assets);

        let mut assets = self.0.asset_map.write().unwrap();

        match assets
            .raw_entry_mut()
            .from_hash(hash, |candidate| check_key(hash, func, key, candidate))
        {
            hash_map::RawEntryMut::Occupied(mut entry) => {
                if let Some(value) = entry.get().upgrade() {
                    return value;
                }

                // We can't reuse the key's existing ID since a dropped `Asset` may be in the
                // process of removing the original asset instance and we want to ensure that it
                // doesn't delete this revived entry.
                let id = self.0.id_gen.fetch_add(1, Relaxed);

                entry.key_mut().id = id;

                let value = Arc::new(AssetEntry::<AssetEntryValue<O>> {
                    manager: Arc::downgrade(&self.0),
                    hash,
                    id,
                    value: OnceLock::new(),
                });
                let value = value as Arc<AssetEntry>;

                entry.insert(Arc::downgrade(&value));

                return value;
            }
            hash_map::RawEntryMut::Vacant(entry) => {
                let id = self.0.id_gen.fetch_add(1, Relaxed);
                let value = Arc::new(AssetEntry::<AssetEntryValue<O>> {
                    manager: Arc::downgrade(&self.0),
                    hash,
                    id,
                    value: OnceLock::new(),
                });
                let value = value as Arc<AssetEntry>;

                entry.insert_with_hasher(
                    hash,
                    AssetKeyErased {
                        hash,
                        id,
                        func,
                        value: Box::new(key.to_owned()),
                    },
                    Arc::downgrade(&value),
                    |k| k.hash,
                );

                value
            }
        }
    }
}

pub trait AssetLoader: Sized {
    fn push_keep_alive(&mut self, keep_alive: &AssetKeepAlive);

    fn manager(&self) -> &AssetManager;

    fn load_untracked<C, K, O>(
        &self,
        context: C,
        key: K,
        loader: fn(&mut AssetManagerTracked, C, K) -> O,
    ) -> Asset<O>
    where
        K: AssetKey,
        O: 'static + Send + Sync,
    {
        self.manager().load_untracked(context, key, loader)
    }

    fn load<C, K, O>(
        &mut self,
        context: C,
        key: K,
        loader: fn(&mut AssetManagerTracked, C, K) -> O,
    ) -> Asset<O>
    where
        K: AssetKey,
        O: 'static + Send + Sync,
    {
        let asset = self.load_untracked(context, key, loader);
        self.push_keep_alive(asset.keep_alive());
        asset
    }
}

impl AssetLoader for AssetManager {
    fn push_keep_alive(&mut self, _keep_alive: &AssetKeepAlive) {}

    fn manager(&self) -> &AssetManager {
        self
    }
}

#[derive(Debug)]
pub struct AssetManagerTracked {
    manager: AssetManager,
    keep_alive: Vec<AssetKeepAlive>,
}

impl AssetManagerTracked {
    pub fn new(manager: AssetManager) -> Self {
        Self {
            manager,
            keep_alive: Vec::new(),
        }
    }

    pub fn keep_alive(&self) -> &[AssetKeepAlive] {
        &self.keep_alive
    }

    pub fn into_keep_alive(self) -> Vec<AssetKeepAlive> {
        self.keep_alive
    }
}

impl AssetLoader for AssetManagerTracked {
    fn push_keep_alive(&mut self, keep_alive: &AssetKeepAlive) {
        self.keep_alive.push(keep_alive.clone());
    }

    fn manager(&self) -> &AssetManager {
        &self.manager
    }
}

// === Asset === //

#[derive_where(Clone)]
pub struct Asset<T: ?Sized>(MappedArc<AssetEntry, T>);

impl<T: ?Sized + fmt::Debug> fmt::Debug for Asset<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Asset")
            .field("id", &MappedArc::original(&self.0).id)
            .field("value", &<MappedArc<_, _> as Deref>::deref(&self.0))
            .finish()
    }
}

impl<T: ?Sized> hash::Hash for Asset<T> {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        Arc::as_ptr(MappedArc::original(&self.0)).hash(state);
    }
}

impl<T: ?Sized> Eq for Asset<T> {}

impl<T: ?Sized> PartialEq for Asset<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(MappedArc::original(&self.0), MappedArc::original(&other.0))
    }
}

impl<T: ?Sized> Deref for Asset<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: ?Sized> Borrow<AssetKeepAlive> for Asset<T> {
    fn borrow(&self) -> &AssetKeepAlive {
        self.keep_alive()
    }
}

impl<T: ?Sized> Asset<T> {
    pub fn new_untracked(value: T) -> Self
    where
        T: 'static + Sized + Send + Sync,
    {
        Asset(MappedArc::new(
            Arc::new(AssetEntry::<AssetEntryValue<T>> {
                manager: Weak::default(),
                hash: 0,
                id: 0,
                value: OnceLock::from(AssetEntryValueInner {
                    _keep_alive: Vec::new(),
                    value,
                }),
            }),
            |v| {
                &v.value
                    .downcast_ref::<AssetEntryValue<T>>()
                    .unwrap()
                    .get()
                    .unwrap()
                    .value
            },
        ))
    }

    pub fn try_map<V: ?Sized, E>(
        me: Self,
        map: impl FnOnce(&T) -> Result<&V, E>,
    ) -> Result<Asset<V>, (Asset<T>, E)> {
        match MappedArc::try_map(me.0, map) {
            Ok(v) => Ok(Asset(v)),
            Err((v, e)) => Err((Asset(v), e)),
        }
    }

    pub fn map<V: ?Sized>(me: Self, map: impl FnOnce(&T) -> &V) -> Asset<V> {
        Asset(MappedArc::map(me.0, map))
    }

    pub fn keep_alive(&self) -> &AssetKeepAlive {
        unsafe {
            &*(MappedArc::original(&self.0) as *const Arc<AssetEntry> as *const AssetKeepAlive)
        }
    }
}

#[derive(Clone)]
#[repr(transparent)]
pub struct AssetKeepAlive(Arc<AssetEntry>);

impl fmt::Debug for AssetKeepAlive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("AssetKeepAlive").field(&self.0.id).finish()
    }
}

impl hash::Hash for AssetKeepAlive {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

impl Eq for AssetKeepAlive {}

impl PartialEq for AssetKeepAlive {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

// === AssetManager Keys === //

pub trait AssetKey: hash::Hash {
    type Owned: AssetKeyOwned;

    fn to_owned(&self) -> Self::Owned;

    fn matches(&self, owned: &Self::Owned) -> bool;
}

#[derive(Debug, Copy, Clone, Hash)]
pub struct CloneKey<T>(pub T);

impl<T> AssetKey for CloneKey<T>
where
    T: hash::Hash + Eq + ToOwned,
    T::Owned: 'static + fmt::Debug + Send + Sync,
{
    type Owned = T::Owned;

    fn to_owned(&self) -> Self::Owned {
        self.0.to_owned()
    }

    fn matches(&self, owned: &Self::Owned) -> bool {
        &self.0 == owned.borrow()
    }
}

#[derive(Debug, Copy, Clone, Hash)]
pub struct RefKey<'a, T: ?Sized>(pub &'a T);

impl<T> AssetKey for RefKey<'_, T>
where
    T: ?Sized + hash::Hash + Eq + ToOwned,
    T::Owned: 'static + fmt::Debug + Send + Sync,
{
    type Owned = T::Owned;

    fn to_owned(&self) -> Self::Owned {
        self.0.to_owned()
    }

    fn matches(&self, owned: &Self::Owned) -> bool {
        self.0 == owned.borrow()
    }
}

#[derive(Debug, Copy, Clone, Hash)]
pub struct ListKey<'a, T>(pub &'a [&'a T]);

impl<T> AssetKey for ListKey<'_, T>
where
    T: hash::Hash + Eq + ToOwned,
    T::Owned: 'static + fmt::Debug + Send + Sync,
{
    type Owned = Vec<T::Owned>;

    fn to_owned(&self) -> Self::Owned {
        self.0.iter().map(|v| (*v).to_owned()).collect()
    }

    fn matches(&self, owned: &Self::Owned) -> bool {
        if self.0.len() != owned.len() {
            return false;
        }

        self.0
            .iter()
            .zip(owned)
            .all(|(lhs, rhs)| *lhs == rhs.borrow())
    }
}

macro_rules! impl_asset_key {
    ($($name:ident:$field:tt),*) => {
        impl<$($name: AssetKey),*> AssetKey for ($($name,)*) {
            type Owned = ($($name::Owned,)*);

            fn to_owned(&self) -> Self::Owned {
                ($(self.$field.to_owned(),)*)
            }

            fn matches(&self, #[allow(unused)] owned: &Self::Owned) -> bool {
                $(self.$field.matches(&owned.$field) && )* true
            }
        }
    };
}

impl_tuples!(impl_asset_key);

pub trait AssetKeyOwned: 'static + fmt::Debug + Send + Sync {
    fn as_any(&self) -> &dyn Any;
}

impl<T> AssetKeyOwned for T
where
    T: 'static + fmt::Debug + Send + Sync,
{
    fn as_any(&self) -> &dyn Any {
        self
    }
}
