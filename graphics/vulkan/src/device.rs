use std::sync::Arc;
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
use crate::renderpass::VkRenderPassLayout;
use crate::renderpass::VkRenderPass;
use crate::texture::VkTexture;
use crate::texture::VkRenderTargetView;
use crate::sync::VkSemaphore;
use crate::sync::VkFence;

pub struct VkDevice {
  adapter: Arc<VkAdapter>,
  device: ash::Device,
  graphics_queue_info: VkQueueInfo,
  compute_queue_info: Option<VkQueueInfo>,
  transfer_queue_info: Option<VkQueueInfo>,
  allocator: Mutex<vk_mem::Allocator>,
  extensions: VkAdapterExtensionSupport
}

impl VkDevice {
  pub fn new(
    adapter: Arc<VkAdapter>,
    device: ash::Device,
    graphics_queue_info: VkQueueInfo,
    compute_queue_info: Option<VkQueueInfo>,
    transfer_queue_info: Option<VkQueueInfo>,
    extensions: VkAdapterExtensionSupport) -> Self {

    let allocator_info = vk_mem::AllocatorCreateInfo {
      physical_device: *adapter.get_physical_device_handle(),
      device: device.clone(),
      instance: adapter.get_instance().get_ash_instance().clone(),
      flags: if extensions.intersects(VkAdapterExtensionSupport::DEDICATED_ALLOCATION) && extensions.intersects(VkAdapterExtensionSupport::GET_MEMORY_PROPERTIES2) { vk_mem::AllocatorCreateFlags::KHR_DEDICATED_ALLOCATION } else { vk_mem::AllocatorCreateFlags::NONE },
      preferred_large_heap_block_size: 0,
      frame_in_use_count: 3,
      heap_size_limits: None
    };
    let allocator = vk_mem::Allocator::new(&allocator_info).expect("Failed to create memory allocator.");

    return VkDevice {
      adapter: adapter,
      device: device,
      graphics_queue_info: graphics_queue_info,
      compute_queue_info: compute_queue_info,
      transfer_queue_info: transfer_queue_info,
      allocator: Mutex::new(allocator),
      extensions: extensions
    };
  }

  pub fn get_ash_device(&self) -> &ash::Device {
    return &self.device;
  }

  pub fn get_adapter(&self) -> &VkAdapter {
    return self.adapter.as_ref();
  }

  pub fn get_allocator(&self) -> &Mutex<vk_mem::Allocator> {
    return &self.allocator;
  }
}

impl Drop for VkDevice {
  fn drop(&mut self) {
    let mut allocator = self.allocator.lock().unwrap();
    allocator.destroy();
    unsafe {
      self.device.destroy_device(None);
    }
  }
}

impl Device<VkBackend> for VkDevice {
  fn create_queue(self: Arc<Self>, queue_type: QueueType) -> Option<Arc<VkQueue>> {
    return match queue_type {
      QueueType::Graphics => {
        let vk_queue = unsafe { self.device.get_device_queue(self.graphics_queue_info.queue_family_index as u32, self.graphics_queue_info.queue_index as u32) };
        return Some(Arc::new(VkQueue::new(self.graphics_queue_info.clone(), vk_queue, self.clone())));
      }
      QueueType::Compute => {
        self.compute_queue_info.map(|info| {
            let vk_queue = unsafe { self.device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32) };
            Arc::new(VkQueue::new(info.clone(), vk_queue, self.clone()))
          }
        )
      }
      QueueType::Transfer => {
        self.transfer_queue_info.map(|info| {
            let vk_queue = unsafe { self.device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32) };
            Arc::new(VkQueue::new(info.clone(), vk_queue, self.clone()))
          }
        )
      }
    }
  }

  fn create_buffer(self: Arc<Self>, size: usize, memory_usage: MemoryUsage, usage: BufferUsage) -> Arc<VkBuffer> {
    let mut allocator = self.allocator.lock().unwrap();
    return Arc::new(VkBuffer::new(self.clone(), size, memory_usage, &mut allocator, usage));
  }

  fn create_shader(self: Arc<Self>, shader_type: ShaderType, bytecode: &Vec<u8>) -> Arc<VkShader> {
    return Arc::new(VkShader::new(self, shader_type, bytecode));
  }

  fn create_pipeline(self: Arc<Self>, info: &PipelineInfo<VkBackend>) -> Arc<VkPipeline> {
    return Arc::new(VkPipeline::new(self.clone(), info));
  }

  fn create_renderpass_layout(self: Arc<Self>, info: &RenderPassLayoutInfo) -> Arc<VkRenderPassLayout> {
    return Arc::new(VkRenderPassLayout::new(self.clone(), info));
  }

  fn create_renderpass(self: Arc<Self>, info: &RenderPassInfo<VkBackend>) -> Arc<VkRenderPass> {
    return Arc::new(VkRenderPass::new(self.clone(), info));
  }

  fn create_render_target_view(self: Arc<Self>, texture: Arc<VkTexture>) -> Arc<VkRenderTargetView> {
    return Arc::new(VkRenderTargetView::new(self.clone(), texture));
  }

  fn create_semaphore(self: Arc<Self>) -> Arc<VkSemaphore> {
    return Arc::new(VkSemaphore::new(self.clone()));
  }

  fn create_fence(self: Arc<Self>) -> Arc<VkFence> {
    return Arc::new(VkFence::new(self.clone()));
  }

  fn wait_for_idle(&self) {
    unsafe { self.device.device_wait_idle(); }
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
