use glam::{Mat2, Vec2, Vec3, Vec4};
use thunderdome::Index;

use crate::Context;

use super::{
    quad::{
        create_solid_quad_shader, QuadBrushHandle, QuadRenderer, QuadShaderHandle,
        SolidQuadInstance, SolidQuadUniforms,
    },
    utils::Crevice,
};

// === WgpuContext === //

#[derive(Debug)]
pub struct WgpuContext {
    quads: QuadRenderer,
    fill_rect_shader: QuadShaderHandle,
    fill_rect_brush: QuadBrushHandle,
}

impl WgpuContext {
    pub fn new(device: wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let mut quads = QuadRenderer::new(device, format);
        let fill_rect_shader = create_solid_quad_shader(&mut quads);

        Self {
            quads,
            fill_rect_shader,
            fill_rect_brush: QuadBrushHandle(Index::DANGLING),
        }
    }

    pub fn reset(&mut self) {
        self.quads.reset();

        self.fill_rect_brush = self.quads.start_brush(
            self.fill_rect_shader,
            &Crevice(&SolidQuadUniforms {
                affine_mat: Mat2::IDENTITY,
                affine_trans: Vec2::ZERO,
            }),
        );
    }

    pub fn prepare(&mut self, queue: &wgpu::Queue) {
        self.quads.prepare(queue);
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.quads.render(pass);
    }
}

impl Context for WgpuContext {
    fn fill_rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
        self.quads.push_instance(
            self.fill_rect_brush,
            &Crevice(&SolidQuadInstance {
                pos: Vec3::new(x, y, self.quads.next_depth()),
                size: Vec2::new(width, height),
                color: Vec4::new(1., 0., 1., 1.),
            }),
        );
    }
}
