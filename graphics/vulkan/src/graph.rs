use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use ash::vk;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::graph::RenderGraph;
use sourcerenderer_core::graphics::graph::RenderGraphInfo;
use sourcerenderer_core::graphics::graph::RenderPassInfo;
use sourcerenderer_core::graphics::graph::RenderGraphAttachmentInfo;
use sourcerenderer_core::graphics::graph::BACK_BUFFER_ATTACHMENT_NAME;
use sourcerenderer_core::graphics::Texture;

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
use VkCommandBufferRecorder;

pub struct VkAttachment {
  texture: vk::Image,
  view: vk::ImageView
}

pub struct VkRenderGraph {
  device: Arc<RawVkDevice>,
  passes: Vec<VkRenderGraphPass>,
  attachments: HashMap<String, VkAttachment>,
  context: Arc<VkThreadContextManager>,
  swapchain: Arc<VkSwapchain>,
  does_render_to_frame_buffer: bool,
  graphics_queue: Arc<VkQueue>,
  compute_queue: Option<Arc<VkQueue>>,
  transfer_queue: Option<Arc<VkQueue>>
}

pub struct VkRenderGraphPass { // TODO rename to VkRenderPass
  device: Arc<RawVkDevice>,
  render_pass: Arc<VkRenderPass>,
  frame_buffer: Vec<Arc<VkFrameBuffer>>,
  callback: Arc<dyn (Fn(&mut VkCommandBufferRecorder) -> usize) + Send + Sync>,
  is_rendering_to_swap_chain: bool
}

impl VkRenderGraph {
  pub fn new(device: &Arc<RawVkDevice>,
             context: &Arc<VkThreadContextManager>,
             graphics_queue: &Arc<VkQueue>,
             compute_queue: &Option<Arc<VkQueue>>,
             transfer_queue: &Option<Arc<VkQueue>>,
             info: &RenderGraphInfo<VkBackend>,
             swapchain: &Arc<VkSwapchain>) -> Self {

    // SHORTTERM
    // TODO: allocate images & image views
    // TODO: add render callback
    // TODO: allocate command pool & buffers
    // TODO: lazily create frame buffer for swapchain images
    // TODO: integrate with new job system + figure out threading
    // TODO: recreate graph when swapchain changes
    // TODO: more generic support for external images / one time rendering
    // TODO: sort passes by dependencies
    // TODO: merge passes
    // TODO: async compute
    // TODO: transient resources

    let mut layouts: HashMap<&str, vk::ImageLayout> = HashMap::new();
    layouts.insert(BACK_BUFFER_ATTACHMENT_NAME, vk::ImageLayout::UNDEFINED);

    let attachments: HashMap<String, VkAttachment> = HashMap::new();

    let mut did_render_to_backbuffer = false;

    let passes: Vec<VkRenderGraphPass> = info.passes.iter().map(|p| {
      let vk_device = &device.device;
      let pass_renders_to_backbuffer = p.outputs.iter().any(|output| &output.name == BACK_BUFFER_ATTACHMENT_NAME);
      did_render_to_backbuffer |= pass_renders_to_backbuffer;

      let mut render_pass_attachments: Vec<vk::AttachmentDescription> = Vec::new();
      let mut attachment_indices: HashMap<&str, u32> = HashMap::new();
      let mut dependencies: Vec<vk::SubpassDependency> = Vec::new();

      for output in &p.outputs {
        let index = render_pass_attachments.len() as u32;
        if &output.name == BACK_BUFFER_ATTACHMENT_NAME {
          let info = swapchain.get_textures().first().unwrap().get_info();
          render_pass_attachments.push(
            vk::AttachmentDescription {
              format: format_to_vk(info.format),
              samples: samples_to_vk(info.samples),
              load_op: vk::AttachmentLoadOp::CLEAR,
              store_op: vk::AttachmentStoreOp::STORE,
              stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
              stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
              initial_layout: *layouts.get(&output.name as &str).unwrap_or(&vk::ImageLayout::UNDEFINED),
              final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
              ..Default::default()
            }
          );
          layouts.insert(&output.name as &str, vk::ImageLayout::PRESENT_SRC_KHR);
          attachment_indices.insert(&output.name as &str, index);
        } else {
          let attachment = info.attachments.get(&output.name).expect("Output not attachment not declared.");
          render_pass_attachments.push(
            vk::AttachmentDescription {
              format: format_to_vk(attachment.format),
              samples: samples_to_vk(attachment.samples),
              load_op: vk::AttachmentLoadOp::CLEAR,
              store_op: vk::AttachmentStoreOp::STORE,
              stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
              stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
              initial_layout: *layouts.get(&output.name as &str).unwrap_or(&vk::ImageLayout::UNDEFINED),
              final_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
              ..Default::default()
            }
          );
          layouts.insert(&output.name as &str, vk::ImageLayout::PRESENT_SRC_KHR);
          attachment_indices.insert(&output.name as &str, index);
        }
      }

      let input_attachments: Vec<vk::AttachmentReference> = p.inputs
        .iter()
        .map(|i| vk::AttachmentReference {
          attachment: (*attachment_indices.get(&i.name as &str).expect(format!("Couldn't find index for {}", &i.name).as_str())) as u32,
          layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        })
        .collect();

      let output_attachments: Vec<vk::AttachmentReference> = p.outputs
        .iter()
        .map(|i| vk::AttachmentReference {
          attachment: (*attachment_indices.get(&i.name as &str).expect(format!("Couldn't find index for {}", &i.name).as_str())) as u32,
          layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        })
        .collect();
      let subpass = vk::SubpassDescription {
        p_input_attachments: input_attachments.as_ptr(),
        input_attachment_count: input_attachments.len() as u32,
        p_color_attachments: output_attachments.as_ptr(),
        color_attachment_count: output_attachments.len() as u32,
        ..Default::default()
      };
      let render_pass_create_info = vk::RenderPassCreateInfo {
        p_attachments: render_pass_attachments.as_ptr(),
        attachment_count: render_pass_attachments.len() as u32,
        p_subpasses: &subpass as *const vk::SubpassDescription,
        subpass_count: 1,
        p_dependencies: dependencies.as_ptr(),
        dependency_count: dependencies.len() as u32,
        ..Default::default()
      };
      let render_pass = Arc::new(VkRenderPass::new(device, &render_pass_create_info));

      let mut frame_buffers: Vec<Arc<VkFrameBuffer>> = Vec::new();
      let swapchain_views = swapchain.get_views();
      let frame_buffer_count = if p.outputs.iter().any(|o| &o.name == BACK_BUFFER_ATTACHMENT_NAME) {
        swapchain_views.len()
      } else {
        1
      };
      for i in 0..frame_buffer_count {
        let frame_buffer_attachments: Vec<vk::ImageView> = p.outputs.iter().map(|a| if &a.name == BACK_BUFFER_ATTACHMENT_NAME {
          swapchain_views[i]
        } else {
          attachments[&a.name as &str].view
        }).collect();

        let (width, height) = if &p.outputs[0].name == BACK_BUFFER_ATTACHMENT_NAME {
          (swapchain.get_width(), swapchain.get_height())
        } else {
          let attachment_info = &info.attachments[&p.outputs[0].name as &str];
          (attachment_info.width as u32, attachment_info.height as u32)
        };

        let frame_buffer_info = vk::FramebufferCreateInfo {
          render_pass: *render_pass.get_handle(),
          attachment_count: frame_buffer_attachments.len() as u32,
          p_attachments: frame_buffer_attachments.as_ptr(),
          layers: 1,
          width,
          height,
          ..Default::default()
        };
        let frame_buffer = Arc::new(VkFrameBuffer::new(device, &frame_buffer_info));
        frame_buffers.push(frame_buffer);
      }

      VkRenderGraphPass {
        device: device.clone(),
        frame_buffer: frame_buffers,
        render_pass,
        callback: p.render.clone(),
        is_rendering_to_swap_chain: pass_renders_to_backbuffer
      }
    }).collect();

    return VkRenderGraph {
      device: device.clone(),
      context: context.clone(),
      passes,
      attachments,
      graphics_queue: graphics_queue.clone(),
      compute_queue: compute_queue.clone(),
      transfer_queue: transfer_queue.clone(),
      swapchain: swapchain.clone(),
      does_render_to_frame_buffer: did_render_to_backbuffer
    };
  }
}

impl RenderGraph<VkBackend> for VkRenderGraph {
  fn recreate(&mut self, swap_chain: &VkSwapchain) {

  }

  fn render(&mut self) {
    self.context.begin_frame();
    let thread_context = self.context.get_thread_context();
    let mut frame_context = thread_context.get_frame_context();

    let prepare_semaphore = self.context.get_shared().get_semaphore();
    let cmd_semaphore = self.context.get_shared().get_semaphore();
    let cmd_fence = self.context.get_shared().get_fence();
    let swapchain_image_index = if self.does_render_to_frame_buffer {
      let (_, index) = self.swapchain.prepare_back_buffer(&prepare_semaphore);
      Some(index)
    } else {
      None
    };

    let mut cmd_buffer = frame_context.get_command_pool().get_command_buffer(CommandBufferType::PRIMARY);
    for pass in &self.passes {
      cmd_buffer.begin_render_pass(&pass.render_pass, &pass.frame_buffer[self.context.get_frame_counter() as usize % pass.frame_buffer.len()], RenderpassRecordingMode::Commands);
      (pass.callback)(&mut cmd_buffer);
      cmd_buffer.end_render_pass();
    }
    let submission = cmd_buffer.finish();

    let prepare_semaphores = [&*prepare_semaphore];
    let cmd_semaphores = [&*cmd_semaphore];

    let wait_semaphores: &[&VkSemaphore] = if swapchain_image_index.is_some() {
      &prepare_semaphores
    } else {
      &[]
    };
    let signal_semaphores: &[&VkSemaphore] = if swapchain_image_index.is_some() {
      &cmd_semaphores
    } else {
      &[]
    };
    self.graphics_queue.submit(submission, Some(&cmd_fence), &wait_semaphores, &signal_semaphores);
    if swapchain_image_index.is_some() {
      frame_context.track_semaphore(prepare_semaphore);
    }

    if let Some(index) = swapchain_image_index {
      self.graphics_queue.present(&self.swapchain, index, &[&cmd_semaphore]);
      frame_context.track_semaphore(cmd_semaphore);
      self.context.end_frame(cmd_fence);
    }
  }
}
