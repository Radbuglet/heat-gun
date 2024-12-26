use std::{any::TypeId, ops::Range};

use hg_utils::{
    hash::{hash_map::RawEntryMut, FxHashMap, IterHashExt},
    iter::{MergeIter, RemoveIter},
};
use rustc_hash::FxBuildHasher;
use thunderdome::{Arena, Index};

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct ComponentId(TypeId);

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct ArchetypeId(Index);

#[derive(Debug)]
pub struct ArchetypeStore {
    arena: Arena<ArchetypeData>,
    comp_buf: Vec<ComponentId>,
    map: FxHashMap<ArchetypeKey, ArchetypeId>,
    root: ArchetypeId,
}

#[derive(Debug)]
struct ArchetypeKey {
    hash: u64,
    comps: Range<usize>,
}

#[derive(Debug)]
struct ArchetypeData {
    comps: Range<usize>,
    pos: FxHashMap<ComponentId, ArchetypeId>,
    neg: FxHashMap<ComponentId, ArchetypeId>,
}

impl Default for ArchetypeStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ArchetypeStore {
    pub fn new() -> Self {
        let mut arena = Arena::new();
        let root = ArchetypeId(arena.insert(ArchetypeData {
            comps: 0..0,
            pos: FxHashMap::default(),
            neg: FxHashMap::default(),
        }));

        let mut map = FxHashMap::default();
        let hash = FxBuildHasher.hash_one_iter([] as [ComponentId; 0]);

        let RawEntryMut::Vacant(entry) = map.raw_entry_mut().from_hash(hash, |_| unreachable!())
        else {
            unreachable!();
        };

        entry.insert_with_hasher(
            hash,
            ArchetypeKey { hash, comps: 0..0 },
            root,
            |_| unreachable!(),
        );

        Self {
            arena,
            comp_buf: Vec::new(),
            map,
            root,
        }
    }

    pub fn root(&self) -> ArchetypeId {
        self.root
    }

    fn lookup<M: LookupMode>(&mut self, base: ArchetypeId, with: ComponentId) -> ArchetypeId {
        let base_data = &self.arena[base.0];

        // Attempt to find a cached extension/de-extension of an existing archetype.
        if let Some(&shortcut) = ternary(M::POLARITY, &base_data.pos, &base_data.neg).get(&with) {
            return shortcut;
        }

        // Determine the set of components for which we're looking.
        let comps = M::comps(&self.comp_buf[base_data.comps.clone()], with);
        let hash = FxBuildHasher.hash_one_iter(comps.clone());

        let entry = self.map.raw_entry_mut().from_hash(hash, |other| {
            if hash != other.hash {
                return false;
            }

            self.comp_buf[other.comps.clone()]
                .iter()
                .copied()
                .eq(comps.clone())
        });

        // If it exists already, use it!
        let entry = match entry {
            RawEntryMut::Occupied(entry) => {
                let target = *entry.get();

                let base_data = &mut self.arena[base.0];
                ternary(M::POLARITY, &mut base_data.pos, &mut base_data.neg).insert(with, target);

                return target;
            }
            RawEntryMut::Vacant(entry) => entry,
        };

        // Otherwise, we have to create an entirely new archetype.
        let range_start = self.comp_buf.len();
        let comps = comps.collect::<Vec<_>>();
        self.comp_buf.extend(comps);
        let comps = range_start..self.comp_buf.len();

        let mut new_data = ArchetypeData {
            comps: comps.clone(),
            pos: FxHashMap::default(),
            neg: FxHashMap::default(),
        };

        ternary(!M::POLARITY, &mut new_data.pos, &mut new_data.neg).insert(with, base);

        let new = ArchetypeId(self.arena.insert(new_data));

        let base_data = &mut self.arena[base.0];
        ternary(M::POLARITY, &mut base_data.pos, &mut base_data.neg).insert(with, new);

        entry.insert_with_hasher(hash, ArchetypeKey { hash, comps }, new, |entry| entry.hash);

        new
    }

    pub fn lookup_extend(&mut self, base: ArchetypeId, with: ComponentId) -> ArchetypeId {
        self.lookup::<ExtendLookupMode>(base, with)
    }

    pub fn lookup_remove(&mut self, base: ArchetypeId, without: ComponentId) -> ArchetypeId {
        self.lookup::<RemoveLookupMode>(base, without)
    }
}

trait LookupMode {
    const POLARITY: bool;

    fn comps(base: &[ComponentId], with: ComponentId) -> impl Iterator<Item = ComponentId> + Clone;
}

struct ExtendLookupMode;

impl LookupMode for ExtendLookupMode {
    const POLARITY: bool = true;

    fn comps(base: &[ComponentId], with: ComponentId) -> impl Iterator<Item = ComponentId> + Clone {
        MergeIter::new(base.iter().copied(), [with])
    }
}

struct RemoveLookupMode;

impl LookupMode for RemoveLookupMode {
    const POLARITY: bool = true;

    fn comps(base: &[ComponentId], with: ComponentId) -> impl Iterator<Item = ComponentId> + Clone {
        RemoveIter::new(base.iter().copied(), [with])
    }
}

fn ternary<T>(cond: bool, pos: T, neg: T) -> T {
    if cond {
        pos
    } else {
        neg
    }
}
