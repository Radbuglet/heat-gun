use std::{
    context::{pack, Bundle, DerefCx},
    fmt,
};

use hg_ecs::{
    component,
    entity::{Component, EntityStore},
    AccessComp, AccessRes, Entity, Obj, WORLD,
};
use hg_utils::hash::FxHashMap;

use super::TileId;

// === TilePalette === //

#[derive(Debug, Default)]
pub struct TilePalette {
    list: Vec<Entity>,
    by_name: FxHashMap<String, TileId>,
}

component!(TilePalette);

impl TilePalette {
    pub fn register(&mut self, name: &str, descriptor: Entity) -> TileId {
        let id = TileId::from_usize(self.list.len()).expect("too many tiles registered");
        self.list.push(descriptor);
        self.by_name.insert(name.to_string(), id);
        id
    }

    pub fn lookup(&self, id: TileId) -> Entity {
        self.list[id.usize()]
    }

    pub fn lookup_by_name(&self, id: &str) -> TileId {
        self.by_name[id]
    }
}

// === PaletteCache === //

pub type CacheLookupCx<'a, V> = (
    &'a WORLD,
    &'a AccessRes<EntityStore>,
    &'a AccessComp<TilePalette>,
    &'a AccessComp<V>,
);

pub trait PaletteCache {
    type Item: Component;

    fn new(palette: Obj<TilePalette>) -> Self;

    fn lookup(&mut self, id: TileId, cx: Bundle<CacheLookupCx<'_, Self::Item>>) -> Obj<Self::Item>;
}

pub struct DensePaletteCache<V: Component> {
    palette: Obj<TilePalette>,
    list: Vec<Option<Obj<V>>>,
}

impl<V: Component> fmt::Debug for DensePaletteCache<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DensePaletteCache").finish_non_exhaustive()
    }
}

impl<V: Component> PaletteCache for DensePaletteCache<V> {
    type Item = V;

    fn new(palette: Obj<TilePalette>) -> Self {
        Self {
            palette,
            list: Vec::new(),
        }
    }

    fn lookup(&mut self, id: TileId, cx: Bundle<CacheLookupCx<'_, Self::Item>>) -> Obj<Self::Item> {
        if self.list.len() <= id.usize() {
            self.list.resize(id.usize() + 1, None);
        }

        let slot = &mut self.list[id.usize()];

        if let Some(item) = *slot {
            return item;
        }

        *slot.insert(
            self.palette
                .deref_cx(pack!(cx))
                .lookup(id)
                .get::<V>(pack!(cx)),
        )
    }
}
