use hg_ecs::{bind, component, Obj};

use crate::{
    base::tile::{DensePaletteCache, PaletteCache, TileLayerSet},
    utils::math::Segment,
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
                for pos in layer.config.actor_aabb_to_tile(aabb).inclusive().iter() {
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
        check_hull: |world, entity, aabb, translation| {
            todo!();
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
