use super::{
    assets::{Asset, AssetKey, AssetLoader, ListKey, OptionKey, RefKey},
    gfx_bundle::GfxContext,
    transform::TransformUniformData,
};

pub fn load_pipeline_layout<E>(
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

#[derive(Debug)]
pub struct SimplePipelineDescriptor<'a> {
    pub label: Option<&'a str>,
    pub vertex_module: &'a Asset<wgpu::ShaderModule>,
    pub fragment_module: &'a Asset<wgpu::ShaderModule>,
    pub vertex_entry: Option<&'a str>,
    pub fragment_entry: Option<&'a str>,
    pub instance_stride: wgpu::BufferAddress,
    pub instance_attributes: &'a [wgpu::VertexAttribute],
    pub color_format: wgpu::TextureFormat,
    pub depth_format: wgpu::TextureFormat,
}

impl AssetKey for SimplePipelineDescriptor<'_> {
    type Owned = (
        Option<String>,
        Asset<wgpu::ShaderModule>,
        Asset<wgpu::ShaderModule>,
        Option<String>,
        Option<String>,
        wgpu::BufferAddress,
        Vec<wgpu::VertexAttribute>,
        wgpu::TextureFormat,
        wgpu::TextureFormat,
    );

    fn delegated(&self) -> impl AssetKey<Owned = Self::Owned> + '_ {
        (
            OptionKey(self.label),
            RefKey(self.vertex_module),
            RefKey(self.fragment_module),
            OptionKey(self.vertex_entry),
            OptionKey(self.fragment_entry),
            RefKey(&self.instance_stride),
            RefKey(self.instance_attributes),
            RefKey(&self.color_format),
            RefKey(&self.depth_format),
        )
    }
}

impl SimplePipelineDescriptor<'_> {
    pub fn load<E>(
        &self,
        assets: &mut impl AssetLoader<Error = E>,
        gfx: &GfxContext,
    ) -> Result<Asset<wgpu::RenderPipeline>, E> {
        assets.load(gfx, self, |assets, gfx, me| {
            let layout = TransformUniformData::group_layout(assets, gfx).unwrap();
            let layout = load_pipeline_layout(assets, gfx, &[&layout]);

            gfx.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: me.label,
                    layout: Some(&*layout.unwrap()),
                    vertex: wgpu::VertexState {
                        module: &me.vertex_module,
                        entry_point: me.vertex_entry,
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: me.instance_stride,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: me.instance_attributes,
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
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &me.fragment_module,
                        entry_point: me.fragment_entry,
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
