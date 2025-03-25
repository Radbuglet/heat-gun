use crate::{Asset, AssetKey, AssetLoader, ListKey};

// === InstancePipeline === //

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
    pub fn load(&self, assets: &mut impl AssetLoader) -> Asset<Pipeline> {
        assets.load((), self, |_assets, (), key| key.to_owned_key())
    }
}

impl AssetKey for PipelineRef<'_> {
    type Owned = Pipeline;

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

    fn matches(&self, owned: &Self::Owned) -> bool {
        self.label == owned.label.as_deref()
            && self.instance_stride == owned.instance_stride
            && self.instance_attributes == owned.instance_attributes
            && ListKey(self.bind_group_layouts).matches(&owned.bind_group_layouts)
            && self.vertex_module == &owned.vertex_module
            && self.fragment_module == &owned.fragment_module
            && self.vertex_entry_name == owned.vertex_entry_name.as_deref()
            && self.fragment_entry_name == owned.fragment_entry_name.as_deref()
    }
}
