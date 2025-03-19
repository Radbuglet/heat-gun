use std::{
    any::Any,
    borrow::Borrow,
    fmt, hash,
    marker::PhantomData,
    mem::ManuallyDrop,
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
};

// === AssetManager === //

#[derive(Debug, Clone, Default)]
pub struct AssetManager(Arc<AssetManagerInner>);

#[derive(Debug, Default)]
struct AssetManagerInner {
    asset_map: RwLock<FxHashMap<Key, Weak<dyn Any + Send + Sync>>>,
    id_gen: AtomicU64,
}

#[derive(Debug)]
struct Key {
    hash: u64,
    id: u64,
    func: usize,
    value: Box<dyn AssetKeyOwned>,
}

struct Value<T> {
    manager: Weak<AssetManagerInner>,
    hash: u64,
    id: u64,
    value: OnceLock<T>,
}

impl AssetManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load<C, K, O>(&self, context: C, key: K, loader: fn(&Self, C, K) -> O) -> Asset<O>
    where
        K: AssetKey,
        O: 'static + Send + Sync,
    {
        let entry = self
            .load_entry::<K, O>(&key, loader as usize)
            .downcast::<Value<O>>()
            .ok()
            .unwrap();

        entry.value.get_or_init(|| loader(self, context, key));

        Asset {
            _ty: PhantomData,
            entry: ManuallyDrop::new(entry),
        }
    }

    fn load_entry<K, O>(&self, key: &K, func: usize) -> Arc<dyn Any + Send + Sync>
    where
        K: AssetKey,
        O: 'static + Send + Sync,
    {
        fn check_key<K: AssetKey>(hash: u64, func: usize, key: &K, candidate: &Key) -> bool {
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

                let value = Arc::new(Value::<O> {
                    manager: Arc::downgrade(&self.0),
                    hash,
                    id,
                    value: OnceLock::new(),
                });
                let value = value as Arc<dyn Any + Send + Sync>;

                entry.insert(Arc::downgrade(&value));

                return value;
            }
            hash_map::RawEntryMut::Vacant(entry) => {
                let id = self.0.id_gen.fetch_add(1, Relaxed);
                let value = Arc::new(Value::<O> {
                    manager: Arc::downgrade(&self.0),
                    hash,
                    id,
                    value: OnceLock::new(),
                });
                let value = value as Arc<dyn Any + Send + Sync>;

                entry.insert_with_hasher(
                    hash,
                    Key {
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

    pub fn len(&self) -> usize {
        self.0.asset_map.read().unwrap().len()
    }
}

#[derive_where(Clone)]
pub struct Asset<T: 'static + Send + Sync> {
    _ty: PhantomData<fn(T) -> T>,
    entry: ManuallyDrop<Arc<Value<T>>>,
}

impl<T> hash::Hash for Asset<T>
where
    T: 'static + Send + Sync,
{
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.entry).hash(state);
    }
}

impl<T> Eq for Asset<T> where T: 'static + Send + Sync {}

impl<T> PartialEq for Asset<T>
where
    T: 'static + Send + Sync,
{
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.entry, &other.entry)
    }
}

impl<T> fmt::Debug for Asset<T>
where
    T: 'static + Send + Sync + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Asset").field(&**self).finish()
    }
}

impl<T> Deref for Asset<T>
where
    T: 'static + Send + Sync,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.entry.value.get().unwrap()
    }
}

impl<T> Drop for Asset<T>
where
    T: 'static + Send + Sync,
{
    fn drop(&mut self) {
        let entry = unsafe { ManuallyDrop::take(&mut self.entry) };

        let Some(entry) = Arc::into_inner(entry) else {
            // (entry still alive)
            return;
        };

        let Some(manager) = entry.manager.upgrade() else {
            return;
        };

        let mut map = manager.asset_map.write().unwrap();

        let hash_map::RawEntryMut::Occupied(entry) = map
            .raw_entry_mut()
            .from_hash(entry.hash, |v| v.id == entry.id)
        else {
            return;
        };

        entry.remove();
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
