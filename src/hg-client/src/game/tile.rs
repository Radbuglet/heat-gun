use std::{
    context::{pack, Bundle, DerefCx},
    fmt, mem,
};

use hg_ecs::{
    component,
    entity::{Component, EntityStore},
    AccessComp, AccessRes, Entity, Obj, WORLD,
};
use hg_utils::hash::FxHashMap;
use macroquad::{
    color::Color,
    math::{IVec2, Rect, Vec2},
    shapes::draw_rectangle,
};

// === TileId === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Default)]
pub struct TileId(pub u16);

impl TileId {
    pub fn from_usize(v: usize) -> Option<Self> {
        u16::try_from(v).ok().map(TileId)
    }

    pub fn usize(self) -> usize {
        self.0 as usize
    }
}

// === TileMap === //

const CHUNK_EDGE: usize = 16;
const CHUNK_AREA: usize = CHUNK_EDGE * CHUNK_EDGE;

fn decompose_pos(pos: IVec2) -> (IVec2, usize) {
    let chunk_size = IVec2::splat(CHUNK_EDGE as i32);
    let rel = pos.rem_euclid(chunk_size);

    (
        pos.div_euclid(chunk_size),
        rel.y as usize * CHUNK_EDGE + rel.x as usize,
    )
}

#[derive(Default)]
pub struct TileMap {
    curr_chunk_pos: IVec2,
    curr_chunk_data: TileChunk,
    chunks: FxHashMap<IVec2, TileChunk>,
}

impl fmt::Debug for TileMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TileMap").finish_non_exhaustive()
    }
}

impl TileMap {
    fn load_chunk(&mut self, chunk: IVec2) {
        if self.curr_chunk_pos == chunk {
            return;
        }

        let taken = self.chunks.remove(&chunk);

        if self.curr_chunk_data.solid_num == 0 {
            if let Some(taken) = taken {
                self.curr_chunk_data = taken;
            } else {
                // (we can keep the empty chunk as-is)
            }
        } else {
            let mut taken = taken.unwrap_or_default();
            mem::swap(&mut self.curr_chunk_data, &mut taken);
            self.chunks.insert(self.curr_chunk_pos, taken);
        }

        self.curr_chunk_pos = chunk;
    }

    pub fn get(&mut self, pos: IVec2) -> TileId {
        let (chunk, rel) = decompose_pos(pos);

        self.load_chunk(chunk);
        self.curr_chunk_data.data[rel]
    }

    pub fn set(&mut self, pos: IVec2, data: TileId) {
        let (chunk, rel) = decompose_pos(pos);

        self.load_chunk(chunk);

        let chunk = &mut self.curr_chunk_data;
        let cell = &mut chunk.data[rel];

        chunk.solid_num += (data.0 != 0) as i32 - (cell.0 != 0) as i32;
        *cell = data;
    }
}

struct TileChunk {
    solid_num: i32,
    data: Box<[TileId; CHUNK_AREA]>,
}

impl Default for TileChunk {
    fn default() -> Self {
        Self {
            solid_num: 0,
            data: Box::new([TileId(0); CHUNK_AREA]),
        }
    }
}

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

// === TileConfig === //

#[derive(Debug, Copy, Clone)]
pub struct TileConfig {
    pub offset: Vec2,
    pub size: Vec2,
}

impl TileConfig {
    pub fn new(offset: Vec2, size: Vec2) -> Self {
        Self { offset, size }
    }
    pub fn world_to_tile(self, world: Vec2) -> IVec2 {
        let world = world - self.offset;
        world.div_euclid(self.size).as_ivec2()
    }

    pub fn tile_to_world(self, tile: IVec2) -> Rect {
        let origin = self.offset + tile.as_vec2() * self.size;
        Rect::new(origin.x, origin.y, self.size.x, self.size.y)
    }
}

// === PaletteVisuals === //

#[derive(Debug, Copy, Clone)]
pub enum PaletteVisuals {
    Air,
    Solid(Color),
}

component!(PaletteVisuals);

impl PaletteVisuals {
    pub fn render(self, at: Rect) {
        match self {
            PaletteVisuals::Air => {}
            PaletteVisuals::Solid(color) => {
                draw_rectangle(at.x, at.y, at.w, at.h, color);
            }
        }
    }
}

// === TileLayer === //

#[derive(Debug)]
pub struct TileLayer {
    pub map: TileMap,
    pub config: TileConfig,
    pub palette: Obj<TilePalette>,
}

component!(TileLayer);

impl TileLayer {
    pub fn new(config: TileConfig, palette: Obj<TilePalette>) -> Self {
        Self {
            map: TileMap::default(),
            config,
            palette,
        }
    }

    pub fn render(
        mut self: Obj<Self>,
        cache: &mut impl PaletteCache<Item = PaletteVisuals>,
        visible: Rect,
    ) {
        let visible_origin = Vec2::new(visible.x, visible.y);
        let visible_size = Vec2::new(visible.w, visible.h);

        let tile_min = self.config.world_to_tile(visible_origin);
        let tile_max = self.config.world_to_tile(visible_origin + visible_size);

        let (tile_min, tile_max) = (tile_min.min(tile_max), tile_min.max(tile_max));

        for x in tile_min.x..=tile_max.x {
            for y in tile_min.y..=tile_max.y {
                let pos = IVec2::new(x, y);
                let rect = self.config.tile_to_world(pos);
                let tile = self.map.get(pos);
                let tile = cache.lookup(tile);

                tile.render(rect);
            }
        }
    }
}

// === TileRenderer === //

#[derive(Debug)]
pub struct TileRenderer {
    layers: Vec<Obj<TileLayer>>,
    cache: DensePaletteCache<PaletteVisuals>,
}

component!(TileRenderer);

impl TileRenderer {
    pub fn new(layers: Vec<Obj<TileLayer>>) -> Self {
        assert!(!layers.is_empty());
        let palette = layers[0].palette;

        Self {
            layers,
            cache: DensePaletteCache::new(palette),
        }
    }

    pub fn render(&mut self, visible_rect: Rect) {
        for &layer in &self.layers {
            layer.render(&mut self.cache, visible_rect);
        }
    }
}
