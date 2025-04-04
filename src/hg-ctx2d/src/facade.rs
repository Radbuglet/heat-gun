use std::ops::{Deref, DerefMut};

use crate::{
    base::{
        assets::AssetManager,
        facade::{CanvasBase, CanvasFinishDescriptor, Renderer, RendererFinish},
        gfx_bundle::GfxContext,
    },
    brush::rect::RectRenderer,
};

#[derive(Debug)]
pub struct Canvas {
    base: CanvasBase,
    renderers: CanvasRenderers,
}

#[derive(Debug)]
struct CanvasRenderers {
    rect: RectRenderer,
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

    pub fn finish(&mut self, descriptor: CanvasFinishDescriptor<'_>) {
        self.base.finish(descriptor, &mut self.renderers);
    }
}

impl Renderer for CanvasRenderers {
    fn finish(&mut self, req: RendererFinish<'_>) {
        self.rect.finish(req);
    }
}

impl Deref for Canvas {
    type Target = CanvasBase;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl DerefMut for Canvas {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}
