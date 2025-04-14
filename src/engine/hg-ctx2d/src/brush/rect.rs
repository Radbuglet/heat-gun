use std::{borrow::Cow, mem};

use crevice::std430::AsStd430;
use glam::{Vec2, Vec3, Vec4};

use crate::base::{
    Asset, AssetLoader, Crevice, GfxContext, RawBrushDescriptor, RawBrushDescriptorRef,
    RawBrushHandle, RawCanvas,
};

#[derive(Debug)]
pub struct SolidRectBrush {
    handle: RawBrushHandle,
}

#[derive(Debug, Clone, AsStd430)]
pub struct SolidRectInstance {
    pub pos: Vec3,
    pub size: Vec2,
    pub color: Vec4,
}

impl SolidRectBrush {
    pub fn new(ctx: &mut RawCanvas) -> Self {
        Self {
            handle: ctx.create_brush(Self::pipeline(&mut ctx.assets().clone(), ctx.gfx())),
        }
    }

    pub fn push(&mut self, ctx: &mut RawCanvas, pos: Vec2, size: Vec2, color: Vec4) {
        ctx.draw(
            self.handle,
            1,
            &Crevice(&SolidRectInstance {
                pos: pos.extend(ctx.depth()),
                size,
                color,
            }),
        );
    }

    pub fn shader(assets: &mut impl AssetLoader, gfx: &GfxContext) -> Asset<wgpu::ShaderModule> {
        assets.load(gfx, (), |_assets, gfx, ()| {
            gfx.device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("solid rect shader"),
                    source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                        "../shaders/solid_rect.wgsl"
                    ))),
                })
        })
    }

    pub fn pipeline(assets: &mut impl AssetLoader, gfx: &GfxContext) -> Asset<RawBrushDescriptor> {
        assets.load_alias(gfx, (), |assets, gfx, ()| {
            let shader = Self::shader(assets, gfx);

            RawBrushDescriptorRef {
                label: Some("solid rect brush"),
                vertex_module: &shader,
                vertex_entry: Some("vs_main"),
                fragment_module: &shader,
                fragment_entry: Some("fs_main"),
                instance_stride: SolidRectInstance::std430_size_static() as _,
                instance_attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: mem::offset_of!(<SolidRectInstance as AsStd430>::Output, pos) as _,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: mem::offset_of!(<SolidRectInstance as AsStd430>::Output, size) as _,
                        shader_location: 1,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: mem::offset_of!(<SolidRectInstance as AsStd430>::Output, color)
                            as _,
                        shader_location: 2,
                    },
                ],
                uniforms: &[],
            }
            .intern(assets)
        })
    }
}
