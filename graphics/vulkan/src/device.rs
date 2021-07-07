use std::sync::{Arc};

use ash::vk;
use ash::version::{DeviceV1_0};

use sourcerenderer_core::graphics::*;
use crate::{queue::VkQueue, texture::VkSampler};
use crate::queue::VkQueueInfo;
use crate::{VkBackend, VkRenderPass};
use crate::VkAdapterExtensionSupport;
use crate::pipeline::VkPipeline;
use crate::pipeline::VkShader;
use crate::texture::VkTexture;
use crate::sync::VkFence;
use crate::graph::VkRenderGraph;

use crate::{VkThreadManager, VkShared};
use crate::raw::{RawVkDevice, RawVkInstance};
use crate::pipeline::VkGraphicsPipelineInfo;
use crate::buffer::VkBufferSlice;
use std::cmp::min;
use crate::texture::VkTextureView;
use crate::transfer::VkTransfer;
use crate::graph_template::{VkRenderGraphTemplate, VkPassType};
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
    extensions: VkAdapterExtensionSupport,
    max_surface_image_count: u32) -> Self {

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
      extensions,
      graphics_queue_info,
      transfer_queue_info,
      compute_queue_info
    });

    let shared = Arc::new(VkShared::new(&raw));

    let graphics_queue = {
      let vk_queue = unsafe { raw.device.get_device_queue(graphics_queue_info.queue_family_index as u32, graphics_queue_info.queue_index as u32) };
      Arc::new(VkQueue::new(graphics_queue_info, vk_queue, &raw, &shared))
    };

    let compute_queue = compute_queue_info.map(|info| {
      let vk_queue = unsafe { raw.device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32) };
      Arc::new(VkQueue::new(info, vk_queue, &raw, &shared))
    });

    let transfer_queue = transfer_queue_info.map(|info| {
      let vk_queue = unsafe { raw.device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32) };
      Arc::new(VkQueue::new(info, vk_queue, &raw, &shared))
    });

    let context = Arc::new(VkThreadManager::new(&raw, &graphics_queue, &compute_queue, &transfer_queue, &shared, min(3, max_surface_image_count)));

    let transfer = VkTransfer::new(&raw, &graphics_queue, &transfer_queue, &shared);

    VkDevice {
      device: raw,
      graphics_queue,
      compute_queue,
      transfer_queue,
      extensions,
      context,
      transfer
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
  fn create_buffer(&self, length: usize, memory_usage: MemoryUsage, usage: BufferUsage, name: Option<&str>) -> Arc<VkBufferSlice> {
    self.context.get_shared().get_buffer_allocator().get_slice(memory_usage, usage, length, name)
  }

  fn upload_data<T>(&self, data: &[T], memory_usage: MemoryUsage, usage: BufferUsage) -> Arc<VkBufferSlice> where T: 'static + Send + Sync + Sized + Clone {
    assert_ne!(memory_usage, MemoryUsage::GpuOnly);
    let slice = self.context.get_shared().get_buffer_allocator().get_slice(memory_usage, usage, std::mem::size_of_val(data), None);
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
    Arc::new(VkTexture::new(&self.device, info, name, vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST))
  }

  fn create_shader_resource_view(&self, texture: &Arc<VkTexture>, info: &TextureShaderResourceViewInfo) -> Arc<VkTextureView> {
    Arc::new(VkTextureView::new_shader_resource_view(&self.device, texture, info))
  }

  fn create_graphics_pipeline(&self, info: &GraphicsPipelineInfo<VkBackend>, graph_template: &<VkBackend as Backend>::RenderGraphTemplate, pass_name: &str, subpass_index: u32) -> Arc<<VkBackend as Backend>::GraphicsPipeline> {
    let pass = graph_template.passes.iter().find(|pass| pass.name.as_str() == pass_name).unwrap_or_else(|| panic!("Can not find pass {} in render graph.", pass_name));
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

  fn create_sampler(&self, info: &SamplerInfo) -> Arc<VkSampler> {
    Arc::new(VkSampler::new(&self.device, info))
  }

  fn create_compute_pipeline(&self, shader: &Arc<VkShader>) -> Arc<VkPipeline> {
    Arc::new(VkPipeline::new_compute(&self.device, shader, self.context.shared()))
  }

  fn wait_for_idle(&self) {
    let c_graphics_queue = self.graphics_queue.clone();
    let graphics_join = std::thread::spawn(move || c_graphics_queue.wait_for_idle() );

    let transfer_join = self.transfer_queue.as_ref().map(|q| {
      let c_queue = q.clone();
      std::thread::spawn(move || c_queue.wait_for_idle())
    });

    let compute_join = self.compute_queue.as_ref().map(|q| {
      let c_queue = q.clone();
      std::thread::spawn(move || c_queue.wait_for_idle())
    });

    graphics_join.join().unwrap();
    if let Some(join) = transfer_join {
      join.join().unwrap();
    }
    if let Some(join) = compute_join {
      join.join().unwrap();
    }
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


  fn create_graphics_pipeline_1(&self, info: &GraphicsPipelineInfo<VkBackend>, renderpass_info: &RenderPassInfo, subpass: u32) -> Arc<<VkBackend as Backend>::GraphicsPipeline> {
    let shared = self.context.get_shared();
    let mut rp_opt = {
      let render_passes = shared.get_render_passes().read().unwrap();
      render_passes.get(renderpass_info).map(|rp_ref| rp_ref.clone())
    };
    if rp_opt.is_none() {
      let rp = Arc::new(VkRenderPass::new_pipeline_compat(&self.device, renderpass_info));
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

  fn init_texture(&self, texture: &Arc<VkTexture>, buffer: &Arc<VkBufferSlice>, mip_level: u32, array_layer: u32) {
    self.transfer.init_texture(texture, buffer, mip_level, array_layer);
  }

  fn init_texture_async(&self, texture: &Arc<VkTexture>, buffer: &Arc<VkBufferSlice>, mip_level: u32, array_layer: u32) -> Option<Arc<VkFence>> {
    self.transfer.init_texture_async(texture, buffer, mip_level, array_layer)
  }

  fn init_buffer(&self, src_buffer: &Arc<VkBufferSlice>, dst_buffer: &Arc<VkBufferSlice>) {
    self.transfer.init_buffer(src_buffer, dst_buffer);
  }

  fn flush_transfers(&self) {
    self.transfer.flush();
  }

  fn free_completed_transfers(&self) {
    self.transfer.try_free_used_buffers();
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
