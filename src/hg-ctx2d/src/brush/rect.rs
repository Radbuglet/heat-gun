use glam::{Vec2, Vec4};

use crate::base::{
    buffer::{DynamicBufferHandle, DynamicBufferOpts},
    facade::{CanvasBase, Renderer, RendererFinish},
};

#[derive(Debug)]
pub struct RectRenderer {
    instances: DynamicBufferHandle,
}

impl RectRenderer {
    pub fn new(canvas: &mut CanvasBase) -> Self {
        Self {
            instances: canvas.buffers.create(DynamicBufferOpts {
                label: Some("rect instance data"),
                maintain_cpu_copy: false,
                usages: wgpu::BufferUsages::COPY_DST,
            }),
        }
    }

    pub fn push(&mut self, canvas: &mut CanvasBase, corner: Vec2, size: Vec2, color: Vec4) {
        todo!()
    }
}

impl Renderer for RectRenderer {
    fn finish(&mut self, req: RendererFinish<'_>) {
        match req {
            RendererFinish::Data(_) => todo!(),
            RendererFinish::Passes(_, _) => todo!(),
        }
    }
}
