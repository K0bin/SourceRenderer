
use std::sync::Arc;
use std::hash::{Hash, Hasher};

use ash::vk::{self, FramebufferCreateFlags, FramebufferCreateInfo};
use ash::version::DeviceV1_0;
use smallvec::SmallVec;

use crate::{raw::RawVkDevice, texture::VkTextureView};

pub struct VkRenderPass {
  device: Arc<RawVkDevice>,
  render_pass: vk::RenderPass
}

impl VkRenderPass {
  pub fn new(device: &Arc<RawVkDevice>, info: &vk::RenderPassCreateInfo) -> Self {
    Self {
      device: device.clone(),
      render_pass: unsafe { device.create_render_pass(info, None).unwrap() }
    }
  }

  pub fn get_handle(&self) -> &vk::RenderPass {
    &self.render_pass
  }
}

impl Drop for VkRenderPass {
  fn drop(&mut self) {
    unsafe {
      self.device.destroy_render_pass(self.render_pass, None);
    }
  }
}

impl Hash for VkRenderPass {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.render_pass.hash(state);
  }
}

impl PartialEq for VkRenderPass {
  fn eq(&self, other: &Self) -> bool {
    self.render_pass == other.render_pass
  }
}

impl Eq for VkRenderPass {}

pub struct VkFrameBuffer {
  device: Arc<RawVkDevice>,
  frame_buffer: vk::Framebuffer,
  width: u32,
  height: u32,
  render_pass: Arc<VkRenderPass>,
  attachments: SmallVec<[Arc<VkTextureView>; 8]>
}

impl VkFrameBuffer {
  pub(crate) fn new(device: &Arc<RawVkDevice>, width: u32, height: u32, render_pass: &Arc<VkRenderPass>, attachments: &[&Arc<VkTextureView>]) -> Self {
    let mut vk_attachments = SmallVec::<[vk::ImageView; 8]>::new();
    let mut attachment_refs = SmallVec::<[Arc<VkTextureView>; 8]>::new();
    for attachment in attachments {
      vk_attachments.push(*attachment.get_view_handle());
      attachment_refs.push((*attachment).clone());
    }

    Self {
      device: device.clone(),
      frame_buffer: unsafe { device.create_framebuffer(&vk::FramebufferCreateInfo {
          flags: vk::FramebufferCreateFlags::empty(),
          render_pass: *render_pass.get_handle(),
          attachment_count: vk_attachments.len() as u32,
          p_attachments: vk_attachments.as_ptr(),
          width: width,
          height: height,
          layers: 1,
          ..Default::default()
      }, None).unwrap() },
      width: width,
      height: height,
      attachments: attachment_refs,
      render_pass: render_pass.clone()
    }
  }

  pub(crate) fn get_handle(&self) -> &vk::Framebuffer {
    &self.frame_buffer
  }

  pub(crate) fn width(&self) -> u32 {
    self.width
  }

  pub(crate) fn height(&self) -> u32 {
    self.height
  }
}

impl Drop for VkFrameBuffer {
  fn drop(&mut self) {
    unsafe {
      self.device.destroy_framebuffer(self.frame_buffer, None);
    }
  }
}
