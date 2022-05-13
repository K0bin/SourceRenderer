use std::ffi::c_void;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Weak;

use ash::vk;

use sourcerenderer_core::graphics::TextureDepthStencilView;
use sourcerenderer_core::graphics::TextureRenderTargetView;
use sourcerenderer_core::graphics::TextureUsage;
use sourcerenderer_core::graphics::{AddressMode, Filter, SamplerInfo, Texture, TextureInfo, TextureSamplingView, TextureViewInfo, TextureStorageView};

use crate::bindless::VkBindlessDescriptorSet;
use crate::raw::VkFeatures;
use crate::{VkBackend, raw::RawVkDevice};
use crate::format::format_to_vk;

use crate::pipeline::{samples_to_vk, compare_func_to_vk};
use vk_mem::MemoryUsage;
use std::cmp::max;
use std::hash::{Hash, Hasher};
use std::ffi::CString;
use ash::vk::Handle;

pub struct VkTexture {
  image: vk::Image,
  allocation: Option<vk_mem::Allocation>,
  device: Arc<RawVkDevice>,
  info: TextureInfo,
  bindless_slot: Mutex<Option<VkTextureBindlessSlot>>
}

unsafe impl Send for VkTexture {}
unsafe impl Sync for VkTexture {}

struct VkTextureBindlessSlot {
  bindless_set: Weak<VkBindlessDescriptorSet>,
  slot: u32
}

impl VkTexture {
  pub fn new(device: &Arc<RawVkDevice>, info: &TextureInfo, name: Option<&str>) -> Self {
    let create_info = vk::ImageCreateInfo {
      flags: vk::ImageCreateFlags::empty(),
      tiling: vk::ImageTiling::OPTIMAL,
      initial_layout: vk::ImageLayout::UNDEFINED,
      sharing_mode: vk::SharingMode::EXCLUSIVE,
      usage: texture_usage_to_vk(info.usage),
      image_type: vk::ImageType::TYPE_2D, // FIXME: if info.height <= 1 { vk::ImageType::TYPE_1D } else if info.depth <= 1 { vk::ImageType::TYPE_2D } else { vk::ImageType::TYPE_3D},
      extent: vk::Extent3D {
        width: max(1, info.width),
        height: max(1, info.height),
        depth: max(1, info.depth)
      },
      format: format_to_vk(info.format, device.supports_d24),
      mip_levels: info.mip_levels,
      array_layers: info.array_length,
      samples: samples_to_vk(info.samples),
      ..Default::default()
    };

    let mut props: vk::ImageFormatProperties2 = Default::default();
    unsafe {
      device.instance.get_physical_device_image_format_properties2(device.physical_device, &vk::PhysicalDeviceImageFormatInfo2 {
        format: create_info.format,
        ty: create_info.image_type,
        tiling: create_info.tiling,
        usage: create_info.usage,
        flags: create_info.flags,
        ..Default::default()
      }, &mut props).unwrap()
    };

    let mut alloc_info = vk_mem::AllocationCreateInfo::new();
    alloc_info = alloc_info.usage(MemoryUsage::GpuOnly);
    let (image, allocation, _allocation_info) = unsafe { device.allocator.create_image(&create_info, &alloc_info).unwrap() };
    if let Some(name) = name {
      if let Some(debug_utils) = device.instance.debug_utils.as_ref() {
        let name_cstring = CString::new(name).unwrap();
        unsafe {
          debug_utils.debug_utils_loader.debug_utils_set_object_name(device.handle(), &vk::DebugUtilsObjectNameInfoEXT {
            object_type: vk::ObjectType::IMAGE,
            object_handle: image.as_raw(),
            p_object_name: name_cstring.as_ptr(),
            ..Default::default()
          }).unwrap();
        }
      }
    }
    Self {
      image,
      allocation: Some(allocation),
      device: device.clone(),
      info: info.clone(),
      bindless_slot: Mutex::new(None)
    }
  }

  pub fn from_image(device: &Arc<RawVkDevice>, image: vk::Image, info: TextureInfo) -> Self {
    VkTexture {
      image,
      device: device.clone(),
      info,
      allocation: None,
      bindless_slot: Mutex::new(None)
    }
  }

  pub fn handle(&self) -> &vk::Image {
    &self.image
  }

  pub(crate) fn set_bindless_slot(&self, bindless_set: &Arc<VkBindlessDescriptorSet>, slot: u32) {
    let mut lock = self.bindless_slot.lock().unwrap();
    *lock = Some(VkTextureBindlessSlot {
      bindless_set: Arc::downgrade(bindless_set),
      slot,
    });
  }
}

fn texture_usage_to_vk(usage: TextureUsage) -> vk::ImageUsageFlags {
  let mut flags = vk::ImageUsageFlags::empty();

  if usage.contains(TextureUsage::STORAGE) {
    flags |= vk::ImageUsageFlags::STORAGE;
  }

  if usage.contains(TextureUsage::SAMPLED) {
    flags |= vk::ImageUsageFlags::SAMPLED;
  }

  let transfer_src_usages = TextureUsage::BLIT_SRC
  | TextureUsage::COPY_SRC
  | TextureUsage::RESOLVE_SRC; // TODO: sync2
  if usage.intersects(transfer_src_usages) {
    flags |= vk::ImageUsageFlags::TRANSFER_SRC;
  }

  let transfer_dst_usages = TextureUsage::BLIT_DST
  | TextureUsage::COPY_DST
  | TextureUsage::RESOLVE_DST;
  if usage.intersects(transfer_dst_usages) {
    flags |= vk::ImageUsageFlags::TRANSFER_DST;
  }

  if usage.contains(TextureUsage::DEPTH_STENCIL) {
    flags |= vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT;
  }

  if usage.contains(TextureUsage::RENDER_TARGET) {
    flags |= vk::ImageUsageFlags::COLOR_ATTACHMENT;
  }

  flags
}

impl Drop for VkTexture {
  fn drop(&mut self) {
    let mut bindless_slot = self.bindless_slot.lock().unwrap();
    if let Some(bindless_slot) = bindless_slot.take() {
      let set = bindless_slot.bindless_set.upgrade();
      if let Some(set) = set {
        set.free_slot(bindless_slot.slot);
      }
    }

    if let Some(alloc) = self.allocation {
      unsafe {
        self.device.allocator.destroy_image(self.image, alloc);
      }
    }
  }
}

impl Texture for VkTexture {
  fn info(&self) -> &TextureInfo {
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
    Filter::Nearest => vk::Filter::NEAREST,
    Filter::Max => vk::Filter::LINEAR,
    Filter::Min => vk::Filter::LINEAR,
  }
}
fn filter_to_vk_mip(filter: Filter) -> vk::SamplerMipmapMode {
  match filter {
    Filter::Linear => vk::SamplerMipmapMode::LINEAR,
    Filter::Nearest => vk::SamplerMipmapMode::NEAREST,
    Filter::Max => panic!("Can't use max as mipmap filter."),
    Filter::Min => panic!("Can't use min as mipmap filter."),
  }
}
fn filter_to_reduction_mode(filter: Filter) -> vk::SamplerReductionMode {
  match filter {
    Filter::Max => vk::SamplerReductionMode::MAX,
    Filter::Min => vk::SamplerReductionMode::MIN,
    _ => unreachable!()
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
  texture: Arc<VkTexture>,
  device: Arc<RawVkDevice>,
  info: TextureViewInfo,
}

impl VkTextureView {
  pub(crate) fn new(device: &Arc<RawVkDevice>, texture: &Arc<VkTexture>, info: &TextureViewInfo, name: Option<&str>) -> Self {
    let view_create_info = vk::ImageViewCreateInfo {
      image: *texture.handle(),
      view_type: vk::ImageViewType::TYPE_2D, // FIXME: if texture.info().height <= 1 { vk::ImageViewType::TYPE_1D } else if texture.info().depth <= 1 { vk::ImageViewType::TYPE_2D } else { vk::ImageViewType::TYPE_3D},
      format: format_to_vk(texture.info.format, device.supports_d24),
      components: vk::ComponentMapping {
        r: vk::ComponentSwizzle::IDENTITY,
        g: vk::ComponentSwizzle::IDENTITY,
        b: vk::ComponentSwizzle::IDENTITY,
        a: vk::ComponentSwizzle::IDENTITY,
      },
      subresource_range: vk::ImageSubresourceRange {
        aspect_mask: if texture.info().format.is_depth() {
          vk::ImageAspectFlags::DEPTH
        } else {
          vk::ImageAspectFlags::COLOR
        },
        base_mip_level: info.base_mip_level,
        level_count: info.mip_level_length,
        base_array_layer: info.base_array_layer,
        layer_count: info.array_layer_length
      },
      ..Default::default()
    };
    let view = unsafe {
      device.create_image_view(&view_create_info, None)
    }.unwrap();

    if let Some(name) = name {
      if let Some(debug_utils) = device.instance.debug_utils.as_ref() {
        let name_cstring = CString::new(name).unwrap();
        unsafe {
          debug_utils.debug_utils_loader.debug_utils_set_object_name(device.handle(), &vk::DebugUtilsObjectNameInfoEXT {
            object_type: vk::ObjectType::IMAGE_VIEW,
            object_handle: view.as_raw(),
            p_object_name: name_cstring.as_ptr(),
            ..Default::default()
          }).unwrap();
        }
      }
    }

    Self {
      view,
      texture: texture.clone(),
      device: device.clone(),
      info: info.clone(),
    }
  }

  #[inline]
  pub(crate) fn view_handle(&self) -> &vk::ImageView {
    &self.view
  }

  pub(crate) fn texture(&self) -> &Arc<VkTexture> {
    &self.texture
  }
}

impl Drop for VkTextureView {
  fn drop(&mut self) {
    unsafe {
      self.device.destroy_image_view(self.view, None);
    }
  }
}

impl TextureSamplingView<VkBackend> for VkTextureView {
  fn texture(&self) -> &Arc<VkTexture> {
    &self.texture
  }

  fn info(&self) -> &TextureViewInfo {
    &self.info
  }
}

impl TextureStorageView<VkBackend> for VkTextureView {
  fn texture(&self) -> &Arc<VkTexture> {
    &self.texture
  }

  fn info(&self) -> &TextureViewInfo {
    &self.info
  }
}

impl TextureRenderTargetView<VkBackend> for VkTextureView {
  fn texture(&self) -> &Arc<VkTexture> {
    &self.texture
  }

  fn info(&self) -> &TextureViewInfo {
    &self.info
  }
}

impl TextureDepthStencilView<VkBackend> for VkTextureView {
  fn texture(&self) -> &Arc<VkTexture> {
    &self.texture
  }

  fn info(&self) -> &TextureViewInfo {
    &self.info
  }
}

impl Hash for VkTextureView {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.texture.hash(state);
    self.view.hash(state);
  }
}

impl PartialEq for VkTextureView {
  fn eq(&self, other: &Self) -> bool {
    self.texture == other.texture
    && self.view == other.view
  }
}

impl Eq for VkTextureView {}

pub struct VkSampler {
  sampler: vk::Sampler,
  device: Arc<RawVkDevice>
}

impl VkSampler {
  pub fn new(device: &Arc<RawVkDevice>, info: &SamplerInfo) -> Self {
    let mut sampler_create_info = vk::SamplerCreateInfo {
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
      compare_op: info.compare_op.map_or(vk::CompareOp::ALWAYS, compare_func_to_vk),
      min_lod: info.min_lod,
      max_lod: info.max_lod.unwrap_or(vk::LOD_CLAMP_NONE),
      border_color: vk::BorderColor::INT_OPAQUE_BLACK,
      unnormalized_coordinates: 0,
      ..Default::default()
    };

    let mut sampler_minmax_info = vk::SamplerReductionModeCreateInfo::default();
    if info.min_filter == Filter::Min || info.min_filter == Filter::Max {
      assert!(device.features.contains(VkFeatures::MIN_MAX_FILTER));

      sampler_minmax_info.reduction_mode = filter_to_reduction_mode(info.min_filter);
      sampler_create_info.p_next = &sampler_minmax_info as *const vk::SamplerReductionModeCreateInfo as *const c_void;
    }
    debug_assert_ne!(info.mag_filter, Filter::Min);
    debug_assert_ne!(info.mag_filter, Filter::Max);
    debug_assert_ne!(info.mip_filter, Filter::Min);
    debug_assert_ne!(info.mip_filter, Filter::Max);

    let sampler = unsafe {
      device.create_sampler(&sampler_create_info, None)
    }.unwrap();

    Self {
      sampler,
      device: device.clone()
    }
  }

  #[inline]
  pub(crate) fn handle(&self) -> &vk::Sampler {
    &self.sampler
  }
}

impl Drop for VkSampler {
  fn drop(&mut self) {
    unsafe {
      self.device.destroy_sampler(self.sampler, None);
    }
  }
}

impl Hash for VkSampler {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.sampler.hash(state);
  }
}

impl PartialEq for VkSampler {
  fn eq(&self, other: &Self) -> bool {
    self.sampler == other.sampler
  }
}

impl Eq for VkSampler {}
