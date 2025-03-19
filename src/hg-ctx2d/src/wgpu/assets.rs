use std::{
    any::Any,
    borrow::Borrow,
    fmt, hash,
    marker::PhantomData,
    ops::Deref,
    sync::{Arc, OnceLock, RwLock},
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
    asset_map: RwLock<FxHashMap<Key, Arc<dyn Any + Send + Sync>>>,
}

#[derive(Debug)]
struct Key {
    hash: u64,
    func: usize,
    value: Box<dyn AssetKeyOwned>,
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
        let entry = self.load_entry::<K, O>(&key, loader as usize);

        (&*entry as &(dyn Any + Send + Sync))
            .downcast_ref::<OnceLock<O>>()
            .unwrap()
            .get_or_init(|| loader(self, context, key));

        Asset {
            _ty: PhantomData,
            entry,
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
            return v.clone();
        }

        drop(assets);

        let mut assets = self.0.asset_map.write().unwrap();

        match assets
            .raw_entry_mut()
            .from_hash(hash, |candidate| check_key(hash, func, key, candidate))
        {
            hash_map::RawEntryMut::Occupied(entry) => {
                return entry.get().clone();
            }
            hash_map::RawEntryMut::Vacant(entry) => {
                let value = Arc::new(OnceLock::<O>::new());

                entry.insert_with_hasher(
                    hash,
                    Key {
                        hash,
                        func,
                        value: Box::new(key.to_owned()),
                    },
                    value.clone(),
                    |k| k.hash,
                );

                value
            }
        }
    }

    pub fn flush(&self) {
        self.0
            .asset_map
            .write()
            .unwrap()
            .retain(|_k, v| Arc::strong_count(v) != 1);
    }
}

#[derive_where(Clone)]
pub struct Asset<T: 'static + Send + Sync> {
    _ty: PhantomData<fn(T) -> T>,
    entry: Arc<dyn Any + Send + Sync>,
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
        &self
            .entry
            .downcast_ref::<OnceLock<T>>()
            .unwrap()
            .get()
            .unwrap()
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
