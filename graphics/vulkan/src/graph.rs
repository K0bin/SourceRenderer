use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::u32;
use std::cmp::{min};

use ash::vk;

use thread_manager::{VkThreadManager, VkFrameLocal};

use sourcerenderer_core::graphics::{CommandBufferType, RenderpassRecordingMode, Format, SampleCount, ExternalResource};
use sourcerenderer_core::graphics::{BufferUsage, InnerCommandBufferProvider, LoadAction, MemoryUsage, RenderGraph, RenderGraphResources, RenderGraphResourceError, RenderPassCallbacks, RenderPassTextureExtent, StoreAction};
use sourcerenderer_core::graphics::RenderGraphInfo;
use sourcerenderer_core::graphics::BACK_BUFFER_ATTACHMENT_NAME;
use sourcerenderer_core::graphics::{Texture, TextureInfo};

use ::{VkRenderPass, VkQueue, VkFence, VkTexture, VkFrameBuffer, VkSemaphore};
use texture::VkTextureView;
use buffer::VkBufferSlice;
use graph_template::{VkRenderGraphTemplate, VkPassType, VkBarrierTemplate, VkResourceTemplate};
use crate::VkBackend;
use crate::raw::RawVkDevice;
use crate::VkSwapchain;
use VkCommandBufferRecorder;
use rayon;

pub enum VkResource {
  Texture {
    texture: Arc<VkTexture>,
    view: Arc<VkTextureView>,
    name: String,
    format: Format,
    samples: SampleCount,
    extent: RenderPassTextureExtent,
    depth: u32,
    levels: u32,
    external: bool,
    load_action: LoadAction,
    store_action: StoreAction,
    stencil_load_action: LoadAction,
    stencil_store_action: StoreAction,
    is_backbuffer: bool
  },
  Buffer {
    buffer: Arc<VkBufferSlice>,
    name: String,
    format: Option<Format>,
    size: u32,
    clear: bool
  },
}

pub struct VkRenderGraph {
  device: Arc<RawVkDevice>,
  passes: Vec<Arc<VkPass>>,
  template: Arc<VkRenderGraphTemplate>,
  resources: HashMap<String, VkResource>,
  thread_manager: Arc<VkThreadManager>,
  swapchain: Arc<VkSwapchain>,
  graphics_queue: Arc<VkQueue>,
  compute_queue: Option<Arc<VkQueue>>,
  transfer_queue: Option<Arc<VkQueue>>,
  renders_to_swapchain: bool,
  info: RenderGraphInfo<VkBackend>,
  external_resources: Option<HashMap<String, ExternalResource<VkBackend>>>
}

pub struct VkRenderGraphResources<'a> {
  resources: &'a HashMap<String, VkResource>,
  external_resources: &'a Option<HashMap<String, ExternalResource<VkBackend>>>,
  pass_resource_names: &'a HashSet<String>,
}

impl<'a> RenderGraphResources<VkBackend> for VkRenderGraphResources<'a> {
  fn get_buffer(&self, name: &str) -> Result<&Arc<VkBufferSlice>, RenderGraphResourceError> {
    let resource = self.resources.get(name);
    if resource.is_none() {
      let external = self.external_resources.as_ref().and_then(|external_resources| external_resources.get(name));
      return if external.is_some() {
        match external.unwrap() {
          ExternalResource::Buffer(buffer) => Ok(buffer),
          _ => Err(RenderGraphResourceError::WrongResourceType)
        }
      } else {
        Err(RenderGraphResourceError::NotFound)
      };
    }
    if !self.pass_resource_names.contains(name) {
      return Err(RenderGraphResourceError::NotAllowed);
    }
    match resource.unwrap() {
      VkResource::Buffer {
        buffer, ..
      } => Ok(buffer),
      _ => Err(RenderGraphResourceError::WrongResourceType)
    }
  }

  fn get_texture(&self, name: &str) -> Result<&Arc<VkTextureView>, RenderGraphResourceError> {
    let resource = self.resources.get(name);
    if resource.is_none() {
      let external = self.external_resources.as_ref().and_then(|external_resources| external_resources.get(name));
      return if external.is_some() {
        match external.unwrap() {
          ExternalResource::Texture(view) => Ok(view),
          _ => Err(RenderGraphResourceError::WrongResourceType)
        }
      } else {
        Err(RenderGraphResourceError::NotFound)
      };
    }
    if !self.pass_resource_names.contains(name) {
      return Err(RenderGraphResourceError::NotAllowed);
    }
    match resource.unwrap() {
      VkResource::Texture {
        view, ..
      } => Ok(view),
      _ => Err(RenderGraphResourceError::WrongResourceType)
    }
  }
}

pub enum VkPass {
  Graphics {
    framebuffers: Vec<Arc<VkFrameBuffer>>,
    renderpass: Arc<VkRenderPass>,
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
    image_barriers: Vec<vk::ImageMemoryBarrier>,
    buffer_barriers: Vec<vk::BufferMemoryBarrier>,
    renders_to_swapchain: bool,
    clear_values: Vec<vk::ClearValue>,
    callbacks: RenderPassCallbacks<VkBackend>,
    resources: HashSet<String>
  },
  Compute {
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
    image_barriers: Vec<vk::ImageMemoryBarrier>,
    buffer_barriers: Vec<vk::BufferMemoryBarrier>,
    callbacks: RenderPassCallbacks<VkBackend>,
    resources: HashSet<String>
  },
  Copy
}

unsafe impl Send for VkPass {}
unsafe impl Sync for VkPass {}

impl VkRenderGraph {
  pub fn new(device: &Arc<RawVkDevice>,
             context: &Arc<VkThreadManager>,
             graphics_queue: &Arc<VkQueue>,
             compute_queue: &Option<Arc<VkQueue>>,
             transfer_queue: &Option<Arc<VkQueue>>,
             template: &Arc<VkRenderGraphTemplate>,
             info: &RenderGraphInfo<VkBackend>,
             swapchain: &Arc<VkSwapchain>,
             external_resources: Option<&HashMap<String, ExternalResource<VkBackend>>>) -> Self {
    let mut resources: HashMap<String, VkResource> = HashMap::new();
    let resource_metadata = template.resources();
    for (_name, attachment_info) in resource_metadata {
      // TODO: aliasing
      match &attachment_info.template {
        // TODO: transient
        VkResourceTemplate::Texture {
          name, extent, format,
          depth, levels, samples,
          external, load_action, store_action,
          stencil_load_action, stencil_store_action, is_backbuffer
        } => {
          if *is_backbuffer {
            continue;
          }

          let mut usage = vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::INPUT_ATTACHMENT
            | vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::TRANSFER_DST;

          if format.is_depth() || format.is_stencil() {
            usage |= vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT;
          } else {
            usage |= vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::STORAGE;
          }

          let (width, height) = match extent {
            RenderPassTextureExtent::RelativeToSwapchain {
              width: output_width, height: output_height
            } => {
              ((swapchain.get_width() as f32 * *output_width) as u32,
               (swapchain.get_height() as f32 * *output_height) as u32)
            },
            RenderPassTextureExtent::Absolute {
              width: output_width, height: output_height
            } => {
              (*output_width,
               *output_height)
            }
          };

          let texture = Arc::new(VkTexture::new(&device, &TextureInfo {
            format: *format,
            width,
            height,
            depth: *depth,
            mip_levels: *levels,
            array_length: 1,
            samples: *samples
          }, Some(name.as_str()), usage));

          let view = Arc::new(VkTextureView::new_attachment_view(device, &texture));
          resources.insert(name.clone(), VkResource::Texture {
            texture,
            view,
            name: name.clone(),
            format: *format,
            samples: *samples,
            extent: extent.clone(),
            depth: *depth,
            levels: *levels,
            external: *external,
            load_action: *load_action,
            store_action: *store_action,
            stencil_load_action: *stencil_load_action,
            stencil_store_action: *stencil_store_action,
            is_backbuffer: false
          });
        }

        VkResourceTemplate::Buffer {
          name, format, size, clear
        } => {
          let allocator = context.get_shared().get_buffer_allocator();
          let buffer = Arc::new(allocator.get_slice(MemoryUsage::GpuOnly, BufferUsage::STORAGE | BufferUsage::CONSTANT | BufferUsage::COPY_DST, *size as usize));
          resources.insert(name.clone(), VkResource::Buffer {
            buffer,
            name: name.clone(),
            format: *format,
            clear: *clear,
            size: *size
          });
        }

        _ => {}
      }
    }

    let mut finished_passes: Vec<Arc<VkPass>> = Vec::new();
    let swapchain_views = swapchain.get_views();
    let passes = template.passes();
    for pass in passes {
      match &pass.pass_type {
        VkPassType::Graphics {
          render_pass, attachments, barriers
        } => {
          let mut clear_values = Vec::<vk::ClearValue>::new();

          let mut width = u32::MAX;
          let mut height = u32::MAX;
          let framebuffer_count = if pass.renders_to_swapchain { swapchain_views.len() } else { 1 };
          let mut framebuffer_attachments: Vec<Vec<vk::ImageView>> = Vec::with_capacity(framebuffer_count);
          for _ in 0..framebuffer_count {
            framebuffer_attachments.push(Vec::new());
          }

          for pass_attachment in attachments {
            if pass_attachment == BACK_BUFFER_ATTACHMENT_NAME {
              clear_values.push(vk::ClearValue {
                color: vk::ClearColorValue {
                  uint32: [0u32; 4]
                }
              });
            } else {
              let resource = resources.get(pass_attachment.as_str()).unwrap();
              let resource_texture = match resource {
                VkResource::Texture { texture, .. } => texture,
                _ => { continue; }
              };
              let format = resource_texture.get_info().format;
              if format.is_depth() || format.is_stencil() {
                clear_values.push(vk::ClearValue {
                  depth_stencil: vk::ClearDepthStencilValue {
                    depth: 9999f32,
                    stencil: 0u32
                  }
                });
              } else {
                clear_values.push(vk::ClearValue {
                  color: vk::ClearColorValue {
                    uint32: [0u32; 4]
                  }
                });
              }
            }

            if pass_attachment == BACK_BUFFER_ATTACHMENT_NAME {
              width = min(width, swapchain.get_width());
              height = min(height, swapchain.get_height());
            } else {
              let resource = resources.get(pass_attachment.as_str()).unwrap();
              let resource_texture = match resource {
                VkResource::Texture { texture, .. } => texture,
                _ => unreachable!()
              };
              let texture_info = resource_texture.get_info();
              width = min(width, texture_info.width);
              height = min(height, texture_info.height);
            }

            for i in 0..framebuffer_count {
              if pass_attachment == BACK_BUFFER_ATTACHMENT_NAME {
                framebuffer_attachments.get_mut(i).unwrap()
                  .push(*swapchain_views[i].get_view_handle());
              } else {
                let resource = resources.get(pass_attachment.as_str()).unwrap();
                let resource_view = match resource {
                  VkResource::Texture { view, .. } => view,
                  _ => unreachable!()
                };
                framebuffer_attachments.get_mut(i).unwrap()
                  .push(*resource_view.get_view_handle());
              }
            }
          }

          if width == u32::MAX || height == u32::MAX {
            panic!("Failed to determine frame buffer dimensions");
          }

          let mut framebuffers: Vec<Arc<VkFrameBuffer>> = Vec::new();
          for fb_attachments in framebuffer_attachments {
            let framebuffer_info = vk::FramebufferCreateInfo {
              render_pass: *render_pass.get_handle(),
              attachment_count: fb_attachments.len() as u32,
              p_attachments: fb_attachments.as_ptr(),
              layers: 1,
              width,
              height,
              ..Default::default()
            };
            let framebuffer = Arc::new(VkFrameBuffer::new(device, &framebuffer_info));
            framebuffers.push(framebuffer);
          }

          let mut src_stage = vk::PipelineStageFlags::empty();
          let mut dst_stage = vk::PipelineStageFlags::empty();
          let mut image_barriers = Vec::<vk::ImageMemoryBarrier>::new();
          let mut buffer_barriers = Vec::<vk::BufferMemoryBarrier>::new();
          for barrier_template in barriers {
            match barrier_template {
              VkBarrierTemplate::Image {
                name, old_layout, new_layout, src_access_mask, dst_access_mask, src_stage: image_src_stage, dst_stage: image_dst_stage, src_queue_family_index, dst_queue_family_index } => {
                src_stage |= *image_src_stage;
                dst_stage |= *image_dst_stage;

                let metadata = resource_metadata.get(name.as_str()).unwrap();
                let is_external = match metadata.template {
                  VkResourceTemplate::Texture { .. } => false,
                  VkResourceTemplate::ExternalTexture { .. } => true,
                  _ => panic!("Mismatched resource type")
                };
                let texture = if !is_external {
                  let resource = resources.get(name.as_str()).unwrap();
                  let resource_texture = match resource {
                    VkResource::Texture { texture, .. } => texture,
                    _ => unreachable!()
                  };
                  resource_texture
                } else {
                  let resource = external_resources
                    .expect(format!("Can't find resource {}", name).as_str())
                    .get(name.as_str()).unwrap();
                  let resource_view = match resource {
                    ExternalResource::Texture(view) => view,
                    _ => unreachable!()
                  };
                  resource_view.texture()
                };

                image_barriers.push(vk::ImageMemoryBarrier {
                  src_access_mask: *src_access_mask,
                  dst_access_mask: *dst_access_mask,
                  old_layout: *old_layout,
                  new_layout: *new_layout,
                  src_queue_family_index: *src_queue_family_index,
                  dst_queue_family_index: *dst_queue_family_index,
                  image: *texture.get_handle(),
                  subresource_range: vk::ImageSubresourceRange {
                    aspect_mask: if texture.get_info().format.is_depth() && texture.get_info().format.is_stencil() {
                      vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
                    } else if texture.get_info().format.is_depth() {
                      vk::ImageAspectFlags::DEPTH
                    } else {
                      vk::ImageAspectFlags::COLOR
                    },
                    base_mip_level: 0,
                    level_count: texture.get_info().mip_levels,
                    base_array_layer: 0,
                    layer_count: texture.get_info().array_length
                  },
                  ..Default::default()
                });
              }
              VkBarrierTemplate::Buffer {
                name, src_access_mask, dst_access_mask, src_stage: buffer_src_stage, dst_stage: buffer_dst_stage, src_queue_family_index, dst_queue_family_index } => {
                src_stage |= *buffer_src_stage;
                dst_stage |= *buffer_dst_stage;

                let metadata = resource_metadata.get(name.as_str()).unwrap();
                let is_external = match metadata.template {
                  VkResourceTemplate::Buffer { .. } => false,
                  VkResourceTemplate::ExternalBuffer { .. } => true,
                  _ => panic!("Mismatched resource type")
                };
                let buffer = if !is_external {
                  let resource = resources.get(name.as_str()).unwrap();
                  let resource_buffer = match resource {
                    VkResource::Buffer { buffer, .. } => buffer,
                    _ => unreachable!()
                  };
                  resource_buffer
                } else {
                  let resource = external_resources
                    .expect(format!("Can't find resource {}", name).as_str())
                    .get(name.as_str()).unwrap();
                  let resource_buffer = match resource {
                    ExternalResource::Buffer(buffer) => buffer,
                    _ => unreachable!()
                  };
                  resource_buffer
                };
                let (offset, length) = buffer.get_offset_and_length();
                buffer_barriers.push(vk::BufferMemoryBarrier {
                  src_access_mask: *src_access_mask,
                  dst_access_mask: *dst_access_mask,
                  src_queue_family_index: *src_queue_family_index,
                  dst_queue_family_index: *dst_queue_family_index,
                  buffer: *buffer.get_buffer().get_handle(),
                  offset: offset as u64,
                  size: length as u64,
                  ..Default::default()
                });
              }
            }
          }

          let callbacks: RenderPassCallbacks<VkBackend> = info.pass_callbacks[&pass.name].clone();

          finished_passes.push(Arc::new(VkPass::Graphics {
            framebuffers,
            src_stage,
            dst_stage,
            image_barriers,
            buffer_barriers,
            callbacks,
            renders_to_swapchain: pass.renders_to_swapchain,
            renderpass: render_pass.clone(),
            clear_values,
            resources: pass.resources.clone()
          }));
        },

        VkPassType::Compute {
          barriers
        } => {
          let mut src_stage = vk::PipelineStageFlags::empty();
          let mut dst_stage = vk::PipelineStageFlags::empty();
          let mut image_barriers = Vec::<vk::ImageMemoryBarrier>::new();
          let mut buffer_barriers = Vec::<vk::BufferMemoryBarrier>::new();
          for barrier_template in barriers {
            match barrier_template {
              VkBarrierTemplate::Image {
                name, old_layout, new_layout, src_access_mask, dst_access_mask, src_stage: image_src_stage, dst_stage: image_dst_stage, src_queue_family_index, dst_queue_family_index } => {
                src_stage |= *image_src_stage;
                dst_stage |= *image_dst_stage;

                let metadata = resource_metadata.get(name.as_str()).unwrap();
                let is_external = match metadata.template {
                  VkResourceTemplate::Texture { .. } => false,
                  VkResourceTemplate::ExternalTexture { .. } => true,
                  _ => panic!("Mismatched resource type")
                };
                let texture = if !is_external {
                  let resource = resources.get(name.as_str()).unwrap();
                  let resource_texture = match resource {
                    VkResource::Texture { texture, .. } => texture,
                    _ => unreachable!()
                  };
                  resource_texture
                } else {
                  let resource = external_resources
                    .expect(format!("Can't find resource {}", name).as_str())
                    .get(name.as_str()).unwrap();
                  let resource_view = match resource {
                    ExternalResource::Texture(view) => view,
                    _ => unreachable!()
                  };
                  resource_view.texture()
                };

                image_barriers.push(vk::ImageMemoryBarrier {
                  src_access_mask: *src_access_mask,
                  dst_access_mask: *dst_access_mask,
                  old_layout: *old_layout,
                  new_layout: *new_layout,
                  src_queue_family_index: *src_queue_family_index,
                  dst_queue_family_index: *dst_queue_family_index,
                  image: *texture.get_handle(),
                  subresource_range: vk::ImageSubresourceRange {
                    aspect_mask: if texture.get_info().format.is_depth() && texture.get_info().format.is_stencil() {
                      vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
                    } else if texture.get_info().format.is_depth() {
                      vk::ImageAspectFlags::DEPTH
                    } else {
                      vk::ImageAspectFlags::COLOR
                    },
                    base_mip_level: 0,
                    level_count: texture.get_info().mip_levels,
                    base_array_layer: 0,
                    layer_count: texture.get_info().array_length
                  },
                  ..Default::default()
                });
              }
              VkBarrierTemplate::Buffer {
                name, src_access_mask, dst_access_mask, src_stage: buffer_src_stage, dst_stage: buffer_dst_stage, src_queue_family_index, dst_queue_family_index } => {
                src_stage |= *buffer_src_stage;
                dst_stage |= *buffer_dst_stage;
                let metadata = resource_metadata.get(name.as_str()).unwrap();
                let is_external = match metadata.template {
                  VkResourceTemplate::Buffer { .. } => false,
                  VkResourceTemplate::ExternalBuffer { .. } => true,
                  _ => panic!("Mismatched resource type")
                };
                let buffer = if !is_external {
                  let resource = resources.get(name.as_str()).unwrap();
                  let resource_buffer = match resource {
                    VkResource::Buffer { buffer, .. } => buffer,
                    _ => unreachable!()
                  };
                  resource_buffer
                } else {
                  let resource = external_resources
                    .expect(format!("Can't find resource {}", name).as_str())
                    .get(name.as_str()).unwrap();
                  let resource_buffer = match resource {
                    ExternalResource::Buffer(buffer) => buffer,
                    _ => unreachable!()
                  };
                  resource_buffer
                };
                let (offset, length) = buffer.get_offset_and_length();
                buffer_barriers.push(vk::BufferMemoryBarrier {
                  src_access_mask: *src_access_mask,
                  dst_access_mask: *dst_access_mask,
                  src_queue_family_index: *src_queue_family_index,
                  dst_queue_family_index: *dst_queue_family_index,
                  buffer: *buffer.get_buffer().get_handle(),
                  offset: offset as u64,
                  size: length as u64,
                  ..Default::default()
                });
              }
            }
          }

          let callbacks: RenderPassCallbacks<VkBackend> = info.pass_callbacks[&pass.name].clone();

          finished_passes.push(Arc::new(VkPass::Compute {
            src_stage,
            dst_stage,
            image_barriers,
            buffer_barriers,
            callbacks,
            resources: pass.resources.clone()
          }))
        },

        _ => unimplemented!()
      }
    }

    Self {
      device: device.clone(),
      template: template.clone(),
      passes: finished_passes,
      resources,
      thread_manager: context.clone(),
      swapchain: swapchain.clone(),
      graphics_queue: graphics_queue.clone(),
      compute_queue: compute_queue.clone(),
      transfer_queue: transfer_queue.clone(),
      renders_to_swapchain: template.renders_to_swapchain(),
      info: info.clone(),
      external_resources: external_resources.map(|external_resources| external_resources.clone())
    }
  }

  fn execute_cmd_buffer(&self,
                        cmd_buffer: &mut VkCommandBufferRecorder,
                        frame_local: &mut VkFrameLocal,
                        fence: Option<&VkFence>,
                        wait_semaphores: &[&VkSemaphore],
                        signal_semaphore: &[&VkSemaphore]) {
    let finished_cmd_buffer = std::mem::replace(cmd_buffer, frame_local.get_command_buffer(CommandBufferType::PRIMARY));
    self.graphics_queue.submit(finished_cmd_buffer.finish(), fence, wait_semaphores, signal_semaphore);
    let c_queue = self.graphics_queue.clone();
    rayon::spawn(move || c_queue.process_submissions());
  }
}

impl RenderGraph<VkBackend> for VkRenderGraph {
  fn recreate(old: &Self, swapchain: &Arc<VkSwapchain>) -> Self {
    VkRenderGraph::new(&old.device, &old.thread_manager, &old.graphics_queue, &old.compute_queue, &old.transfer_queue, &old.template, &old.info, swapchain, old.external_resources.as_ref())
  }

  fn render(&mut self) -> Result<(), ()> {
    self.thread_manager.begin_frame();

    let prepare_semaphore = self.thread_manager.get_shared().get_semaphore();
    let cmd_semaphore = self.thread_manager.get_shared().get_semaphore();
    let cmd_fence = self.thread_manager.get_shared().get_fence();
    let mut image_index: u32 = 0;

    if self.renders_to_swapchain {
      let result = self.swapchain.prepare_back_buffer(&prepare_semaphore);
      if result.is_err() || !result.unwrap().1 && false {
        return Err(())
      }
      let (index, _) = result.unwrap();
      image_index = index
    }

    let framebuffer_index = image_index as usize;
    for pass in &self.passes {
      let mut thread_local = self.thread_manager.get_thread_local();
      let mut frame_local = thread_local.get_frame_local();
      let mut cmd_buffer = frame_local.get_command_buffer(CommandBufferType::PRIMARY);

      match pass as &VkPass {
        VkPass::Graphics {
          framebuffers,
          src_stage,
          dst_stage,
          image_barriers,
          buffer_barriers,
          callbacks,
          renderpass,
          renders_to_swapchain,
          clear_values,
          resources: pass_resource_names
        } => {
          let graph_resources = VkRenderGraphResources {
            resources: &self.resources,
            external_resources: &self.external_resources,
            pass_resource_names
          };
          let graph_resources_ref: &'static VkRenderGraphResources = unsafe { std::mem::transmute(&graph_resources) };

          if *src_stage != vk::PipelineStageFlags::empty() || !buffer_barriers.is_empty() || !image_barriers.is_empty() {
            cmd_buffer.barrier(*src_stage, *dst_stage, vk::DependencyFlags::empty(),
                               &[], buffer_barriers, image_barriers);
          }
          match callbacks {
            RenderPassCallbacks::Regular(callbacks) => {
              cmd_buffer.begin_render_pass(&renderpass, &framebuffers[if *renders_to_swapchain { framebuffer_index } else { 0 }], &clear_values, RenderpassRecordingMode::Commands);
              for i in 0..callbacks.len() {
                if i != 0 {
                  cmd_buffer.advance_subpass();
                }
                let callback = &callbacks[i];
                (callback)(&mut cmd_buffer, graph_resources_ref);
              }
              cmd_buffer.end_render_pass();
            }
            RenderPassCallbacks::InternallyThreaded(callbacks) => {
              cmd_buffer.begin_render_pass(&renderpass, &framebuffers[if *renders_to_swapchain { framebuffer_index } else { 0 }], &clear_values, RenderpassRecordingMode::CommandBuffers);
              let provider = self.thread_manager.clone() as Arc<dyn InnerCommandBufferProvider<VkBackend>>;
              for i in 0..callbacks.len() {
                if i != 0 {
                  cmd_buffer.advance_subpass();
                }
                let callback = &callbacks[i];
                let inner_cmd_buffers = (callback)(&provider, graph_resources_ref);
                for inner_cmd_buffer in inner_cmd_buffers {
                  cmd_buffer.execute_inner_command_buffer(inner_cmd_buffer);
                }
              }
              cmd_buffer.end_render_pass();
            }
          }
          let prepare_semaphores = [&**prepare_semaphore];
          let cmd_semaphores = [&**cmd_semaphore];

          let wait_semaphores: &[&VkSemaphore] = if *renders_to_swapchain {
            &prepare_semaphores
          } else {
            &[]
          };
          let signal_semaphores: &[&VkSemaphore] = if *renders_to_swapchain {
            &cmd_semaphores
          } else {
            &[]
          };

          let fence = if *renders_to_swapchain {
            Some(&cmd_fence as &VkFence)
          } else {
            None
          };

          self.execute_cmd_buffer(&mut cmd_buffer, &mut frame_local, fence, &wait_semaphores, &signal_semaphores);
          if *renders_to_swapchain {
            frame_local.track_semaphore(&prepare_semaphore);
          }
        }

        VkPass::Compute {
          src_stage,
          dst_stage,
          buffer_barriers,
          image_barriers,
          callbacks,
          resources: pass_resource_names
        } => {
          let graph_resources = VkRenderGraphResources {
            resources: &self.resources,
            external_resources: &self.external_resources,
            pass_resource_names
          };
          let graph_resources_ref: &'static VkRenderGraphResources = unsafe { std::mem::transmute(&graph_resources) };

          if *src_stage != vk::PipelineStageFlags::empty() || !buffer_barriers.is_empty() || !image_barriers.is_empty() {
            cmd_buffer.barrier(*src_stage, *dst_stage, vk::DependencyFlags::empty(),
              &[], buffer_barriers, image_barriers);
          }
          match callbacks {
            RenderPassCallbacks::Regular(callbacks) => {
              for callback in callbacks {
                (callback)(&mut cmd_buffer, graph_resources_ref);
              }
            }
            RenderPassCallbacks::InternallyThreaded(callbacks) => {
              let provider = self.thread_manager.clone() as Arc<dyn InnerCommandBufferProvider<VkBackend>>;
              let callback = &callbacks[0];
              let inner_cmd_buffers = (callback)(&provider, graph_resources_ref);
              for inner_cmd_buffer in inner_cmd_buffers {
                cmd_buffer.execute_inner_command_buffer(inner_cmd_buffer);
              }
            }
          }

          self.execute_cmd_buffer(&mut cmd_buffer, &mut frame_local, None, &[], &[]);
        }

        VkPass::Copy => {}
      }
    }

    let mut thread_context = self.thread_manager.get_thread_local();
    let mut frame_context = thread_context.get_frame_local();

    if self.renders_to_swapchain {
      let result = self.graphics_queue.present(&self.swapchain, image_index, &[&cmd_semaphore]);
      if result.is_err() || !result.unwrap() && false {
        return Err(());
      }

      frame_context.track_semaphore(&cmd_semaphore);
    }

    self.thread_manager.end_frame(&cmd_fence);
    Ok(())
  }
}

fn store_action_to_vk(store_action: StoreAction) -> vk::AttachmentStoreOp {
  match store_action {
    StoreAction::DontCare => vk::AttachmentStoreOp::DONT_CARE,
    StoreAction::Store => vk::AttachmentStoreOp::STORE
  }
}

fn load_action_to_vk(load_action: LoadAction) -> vk::AttachmentLoadOp {
  match load_action {
    LoadAction::DontCare => vk::AttachmentLoadOp::DONT_CARE,
    LoadAction::Load => vk::AttachmentLoadOp::LOAD,
    LoadAction::Clear => vk::AttachmentLoadOp::CLEAR
  }
}
