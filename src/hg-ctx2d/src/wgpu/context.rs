use crate::Context;

use super::quad::{create_solid_quad_shader, QuadRenderer, QuadShaderHandle};

// === WgpuContext === //

#[derive(Debug)]
pub struct WgpuContext {
    quads: QuadRenderer,
    fill_rect_shader: QuadShaderHandle,
}

impl WgpuContext {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let mut quads = QuadRenderer::new(device, queue, format);
        let fill_rect_shader = create_solid_quad_shader(&mut quads);

        Self {
            quads,
            fill_rect_shader,
        }
    }

    pub fn reset(&mut self) {
        self.quads.reset();
    }

    pub fn prepare(&mut self) {
        self.quads.prepare();
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.quads.render(pass);
    }
}

impl Context for WgpuContext {
    fn fill_rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
        todo!()
    }
}

// === StreamWritable === //

pub trait StreamWritable {
    fn write_to(&self, out: &mut impl Extend<u8>);
}

impl StreamWritable for [u8] {
    fn write_to(&self, out: &mut impl Extend<u8>) {
        out.extend(self.iter().copied());
    }
}

impl<T: bytemuck::Pod> StreamWritable for T {
    fn write_to(&self, out: &mut impl Extend<u8>) {
        bytemuck::bytes_of(self).write_to(out);
    }
}
