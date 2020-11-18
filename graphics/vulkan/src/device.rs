use std::sync::{Arc};

use ash::vk;
use ash::version::{DeviceV1_0};

use sourcerenderer_core::graphics::*;
use crate::queue::VkQueue;
use crate::queue::VkQueueInfo;
use crate::VkBackend;
use crate::VkAdapterExtensionSupport;
use crate::pipeline::VkPipeline;
use crate::pipeline::VkShader;
use crate::texture::VkTexture;
use crate::sync::VkFence;
use crate::graph::VkRenderGraph;

use ::{VkThreadManager, VkShared};
use raw::{RawVkDevice, RawVkInstance};
use pipeline::VkGraphicsPipelineInfo;
use buffer::VkBufferSlice;
use std::cmp::min;
use texture::VkTextureView;
use transfer::VkTransfer;
use graph_template::{VkRenderGraphTemplate, VkPassType};
use std::collections::HashMap;

pub struct VkDevice {
  device: Arc<RawVkDevice>,
  graphics_queue: Arc<VkQueue>,
  compute_queue: Option<Arc<VkQueue>>,
  transfer_queue: Option<Arc<VkQueue>>,
  extensions: VkAdapterExtensionSupport,
  context: Arc<VkThreadManager>,
  transfer: VkTransfer
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
      extensions: extensions.clone()
    });

    let shared = Arc::new(VkShared::new(&raw));

    let graphics_queue = {
      let vk_queue = unsafe { raw.device.get_device_queue(graphics_queue_info.queue_family_index as u32, graphics_queue_info.queue_index as u32) };
      Arc::new(VkQueue::new(graphics_queue_info, vk_queue, &raw, &shared))
    };

    let compute_queue = compute_queue_info.map(|info| {
      let vk_queue = unsafe { raw.device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32) };
      Arc::new(VkQueue::new(info.clone(), vk_queue, &raw, &shared))
    });

    let transfer_queue = transfer_queue_info.map(|info| {
      let vk_queue = unsafe { raw.device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32) };
      Arc::new(VkQueue::new(info.clone(), vk_queue, &raw, &shared))
    });

    let context = Arc::new(VkThreadManager::new(&raw, &graphics_queue, &compute_queue, &transfer_queue, &shared, 3));

    let transfer = VkTransfer::new(&raw, &graphics_queue, &transfer_queue, &shared);

    return VkDevice {
      device: raw.clone(),
      graphics_queue,
      compute_queue,
      transfer_queue,
      extensions,
      context,
      transfer
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
  fn create_buffer(&self, length: usize, memory_usage: MemoryUsage, usage: BufferUsage) -> Arc<VkBufferSlice> {
    Arc::new(self.context.get_shared().get_buffer_allocator().get_slice(memory_usage, usage, length))
  }

  fn upload_data<T>(&self, data: &T, memory_usage: MemoryUsage, usage: BufferUsage) -> Arc<VkBufferSlice> where T: 'static + Send + Sync + Sized + Clone {
    self.upload_data_raw(unsafe {
      std::slice::from_raw_parts(
        (data as *const T) as *const u8,
        std::mem::size_of_val(data))
      },
      memory_usage,
      usage
    )
  }

  fn upload_data_slice<T>(&self, data: &[T], memory_usage: MemoryUsage, usage: BufferUsage) -> Arc<VkBufferSlice> where T: 'static + Send + Sync + Sized + Clone {
    self.upload_data_raw(unsafe {
      std::slice::from_raw_parts(
        data.as_ptr() as *const u8,
        std::mem::size_of_val(data))
      },
      memory_usage,
      usage
    )
  }

  fn upload_data_raw(&self, data: &[u8], memory_usage: MemoryUsage, usage: BufferUsage) -> Arc<VkBufferSlice> {
    assert_ne!(memory_usage, MemoryUsage::GpuOnly);
    let slice = self.context.get_shared().get_buffer_allocator().get_slice(memory_usage, usage, data.len());
    unsafe {
      let ptr = slice.map_unsafe(false).expect("Failed to map buffer slice");
      std::ptr::copy(data.as_ptr(), ptr, min(data.len(), slice.get_offset_and_length().1));
      slice.unmap_unsafe(true);
    }
    Arc::new(slice)
  }

  fn create_shader(&self, shader_type: ShaderType, bytecode: &Vec<u8>, name: Option<&str>) -> Arc<VkShader> {
    return Arc::new(VkShader::new(&self.device, shader_type, bytecode, name));
  }

  fn create_texture(&self, info: &TextureInfo, name: Option<&str>) -> Arc<VkTexture> {
    return Arc::new(VkTexture::new(&self.device, info, name, vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST));
  }

  fn create_shader_resource_view(&self, texture: &Arc<VkTexture>, info: &TextureShaderResourceViewInfo) -> Arc<VkTextureView> {
    return Arc::new(VkTextureView::new_shader_resource_view(&self.device, texture, info));
  }

  fn create_graphics_pipeline(&self, info: &GraphicsPipelineInfo<VkBackend>, graph_template: &<VkBackend as Backend>::RenderGraphTemplate, pass_name: &str, subpass_index: u32) -> Arc<<VkBackend as Backend>::GraphicsPipeline> {
    let pass = graph_template.passes.iter().find(|pass| pass.name.as_str() == pass_name).expect(format!("Can not find pass {} in render graph.", pass_name).as_str());
    match &pass.pass_type {
      VkPassType::Graphics {
        render_pass, ..
      } => {
        Arc::new(VkPipeline::new_graphics(&self.device, &VkGraphicsPipelineInfo {
          info,
          render_pass,
          sub_pass: subpass_index
        }, self.context.shared()))
      },
      _ => panic!("Pass by name: {} is not a graphics pass.", pass_name)
    }
  }

  fn create_compute_pipeline(&self, shader: &Arc<VkShader>) -> Arc<VkPipeline> {
    Arc::new(VkPipeline::new_compute(&self.device, shader, self.context.shared()))
  }

  fn wait_for_idle(&self) {
    unsafe { self.device.device.device_wait_idle(); }
  }

  fn create_render_graph_template(&self, graph_info: &RenderGraphTemplateInfo) -> Arc<<VkBackend as Backend>::RenderGraphTemplate> {
    Arc::new(VkRenderGraphTemplate::new(&self.device, graph_info))
  }

  fn create_render_graph(&self,
                         template: &Arc<<VkBackend as Backend>::RenderGraphTemplate>,
                         info: &RenderGraphInfo<VkBackend>,
                         swapchain: &Arc<<VkBackend as Backend>::Swapchain>,
                         external_resources: Option<&HashMap<String, ExternalResource<VkBackend>>>) -> <VkBackend as Backend>::RenderGraph {
    VkRenderGraph::new(&self.device, &self.context, &self.graphics_queue, &self.compute_queue, &self.transfer_queue, template, info, swapchain, external_resources)
  }

  fn init_texture(&self, texture: &Arc<VkTexture>, buffer: &Arc<VkBufferSlice>, mip_level: u32, array_layer: u32) -> Arc<VkFence> {
    self.transfer.init_texture(texture, buffer, mip_level, array_layer)
  }

  fn init_buffer(&self, src_buffer: &Arc<VkBufferSlice>, dst_buffer: &Arc<VkBufferSlice>) -> Arc<VkFence> {
    self.transfer.init_buffer(src_buffer, dst_buffer)
  }

  fn flush_transfers(&self) {
    self.transfer.flush();
  }

  fn free_completed_transfers(&self) {
    self.transfer.try_free_used_buffers();
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
