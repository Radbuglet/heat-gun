use std::{
    any::type_name,
    context::{pack, unpack, Bundle},
};

use hg_utils::hash::hash_map::Entry;
use thunderdome::{Arena, Index};

use crate::{
    archetype::{ArchetypeId, ArchetypeStore, ComponentId},
    obj::Component,
    resource, AccessComp, AccessCompMut, AccessCompRef, AccessRes, Obj, Resource, WORLD,
};

// === Store === //

#[derive(Debug, Default)]
pub struct EntityStore {
    entities: Arena<EntityInfo>,
    archetypes: ArchetypeStore,
}

#[derive(Debug)]
struct EntityInfo {
    archetype: ArchetypeId,
}

resource!(EntityStore);

// === Entity === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct Entity(Index);

impl Entity {
    pub fn new() -> Self {
        let index = EntityStore::fetch_mut().entities.insert(EntityInfo {
            archetype: ArchetypeId::EMPTY,
        });

        Self(index)
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
    }
}
