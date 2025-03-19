use std::{borrow::Cow, cmp, fmt, marker::PhantomData, mem::offset_of, u32};

use crevice::std430::AsStd430;
use derive_where::derive_where;
use glam::{Mat2, Vec2, Vec3, Vec4};
use hg_utils::hash::FxHashMap;
use thunderdome::{Arena, Index};

use super::{
    assets::{Asset, AssetManager, CloneKey, ListKey},
    utils::{Crevice, DepthEpoch, DepthGenerator, StreamWritable, StreamWritesSized},
};

// === Core === //

#[derive_where(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct ShaderHandle<U, I> {
    pub _ty: PhantomData<fn(U, I) -> (U, I)>,
    pub raw: Index,
}

#[derive_where(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct BrushHandle<I> {
    pub _ty: PhantomData<fn(I) -> I>,
    pub raw: Index,
}

#[derive(Debug)]
pub struct InstanceRenderer {
    device: wgpu::Device,
    assets: AssetManager,
    uniform_bind_layout: Asset<wgpu::BindGroupLayout>,

    // State
    depth: DepthGenerator,
    shaders: Arena<Shader>,
    brushes: Arena<Brush>,
}

#[derive(Debug)]
struct Shader {
    debug_name: String,

    // Pipeline
    pipeline: wgpu::RenderPipeline,

    // Buffers
    uniform_bind: Option<wgpu::BindGroup>,
    uniform_buffer: Option<wgpu::Buffer>,
    instance_buffer: Option<wgpu::Buffer>,

    // Config
    uniform_stride: usize,
    instance_stride: usize,

    // State
    uniform_data: Vec<u8>,
    bind_groups: FxHashMap<u32, wgpu::BindGroup>,
    brushes: Vec<Index>,
}

#[derive(Debug)]
struct Brush {
    shader: Index,
    uniform_offset: usize,
    instance_data: Vec<u8>,
    shader_instance_buf_offset: u32,
    instance_count: u32,
    epoch_starts: Vec<(DepthEpoch, u32)>,
    bind_groups: FxHashMap<u32, wgpu::BindGroup>,
    last_depth_epoch: DepthEpoch,
}

impl InstanceRenderer {
    pub fn new(device: wgpu::Device, assets: AssetManager) -> Self {
        let uniform_bind_layout = load_uniform_buffer_bind_layout(&assets, &device);

        Self {
            device,
            assets,
            uniform_bind_layout,
            depth: DepthGenerator::new(),
            shaders: Arena::new(),
            brushes: Arena::new(),
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn create_shader<U, I>(
        &mut self,
        debug_name: impl fmt::Display,
        pipeline: wgpu::RenderPipeline,
        uniform: U,
        instance: I,
    ) -> ShaderHandle<U, I>
    where
        U: StreamWritesSized,
        I: StreamWritesSized,
    {
        let raw = self.shaders.insert(Shader {
            debug_name: debug_name.to_string(),
            pipeline,
            uniform_buffer: None,
            instance_buffer: None,
            uniform_bind: None,
            uniform_stride: uniform.size(),
            instance_stride: instance.size(),
            uniform_data: Vec::new(),
            brushes: Vec::new(),
            bind_groups: FxHashMap::default(),
        });

        ShaderHandle {
            _ty: PhantomData,
            raw,
        }
    }

    pub fn destroy_shader<U, I>(&mut self, handle: ShaderHandle<U, I>) {
        self.shaders.remove(handle.raw).unwrap();
    }

    pub fn start_brush<U, I>(
        &mut self,
        shader: ShaderHandle<U, I>,
        uniform_data: &impl StreamWritable<U>,
    ) -> BrushHandle<I> {
        let shader_handle = shader;
        let shader = &mut self.shaders[shader_handle.raw];

        // Write uniform data
        let uniform_offset = shader.uniform_data.len();
        uniform_data.write_to(&mut shader.uniform_data);
        assert_eq!(
            shader.uniform_data.len() - uniform_offset,
            shader.uniform_stride
        );

        // Register the brush
        let raw = self.brushes.insert(Brush {
            shader: shader_handle.raw,
            uniform_offset,
            instance_data: Vec::new(),
            shader_instance_buf_offset: 0,
            instance_count: 0,
            epoch_starts: Vec::new(),
            last_depth_epoch: self.depth.epoch,
            bind_groups: FxHashMap::default(),
        });

        shader.brushes.push(raw);

        BrushHandle {
            _ty: PhantomData,
            raw,
        }
    }

    pub fn next_depth(&self) -> f32 {
        self.depth.curr().value
    }

    pub fn push_instance<I>(&mut self, brush: BrushHandle<I>, instance: &impl StreamWritable<I>) {
        let brush = &mut self.brushes[brush.raw];

        // Start a new epoch run if necessary
        let curr_epoch = self.depth.curr().epoch;

        if brush.epoch_starts.is_empty() {
            debug_assert!(brush.instance_data.is_empty());
            brush.epoch_starts.push((curr_epoch, 0));
            brush.last_depth_epoch = curr_epoch;
        }

        if curr_epoch != brush.last_depth_epoch {
            brush.epoch_starts.push((curr_epoch, brush.instance_count));
            brush.last_depth_epoch = curr_epoch;
        }

        // Write the instance data
        let old_len = brush.instance_data.len();
        instance.write_to(&mut brush.instance_data);
        assert_eq!(
            brush.instance_data.len() - old_len,
            self.shaders[brush.shader].instance_stride
        );

        brush.instance_count += 1;

        // Advance to the next depth level
        self.depth.next();
    }

    pub fn set_shader_bind_group<U, I>(
        &mut self,
        shader: ShaderHandle<U, I>,
        idx: u32,
        group: wgpu::BindGroup,
    ) {
        self.shaders[shader.raw].bind_groups.insert(idx, group);
    }

    pub fn set_brush_bind_group<I>(
        &mut self,
        brush: BrushHandle<I>,
        idx: u32,
        group: wgpu::BindGroup,
    ) {
        self.brushes[brush.raw].bind_groups.insert(idx, group);
    }

    pub fn reset(&mut self) {
        self.depth.reset();

        self.brushes.clear();

        for (_, shader) in &mut self.shaders {
            shader.uniform_data.clear();
            shader.brushes.clear();
        }
    }

    pub fn prepare(&mut self, queue: &wgpu::Queue) {
        for (_, shader) in &mut self.shaders {
            // Re-create the uniform buffer if necessary
            let min_uniform_size = shader.uniform_data.len() as wgpu::BufferAddress;

            if shader
                .uniform_buffer
                .as_ref()
                .is_none_or(|v| v.size() < min_uniform_size)
            {
                let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("{} uniform buffer", shader.debug_name)),
                    size: min_uniform_size,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });

                let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&format!("{} bind group", shader.debug_name)),
                    layout: &self.uniform_bind_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &uniform_buffer,
                            offset: 0,
                            size: None,
                        }),
                    }],
                });

                shader.uniform_buffer = Some(uniform_buffer);
                shader.uniform_bind = Some(bind_group);
            }

            // Write the uniform buffer
            queue.write_buffer(
                shader.uniform_buffer.as_ref().unwrap(),
                0,
                &shader.uniform_data,
            );

            // Determine the required size of the instance buffer
            let min_instance_size = shader
                .brushes
                .iter()
                .map(|&brush| self.brushes[brush].instance_data.len() as wgpu::BufferAddress)
                .sum();

            // Re-create the instance buffer if necessary
            if shader
                .instance_buffer
                .as_ref()
                .is_none_or(|v| v.size() < min_instance_size)
            {
                let instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("{} instance buffer", shader.debug_name)),
                    size: min_instance_size,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });

                shader.instance_buffer = Some(instance_buffer);
            }

            // Write the instance buffer
            let mut byte_offset_accum = 0;
            let mut instance_offset_accum = 0;

            for &brush in &shader.brushes {
                let brush = &mut self.brushes[brush];

                queue.write_buffer(
                    shader.instance_buffer.as_ref().unwrap(),
                    byte_offset_accum,
                    &brush.instance_data,
                );
                byte_offset_accum += brush.instance_data.len() as wgpu::BufferAddress;

                brush.shader_instance_buf_offset = instance_offset_accum;
                instance_offset_accum += brush.instance_count;
            }
        }
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        let mut brush_to_next_epoch_idx =
            (0..self.brushes.len()).map(|_| 0usize).collect::<Vec<_>>();

        for draw_epoch in 0..=self.depth.epoch.0 {
            let draw_epoch = DepthEpoch(draw_epoch);

            for (_, shader) in &self.shaders {
                pass.set_pipeline(&shader.pipeline);
                pass.set_vertex_buffer(0, shader.instance_buffer.as_ref().unwrap().slice(..));

                for &brush_handle in &shader.brushes {
                    let brush = &self.brushes[brush_handle];

                    // Figure out the run of vertices this brush contributes to this epoch
                    let instance_range_rel = {
                        let epoch_data_idx =
                            &mut brush_to_next_epoch_idx[brush_handle.slot() as usize];

                        let (next_epoch_id, epoch_start) = brush.epoch_starts[*epoch_data_idx];

                        match next_epoch_id.cmp(&draw_epoch) {
                            cmp::Ordering::Less => {
                                // This brush does not render to this epoch.
                                continue;
                            }
                            cmp::Ordering::Equal => {
                                // This brush renders this epoch—fallthrough!
                            }
                            cmp::Ordering::Greater => unreachable!(),
                        }

                        let epoch_end = brush
                            .epoch_starts
                            .get(*epoch_data_idx + 1)
                            .map(|&(_, start_idx)| start_idx)
                            .unwrap_or(brush.instance_count);

                        *epoch_data_idx += 1;

                        epoch_start..epoch_end
                    };

                    let instance_range_abs = {
                        let start = instance_range_rel.start + brush.shader_instance_buf_offset;
                        let end = instance_range_rel.end + brush.shader_instance_buf_offset;
                        start..end
                    };

                    // Draw it!
                    pass.set_bind_group(0, &shader.uniform_bind, &[brush.uniform_offset as u32]);

                    for (&idx, group) in &shader.bind_groups {
                        pass.set_bind_group(idx, group, &[]);
                    }

                    for (&idx, group) in &brush.bind_groups {
                        pass.set_bind_group(idx, group, &[]);
                    }

                    pass.draw(0..6, instance_range_abs);
                }
            }
        }
    }
}

pub fn load_uniform_buffer_bind_layout(
    assets: &AssetManager,
    device: &wgpu::Device,
) -> Asset<wgpu::BindGroupLayout> {
    assets.load(device, (), |_assets, device, ()| {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniform buffer bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::all(),
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    })
}

pub fn load_pipeline_layout(
    assets: &AssetManager,
    device: &wgpu::Device,
    layouts: &[&wgpu::BindGroupLayout],
) -> Asset<wgpu::PipelineLayout> {
    assets.load(
        device,
        ListKey(layouts),
        |_assets, device, ListKey(layouts)| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: layouts,
                push_constant_ranges: &[],
            })
        },
    )
}

// === Solid Shader === //

const SOLID_SHADER_STR: &str = include_str!("shaders/solid.wgsl");

#[derive(Debug, Copy, Clone, AsStd430)]
pub struct SolidQuadUniforms {
    pub affine_mat: Mat2,
    pub affine_trans: Vec2,
}

#[derive(Debug, Copy, Clone, AsStd430)]
pub struct SolidQuadInstance {
    pub pos: Vec3,
    pub size: Vec2,
    pub color: Vec4,
}

pub type SolidQuadShader = ShaderHandle<Crevice<SolidQuadUniforms>, Crevice<SolidQuadInstance>>;
pub type SolidQuadBrush = BrushHandle<Crevice<SolidQuadInstance>>;

pub fn load_solid_quad_pipeline(
    assets: &AssetManager,
    device: &wgpu::Device,
    texture_format: wgpu::TextureFormat,
) -> Asset<wgpu::RenderPipeline> {
    assets.load(
        device,
        CloneKey(texture_format),
        |assets, device, CloneKey(texture_format)| {
            let module = assets.load(device, (), |_assets, device, ()| {
                device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("solid quad shader module"),
                    source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SOLID_SHADER_STR)),
                })
            });

            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("solid quad pipeline"),
                layout: Some(&load_pipeline_layout(
                    assets,
                    device,
                    &[&load_uniform_buffer_bind_layout(assets, device)],
                )),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: SolidQuadInstance::std430_size_static() as _,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x3,
                                offset: offset_of!(<SolidQuadInstance as AsStd430>::Output, pos)
                                    as u64,
                                shader_location: 0,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: offset_of!(<SolidQuadInstance as AsStd430>::Output, size)
                                    as u64,
                                shader_location: 1,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: offset_of!(<SolidQuadInstance as AsStd430>::Output, color)
                                    as u64,
                                shader_location: 2,
                            },
                        ],
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
                    module: &module,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: texture_format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::all(),
                    })],
                }),
                multiview: None,
                cache: None,
            })
        },
    )
}

// === Gradient Shader === //

// TODO

// === Texture Shader === //

// TODO

// === Circle Shader === //

// TODO
