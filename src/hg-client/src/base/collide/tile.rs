use hg_ecs::{bind, component, Obj};
use macroquad::color::{BLUE, DARKBLUE, WHITE};

use crate::{
    base::{
        debug::debug_draw,
        tile::{DensePaletteCache, PaletteCache, TileLayerSet},
    },
    utils::math::AabbI,
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
        check_hull_percent: |world, entity, request| {
            bind!(world);

            let dbg = debug_draw().frame();
            let collider = &mut *entity.get::<TileCollider>();
            let mut max_trans = 1.;

            dbg.line_rect(request.candidate_aabb(), 15., DARKBLUE);

            for &(mut layer) in collider.map.layers() {
                for pos in layer
                    .config
                    .actor_aabb_to_tile(request.candidate_aabb())
                    .iter_inclusive()
                {
                    dbg.line_rect(layer.config.tile_to_actor_aabb(pos), 5., WHITE);
                }

                let step_size = layer.config.size;
                let mut aabb = request.start_aabb();

                let mut prev_tiles_covered = AabbI::ZERO;

                let steps_taken =
                    (max_trans * request.delta_len() / layer.config.size).ceil() as u64;

                'scan: for _ in 0..steps_taken {
                    let tiled_covered = layer.config.actor_aabb_to_tile(aabb).inclusive();

                    for tile in tiled_covered.diff_exclusive(prev_tiles_covered) {
                        let tile_mat = layer.map.get(tile);
                        let tile_mat = collider.cache.lookup(tile_mat);

                        match &*tile_mat {
                            PaletteCollider::Solid => {
                                dbg.line_rect(layer.config.tile_to_actor_aabb(tile), 15., BLUE);

                                let tile_aabb = layer.config.tile_to_actor_aabb(tile);
                                let candidate_max_trans = request.hull_cast_percent(tile_aabb);

                                if candidate_max_trans < max_trans {
                                    max_trans = candidate_max_trans;
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

            max_trans
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
