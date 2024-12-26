use std::hash::{BuildHasher, BuildHasherDefault, Hash, Hasher};

use hashbrown::{HashMap, HashSet};
use rustc_hash::FxHasher;

pub type FxBuildHasher = BuildHasherDefault<FxHasher>;
pub type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;
pub type FxHashSet<T> = HashSet<T, FxBuildHasher>;

pub trait IterHashExt: BuildHasher {
    fn hash_one_iter(&self, iter: impl IntoIterator<Item: Hash>) -> u64 {
        let mut hasher = self.build_hasher();
        let mut len = 0;

        for item in iter {
            item.hash(&mut hasher);
            len += 1;
        }

        hasher.write_usize(len);
        hasher.finish()
    }
}

impl<B: ?Sized + BuildHasher> IterHashExt for B {}
