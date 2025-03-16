use std::{
    any::Any,
    borrow::Borrow,
    fmt, hash,
    marker::PhantomData,
    ops::Deref,
    sync::{
        atomic::{AtomicU64, Ordering::*},
        Arc, OnceLock, RwLock,
    },
    time::{Duration, Instant},
};

use derive_where::derive_where;
use hg_utils::{
    hash::{fx_hash_one, hash_map, FxHashMap},
    impl_tuples,
};

// === AssetManager === //

#[derive(Debug)]
pub struct AssetManager {
    created: Instant,
    asset_map: RwLock<FxHashMap<Key, Arc<dyn AssetValueErased>>>,
}

#[derive(Debug)]
struct Key {
    hash: u64,
    value: Box<dyn AssetKeyOwned>,
}

impl AssetManager {
    pub fn load<C, K, O>(
        &self,
        context: C,
        key: K,
        loader: fn(C, K) -> (O, CachePolicy),
    ) -> Asset<O>
    where
        K: AssetKey,
        O: 'static + Send + Sync,
    {
        let entry = self.load_entry::<K, O>(&key);

        // Ensure that the asset is loaded
        let inner = entry.downcast_ref::<O>().unwrap().get_or_init(|| {
            let (value, policy) = loader(context, key);

            AssetValueInner {
                timeout: policy.timeout,
                last_use: AtomicU64::new(0),
                value,
            }
        });

        // Update LRU timestamp
        inner
            .last_use
            .store((Instant::now() - self.created).as_millis() as u64, Relaxed);

        Asset {
            _ty: PhantomData,
            entry,
        }
    }

    fn load_entry<K, O>(&self, key: &K) -> Arc<dyn AssetValueErased>
    where
        K: AssetKey,
        O: 'static + Send + Sync,
    {
        fn check_key<K: AssetKey>(hash: u64, key: &K, candidate: &Key) -> bool {
            candidate.hash == hash
                && candidate
                    .value
                    .as_any()
                    .downcast_ref::<K::Owned>()
                    .is_some_and(|v| key.matches(v))
        }

        let hash = fx_hash_one(&key);

        let assets = self.asset_map.read().unwrap();
        if let Some((_k, v)) = assets
            .raw_entry()
            .from_hash(hash, |candidate| check_key(hash, key, candidate))
        {
            return v.clone();
        }

        drop(assets);

        let mut assets = self.asset_map.write().unwrap();

        match assets
            .raw_entry_mut()
            .from_hash(hash, |candidate| check_key(hash, key, candidate))
        {
            hash_map::RawEntryMut::Occupied(entry) => {
                return entry.get().clone();
            }
            hash_map::RawEntryMut::Vacant(entry) => {
                let value = Arc::new(AssetValue::<O>::new());

                entry.insert_with_hasher(
                    hash,
                    Key {
                        hash,
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
        let now = Instant::now();

        self.asset_map
            .write()
            .unwrap()
            .retain(|_k, v| Arc::strong_count(v) != 1 || !v.did_expire(self.created, now));
    }
}

#[derive_where(Clone)]
pub struct Asset<T: 'static + Send + Sync> {
    _ty: PhantomData<fn(T) -> T>,
    entry: Arc<dyn AssetValueErased>,
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
        &self.entry.downcast_ref::<T>().unwrap().get().unwrap().value
    }
}

#[derive(Debug, Clone, Default)]
pub struct CachePolicy {
    pub timeout: Duration,
}

impl CachePolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = duration;
        self
    }
}

// === AssetManager Keys === //

pub trait AssetKey: hash::Hash {
    type Owned: AssetKeyOwned;

    fn to_owned(&self) -> Self::Owned;

    fn matches(&self, owned: &Self::Owned) -> bool;
}

macro_rules! impl_asset_key {
    ($($name:ident:$field:tt),*) => {
        impl<$($name: ToOwned),*> AssetKey for ($($name,)*)
        where
            $(
                $name: hash::Hash + Eq,
                $name::Owned: 'static + fmt::Debug + hash::Hash + Eq + Send + Sync,
            )*
        {
            type Owned = ($($name::Owned,)*);

            fn to_owned(&self) -> Self::Owned {
                ($(self.$field.to_owned(),)*)
            }

            fn matches(&self, #[allow(unused)] owned: &Self::Owned) -> bool {
                $(Borrow::<$name>::borrow(&owned.$field) == &self.$field && )* true
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
    T: 'static + fmt::Debug + hash::Hash + Eq + Send + Sync,
{
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// === AssetManager Values === //

type AssetValue<T> = OnceLock<AssetValueInner<T>>;

#[derive(Debug)]
struct AssetValueInner<T> {
    timeout: Duration,
    last_use: AtomicU64,
    value: T,
}

trait AssetValueErased: 'static + Send + Sync {
    fn as_any(&self) -> &dyn Any;

    fn did_expire(&self, created: Instant, now: Instant) -> bool;
}

impl<T> AssetValueErased for AssetValue<T>
where
    T: 'static + Send + Sync,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn did_expire(&self, created: Instant, now: Instant) -> bool {
        let Some(inner) = self.get() else {
            // This can only happen if all threads to have acquired the uninitialized entry panic
            // without actually initializing it. We let the entry be deleted to ensure that a
            // transient initialization error for an otherwise unused resource doesn't cause a leak.
            return true;
        };

        let last_used = created + Duration::from_millis(inner.last_use.load(Relaxed));
        let Some(since_use) = now.checked_duration_since(last_used) else {
            return false;
        };

        since_use > inner.timeout
    }
}

impl fmt::Debug for dyn AssetValueErased {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("dyn AssetEntryErased")
    }
}

impl dyn AssetValueErased {
    fn downcast_ref<T: 'static>(&self) -> Option<&AssetValue<T>> {
        self.as_any().downcast_ref()
    }
}
