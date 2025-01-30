use glam::{IVec2, Vec2};
use smallvec::SmallVec;

use crate::utils::math::{ilerp_f32, Axis2, Sign, TileFace, Vec2Ext as _};

use super::TileConfig;

// === Raw TileCast === //

pub type TileCastHitBuffer = SmallVec<[TileCastHit; 2]>;

#[derive(Debug)]
pub struct TileCast {
    pos: Vec2,
    dir: Vec2,
    dist: f32,
    config: TileConfig,
}

#[derive(Debug, Copy, Clone)]
pub struct TileCastHit {
    pub face: TileFace,
    pub entered_tile: IVec2,
    pub isect_pos: Vec2,
    pub dist: f32,
}

impl TileCast {
    pub fn step_ray(&mut self) -> TileCastHitBuffer {
        let mut intersections = TileCastHitBuffer::new();

        // Collect all possible intersections
        let origin_tile = self.config.actor_to_tile(self.pos);
        let dest = self.pos + self.dir;

        for axis in Axis2::iter() {
            let origin_value = self.pos.axis(axis);
            let delta_value = self.dir.axis(axis);
            let delta_sign = Sign::of_biased(delta_value);
            let dest_value = dest.axis(axis);

            // Ensure that we crossed a block boundary
            if self.config.actor_to_tile_axis(axis, origin_value)
                == self.config.actor_to_tile_axis(axis, dest_value)
            {
                continue;
            }

            // If we did, add a ray intersection
            let iface_value = self
                .config
                .tile_edge_line(origin_tile, TileFace::compose(axis, delta_sign))
                .norm;

            let isect_pos = self
                .pos
                .lerp(self.dir, ilerp_f32(origin_value, dest_value, iface_value));

            intersections.push(TileCastHit {
                face: TileFace::compose(axis, delta_sign),
                entered_tile: IVec2::ZERO,
                dist: self.pos.distance(isect_pos),
                isect_pos,
            });
        }

        // Sort them by distance
        intersections.sort_by(|a, b| a.dist.total_cmp(&b.dist));

        // Update tile positions
        let mut tile_pos = origin_tile;
        for intersection in &mut intersections {
            tile_pos += intersection.face.as_ivec();
            intersection.entered_tile = tile_pos;
        }

        // Update distances
        for intersection in &mut intersections {
            intersection.dist += self.dist;
        }

        // Update ray state
        self.pos = dest;
        self.dist += self.dir.length();

        intersections
    }
}
