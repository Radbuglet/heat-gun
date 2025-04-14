use std::ops::{Deref, DerefMut};

use glam::{Vec2, Vec4};

use crate::{
    base::{AssetManager, FinishDescriptor, GfxContext, RawCanvas},
    brush::SolidRectBrush,
};

#[derive(Debug)]
pub struct Canvas {
    pub raw: RawCanvas,
    pub brushes: CanvasBrushes,
}

#[derive(Debug)]
pub struct CanvasBrushes {
    pub solid_rect: SolidRectBrush,
}

impl CanvasBrushes {
    pub fn new(ctx: &mut RawCanvas) -> Self {
        Self {
            solid_rect: SolidRectBrush::new(ctx),
        }
    }
}

impl Canvas {
    pub fn new(assets: AssetManager, gfx: GfxContext) -> Self {
        let mut raw = RawCanvas::new(assets, gfx);
        let brushes = CanvasBrushes::new(&mut raw);

        Self { raw, brushes }
    }

    pub fn fill_rect(&mut self, pos: Vec2, size: Vec2, color: Vec4) {
        self.brushes
            .solid_rect
            .push(&mut self.raw, pos, size, color);
    }

    pub fn finish(&mut self, descriptor: FinishDescriptor<'_>) {
        // TODO: finish brushes

        self.raw.finish_raw(descriptor);
    }

    pub fn reclaim(&mut self) {
        self.raw.reclaim_raw();
    }
}

impl Deref for Canvas {
    type Target = RawCanvas;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl DerefMut for Canvas {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.raw
    }
}
