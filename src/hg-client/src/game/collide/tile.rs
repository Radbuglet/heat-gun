use hg_ecs::{bind, component, Obj};

use crate::game::tile::{DensePaletteCache, PaletteCache, TileLayer};

use super::bus::{ColliderMat, CustomColliderMat};

// === TileCollider === //

#[derive(Debug)]
pub struct TileCollider {
    layers: Vec<Obj<TileLayer>>,
    cache: DensePaletteCache<PaletteCollider>,
}

impl TileCollider {
    pub const MATERIAL: ColliderMat = ColliderMat::Custom(&CustomColliderMat {
        name: "tile collider",
        check_aabb: |world, entity, aabb| {
            bind!(world);

            let collider = &mut *entity.get::<TileCollider>();

            for layer in &mut collider.layers {
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
    });

    pub fn new(layers: Vec<Obj<TileLayer>>) -> Self {
        let palette = layers[0].palette;

        Self {
            layers,
            cache: DensePaletteCache::new(palette),
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
