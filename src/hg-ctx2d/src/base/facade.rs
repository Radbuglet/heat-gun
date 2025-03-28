use std::hash::Hash;

use hg_utils::hash::FxHashMap;
use thunderdome::{Arena, Index};

use crate::{
    assets::{AssetKey, ListKey, RefKey},
    Asset, AssetLoader, AssetManager,
};

use super::{
    buffer::{DynamicBufferHandle, DynamicBufferManager, DynamicBufferOpts},
    GfxContext, StreamWrite,
};

// === Pipeline === //

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct Pipeline {
    pub label: Option<String>,
    pub instance_stride: wgpu::BufferAddress,
    pub instance_attributes: Vec<wgpu::VertexAttribute>,
    pub bind_group_layouts: Vec<Asset<wgpu::BindGroupLayout>>,
    pub vertex_module: Asset<wgpu::ShaderModule>,
    pub fragment_module: Asset<wgpu::ShaderModule>,
    pub vertex_entry_name: Option<String>,
    pub fragment_entry_name: Option<String>,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct PipelineRef<'a> {
    pub label: Option<&'a str>,
    pub instance_stride: wgpu::BufferAddress,
    pub instance_attributes: &'a [wgpu::VertexAttribute],
    pub bind_group_layouts: &'a [&'a Asset<wgpu::BindGroupLayout>],
    pub vertex_module: &'a Asset<wgpu::ShaderModule>,
    pub fragment_module: &'a Asset<wgpu::ShaderModule>,
    pub vertex_entry_name: Option<&'a str>,
    pub fragment_entry_name: Option<&'a str>,
}

impl PipelineRef<'_> {
    pub fn load<E>(&self, assets: &mut impl AssetLoader<Error = E>) -> Result<Asset<Pipeline>, E> {
        assets.load((), self, |_assets, (), key| key.to_owned_key())
    }
}

impl AssetKey for PipelineRef<'_> {
    type Owned = Pipeline;

    fn delegated(&self) -> impl AssetKey<Owned = Self::Owned> + '_ {
        self
    }

    fn hash_key(&self, state: &mut impl std::hash::Hasher) {
        self.hash(state);
    }

    fn to_owned_key(&self) -> Self::Owned {
        Pipeline {
            label: self.label.map(ToString::to_string),
            instance_stride: self.instance_stride,
            instance_attributes: self.instance_attributes.iter().copied().collect(),
            bind_group_layouts: ListKey(self.bind_group_layouts).to_owned_key(),
            vertex_module: self.vertex_module.clone(),
            fragment_module: self.fragment_module.clone(),
            vertex_entry_name: self.vertex_entry_name.map(ToString::to_string),
            fragment_entry_name: self.fragment_entry_name.map(ToString::to_string),
        }
    }

    fn matches_key(&self, owned: &Self::Owned) -> bool {
        self.label == owned.label.as_deref()
            && self.instance_stride == owned.instance_stride
            && self.instance_attributes == owned.instance_attributes
            && ListKey(self.bind_group_layouts).matches_key(&owned.bind_group_layouts)
            && self.vertex_module == &owned.vertex_module
            && self.fragment_module == &owned.fragment_module
            && self.vertex_entry_name == owned.vertex_entry_name.as_deref()
            && self.fragment_entry_name == owned.fragment_entry_name.as_deref()
    }
}

// === Context === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct BrushHandle(pub Index);

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct InstanceHandle(pub Index);

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct PassHandle(pub Index);

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Default)]
pub enum PreserveMode {
    #[default]
    Ephemeral,
    Cleared,
    Preserved,
}

#[derive(Debug, Clone, Default)]
pub struct BrushOptions<'a> {
    pub label: Option<&'a str>,
    pub granular: bool,
    pub preserve_mode: PreserveMode,
}

#[derive(Debug, Clone, Default)]
pub struct PassOptions<'a> {
    pub label: Option<&'a str>,
    pub preserve_mode: PreserveMode,
}

#[derive(Debug)]
pub struct Context {
    // Services
    assets: AssetManager,
    gfx: GfxContext,
    buffers: DynamicBufferManager,

    // State
    brushes: Arena<Brush>,
    instances: Arena<Instance>,
    passes: Arena<Pass>,
}

#[derive(Debug)]
struct Brush {
    preserve_mode: PreserveMode,
    pipeline: Asset<Pipeline>,
    buffer: DynamicBufferHandle,
    bind_groups: FxHashMap<u32, wgpu::BindGroup>,
    instances: Option<Vec<InstanceHandle>>,
    instance_count: u32,
}

#[derive(Debug, Copy, Clone)]
struct Instance {
    brush: BrushHandle,
    offset: wgpu::BufferAddress,
}

#[derive(Debug)]
struct Pass {
    label: Option<String>,
    preserve_mode: PreserveMode,
    commands: Vec<PassCommand>,
}

#[derive(Debug)]
enum PassCommand {
    Draw(BrushHandle),
}

impl Context {
    pub fn new(assets: AssetManager, gfx: GfxContext) -> Self {
        let buffers = DynamicBufferManager::new(gfx.clone());

        Self {
            assets,
            gfx,
            buffers,
            brushes: Arena::new(),
            instances: Arena::new(),
            passes: Arena::new(),
        }
    }

    pub fn assets(&self) -> &AssetManager {
        &self.assets
    }

    pub fn gfx(&self) -> &GfxContext {
        &self.gfx
    }

    // === Geometry management === //

    pub fn create_brush(
        &mut self,
        pipeline: Asset<Pipeline>,
        opts: BrushOptions<'_>,
    ) -> BrushHandle {
        let BrushOptions {
            label,
            granular,
            preserve_mode,
        } = opts;

        let buffer = self.buffers.create(DynamicBufferOpts {
            label,
            maintain_cpu_copy: granular || pipeline.instance_stride < wgpu::COPY_BUFFER_ALIGNMENT,
            usages: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
        });

        BrushHandle(self.brushes.insert(Brush {
            preserve_mode,
            pipeline,
            buffer,
            bind_groups: FxHashMap::default(),
            instances: granular.then_some(Vec::new()),
            instance_count: 0,
        }))
    }

    pub fn set_brush_binding(&mut self, brush: BrushHandle, idx: u32, group: wgpu::BindGroup) {
        self.brushes[brush.0].bind_groups.insert(idx, group);
    }

    pub fn destroy_brush(&mut self, brush: BrushHandle) {
        self.clear_brush(brush);

        let brush = self.brushes.remove(brush.0).unwrap();
        self.buffers.destroy(brush.buffer);
    }

    pub fn clear_brush(&mut self, brush: BrushHandle) {
        let state = &mut self.brushes[brush.0];

        self.buffers.clear(state.buffer);
        state.instance_count = 0;

        if let Some(instance_vec) = &mut state.instances {
            for instance in instance_vec.drain(..) {
                self.instances.remove(instance.0).unwrap();
            }
        }
    }

    pub fn brush_len(&self, brush: BrushHandle) -> u32 {
        self.brushes[brush.0].instance_count
    }

    pub fn push_instance(
        &mut self,
        brush: BrushHandle,
        data: &impl StreamWrite,
    ) -> Option<InstanceHandle> {
        let state = &mut self.brushes[brush.0];

        // Push data
        let start = self.buffers.len(state.buffer);
        let written = self.buffers.extend(state.buffer, data);

        assert_eq!(written, state.pipeline.instance_stride);

        // Define instance
        state.instance_count += 1;

        state.instances.as_mut().map(|instances| {
            let handle = InstanceHandle(self.instances.insert(Instance {
                brush,
                offset: start,
            }));

            instances.push(handle);
            handle
        })
    }

    pub fn modify_instance(&mut self, instance: InstanceHandle, data: &impl StreamWrite) {
        let Instance { brush, offset } = self.instances[instance.0];

        let state = &self.brushes[brush.0];
        let written = self.buffers.write(state.buffer, offset, data);
        assert_eq!(written, state.pipeline.instance_stride);
    }

    pub fn remove_instance(&mut self, instance: InstanceHandle) {
        // Remove instance handle
        let Instance { brush, offset } = self.instances.remove(instance.0).unwrap();

        let state = &mut self.brushes[brush.0];

        // Swap-remove from instance vector
        let instance_vec = state.instances.as_mut().unwrap();

        let index = (offset / state.pipeline.instance_stride) as usize;
        instance_vec.swap_remove(index);

        if let Some(&moved) = instance_vec.get(index) {
            self.instances[moved.0].offset = offset;
        }

        // Swap-remove from buffer
        self.buffers
            .swap_remove_using_local(state.buffer, offset, state.pipeline.instance_stride);

        // Modify instance count for rendering
        state.instance_count -= 1;
    }

    // === Pass management === //

    pub fn create_pass(&mut self, opts: PassOptions<'_>) -> PassHandle {
        let PassOptions {
            label,
            preserve_mode,
        } = opts;

        PassHandle(self.passes.insert(Pass {
            label: label.map(|v| v.to_string()),
            preserve_mode,
            commands: Vec::new(),
        }))
    }

    pub fn clear_pass(&mut self, pass: PassHandle) {
        let state = &mut self.passes[pass.0];

        state.commands.clear();
    }

    pub fn destroy_pass(&mut self, pass: PassHandle) {
        self.clear_pass(pass);
        self.passes.remove(pass.0).unwrap();
    }

    pub fn pass_draw(&mut self, pass: PassHandle, brush: BrushHandle) {
        self.passes[pass.0].commands.push(PassCommand::Draw(brush));
    }

    pub fn tick_lifetimes(&mut self) {
        let affected_brushes = self
            .brushes
            .iter()
            .filter(|(_idx, state)| state.preserve_mode != PreserveMode::Preserved)
            .map(|(k, _v)| BrushHandle(k))
            .collect::<Vec<_>>();

        for brush in affected_brushes {
            match self.brushes[brush.0].preserve_mode {
                PreserveMode::Ephemeral => {
                    self.destroy_brush(brush);
                }
                PreserveMode::Cleared => {
                    self.clear_brush(brush);
                }
                PreserveMode::Preserved => unreachable!(),
            }
        }

        let affected_passes = self
            .passes
            .iter()
            .filter(|(_idx, state)| state.preserve_mode != PreserveMode::Preserved)
            .map(|(k, _v)| PassHandle(k))
            .collect::<Vec<_>>();

        for pass in affected_passes {
            match self.passes[pass.0].preserve_mode {
                PreserveMode::Ephemeral => {
                    self.destroy_pass(pass);
                }
                PreserveMode::Cleared => {
                    self.clear_pass(pass);
                }
                PreserveMode::Preserved => unreachable!(),
            }
        }
    }
}

// === Renderer === //

#[derive(Debug, Clone)]
pub struct RendererOpts<'a> {
    pub label: Option<&'a str>,
    pub view_format: wgpu::TextureFormat,
    pub depth_format: wgpu::TextureFormat,
}

#[derive(Debug)]
pub struct Renderer {}

impl Renderer {
    pub fn new(gfx: &GfxContext, opts: RendererOpts<'_>) -> Self {
        todo!()
    }

    pub fn encode(
        &mut self,
        cx: &Context,
        pass: PassHandle,
        encoder: &mut wgpu::CommandEncoder,
        surface_view: &wgpu::TextureView,
    ) {
        let state = &cx.passes[pass.0];

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: state.label.as_deref(),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: surface_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        for cmd in &state.commands {
            match cmd {
                PassCommand::Draw(brush) => {
                    let state = &cx.brushes[brush.0];

                    // TODO
                }
            }
        }
    }
}

#[derive(Debug, Hash, Clone)]
struct PipelineCreationKey<'a> {
    pipeline: &'a Asset<Pipeline>,
    view_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
}

impl AssetKey for PipelineCreationKey<'_> {
    type Owned = (Asset<Pipeline>, wgpu::TextureFormat, wgpu::TextureFormat);

    fn delegated(&self) -> impl AssetKey<Owned = Self::Owned> + '_ {
        (
            RefKey(self.pipeline),
            RefKey(&self.view_format),
            RefKey(&self.depth_format),
        )
    }
}

impl PipelineCreationKey<'_> {
    fn load<E>(
        self,
        assets: &mut impl AssetLoader<Error = E>,
        gfx: &GfxContext,
    ) -> Result<Asset<wgpu::RenderPipeline>, E> {
        assets.load(gfx, self, |assets, gfx, key| {
            gfx.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: key.pipeline.label.as_deref(),
                    layout: Some(
                        &load_pipeline_layout(assets, gfx, &key.pipeline.bind_group_layouts)
                            .unwrap(),
                    ),
                    vertex: wgpu::VertexState {
                        module: &key.pipeline.vertex_module,
                        entry_point: key.pipeline.vertex_entry_name.as_deref(),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: key.pipeline.instance_stride,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &key.pipeline.instance_attributes,
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
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &key.pipeline.fragment_module,
                        entry_point: key.pipeline.fragment_entry_name.as_deref(),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: key.view_format,
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

fn load_pipeline_layout<E>(
    assets: &mut impl AssetLoader<Error = E>,
    gfx: &GfxContext,
    layouts: &[Asset<wgpu::BindGroupLayout>],
) -> Result<Asset<wgpu::PipelineLayout>, E> {
    assets.load(gfx, RefKey(layouts), |_assets, gfx, RefKey(layouts)| {
        gfx.device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: layouts.iter().map(|v| &**v).collect::<Vec<_>>().as_slice(),
                push_constant_ranges: &[],
            })
    })
}
