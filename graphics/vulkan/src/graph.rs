use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::u32;
use std::cmp::{min};

use ash::vk;

use crate::{command::VkInnerCommandBufferInfo, thread_manager::{VkThreadManager, VkFrameLocal}};

use sourcerenderer_core::graphics::{CommandBufferType, RenderpassRecordingMode, Format, SampleCount, ExternalResource, TextureDimensions, SwapchainError, Swapchain};
use sourcerenderer_core::graphics::{BufferUsage, InnerCommandBufferProvider, LoadAction, MemoryUsage, RenderGraph, RenderGraphResources, RenderGraphResourceError, RenderPassCallbacks, RenderPassTextureExtent, StoreAction, CommandBuffer};
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
use crate::sync::VkEvent;
use sourcerenderer_core::pool::Recyclable;
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
    load_action: LoadAction,
    store_action: StoreAction,
    stencil_load_action: LoadAction,
    stencil_store_action: StoreAction,
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
  external_resources: Option<HashMap<String, ExternalResource<VkBackend>>>
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
  external_resources: &'a Option<HashMap<String, ExternalResource<VkBackend>>>,
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
      let external = self.external_resources.as_ref().and_then(|external_resources| external_resources.get(name));
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
      let external = self.external_resources.as_ref().and_then(|external_resources| external_resources.get(name));
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
      let external = self.external_resources.as_ref().and_then(|external_resources| external_resources.get(name));
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
    framebuffers: Vec<Arc<VkFrameBuffer>>,
    framebuffers_b: Option<Vec<Arc<VkFrameBuffer>>>,
    renderpass: Arc<VkRenderPass>,
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
    image_barriers: Vec<Vec<vk::ImageMemoryBarrier>>,
    buffer_barriers: Vec<Vec<vk::BufferMemoryBarrier>>,
    image_barriers_b: Option<Vec<Vec<vk::ImageMemoryBarrier>>>,
    buffer_barriers_b: Option<Vec<Vec<vk::BufferMemoryBarrier>>>,
    renders_to_swapchain: bool,
    clear_values: Vec<vk::ClearValue>,
    callbacks: RenderPassCallbacks<VkBackend>,
    resources: HashSet<String>,
    signal_event: Arc<Recyclable<VkEvent>>,
    wait_for_events: Vec<vk::Event>
  },
  ComputeCopy {
    name: String,
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
    image_barriers: Vec<Vec<vk::ImageMemoryBarrier>>,
    buffer_barriers: Vec<Vec<vk::BufferMemoryBarrier>>,
    image_barriers_b: Option<Vec<Vec<vk::ImageMemoryBarrier>>>,
    buffer_barriers_b: Option<Vec<Vec<vk::BufferMemoryBarrier>>>,
    callbacks: RenderPassCallbacks<VkBackend>,
    resources: HashSet<String>,
    signal_event: Arc<Recyclable<VkEvent>>,
    wait_for_events: Vec<vk::Event>,
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
    let mut events = Vec::<Arc<Recyclable<VkEvent>>>::new();
    for _ in 0..template.passes.len() {
      events.push(context.shared().get_event());
    }

    let resource_metadata = template.resources();
    for attachment_info in resource_metadata.values() {
      let has_history_resource = if let Some(history_usage) = attachment_info.history_usage.as_ref() {
        history_usage.first_used_in_pass_index >= attachment_info.produced_in_pass_index
      } else {
        false
      };
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
            samples: *samples
          };

          let texture = Arc::new(VkTexture::new(&device, &texture_info, Some(name.as_str()), usage));
          let view = Arc::new(VkTextureView::new_attachment_view(device, &texture));

          let (texture_b, view_b) = if has_history_resource {
            let texture = Arc::new(VkTexture::new(&device, &texture_info, Some((name.clone() + "B").as_str()), usage));
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
          let buffer = allocator.get_slice(MemoryUsage::GpuOnly, BufferUsage::STORAGE | BufferUsage::CONSTANT | BufferUsage::COPY_DST, *size as usize, Some(name));
          let buffer_b = if has_history_resource {
            Some(allocator.get_slice(MemoryUsage::GpuOnly, BufferUsage::STORAGE | BufferUsage::CONSTANT | BufferUsage::COPY_DST, *size as usize, Some(name)))
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
          render_pass, attachments, barriers
        } => {
          let mut clear_values = Vec::<vk::ClearValue>::new();

          let mut width = u32::MAX;
          let mut height = u32::MAX;
          let framebuffer_count = if pass.renders_to_swapchain { swapchain_views.len() } else { 1 };
          let mut framebuffer_attachments: Vec<Vec<Arc<VkTextureView>>> = Vec::with_capacity(framebuffer_count);
          let mut history_framebuffer_attachments: Vec<Vec<Arc<VkTextureView>>> = Vec::with_capacity(framebuffer_count);
          for _ in 0..framebuffer_count {
            framebuffer_attachments.push(Vec::new());
            history_framebuffer_attachments.push(Vec::new());
          }

          let mut uses_history_resources = false;
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

            for i in 0..framebuffer_count {
              if pass_attachment == BACK_BUFFER_ATTACHMENT_NAME {
                framebuffer_attachments.get_mut(i).unwrap()
                  .push(swapchain_views[i].clone());
                history_framebuffer_attachments.get_mut(i).unwrap()
                  .push(swapchain_views[i].clone());
              } else {
                let resource = resources.get(pass_attachment.as_str()).unwrap();
                let resource_view = match resource {
                  VkResource::Texture { view, .. } => view,
                  _ => unreachable!()
                };
                let resource_history_view = match resource {
                  VkResource::Texture { view_b: history_view, .. } => history_view,
                  _ => unreachable!()
                };
                framebuffer_attachments.get_mut(i).unwrap()
                  .push(resource_view.clone());
                if let Some(view) = resource_history_view {
                  history_framebuffer_attachments.get_mut(i).unwrap()
                    .push(view.clone());
                  uses_history_resources = true;
                } else {
                  history_framebuffer_attachments.get_mut(i).unwrap()
                    .push(resource_view.clone());
                }
              }
            }
          }

          if width == u32::MAX || height == u32::MAX {
            panic!("Failed to determine frame buffer dimensions");
          }

          let mut framebuffers: Vec<Arc<VkFrameBuffer>> = Vec::with_capacity(framebuffer_attachments.len());
          for fb_attachments in framebuffer_attachments {
            let attachment_refs: Vec<&Arc<VkTextureView>> = fb_attachments.iter().map(|a| a).collect();
            let framebuffer = Arc::new(VkFrameBuffer::new(device, width, height, render_pass, &attachment_refs));
            framebuffers.push(framebuffer);
          }
          let history_framebuffers = if uses_history_resources {
            let mut history_framebuffers: Vec<Arc<VkFrameBuffer>> = Vec::with_capacity(history_framebuffer_attachments.len());
            for fb_attachments in history_framebuffer_attachments {
              let attachment_refs: Vec<&Arc<VkTextureView>> = fb_attachments.iter().map(|a| a).collect();
              let framebuffer = Arc::new(VkFrameBuffer::new(device, width, height, render_pass, &attachment_refs));
              history_framebuffers.push(framebuffer);
            }
            Some(history_framebuffers)
          } else {
            None
          };

          let mut wait_events = Vec::<vk::Event>::new();
          let mut src_stage = vk::PipelineStageFlags::empty();
          let mut dst_stage = vk::PipelineStageFlags::empty();
          let mut image_barriers = Vec::<Vec::<vk::ImageMemoryBarrier>>::new();
          let mut buffer_barriers = Vec::<Vec::<vk::BufferMemoryBarrier>>::new();
          let (mut image_barriers_b, mut buffer_barriers_b) = if pass.has_history_resources {
            (Some(Vec::<Vec::<vk::ImageMemoryBarrier>>::new()), Some(Vec::<Vec::<vk::BufferMemoryBarrier>>::new()))
          } else {
            (None, None)
          };
          for i in 0..framebuffer_count {
            image_barriers.push(Vec::new());
            buffer_barriers.push(Vec::new());
            if let Some(image_barriers_b) = image_barriers_b.as_mut() {
              image_barriers_b.push(Vec::new());
            }
            if let Some(buffer_barriers_b) = buffer_barriers_b.as_mut() {
              buffer_barriers_b.push(Vec::new());
            }
            let image_barriers = image_barriers.last_mut().unwrap();
            let buffer_barriers = buffer_barriers.last_mut().unwrap();
            let mut image_barriers_b = image_barriers_b.as_mut().map(|v| v.last_mut().unwrap());
            let mut buffer_barriers_b = buffer_barriers_b.as_mut().map(|v| v.last_mut().unwrap());

            for barrier_template in barriers {
              match barrier_template {
                VkBarrierTemplate::Image {
                  name, old_layout, new_layout, src_access_mask, dst_access_mask, src_stage: image_src_stage, dst_stage: image_dst_stage, src_queue_family_index, dst_queue_family_index, is_history
                } => {
                  src_stage |= *image_src_stage;
                  dst_stage |= *image_dst_stage;

                  let metadata = resource_metadata.get(name.as_str()).unwrap();
                  let is_external = match metadata.template {
                    VkResourceTemplate::Texture { .. } => false,
                    VkResourceTemplate::ExternalTexture { .. } => true,
                    _ => panic!("Mismatched resource type")
                  };
                  let (texture, texture_b) = if name == BACK_BUFFER_ATTACHMENT_NAME {
                    (swapchain_views[i].texture(), None)
                  } else if !is_external {
                    if !pass.has_history_resources && !pass.has_external_resources {
                      wait_events.push(*(events[metadata.produced_in_pass_index as usize].handle()));
                    }

                    println!("getting resource: {}", &name);
                    let resource = resources.get(name.as_str()).unwrap();
                    match resource {
                      VkResource::Texture { texture, texture_b, .. } => (texture, texture_b.as_ref()),
                      _ => unreachable!()
                    }
                  } else {
                    let resource = external_resources
                        .and_then(|r| r.get(name.as_str()))
                        .unwrap_or_else(|| panic!("Can't find resource {}", name));
                    let resource_view = match resource {
                      ExternalResource::Texture(view) => view,
                      _ => unreachable!()
                    };
                    (resource_view.texture(), None)
                  };

                  let mut image_barrier = vk::ImageMemoryBarrier {
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
                  };
                  let mut image_barrier_b = image_barrier;

                  if let Some(texture_b) = texture_b {
                    uses_history_resources = true;
                    image_barrier_b.image = *texture_b.get_handle();
                  }

                  if *is_history {
                    std::mem::swap(&mut image_barrier.image, &mut image_barrier_b.image);
                  }
                  image_barriers.push(image_barrier);
                  if let Some(image_barriers_b) = image_barriers_b.as_mut() {
                    image_barriers_b.push(image_barrier_b);
                  }
                }
                VkBarrierTemplate::Buffer {
                  name, src_access_mask, dst_access_mask, src_stage: buffer_src_stage, dst_stage: buffer_dst_stage, src_queue_family_index, dst_queue_family_index, is_history
                } => {
                  src_stage |= *buffer_src_stage;
                  dst_stage |= *buffer_dst_stage;

                  let metadata = resource_metadata.get(name.as_str()).unwrap();
                  let is_external = match metadata.template {
                    VkResourceTemplate::Buffer { .. } => false,
                    VkResourceTemplate::ExternalBuffer { .. } => true,
                    _ => panic!("Mismatched resource type")
                  };
                  let (buffer, buffer_b) = if !is_external {
                    if !pass.has_history_resources && !pass.has_external_resources {
                      wait_events.push(*(events[metadata.produced_in_pass_index as usize].handle()));
                    }

                    let resource = resources.get(name.as_str()).unwrap();
                    match resource {
                      VkResource::Buffer { buffer, buffer_b, .. } => (buffer, buffer_b.as_ref()),
                      _ => unreachable!()
                    }
                  } else {
                    let resource = external_resources
                        .and_then(|r| r.get(name.as_str()))
                        .unwrap_or_else(|| panic!("Can't find resource {}", name));
                    let resource_buffer = match resource {
                      ExternalResource::Buffer(buffer) => buffer,
                      _ => unreachable!()
                    };
                    (resource_buffer, None)
                  };
                  let (offset, length) = buffer.get_offset_and_length();

                  let mut buffer_barrier = vk::BufferMemoryBarrier {
                    src_access_mask: *src_access_mask,
                    dst_access_mask: *dst_access_mask,
                    src_queue_family_index: *src_queue_family_index,
                    dst_queue_family_index: *dst_queue_family_index,
                    buffer: *buffer.get_buffer().get_handle(),
                    offset: offset as u64,
                    size: length as u64,
                    ..Default::default()
                  };
                  let mut buffer_barrier_b = buffer_barrier;

                  if let Some(buffer_b) = buffer_b {
                    uses_history_resources = true;
                    buffer_barrier.buffer = *buffer_b.get_buffer().get_handle();
                    buffer_barrier.offset = buffer_b.get_offset() as u64;
                    buffer_barrier.size = buffer_b.get_length() as u64;
                  }

                  if *is_history {
                    std::mem::swap(&mut buffer_barrier.buffer, &mut buffer_barrier_b.buffer);
                    std::mem::swap(&mut buffer_barrier.offset, &mut buffer_barrier_b.offset);
                    std::mem::swap(&mut buffer_barrier.size, &mut buffer_barrier_b.size);
                  }
                  buffer_barriers.push(buffer_barrier);
                  if let Some(buffer_barriers_b) = buffer_barriers_b.as_mut() {
                    buffer_barriers_b.push(buffer_barrier_b);
                  }
                }
              }
            }
          }

          let callbacks: RenderPassCallbacks<VkBackend> = info.pass_callbacks[&pass.name].clone();

          let index = finished_passes.len();
          finished_passes.push(VkPass::Graphics {
            name: pass.name.clone(),
            framebuffers,
            framebuffers_b: history_framebuffers,
            src_stage,
            dst_stage,
            image_barriers,
            buffer_barriers,
            image_barriers_b,
            buffer_barriers_b,
            callbacks,
            renders_to_swapchain: pass.renders_to_swapchain,
            renderpass: render_pass.clone(),
            clear_values,
            resources: pass.resources.clone(),
            signal_event: events[index].clone(),
            wait_for_events: wait_events
          });
        },

        VkPassType::ComputeCopy {
          barriers, is_compute: _
        } => {
          let framebuffer_count = if pass.renders_to_swapchain { swapchain_views.len() } else { 1 };

          let mut has_history_resources = false;
          let mut wait_events = Vec::<vk::Event>::new();
          let mut src_stage = vk::PipelineStageFlags::empty();
          let mut dst_stage = vk::PipelineStageFlags::empty();
          let mut image_barriers = Vec::<Vec::<vk::ImageMemoryBarrier>>::new();
          let mut buffer_barriers = Vec::<Vec::<vk::BufferMemoryBarrier>>::new();
          let (mut image_barriers_b, mut buffer_barriers_b) = if pass.has_history_resources {
            (Some(Vec::<Vec::<vk::ImageMemoryBarrier>>::new()), Some(Vec::<Vec::<vk::BufferMemoryBarrier>>::new()))
          } else {
            (None, None)
          };
          for i in 0..framebuffer_count {
            image_barriers.push(Vec::new());
            buffer_barriers.push(Vec::new());
            if let Some(image_barriers_b) = image_barriers_b.as_mut() {
              image_barriers_b.push(Vec::new());
            }
            if let Some(buffer_barriers_b) = buffer_barriers_b.as_mut() {
              buffer_barriers_b.push(Vec::new());
            }
            let image_barriers = image_barriers.last_mut().unwrap();
            let buffer_barriers = buffer_barriers.last_mut().unwrap();
            let mut image_barriers_b = image_barriers_b.as_mut().map(|v| v.last_mut().unwrap());
            let mut buffer_barriers_b = buffer_barriers_b.as_mut().map(|v| v.last_mut().unwrap());

            for barrier_template in barriers {
              match barrier_template {
                VkBarrierTemplate::Image {
                  name, old_layout, new_layout, src_access_mask, dst_access_mask, src_stage: image_src_stage, dst_stage: image_dst_stage, src_queue_family_index, dst_queue_family_index, is_history
                } => {
                  src_stage |= *image_src_stage;
                  dst_stage |= *image_dst_stage;

                  let metadata = resource_metadata.get(name.as_str()).unwrap();
                  let is_external = match metadata.template {
                    VkResourceTemplate::Texture { .. } => false,
                    VkResourceTemplate::ExternalTexture { .. } => true,
                    _ => panic!("Mismatched resource type")
                  };
                  let (texture, texture_b) = if name == BACK_BUFFER_ATTACHMENT_NAME {
                    (swapchain_views[i].texture(), None)
                  } else if !is_external {
                    if !pass.has_history_resources && !pass.has_external_resources {
                      wait_events.push(*(events[metadata.produced_in_pass_index as usize].handle()));
                    }

                    let resource = resources.get(name.as_str()).unwrap();
                    match resource {
                      VkResource::Texture { texture, texture_b, .. } => (texture, texture_b.as_ref()),
                      _ => unreachable!()
                    }
                  } else {
                    let resource = external_resources
                        .and_then(|r| r.get(name.as_str()))
                        .unwrap_or_else(|| panic!("Can't find resource {}", name));
                    let resource_view = match resource {
                      ExternalResource::Texture(view) => view,
                      _ => unreachable!()
                    };
                    (resource_view.texture(), None)
                  };

                  let mut image_barrier = vk::ImageMemoryBarrier {
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
                  };
                  let mut image_barrier_b = image_barrier;

                  if let Some(texture_b) = texture_b {
                    has_history_resources = true;
                    image_barrier_b.image = *texture_b.get_handle();
                  }

                  if *is_history {
                    std::mem::swap(&mut image_barrier.image, &mut image_barrier_b.image);
                  }
                  image_barriers.push(image_barrier);
                  if let Some(image_barriers_b) = image_barriers_b.as_mut() {
                    image_barriers_b.push(image_barrier_b);
                  }
                }
                VkBarrierTemplate::Buffer {
                  name, src_access_mask, dst_access_mask, src_stage: buffer_src_stage, dst_stage: buffer_dst_stage, src_queue_family_index, dst_queue_family_index, is_history
                } => {
                  src_stage |= *buffer_src_stage;
                  dst_stage |= *buffer_dst_stage;
                  let metadata = resource_metadata.get(name.as_str()).unwrap();
                  let is_external = match metadata.template {
                    VkResourceTemplate::Buffer { .. } => false,
                    VkResourceTemplate::ExternalBuffer { .. } => true,
                    _ => panic!("Mismatched resource type")
                  };
                  let (buffer, buffer_b) = if !is_external {
                    if !pass.has_history_resources && !pass.has_external_resources {
                      wait_events.push(*(events[metadata.produced_in_pass_index as usize].handle()));
                    }

                    let resource = resources.get(name.as_str()).unwrap();
                    match resource {
                      VkResource::Buffer { buffer, buffer_b, .. } => (buffer, buffer_b.as_ref()),
                      _ => unreachable!()
                    }
                  } else {
                    let resource = external_resources
                        .and_then(|r| r.get(name.as_str()))
                        .unwrap_or_else(|| panic!("Can't find resource {}", name));
                    let resource_buffer = match resource {
                      ExternalResource::Buffer(buffer) => buffer,
                      _ => unreachable!()
                    };
                    (resource_buffer, None)
                  };
                  let (offset, length) = buffer.get_offset_and_length();

                  let mut buffer_barrier = vk::BufferMemoryBarrier {
                    src_access_mask: *src_access_mask,
                    dst_access_mask: *dst_access_mask,
                    src_queue_family_index: *src_queue_family_index,
                    dst_queue_family_index: *dst_queue_family_index,
                    buffer: *buffer.get_buffer().get_handle(),
                    offset: offset as u64,
                    size: length as u64,
                    ..Default::default()
                  };
                  let mut buffer_barrier_b = buffer_barrier;

                  if let Some(buffer_b) = buffer_b {
                    has_history_resources = true;
                    buffer_barrier.buffer = *buffer_b.get_buffer().get_handle();
                    buffer_barrier.offset = buffer_b.get_offset() as u64;
                    buffer_barrier.size = buffer_b.get_length() as u64;
                  }

                  if *is_history {
                    std::mem::swap(&mut buffer_barrier.buffer, &mut buffer_barrier_b.buffer);
                    std::mem::swap(&mut buffer_barrier.offset, &mut buffer_barrier_b.offset);
                    std::mem::swap(&mut buffer_barrier.size, &mut buffer_barrier_b.size);
                  }
                  buffer_barriers.push(buffer_barrier);
                  if let Some(buffer_barriers_b) = buffer_barriers_b.as_mut() {
                    buffer_barriers_b.push(buffer_barrier_b);
                  }
                }
              }
            }
          }

          let callbacks: RenderPassCallbacks<VkBackend> = info.pass_callbacks[&pass.name].clone();

          let index = finished_passes.len();
          finished_passes.push(VkPass::ComputeCopy {
            name: pass.name.clone(),
            src_stage,
            dst_stage,
            image_barriers,
            buffer_barriers,
            image_barriers_b,
            buffer_barriers_b,
            callbacks,
            resources: pass.resources.clone(),
            signal_event: events[index].clone(),
            wait_for_events: wait_events,
            renders_to_swapchain: pass.renders_to_swapchain
          })
        }
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
      external_resources: external_resources.cloned()
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
    VkRenderGraph::new(&old.device, &old.thread_manager, &old.graphics_queue, &old.compute_queue, &old.transfer_queue, &old.template, &old.info, swapchain, old.external_resources.as_ref())
  }

  fn render(&mut self) -> Result<(), SwapchainError> {
    self.thread_manager.begin_frame();

    let prepare_semaphore = self.thread_manager.get_shared().get_semaphore();
    let cmd_semaphore = self.thread_manager.get_shared().get_semaphore();
    let cmd_fence = self.thread_manager.get_shared().get_fence();
    let thread_local = self.thread_manager.get_thread_local();
    let frame_local = thread_local.get_frame_local();
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
    for pass in &self.passes {
      let mut cmd_buffer = frame_local.get_command_buffer();

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
          resources: pass_resource_names,
          wait_for_events,
          signal_event,
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

          if *src_stage != vk::PipelineStageFlags::empty() || !buffer_barriers.is_empty() || !image_barriers.is_empty() {
            if wait_for_events.is_empty() {
              cmd_buffer.barrier(*src_stage, *dst_stage, vk::DependencyFlags::empty(),
                                 &[], &buffer_barriers[framebuffer_index], &image_barriers[framebuffer_index]);
            } else {
              cmd_buffer.wait_events(wait_for_events, *src_stage, *dst_stage, &[], &buffer_barriers[framebuffer_index], &image_barriers[framebuffer_index]);
            }
          }
          match callbacks {
            RenderPassCallbacks::Regular(callbacks) => {
              cmd_buffer.begin_render_pass(&renderpass, &framebuffers[framebuffer_index], &clear_values, RenderpassRecordingMode::Commands);
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
              cmd_buffer.begin_render_pass(&renderpass, &framebuffers[framebuffer_index], &clear_values, RenderpassRecordingMode::CommandBuffers);
              for i in 0..callbacks.len() {
                if i != 0 {
                  cmd_buffer.advance_subpass();
                }
                let inner_info = VkInnerCommandBufferInfo {
                  render_pass: renderpass.clone(),
                  frame_buffer: framebuffers[framebuffer_index].clone(),
                  sub_pass: i as u32
                };
                let provider = VkCommandBufferProvider {
                  inner_info: Some(inner_info),
                  thread_manager: self.thread_manager.clone(),
                };
                let callback = &callbacks[i];
                let inner_cmd_buffers = (callback)(&provider, graph_resources_ref);
                for inner_cmd_buffer in inner_cmd_buffers {
                  cmd_buffer.execute_inner_command_buffer(inner_cmd_buffer);
                }
              }
              cmd_buffer.end_render_pass();
            }
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
          self.execute_cmd_buffer(&mut cmd_buffer, &frame_local, fence, &wait_semaphores, &signal_semaphores);
          cmd_buffer.signal_event(*(signal_event.handle()), vk::PipelineStageFlags::ALL_GRAPHICS);
        }

        VkPass::ComputeCopy {
          src_stage,
          dst_stage,
          buffer_barriers,
          image_barriers,
          callbacks,
          resources: pass_resource_names,
          signal_event,
          wait_for_events,
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

          if *src_stage != vk::PipelineStageFlags::empty() || !buffer_barriers.is_empty() || !image_barriers.is_empty() {
            if wait_for_events.is_empty() {
              cmd_buffer.barrier(*src_stage, *dst_stage, vk::DependencyFlags::empty(),
                                 &[], &buffer_barriers[framebuffer_index], &image_barriers[framebuffer_index]);
            } else {
              cmd_buffer.wait_events(wait_for_events, *src_stage, *dst_stage, &[], &buffer_barriers[framebuffer_index], &image_barriers[framebuffer_index]);
            }
          }
          match callbacks {
            RenderPassCallbacks::Regular(callbacks) => {
              for callback in callbacks {
                (callback)(&mut cmd_buffer, graph_resources_ref);
              }
            }
            RenderPassCallbacks::InternallyThreaded(callbacks) => {
              let provider = VkCommandBufferProvider {
                inner_info: None,
                thread_manager: self.thread_manager.clone(),
              };
              let callback = &callbacks[0];
              let inner_cmd_buffers = (callback)(&provider, graph_resources_ref);
              for inner_cmd_buffer in inner_cmd_buffers {
                cmd_buffer.execute_inner_command_buffer(inner_cmd_buffer);
              }
            }
          }

          self.execute_cmd_buffer(&mut cmd_buffer, &frame_local, None, &[], &[]);
          cmd_buffer.signal_event(*(signal_event.handle()), vk::PipelineStageFlags::COMPUTE_SHADER);
        }
      }
    }

    if self.renders_to_swapchain {
      self.graphics_queue.present(&self.swapchain, image_index, &[&cmd_semaphore]);
      let c_graphics_queue = self.graphics_queue.clone();
      rayon::spawn(move || c_graphics_queue.process_submissions());
    }

    // A-B swap for history resources
    for pass in &mut self.passes {
      match pass {
        VkPass::Graphics {
          framebuffers,
          framebuffers_b,
          buffer_barriers,
          buffer_barriers_b,
          image_barriers,
          image_barriers_b,
          ..
        } => {
          if framebuffers_b.is_some() {
            let temp = framebuffers_b.take().unwrap();
            *framebuffers_b = Some(std::mem::replace(framebuffers, temp));
          }
          if image_barriers_b.is_some() {
            let temp = image_barriers_b.take().unwrap();
            *image_barriers_b = Some(std::mem::replace(image_barriers, temp));
          }
          if buffer_barriers_b.is_some() {
            let temp = buffer_barriers_b.take().unwrap();
            *buffer_barriers_b = Some(std::mem::replace(buffer_barriers, temp));
          }
        }

        VkPass::ComputeCopy {
          buffer_barriers,
          buffer_barriers_b,
          image_barriers,
          image_barriers_b,
          ..
        } => {
          if image_barriers_b.is_some() {
            let temp = image_barriers_b.take().unwrap();
            *image_barriers_b = Some(std::mem::replace(image_barriers, temp));
          }
          if buffer_barriers_b.is_some() {
            let temp = buffer_barriers_b.take().unwrap();
            *buffer_barriers_b = Some(std::mem::replace(buffer_barriers, temp));
          }
        }
      }
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
