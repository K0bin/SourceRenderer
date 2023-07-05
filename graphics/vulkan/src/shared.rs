use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{
    Arc,
    RwLock,
};

use ash::vk;
use ash::vk::Handle;
use smallvec::SmallVec;
use sourcerenderer_core::graphics::{
    ShaderType,
    Texture,
};

use crate::bindless::{
    VkBindlessDescriptorSet,
    BINDLESS_TEXTURE_SET_INDEX,
};
use crate::buffer::BufferAllocator;
use crate::descriptor::{
    VkConstantRange,
    VkDescriptorSetEntryInfo,
    VkDescriptorSetLayout,
};
use crate::pipeline::{
    VkPipelineLayout,
    VkShader,
};
use crate::raw::{
    RawVkDevice,
    VkFeatures,
};
use crate::renderpass::{
    VkFrameBuffer,
    VkRenderPassInfo,
};
use crate::texture::VkTextureView;
use crate::{
    VkPipeline,
    VkRenderPass,
};

pub struct VkShared {
    device: Arc<RawVkDevice>,
    buffers: BufferAllocator, // consider per thread
    descriptor_set_layouts: RwLock<HashMap<VkDescriptorSetLayoutKey, Arc<VkDescriptorSetLayout>>>,
    pipeline_layouts: RwLock<HashMap<VkPipelineLayoutKey, Arc<VkPipelineLayout>>>,
    render_passes: RwLock<HashMap<VkRenderPassInfo, Arc<VkRenderPass>>>,
    frame_buffers: RwLock<HashMap<SmallVec<[u64; 8]>, Arc<VkFrameBuffer>>>,
    bindless_texture_descriptor_set: Option<Arc<VkBindlessDescriptorSet>>,
    clear_buffer_meta_pipeline: Arc<VkPipeline>,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Default)]
pub(crate) struct VkDescriptorSetLayoutKey {
    pub(crate) bindings: Vec<VkDescriptorSetEntryInfo>,
    pub(crate) flags: vk::DescriptorSetLayoutCreateFlags,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub(crate) struct VkPipelineLayoutKey {
    pub(crate) descriptor_set_layouts:
        [VkDescriptorSetLayoutKey; (BINDLESS_TEXTURE_SET_INDEX + 1) as usize],
    pub(crate) push_constant_ranges: [Option<VkConstantRange>; 3],
}

impl VkShared {
    pub fn new(device: &Arc<RawVkDevice>) -> Self {
        let mut descriptor_set_layouts =
            HashMap::<VkDescriptorSetLayoutKey, Arc<VkDescriptorSetLayout>>::new();

        let bindless_texture_descriptor_set =
            if device.features.contains(VkFeatures::DESCRIPTOR_INDEXING) {
                let bindless_set = Arc::new(VkBindlessDescriptorSet::new(
                    device,
                    vk::DescriptorType::SAMPLED_IMAGE,
                ));
                let (layout_key, descriptor_layout) = bindless_set.layout();
                descriptor_set_layouts.insert(layout_key.clone(), descriptor_layout.clone());
                Some(bindless_set)
            } else {
                None
            };

        let shader_bytes = include_bytes!("../meta_shaders/clear_buffer.comp.spv");
        let shader = Arc::new(VkShader::new(
            device,
            ShaderType::ComputeShader,
            &shader_bytes[..],
            Some("ClearBufferMeta"),
        ));
        let clear_buffer_meta_pipeline = Arc::new(VkPipeline::new_compute_meta(
            device,
            &shader,
            Some("ClearBufferPipeline"),
        ));

        Self {
            device: device.clone(),
            buffers: BufferAllocator::new(device, true),
            descriptor_set_layouts: RwLock::new(descriptor_set_layouts),
            pipeline_layouts: RwLock::new(HashMap::new()),
            render_passes: RwLock::new(HashMap::new()),
            frame_buffers: RwLock::new(HashMap::new()),
            bindless_texture_descriptor_set,
            clear_buffer_meta_pipeline,
        }
    }

    #[inline]
    pub(crate) fn get_clear_buffer_meta_pipeline(&self) -> &Arc<VkPipeline> {
        &self.clear_buffer_meta_pipeline
    }

    pub(crate) fn get_descriptor_set_layout(
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
    pub(crate) fn get_pipeline_layout(
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

    pub(crate) fn get_render_pass(&self, info: VkRenderPassInfo) -> Arc<VkRenderPass> {
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

    pub(crate) fn get_framebuffer(
        &self,
        render_pass: &Arc<VkRenderPass>,
        attachments: &[&Arc<VkTextureView>],
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
                a.texture().info().width.max(old.0),
                a.texture().info().height.max(old.1),
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
    pub(crate) fn buffer_allocator(&self) -> &BufferAllocator {
        &self.buffers
    }

    #[inline]
    pub(crate) fn bindless_texture_descriptor_set(&self) -> Option<&Arc<VkBindlessDescriptorSet>> {
        self.bindless_texture_descriptor_set.as_ref()
    }
}
