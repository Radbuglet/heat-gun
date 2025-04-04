use super::{
    assets::AssetManager,
    buffer::DynamicBufferManager,
    depth::{DepthGenerator, PassScheduler},
    gfx_bundle::GfxContext,
    transform::TransformManager,
};

// === CanvasBase === //

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
    pub color_format: wgpu::TextureFormat,
    pub color_load: wgpu::LoadOp<wgpu::Color>,
    pub depth_attachment: &'a wgpu::TextureView,
    pub depth_format: wgpu::TextureFormat,
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

    pub fn finish(&mut self, descriptor: CanvasFinishDescriptor<'_>, renderer: &mut dyn Renderer) {
        let CanvasFinishDescriptor {
            encoder,
            color_attachment,
            color_format,
            color_load,
            depth_attachment,
            depth_format,
        } = descriptor;

        renderer.finish(
            &mut FinishCtx {
                canvas: self,
                color_format,
                depth_format,
            },
            &mut FinishReq::Data,
        );
        self.buffers.flush(encoder);

        let mut scheduler = PassScheduler::new();
        renderer.finish(
            &mut FinishCtx {
                canvas: self,
                color_format,
                depth_format,
            },
            &mut FinishReq::Pass(&mut scheduler),
        );

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

        renderer.finish(
            &mut FinishCtx {
                canvas: self,
                color_format,
                depth_format,
            },
            &mut FinishReq::Reset,
        );
    }
}

// === Renderer === //

pub trait Renderer {
    fn finish(&mut self, ctx: &mut FinishCtx<'_>, req: &mut FinishReq<'_>) {
        match req {
            FinishReq::Data => self.finish_data(ctx),
            FinishReq::Pass(scheduler) => self.finish_pass(ctx, scheduler),
            FinishReq::Reset => self.finish_reset(ctx),
        }
    }

    fn finish_data(&mut self, ctx: &mut FinishCtx<'_>) {
        let _ = ctx;
    }

    fn finish_pass(&mut self, ctx: &mut FinishCtx<'_>, scheduler: &mut PassScheduler) {
        let _ = ctx;
        let _ = scheduler;
    }

    fn finish_reset(&mut self, ctx: &mut FinishCtx<'_>) {
        let _ = ctx;
    }
}

pub struct FinishCtx<'a> {
    pub canvas: &'a mut CanvasBase,
    pub color_format: wgpu::TextureFormat,
    pub depth_format: wgpu::TextureFormat,
}

pub enum FinishReq<'a> {
    Data,
    Pass(&'a mut PassScheduler),
    Reset,
}
