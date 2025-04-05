use std::hash::Hash;

use crevice::std430::AsStd430;
use glam::{Affine2, Mat2, Vec2};
use hg_utils::hash::FxHashMap;

use super::{
    assets::{Asset, AssetLoader},
    buffer::{DynamicBufferHandle, DynamicBufferManager, DynamicBufferOpts},
    gfx_bundle::GfxContext,
    stream::Crevice,
};

// === TransformManager === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct TransformOffset(pub u32);

#[derive(Debug)]
pub(super) struct TransformManager {
    buffer: DynamicBufferHandle,
    offset_map: FxHashMap<Affine2Bits, TransformOffset>,
    curr_xf: Affine2,
    curr_offset: Option<TransformOffset>,
}

impl TransformManager {
    pub fn new(buffers: &mut DynamicBufferManager) -> Self {
        Self {
            buffer: buffers.create(DynamicBufferOpts {
                label: Some("transform uniform buffer"),
                maintain_cpu_copy: false,
                usages: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            }),
            offset_map: FxHashMap::default(),
            curr_xf: Affine2::IDENTITY,
            curr_offset: None,
        }
    }

    pub fn reset(&mut self, buffers: &mut DynamicBufferManager) {
        buffers.clear(self.buffer);
        self.offset_map.clear();
        self.curr_xf = Affine2::IDENTITY;
        self.curr_offset = None;
    }

    pub fn buffer(&self) -> DynamicBufferHandle {
        self.buffer
    }

    #[must_use]
    pub fn transform(&self) -> Affine2 {
        self.curr_xf
    }

    pub fn set_transform(&mut self, xf: Affine2) {
        self.curr_xf = xf;
        self.curr_offset = None;
    }

    pub fn transform_offset(&mut self, buffers: &mut DynamicBufferManager) -> TransformOffset {
        if let Some(curr) = self.curr_offset {
            return curr;
        }

        let offset = *self
            .offset_map
            .entry(Affine2Bits::from(self.curr_xf))
            .or_insert_with(|| {
                let offset = buffers.len(self.buffer) as u32;
                buffers.extend(
                    self.buffer,
                    &Crevice(&TransformUniformData {
                        xf_mat: self.curr_xf.matrix2,
                        xf_trans: self.curr_xf.translation,
                    }),
                );
                TransformOffset(offset)
            });

        self.curr_offset = Some(offset);
        offset
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
struct Affine2Bits([u32; 8]);

impl From<Affine2> for Affine2Bits {
    fn from(value: Affine2) -> Self {
        let [a, b, c, d, e, f] = value.to_cols_array();
        let [g, h] = value.translation.to_array();
        Self([a, b, c, d, e, f, g, h].map(f32::to_bits))
    }
}

// === Uniforms === //

#[derive(Debug, Copy, Clone, AsStd430)]
pub(super) struct TransformUniformData {
    pub xf_mat: Mat2,
    pub xf_trans: Vec2,
}

impl TransformUniformData {
    pub fn group_layout<E>(
        assets: &mut impl AssetLoader<Error = E>,
        gfx: &GfxContext,
    ) -> Result<Asset<wgpu::BindGroupLayout>, E> {
        assets.load(gfx, (), |_assets, gfx, ()| {
            gfx.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: todo!(),
                    entries: todo!(),
                })
        })
    }
}
