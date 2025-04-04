use std::{ops::Deref, sync::Arc};

#[derive(Debug, Clone)]
pub struct GfxContext(Arc<GfxContextInner>);

#[derive(Debug)]
pub struct GfxContextInner {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub features: wgpu::Features,
    pub limits: wgpu::Limits,
}

impl GfxContext {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let features = device.features();
        let limits = device.limits();

        Self(Arc::new(GfxContextInner {
            device,
            queue,
            features,
            limits,
        }))
    }
}

impl Deref for GfxContext {
    type Target = GfxContextInner;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}
