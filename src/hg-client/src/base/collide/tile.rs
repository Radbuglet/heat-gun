use hg_ecs::{bind, component, Obj};
use macroquad::{
    color::{BLUE, ORANGE},
    math::Vec2,
};

use crate::{
    base::{
        debug::debug_draw,
        tile::{DensePaletteCache, PaletteCache, TileLayerSet},
    },
    utils::math::{AabbI, SAFETY_THRESHOLD},
};

use super::bus::{ColliderMat, CustomColliderMat};

// === TileCollider === //

#[derive(Debug)]
pub struct TileCollider {
    map: Obj<TileLayerSet>,
    cache: DensePaletteCache<PaletteCollider>,
}

impl TileCollider {
    pub const MATERIAL: ColliderMat = ColliderMat::Custom(&CustomColliderMat {
        name: "tile collider",
        check_aabb: |world, entity, aabb| {
            bind!(world);

            let collider = &mut *entity.get::<TileCollider>();

            for &(mut layer) in collider.map.layers() {
                for pos in layer.config.actor_aabb_to_tile(aabb).iter_inclusive() {
                    let tile = layer.map.get(pos);

                    let collider = collider.cache.lookup(tile);

                    match &*collider {
                        PaletteCollider::Solid => {
                            return true;
                        }
                        PaletteCollider::Disabled => {
                            // (ignored)
                        }
                    }
                }
            }

            false
        },
        cast_hull: |world, entity, request| {
            bind!(world);

            let collider = &mut *entity.get::<TileCollider>();
            let mut result = request.result_clear();
            let mut dbg_aabb = None;

            for &(mut layer) in collider.map.layers() {
                let step_size = layer.config.size;
                let mut aabb = request
                    .start_aabb()
                    .grow(Vec2::splat(SAFETY_THRESHOLD * 2.0));

                let mut prev_tiles_covered = AabbI::ZERO;

                let steps_taken = (result.dist / layer.config.size).ceil() as u64;

                'scan: for _ in 0..=steps_taken {
                    let tiled_covered = layer.config.actor_aabb_to_tile(aabb).inclusive();

                    for tile in tiled_covered.diff_exclusive(prev_tiles_covered) {
                        let tile_mat = layer.map.get(tile);
                        let tile_mat = collider.cache.lookup(tile_mat);

                        match &*tile_mat {
                            PaletteCollider::Solid => {
                                let tile_aabb = layer.config.tile_to_actor_aabb(tile);
                                let candidate_result = request.hull_cast(tile_aabb);

                                debug_draw().frame().line_rect(
                                    layer.config.tile_to_actor_aabb(tile),
                                    15.,
                                    ORANGE,
                                );

                                if candidate_result < result {
                                    result = candidate_result;
                                    dbg_aabb = Some(layer.config.tile_to_actor_aabb(tile));
                                    break 'scan;
                                }
                            }
                            PaletteCollider::Disabled => continue,
                        }
                    }

                    aabb = aabb.translated(request.delta_norm() * step_size);
                    prev_tiles_covered = tiled_covered;
                }
            }

            if let Some(dbg_aabb) = dbg_aabb {
                debug_draw()
                    .frame()
                    .line_rect(dbg_aabb.grow(Vec2::splat(15.)), 15., BLUE);
            }

            result
        },
    });

    pub fn new(layers: Obj<TileLayerSet>) -> Self {
        Self {
            map: layers,
            cache: DensePaletteCache::new(layers.palette()),
        }
    }
}

component!(TileCollider);

// === PaletteCollider === //

#[derive(Debug)]
pub enum PaletteCollider {
    Solid,
    Disabled,
}

component!(PaletteCollider);
