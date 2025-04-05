use thunderdome::{Arena, Index};

use super::{
    assets::{Asset, AssetKey, AssetLoader, AssetManager, AssetRetainer, ListKey, RefKey},
    buffer::{DynamicBufferHandle, DynamicBufferManager, DynamicBufferOpts},
    depth::DepthGenerator,
    gfx_bundle::GfxContext,
    stream::StreamWrite,
    transform::{TransformManager, TransformUniformData},
};

// === Brush Descriptors === //

#[derive(Debug, Clone)]
pub struct RawBrushDescriptorRef<'a> {
    pub label: Option<&'a str>,
    pub vertex_module: &'a Asset<wgpu::ShaderModule>,
    pub vertex_entry: Option<&'a str>,
    pub fragment_module: &'a Asset<wgpu::ShaderModule>,
    pub fragment_entry: Option<&'a str>,
    pub instance_stride: wgpu::BufferAddress,
    pub instance_attributes: &'a [wgpu::VertexAttribute],
    pub uniforms: &'a [Option<&'a Asset<wgpu::BindGroupLayout>>],
}

#[derive(Debug)]
pub struct RawBrushDescriptor {
    pub label: Option<String>,
    pub vertex_module: Asset<wgpu::ShaderModule>,
    pub vertex_entry: Option<String>,
    pub fragment_module: Asset<wgpu::ShaderModule>,
    pub fragment_entry: Option<String>,
    pub instance_stride: wgpu::BufferAddress,
    pub instance_attributes: Vec<wgpu::VertexAttribute>,
    pub uniforms: Vec<Option<Asset<wgpu::BindGroupLayout>>>,
}

// === Brush Pipelines === //

fn load_pipeline_layout<E>(
    assets: &mut impl AssetLoader<Error = E>,
    gfx: &GfxContext,
    layouts: &[&Asset<wgpu::BindGroupLayout>],
) -> Result<Asset<wgpu::PipelineLayout>, E> {
    assets.load(gfx, ListKey(layouts), |_assets, gfx, ListKey(layouts)| {
        gfx.device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &layouts.iter().map(|v| &***v).collect::<Vec<_>>(),
                push_constant_ranges: &[],
            })
    })
}

#[derive(Debug, Clone)]
pub struct RawBrushPipelineDescriptor<'a> {
    pub descriptor: &'a Asset<RawBrushDescriptor>,
    pub color_format: wgpu::TextureFormat,
    pub depth_format: wgpu::TextureFormat,
    pub clip_mode: RawBrushClipMode,
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum RawBrushClipMode {
    SetClip,
    ObeyClip,
    IgnoreClip,
}

impl AssetKey for RawBrushPipelineDescriptor<'_> {
    type Owned = (
        Asset<RawBrushDescriptor>,
        wgpu::TextureFormat,
        wgpu::TextureFormat,
        RawBrushClipMode,
    );

    fn delegated(&self) -> impl AssetKey<Owned = Self::Owned> + '_ {
        (
            RefKey(self.descriptor),
            RefKey(&self.color_format),
            RefKey(&self.depth_format),
            RefKey(&self.clip_mode),
        )
    }
}

impl RawBrushPipelineDescriptor<'_> {
    pub fn load<E>(
        &self,
        assets: &mut impl AssetLoader<Error = E>,
        gfx: &GfxContext,
    ) -> Result<Asset<wgpu::RenderPipeline>, E> {
        assets.load(gfx, self, |assets, gfx, me| {
            let layout = TransformUniformData::group_layout(assets, gfx).unwrap();
            let layout = load_pipeline_layout(assets, gfx, &[&layout]);

            let stencil_state = match me.clip_mode {
                RawBrushClipMode::SetClip => Some(wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Always,
                    fail_op: wgpu::StencilOperation::Replace,
                    depth_fail_op: wgpu::StencilOperation::Replace,
                    pass_op: wgpu::StencilOperation::Replace,
                }),
                RawBrushClipMode::ObeyClip => Some(wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Equal,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::Keep,
                }),
                RawBrushClipMode::IgnoreClip => None,
            };

            let stencil_state = stencil_state
                .map(|face_state| wgpu::StencilState {
                    front: face_state,
                    back: face_state,
                    read_mask: u32::MAX,
                    write_mask: u32::MAX,
                })
                .unwrap_or(wgpu::StencilState::default());

            gfx.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: me.descriptor.label.as_deref(),
                    layout: Some(&*layout.unwrap()),
                    vertex: wgpu::VertexState {
                        module: &me.descriptor.vertex_module,
                        entry_point: me.descriptor.vertex_entry.as_deref(),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: me.descriptor.instance_stride,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &me.descriptor.instance_attributes,
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
                        format: me.color_format,
                        depth_write_enabled: true,
                        depth_compare: wgpu::CompareFunction::Less,
                        stencil: stencil_state,
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &me.descriptor.fragment_module,
                        entry_point: me.descriptor.fragment_entry.as_deref(),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: me.color_format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::all(),
                        })],
                    }),
                    multiview: None,
                    cache: None,
                })
        })
    }
}

// === RawCanvas Descriptors === //

#[derive(Debug)]
pub struct FinishDescriptor<'a> {
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub color_attachment: &'a wgpu::TextureView,
    pub color_format: wgpu::TextureFormat,
    pub color_load: wgpu::LoadOp<wgpu::Color>,
    pub depth_attachment: &'a wgpu::TextureView,
    pub depth_format: wgpu::TextureFormat,
}

// === RawCanvas === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct RawBrushHandle(pub Index);

#[derive(Debug)]
pub struct RawCanvas {
    assets: AssetManager,
    retainer: AssetRetainer,
    buffers: DynamicBufferManager,
    depth_gen: DepthGenerator,
    gfx: GfxContext,
    transform: TransformManager,

    brushes: Arena<RawBrush>,
}

#[derive(Debug)]
struct RawBrush {
    descriptor: Asset<RawBrushDescriptor>,
    buffer: DynamicBufferHandle,
    uniforms: Vec<Asset<wgpu::BindGroup>>,
}

impl RawCanvas {
    pub fn new(assets: AssetManager, gfx: GfxContext) -> Self {
        let retainer = AssetRetainer::new(assets.clone());
        let mut buffers = DynamicBufferManager::new(gfx.clone());
        let transform = TransformManager::new(&mut buffers);

        RawCanvas {
            assets,
            retainer,
            buffers,
            depth_gen: DepthGenerator::new(),
            gfx,
            transform,
            brushes: Arena::new(),
        }
    }

    pub fn create_brush(
        &mut self,
        descriptor: Asset<RawBrushDescriptor>,
        uniforms: impl IntoIterator<Item = Asset<wgpu::BindGroup>>,
    ) -> RawBrushHandle {
        let uniforms = uniforms.into_iter().collect();
        let buffer = self.buffers.create(DynamicBufferOpts {
            label: todo!(),
            maintain_cpu_copy: todo!(),
            usages: todo!(),
        });

        let handle = RawBrushHandle(self.brushes.insert(RawBrush {
            descriptor,
            buffer,
            uniforms,
        }));

        handle
    }

    pub fn destroy_brush(&mut self, brush: RawBrushHandle) {
        todo!()
    }

    pub fn set_scissor(&mut self, rect: Option<[u32; 4]>) {
        todo!()
    }

    pub fn set_blend(&mut self, state: Option<wgpu::BlendState>) {
        todo!()
    }

    pub fn start_clip(&mut self) {
        todo!()
    }

    pub fn end_clip(&mut self) {
        todo!()
    }

    pub fn unset_clip(&mut self) {
        todo!()
    }

    #[must_use]
    pub fn depth(&self) -> f32 {
        todo!()
    }

    pub fn draw(&mut self, brush: RawBrushHandle, data: &impl StreamWrite) {
        todo!()
    }

    pub fn finish(&mut self, descriptor: FinishDescriptor<'_>) {
        let FinishDescriptor {
            encoder,
            color_attachment,
            color_format,
            color_load,
            depth_attachment,
            depth_format,
        } = descriptor;

        self.buffers.flush(encoder);

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
    }
}
