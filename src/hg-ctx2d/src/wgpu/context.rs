use std::marker::PhantomData;

use glam::{Mat2, Vec2, Vec3, Vec4};
use thunderdome::Index;

use crate::Context;

use super::{
    assets::AssetManager,
    instances::{
        load_solid_quad_pipeline, BrushHandle, InstanceRenderer, SolidQuadBrush, SolidQuadInstance,
        SolidQuadShader, SolidQuadUniforms,
    },
    utils::Crevice,
};

// === WgpuContext === //

#[derive(Debug)]
pub struct WgpuContext {
    assets: AssetManager,
    device: wgpu::Device,
    renderer: InstanceRenderer,
    fill_rect_shader: SolidQuadShader,
    fill_rect_brush: SolidQuadBrush,
}

impl WgpuContext {
    pub fn new(assets: AssetManager, device: wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let mut renderer = InstanceRenderer::new(device.clone(), assets.clone());
        let fill_rect_shader = renderer.create_shader(
            "solid quad",
            (*load_solid_quad_pipeline(&assets, &device, format)).clone(),
            Crevice::<SolidQuadUniforms>::new(),
            Crevice::<SolidQuadInstance>::new(),
        );

        Self {
            assets,
            device,
            renderer,
            fill_rect_shader,
            fill_rect_brush: BrushHandle {
                _ty: PhantomData,
                raw: Index::DANGLING,
            },
        }
    }

    pub fn reset(&mut self) {
        self.renderer.reset();

        self.fill_rect_brush = self.renderer.start_brush(
            self.fill_rect_shader,
            &SolidQuadUniforms {
                affine_mat: Mat2::IDENTITY,
                affine_trans: Vec2::ZERO,
            },
        );
    }

    pub fn prepare(&mut self, queue: &wgpu::Queue) {
        self.renderer.prepare(queue);
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.renderer.render(pass);
    }
}

impl Context for WgpuContext {
    fn fill_rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
        self.renderer.push_instance(
            self.fill_rect_brush,
            &SolidQuadInstance {
                pos: Vec3::new(x, y, self.renderer.next_depth()),
                size: Vec2::new(width, height),
                color: Vec4::new(1., 0., 1., 1.),
            },
        );
    }
}
