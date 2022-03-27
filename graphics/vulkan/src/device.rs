use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc};

use ash::vk;

use sourcerenderer_core::graphics::*;
use crate::bindless::VkBindlessDescriptorSet;
use crate::rt::VkAccelerationStructure;
use crate::{queue::VkQueue, texture::VkSampler};
use crate::queue::{VkQueueInfo, VkQueueType};
use crate::{VkBackend, VkRenderPass, VkSemaphore};
use crate::VkAdapterExtensionSupport;
use crate::pipeline::VkPipeline;
use crate::pipeline::VkShader;
use crate::texture::VkTexture;
use crate::sync::VkFence;

use crate::{VkThreadManager, VkShared};
use crate::raw::{RawVkDevice, RawVkInstance, VkFeatures};
use crate::pipeline::VkGraphicsPipelineInfo;
use crate::buffer::VkBufferSlice;
use std::cmp::min;
use crate::texture::VkTextureView;
use crate::transfer::VkTransfer;

pub struct VkDevice {
  device: Arc<RawVkDevice>,
  graphics_queue: Arc<VkQueue>,
  compute_queue: Option<Arc<VkQueue>>,
  transfer_queue: Option<Arc<VkQueue>>,
  context: Arc<VkThreadManager>,
  transfer: VkTransfer,
  shared: Arc<VkShared>,
  query_count: AtomicU64,
}

impl VkDevice {
  pub fn new(
    device: ash::Device,
    instance: &Arc<RawVkInstance>,
    physical_device: vk::PhysicalDevice,
    graphics_queue_info: VkQueueInfo,
    compute_queue_info: Option<VkQueueInfo>,
    transfer_queue_info: Option<VkQueueInfo>,
    features: VkFeatures,
    max_surface_image_count: u32) -> Self {

    let mut vma_flags = vk_mem::AllocatorCreateFlags::NONE;
    if features.intersects(VkFeatures::DEDICATED_ALLOCATION) {
      vma_flags |= vk_mem::AllocatorCreateFlags::KHR_DEDICATED_ALLOCATION;
    }
    if features.intersects(VkFeatures::RAY_TRACING) {
      vma_flags |= vk_mem::AllocatorCreateFlags::BUFFER_DEVICE_ADDRESS;
    }

    let allocator_info = vk_mem::AllocatorCreateInfo {
      physical_device,
      device: device.clone(),
      instance: instance.instance.clone(),
      flags: vma_flags,
      preferred_large_heap_block_size: 0,
      frame_in_use_count: 3,
      heap_size_limits: None,
      allocation_callbacks: None,
      vulkan_api_version: vk::API_VERSION_1_1
    };
    let allocator = unsafe { vk_mem::Allocator::new(&allocator_info).expect("Failed to create memory allocator.") };

    let raw_graphics_queue = unsafe { device.get_device_queue(graphics_queue_info.queue_family_index as u32, graphics_queue_info.queue_index as u32) };
    let raw_compute_queue = compute_queue_info.map(|info| unsafe { device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32) });
    let raw_transfer_queue = transfer_queue_info.map(|info| unsafe { device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32) });

    let raw = Arc::new(RawVkDevice::new(
      device,
      allocator,
      physical_device,
      instance.clone(),
      features,
      graphics_queue_info,
      compute_queue_info,
      transfer_queue_info,
      raw_graphics_queue,
      raw_compute_queue,
      raw_transfer_queue
    ));

    let shared = Arc::new(VkShared::new(&raw));

    let context = Arc::new(VkThreadManager::new(&raw, &graphics_queue_info, compute_queue_info.as_ref(), transfer_queue_info.as_ref(), &shared, min(3, max_surface_image_count)));

    let graphics_queue = {
      Arc::new(VkQueue::new(graphics_queue_info, VkQueueType::Graphics, &raw, &shared, &context))
    };

    let compute_queue = compute_queue_info.map(|info|
      Arc::new(VkQueue::new(info, VkQueueType::Compute, &raw, &shared, &context))
    );

    let transfer_queue = transfer_queue_info.map(|info|
      Arc::new(VkQueue::new(info, VkQueueType::Transfer, &raw, &shared, &context))
    );

    let transfer = VkTransfer::new(&raw, &graphics_queue, &transfer_queue, &shared);

    VkDevice {
      device: raw,
      graphics_queue,
      compute_queue,
      transfer_queue,
      context,
      transfer,
      shared,
      query_count: AtomicU64::new(0),
    }
  }

  #[inline]
  pub fn get_inner(&self) -> &Arc<RawVkDevice> {
    &self.device
  }

  #[inline]
  pub fn get_graphics_queue(&self) -> &Arc<VkQueue> {
    &self.graphics_queue
  }

  #[inline]
  pub fn get_compute_queue(&self) -> &Option<Arc<VkQueue>> {
    &self.compute_queue
  }

  #[inline]
  pub fn get_transfer_queue(&self) -> &Option<Arc<VkQueue>> {
    &self.transfer_queue
  }
}

impl Device<VkBackend> for VkDevice {
  fn create_buffer(&self, info: &BufferInfo, memory_usage: MemoryUsage, name: Option<&str>) -> Arc<VkBufferSlice> {
    self.context.get_shared().get_buffer_allocator().get_slice(info, memory_usage, name)
  }

  fn upload_data<T>(&self, data: &[T], memory_usage: MemoryUsage, usage: BufferUsage) -> Arc<VkBufferSlice> where T: 'static + Send + Sync + Sized + Clone {
    assert_ne!(memory_usage, MemoryUsage::GpuOnly);
    let slice = self.context.get_shared().get_buffer_allocator().get_slice(&BufferInfo {
      size: std::mem::size_of_val(data),
      usage
    }, memory_usage, None);
    unsafe {
      let ptr = slice.map_unsafe(false).expect("Failed to map buffer slice");
      std::ptr::copy(data.as_ptr(), ptr as *mut T, data.len());
      slice.unmap_unsafe(true);
    }
    slice
  }

  fn create_shader(&self, shader_type: ShaderType, bytecode: &[u8], name: Option<&str>) -> Arc<VkShader> {
    Arc::new(VkShader::new(&self.device, shader_type, bytecode, name))
  }

  fn create_texture(&self, info: &TextureInfo, name: Option<&str>) -> Arc<VkTexture> {
    Arc::new(VkTexture::new(&self.device, info, name))
  }

  fn create_sampling_view(&self, texture: &Arc<VkTexture>, info: &TextureSamplingViewInfo, name: Option<&str>) -> Arc<VkTextureView> {
    Arc::new(VkTextureView::new(&self.device, texture, info, name))
  }

  fn create_render_target_view(&self, texture: &Arc<VkTexture>, info: &TextureRenderTargetViewInfo, name: Option<&str>) -> Arc<VkTextureView> {
    let srv_info = TextureSamplingViewInfo {
      base_mip_level: info.base_mip_level,
      mip_level_length: info.mip_level_length,
      base_array_level: info.base_array_level,
      array_level_length: info.array_level_length,
    };
    Arc::new(VkTextureView::new(&self.device, texture, &srv_info, name))
  }

  fn create_storage_view(&self, texture: &Arc<VkTexture>, info: &TextureStorageViewInfo, name: Option<&str>) -> Arc<VkTextureView> {
    let srv_info = TextureSamplingViewInfo {
      base_mip_level: info.base_mip_level,
      mip_level_length: info.mip_level_length,
      base_array_level: info.base_array_level,
      array_level_length: info.array_level_length,
    };
    Arc::new(VkTextureView::new(&self.device, texture, &srv_info, name))
  }

  fn create_depth_stencil_view(&self, texture: &Arc<VkTexture>, info: &TextureDepthStencilViewInfo, name: Option<&str>) -> Arc<VkTextureView> {
    assert!(texture.get_info().format.is_depth() || texture.get_info().format.is_stencil());
    let srv_info = TextureSamplingViewInfo {
      base_mip_level: info.base_mip_level,
      mip_level_length: info.mip_level_length,
      base_array_level: info.base_array_level,
      array_level_length: info.array_level_length,
    };
    Arc::new(VkTextureView::new(&self.device, texture, &srv_info, name))
  }

  fn create_sampler(&self, info: &SamplerInfo) -> Arc<VkSampler> {
    Arc::new(VkSampler::new(&self.device, info))
  }

  fn create_compute_pipeline(&self, shader: &Arc<VkShader>) -> Arc<VkPipeline> {
    Arc::new(VkPipeline::new_compute(&self.device, shader, self.context.shared()))
  }

  fn wait_for_idle(&self) {
    self.device.wait_for_idle();
  }

  fn create_graphics_pipeline(&self, info: &GraphicsPipelineInfo<VkBackend>, renderpass_info: &RenderPassInfo, subpass: u32) -> Arc<<VkBackend as Backend>::GraphicsPipeline> {
    let shared = self.context.get_shared();
    let mut rp_opt = {
      let render_passes = shared.get_render_passes().read().unwrap();
      render_passes.get(renderpass_info).cloned()
    };
    if rp_opt.is_none() {
      let rp = Arc::new(VkRenderPass::new(&self.device, renderpass_info));
      let mut render_passes = shared.get_render_passes().write().unwrap();
      render_passes.insert(renderpass_info.clone(), rp.clone());
      rp_opt = Some(rp);
    }
    let rp = rp_opt.unwrap();
    let vk_info = VkGraphicsPipelineInfo {
      info,
      render_pass: &rp,
      sub_pass: subpass,
    };
    Arc::new(VkPipeline::new_graphics(&self.device, &vk_info, shared))
  }

  fn init_texture(&self, texture: &Arc<VkTexture>, buffer: &Arc<VkBufferSlice>, mip_level: u32, array_layer: u32, buffer_offset: usize) {
    self.transfer.init_texture(texture, buffer, mip_level, array_layer, buffer_offset);
  }

  fn init_texture_async(&self, texture: &Arc<VkTexture>, buffer: &Arc<VkBufferSlice>, mip_level: u32, array_layer: u32, buffer_offset: usize) -> Option<Arc<VkFence>> {
    self.transfer.init_texture_async(texture, buffer, mip_level, array_layer, buffer_offset)
  }

  fn init_buffer(&self, src_buffer: &Arc<VkBufferSlice>, dst_buffer: &Arc<VkBufferSlice>, src_offset: usize, dst_offset: usize, length: usize) {
    self.transfer.init_buffer(src_buffer, dst_buffer, src_offset, dst_offset, length);
  }

  fn flush_transfers(&self) {
    self.transfer.flush();
  }

  fn free_completed_transfers(&self) {
    self.transfer.try_free_used_buffers();
  }

  fn create_fence(&self) -> Arc<VkFence> {
    self.context.shared().get_fence()
  }

  fn create_semaphore(&self) -> Arc<VkSemaphore> {
    self.context.shared().get_semaphore()
  }

  fn graphics_queue(&self) -> &Arc<VkQueue> {
    &self.graphics_queue
  }

  fn prerendered_frames(&self) -> u32 {
    self.context.prerendered_frames()
  }

  fn supports_bindless(&self) -> bool {
    self.device.features.contains(VkFeatures::DESCRIPTOR_INDEXING)
  }

  fn supports_ray_tracing(&self) -> bool {
    self.device.features.contains(VkFeatures::RAY_TRACING)
  }

  fn insert_texture_into_bindless_heap(&self, texture: &Arc<VkTextureView>) -> u32 {
    let bindless_set = self.shared.bindless_texture_descriptor_set().expect("Descriptor indexing is not supported on this device.");
    let slot = bindless_set.write_texture_descriptor(texture);
    texture.texture().set_bindless_slot(bindless_set, slot);
    slot
  }

  fn get_bottom_level_acceleration_structure_size(&self, info: &BottomLevelAccelerationStructureInfo<VkBackend>) -> AccelerationStructureSizes {
    VkAccelerationStructure::bottom_level_size(&self.device, info)
  }

  fn get_top_level_acceleration_structure_size(&self, info: &TopLevelAccelerationStructureInfo<VkBackend>) -> AccelerationStructureSizes {
    VkAccelerationStructure::top_level_size(&self.device, info)
  }

  fn create_raytracing_pipeline(&self, info: &RayTracingPipelineInfo<VkBackend>) -> Arc<VkPipeline> {
    Arc::new(VkPipeline::new_ray_tracing(&self.device, info, &self.shared))
  }
}

impl Drop for VkDevice {
  fn drop(&mut self) {
    self.wait_for_idle();
  }
}

pub fn memory_usage_to_vma(memory_usage: MemoryUsage) -> vk_mem::MemoryUsage {
  match memory_usage {
    MemoryUsage::CpuOnly => vk_mem::MemoryUsage::CpuOnly,
    MemoryUsage::GpuOnly => vk_mem::MemoryUsage::GpuOnly,
    MemoryUsage::CpuToGpu => vk_mem::MemoryUsage::CpuToGpu,
    MemoryUsage::GpuToCpu => vk_mem::MemoryUsage::GpuToCpu,
  }
}
