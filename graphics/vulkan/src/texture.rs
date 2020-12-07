use std::sync::Arc;

use ash::vk;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::{Texture, TextureInfo, TextureShaderResourceView, TextureShaderResourceViewInfo, Filter, AddressMode};

use crate::raw::RawVkDevice;
use crate::format::format_to_vk;

use pipeline::{samples_to_vk, compare_func_to_vk};
use vk_mem::MemoryUsage;
use std::cmp::max;
use std::hash::{Hash, Hasher};
use std::ffi::CString;
use ash::vk::Handle;

pub struct VkTexture {
  image: vk::Image,
  allocation: Option<vk_mem::Allocation>,
  device: Arc<RawVkDevice>,
  info: TextureInfo
}

impl VkTexture {
  pub fn new(device: &Arc<RawVkDevice>, info: &TextureInfo, name: Option<&str>, usage: vk::ImageUsageFlags) -> Self {
    let create_info = vk::ImageCreateInfo {
      flags: vk::ImageCreateFlags::empty(),
      tiling: vk::ImageTiling::OPTIMAL,
      initial_layout: vk::ImageLayout::UNDEFINED,
      sharing_mode: vk::SharingMode::EXCLUSIVE,
      usage,
      image_type: vk::ImageType::TYPE_2D, // FIXME: if info.height <= 1 { vk::ImageType::TYPE_1D } else if info.depth <= 1 { vk::ImageType::TYPE_2D } else { vk::ImageType::TYPE_3D},
      extent: vk::Extent3D {
        width: max(1, info.width),
        height: max(1, info.height),
        depth: max(1, info.depth)
      },
      format: format_to_vk(info.format),
      mip_levels: info.mip_levels,
      array_layers: info.array_length,
      samples: samples_to_vk(info.samples),
      ..Default::default()
    };
    let alloc_info = vk_mem::AllocationCreateInfo {
      usage: MemoryUsage::GpuOnly,
      ..Default::default()
    };
    let (image, allocation, _allocation_info) = device.allocator.create_image(&create_info, &alloc_info).unwrap();
    if let Some(name) = name {
      let name_cstring = CString::new(name).unwrap();
      unsafe {
        device.instance.debug_utils_loader.debug_utils_set_object_name(device.handle(), &vk::DebugUtilsObjectNameInfoEXT {
          object_type: vk::ObjectType::IMAGE,
          object_handle: image.as_raw(),
          p_object_name: name_cstring.as_ptr(),
          ..Default::default()
        });
      }
    }
    Self {
      image,
      allocation: Some(allocation),
      device: device.clone(),
      info: info.clone(),
    }
  }

  pub fn from_image(device: &Arc<RawVkDevice>, image: vk::Image, info: TextureInfo) -> Self {
    return VkTexture {
      image,
      device: device.clone(),
      info,
      allocation: None
    };
  }

  pub fn get_handle(&self) -> &vk::Image {
    return &self.image;
  }
}

impl Drop for VkTexture {
  fn drop(&mut self) {
    if let Some(alloc) = &self.allocation {
      self.device.allocator.destroy_image(self.image, alloc);
    }
  }
}

impl Texture for VkTexture {
  fn get_info(&self) -> &TextureInfo {
    &self.info
  }
}

impl Hash for VkTexture {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.image.hash(state);
  }
}

impl PartialEq for VkTexture {
  fn eq(&self, other: &Self) -> bool {
    self.image == other.image
  }
}

impl Eq for VkTexture {}

fn filter_to_vk(filter: Filter) -> vk::Filter {
  match filter {
    Filter::Linear => vk::Filter::LINEAR,
    Filter::Nearest => vk::Filter::NEAREST
  }
}
fn filter_to_vk_mip(filter: Filter) -> vk::SamplerMipmapMode {
  match filter {
    Filter::Linear => vk::SamplerMipmapMode::LINEAR,
    Filter::Nearest => vk::SamplerMipmapMode::NEAREST
  }
}

fn address_mode_to_vk(address_mode: AddressMode) -> vk::SamplerAddressMode {
  match address_mode {
    AddressMode::Repeat => vk::SamplerAddressMode::REPEAT,
    AddressMode::ClampToBorder => vk::SamplerAddressMode::CLAMP_TO_BORDER,
    AddressMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
    AddressMode::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
  }
}

pub struct VkTextureView {
  view: vk::ImageView,
  sampler: Option<vk::Sampler>,
  texture: Arc<VkTexture>,
  device: Arc<RawVkDevice>
}

impl VkTextureView {
  pub(crate) fn new_shader_resource_view(device: &Arc<RawVkDevice>, texture: &Arc<VkTexture>, info: &TextureShaderResourceViewInfo) -> Self {
    let view_create_info = vk::ImageViewCreateInfo {
      image: *texture.get_handle(),
      view_type: vk::ImageViewType::TYPE_2D, // FIXME: if texture.get_info().height <= 1 { vk::ImageViewType::TYPE_1D } else if texture.get_info().depth <= 1 { vk::ImageViewType::TYPE_2D } else { vk::ImageViewType::TYPE_3D},
      format: format_to_vk(texture.info.format),
      components: vk::ComponentMapping {
        r: vk::ComponentSwizzle::IDENTITY,
        g: vk::ComponentSwizzle::IDENTITY,
        b: vk::ComponentSwizzle::IDENTITY,
        a: vk::ComponentSwizzle::IDENTITY,
      },
      subresource_range: vk::ImageSubresourceRange {
        aspect_mask: if texture.get_info().format.is_depth() && texture.info.format.is_stencil() {
          vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
        } else if texture.get_info().format.is_depth() {
          vk::ImageAspectFlags::DEPTH
        } else {
          vk::ImageAspectFlags::COLOR
        },
        base_mip_level: info.base_mip_level,
        level_count: info.mip_level_length,
        base_array_layer: info.base_array_level,
        layer_count: info.array_level_length
      },
      ..Default::default()
    };
    let view = unsafe {
      device.create_image_view(&view_create_info, None)
    }.unwrap();

    let sampler_create_info = vk::SamplerCreateInfo {
      mag_filter: filter_to_vk(info.mag_filter),
      min_filter: filter_to_vk(info.mag_filter),
      mipmap_mode: filter_to_vk_mip(info.mip_filter),
      address_mode_u: address_mode_to_vk(info.address_mode_u),
      address_mode_v: address_mode_to_vk(info.address_mode_v),
      address_mode_w: address_mode_to_vk(info.address_mode_u),
      mip_lod_bias: info.mip_bias,
      anisotropy_enable: (info.max_anisotropy.abs() >= 1.0f32) as u32,
      max_anisotropy: info.max_anisotropy,
      compare_enable: info.compare_op.is_some() as u32,
      compare_op: info.compare_op.map_or(vk::CompareOp::ALWAYS, |comp| compare_func_to_vk(comp)),
      min_lod: info.min_lod,
      max_lod: info.max_lod,
      border_color: vk::BorderColor::INT_OPAQUE_BLACK,
      unnormalized_coordinates: 0,
      ..Default::default()
    };
    let sampler = unsafe {
      device.create_sampler(&sampler_create_info, None)
    }.unwrap();

    Self {
      view,
      sampler: Some(sampler),
      texture: texture.clone(),
      device: device.clone()
    }
  }

  pub(crate) fn new_attachment_view(device: &Arc<RawVkDevice>, texture: &Arc<VkTexture>) -> Self {
    let info = texture.get_info();
    let vk_info = vk::ImageViewCreateInfo {
      image: *texture.get_handle(),
      view_type: if texture.get_info().height <= 1 { vk::ImageViewType::TYPE_1D } else if texture.get_info().depth <= 1 { vk::ImageViewType::TYPE_2D } else { vk::ImageViewType::TYPE_3D},
      format: format_to_vk(info.format),
      components: vk::ComponentMapping {
        r: vk::ComponentSwizzle::IDENTITY,
        g: vk::ComponentSwizzle::IDENTITY,
        b: vk::ComponentSwizzle::IDENTITY,
        a: vk::ComponentSwizzle::IDENTITY,
      },
      subresource_range: vk::ImageSubresourceRange {
        aspect_mask: if texture.get_info().format.is_depth() && texture.info.format.is_stencil() {
          vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
        } else if texture.get_info().format.is_depth() {
          vk::ImageAspectFlags::DEPTH
        } else {
          vk::ImageAspectFlags::COLOR
        },
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1
      },
      ..Default::default()
    };
    let view = unsafe { device.create_image_view(&vk_info, None).unwrap() };
    return VkTextureView {
      texture: texture.clone(),
      view,
      sampler: None,
      device: device.clone()
    };
  }

  #[inline]
  pub(crate) fn get_view_handle(&self) -> &vk::ImageView {
    &self.view
  }

  #[inline]
  pub(crate) fn get_sampler_handle(&self) -> Option<&vk::Sampler> {
    self.sampler.as_ref()
  }

  pub(crate) fn texture(&self) -> &Arc<VkTexture> {
    &self.texture
  }
}

impl Drop for VkTextureView {
  fn drop(&mut self) {
    unsafe {
      self.device.destroy_image_view(self.view, None);
      if let Some(sampler) = self.sampler {
        self.device.destroy_sampler(sampler, None);
      }
    }
  }
}

impl TextureShaderResourceView for VkTextureView {}

impl Hash for VkTextureView {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.texture.hash(state);
    self.view.hash(state);
    self.sampler.hash(state);
  }
}

impl PartialEq for VkTextureView {
  fn eq(&self, other: &Self) -> bool {
    self.texture == other.texture
    && self.view == other.view
    && self.sampler == other.sampler
  }
}

impl Eq for VkTextureView {}
