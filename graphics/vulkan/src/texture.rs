use std::sync::Arc;

use ash::vk;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::Texture;
use sourcerenderer_core::graphics::Format;
use sourcerenderer_core::graphics::RenderTargetView;

use crate::VkDevice;
use crate::raw::RawVkDevice;
use crate::format::format_to_vk;
use crate::VkBackend;

pub struct VkTexture {
  image: vk::Image,
  device: Arc<RawVkDevice>,
  format: Format,
  width: u32,
  height: u32,
  depth: u32,
  mip_levels: u32,
  array_length: u32,
  borrowed: bool
}

pub struct VkRenderTargetView {
  texture: Arc<VkTexture>,
  view: vk::ImageView,
  device: Arc<RawVkDevice>
}

impl VkTexture {
  pub fn new(device: Arc<RawVkDevice>) -> Self {
    unimplemented!();
  }

  pub fn from_image(device: &Arc<RawVkDevice>, image: vk::Image, format: Format, width: u32, height: u32, depth: u32, mip_levels: u32, array_length: u32) -> Self {
    return VkTexture {
      image,
      device: device.clone(),
      format,
      width,
      height,
      depth,
      mip_levels,
      array_length,
      borrowed: true
    };
  }

  pub fn get_handle(&self) -> &vk::Image {
    return &self.image;
  }
}

impl Texture for VkTexture {

}

impl VkRenderTargetView {
  pub fn new(device: &Arc<RawVkDevice>, texture: Arc<VkTexture>) -> Self {
    let vk_device = &device.device;
    let info = vk::ImageViewCreateInfo {
      image: *texture.get_handle(),
      view_type: if texture.depth > 1 { vk::ImageViewType::TYPE_3D } else { vk::ImageViewType::TYPE_2D },
      format: format_to_vk(texture.format),
      components: vk::ComponentMapping {
        r: vk::ComponentSwizzle::IDENTITY,
        g: vk::ComponentSwizzle::IDENTITY,
        b: vk::ComponentSwizzle::IDENTITY,
        a: vk::ComponentSwizzle::IDENTITY,
      },
      subresource_range: vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1
      },
      ..Default::default()
    };
    let view = unsafe { vk_device.create_image_view(&info, None).unwrap() };
    return VkRenderTargetView {
      texture,
      view,
      device: device.clone()
    };
  }

  pub fn get_handle(&self) -> &vk::ImageView {
    return &self.view;
  }
}

impl RenderTargetView<VkBackend> for VkRenderTargetView {
  fn get_texture(&self) -> Arc<VkTexture> {
    return self.texture.clone();
  }
}
