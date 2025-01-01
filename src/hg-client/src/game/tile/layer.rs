// === TileLayer === //

use hg_ecs::{component, Obj};

use super::{TileConfig, TileMap, TilePalette};

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
}
