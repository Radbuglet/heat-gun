use glam::{Affine2, Vec2};

use crate::{
    base::{
        assets::AssetManager,
        facade::{CanvasBase, CanvasFinishDescriptor, FinishCtx, FinishReq, Renderer},
        gfx_bundle::GfxContext,
    },
    brush::rect::RectRenderer,
};

#[derive(Debug)]
pub struct Canvas {
    pub base: CanvasBase,
    pub renderers: CanvasRenderers,
}

#[derive(Debug)]
pub struct CanvasRenderers {
    pub rect: RectRenderer,
}

impl Canvas {
    pub fn new(assets: AssetManager, gfx: GfxContext) -> Self {
        let mut base = CanvasBase::new(assets, gfx);
        let rect = RectRenderer::new(&mut base);

        Self {
            base,
            renderers: CanvasRenderers { rect },
        }
    }

    #[must_use]
    pub fn transform(&self) -> Affine2 {
        self.base.transform.transform()
    }

    pub fn set_transform(&mut self, xf: Affine2) {
        self.base.transform.set_transform(xf);
    }

    pub fn apply_transform(&mut self, xf: Affine2) {
        self.set_transform(self.transform() * xf);
    }

    pub fn translate(&mut self, by: Vec2) {
        self.apply_transform(Affine2::from_translation(by));
    }

    pub fn scale(&mut self, by: Vec2) {
        self.apply_transform(Affine2::from_scale(by));
    }

    pub fn rotate_rad(&mut self, rad: f32) {
        self.apply_transform(Affine2::from_angle(rad));
    }

    pub fn rotate_deg(&mut self, deg: f32) {
        self.rotate_rad(deg.to_radians());
    }

    pub fn finish(&mut self, descriptor: CanvasFinishDescriptor<'_>) {
        self.base.finish(descriptor, &mut self.renderers);
    }
}

impl Renderer for CanvasRenderers {
    fn finish(&mut self, ctx: &mut FinishCtx<'_>, req: &mut FinishReq<'_>) {
        self.rect.finish(ctx, req);
    }
}
