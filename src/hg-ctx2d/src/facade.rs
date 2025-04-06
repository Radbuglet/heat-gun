use glam::{Affine2, Vec2, Vec4};

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

    #[must_use]
    pub fn transform(&self) -> Affine2 {
        self.raw.transform()
    }

    pub fn set_transform(&mut self, xf: Affine2) {
        self.raw.set_transform(xf);
    }

    #[must_use]
    pub fn blend(&self) -> Option<wgpu::BlendState> {
        self.raw.blend()
    }

    pub fn set_blend(&mut self, state: Option<wgpu::BlendState>) {
        self.raw.set_blend(state);
    }

    pub fn set_scissor(&mut self, rect: Option<[u32; 4]>) {
        self.raw.set_scissor(rect);
    }

    pub fn start_clip(&mut self) {
        self.raw.start_clip();
    }

    pub fn end_clip(&mut self) {
        self.raw.end_clip();
    }

    pub fn unset_clip(&mut self) {
        self.raw.unset_clip();
    }

    pub fn fill_rect(&mut self, pos: Vec2, size: Vec2, color: Vec4) {
        self.brushes
            .solid_rect
            .push(&mut self.raw, pos, size, color);
    }

    pub fn finish(&mut self, descriptor: FinishDescriptor<'_>) {
        // TODO: Finish brushes

        self.raw.finish(descriptor);
    }

    pub fn reclaim(&mut self) {
        self.raw.reclaim();
    }
}
