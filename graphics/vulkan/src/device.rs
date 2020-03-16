use std::sync::{Arc, RwLock};
use std::sync::Weak;
use std::sync::Mutex;

use ash::vk;
use ash::extensions::khr;
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};

use sourcerenderer_core::graphics::*;
use crate::queue::VkQueue;
use crate::queue::VkQueueInfo;
use crate::adapter::VkAdapter;
use crate::VkBackend;
use crate::buffer::VkBuffer;
use crate::buffer::buffer_usage_to_vk;
use crate::VkAdapterExtensionSupport;
use crate::pipeline::VkPipeline;
use crate::pipeline::VkShader;
use crate::renderpass::VkRenderPass;
use crate::texture::VkTexture;
use crate::texture::VkRenderTargetView;
use crate::sync::VkSemaphore;
use crate::sync::VkFence;
use crate::graph::VkRenderGraph;
use crate::swapchain::VkSwapchain;
use context::{VkGraphicsContext, VkSharedCaches};
use raw::{RawVkDevice, RawVkInstance};
use std::collections::HashMap;
use pipeline::VkPipelineInfo;
use buffer::VkBufferSlice;

pub struct VkDevice {
  device: Arc<RawVkDevice>,
  graphics_queue: Arc<VkQueue>,
  compute_queue: Option<Arc<VkQueue>>,
  transfer_queue: Option<Arc<VkQueue>>,
  extensions: VkAdapterExtensionSupport,
  context: Arc<VkGraphicsContext>
}

impl VkDevice {
  pub fn new(
    device: ash::Device,
    instance: &Arc<RawVkInstance>,
    physical_device: vk::PhysicalDevice,
    graphics_queue_info: VkQueueInfo,
    compute_queue_info: Option<VkQueueInfo>,
    transfer_queue_info: Option<VkQueueInfo>,
    extensions: VkAdapterExtensionSupport) -> Self {

    let allocator_info = vk_mem::AllocatorCreateInfo {
      physical_device,
      device: device.clone(),
      instance: instance.instance.clone(),
      flags: if extensions.intersects(VkAdapterExtensionSupport::DEDICATED_ALLOCATION) && extensions.intersects(VkAdapterExtensionSupport::GET_MEMORY_PROPERTIES2) { vk_mem::AllocatorCreateFlags::KHR_DEDICATED_ALLOCATION } else { vk_mem::AllocatorCreateFlags::NONE },
      preferred_large_heap_block_size: 0,
      frame_in_use_count: 3,
      heap_size_limits: None
    };
    let allocator = vk_mem::Allocator::new(&allocator_info).expect("Failed to create memory allocator.");

    let raw = Arc::new(RawVkDevice {
      device,
      allocator,
      physical_device,
      instance: instance.clone(),
    });

    let caches = Arc::new(VkSharedCaches::new(&raw));

    let graphics_queue = {
      let vk_queue = unsafe { raw.device.get_device_queue(graphics_queue_info.queue_family_index as u32, graphics_queue_info.queue_index as u32) };
      Arc::new(VkQueue::new(graphics_queue_info, vk_queue, &raw, &caches))
    };

    let compute_queue = compute_queue_info.map(|info| {
      let vk_queue = unsafe { raw.device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32) };
      Arc::new(VkQueue::new(info.clone(), vk_queue, &raw, &caches))
    });

    let transfer_queue = transfer_queue_info.map(|info| {
      let vk_queue = unsafe { raw.device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32) };
      Arc::new(VkQueue::new(info.clone(), vk_queue, &raw, &caches))
    });

    let context = Arc::new(VkGraphicsContext::new(&raw, &graphics_queue, &compute_queue, &transfer_queue, &caches));

    return VkDevice {
      device: raw.clone(),
      graphics_queue,
      compute_queue,
      transfer_queue,
      extensions,
      context
    };
  }

  #[inline]
  pub fn get_inner(&self) -> &Arc<RawVkDevice> {
    return &self.device;
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
  fn create_buffer(&self, size: usize, memory_usage: MemoryUsage, usage: BufferUsage) -> VkBufferSlice {
    unimplemented!();
    //return VkBuffer::new(&self.device, size, memory_usage, &self.device.allocator, usage);
  }

  fn upload_data<T>(&self, data: T) -> <VkBackend as Backend>::Buffer {
    let slice = self.context.get_caches().buffers.get_slice(MemoryUsage::CpuToGpu, std::mem::size_of::<T>());
    {
      let mut map = slice.map().expect("Mapping failed");
      std::mem::replace::<T>(map.get_data(), data);
    }
    slice
  }

  fn create_shader(&self, shader_type: ShaderType, bytecode: &Vec<u8>) -> VkShader {
    return VkShader::new(&self.device, shader_type, bytecode);
  }

  fn create_render_target_view(&self, texture: Arc<VkTexture>) -> VkRenderTargetView {
    return VkRenderTargetView::new(&self.device, texture);
  }

  fn wait_for_idle(&self) {
    unsafe { self.device.device.device_wait_idle(); }
  }

  fn create_render_graph(&self, graph_info: &sourcerenderer_core::graphics::graph::RenderGraphInfo<VkBackend>, swapchain: &Arc<VkSwapchain>) -> VkRenderGraph {
    return VkRenderGraph::new(&self.device, &self.context, &self.graphics_queue, &self.compute_queue, &self.transfer_queue, graph_info, swapchain);
  }
}

pub fn memory_usage_to_vma(memory_usage: MemoryUsage) -> vk_mem::MemoryUsage {
  return match memory_usage {
    MemoryUsage::CpuOnly => vk_mem::MemoryUsage::CpuOnly,
    MemoryUsage::GpuOnly => vk_mem::MemoryUsage::GpuOnly,
    MemoryUsage::CpuToGpu => vk_mem::MemoryUsage::CpuToGpu,
    MemoryUsage::GpuToCpu => vk_mem::MemoryUsage::GpuToCpu,
  };
}
