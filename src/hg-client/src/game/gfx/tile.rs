use hg_ecs::{component, Obj};
use macroquad::color::Color;

use crate::{
    game::tile::{DensePaletteCache, PaletteCache as _, TileLayer},
    utils::math::{Aabb, MqAabbExt},
};

// === PaletteVisuals === //

#[derive(Debug, Copy, Clone)]
pub enum PaletteVisuals {
    Air,
    Solid(Color),
}

component!(PaletteVisuals);

impl PaletteVisuals {
    pub fn render(self, at: Aabb) {
        match self {
            PaletteVisuals::Air => {}
            PaletteVisuals::Solid(color) => {
                at.draw_solid(color);
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

    pub fn render(&mut self, visible: Aabb) {
        for &(mut layer) in &self.layers {
            let visible = layer.config.actor_aabb_to_tile(visible);

            for pos in visible.inclusive().iter() {
                let rect = layer.config.tile_to_actor_aabb(pos);
                let tile = layer.map.get(pos);
                let tile = self.cache.lookup(tile);

                tile.render(rect);
            }
        }
    }
}
