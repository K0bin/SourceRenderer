use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::u32;
use std::cmp::min;

use ash::vk;
use smallvec::SmallVec;
use sourcerenderer_core::graphics::Barrier;
use sourcerenderer_core::graphics::TextureUsage;

use crate::{command::VkInnerCommandBufferInfo, thread_manager::{VkThreadManager, VkFrameLocal}};

use sourcerenderer_core::graphics::{RenderpassRecordingMode, Format, SampleCount, ExternalResource, TextureDimensions, SwapchainError, Swapchain, BufferInfo};
use sourcerenderer_core::graphics::{BufferUsage, InnerCommandBufferProvider, MemoryUsage, RenderGraph, RenderGraphResources, RenderGraphResourceError, RenderPassCallbacks, RenderPassTextureExtent, CommandBuffer};
use sourcerenderer_core::graphics::RenderGraphInfo;
use sourcerenderer_core::graphics::BACK_BUFFER_ATTACHMENT_NAME;
use sourcerenderer_core::graphics::{Texture, TextureInfo};

use crate::{VkRenderPass, VkQueue, VkFence, VkTexture, VkFrameBuffer, VkSemaphore};
use crate::texture::VkTextureView;
use crate::buffer::VkBufferSlice;
use crate::graph_template::{VkRenderGraphTemplate, VkPassType, VkBarrierTemplate, VkResourceTemplate};
use crate::VkBackend;
use crate::raw::RawVkDevice;
use crate::VkSwapchain;
use crate::VkCommandBufferRecorder;
use rayon;
use crate::swapchain::VkSwapchainState;
use sourcerenderer_core::Matrix4;

pub enum VkResource {
  Texture {
    texture: Arc<VkTexture>,
    texture_b: Option<Arc<VkTexture>>,
    view: Arc<VkTextureView>,
    view_b: Option<Arc<VkTextureView>>,
    name: String,
    format: Format,
    samples: SampleCount,
    extent: RenderPassTextureExtent,
    depth: u32,
    levels: u32,
    external: bool,
    is_backbuffer: bool
  },
  Buffer {
    buffer: Arc<VkBufferSlice>,
    buffer_b: Option<Arc<VkBufferSlice>>,
    name: String,
    format: Option<Format>,
    size: u32,
    clear: bool
  },
}

pub struct VkRenderGraph {
  device: Arc<RawVkDevice>,
  passes: Vec<VkPass>,
  template: Arc<VkRenderGraphTemplate>,
  resources: HashMap<String, VkResource>,
  thread_manager: Arc<VkThreadManager>,
  swapchain: Arc<VkSwapchain>,
  graphics_queue: Arc<VkQueue>,
  compute_queue: Option<Arc<VkQueue>>,
  transfer_queue: Option<Arc<VkQueue>>,
  renders_to_swapchain: bool,
  info: RenderGraphInfo<VkBackend>,
  external_resources: HashMap<String, ExternalResource<VkBackend>>,
  fb_cache: HashMap<SmallVec<[Arc<VkTextureView>; 8]>, Arc<VkFrameBuffer>>
}

pub struct VkCommandBufferProvider {
  inner_info: Option<VkInnerCommandBufferInfo>,
  thread_manager: Arc<VkThreadManager>
}

impl InnerCommandBufferProvider<VkBackend> for VkCommandBufferProvider {
  fn get_inner_command_buffer(&self) -> VkCommandBufferRecorder {
    let thread_local = self.thread_manager.get_thread_local();
    let frame_local = thread_local.get_frame_local();
    frame_local.get_inner_command_buffer(self.inner_info.as_ref())
  }
}

pub struct VkRenderGraphResources<'a> {
  resources: &'a HashMap<String, VkResource>,
  external_resources: &'a HashMap<String, ExternalResource<VkBackend>>,
  pass_resource_names: &'a HashSet<String>,
  swapchain: &'a VkSwapchain,
  swapchain_image_index: u32
}

impl<'a> VkRenderGraphResources<'a> {
  fn get_texture_view(&self, name: &str, history: bool) -> Result<&Arc<VkTextureView>, RenderGraphResourceError> {
    if !self.pass_resource_names.contains(name) {
      return Err(RenderGraphResourceError::NotAllowed);
    }

    if name == BACK_BUFFER_ATTACHMENT_NAME {
      return Ok(&self.swapchain.get_views()[self.swapchain_image_index as usize]);
    }

    let resource = self.resources.get(name);
    if resource.is_none() {
      let external = self.external_resources.get(name);
      return if let Some(external) = external {
        match external {
          ExternalResource::Texture(view) => Ok(view),
          _ => Err(RenderGraphResourceError::WrongResourceType)
        }
      } else {
        Err(RenderGraphResourceError::NotFound)
      };
    }
    match resource.unwrap() {
      VkResource::Texture {
        view, view_b, ..
      } => {
        if !history {
          Ok(view)
        } else if let Some(view_b) = view_b {
          Ok(view_b)
        } else {
          Err(RenderGraphResourceError::NoHistory)
        }
      },
      _ => Err(RenderGraphResourceError::WrongResourceType)
    }
  }
}

impl<'a> RenderGraphResources<VkBackend> for VkRenderGraphResources<'a> {
  fn get_buffer(&self, name: &str, history: bool) -> Result<&Arc<VkBufferSlice>, RenderGraphResourceError> {
    if !self.pass_resource_names.contains(name) {
      return Err(RenderGraphResourceError::NotAllowed);
    }
    let resource = self.resources.get(name);
    if resource.is_none() {
      let external = self.external_resources.get(name);
      return if let Some(external) = external {
        match external {
          ExternalResource::Buffer(buffer) => Ok(buffer),
          _ => Err(RenderGraphResourceError::WrongResourceType)
        }
      } else {
        Err(RenderGraphResourceError::NotFound)
      };
    }
    match resource.unwrap() {
      VkResource::Buffer {
        buffer, buffer_b, ..
      } => {
        if !history {
          Ok(buffer)
        } else if let Some(buffer_b) = buffer_b {
          Ok(buffer_b)
        } else {
          Err(RenderGraphResourceError::NoHistory)
        }
      },
      _ => Err(RenderGraphResourceError::WrongResourceType)
    }
  }

  fn texture_dimensions(&self, name: &str) -> Result<TextureDimensions, RenderGraphResourceError> {
    if name == BACK_BUFFER_ATTACHMENT_NAME {
      return Ok(TextureDimensions {
        width: self.swapchain.get_width(),
        height: self.swapchain.get_height(),
        depth: 1,
        array_count: 1,
        mip_levels: 1
      });
    };

    let resource = self.resources.get(name);
    if resource.is_none() {
      let external = self.external_resources.get(name);
      return if let Some(external) = external {
        match external {
          ExternalResource::Texture(view) => {
            let info = view.texture().get_info();
            Ok(TextureDimensions {
              width: info.width,
              height: info.height,
              depth: info.depth,
              array_count: info.array_length,
              mip_levels: info.mip_levels
            })
          },
          _ => Err(RenderGraphResourceError::WrongResourceType)
        }
      } else {
        Err(RenderGraphResourceError::NotFound)
      };
    }
    match resource.unwrap() {
      VkResource::Texture {
        view, ..
      } => {
        let info = view.texture().get_info();
        Ok(TextureDimensions {
          width: info.width,
          height: info.height,
          depth: info.depth,
          array_count: info.array_length,
          mip_levels: info.mip_levels
        })
      },
      _ => Err(RenderGraphResourceError::WrongResourceType)
    }
  }

  fn swapchain_transform(&self) -> &Matrix4 {
    self.swapchain.transform()
  }

  fn get_texture_srv(&self, name: &str, history: bool) -> Result<&Arc<VkTextureView>, RenderGraphResourceError> {
    self.get_texture_view(name, history)
  }

  fn get_texture_uav(&self, name: &str, history: bool) -> Result<&Arc<VkTextureView>, RenderGraphResourceError> {
    self.get_texture_view(name, history)
  }
}

pub enum VkPass {
  Graphics {
    name: String,
    renderpass: Arc<VkRenderPass>,
    renders_to_swapchain: bool,
    callbacks: RenderPassCallbacks<VkBackend>,
    resources: HashSet<String>,
  },
  ComputeCopy {
    name: String,
    callbacks: RenderPassCallbacks<VkBackend>,
    resources: HashSet<String>,
    renders_to_swapchain: bool
  }
}

unsafe impl Send for VkPass {}
unsafe impl Sync for VkPass {}

impl VkRenderGraph {
  #[allow(unused_assignments, unused_variables)] // TODO
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
    for attachment_info in resource_metadata.values() {
      let has_history_resource = if let Some(history_usage) = attachment_info.history.as_ref() {
        history_usage.pass_range.first_used_in_pass_index >= attachment_info.pass_range.first_used_in_pass_index
      } else {
        false
      };
      // TODO: aliasing
      match &attachment_info.template {
        // TODO: transient
        VkResourceTemplate::Texture {
          name, extent, format,
          depth, levels, samples,
          external, is_backbuffer
        } => {
          if *is_backbuffer {
            continue;
          }

          let mut usage = TextureUsage::VERTEX_SHADER_SAMPLED | TextureUsage::FRAGMENT_SHADER_SAMPLED | TextureUsage::COMPUTE_SHADER_SAMPLED | TextureUsage::FRAGMENT_SHADER_LOCAL
              | TextureUsage::COPY_SRC | TextureUsage::COPY_SRC;

          if format.is_depth() || format.is_stencil() {
            usage |= TextureUsage::DEPTH_READ | TextureUsage::DEPTH_WRITE;
          } else {
            usage |= TextureUsage::RENDER_TARGET | TextureUsage::VERTEX_SHADER_STORAGE_WRITE | TextureUsage::FRAGMENT_SHADER_STORAGE_WRITE | TextureUsage::COMPUTE_SHADER_STORAGE_WRITE
              |  TextureUsage::VERTEX_SHADER_STORAGE_READ | TextureUsage::FRAGMENT_SHADER_STORAGE_READ | TextureUsage::COMPUTE_SHADER_STORAGE_READ;
          }

          let (width, height) = match extent {
            RenderPassTextureExtent::RelativeToSwapchain {
              width: output_width, height: output_height
            } => {
              ((swapchain.get_width() as f32 * output_width) as u32,
               (swapchain.get_height() as f32 * output_height) as u32)
            },
            RenderPassTextureExtent::Absolute {
              width: output_width, height: output_height
            } => {
              (*output_width,
               *output_height)
            }
          };

          let texture_info = TextureInfo {
            format: *format,
            width,
            height,
            depth: *depth,
            mip_levels: *levels,
            array_length: 1,
            samples: *samples,
            usage
          };

          let texture = Arc::new(VkTexture::new(&device, &texture_info, Some(name.as_str())));
          let view = Arc::new(VkTextureView::new_attachment_view(device, &texture));

          let (texture_b, view_b) = if has_history_resource {
            let texture = Arc::new(VkTexture::new(&device, &texture_info, Some((name.clone() + "B").as_str())));
            let view = Arc::new(VkTextureView::new_attachment_view(device, &texture));
            (Some(texture), Some(view))
          } else {
            (None, None)
          };

          resources.insert(name.clone(), VkResource::Texture {
            texture,
            view,
            texture_b,
            view_b,
            name: name.clone(),
            format: *format,
            samples: *samples,
            extent: extent.clone(),
            depth: *depth,
            levels: *levels,
            external: *external,
            is_backbuffer: false
          });
        }

        VkResourceTemplate::Buffer {
          name, format, size, clear
        } => {
          let allocator = context.get_shared().get_buffer_allocator();
          let buffer = allocator.get_slice(&BufferInfo {
            size: *size as usize,
            usage: BufferUsage::FRAGMENT_SHADER_STORAGE_READ | BufferUsage::FRAGMENT_SHADER_STORAGE_WRITE | BufferUsage::VERTEX_SHADER_STORAGE_READ | BufferUsage::VERTEX_SHADER_STORAGE_WRITE | BufferUsage::COMPUTE_SHADER_STORAGE_READ | BufferUsage::COMPUTE_SHADER_STORAGE_WRITE | BufferUsage::VERTEX_SHADER_CONSTANT | BufferUsage::FRAGMENT_SHADER_CONSTANT | BufferUsage::COMPUTE_SHADER_CONSTANT | BufferUsage::COPY_DST | BufferUsage::COPY_SRC
          }, MemoryUsage::GpuOnly, Some(name));
          let buffer_b = if has_history_resource {
            Some(allocator.get_slice(&BufferInfo {
              size: *size as usize,
              usage: BufferUsage::FRAGMENT_SHADER_STORAGE_READ | BufferUsage::FRAGMENT_SHADER_STORAGE_WRITE | BufferUsage::VERTEX_SHADER_STORAGE_READ | BufferUsage::VERTEX_SHADER_STORAGE_WRITE | BufferUsage::COMPUTE_SHADER_STORAGE_READ | BufferUsage::COMPUTE_SHADER_STORAGE_WRITE | BufferUsage::VERTEX_SHADER_CONSTANT | BufferUsage::FRAGMENT_SHADER_CONSTANT | BufferUsage::COMPUTE_SHADER_CONSTANT | BufferUsage::COPY_DST | BufferUsage::COPY_SRC,
            }, MemoryUsage::GpuOnly, Some(name)))
          } else {
            None
          };
          resources.insert(name.clone(), VkResource::Buffer {
            buffer,
            buffer_b,
            name: name.clone(),
            format: *format,
            clear: *clear,
            size: *size
          });
        }

        _ => {}
      }
    }

    let mut finished_passes: Vec<VkPass> = Vec::new();
    let swapchain_views = swapchain.get_views();
    let passes = template.passes();
    for pass in passes {
      match &pass.pass_type {
        VkPassType::Graphics {
          render_pass, attachments
        } => {
          let mut clear_values = Vec::<vk::ClearValue>::new();

          let mut width = u32::MAX;
          let mut height = u32::MAX;

          for pass_attachment in attachments {
            if pass_attachment == BACK_BUFFER_ATTACHMENT_NAME {
              clear_values.push(vk::ClearValue {
                color: vk::ClearColorValue {
                  float32: [0f32; 4]
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
                    depth: 1f32,
                    stencil: 0u32
                  }
                });
              } else {
                clear_values.push(vk::ClearValue {
                  color: vk::ClearColorValue {
                    float32: [0f32; 4]
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
          }

          if width == u32::MAX || height == u32::MAX {
            panic!("Failed to determine frame buffer dimensions");
          }

          let callbacks: RenderPassCallbacks<VkBackend> = info.pass_callbacks[&pass.name].clone();

          let index = finished_passes.len();
          finished_passes.push(VkPass::Graphics {
            name: pass.name.clone(),
            callbacks,
            resources: pass.resources.clone(),
            renders_to_swapchain: pass.renders_to_swapchain,
            renderpass: render_pass.clone(),
          });
        },

        VkPassType::ComputeCopy {
          is_compute: _
        } => {
          let callbacks: RenderPassCallbacks<VkBackend> = info.pass_callbacks[&pass.name].clone();

          let index = finished_passes.len();
          finished_passes.push(VkPass::ComputeCopy {
            name: pass.name.clone(),
            callbacks,
            resources: pass.resources.clone(),
            renders_to_swapchain: pass.renders_to_swapchain
          })
        }
      }
    }

    // Initial transition for history images
    // TODO: clear them
    let mut initialized_history_resources = HashSet::<String>::new();
    let mut image_barriers = Vec::<Barrier<VkBackend>>::new();

    for (name, metadata) in &template.resources {
      if metadata.history.is_none() || initialized_history_resources.contains(name) {
        continue;
      }

      let history = metadata.history.as_ref().unwrap();
      if history.initial_usage.is_empty() {
        continue;
      }

      let resource = resources.get(name).unwrap();
      let texture = match resource {
        VkResource::Texture { texture_b, .. } => {
          texture_b.as_ref().unwrap()
        },
        _ => { continue; }
      };
      let format = texture.get_info().format;
      let mut aspect = vk::ImageAspectFlags::empty();
      if !format.is_depth() && !format.is_stencil() {
        aspect = vk::ImageAspectFlags::COLOR;
      } else {
        if format.is_depth() {
          aspect |= vk::ImageAspectFlags::DEPTH;
        }
        if format.is_stencil() {
          aspect |= vk::ImageAspectFlags::STENCIL;
        }
      }

      image_barriers.push(Barrier::TextureBarrier {
        old_primary_usage: TextureUsage::empty(),
        new_primary_usage: history.initial_usage,
        old_usages: TextureUsage::empty(),
        new_usages: history.initial_usage,
        texture: texture
    });
      initialized_history_resources.insert(name.clone());
    }
    if !image_barriers.is_empty() {
      let mut cmd_buffer = context.get_thread_local().get_frame_local().get_command_buffer();
      cmd_buffer.barrier(&image_barriers);
      graphics_queue.submit(cmd_buffer.finish(), None, &[], &[]);
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
      external_resources: external_resources.cloned().unwrap_or_else(|| HashMap::new()),
      fb_cache: HashMap::new()
    }
  }

  fn execute_cmd_buffer(&self,
                        cmd_buffer: &mut VkCommandBufferRecorder,
                        frame_local: &VkFrameLocal,
                        fence: Option<&Arc<VkFence>>,
                        wait_semaphores: &[&VkSemaphore],
                        signal_semaphore: &[&VkSemaphore]) {
    let finished_cmd_buffer = std::mem::replace(cmd_buffer, frame_local.get_command_buffer());
    self.graphics_queue.submit(finished_cmd_buffer.finish(), fence, wait_semaphores, signal_semaphore);
    let c_queue = self.graphics_queue.clone();
    rayon::spawn(move || c_queue.process_submissions());
  }
}

impl RenderGraph<VkBackend> for VkRenderGraph {
  fn recreate(old: &Self, swapchain: &Arc<VkSwapchain>) -> Self {
    VkRenderGraph::new(&old.device, &old.thread_manager, &old.graphics_queue, &old.compute_queue, &old.transfer_queue, &old.template, &old.info, swapchain, Some(&old.external_resources))
  }

  fn render(&mut self) -> Result<(), SwapchainError> {
    self.thread_manager.begin_frame();

    let prepare_semaphore = self.thread_manager.get_shared().get_semaphore();
    let cmd_semaphore = self.thread_manager.get_shared().get_semaphore();
    let cmd_fence = self.thread_manager.get_shared().get_fence();
    let thread_manager = self.thread_manager.clone(); // clone here so we don't borrow self and don't have to acquire the frame local over and over again
    let thread_local = thread_manager.get_thread_local();
    let frame_local = thread_local.get_frame_local();
    let frame_counter = thread_manager.get_frame_counter();
    let mut image_index: u32 = 0;

    if self.renders_to_swapchain {
      if self.swapchain.surface().is_lost() {
        return Err(SwapchainError::SurfaceLost);
      }
      let swapchain_state = self.swapchain.state();
      if swapchain_state != VkSwapchainState::Okay {
        return Err(SwapchainError::Other);
      }

      let result = self.swapchain.prepare_back_buffer(&prepare_semaphore);
      if result.is_err() {
        return Err(match result.err().unwrap() {
          vk::Result::ERROR_OUT_OF_DATE_KHR => {
            if cfg!(target_os = "android") {
              SwapchainError::SurfaceLost
            } else {
              SwapchainError::Other
            }
          }
          vk::Result::ERROR_SURFACE_LOST_KHR => {
            SwapchainError::SurfaceLost
          }
          _ => { panic!("Acquiring image failed"); }
        });
      }

      frame_local.track_semaphore(&prepare_semaphore);
      let (index, _) = result.unwrap();
      image_index = index
    }

    let framebuffer_index = image_index as usize;
    for (index, pass) in self.passes.iter().enumerate() {
      let mut cmd_buffer = frame_local.get_command_buffer();

      match pass as &VkPass {
        VkPass::Graphics {
          callbacks,
          renderpass,
          renders_to_swapchain,
          resources: pass_resource_names,
          ..
        } => {
          let template = &self.template.passes[index];
          let attachments = match &template.pass_type {
            VkPassType::Graphics { attachments, .. } => attachments,
            _ => unreachable!()
          };
          let frame_buffer = get_frame_buffer(&mut self.fb_cache, &self.resources, &self.device, renderpass, &self.swapchain.get_views()[framebuffer_index], attachments);

          let framebuffer_index = if *renders_to_swapchain { framebuffer_index } else { 0 };

          let graph_resources = VkRenderGraphResources {
            resources: &self.resources,
            external_resources: &self.external_resources,
            pass_resource_names,
            swapchain: self.swapchain.as_ref(),
            swapchain_image_index: image_index
          };
          let graph_resources_ref: &'static VkRenderGraphResources = unsafe { std::mem::transmute(&graph_resources) };

          let mut clear_values = SmallVec::<[vk::ClearValue; 16]>::new();
          emit_barrier(&mut cmd_buffer, &template.barriers, &self.resources, &self.external_resources, &self.swapchain.get_views()[framebuffer_index]);
          cmd_buffer.flush_barriers();
          match &template.pass_type {
            VkPassType::Graphics { attachments, .. } => {

              for attachment in attachments {
                if attachment == BACK_BUFFER_ATTACHMENT_NAME {
                  clear_values.push(vk::ClearValue {
                    color: vk::ClearColorValue {
                      float32: [0f32; 4]
                    }
                  })
                } else {
                  let resource = self.resources.get(attachment).unwrap();
                  match resource {
                    VkResource::Texture { format, .. } => {
                      if format.is_depth() || format.is_stencil() {
                        clear_values.push(vk::ClearValue {
                          depth_stencil: vk::ClearDepthStencilValue {
                            depth: 1f32,
                            stencil: 0u32
                          }
                        });
                      } else {
                        clear_values.push(vk::ClearValue {
                          color: vk::ClearColorValue {
                            float32: [0f32; 4]
                          }
                        });
                      }
                    }
                    _ => unreachable!()
                  }
                }
              }

            },
            _ => unreachable!()
          }

          match callbacks {
            RenderPassCallbacks::Regular(callbacks) => {
              cmd_buffer.begin_render_pass(&renderpass, &frame_buffer, &clear_values, RenderpassRecordingMode::Commands);
              for i in 0..callbacks.len() {
                if i != 0 {
                  cmd_buffer.advance_subpass();
                }
                let callback = &callbacks[i];
                (callback)(&mut cmd_buffer, graph_resources_ref, frame_counter);
              }
              cmd_buffer.end_render_pass();
            }
            RenderPassCallbacks::InternallyThreaded(callbacks) => {
              cmd_buffer.begin_render_pass(&renderpass, &frame_buffer, &clear_values, RenderpassRecordingMode::CommandBuffers);
              for i in 0..callbacks.len() {
                if i != 0 {
                  cmd_buffer.advance_subpass();
                }
                let inner_info = VkInnerCommandBufferInfo {
                  render_pass: renderpass.clone(),
                  frame_buffer: frame_buffer.clone(),
                  sub_pass: i as u32
                };
                let provider = VkCommandBufferProvider {
                  inner_info: Some(inner_info),
                  thread_manager: self.thread_manager.clone(),
                };
                let callback = &callbacks[i];
                let inner_cmd_buffers = (callback)(&provider, graph_resources_ref, frame_counter);
                for inner_cmd_buffer in inner_cmd_buffers {
                  cmd_buffer.execute_inner_command_buffer(inner_cmd_buffer);
                }
              }
              cmd_buffer.end_render_pass();
            }
          }

          let cmd_semaphore = self.thread_manager.get_shared().get_semaphore();
          let prepare_semaphores = [prepare_semaphore.as_ref().as_ref()];
          let cmd_semaphores = [cmd_semaphore.as_ref().as_ref()];

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
            Some(&cmd_fence)
          } else {
            None
          };


          frame_local.track_semaphore(&cmd_semaphore);
          self.execute_cmd_buffer(&mut cmd_buffer, &frame_local, fence, wait_semaphores, signal_semaphores);
        }

        VkPass::ComputeCopy {
          callbacks,
          resources: pass_resource_names,
          renders_to_swapchain,
          ..
        } => {
          let framebuffer_index = if *renders_to_swapchain { framebuffer_index } else { 0 };

          let graph_resources = VkRenderGraphResources {
            resources: &self.resources,
            external_resources: &self.external_resources,
            pass_resource_names,
            swapchain: self.swapchain.as_ref(),
            swapchain_image_index: image_index
          };
          let graph_resources_ref: &'static VkRenderGraphResources = unsafe { std::mem::transmute(&graph_resources) };

          let template = &self.template.passes[index];
          emit_barrier(&mut cmd_buffer, &template.barriers, &self.resources, &self.external_resources, &self.swapchain.get_views()[framebuffer_index]);
          cmd_buffer.flush_barriers();

          match callbacks {
            RenderPassCallbacks::Regular(callbacks) => {
              for callback in callbacks {
                (callback)(&mut cmd_buffer, graph_resources_ref, frame_counter);
              }
            }
            RenderPassCallbacks::InternallyThreaded(callbacks) => {
              let provider = VkCommandBufferProvider {
                inner_info: None,
                thread_manager: self.thread_manager.clone(),
              };
              let callback = &callbacks[0];
              let inner_cmd_buffers = (callback)(&provider, graph_resources_ref, frame_counter);
              for inner_cmd_buffer in inner_cmd_buffers {
                cmd_buffer.execute_inner_command_buffer(inner_cmd_buffer);
              }
            }
          }

          if *renders_to_swapchain {
            // TRANSITION TO PRESENT

            let is_compute = match &template.pass_type {
              VkPassType::ComputeCopy { is_compute } => *is_compute,
              _ => unreachable!()
            };
            let src_stage = if is_compute { vk::PipelineStageFlags::COMPUTE_SHADER } else { vk::PipelineStageFlags::TRANSFER };
            let old_layout = if is_compute { vk::ImageLayout::GENERAL } else { vk::ImageLayout::TRANSFER_DST_OPTIMAL };
            let src_access = if is_compute { vk::AccessFlags::SHADER_WRITE } else { vk::AccessFlags::TRANSFER_WRITE };
            let view = &self.swapchain.get_views()[framebuffer_index];
            let texture = view.texture();

            cmd_buffer.barrier_vk(src_stage, vk::PipelineStageFlags::ALL_COMMANDS, vk::DependencyFlags::empty(), &[], &[], &[
              vk::ImageMemoryBarrier {
                src_access_mask: src_access,
                dst_access_mask: vk::AccessFlags::MEMORY_READ,
                src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                old_layout,
                new_layout: vk::ImageLayout::PRESENT_SRC_KHR,
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
                image: *texture.get_handle(),
                ..Default::default()
              }
            ])
          }

          let prepare_semaphores = [prepare_semaphore.as_ref().as_ref()];
          let cmd_semaphores = [cmd_semaphore.as_ref().as_ref()];

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
            Some(&cmd_fence)
          } else {
            None
          };


          frame_local.track_semaphore(&cmd_semaphore);
          self.execute_cmd_buffer(&mut cmd_buffer, &frame_local, fence, wait_semaphores, signal_semaphores);
        }
      }
    }

    if self.renders_to_swapchain {
      self.graphics_queue.present(&self.swapchain, image_index, &[&cmd_semaphore]);
      let c_graphics_queue = self.graphics_queue.clone();
      rayon::spawn(move || c_graphics_queue.process_submissions());
    }

    for resource in &mut self.resources.values_mut() {
      match resource {
        VkResource::Texture { view, view_b, texture, texture_b, .. } => {
          if view_b.is_some() {
            let temp = view_b.take().unwrap();
            *view_b = Some(std::mem::replace(view, temp));
            let temp = texture_b.take().unwrap();
            *texture_b = Some(std::mem::replace(texture, temp));
          }
        }
        VkResource::Buffer { buffer, buffer_b, .. } => {
          if buffer_b.is_some() {
            let temp = buffer_b.take().unwrap();
            *buffer_b = Some(std::mem::replace(buffer, temp));
          }
        }
      }
    }

    self.thread_manager.end_frame(&cmd_fence);
    Ok(())
  }

  fn swapchain(&self) -> &Arc<VkSwapchain> {
    &self.swapchain
  }
}

fn emit_barrier(
  command_buffer: &mut VkCommandBufferRecorder,
  barrier_templates: &[VkBarrierTemplate],
  resources: &HashMap<String, VkResource>,
  external_resources: &HashMap<String, ExternalResource<VkBackend>>,
  backbuffer: &Arc<VkTextureView>
) {
  let mut combined_src_stages = vk::PipelineStageFlags::empty();
  let mut combined_dst_stages = vk::PipelineStageFlags::empty();
  let mut barriers = SmallVec::<[Barrier<VkBackend>; 8]>::new();


  for barrier in barrier_templates {
    match barrier {
        VkBarrierTemplate::Image { name, is_history, old_usage, new_usage, old_primary_usage, new_primary_usage } => {
          let texture = if name == BACK_BUFFER_ATTACHMENT_NAME {
            backbuffer.texture()
          } else {
            external_resources.get(name).map_or_else(|| {
              match resources.get(name).unwrap() {
                VkResource::Texture { texture, texture_b, .. } => {
                  if !*is_history { texture } else { texture_b.as_ref().unwrap() }
                },
                _ => unreachable!()
              }
            }, |ext| match ext {
              ExternalResource::Texture(texture) => texture.texture(),
              _ => unreachable!()
            })
          };

          barriers.push(Barrier::TextureBarrier {
            old_primary_usage: *old_primary_usage,
            new_primary_usage: *new_primary_usage,
            old_usages: *old_usage,
            new_usages: *new_usage,
            texture
          });
        }
        VkBarrierTemplate::Buffer { name, is_history, old_usage, new_usage, old_primary_usage, new_primary_usage } => {
          let buffer = external_resources.get(name).map_or_else(|| {
            match resources.get(name).unwrap() {
              VkResource::Buffer { buffer, buffer_b, .. } => {
                if !*is_history { buffer } else { buffer_b.as_ref().unwrap() }
              },
              _ => unreachable!()
            }
          }, |ext| match ext {
            ExternalResource::Buffer(buffer) => buffer,
            _ => unreachable!()
          });
          barriers.push(Barrier::BufferBarrier {
            old_primary_usage: *old_primary_usage,
            new_primary_usage: *new_primary_usage,
            old_usages: *old_usage,
            new_usages: *new_usage,
            buffer
          });
        }
    }
  }

  command_buffer.barrier(&barriers);
}

fn get_frame_buffer(
  fb_cache: &mut HashMap<SmallVec<[Arc<VkTextureView>; 8]>, Arc<VkFrameBuffer>>,
  resources: &HashMap<String, VkResource>,
  device: &Arc<RawVkDevice>,
  render_pass: &Arc<VkRenderPass>,
  backbuffer: &Arc<VkTextureView>,
  attachment_names: &[String]
) -> Arc<VkFrameBuffer> {
  let mut width = u32::MAX;
  let mut height = u32::MAX;
  let key: SmallVec<[Arc<VkTextureView>; 8]> = attachment_names.iter().map(|name| {
    if name == BACK_BUFFER_ATTACHMENT_NAME {
      backbuffer.clone()
    } else {
      match resources.get(name).unwrap() {
        VkResource::Texture { view, texture, .. } => {
          width = min(texture.get_info().width, width);
          height = min(texture.get_info().height, height);
          view.clone()
        },
        _ => unreachable!()
      }
    }
  }).collect();

  if let Some(entry) = fb_cache.get(&key) {
    return entry.clone();
  }

  let fb = {
    let attachments: SmallVec<[&Arc<VkTextureView>; 8]> = key.iter().collect();
    Arc::new(VkFrameBuffer::new(device, width, height, render_pass, &attachments))
  };
  fb_cache.insert(key, fb.clone());
  fb
}
