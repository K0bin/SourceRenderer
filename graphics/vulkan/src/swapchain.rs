use std::sync::Arc;
use std::cmp::{min, max};
use std::sync::atomic::{AtomicU32, Ordering};

use crossbeam_utils::atomic::AtomicCell;

use ash::vk;
use ash::extensions::khr::Swapchain as SwapchainLoader;

use sourcerenderer_core::graphics::{Swapchain, TextureInfo, SampleCount, SwapchainError, Backend};
use sourcerenderer_core::graphics::Texture;
use sourcerenderer_core::graphics::Format;

use crate::{VkSurface, VkBackend};
use crate::raw::{RawVkInstance, RawVkDevice};
use crate::VkTexture;
use crate::VkSemaphore;
use crate::texture::VkTextureView;

use ash::prelude::VkResult;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum VkSwapchainState {
  Okay,
  Suboptimal,
  OutOfDate,
  SurfaceLost,
  Retired
}

pub struct VkSwapchain {
  textures: Vec<Arc<VkTexture>>,
  views: Vec<Arc<VkTextureView>>,
  swapchain: vk::SwapchainKHR,
  swapchain_loader: SwapchainLoader,
  instance: Arc<RawVkInstance>,
  surface: Arc<VkSurface>,
  device: Arc<RawVkDevice>,
  vsync: bool,
  state: AtomicCell<VkSwapchainState>,
  acquired_image: AtomicU32,
  presented_image: AtomicU32,
}

impl VkSwapchain {
  fn new_internal(vsync: bool, width: u32, height: u32, device: &Arc<RawVkDevice>, surface: &Arc<VkSurface>, old_swapchain: Option<&Self>) -> Result<Arc<Self>, SwapchainError> {
    let vk_device = &device.device;
    let instance = &device.instance;

    return unsafe {
      let physical_device = device.physical_device;
      let present_modes = match surface.get_present_modes(&physical_device) {
        Ok(present_modes) => present_modes,
        Err(_e) => return Err(SwapchainError::SurfaceLost)
      };
      let present_mode = VkSwapchain::pick_present_mode(vsync, present_modes);
      let swapchain_loader = SwapchainLoader::new(&instance.instance, vk_device);

      let capabilities = match surface.get_capabilities(&physical_device) {
        Ok(capabilities) => capabilities,
        Err(_e) => return Err(SwapchainError::SurfaceLost)
      };
      let formats = match surface.get_formats(&physical_device) {
        Ok(format) => format,
        Err(_e) => return Err(SwapchainError::SurfaceLost)
      };
      let format = VkSwapchain::pick_format(&formats);

      let (width, height) = VkSwapchain::pick_extent(&capabilities, width, height);
      let extent = vk::Extent2D {
        width,
        height
      };

      if width == 0 || height == 0 {
        return Err(SwapchainError::ZeroExtents);
      }

      if !capabilities.supported_usage_flags.contains(vk::ImageUsageFlags::COLOR_ATTACHMENT) {
        panic!("Rendering to the surface is not supported.");
      }

      let image_count = VkSwapchain::pick_image_count(&capabilities, 3);

      let swapchain_create_info = vk::SwapchainCreateInfoKHR {
        surface: *surface.get_surface_handle(),
        min_image_count: image_count,
        image_format: format.format,
        image_color_space: format.color_space,
        image_extent: extent,
        image_array_layers: 1,
        image_usage: vk::ImageUsageFlags::COLOR_ATTACHMENT,
        present_mode,
        image_sharing_mode: vk::SharingMode::EXCLUSIVE,
        pre_transform: capabilities.current_transform,
        composite_alpha: if capabilities.supported_composite_alpha.contains(vk::CompositeAlphaFlagsKHR::OPAQUE) {
          vk::CompositeAlphaFlagsKHR::OPAQUE
        } else {
          vk::CompositeAlphaFlagsKHR::INHERIT
        },
        clipped: vk::TRUE,
        old_swapchain: old_swapchain.map_or(vk::SwapchainKHR::null(), |swapchain| *swapchain.get_handle()),
        ..Default::default()
      };

      if let Some(old_swapchain) = old_swapchain {
        old_swapchain.set_state(VkSwapchainState::Retired);
      }

      let swapchain = swapchain_loader.create_swapchain(&swapchain_create_info, None).map_err(|e|
      match e {
        vk::Result::ERROR_SURFACE_LOST_KHR => SwapchainError::SurfaceLost,
        _ => SwapchainError::Other
      })?;
      let swapchain_images = swapchain_loader.get_swapchain_images(swapchain).unwrap();
      let textures: Vec<Arc<VkTexture>> = swapchain_images
        .iter()
        .map(|image|
          Arc::new(VkTexture::from_image(device, *image, TextureInfo {
            format: surface_vk_format_to_core(format.format),
            width,
            height,
            array_length: 1u32,
            mip_levels: 1u32,
            depth: 1u32,
            samples: SampleCount::Samples1
          })))
        .collect();

      let swapchain_image_views: Vec<Arc<VkTextureView>> = textures
        .iter()
        .map(|texture| {
          Arc::new(VkTextureView::new_attachment_view(device, texture))
        })
        .collect();

      println!("New swapchain!");
      Ok(Arc::new(VkSwapchain {
        textures,
        views: swapchain_image_views,
        swapchain,
        swapchain_loader,
        instance: device.instance.clone(),
        surface: surface.clone(),
        device: device.clone(),
        vsync,
        state: AtomicCell::new(VkSwapchainState::Okay),
        presented_image: AtomicU32::new(0),
        acquired_image: AtomicU32::new(0)
      }))
    }
  }

  pub fn new(vsync: bool, width: u32, height: u32, device: &Arc<RawVkDevice>, surface: &Arc<VkSurface>) -> Result<Arc<Self>, SwapchainError> {
    VkSwapchain::new_internal(vsync, width, height, device, surface, None)
  }

  pub fn pick_extent(capabilities: &vk::SurfaceCapabilitiesKHR, preferred_width: u32, preferred_height: u32) -> (u32, u32) {
    if capabilities.current_extent.width != u32::MAX && capabilities.current_extent.height != u32::MAX {
      (capabilities.current_extent.width, capabilities.current_extent.height)
    } else {
      (
        min(max(preferred_width, capabilities.min_image_extent.width), capabilities.max_image_extent.width),
        min(max(preferred_height, capabilities.min_image_extent.height), capabilities.max_image_extent.height)
      )
    }
  }

  pub fn pick_format(formats: &[vk::SurfaceFormatKHR]) -> vk::SurfaceFormatKHR {
    return if formats.len() == 1 && formats[0].format == vk::Format::UNDEFINED {
      vk::SurfaceFormatKHR {
        format: vk::Format::B8G8R8A8_UNORM,
        color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR
      }
    } else {
      *formats
        .iter()
        .find(|&format|
          (format.format == vk::Format::B8G8R8A8_UNORM && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR)
          || (format.format == vk::Format::R8G8B8A8_UNORM && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR)
        )
        .expect("No compatible format found")
    }
  }

  pub fn pick_image_count(capabilities: &vk::SurfaceCapabilitiesKHR, preferred: u32) -> u32 {
    let mut image_count = max(capabilities.min_image_count + 1, preferred);
    if capabilities.max_image_count != 0 {
      image_count = min(capabilities.max_image_count, image_count);
    }
    image_count
  }

  unsafe fn pick_present_mode(vsync: bool, present_modes: Vec<vk::PresentModeKHR>) -> vk::PresentModeKHR {
    if !vsync {
      if let Some(mode) = present_modes
        .iter()
        .filter(|&&mode| mode == vk::PresentModeKHR::IMMEDIATE)
        .nth(0) {
        return *mode;
      }
    }

    return *present_modes
      .iter()
      .filter(|&&mode| mode == vk::PresentModeKHR::FIFO)
      .nth(0).expect("No compatible present mode found");
  }

  pub fn get_loader(&self) -> &SwapchainLoader {
    return &self.swapchain_loader;
  }

  pub fn get_handle(&self) -> &vk::SwapchainKHR {
    return &self.swapchain;
  }

  pub fn get_textures(&self) -> &[Arc<VkTexture>] {
    &self.textures
  }

  pub fn get_views(&self) -> &[Arc<VkTextureView>] {
    return &self.views[..];
  }

  pub fn get_width(&self) -> u32 {
     self.textures.first().unwrap().get_info().width
  }

  pub fn get_height(&self) -> u32 {
    self.textures.first().unwrap().get_info().height
  }

  pub fn prepare_back_buffer(&self, semaphore: &VkSemaphore) -> VkResult<(u32, bool)> {
    while self.presented_image.load(Ordering::SeqCst) != self.acquired_image.load(Ordering::SeqCst) {}
    let result = unsafe { self.swapchain_loader.acquire_next_image(self.swapchain, std::u64::MAX, *semaphore.get_handle(), vk::Fence::null()) };
    if let Ok((image, is_optimal)) = result {
      if !is_optimal && false {
        self.set_state(VkSwapchainState::Suboptimal);
      }
      self.acquired_image.store(image, Ordering::SeqCst);
    } else {
      match result.err().unwrap() {
        vk::Result::ERROR_SURFACE_LOST_KHR => {
          self.set_state(VkSwapchainState::SurfaceLost);
        }
        vk::Result::ERROR_OUT_OF_DATE_KHR => {
          self.set_state(VkSwapchainState::OutOfDate);
        }
        _ => {}
      }
    }
    result
  }

  pub(crate) fn set_presented_image(&self, presented_image_index: u32) {
    self.presented_image.store(presented_image_index, Ordering::SeqCst);
  }

  pub fn set_state(&self, state: VkSwapchainState) {
    let old = self.state.swap(state);
    if old != state {
      println!("Swapchain state changed from {:?} to: {:?}", old, state);
    }
  }

  pub fn state(&self) -> VkSwapchainState {
    self.state.load()
  }
}

impl Drop for VkSwapchain {
  fn drop(&mut self) {
    unsafe {
      self.swapchain_loader.destroy_swapchain(self.swapchain, None)
    }
  }
}

impl Swapchain<VkBackend> for VkSwapchain {
  fn recreate(old: &Self, width: u32, height: u32) -> Result<Arc<Self>, SwapchainError> {
    println!("recreating swapchain");
    let old_sc_state = old.state();
    assert_ne!(old_sc_state, VkSwapchainState::SurfaceLost);

    if old.state() == VkSwapchainState::Retired {
      println!("swapchain was retired, recreating from scratch");
      VkSwapchain::new_internal(old.vsync, width, height, &old.device, &old.surface, None)
    } else {
      VkSwapchain::new_internal(old.vsync, width, height, &old.device, &old.surface, Some(&old))
    }
  }

  fn recreate_on_surface(old: &Self, surface: &Arc<VkSurface>, width: u32, height: u32) -> Result<Arc<Self>, SwapchainError> {
    println!("recreating swapchain on new surface");
    VkSwapchain::new_internal(old.vsync, width, height, &old.device, surface, None)
  }

  fn sample_count(&self) -> SampleCount {
    self.textures.first().unwrap().get_info().samples
  }

  fn format(&self) -> Format {
    self.textures.first().unwrap().get_info().format
  }

  fn surface(&self) -> &Arc<VkSurface> {
    &self.surface
  }
}

pub(crate) enum VkSwapchainAcquireResult<'a> {
  Success {
    back_buffer: &'a Arc<VkTexture>,
    back_buffer_index: u32
  },
  SubOptimal {
    back_buffer: &'a Arc<VkTexture>,
    back_buffer_index: u32
  },
  Broken,
  DeviceLost
}

fn surface_vk_format_to_core(format: vk::Format) -> Format {
  match format {
    vk::Format::B8G8R8A8_UNORM => Format::BGRA8UNorm,
    vk::Format::R8G8B8A8_UNORM => Format::RGBA8,
    _ => panic!("Unsupported format: {:?}", format)
  }
}
