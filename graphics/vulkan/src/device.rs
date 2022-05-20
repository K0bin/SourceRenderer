use std::sync::atomic::AtomicU64;
use std::sync::{Arc};

use ash::vk;

use sourcerenderer_core::graphics::*;
use crate::renderpass::{VkRenderPassInfo, VkAttachmentInfo, VkSubpassInfo};
use crate::rt::VkAccelerationStructure;
use crate::{queue::VkQueue, texture::VkSampler};
use crate::queue::{VkQueueInfo, VkQueueType};
use crate::{VkBackend, VkSemaphore};
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

    let mut vma_flags = vma_sys::VmaAllocatorCreateFlags::default();
    if features.intersects(VkFeatures::DEDICATED_ALLOCATION) {
      vma_flags |= vma_sys::VmaAllocatorCreateFlagBits_VMA_ALLOCATOR_CREATE_KHR_DEDICATED_ALLOCATION_BIT as u32;
    }
    if features.intersects(VkFeatures::RAY_TRACING) {
      vma_flags |= vma_sys::VmaAllocatorCreateFlagBits_VMA_ALLOCATOR_CREATE_BUFFER_DEVICE_ADDRESS_BIT as u32;
    }

    let allocator = unsafe {
      unsafe extern "system" fn get_instance_proc_addr_stub(
          _instance: ash::vk::Instance,
          _p_name: *const ::std::os::raw::c_char,
      ) -> ash::vk::PFN_vkVoidFunction {
          panic!("VMA_DYNAMIC_VULKAN_FUNCTIONS is unsupported")
      }

      unsafe extern "system" fn get_get_device_proc_stub(
          _device: ash::vk::Device,
          _p_name: *const ::std::os::raw::c_char,
      ) -> ash::vk::PFN_vkVoidFunction {
          panic!("VMA_DYNAMIC_VULKAN_FUNCTIONS is unsupported")
      }

      let routed_functions = vma_sys::VmaVulkanFunctions {
        vkGetInstanceProcAddr: get_instance_proc_addr_stub,
        vkGetDeviceProcAddr: get_get_device_proc_stub,
        vkGetPhysicalDeviceProperties: instance
            .fp_v1_0()
            .get_physical_device_properties,
        vkGetPhysicalDeviceMemoryProperties: instance
            .fp_v1_0()
            .get_physical_device_memory_properties,
        vkAllocateMemory: device.fp_v1_0().allocate_memory,
        vkFreeMemory: device.fp_v1_0().free_memory,
        vkMapMemory: device.fp_v1_0().map_memory,
        vkUnmapMemory: device.fp_v1_0().unmap_memory,
        vkFlushMappedMemoryRanges: device.fp_v1_0().flush_mapped_memory_ranges,
        vkInvalidateMappedMemoryRanges: device
            .fp_v1_0()
            .invalidate_mapped_memory_ranges,
        vkBindBufferMemory: device.fp_v1_0().bind_buffer_memory,
        vkBindImageMemory: device.fp_v1_0().bind_image_memory,
        vkGetBufferMemoryRequirements: device
            .fp_v1_0()
            .get_buffer_memory_requirements,
        vkGetImageMemoryRequirements: device
            .fp_v1_0()
            .get_image_memory_requirements,
        vkCreateBuffer: device.fp_v1_0().create_buffer,
        vkDestroyBuffer: device.fp_v1_0().destroy_buffer,
        vkCreateImage: device.fp_v1_0().create_image,
        vkDestroyImage: device.fp_v1_0().destroy_image,
        vkCmdCopyBuffer: device.fp_v1_0().cmd_copy_buffer,
        vkGetBufferMemoryRequirements2KHR: device
            .fp_v1_1()
            .get_buffer_memory_requirements2,
        vkGetImageMemoryRequirements2KHR: device
            .fp_v1_1()
            .get_image_memory_requirements2,
        vkBindBufferMemory2KHR: device.fp_v1_1().bind_buffer_memory2,
        vkBindImageMemory2KHR: device.fp_v1_1().bind_image_memory2,
        vkGetPhysicalDeviceMemoryProperties2KHR: instance
            .fp_v1_1()
            .get_physical_device_memory_properties2,
        vkGetDeviceBufferMemoryRequirements: device.fp_v1_3().get_device_buffer_memory_requirements,
        vkGetDeviceImageMemoryRequirements: device.fp_v1_3().get_device_image_memory_requirements,
      };

      let vma_create_info = vma_sys::VmaAllocatorCreateInfo {
        flags: vma_flags,
        physicalDevice: physical_device,
        device: device.handle(),
        preferredLargeHeapBlockSize: 0,
        pAllocationCallbacks: std::ptr::null(),
        pDeviceMemoryCallbacks: std::ptr::null(),
        pHeapSizeLimit: std::ptr::null(),
        pVulkanFunctions: &routed_functions,
        instance: instance.handle(),
        vulkanApiVersion: vk::API_VERSION_1_1,
        pTypeExternalMemoryHandleTypes: std::ptr::null()
      };

      let mut allocator: vma_sys::VmaAllocator = std::ptr::null_mut();
      assert_eq!(vma_sys::vmaCreateAllocator(&vma_create_info, &mut allocator), vk::Result::SUCCESS);
      allocator
    };

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
  pub fn inner(&self) -> &Arc<RawVkDevice> {
    &self.device
  }

  #[inline]
  pub fn graphics_queue(&self) -> &Arc<VkQueue> {
    &self.graphics_queue
  }

  #[inline]
  pub fn compute_queue(&self) -> &Option<Arc<VkQueue>> {
    &self.compute_queue
  }

  #[inline]
  pub fn transfer_queue(&self) -> &Option<Arc<VkQueue>> {
    &self.transfer_queue
  }
}

impl Device<VkBackend> for VkDevice {
  fn create_buffer(&self, info: &BufferInfo, memory_usage: MemoryUsage, name: Option<&str>) -> Arc<VkBufferSlice> {
    self.context.shared().buffer_allocator().get_slice(info, memory_usage, name)
  }

  fn upload_data<T>(&self, data: &[T], memory_usage: MemoryUsage, usage: BufferUsage) -> Arc<VkBufferSlice> where T: 'static + Send + Sync + Sized + Clone {
    assert_ne!(memory_usage, MemoryUsage::VRAM);
    let slice = self.context.shared().buffer_allocator().get_slice(&BufferInfo {
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

  fn create_sampling_view(&self, texture: &Arc<VkTexture>, info: &TextureViewInfo, name: Option<&str>) -> Arc<VkTextureView> {
    Arc::new(VkTextureView::new(&self.device, texture, info, name))
  }

  fn create_render_target_view(&self, texture: &Arc<VkTexture>, info: &TextureViewInfo, name: Option<&str>) -> Arc<VkTextureView> {
    let srv_info = TextureViewInfo {
      base_mip_level: info.base_mip_level,
      mip_level_length: info.mip_level_length,
      base_array_layer: info.base_array_layer,
      array_layer_length: info.array_layer_length,
    };
    Arc::new(VkTextureView::new(&self.device, texture, &srv_info, name))
  }

  fn create_storage_view(&self, texture: &Arc<VkTexture>, info: &TextureViewInfo, name: Option<&str>) -> Arc<VkTextureView> {
    let srv_info = TextureViewInfo {
      base_mip_level: info.base_mip_level,
      mip_level_length: info.mip_level_length,
      base_array_layer: info.base_array_layer,
      array_layer_length: info.array_layer_length,
    };
    Arc::new(VkTextureView::new(&self.device, texture, &srv_info, name))
  }

  fn create_depth_stencil_view(&self, texture: &Arc<VkTexture>, info: &TextureViewInfo, name: Option<&str>) -> Arc<VkTextureView> {
    assert!(texture.info().format.is_depth() || texture.info().format.is_stencil());
    let srv_info = TextureViewInfo {
      base_mip_level: info.base_mip_level,
      mip_level_length: info.mip_level_length,
      base_array_layer: info.base_array_layer,
      array_layer_length: info.array_layer_length,
    };
    Arc::new(VkTextureView::new(&self.device, texture, &srv_info, name))
  }

  fn create_sampler(&self, info: &SamplerInfo) -> Arc<VkSampler> {
    Arc::new(VkSampler::new(&self.device, info))
  }

  fn create_compute_pipeline(&self, shader: &Arc<VkShader>, name: Option<&str>) -> Arc<VkPipeline> {
    Arc::new(VkPipeline::new_compute(&self.device, shader, self.context.shared(), name))
  }

  fn wait_for_idle(&self) {
    self.device.wait_for_idle();
  }

  fn create_graphics_pipeline(&self, info: &GraphicsPipelineInfo<VkBackend>, renderpass_info: &RenderPassInfo, subpass: u32, name: Option<&str>) -> Arc<<VkBackend as Backend>::GraphicsPipeline> {
    let shared = self.context.shared();
    let rp_info = VkRenderPassInfo {
      attachments: renderpass_info.attachments.iter().map(|a| VkAttachmentInfo {
          format: a.format,
          samples: a.samples,
          load_op: LoadOp::DontCare,
          store_op: StoreOp::DontCare,
          stencil_load_op: LoadOp::DontCare,
          stencil_store_op: StoreOp::DontCare,
      }).collect(),
      subpasses: renderpass_info.subpasses.iter().map(|sp| VkSubpassInfo {
        input_attachments: sp.input_attachments.iter().cloned().collect(),
        output_color_attachments: sp.output_color_attachments.iter().cloned().collect(),
        depth_stencil_attachment: sp.depth_stencil_attachment.clone(),
      }).collect(),
    };
    let rp = shared.get_render_pass(rp_info);
    let vk_info = VkGraphicsPipelineInfo {
      info,
      render_pass: &rp,
      sub_pass: subpass,
    };
    Arc::new(VkPipeline::new_graphics(&self.device, &vk_info, shared, name))
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

  fn supports_indirect(&self) -> bool {
    self.device.features.contains(VkFeatures::ADVANCED_INDIRECT)
  }

  fn supports_min_max_filter(&self) -> bool {
    self.device.features.contains(VkFeatures::MIN_MAX_FILTER)
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

#[derive(Debug)]
pub(crate) struct VulkanMemoryFlags {
  pub(crate) preferred: vk::MemoryPropertyFlags,
  pub(crate) required: vk::MemoryPropertyFlags
}

pub(crate) fn memory_usage_to_vma(memory_usage: MemoryUsage) -> VulkanMemoryFlags {
  use vk::MemoryPropertyFlags as VkMem;
  match memory_usage {
    MemoryUsage::CachedRAM => VulkanMemoryFlags {
      preferred: VkMem::HOST_COHERENT,
      required: VkMem::HOST_VISIBLE | VkMem::HOST_CACHED
    },
    MemoryUsage::VRAM => VulkanMemoryFlags {
      preferred: VkMem::DEVICE_LOCAL,
      required: VkMem::empty()
    },
    MemoryUsage::UncachedRAM => VulkanMemoryFlags {
      preferred: VkMem::HOST_COHERENT,
      required: VkMem::HOST_VISIBLE
    },
    MemoryUsage::MappableVRAM => VulkanMemoryFlags {
      preferred: VkMem::DEVICE_LOCAL | VkMem::HOST_COHERENT, // Fall back to uncached RAM
      required: VkMem::HOST_VISIBLE
    }
  }
}
