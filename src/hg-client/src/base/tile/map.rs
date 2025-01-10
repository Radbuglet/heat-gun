use std::{fmt, mem};

use hg_utils::hash::FxHashMap;
use macroquad::math::{IVec2, Vec2};

use crate::utils::math::{AaLine, Aabb, AabbI, Axis2, TileFace};

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

// === TileConfig === //

#[derive(Debug, Copy, Clone)]
pub struct TileConfig {
    pub offset: Vec2,
    pub size: f32,
}

impl TileConfig {
    pub fn from_size(size: f32) -> Self {
        Self {
            size,
            offset: Vec2::ZERO,
        }
    }

    pub fn actor_to_tile_axis(&self, axis: Axis2, value: f32) -> i32 {
        let _ = axis;
        value.div_euclid(self.size).floor() as i32
    }

    pub fn actor_to_tile(&self, Vec2 { x, y }: Vec2) -> IVec2 {
        IVec2::new(
            self.actor_to_tile_axis(Axis2::X, x),
            self.actor_to_tile_axis(Axis2::Y, y),
        )
    }

    pub fn actor_aabb_to_tile(&self, aabb: Aabb) -> AabbI {
        AabbI {
            min: self.actor_to_tile(aabb.min),
            max: self.actor_to_tile(aabb.max),
        }
    }

    pub fn tile_to_actor_aabb(&self, IVec2 { x, y }: IVec2) -> Aabb {
        Aabb::new_sized(
            Vec2::new(x as f32, y as f32) * self.size,
            Vec2::splat(self.size),
        )
    }

    pub fn tile_edge_line(&self, tile: IVec2, face: TileFace) -> AaLine {
        self.tile_to_actor_aabb(tile).edge_line(face)
    }
}
