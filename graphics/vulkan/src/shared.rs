use std::{
    collections::HashMap,
    hash::Hash,
    sync::{
        Arc,
        RwLock,
    },
};

use ash::{
    vk,
    vk::Handle,
};
use smallvec::SmallVec;
use sourcerenderer_core::gpu;

use super::*;

pub struct VkShared {
    device: Arc<RawVkDevice>,
    descriptor_set_layouts: RwLock<HashMap<VkDescriptorSetLayoutKey, Arc<VkDescriptorSetLayout>>>,
    pipeline_layouts: RwLock<HashMap<VkPipelineLayoutKey, Arc<VkPipelineLayout>>>,
    bindless_texture_descriptor_set: Option<VkBindlessDescriptorSet>,
    clear_buffer_meta_pipeline: VkPipeline,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Default)]
pub(super) struct VkDescriptorSetLayoutKey {
    pub(super) bindings: SmallVec<[VkDescriptorSetEntryInfo; PER_SET_BINDINGS]>,
    pub(super) flags: vk::DescriptorSetLayoutCreateFlags,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub(super) struct VkPipelineLayoutKey {
    pub(super) descriptor_set_layouts:
        [VkDescriptorSetLayoutKey; gpu::TOTAL_SET_COUNT as usize],
    pub(super) push_constant_ranges: [Option<VkConstantRange>; 3],
}

impl VkShared {
    pub fn new(device: &Arc<RawVkDevice>) -> Self {
        let mut descriptor_set_layouts =
            HashMap::<VkDescriptorSetLayoutKey, Arc<VkDescriptorSetLayout>>::new();

        let bindless_texture_descriptor_set =
        if device.features.contains(VkFeatures::DESCRIPTOR_INDEXING) {
            let bindless_set = VkBindlessDescriptorSet::new(device);
            let (layout_key, descriptor_layout) = bindless_set.layout();
            descriptor_set_layouts.insert(layout_key.clone(), descriptor_layout.clone());
            Some(bindless_set)
        } else {
            None
        };

        let shader_bytes = include_bytes!("../meta_shaders/clear_buffer.comp.json");
        let packed: gpu::PackedShader = serde_json::from_slice(shader_bytes).unwrap();
        let shader = VkShader::new(
            device,
            packed,
            Some("ClearBufferMeta"),
        );
        let clear_buffer_meta_pipeline =
            VkPipeline::new_compute_meta(device, &shader, Some("ClearBufferPipeline"));

        Self {
            device: device.clone(),
            descriptor_set_layouts: RwLock::new(descriptor_set_layouts),
            pipeline_layouts: RwLock::new(HashMap::new()),
            bindless_texture_descriptor_set,
            clear_buffer_meta_pipeline,
        }
    }

    #[inline]
    pub(super) fn get_clear_buffer_meta_pipeline(&self) -> &VkPipeline {
        &self.clear_buffer_meta_pipeline
    }

    pub(super) fn get_descriptor_set_layout(
        &self,
        layout_key: &VkDescriptorSetLayoutKey,
    ) -> Arc<VkDescriptorSetLayout> {
        {
            let cache = self.descriptor_set_layouts.read().unwrap();
            if let Some(layout) = cache.get(layout_key) {
                return layout.clone();
            }
        }

        let mut largest_index = 0;
        for binding in &layout_key.bindings {
            assert!(binding.index > largest_index);
            largest_index = binding.index;
        }

        let layout = Arc::new(VkDescriptorSetLayout::new(
            &layout_key.bindings,
            layout_key.flags,
            &self.device,
        ));
        let mut cache = self.descriptor_set_layouts.write().unwrap();
        cache.insert(layout_key.clone(), layout.clone());
        layout
    }

    #[inline]
    pub(super) fn get_pipeline_layout(
        &self,
        layout_key: &VkPipelineLayoutKey,
    ) -> Arc<VkPipelineLayout> {
        {
            let cache = self.pipeline_layouts.read().unwrap();
            if let Some(layout) = cache.get(layout_key) {
                return layout.clone();
            }
        }

        let mut descriptor_sets: [Option<Arc<VkDescriptorSetLayout>>; gpu::TOTAL_SET_COUNT] = Default::default();
        for i in 0..layout_key.descriptor_set_layouts.len() {
            let set_key = &layout_key.descriptor_set_layouts[i];
            descriptor_sets[i] = Some(self.get_descriptor_set_layout(set_key));
        }

        let pipeline_layout = Arc::new(VkPipelineLayout::new(
            &descriptor_sets,
            &layout_key.push_constant_ranges,
            &self.device,
        ));
        let mut cache = self.pipeline_layouts.write().unwrap();
        cache.insert(layout_key.clone(), pipeline_layout.clone());
        pipeline_layout
    }

    #[inline]
    pub(super) fn bindless_texture_descriptor_set(&self) -> Option<&VkBindlessDescriptorSet> {
        self.bindless_texture_descriptor_set.as_ref()
    }
}
