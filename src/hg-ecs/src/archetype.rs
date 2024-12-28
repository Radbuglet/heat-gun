use std::{
    cmp::Ordering,
    hash::Hash,
    ops::{Deref, Range},
};

use hg_utils::{
    hash::{hash_map::RawEntryMut, FxHashMap, IterHashExt},
    iter::{MergeIter, RemoveIter},
};
use index_vec::{define_index_type, IndexVec};
use rustc_hash::FxBuildHasher;

use crate::{obj::Component, Entity, World};

// === ComponentId === //

#[derive(Debug, Copy, Clone)]
pub struct ComponentId(&'static ComponentInfo);

impl ComponentId {
    pub fn of<T: Component>() -> Self {
        struct Helper<T>(T);

        impl<T: Component> Helper<T> {
            const INFO: &'static ComponentInfo = &ComponentInfo {
                remove: |world, entity| {
                    let mut world = world.reborrow();

                    entity.remove_from_storage::<T>(world.bundle());
                },
            };
        }

        Self(Helper::<T>::INFO)
    }
}

impl Deref for ComponentId {
    type Target = ComponentInfo;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl Hash for ComponentId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.0 as *const ComponentInfo).hash(state);
    }
}

impl Eq for ComponentId {}

impl PartialEq for ComponentId {
    fn eq(&self, other: &Self) -> bool {
        self.0 as *const ComponentInfo == other.0 as *const ComponentInfo
    }
}

impl Ord for ComponentId {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.0 as *const ComponentInfo).cmp(&(other.0 as *const ComponentInfo))
    }
}

impl PartialOrd for ComponentId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(&other))
    }
}

#[derive(Debug)]
pub struct ComponentInfo {
    pub remove: fn(&mut World, Entity),
}

// === ArchetypeId === //

define_index_type! {
    pub struct ArchetypeId = usize;
}

impl ArchetypeId {
    pub const EMPTY: Self = Self { _raw: 0 };
}

// === ArchetypeStore === //

#[derive(Debug)]
pub struct ArchetypeStore {
    arena: IndexVec<ArchetypeId, ArchetypeData>,
    comp_buf: Vec<ComponentId>,
    map: FxHashMap<ArchetypeKey, ArchetypeId>,
    comp_arches: FxHashMap<ComponentId, Vec<ArchetypeId>>,
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
        let mut arena = IndexVec::new();
        arena.push(ArchetypeData {
            comps: 0..0,
            pos: FxHashMap::default(),
            neg: FxHashMap::default(),
        });

        let mut map = FxHashMap::default();
        let hash = FxBuildHasher.hash_one_iter([] as [ComponentId; 0]);

        let RawEntryMut::Vacant(entry) = map.raw_entry_mut().from_hash(hash, |_| unreachable!())
        else {
            unreachable!();
        };

        entry.insert_with_hasher(
            hash,
            ArchetypeKey { hash, comps: 0..0 },
            ArchetypeId::EMPTY,
            |_| unreachable!(),
        );

        Self {
            arena,
            comp_buf: Vec::new(),
            map,
            comp_arches: FxHashMap::default(),
        }
    }

    fn lookup<M: LookupMode>(&mut self, base: ArchetypeId, with: ComponentId) -> ArchetypeId {
        let base_data = &self.arena[base];

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

                let base_data = &mut self.arena[base];
                ternary(M::POLARITY, &mut base_data.pos, &mut base_data.neg).insert(with, target);

                return target;
            }
            RawEntryMut::Vacant(entry) => entry,
        };

        // Otherwise, we have to create an entirely new archetype.

        // Create the `comps` range.
        let range_start = self.comp_buf.len();
        let comps_vec = comps.collect::<Vec<_>>();
        self.comp_buf.extend(comps_vec.iter().copied());
        let comps = range_start..self.comp_buf.len();

        // Create the `new` archetype with an appropriate back-ref to its original archetype.
        let mut new_data = ArchetypeData {
            comps: comps.clone(),
            pos: FxHashMap::default(),
            neg: FxHashMap::default(),
        };

        ternary(!M::POLARITY, &mut new_data.pos, &mut new_data.neg).insert(with, base);

        let new = self.arena.push(new_data);

        // Update `base` to contain a shortcut to this new archetype.
        let base_data = &mut self.arena[base];
        ternary(M::POLARITY, &mut base_data.pos, &mut base_data.neg).insert(with, new);

        // Update the `map`.
        entry.insert_with_hasher(hash, ArchetypeKey { hash, comps }, new, |entry| entry.hash);

        // Update `comp_arches`.
        for &comp in &comps_vec {
            self.comp_arches.entry(comp).or_default().push(new);
        }

        new
    }

    pub fn lookup_extend(&mut self, base: ArchetypeId, with: ComponentId) -> ArchetypeId {
        self.lookup::<ExtendLookupMode>(base, with)
    }

    pub fn lookup_remove(&mut self, base: ArchetypeId, without: ComponentId) -> ArchetypeId {
        self.lookup::<RemoveLookupMode>(base, without)
    }

    pub fn archetypes_with(&self, id: ComponentId) -> &[ArchetypeId] {
        self.comp_arches.get(&id).map_or(&[], |v| v)
    }

    pub fn components(&self, id: ArchetypeId) -> &[ComponentId] {
        let comps = self.arena[id].comps.clone();
        &self.comp_buf[comps]
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
