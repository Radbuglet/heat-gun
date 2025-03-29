use glam::{Affine2, Vec2};
use hg_utils::hash::{hash_map, FxHashMap};
use thunderdome::{Arena, Index};

use super::{
    buffer::{DynamicBufferHandle, DynamicBufferManager, DynamicBufferOpts},
    Asset, AssetKeepAlive, AssetKey, AssetLoader, AssetManager, GfxContext, RefKey, StreamWrite,
};

// === RawShader === //

#[derive(Debug, Clone)]
pub struct RawShaderRef<'a> {
    pub label: Option<&'a str>,
    pub instance_stride: wgpu::BufferAddress,
    pub texture_count: u16,
    pub vertex_module: &'a Asset<wgpu::ShaderModule>,
    pub vertex_entry: Option<&'a str>,
    pub vertex_attributes: &'a [wgpu::VertexAttribute],
    pub fragment_module: &'a Asset<wgpu::ShaderModule>,
    pub fragment_entry: Option<&'a str>,
}

#[derive(Debug)]
pub struct RawShader {
    pub label: Option<String>,
    pub instance_stride: wgpu::BufferAddress,
    pub texture_count: u16,
    pub vertex_module: Asset<wgpu::ShaderModule>,
    pub vertex_entry: Option<String>,
    pub vertex_attributes: Vec<wgpu::VertexAttribute>,
    pub fragment_module: Asset<wgpu::ShaderModule>,
    pub fragment_entry: Option<String>,
}

// === RawContext === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct RawShaderHandle(pub Index);

#[derive(Debug)]
pub struct RawContext {
    gfx: GfxContext,
    assets: AssetManager,
    buffers: DynamicBufferManager,

    shaders: Arena<RawShaderState>,
    shader_map: FxHashMap<AssetKeepAlive, RawShaderHandle>,

    transforms: DynamicBufferHandle,
    transform_map: FxHashMap<Affine2, u32>,
    curr_transform: Affine2,
    curr_transform_offset: Option<u32>,
}

#[derive(Debug)]
struct RawShaderState {
    rc: u32,
    shader: Asset<RawShader>,
    instance_buffer: DynamicBufferHandle,
}

impl RawContext {
    pub fn new(gfx: GfxContext, assets: AssetManager) -> Self {
        let mut buffers = DynamicBufferManager::new(gfx.clone());
        let transforms = buffers.create(DynamicBufferOpts {
            label: Some("transformation uniform buffer"),
            maintain_cpu_copy: false,
            usages: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
        });

        Self {
            gfx,
            assets,
            buffers,
            shaders: Arena::new(),
            shader_map: FxHashMap::default(),
            transforms,
            transform_map: FxHashMap::default(),
            curr_transform: Affine2::IDENTITY,
            curr_transform_offset: None,
        }
    }

    pub fn gfx(&self) -> &GfxContext {
        &self.gfx
    }

    pub fn assets(&self) -> &AssetManager {
        &self.assets
    }

    // === Shader Management === //

    pub fn ref_shader(&mut self, shader: Asset<RawShader>) -> RawShaderHandle {
        let entry = match self.shader_map.entry(Asset::keep_alive(&shader).clone()) {
            hash_map::Entry::Vacant(entry) => entry,
            hash_map::Entry::Occupied(entry) => {
                let handle = *entry.get();
                self.shaders[handle.0].rc += 1;
                return handle;
            }
        };

        let instance_buffer = self.buffers.create(DynamicBufferOpts {
            label: Some(&format!("{:?} - instance buffer", shader.label)),
            maintain_cpu_copy: false,
            usages: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
        });

        let handle = RawShaderHandle(self.shaders.insert(RawShaderState {
            rc: 1,
            shader,
            instance_buffer,
        }));

        entry.insert(handle);
        handle
    }

    pub fn unref_shader(&mut self, shader: RawShaderHandle) {
        let state = &mut self.shaders[shader.0];

        state.rc -= 1;

        if state.rc > 0 {
            return;
        }

        self.shader_map
            .remove(Asset::keep_alive(&state.shader))
            .unwrap();

        self.buffers.destroy(state.instance_buffer);
    }

    // === Transform Management === //

    #[must_use]
    pub fn transform(&self) -> Affine2 {
        self.curr_transform
    }

    pub fn set_transform(&mut self, xf: Affine2) {
        self.curr_transform = xf;
        self.curr_transform_offset = None;
    }

    pub fn apply_transform(&mut self, xf: Affine2) {
        self.set_transform(self.transform() * xf);
    }

    pub fn translate(&mut self, dt: Vec2) {
        self.apply_transform(Affine2::from_translation(dt));
    }

    pub fn scale(&mut self, sf: Vec2) {
        self.apply_transform(Affine2::from_scale(sf));
    }

    pub fn rotate_rad(&mut self, rad: f32) {
        self.apply_transform(Affine2::from_angle(rad));
    }

    pub fn rotate_deg(&mut self, deg: f32) {
        self.rotate_rad(deg.to_radians());
    }

    // === Drawing === //

    pub fn begin_frame(&mut self) {}

    pub fn end_frame(&mut self, encoder: &mut wgpu::CommandEncoder) {}

    pub fn begin_clip(&mut self) {}

    pub fn set_clip(&mut self) {}

    pub fn unset_clip(&mut self) {}

    pub fn set_scissor(&mut self) {}

    pub fn set_blending(&mut self) {}

    pub fn draw(
        &mut self,
        shader: RawShaderHandle,
        instances: &impl StreamWrite,
        textures: &[Asset<wgpu::Texture>],
    ) {
        // TODO
    }
}

// === Assets === //

struct RawShaderPipelineDescriptor<'a> {
    shader: &'a Asset<RawShader>,
    color_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
    blend_state: Option<wgpu::BlendState>,
}

impl AssetKey for RawShaderPipelineDescriptor<'_> {
    type Owned = (
        Asset<RawShader>,
        wgpu::TextureFormat,
        wgpu::TextureFormat,
        Option<wgpu::BlendState>,
    );

    fn delegated(&self) -> impl AssetKey<Owned = Self::Owned> + '_ {
        (
            RefKey(self.shader),
            RefKey(&self.color_format),
            RefKey(&self.depth_format),
            RefKey(&self.blend_state),
        )
    }
}

impl RawShaderPipelineDescriptor<'_> {
    pub fn load<E>(
        &self,
        assets: &mut impl AssetLoader<Error = E>,
        gfx: &GfxContext,
    ) -> Result<Asset<wgpu::RenderPipeline>, E> {
        assets.load(gfx, self, |assets, gfx, req| {
            let layout = load_pipeline_layout(assets, gfx, req.shader.texture_count).unwrap();

            gfx.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: req.shader.label.as_deref(),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        module: &req.shader.vertex_module,
                        entry_point: req.shader.vertex_entry.as_deref(),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: req.shader.instance_stride,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &req.shader.vertex_attributes,
                        }],
                    },
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        unclipped_depth: false,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        conservative: false,
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: req.depth_format,
                        depth_write_enabled: true,
                        depth_compare: wgpu::CompareFunction::Less,
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &req.shader.fragment_module,
                        entry_point: req.shader.fragment_entry.as_deref(),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: req.color_format,
                            blend: req.blend_state,
                            write_mask: wgpu::ColorWrites::all(),
                        })],
                    }),
                    multiview: None,
                    cache: None,
                })
        })
    }
}

fn load_pipeline_layout<E>(
    assets: &mut impl AssetLoader<Error = E>,
    gfx: &GfxContext,
    texture_count: u16,
) -> Result<Asset<wgpu::PipelineLayout>, E> {
    todo!()
}
