use core::fmt;
use std::{
    any::type_name,
    context::{pack, unpack, Bundle},
};

use hg_utils::hash::hash_map::Entry;
use smallvec::SmallVec;
use thunderdome::{Arena, Index};

use crate::{
    archetype::{ArchetypeId, ArchetypeStore, ComponentId},
    obj::Component,
    resource,
    world::{ImmutableWorld, WorldFmt},
    AccessComp, AccessCompMut, AccessCompRef, AccessRes, Obj, Resource, WORLD,
};

// === Store === //

#[derive(Debug)]
pub struct EntityStore {
    entities: Arena<EntityInfo>,
    archetypes: ArchetypeStore,
    root: Entity,
}

#[derive(Debug)]
struct EntityInfo {
    archetype: ArchetypeId,
    index_in_parent: usize,
    parent: Option<Entity>,
    children: SmallVec<[Entity; 2]>,
}

impl Default for EntityStore {
    fn default() -> Self {
        let mut store = Self {
            entities: Default::default(),
            archetypes: Default::default(),
            root: Entity::DANGLING,
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
            if let Some(world) = world {
                let store = world.read::<EntityStore>();

                let mut f = f.debug_struct("Entity");

                f.field("id", &format_args!("0x{:x}", self.0.to_bits()));

                if let Some(entity) = store.entities.get(self.0) {
                    let comps = store.archetypes.components(entity.archetype);

                    for comp in comps {
                        (comp.debug_fmt)(world, *self, &mut f);
                    }
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
            index_in_parent: 0,
            parent: None,
            children: SmallVec::new(),
        });

        Self(index)
    }

    pub fn root() -> Entity {
        EntityStore::fetch().root
    }

    pub fn is_alive(self) -> bool {
        EntityStore::fetch().entities.contains(self.0)
    }

    pub fn parent(self) -> Option<Entity> {
        EntityStore::fetch().entities[self.0].parent
    }

    pub fn set_parent(self, parent: Option<Entity>) {
        let store = EntityStore::fetch_mut();

        // Remove from old parent
        let me = &mut store.entities[self.0];
        let old_parent = me.parent.take();
        let old_index = me.index_in_parent;

        if let Some(parent) = old_parent {
            let parent = &mut store.entities[parent.0];

            parent.children.swap_remove(old_index);
            if let Some(&moved) = parent.children.get(old_index) {
                store.entities[moved.0].index_in_parent = old_index;
            }
        }

        // Add to new parent
        if let Some(parent) = parent {
            let (me, parent_val) = store.entities.get2_mut(self.0, parent.0);
            let me = me.unwrap();
            let parent_val = parent_val.unwrap();

            me.index_in_parent = parent_val.children.len();
            me.parent = Some(parent);
            parent_val.children.push(self);
        }
    }

    pub fn add<T: Component>(
        self,
        value: T,
        cx: Bundle<(&WORLD, &mut AccessRes<EntityStore>, &mut AccessComp<T>)>,
    ) -> Obj<T> {
        let entities = EntityStore::fetch_mut(pack!(cx));
        let storage = &mut **<T::Arena>::fetch_mut(pack!(cx));

        // Ensure that the entity is alive
        let entity = &mut entities
            .entities
            .get_mut(self.0)
            .unwrap_or_else(|| panic!("{self:?} is not alive"));

        // See if we can update the existing component in-place.
        let entry = match storage.entity_map.entry(self) {
            Entry::Occupied(entry) => {
                let handle = *entry.get();
                storage.arena[handle] = value;
                return Obj::from_raw(handle);
            }
            Entry::Vacant(entry) => entry,
        };

        // Otherwise, create the new `Obj`...
        let handle = storage.arena.insert(value);
        entry.insert(handle);

        // ...and update the `EntityStore` to reflect the additional component.
        entity.archetype = entities
            .archetypes
            .lookup_extend(entity.archetype, ComponentId::of::<T>());

        Obj::from_raw(handle)
    }

    pub fn with<T: Component>(
        self,
        value: T,
        cx: Bundle<(&WORLD, &mut AccessRes<EntityStore>, &mut AccessComp<T>)>,
    ) -> Self {
        self.add(value, pack!(cx));
        self
    }

    pub fn try_get<T: Component>(self, cx: Bundle<AccessCompRef<'_, T>>) -> Option<Obj<T>> {
        <T::Arena>::fetch(pack!(cx))
            .entity_map
            .get(&self)
            .copied()
            .map(Obj::from_raw)
    }

    pub fn get<T: Component>(self, cx: Bundle<AccessCompRef<'_, T>>) -> Obj<T> {
        self.try_get(pack!(cx)).unwrap_or_else(|| {
            panic!(
                "{self:?} does not have component of type `{}`",
                type_name::<T>()
            )
        })
    }

    pub fn remove_now<T: Component>(
        self,
        cx: Bundle<(&WORLD, &mut AccessRes<EntityStore>, &mut AccessComp<T>)>,
    ) -> Option<T> {
        let store = EntityStore::fetch_mut(pack!(cx));

        let Some(entity) = store.entities.get_mut(self.0) else {
            return None;
        };

        entity.archetype = store
            .archetypes
            .lookup_remove(entity.archetype, ComponentId::of::<T>());

        self.remove_from_storage(pack!(cx))
    }

    pub(crate) fn remove_from_storage<T: Component>(
        self,
        cx: Bundle<AccessCompMut<'_, T>>,
    ) -> Option<T> {
        let storage = <T::Arena>::fetch_mut(pack!(cx));

        let Some(obj) = storage.entity_map.remove(&self) else {
            return None;
        };

        Some(storage.arena.remove(obj).unwrap())
    }

    pub fn destroy_now(self, cx: Bundle<(&mut WORLD, &mut AccessRes<EntityStore>)>) {
        let store = EntityStore::fetch_mut(pack!(cx));

        let Some(entity) = store.entities.remove(self.0) else {
            return;
        };

        let arch = entity.archetype;
        let arch_len = store.archetypes.components(arch).len();

        for i in 0..arch_len {
            let comp = EntityStore::fetch(pack!(cx)).archetypes.components(arch)[i];

            (comp.remove)(unpack!(cx => &mut WORLD), self);
        }

        for child in entity.children {
            child.destroy_now(pack!(cx));
        }
    }

    pub fn debug<'a>(self, cx: Bundle<&'a mut WORLD>) -> WorldFmt<'a, Self> {
        WorldFmt::new(self, pack!(cx))
    }
}
