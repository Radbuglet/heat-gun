use std::{
    any::type_name,
    context::{pack, Bundle},
};

use hg_utils::hash::hash_map::Entry;
use thunderdome::{Arena, Index};

use crate::{
    archetype::{ArchetypeId, ArchetypeStore, ComponentId},
    obj::{AccessComp, AccessCompRef, Component, Obj},
    resource, AccessRes, Resource, WORLD,
};

// === Store === //

#[derive(Debug, Default)]
pub struct EntityStore {
    entities: Arena<EntityInfo>,
    condemned: Vec<Entity>,
    archetypes: ArchetypeStore,
}

#[derive(Debug)]
struct EntityInfo {
    condemned: bool,
    archetype: ArchetypeId,
}

resource!(EntityStore);

// === Entity === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct Entity(Index);

impl Entity {
    pub fn new() -> Self {
        let index = EntityStore::fetch_mut().entities.insert(EntityInfo {
            condemned: false,
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

    pub fn destroy(self) {
        let store = EntityStore::fetch_mut();

        let Some(info) = store.entities.get_mut(self.0) else {
            return;
        };

        if info.condemned {
            return;
        }

        info.condemned = true;
        store.condemned.push(self);
    }
}
