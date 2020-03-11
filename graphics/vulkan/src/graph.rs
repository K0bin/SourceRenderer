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

use crate::VkBackend;
use crate::VkDevice;
use crate::raw::RawVkDevice;
use crate::VkSwapchain;
use crate::format::format_to_vk;
use crate::pipeline::samples_to_vk;
use sourcerenderer_core::graphics::{Backend, CommandPool, CommandBuffer, CommandBufferType, Resettable};
use context::VkGraphicsContext;

pub struct VkAttachment {
  texture: vk::Image,
  view: vk::ImageView
}

pub struct VkRenderGraph {
  device: Arc<RawVkDevice>,
  context: Arc<VkGraphicsContext>,
  passes: Vec<VkRenderGraphPass>,
  attachments: HashMap<String, VkAttachment>
}

pub struct VkRenderGraphPass { // TODO rename to VkRenderPass
  device: Arc<RawVkDevice>,
  render_pass: vk::RenderPass,
  frame_buffer: Vec<vk::Framebuffer>,
  callback: Arc<dyn Fn(usize, bool) -> usize>
}

impl VkRenderGraph {
  pub fn new(device: &Arc<RawVkDevice>, context: &Arc<VkGraphicsContext>, info: &RenderGraphInfo, swapchain: &VkSwapchain) -> Self {

    // SHORTTERM
    // TODO: allocate images & image views
    // TODO: add render callback
    // TODO: allocate command pool & buffers
    // TODO: lazily create frame buffer for swapchain images

    // LONGTERM
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

    let passes: Vec<VkRenderGraphPass> = info.passes.iter().map(|p| {
      let vk_device = &device.device;

      let mut render_pass_attachments: Vec<vk::AttachmentDescription> = Vec::new();
      let mut attachment_indices: HashMap<&str, u32> = HashMap::new();
      let mut dependencies: Vec<vk::SubpassDependency> = Vec::new();
      for (key, a) in &info.attachments {
        if p.outputs.iter().any(|o| &o.name == key) {
          let index = render_pass_attachments.len() as u32;
          render_pass_attachments.push(
            vk::AttachmentDescription {
              format: format_to_vk(a.format),
              samples: samples_to_vk(a.samples),
              load_op: vk::AttachmentLoadOp::CLEAR,
              store_op: vk::AttachmentStoreOp::STORE,
              stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
              stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
              initial_layout: *layouts.get(&key as &str).unwrap_or(&vk::ImageLayout::UNDEFINED),
              final_layout: if (key == BACK_BUFFER_ATTACHMENT_NAME) { vk::ImageLayout::PRESENT_SRC_KHR } else { vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL },
              ..Default::default()
            }
          );
          layouts.insert(&key as &str, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
          attachment_indices.insert(&key as &str, index);
        } else if p.inputs.iter().any(|i| &i.name == key) {
          let index = render_pass_attachments.len() as u32;
          let previous_layout = *layouts.get(&key as &str).unwrap_or(&vk::ImageLayout::UNDEFINED);
          render_pass_attachments.push(
            vk::AttachmentDescription {
              format: format_to_vk(a.format),
              samples: samples_to_vk(a.samples),
              load_op: vk::AttachmentLoadOp::LOAD,
              store_op: vk::AttachmentStoreOp::STORE,
              stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
              stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
              initial_layout: previous_layout,
              final_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
              ..Default::default()
            }
          );
          attachment_indices.insert(&key as &str, index);
          dependencies.push(vk::SubpassDependency {
            src_subpass: vk::SUBPASS_EXTERNAL,
            dst_subpass: 1,
            src_stage_mask: vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            dst_stage_mask: vk::PipelineStageFlags::TOP_OF_PIPE,
            src_access_mask: match previous_layout {
              vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL => vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
              vk::ImageLayout::UNDEFINED => vk::AccessFlags::empty(),
              _ => vk::AccessFlags::SHADER_READ
            },
            dst_access_mask: vk::AccessFlags::SHADER_READ,
            ..Default::default()
          });
          layouts.insert(&key as &str, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        }
      }

      let input_attachments: Vec<vk::AttachmentReference> = p.inputs
        .iter()
        .map(|i| vk::AttachmentReference {
          attachment: attachment_indices[&i.name as &str] as u32,
          layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        })
        .collect();

      let output_attachments: Vec<vk::AttachmentReference> = p.outputs
        .iter()
        .map(|i| vk::AttachmentReference {
          attachment: attachment_indices[&i.name as &str] as u32,
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
      let render_pass = unsafe { vk_device.create_render_pass(&render_pass_create_info, None).unwrap() };

      let mut frame_buffers: Vec<vk::Framebuffer> = Vec::new();
      let swapchain_views = swapchain.get_views();
      let frame_buffer_count = if p.outputs.iter().any(|o| o.name == BACK_BUFFER_ATTACHMENT_NAME) {
        1
      } else {
        swapchain_views.len()
      };
      for i in 0..frame_buffer_count {
        let frame_buffer_attachments: Vec<vk::ImageView> = p.outputs.iter().map(|a| if a.name == BACK_BUFFER_ATTACHMENT_NAME {
          swapchain_views[i]
        } else {
          attachments[&a.name as &str].view
        }).collect();

        let (width, height) = if p.outputs[0].name == BACK_BUFFER_ATTACHMENT_NAME {
          (swapchain.get_width(), swapchain.get_height())
        } else {
          let attachment_info = &info.attachments[&p.outputs[0].name as &str];
          (attachment_info.width as u32, attachment_info.height as u32)
        };

        let frame_buffer_info = vk::FramebufferCreateInfo {
          render_pass,
          attachment_count: frame_buffer_attachments.len() as u32,
          p_attachments: frame_buffer_attachments.as_ptr(),
          layers: 1,
          width,
          height,
          ..Default::default()
        };
        let frame_buffer = unsafe { vk_device.create_framebuffer(&frame_buffer_info, None).unwrap() };
        frame_buffers.push(frame_buffer);
      }

      VkRenderGraphPass {
        device: device.clone(),
        frame_buffer: frame_buffers,
        render_pass,
        callback: p.render.clone()
      }
    }).collect();

    return VkRenderGraph {
      device: device.clone(),
      context: context.clone(),
      passes,
      attachments
    };
  }
}

impl RenderGraph<VkBackend> for VkRenderGraph {
  fn recreate(&mut self, swap_chain: &VkSwapchain) {

  }

  fn render(&mut self, frame_index: u64) {
    let thread_context = self.context.get_thread_context();
    let mut frame_context = thread_context.get_frame_context(frame_index);
    let pool = frame_context.get_command_pool();
    let cmd_buffer = pool.get_command_buffer(CommandBufferType::PRIMARY);
    let secondary = pool.get_command_buffer(CommandBufferType::SECONDARY);
    pool.reset();
    for pass in &self.passes {
      //cmd_buffer.begin_render_pass(pass.render_pass)
    }
    unimplemented!()
  }
}

impl Drop for VkRenderGraphPass {
  fn drop(&mut self) {
    let vk_device = &self.device.device;
    unsafe {
      vk_device.destroy_render_pass(self.render_pass, None);
      self.frame_buffer.iter().for_each(|&f| vk_device.destroy_framebuffer(f, None));
    }
  }
}
