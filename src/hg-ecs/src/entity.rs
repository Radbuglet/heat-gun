use thunderdome::{Arena, Index};

use crate::{
    archetype::{ArchetypeId, ArchetypeStore},
    resource, Resource,
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
        let store = EntityStore::fetch_mut();
        let index = store.entities.insert(EntityInfo {
            condemned: false,
            archetype: store.archetypes.root(),
        });

        Self(index)
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
