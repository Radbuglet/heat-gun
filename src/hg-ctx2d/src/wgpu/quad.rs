use std::{cmp, collections::HashMap};

use thunderdome::{Arena, Index};
use wgpu::util::DeviceExt;

use super::{
    context::StreamWritable,
    utils::{DepthEpoch, DepthGenerator},
};

// === Core === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct QuadShaderHandle(pub Index);

impl QuadShaderHandle {
    pub const DANGLING: Self = Self(Index::DANGLING);
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct QuadBrushHandle(pub Index);

impl QuadBrushHandle {
    pub const DANGLING: Self = Self(Index::DANGLING);
}

#[derive(Debug)]
pub struct QuadRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    depth: DepthGenerator,
    shaders: Arena<Shader>,
    brushes: Arena<Brush>,
}

#[derive(Debug)]
struct Shader {
    // Pipeline
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,

    // Buffers
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,

    // State
    uniform_stride: usize,
    instance_stride: usize,

    uniform_data: Vec<u8>,
    brushes: Vec<QuadBrushHandle>,
}

#[derive(Debug)]
struct Brush {
    shader: QuadShaderHandle,
    instance_buffer: Option<wgpu::Buffer>,
    uniform_offset: usize,

    instance_data: Vec<u8>,
    instance_count: u32,
    epoch_starts: Vec<(DepthEpoch, u32)>,
    last_depth_epoch: DepthEpoch,
}

impl QuadRenderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            device,
            queue,
            format,
            depth: DepthGenerator::new(),
            shaders: Arena::new(),
            brushes: Arena::new(),
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
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
                entry_point: vs_main.as_deref(),
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
                entry_point: fs_main.as_deref(),
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

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{debug_name} uniform buffer")),
            size: (uniform_stride * 8) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("{debug_name} bind group")),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_buffer,
                    offset: 0,
                    size: None,
                }),
            }],
        });

        let handle = QuadShaderHandle(self.shaders.insert(Shader {
            pipeline,
            bind_group_layout,
            bind_group,
            uniform_buffer,
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
            instance_buffer: None,
            uniform_offset,
            instance_data: Vec::new(),
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
            assert!(brush.instance_data.is_empty());
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

    pub fn prepare(&mut self) {
        for (_, shader) in &mut self.shaders {
            self.queue
                .write_buffer(&shader.uniform_buffer, 0, &shader.uniform_data);

            for &brush in &shader.brushes {
                let brush = &mut self.brushes[brush.0];

                brush.instance_buffer = Some(self.device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: None,
                        contents: &brush.instance_data,
                        usage: wgpu::BufferUsages::VERTEX,
                    },
                ));
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

                for &brush_handle in &shader.brushes {
                    let brush = &self.brushes[brush_handle.0];

                    // Figure out the run of vertices this brush contributes to this epoch
                    let instance_range = {
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

                    // Draw it!
                    pass.set_bind_group(0, &shader.bind_group, &[brush.uniform_offset as u32]);
                    pass.set_vertex_buffer(0, brush.instance_buffer.as_ref().unwrap().slice(..));
                    pass.draw(0..6, instance_range);
                }
            }
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct QuadShaderOpts<'a> {
    pub debug_name: &'a str,
    pub module: &'a wgpu::ShaderModule,
    pub vs_main: Option<&'a str>,
    pub fs_main: Option<&'a str>,
    pub constants: &'a HashMap<String, f64>,
    pub instance_attributes: &'a [wgpu::VertexAttribute],
    pub instance_stride: usize,
    pub uniform_stride: usize,
}

// === Solid Shader === //

pub fn create_solid_quad_shader(quad: &mut QuadRenderer) -> QuadShaderHandle {
    todo!()
}

// === Gradient Shader === //

// TODO

// === Texture Shader === //

// TODO

// === Circle Shader === //

// TODO
