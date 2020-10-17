use std::collections::{HashMap, VecDeque};
use std::collections::HashSet;
use std::sync::Arc;

use ash::vk;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::{RenderGraph, StoreAction, LoadAction, AttachmentSizeClass, AttachmentInfo, PassInfo, InputAttachmentReference, RenderGraphTemplate, RenderPassCallback};
use sourcerenderer_core::graphics::RenderGraphInfo;
use sourcerenderer_core::graphics::BACK_BUFFER_ATTACHMENT_NAME;
use sourcerenderer_core::graphics::{Texture, TextureInfo, AttachmentBlendInfo};

use crate::VkBackend;
use crate::VkDevice;
use crate::raw::RawVkDevice;
use crate::VkSwapchain;
use crate::format::format_to_vk;
use crate::pipeline::samples_to_vk;
use sourcerenderer_core::graphics::{Backend, CommandBufferType, CommandBuffer, RenderpassRecordingMode, Swapchain};
use context::VkThreadContextManager;
use std::cell::RefCell;
use ::{VkRenderPass, VkQueue};
use ::{VkFrameBuffer, VkSemaphore};
use ::{VkCommandBufferRecorder, VkFence};
use sourcerenderer_core::job::JobScheduler;
use std::sync::atomic::Ordering;
use std::cmp::{max, min};
use std::iter::FromIterator;
use VkTexture;
use texture::VkTextureView;
use graph_template::{VkRenderGraphTemplate, VkPassTemplate, VkPassType};
use sourcerenderer_core::ThreadPool;
use sourcerenderer_core::scope;

pub struct VkAttachment {
  texture: Arc<VkTexture>,
  view: Arc<VkTextureView>,
  info: AttachmentInfo
}

pub struct VkRenderGraph {
  device: Arc<RawVkDevice>,
  passes: Vec<Arc<VkPass>>,
  template: Arc<VkRenderGraphTemplate>,
  attachments: HashMap<String, VkAttachment>,
  context: Arc<VkThreadContextManager>,
  swapchain: Arc<VkSwapchain>,
  graphics_queue: Arc<VkQueue>,
  compute_queue: Option<Arc<VkQueue>>,
  transfer_queue: Option<Arc<VkQueue>>,
  renders_to_swapchain: bool,
  info: RenderGraphInfo<VkBackend>
}

pub enum VkPass {
  Graphics {
    framebuffers: Vec<Arc<VkFrameBuffer>>,
    renderpass: Arc<VkRenderPass>,
    renders_to_swapchain: bool,
    clear_values: Vec<vk::ClearValue>,
    callbacks: Vec<Arc<RenderPassCallback<VkBackend>>>
  },
  Compute,
  Copy
}

impl VkRenderGraph {
  pub fn new(device: &Arc<RawVkDevice>,
             context: &Arc<VkThreadContextManager>,
             graphics_queue: &Arc<VkQueue>,
             compute_queue: &Option<Arc<VkQueue>>,
             transfer_queue: &Option<Arc<VkQueue>>,
             template: &Arc<VkRenderGraphTemplate>,
             info: &RenderGraphInfo<VkBackend>,
             swapchain: &Arc<VkSwapchain>) -> Self {

    let mut attachments: HashMap<String, VkAttachment> = HashMap::new();
    let attachment_infos = template.attachments();
    for (name, attachment_info) in attachment_infos {
      // TODO: aliasing
      match attachment_info {
        // TODO: transient
        AttachmentInfo::Texture {
          format, size_class, width, height, samples, levels, ..
        } => {
          let mut usage = vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::INPUT_ATTACHMENT
            | vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::TRANSFER_DST;

          if format.is_depth() || format.is_stencil() {
            usage |= vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT;
          } else {
            usage |= vk::ImageUsageFlags::STORAGE;
          }

          let texture = Arc::new(VkTexture::new(&device, &TextureInfo {
            format: *format,
            width: if *size_class == AttachmentSizeClass::RelativeToSwapchain {
              (swapchain.get_width() as f32 * *width) as u32
            } else {
              *width as u32
            },
            height: if *size_class == AttachmentSizeClass::RelativeToSwapchain {
              (swapchain.get_height() as f32 * *height) as u32
            } else {
              *height as u32
            },
            depth: 1,
            mip_levels: *levels,
            array_length: 1,
            samples: *samples
          }, Some(name.as_str()), usage));

          let view = Arc::new(VkTextureView::new_render_target_view(device, &texture));
          attachments.insert(name.clone(), VkAttachment {
            texture,
            view,
            info: attachment_info.clone()
          });
        },
        _ => unimplemented!()
      }
    }

    let mut finished_passes: Vec<Arc<VkPass>> = Vec::new();
    let swapchain_views = swapchain.get_views();
    let passes = template.passes();
    for pass in passes {
      match &pass.pass_type {
        VkPassType::Graphics {
          render_pass
        } => {
          let mut clear_values = Vec::<vk::ClearValue>::new();

          let mut width = 0u32;
          let mut height = 0u32;
          let framebuffer_count = if pass.renders_to_swapchain { swapchain_views.len() } else { 1 };
          let mut framebuffer_attachments: Vec<Vec<vk::ImageView>> = Vec::with_capacity(framebuffer_count);
          for _ in 0..framebuffer_count {
            framebuffer_attachments.push(Vec::new());
          }

          for pass_attachment in &pass.attachments {
            if pass_attachment == BACK_BUFFER_ATTACHMENT_NAME {
              clear_values.push(vk::ClearValue {
                color: vk::ClearColorValue {
                  uint32: [0u32; 4]
                }
              });
            } else {
              let attachment = attachments.get(pass_attachment.as_str()).unwrap();
              let format = attachment.texture.get_info().format;
              if format.is_depth() || format.is_stencil() {
                clear_values.push(vk::ClearValue {
                  depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0f32,
                    stencil: 0u32
                  }
                });
              }
            }

            for i in 0..framebuffer_count {
              if pass_attachment == BACK_BUFFER_ATTACHMENT_NAME {
                framebuffer_attachments.get_mut(i).unwrap()
                  .push(*swapchain_views[i].get_view_handle());

                width = swapchain.get_width();
                height = swapchain.get_height();
              } else {
                framebuffer_attachments.get_mut(i).unwrap()
                  .push(*attachments[pass_attachment].view.get_view_handle());

                let texture_info = attachments[pass_attachment].texture.get_info();
                width = texture_info.width;
                height = texture_info.height;
              }
            }
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

          let mut callbacks: Vec<Arc<RenderPassCallback<VkBackend>>> = info.pass_callbacks[&pass.name].clone();

          finished_passes.push(Arc::new(VkPass::Graphics {
            framebuffers,
            callbacks,
            renders_to_swapchain: pass.renders_to_swapchain,
            renderpass: render_pass.clone(),
            clear_values
          }));
        },
        _ => unimplemented!()
      }
    }

    Self {
      device: device.clone(),
      template: template.clone(),
      passes: finished_passes,
      attachments,
      context: context.clone(),
      swapchain: swapchain.clone(),
      graphics_queue: graphics_queue.clone(),
      compute_queue: compute_queue.clone(),
      transfer_queue: transfer_queue.clone(),
      renders_to_swapchain: template.renders_to_swapchain(),
      info: info.clone()
    }
  }
}

impl RenderGraph<VkBackend> for VkRenderGraph {
  fn recreate(old: &Self, swapchain: &Arc<VkSwapchain>) -> Self {
    VkRenderGraph::new(&old.device, &old.context, &old.graphics_queue, &old.compute_queue, &old.transfer_queue, &old.template, &old.info, swapchain)
  }

  fn render(&mut self) -> Result<(), ()> {
    self.context.begin_frame();

    let mut prepare_semaphore = self.context.get_shared().get_semaphore();
    let cmd_semaphore = self.context.get_shared().get_semaphore();
    let cmd_fence = self.context.get_shared().get_fence();
    let mut image_index: u32 = 0;

    if self.renders_to_swapchain {
      let mut result = self.swapchain.prepare_back_buffer(&prepare_semaphore);
      if result.is_err() || !result.unwrap().1 && false {
        return Err(())
      }
      let (index, _) = result.unwrap();
      image_index = index
    }

    for pass in &self.passes {
      let c_context = self.context.clone();
      let c_pass = pass.clone();
      let c_queue = self.graphics_queue.clone();
      let c_prepare_semaphore = prepare_semaphore.clone();
      let c_cmd_semaphore = cmd_semaphore.clone();
      let c_cmd_fence = cmd_fence.clone();
      let framebuffer_index = image_index as usize;
      scope(move |s| {
        s.spawn(move |_| {
          let thread_context = c_context.get_thread_context();
          let mut frame_context = thread_context.get_frame_context();
          let mut cmd_buffer = frame_context.get_command_buffer(CommandBufferType::PRIMARY);

          match &c_pass as &VkPass {
            VkPass::Graphics { framebuffers, callbacks, renderpass, renders_to_swapchain, clear_values } => {
              cmd_buffer.begin_render_pass(&renderpass, &framebuffers[framebuffer_index], &clear_values, RenderpassRecordingMode::Commands);
              for i in 0..callbacks.len() {
                if i != 0 {
                  cmd_buffer.advance_subpass();
                }
                let callback = &callbacks[i];
                (callback)(&mut cmd_buffer);
              }
              cmd_buffer.end_render_pass();
              let submission = cmd_buffer.finish();

              let prepare_semaphores = [&**c_prepare_semaphore];
              let cmd_semaphores = [&**c_cmd_semaphore];

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
                Some(&c_cmd_fence as &VkFence)
              } else {
                None
              };

              c_queue.submit(submission, fence, &wait_semaphores, &signal_semaphores);

              if *renders_to_swapchain {
                frame_context.track_semaphore(&c_prepare_semaphore);
              }
            },
            _ => unimplemented!()
          }
        });
      });
    }

    let thread_context = self.context.get_thread_context();
    let mut frame_context = thread_context.get_frame_context();

    if self.renders_to_swapchain {
      let result = self.graphics_queue.present(&self.swapchain, image_index, &[&cmd_semaphore]);
      if result.is_err() || !result.unwrap() && false {
        return Err(());
      }

      frame_context.track_semaphore(&cmd_semaphore);
    }

    self.context.end_frame(&cmd_fence);
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
