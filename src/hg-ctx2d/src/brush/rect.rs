use crevice::std430::AsStd430;
use glam::{Vec2, Vec4};

use crate::base::{
    buffer::{DynamicBufferHandle, DynamicBufferOpts},
    depth::{DepthRuns, PassScheduler},
    facade::{CanvasBase, FinishCtx, Renderer},
    stream::Crevice,
};

#[derive(Debug)]
pub struct RectRenderer {
    instances: DynamicBufferHandle,
    runs: DepthRuns<u32>,
}

#[derive(Debug, Clone, AsStd430)]
struct RectInstance {
    pub corner: Vec2,
    pub size: Vec2,
    pub color: Vec4,
    pub depth: f32,
}

impl RectRenderer {
    pub fn new(canvas: &mut CanvasBase) -> Self {
        Self {
            instances: canvas.buffers.create(DynamicBufferOpts {
                label: Some("rect instance data"),
                maintain_cpu_copy: false,
                usages: wgpu::BufferUsages::COPY_DST,
            }),
            runs: DepthRuns::new(0),
        }
    }

    pub fn push(&mut self, canvas: &mut CanvasBase, corner: Vec2, size: Vec2, color: Vec4) {
        canvas.buffers.extend(
            self.instances,
            &Crevice(&RectInstance {
                corner,
                size,
                color,
                depth: canvas.depth_gen.depth(),
            }),
        );

        self.runs
            .record_pos(canvas.depth_gen.epoch(), self.runs.last_pos() + 1);

        canvas.depth_gen.next_depth();
    }
}

impl Renderer for RectRenderer {
    fn finish_pass(&mut self, ctx: &mut FinishCtx<'_>, scheduler: &mut PassScheduler) {
        todo!()
    }

    fn finish_reset(&mut self, ctx: &mut FinishCtx<'_>) {
        ctx.canvas.buffers.clear(self.instances);
        self.runs.reset(0);
    }
}
