use hg_ecs::{bind, component, Obj};

use crate::base::tile::{DensePaletteCache, PaletteCache, TileLayerSet};

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

            for &(mut layer) in collider.map.layers() {
                for tile in layer
                    .config
                    .actor_aabb_to_tile(request.candidate_aabb())
                    .iter_inclusive()
                {
                    let tile_mat = layer.map.get(tile);
                    let tile_mat = collider.cache.lookup(tile_mat);

                    match &*tile_mat {
                        PaletteCollider::Solid => {
                            let tile_aabb = layer.config.tile_to_actor_aabb(tile);
                            let candidate_result = request.hull_cast(tile_aabb);
                            result = result.min(candidate_result);
                        }
                        PaletteCollider::Disabled => continue,
                    }
                }
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
