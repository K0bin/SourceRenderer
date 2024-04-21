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
    render_passes: RwLock<HashMap<VkRenderPassInfo, Arc<VkRenderPass>>>,
    frame_buffers: RwLock<HashMap<SmallVec<[u64; 8]>, Arc<VkFrameBuffer>>>,
    bindless_texture_descriptor_set: Option<VkBindlessDescriptorSet>,
    clear_buffer_meta_pipeline: VkPipeline,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Default)]
pub(super) struct VkDescriptorSetLayoutKey {
    pub(super) bindings: SmallVec<[VkDescriptorSetEntryInfo; 16]>,
    pub(super) flags: vk::DescriptorSetLayoutCreateFlags,
}

const BINDLESS_TEXTURE_SET_INDEX: u32 = 3;

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub(super) struct VkPipelineLayoutKey {
    pub(super) descriptor_set_layouts:
        [VkDescriptorSetLayoutKey; (BINDLESS_TEXTURE_SET_INDEX + 1) as usize],
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

        let shader_bytes = include_bytes!("../meta_shaders/clear_buffer.comp.spv");
        let shader = Arc::new(VkShader::new(
            device,
            gpu::ShaderType::ComputeShader,
            &shader_bytes[..],
            Some("ClearBufferMeta"),
        ));
        let clear_buffer_meta_pipeline =
            VkPipeline::new_compute_meta(device, &shader, Some("ClearBufferPipeline"));

        Self {
            device: device.clone(),
            descriptor_set_layouts: RwLock::new(descriptor_set_layouts),
            pipeline_layouts: RwLock::new(HashMap::new()),
            render_passes: RwLock::new(HashMap::new()),
            frame_buffers: RwLock::new(HashMap::new()),
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

        let mut descriptor_sets: [Option<Arc<VkDescriptorSetLayout>>; 5] = Default::default();
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

    pub(super) fn get_render_pass(&self, info: VkRenderPassInfo) -> Arc<VkRenderPass> {
        {
            let cache = self.render_passes.read().unwrap();
            if let Some(renderpass) = cache.get(&info) {
                return renderpass.clone();
            }
        }
        let renderpass = Arc::new(VkRenderPass::new(&self.device, &info));
        let mut cache = self.render_passes.write().unwrap();
        cache.insert(info, renderpass.clone());
        renderpass
    }

    pub(super) fn get_framebuffer(
        &self,
        render_pass: &Arc<VkRenderPass>,
        attachments: &[&VkTextureView],
    ) -> Arc<VkFrameBuffer> {
        let key: SmallVec<[u64; 8]> = attachments
            .iter()
            .map(|a| a.view_handle().as_raw())
            .collect();
        {
            let cache = self.frame_buffers.read().unwrap();
            if let Some(framebuffer) = cache.get(&key) {
                return framebuffer.clone();
            }
        }
        let (width, height) = attachments.iter().fold((0, 0), |old, a| {
            (
                a.texture_info().width.max(old.0),
                a.texture_info().height.max(old.1),
            )
        });
        let frame_buffer = Arc::new(VkFrameBuffer::new(
            &self.device,
            width,
            height,
            render_pass,
            attachments,
        ));
        let mut cache = self.frame_buffers.write().unwrap();
        cache.insert(key, frame_buffer.clone());
        frame_buffer
    }

    #[inline]
    pub(super) fn bindless_texture_descriptor_set(&self) -> Option<&VkBindlessDescriptorSet> {
        self.bindless_texture_descriptor_set.as_ref()
    }
}
