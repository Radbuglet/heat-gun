use std::{cmp, collections::HashMap, mem::offset_of, u32};

use crevice::std430::AsStd430;
use glam::{Mat2, Vec2, Vec3, Vec4};
use thunderdome::{Arena, Index};

use super::{
    context::StreamWritable,
    utils::{DepthEpoch, DepthGenerator},
};

// === Core === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct QuadShaderHandle(pub Index);

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct QuadBrushHandle(pub Index);

#[derive(Debug)]
pub struct QuadRenderer {
    device: wgpu::Device,
    format: wgpu::TextureFormat,
    depth: DepthGenerator,
    shaders: Arena<Shader>,
    brushes: Arena<Brush>,
}

#[derive(Debug)]
struct Shader {
    debug_name: String,

    // Pipeline
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,

    // Buffers
    bind_group: Option<wgpu::BindGroup>,
    uniform_buffer: Option<wgpu::Buffer>,
    instance_buffer: Option<wgpu::Buffer>,

    // State
    uniform_stride: usize,
    instance_stride: usize,

    uniform_data: Vec<u8>,
    brushes: Vec<QuadBrushHandle>,
}

#[derive(Debug)]
struct Brush {
    shader: QuadShaderHandle,
    uniform_offset: usize,
    instance_data: Vec<u8>,
    shader_instance_buf_offset: u32,
    instance_count: u32,
    epoch_starts: Vec<(DepthEpoch, u32)>,
    last_depth_epoch: DepthEpoch,
}

impl QuadRenderer {
    pub fn new(device: wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self {
            device,
            format,
            depth: DepthGenerator::new(),
            shaders: Arena::new(),
            brushes: Arena::new(),
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn create_shader(&mut self, opts: QuadShaderOpts<'_>) -> QuadShaderHandle {
        let Self {
            ref device, format, ..
        } = *self;

        let QuadShaderOpts {
            debug_name,
            module,
            vs_main,
            fs_main,
            constants,
            instance_attributes,
            instance_stride,
            uniform_stride,
        } = opts;

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&format!("{debug_name} bind group layout")),
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
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("{debug_name} pipeline layout")),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("{debug_name} pipeline")),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some(vs_main),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &constants,
                    zero_initialize_workgroup_memory: true,
                },
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: instance_stride as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: instance_attributes,
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
                entry_point: Some(fs_main),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &constants,
                    zero_initialize_workgroup_memory: true,
                },
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::all(),
                })],
            }),
            multiview: None,
            cache: None,
        });

        let handle = QuadShaderHandle(self.shaders.insert(Shader {
            debug_name: opts.debug_name.to_string(),
            pipeline,
            bind_group_layout,
            uniform_buffer: None,
            instance_buffer: None,
            bind_group: None,
            uniform_stride,
            instance_stride,
            uniform_data: Vec::new(),
            brushes: Vec::new(),
        }));

        handle
    }

    pub fn destroy_shader(&mut self, handle: QuadShaderHandle) {
        self.shaders.remove(handle.0).unwrap();
    }

    pub fn start_brush(
        &mut self,
        shader: QuadShaderHandle,
        uniform_data: &(impl ?Sized + StreamWritable),
    ) -> QuadBrushHandle {
        let shader_handle = shader;
        let shader = &mut self.shaders[shader_handle.0];

        // Write uniform data
        let uniform_offset = shader.uniform_data.len();
        uniform_data.write_to(&mut shader.uniform_data);
        assert_eq!(
            shader.uniform_data.len() - uniform_offset,
            shader.uniform_stride
        );

        // Register the brush
        let handle = QuadBrushHandle(self.brushes.insert(Brush {
            shader: shader_handle,
            uniform_offset,
            instance_data: Vec::new(),
            shader_instance_buf_offset: 0,
            instance_count: 0,
            epoch_starts: Vec::new(),
            last_depth_epoch: self.depth.epoch,
        }));

        shader.brushes.push(handle);

        handle
    }

    pub fn next_depth(&self) -> f32 {
        self.depth.curr().value
    }

    pub fn push_instance(
        &mut self,
        brush: QuadBrushHandle,
        instance: &(impl ?Sized + StreamWritable),
    ) {
        let brush = &mut self.brushes[brush.0];

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
            self.shaders[brush.shader.0].instance_stride
        );

        brush.instance_count += 1;

        // Advance to the next depth level
        self.depth.next();
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
                    layout: &shader.bind_group_layout,
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
                shader.bind_group = Some(bind_group);
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
                .map(|&brush| self.brushes[brush.0].instance_data.len() as wgpu::BufferAddress)
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
                let brush = &mut self.brushes[brush.0];

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
                    let brush = &self.brushes[brush_handle.0];

                    // Figure out the run of vertices this brush contributes to this epoch
                    let instance_range_rel = {
                        let epoch_data_idx =
                            &mut brush_to_next_epoch_idx[brush_handle.0.slot() as usize];

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
                    pass.set_bind_group(0, &shader.bind_group, &[brush.uniform_offset as u32]);
                    pass.draw(0..6, instance_range_abs);
                }
            }
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct QuadShaderOpts<'a> {
    pub debug_name: &'a str,
    pub module: &'a wgpu::ShaderModule,
    pub vs_main: &'a str,
    pub fs_main: &'a str,
    pub constants: &'a HashMap<String, f64>,
    pub instance_attributes: &'a [wgpu::VertexAttribute],
    pub instance_stride: usize,
    pub uniform_stride: usize,
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

pub fn create_solid_quad_shader(quad: &mut QuadRenderer) -> QuadShaderHandle {
    let module = quad
        .device()
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("solid quad shader"),
            source: wgpu::ShaderSource::Wgsl(SOLID_SHADER_STR.into()),
        });

    quad.create_shader(QuadShaderOpts {
        debug_name: "solid quad",
        module: &module,
        vs_main: "vs_main",
        fs_main: "fs_main",
        constants: &HashMap::default(),
        instance_attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: offset_of!(<SolidQuadInstance as AsStd430>::Output, pos) as u64,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: offset_of!(<SolidQuadInstance as AsStd430>::Output, size) as u64,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: offset_of!(<SolidQuadInstance as AsStd430>::Output, color) as u64,
                shader_location: 2,
            },
        ],
        instance_stride: SolidQuadInstance::std430_size_static(),
        uniform_stride: SolidQuadUniforms::std430_size_static(),
    })
}

// === Gradient Shader === //

// TODO

// === Texture Shader === //

// TODO

// === Circle Shader === //

// TODO
