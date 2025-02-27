use std::{collections::HashMap, sync::{Arc, RwLock}};

use smallvec::SmallVec;
use web_sys::GpuDevice;

use crate::binding::{WebGPUBindGroupEntryInfo, WebGPUBindGroupLayout, WebGPUPipelineLayout};

use sourcerenderer_core::gpu;

pub type WebGPUBindGroupLayoutKey = SmallVec<[WebGPUBindGroupEntryInfo; gpu::PER_SET_BINDINGS as usize]>;
pub type WebGPUPipelineLayoutKey = [WebGPUBindGroupLayoutKey; gpu::NON_BINDLESS_SET_COUNT as usize];

pub struct WebGPUShared {
    device: GpuDevice,
    bind_group_layouts: RwLock<HashMap<WebGPUBindGroupLayoutKey, Arc<WebGPUBindGroupLayout>>>,
    pipeline_layouts: RwLock<HashMap<WebGPUPipelineLayoutKey, Arc<WebGPUPipelineLayout>>>,
}

impl WebGPUShared {
    pub(crate) fn new(device: &GpuDevice) -> Self {
        Self {
            device: device.clone(),
            bind_group_layouts: RwLock::new(HashMap::new()),
            pipeline_layouts: RwLock::new(HashMap::new())
        }
    }

    #[inline]
    pub(crate) fn get_bind_group_layout(&self, layout_key: &WebGPUBindGroupLayoutKey) -> Arc<WebGPUBindGroupLayout> {
        {
            let cache = self.bind_group_layouts.read().unwrap();
            if let Some(layout) = cache.get(layout_key) {
                return layout.clone();
            }
        }

        let mut largest_index = 0;
        for binding in layout_key {
            assert!(binding.index > largest_index || largest_index == 0);
            largest_index = binding.index;
        }

        let bind_group_layout = Arc::new(WebGPUBindGroupLayout::new(layout_key, &self.device).unwrap());

        let mut cache: std::sync::RwLockWriteGuard<'_, HashMap<SmallVec<[WebGPUBindGroupEntryInfo; 32]>, Arc<WebGPUBindGroupLayout>>> = self.bind_group_layouts.write().unwrap();
        cache.insert(layout_key.clone(), bind_group_layout.clone());
        bind_group_layout
    }

    #[inline]
    pub(super) fn get_pipeline_layout(
        &self,
        layout_key: &WebGPUPipelineLayoutKey,
    ) -> Arc<WebGPUPipelineLayout> {
        {
            let cache = self.pipeline_layouts.read().unwrap();
            if let Some(layout) = cache.get(layout_key) {
                return layout.clone();
            }
        }

        assert!(layout_key.len() <= gpu::NON_BINDLESS_SET_COUNT as usize);
        let mut bind_group_layouts: [Option<Arc<WebGPUBindGroupLayout>>; gpu::NON_BINDLESS_SET_COUNT as usize] = Default::default();
        for i in 0..layout_key.len() {
            let set_key = &layout_key[i];
            bind_group_layouts[i] = Some(self.get_bind_group_layout(set_key));
        }

        let pipeline_layout = Arc::new(WebGPUPipelineLayout::new(
            &self.device,
            &bind_group_layouts
        ));
        let mut cache = self.pipeline_layouts.write().unwrap();
        cache.insert(layout_key.clone(), pipeline_layout.clone());
        pipeline_layout
    }
}
