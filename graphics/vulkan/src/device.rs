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
use crate::renderpass::VkRenderPassLayout;
use crate::renderpass::VkRenderPass;
use crate::texture::VkTexture;
use crate::texture::VkRenderTargetView;
use crate::sync::VkSemaphore;
use crate::sync::VkFence;
use crate::graph::VkRenderGraph;
use crate::swapchain::VkSwapchain;
use context::VkGraphicsContext;
use raw::{RawVkDevice, RawVkInstance};
use std::collections::HashMap;
use pipeline::VkPipelineInfo;

pub struct VkDevice {
  device: Arc<RawVkDevice>,
  graphics_queue: VkQueue,
  compute_queue: Option<VkQueue>,
  transfer_queue: Option<VkQueue>,
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

    let context = Arc::new(VkGraphicsContext::new(&raw));

    let graphics_queue = {
      let vk_queue = unsafe { raw.device.get_device_queue(graphics_queue_info.queue_family_index as u32, graphics_queue_info.queue_index as u32) };
      VkQueue::new(graphics_queue_info, vk_queue, &raw, context.get_caches())
    };

    let compute_queue = compute_queue_info.map(|info| {
      let vk_queue = unsafe { raw.device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32) };
      VkQueue::new(info.clone(), vk_queue, &raw, context.get_caches())
    });

    let transfer_queue = transfer_queue_info.map(|info| {
      let vk_queue = unsafe { raw.device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32) };
      VkQueue::new(info.clone(), vk_queue, &raw, context.get_caches())
    });

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
}

impl Device<VkBackend> for VkDevice {
  fn get_queue(&self, queue_type: QueueType) -> Option<&VkQueue> {
    return match queue_type {
      QueueType::Graphics => {
        Some(&self.graphics_queue)
      }
      QueueType::Compute => {
        self.compute_queue.as_ref()
      }
      QueueType::Transfer => {
        self.transfer_queue.as_ref()
      }
    }
  }

  fn create_buffer(&self, size: usize, memory_usage: MemoryUsage, usage: BufferUsage) -> VkBuffer {
    return VkBuffer::new(&self.device, size, memory_usage, &self.device.allocator, usage);
  }

  fn create_shader(&self, shader_type: ShaderType, bytecode: &Vec<u8>) -> VkShader {
    return VkShader::new(&self.device, shader_type, bytecode);
  }

  fn create_pipeline(&self, info: &PipelineInfo<VkBackend>) -> VkPipeline {
    return VkPipeline::new(&self.device, info);
  }

  fn create_renderpass_layout(&self, info: &RenderPassLayoutInfo) -> VkRenderPassLayout {
    return VkRenderPassLayout::new(&self.device, info);
  }

  fn create_renderpass(&self, info: &RenderPassInfo<VkBackend>) -> VkRenderPass {
    return VkRenderPass::new(&self.device, info);
  }

  fn create_render_target_view(&self, texture: Arc<VkTexture>) -> VkRenderTargetView {
    return VkRenderTargetView::new(&self.device, texture);
  }

  fn create_semaphore(&self) -> VkSemaphore {
    return VkSemaphore::new(&self.device);
  }

  fn create_fence(&self) -> VkFence {
    return VkFence::new(&self.device);
  }

  fn wait_for_idle(&self) {
    unsafe { self.device.device.device_wait_idle(); }
  }

  fn create_render_graph(self: Arc<Self>, graph_info: &sourcerenderer_core::graphics::graph::RenderGraphInfo, swapchain: &VkSwapchain) -> VkRenderGraph {
    return VkRenderGraph::new(&self.device, &self.context, graph_info, swapchain);
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
