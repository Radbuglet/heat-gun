use hg_ecs::{component, Obj};

use super::{TileConfig, TileMap, TilePalette};

// === TileLayerSet === //

#[derive(Debug)]
pub struct TileLayerSet {
    layers: Vec<Obj<TileLayer>>,
}

component!(TileLayerSet);

impl TileLayerSet {
    pub fn new(layers: Vec<Obj<TileLayer>>) -> Self {
        Self { layers }
    }

    pub fn layers(&self) -> &[Obj<TileLayer>] {
        &self.layers
    }

    pub fn palette(&self) -> Obj<TilePalette> {
        self.layers[0].palette
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
}
