use glam::{Affine2, Vec2};

use super::{
    assets::AssetManager,
    buffer::DynamicBufferManager,
    depth::{DepthGenerator, PassScheduler},
    gfx_bundle::GfxContext,
    transform::TransformManager,
};

#[derive(Debug)]
pub struct CanvasBase {
    pub assets: AssetManager,
    pub buffers: DynamicBufferManager,
    pub depth_gen: DepthGenerator,
    pub gfx: GfxContext,
    pub transform: TransformManager,
}

#[derive(Debug)]
pub struct CanvasFinishDescriptor<'a> {
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub color_attachment: &'a wgpu::TextureView,
    pub color_load: wgpu::LoadOp<wgpu::Color>,
    pub depth_attachment: &'a wgpu::TextureView,
}

impl CanvasBase {
    pub fn new(assets: AssetManager, gfx: GfxContext) -> Self {
        let mut buffers = DynamicBufferManager::new(gfx.clone());
        let transform = TransformManager::new(&mut buffers);

        CanvasBase {
            assets,
            buffers,
            depth_gen: DepthGenerator::new(),
            gfx,
            transform,
        }
    }

    #[must_use]
    pub fn transform(&self) -> Affine2 {
        self.transform.transform()
    }

    pub fn set_transform(&mut self, xf: Affine2) {
        self.transform.set_transform(xf);
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

    pub fn finish(&mut self, descriptor: CanvasFinishDescriptor<'_>, renderer: &mut dyn Renderer) {
        let CanvasFinishDescriptor {
            encoder,
            color_attachment,
            color_load,
            depth_attachment,
        } = descriptor;

        renderer.finish(RendererFinish::Data(self));
        self.buffers.flush(encoder);

        let mut scheduler = PassScheduler::new();
        renderer.finish(RendererFinish::Passes(self, &mut scheduler));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("canvas render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_attachment,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: color_load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_attachment,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: wgpu::StoreOp::Discard,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        scheduler.exec(&mut pass);
    }
}

pub trait Renderer {
    fn finish(&mut self, req: RendererFinish<'_>);
}

pub enum RendererFinish<'a> {
    Data(&'a mut CanvasBase),
    Passes(&'a mut CanvasBase, &'a mut PassScheduler),
}
