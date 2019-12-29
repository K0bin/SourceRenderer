use crate::texture::VkRenderTargetView;
use std::sync::Arc;

use ash::vk;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::*;

use crate::VkDevice;
use crate::pipeline::samples_to_vk;
use crate::format::format_to_vk;
use crate::VkBackend;

fn store_op_to_vk(store_op: StoreOp) -> vk::AttachmentStoreOp {
  return match store_op {
    StoreOp::DontCare => vk::AttachmentStoreOp::DONT_CARE,
    StoreOp::Store => vk::AttachmentStoreOp::STORE,
  };
}

fn load_op_to_vk(load_op: LoadOp) -> vk::AttachmentLoadOp {
  return match load_op {
    LoadOp::Clear => vk::AttachmentLoadOp::CLEAR,
    LoadOp::DontCare => vk::AttachmentLoadOp::DONT_CARE,
    LoadOp::Load => vk::AttachmentLoadOp::LOAD
  };
}

fn image_layout_to_vk(image_layout: ImageLayout) -> vk::ImageLayout {
  return match image_layout {
    ImageLayout::Common => vk::ImageLayout::GENERAL,
    ImageLayout::CopyDstOptimal => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
    ImageLayout::CopySrcOptimal => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
    ImageLayout::DepthRead => vk::ImageLayout::DEPTH_READ_ONLY_STENCIL_ATTACHMENT_OPTIMAL,
    ImageLayout::DepthWrite => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
    ImageLayout::Present => vk::ImageLayout::PRESENT_SRC_KHR,
    ImageLayout::RenderTarget => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    ImageLayout::ShaderResource => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    ImageLayout::Undefined => vk::ImageLayout::UNDEFINED
  };
}

pub struct VkRenderPassLayout {
  renderpass: vk::RenderPass,
  device: Arc<VkDevice>
}

pub struct VkRenderPass {
  layout: Arc<VkRenderPassLayout>,
  device: Arc<VkDevice>,
  framebuffer: vk::Framebuffer,
  info: RenderPassInfo<VkBackend>
}

impl VkRenderPassLayout {
  pub fn new(device: Arc<VkDevice>, info: &RenderPassLayoutInfo) -> VkRenderPassLayout {
    let vk_device = device.get_ash_device();

    let mut renderpass_attachments: Vec<vk::AttachmentDescription> = Vec::new();
    for attachment in &info.attachments {
      renderpass_attachments.push(vk::AttachmentDescription {
        load_op: load_op_to_vk(attachment.load_op),
        store_op: store_op_to_vk(attachment.store_op),
        stencil_load_op: load_op_to_vk(attachment.stencil_load_op),
        stencil_store_op: store_op_to_vk(attachment.stencil_store_op),
        samples: samples_to_vk(attachment.samples),
        format: format_to_vk(attachment.format),
        initial_layout: image_layout_to_vk(attachment.initial_layout),
        final_layout: image_layout_to_vk(attachment.final_layout),
        ..Default::default()
      });
    }

    let mut subpasses: Vec<vk::SubpassDescription> = Vec::new();
    for subpass in &info.subpasses  {
      let mut input_references: Vec<vk::AttachmentReference> = Vec::new();
      for reference in &subpass.input_attachments {
        input_references.push(vk::AttachmentReference {
          attachment: reference.index,
          layout: image_layout_to_vk(reference.layout)
        });
      }
      let mut output_references: Vec<vk::AttachmentReference> = Vec::new();
      for reference in &subpass.output_color_attachments {
        output_references.push(vk::AttachmentReference {
          attachment: reference.index,
          layout: image_layout_to_vk(reference.layout)
        });
      }
      let mut resolve_references: Vec<vk::AttachmentReference> = Vec::new();
      for reference in &subpass.output_resolve_attachments {
        resolve_references.push(vk::AttachmentReference {
          attachment: reference.index,
          layout: image_layout_to_vk(reference.layout)
        });
      }
      let mut preserved_references: Vec<u32> = Vec::new();
      for reference in &subpass.preserve_unused_attachments {
        preserved_references.push(*reference);
      }

      let depth_stencil_reference = subpass.depth_stencil_attachment.as_ref().map(|ref reference| {
        vk::AttachmentReference {
          attachment: reference.index,
          layout: image_layout_to_vk(reference.layout)
        }
      });

      subpasses.push(vk::SubpassDescription {
        p_input_attachments: input_references.as_ptr(),
        input_attachment_count: input_references.len() as u32,
        p_color_attachments: output_references.as_ptr(),
        color_attachment_count: output_references.len() as u32,
        p_resolve_attachments: if resolve_references.is_empty() { std::ptr::null() } else { resolve_references.as_ptr() },
        p_preserve_attachments: preserved_references.as_ptr(),
        preserve_attachment_count: preserved_references.len() as u32,
        p_depth_stencil_attachment: if let Some(reference) = depth_stencil_reference { &reference } else { std::ptr::null() },
        ..Default::default()
      });
    }
    let renderpass_create_info = vk::RenderPassCreateInfo {
      p_attachments: renderpass_attachments.as_ptr(),
      attachment_count: renderpass_attachments.len() as u32,
      p_subpasses: subpasses.as_ptr(),
      subpass_count: subpasses.len() as u32,
      ..Default::default()
    };
    let renderpass = unsafe { vk_device.create_render_pass(&renderpass_create_info, None).unwrap() };

    return VkRenderPassLayout {
      renderpass: renderpass,
      device: device
    };
  }

  pub fn get_handle(&self) -> &vk::RenderPass {
    return &self.renderpass;
  }
}

impl Drop for VkRenderPassLayout {
  fn drop(&mut self) {
    let vk_device = self.device.get_ash_device();
    unsafe {
      vk_device.destroy_render_pass(self.renderpass, None);
    }
  }
}

impl RenderPassLayout<VkBackend> for VkRenderPassLayout {

}

impl VkRenderPass {
  pub fn new(device: Arc<VkDevice>, info: &RenderPassInfo<VkBackend>) -> Self {
    let vk_device = device.get_ash_device();
    let vk_layout = unsafe { Arc::from_raw(Arc::into_raw(info.layout.clone()) as *const VkRenderPassLayout) };
    let attachments: Vec<vk::ImageView> = info.attachments
      .iter()
      .map(|attachment| {
        unsafe { *Arc::from_raw(Arc::into_raw(attachment.clone()) as *const VkRenderTargetView).get_handle() }
      })
      .collect();
    let create_info = vk::FramebufferCreateInfo {
      width: info.width,
      height: info.height,
      layers: info.array_length,
      render_pass: *vk_layout.get_handle(),
      p_attachments: attachments.as_ptr(),
      attachment_count: attachments.len() as u32,
      ..Default::default()
    };
    let framebuffer = unsafe { vk_device.create_framebuffer(&create_info, None).unwrap() };
    return VkRenderPass {
      device: device,
      framebuffer: framebuffer,
      layout: vk_layout,
      info: RenderPassInfo {
        layout: info.layout.clone(),
        attachments: info.attachments.clone(),
        width: info.width,
        height: info.height,
        array_length: info.array_length
      }
    }
  }

  pub fn get_framebuffer(&self) -> &vk::Framebuffer {
    return &self.framebuffer;
  }
}

impl Drop for VkRenderPass {
  fn drop(&mut self) {
    let vk_device = self.device.get_ash_device();
    unsafe {
      vk_device.destroy_framebuffer(self.framebuffer, None);
    }
  }
}

impl RenderPass<VkBackend> for VkRenderPass {
  fn get_info(&self) -> &RenderPassInfo<VkBackend> {
    return &self.info;
  }

  fn get_layout(&self) -> Arc<VkRenderPassLayout> {
    return self.layout.clone() as Arc<VkRenderPassLayout>;
  }
}
