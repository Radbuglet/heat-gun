use std::{
    any::type_name,
    context::{pack, Bundle, BundleItemSet, DerefCx, DerefCxMut},
    fmt, iter,
    marker::PhantomData,
    mem,
    ops::DerefMut,
    rc::Rc,
    slice,
};

use derive_where::derive_where;
use hg_utils::hash::{hash_map::Entry, FxHashMap, FxHashSet};
use thunderdome::{Arena, Index};

use crate::{
    archetype::{ArchetypeId, ArchetypeStore, ComponentId},
    resource,
    world::{can_format_entity, can_format_obj, ImmutableWorld, WorldFmt},
    AccessRes, AccessResRef, Resource, World, WORLD,
};

// === Store === //

#[derive(Debug)]
pub struct EntityStore {
    /// The root entity of the game. This is the only entity not to have a parent.
    root: Entity,

    /// An arena of all live entities.
    entities: Arena<EntityInfo>,

    /// Tracks all component archetypes in use.
    archetypes: ArchetypeStore,

    /// A snapshot of members of each archetype since the last `flush`.
    ///
    /// This `Rc` is cloned while the storage is being iterated and is otherwise exclusive.
    query_state: Rc<EntityQueryState>,

    /// Maps entities that have been reshaped to their original archetype. Destroyed entities are
    /// included in the `dead_entities` map instead.
    ///
    /// This is only used to patch up `archetype_members` during a `flush`.
    reshaped_entities: FxHashMap<Entity, ArchetypeId>,

    /// The set of original archetypes and position therein of entities destroyed through
    /// `destroy_now`. Entities in the empty archetype are not included.
    ///
    /// This is only used to patch up `archetype_members` during a `flush`.
    dead_entities: FxHashSet<(ArchetypeId, u32)>,

    // TODO: Document
    target_queue_state: EntityQueueState,

    // TODO: Document
    view_queue_state: Rc<EntityQueueState>,
}

#[derive(Debug)]
struct EntityInfo {
    /// The archetype describing the set of components the entity actively owns. This accounts for
    /// changes by `add` and `remove_now` but not queued operations such as `remove`, which defers
    /// its call to `remove_now` until before reshapes are applied.
    archetype: ArchetypeId,

    /// The index of the entity in its pre-`flush` archetype.
    index_in_archetype: u32,

    /// The index of the node in its parent's `children` vector.
    ///
    /// The MSB indicates whether the entity was condemned.
    index_in_parent_bitor_condemned: u32,

    /// The parent of the entity. This is guaranteed to be alive.
    parent: Option<Entity>,

    /// The entity's set of children. These are guaranteed to be alive.
    children: EntityChildren,
}

impl EntityInfo {
    fn index_in_parent(&self) -> u32 {
        self.index_in_parent_bitor_condemned & !(1 << 31)
    }

    fn condemned(&self) -> bool {
        self.index_in_parent_bitor_condemned >> 31 != 0
    }

    fn set_index_in_parent_unchecked(&mut self, idx: u32) {
        self.index_in_parent_bitor_condemned &= 1 << 31;
        self.index_in_parent_bitor_condemned |= idx;
    }

    fn set_index_in_parent(&mut self, idx: usize) {
        assert!(idx < (1u32 << 31) as usize, "node has too many children");
        self.set_index_in_parent_unchecked(idx as u32);
    }

    fn mark_condemned(&mut self) {
        self.index_in_parent_bitor_condemned |= 1 << 31;
    }
}

#[derive(Debug, Default)]
pub struct EntityQueryState {
    pub index_members: FxHashMap<ArchetypeId, Vec<Entity>>,
    pub comp_members: FxHashMap<(ArchetypeId, ComponentId), Vec<Index>>,
}

#[derive(Debug, Default)]
pub struct EntityQueueState {
    /// The set of entities which have been condemned to be destroyed.
    pub condemned: Vec<Entity>,

    /// The set of components condemned to be removed.
    ///
    /// The second vector is filled during queue normalization.
    pub to_remove: FxHashMap<ComponentId, (FxHashSet<Entity>, Vec<Index>)>,
}

impl EntityQueueState {
    pub fn is_empty(&self) -> bool {
        self.condemned.is_empty()
    }
}

impl Default for EntityStore {
    fn default() -> Self {
        let mut store = Self {
            root: Entity::DANGLING,
            entities: Arena::new(),
            archetypes: ArchetypeStore::new(),
            query_state: Rc::default(),
            reshaped_entities: FxHashMap::default(),
            dead_entities: FxHashSet::default(),
            target_queue_state: EntityQueueState::default(),
            view_queue_state: Rc::default(),
        };

        store.root = Entity::new_root(&mut store);
        store
    }
}

resource!(EntityStore);

// === Entity === //

#[derive(Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct Entity(Index);

impl fmt::Debug for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        ImmutableWorld::try_use_tls(|world| {
            if let Some(world) = world.filter(|_| can_format_entity(*self)) {
                let store = world.read::<EntityStore>();

                let mut f = f.debug_struct("Entity");

                f.field("id", &format_args!("0x{:x}", self.0.to_bits()));

                if let Some(entity) = store.entities.get(self.0) {
                    let comps = store.archetypes.components(entity.archetype);

                    for comp in comps {
                        (comp.debug_fmt)(world, *self, &mut f);
                    }

                    f.field("children", &entity.children.vec);
                } else {
                    f.field("is_alive", &false);
                }

                f.finish()
            } else {
                f.debug_struct("Entity")
                    .field("id", &format_args!("0x{:x}", self.0.to_bits()))
                    .finish()
            }
        })
    }
}

impl Entity {
    pub const DANGLING: Self = Self(Index::DANGLING);

    pub fn new(parent: Entity) -> Self {
        let node = Self::new_root(EntityStore::fetch_mut());
        node.set_parent(Some(parent));
        node
    }

    pub(crate) fn new_root(store: &mut EntityStore) -> Self {
        let index = store.entities.insert(EntityInfo {
            archetype: ArchetypeId::EMPTY,
            index_in_parent_bitor_condemned: 0,
            index_in_archetype: 0,
            parent: None,
            children: EntityChildren { vec: Rc::default() },
        });

        Self(index)
    }

    pub fn root() -> Entity {
        EntityStore::fetch().root
    }

    pub fn service<T: Component>(cx: Bundle<&AccessComp<T>>) -> Obj<T> {
        Self::root().get(pack!(cx))
    }

    pub fn is_alive(self) -> bool {
        EntityStore::fetch().entities.contains(self.0)
    }

    pub fn parent(self) -> Option<Entity> {
        EntityStore::fetch().entities[self.0].parent
    }

    pub fn children<'a>(self) -> EntityChildren {
        EntityStore::fetch().entities[self.0].children.clone()
    }

    pub fn set_parent(self, parent: Option<Entity>) {
        let store = EntityStore::fetch_mut();

        // Remove from old parent
        let me = &mut store.entities[self.0];
        let old_parent = me.parent.take();
        let old_index = me.index_in_parent();

        if let Some(parent) = old_parent {
            let parent = &mut store.entities[parent.0];

            parent.children.mutate().swap_remove(old_index as usize);
            if let Some(&moved) = parent.children.vec.get(old_index as usize) {
                store.entities[moved.0].set_index_in_parent_unchecked(old_index);
            }
        }

        // Add to new parent
        if let Some(parent) = parent {
            let (me, parent_val) = store.entities.get2_mut(self.0, parent.0);
            let me = me.unwrap();
            let parent_val = parent_val.unwrap();

            me.parent = Some(parent);
            me.set_index_in_parent(parent_val.children.len());

            parent_val.children.mutate().push(self);
        }
    }

    fn mark_shape_dirty_before_update(
        reshaped_entities: &mut FxHashMap<Entity, ArchetypeId>,
        entity: Entity,
        entity_info: &EntityInfo,
    ) {
        reshaped_entities
            .entry(entity)
            .or_insert(entity_info.archetype);
    }

    pub fn add<T: Component>(self, value: T, cx: Bundle<&mut AccessComp<T>>) -> Obj<T> {
        let store = EntityStore::fetch_mut();
        let storage = &mut **<T::Arena>::fetch_mut(pack!(@env, cx));

        // Ensure that the entity is alive
        let entity = &mut store
            .entities
            .get_mut(self.0)
            .unwrap_or_else(|| panic!("{self:?} is not alive"));

        assert!(!entity.condemned(), "{self:?} is condemned");

        // See if we can update the existing component in-place.
        let entry = match storage.entity_map.entry(self) {
            Entry::Occupied(entry) => {
                let handle = *entry.get();
                storage.arena[handle].1 = value;
                return Obj::from_raw(handle);
            }
            Entry::Vacant(entry) => entry,
        };

        // Otherwise, create the new `Obj`...
        let handle = storage.arena.insert((self, value));
        entry.insert(handle);

        // ...and update the `EntityStore` to reflect the additional component.
        Self::mark_shape_dirty_before_update(&mut store.reshaped_entities, self, entity);

        entity.archetype = store
            .archetypes
            .lookup_extend(entity.archetype, ComponentId::of::<T>());

        Obj::from_raw(handle)
    }

    pub fn with<T: Component>(self, value: T, cx: Bundle<&mut AccessComp<T>>) -> Self {
        self.add(value, pack!(cx));
        self
    }

    pub fn with_proc<T: BundleItemSet>(
        self,
        attach: impl FnOnce(Entity, Bundle<T>),
        cx: Bundle<T>,
    ) -> Self {
        attach(self, cx);
        self
    }

    pub fn try_get<T: Component>(self, cx: Bundle<&AccessComp<T>>) -> Option<Obj<T>> {
        <T::Arena>::fetch(pack!(@env, cx))
            .entity_map
            .get(&self)
            .copied()
            .map(Obj::from_raw)
    }

    pub fn get<T: Component>(self, cx: Bundle<&AccessComp<T>>) -> Obj<T> {
        self.try_get(pack!(cx)).unwrap_or_else(|| {
            panic!(
                "{self:?} does not have component of type `{}`",
                type_name::<T>()
            )
        })
    }

    pub fn try_deep_get<T: Component>(self, cx: Bundle<&AccessComp<T>>) -> Option<Obj<T>> {
        let mut iter = Some(self);

        while let Some(curr) = iter {
            if let Some(obj) = curr.try_get(pack!(cx)) {
                return Some(obj);
            }

            iter = curr.parent();
        }

        None
    }

    pub fn deep_get<T: Component>(self, cx: Bundle<&AccessComp<T>>) -> Obj<T> {
        self.try_deep_get(pack!(cx)).unwrap_or_else(|| {
            panic!(
                "{self:?} and its ancestry do not have component of type `{}`",
                type_name::<T>(),
            )
        })
    }

    pub fn remove<T: Component>(self) {
        EntityStore::fetch_mut()
            .target_queue_state
            .to_remove
            .entry(ComponentId::of::<T>())
            .or_default()
            .0
            .insert(self);
    }

    pub fn remove_now<T: Component>(self, cx: Bundle<&mut AccessComp<T>>) -> Option<T> {
        let store = EntityStore::fetch_mut();

        let Some(entity) = store.entities.get_mut(self.0) else {
            return None;
        };

        Self::mark_shape_dirty_before_update(&mut store.reshaped_entities, self, entity);

        entity.archetype = store
            .archetypes
            .lookup_remove(entity.archetype, ComponentId::of::<T>());

        self.remove_from_storage(pack!(cx))
    }

    pub(crate) fn remove_from_storage<T: Component>(
        self,
        cx: Bundle<&mut AccessComp<T>>,
    ) -> Option<T> {
        let storage = <T::Arena>::fetch_mut(pack!(@env, cx));

        let Some(obj) = storage.entity_map.remove(&self) else {
            return None;
        };

        Some(storage.arena.remove(obj).unwrap().1)
    }

    pub fn destroy(self) {
        let store = EntityStore::fetch_mut();
        let entity = &mut store.entities[self.0];

        if entity.condemned() {
            return;
        }

        entity.mark_condemned();
        store.target_queue_state.condemned.push(self);
    }

    pub fn destroy_now(self) {
        let store = EntityStore::fetch_mut();

        // Destroy entity information before calling destructors to avoid reentrant operations on
        // dying entities.
        let Some(entity) = store.entities.remove(self.0) else {
            return;
        };

        // Remove ourself from our parent
        if let Some(parent) = entity
            .parent
            .and_then(|parent| store.entities.get_mut(parent.0))
        {
            let children = parent.children.mutate();
            let index_in_parent = entity.index_in_parent();
            children.swap_remove(index_in_parent as usize);

            if let Some(&moved) = children.get(index_in_parent as usize) {
                store.entities[moved.0].set_index_in_parent_unchecked(index_in_parent);
            }
        }

        // Remove from the reshaped map and into the dead map.
        let old_archetype = store
            .reshaped_entities
            .remove(&self)
            .unwrap_or(entity.archetype);

        if old_archetype != ArchetypeId::EMPTY {
            store
                .dead_entities
                .insert((entity.archetype, entity.index_in_archetype));
        }

        // Destroy all the components.
        let arch = entity.archetype;
        let arch_len = store.archetypes.components(arch).len();

        for i in 0..arch_len {
            let comp = EntityStore::fetch().archetypes.components(arch)[i];

            (comp.remove_no_tracking)(&mut WORLD, self);
        }

        // Destroy all the children.
        for child in &entity.children {
            child.destroy_now();
        }
    }

    pub fn debug<'a>(self, cx: Bundle<&'a mut WORLD>) -> WorldFmt<'a, Self> {
        WorldFmt::new(self, pack!(cx))
    }

    pub fn archetypes<'a>(cx: Bundle<AccessResRef<'a, EntityStore>>) -> &'a ArchetypeStore {
        &EntityStore::fetch(pack!(cx)).archetypes
    }

    pub fn archetype(self) -> ArchetypeId {
        EntityStore::fetch().entities[self.0].archetype
    }

    pub fn query_state<'a>(cx: Bundle<AccessResRef<'a, EntityStore>>) -> &'a Rc<EntityQueryState> {
        &EntityStore::fetch(cx).query_state
    }

    pub fn queue_state<'a>(cx: Bundle<AccessResRef<'a, EntityStore>>) -> &'a Rc<EntityQueueState> {
        &EntityStore::fetch(cx).view_queue_state
    }

    pub fn flush(mut f: impl FnMut(&mut World)) {
        // Process queued operations
        loop {
            // See if there are any operations remaining.
            let store = EntityStore::fetch_mut();
            let mut queue = mem::take(&mut store.target_queue_state);
            if queue.is_empty() {
                break;
            }

            // Normalize queue
            let orig_condemned_len = queue.condemned.len();

            for i in 0.. {
                let Some(condemned) = queue.condemned.get(i) else {
                    break;
                };

                let Some(info) = store.entities.get(condemned.0) else {
                    continue;
                };

                for child in &info.children {
                    queue.condemned.push(child);
                }
            }

            for &condemned in &queue.condemned {
                let Some(info) = store.entities.get(condemned.0) else {
                    continue;
                };

                for &comp in store.archetypes.components(info.archetype) {
                    queue.to_remove.entry(comp).or_default().0.insert(condemned);
                }
            }

            for (comp_id, (entities, list)) in queue.to_remove.iter_mut() {
                (comp_id.populate_indices)(&mut WORLD, entities, list);
            }

            // Freeze queue
            let queue = Rc::new(queue);
            EntityStore::fetch_mut().view_queue_state = queue.clone();

            // Run handler
            f(&mut WORLD);

            // Kill condemned entities
            // We don't need to delete the implicit children we added to the condemnation queue
            // because they'll be deleted by `destroy_now`'s recursive logic.
            for &condemned in &queue.condemned[..orig_condemned_len] {
                // Potential double-free on children is OK because this ignores deletions of
                // entities that have already been destroyed.
                condemned.destroy_now();
            }

            // Remove components
            for (comp, (deleted, _indices)) in &queue.to_remove {
                (comp.remove_for_deferred)(&mut WORLD, deleted);
            }
        }

        // Process reshape requests.
        let store = EntityStore::fetch_mut();
        let archetype_members = Rc::get_mut(&mut store.query_state)
            .expect("cannot `flush` the world while it is still being iterated over");

        // Begin with entity destruction since we don't want to try to move the indices of dead
        // entities as we update the archetypes.
        for (arch, idx) in store.dead_entities.drain() {
            let index_members = archetype_members.index_members.get_mut(&arch).unwrap();
            let comp_members = store.archetypes.components(arch);

            // We're going to be swap-removing a bunch of entities out of archetypes and we really
            // don't want to update the indices of dead entities we moved into the middle of the
            // archetype so let's trim all dead entities at the end of the archetype.
            while index_members
                .last()
                .is_some_and(|&entity| !store.entities.contains(entity.0))
            {
                index_members.pop();

                for &comp in comp_members {
                    archetype_members
                        .comp_members
                        .get_mut(&(arch, comp))
                        .unwrap()
                        .pop();
                }
            }

            // If the index is out of bound, we know that end-trimming took care of the entity
            // already.
            if idx as usize >= index_members.len() {
                continue;
            }

            // Otherwise, we need to swap-remove the entity...
            index_members.swap_remove(idx as usize);

            for &comp in comp_members {
                archetype_members
                    .comp_members
                    .get_mut(&(arch, comp))
                    .unwrap()
                    .swap_remove(idx as usize);
            }

            // ...and patch the index of the moved entity.
            if let Some(&moved) = index_members.get(idx as usize) {
                store.entities[moved.0].index_in_archetype = idx;
            }
        }

        // Now, we can handle move requests involving entirely live entities.
        for (entity, old_arch) in store.reshaped_entities.drain() {
            let own_info = &store.entities[entity.0];
            let curr_arch = own_info.archetype;
            let old_idx = own_info.index_in_archetype;

            // Skip over entities which haven't actually changed archetype.
            if curr_arch == old_arch {
                continue;
            }

            // Remove from the old archetype.
            if old_arch != ArchetypeId::EMPTY {
                let index_members = archetype_members.index_members.get_mut(&old_arch).unwrap();
                let comp_members = store.archetypes.components(old_arch);

                index_members.swap_remove(old_idx as usize);

                for &comp in comp_members {
                    archetype_members
                        .comp_members
                        .get_mut(&(old_arch, comp))
                        .unwrap()
                        .swap_remove(old_idx as usize);
                }

                // ...and patch the index of the moved entity.
                if let Some(&moved) = index_members.get(old_idx as usize) {
                    store.entities[moved.0].index_in_archetype = old_idx;
                }
            }

            // Move into the new archetype.
            if curr_arch != ArchetypeId::EMPTY {
                let comp_members = store.archetypes.components(curr_arch);
                let index_members = archetype_members
                    .index_members
                    .entry(curr_arch)
                    .or_insert_with(|| {
                        for &comp in comp_members {
                            archetype_members
                                .comp_members
                                .insert((curr_arch, comp), Vec::new());
                        }

                        Vec::new()
                    });

                // Move into the entity index
                store.entities[entity.0].index_in_archetype = u32::try_from(index_members.len())
                    .unwrap_or_else(|_| panic!("too many entities in archetype {curr_arch:?}"));

                index_members.push(entity);

                // ...and attach their components.
                for &comp in comp_members {
                    archetype_members
                        .comp_members
                        .get_mut(&(curr_arch, comp))
                        .unwrap()
                        .push(unsafe { (comp.fetch_idx)(&WORLD, entity) });
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct EntityChildren {
    vec: Rc<Vec<Entity>>,
}

impl fmt::Debug for EntityChildren {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.vec.iter()).finish()
    }
}

impl EntityChildren {
    pub fn len(&self) -> usize {
        self.vec.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vec.is_empty()
    }

    fn mutate(&mut self) -> &mut Vec<Entity> {
        Rc::get_mut(&mut self.vec)
            .expect("cannot modify the children of an entity while they're being iterated")
    }
}

impl<'a> IntoIterator for &'a EntityChildren {
    type Item = Entity;
    type IntoIter = iter::Copied<slice::Iter<'a, Entity>>;

    fn into_iter(self) -> Self::IntoIter {
        self.vec.iter().copied()
    }
}

// === Component === //

pub type AccessComp<T> = AccessRes<<T as Component>::Arena>;
pub type AccessCompRef<'a, T> = (&'a WORLD, &'a AccessComp<T>);
pub type AccessCompMut<'a, T> = (&'a WORLD, &'a mut AccessComp<T>);

pub trait Component: 'static + Sized + fmt::Debug {
    type Arena: Resource + DerefMut<Target = Storage<Self>>;
}

#[derive(Debug)]
#[derive_where(Default)]
pub struct Storage<T> {
    pub arena: Arena<(Entity, T)>,
    pub entity_map: FxHashMap<Entity, Index>,
}

#[doc(hidden)]
pub mod component_internals {
    pub use {
        super::{Component, Storage},
        crate::resource,
        std::ops::{Deref, DerefMut},
    };
}

#[macro_export]
macro_rules! component {
    ($($ty:ty),*$(,)?) => {$(
        const _: () = {
            #[derive(Default)]
            pub struct Storage($crate::entity::component_internals::Storage<$ty>);

            impl $crate::entity::component_internals::Deref for Storage {
                type Target = $crate::entity::component_internals::Storage<$ty>;

                fn deref(&self) -> &Self::Target {
                    &self.0
                }
            }

            impl $crate::entity::component_internals::DerefMut for Storage {
                fn deref_mut(&mut self) -> &mut Self::Target {
                    &mut self.0
                }
            }

            $crate::entity::component_internals::resource!(Storage);

            impl $crate::entity::component_internals::Component for $ty {
                type Arena = Storage;
            }
        };
    )*};
}

pub use component;

// === Obj === //

#[derive_where(Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct Obj<T: Component> {
    _ty: PhantomData<fn(T) -> T>,
    index: Index,
}

impl<T: Component> fmt::Debug for Obj<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let index = self.index.to_bits();

        ImmutableWorld::try_use_tls(|world| {
            if let Some(world) = world.filter(|_| can_format_obj(*self)) {
                let storage = world.read::<T::Arena>();

                if let Some((_owner, value)) = storage.arena.get(self.index) {
                    f.debug_tuple("Obj")
                        .field(&format_args!("0x{index:x}"))
                        .field(value)
                        .finish()
                } else {
                    struct Dead;

                    impl fmt::Debug for Dead {
                        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                            f.write_str("<dead>")
                        }
                    }

                    f.debug_tuple("Obj")
                        .field(&format_args!("0x{index:x}"))
                        .field(&Dead)
                        .finish()
                }
            } else {
                f.debug_tuple("Obj")
                    .field(&format_args!("0x{index:x}"))
                    .finish()
            }
        })
    }
}

impl<T: Component> Obj<T> {
    pub const DANGLING: Obj<T> = Obj {
        _ty: PhantomData,
        index: Index::DANGLING,
    };

    pub fn from_raw(index: Index) -> Self {
        Self {
            _ty: PhantomData,
            index,
        }
    }

    pub fn raw(me: Self) -> Index {
        me.index
    }

    pub fn is_alive(me: Self, cx: Bundle<&AccessComp<T>>) -> bool {
        <T::Arena>::fetch(pack!(@env, cx)).arena.contains(me.index)
    }

    pub fn entity(self, cx: Bundle<&AccessComp<T>>) -> Entity {
        <T::Arena>::fetch(pack!(@env, cx)).arena[self.index].0
    }

    pub fn debug<'a>(self, cx: Bundle<&'a mut WORLD>) -> WorldFmt<'a, Self> {
        WorldFmt::new(self, pack!(cx))
    }
}

impl<'i, 'o, T: Component> DerefCx<'i, 'o> for Obj<T> {
    type ContextRef = AccessCompRef<'o, T>;
    type TargetCx = T;

    fn deref_cx(&'i self, cx: Bundle<Self::ContextRef>) -> &'o Self::TargetCx {
        &<T::Arena>::fetch(pack!(cx)).arena[self.index].1
    }
}

impl<'i, 'o, T: Component> DerefCxMut<'i, 'o> for Obj<T> {
    type ContextMut = AccessCompMut<'o, T>;

    fn deref_cx_mut(&'i mut self, cx: Bundle<Self::ContextMut>) -> &'o mut Self::TargetCx {
        &mut <T::Arena>::fetch_mut(pack!(cx)).arena[self.index].1
    }
}
