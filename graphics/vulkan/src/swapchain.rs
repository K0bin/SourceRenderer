use std::sync::{Arc, Mutex, MutexGuard};
use std::cmp::{min, max};
use std::sync::atomic::{AtomicU32, Ordering};

use crossbeam_utils::atomic::AtomicCell;

use ash::vk;
use ash::extensions::khr::Swapchain as SwapchainLoader;

use sourcerenderer_core::graphics::{Swapchain, TextureInfo, SampleCount, SwapchainError};
use sourcerenderer_core::graphics::Texture;
use sourcerenderer_core::graphics::Format;

use crate::{VkSurface, VkBackend};
use crate::raw::{RawVkInstance, RawVkDevice};
use crate::VkTexture;
use crate::VkSemaphore;
use crate::texture::VkTextureView;

use ash::prelude::VkResult;
use ash::vk::SurfaceTransformFlagsKHR;
use sourcerenderer_core::{Matrix4, Vec3};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum VkSwapchainState {
  Okay,
  Suboptimal,
  OutOfDate,
  Retired
}

pub struct VkSwapchain {
  textures: Vec<Arc<VkTexture>>,
  views: Vec<Arc<VkTextureView>>,
  swapchain: Mutex<vk::SwapchainKHR>,
  swapchain_loader: SwapchainLoader,
  instance: Arc<RawVkInstance>,
  surface: Arc<VkSurface>,
  device: Arc<RawVkDevice>,
  vsync: bool,
  state: AtomicCell<VkSwapchainState>,
  acquired_image: AtomicU32,
  presented_image: AtomicU32,
  transform_matrix: Matrix4
}

impl VkSwapchain {
  fn new_internal(vsync: bool, width: u32, height: u32, device: &Arc<RawVkDevice>, surface: &Arc<VkSurface>, old_swapchain: Option<&Self>) -> Result<Arc<Self>, SwapchainError> {
    if surface.is_lost() {
      return Err(SwapchainError::SurfaceLost);
    }

    let vk_device = &device.device;
    let instance = &device.instance;

    unsafe {
      let physical_device = device.physical_device;
      let present_modes = match surface.get_present_modes(&physical_device) {
        Ok(present_modes) => present_modes,
        Err(e) =>  {
          match e {
            vk::Result::ERROR_SURFACE_LOST_KHR => {
              surface.mark_lost();
              return Err(SwapchainError::SurfaceLost);
            }
            _ => { panic!("Could not get surface modes: {:?}", e); }
          }
        }
      };
      let present_mode = VkSwapchain::pick_present_mode(vsync, present_modes);
      let swapchain_loader = SwapchainLoader::new(&instance.instance, vk_device);

      let capabilities = match surface.get_capabilities(&physical_device) {
        Ok(capabilities) => capabilities,
        Err(e) =>  {
          match e {
            vk::Result::ERROR_SURFACE_LOST_KHR => {
              surface.mark_lost();
              return Err(SwapchainError::SurfaceLost);
            }
            _ => { panic!("Could not get surface capabilities: {:?}", e); }
          }
        }
      };
      let formats = match surface.get_formats(&physical_device) {
        Ok(format) => format,
        Err(e) =>  {
          match e {
            vk::Result::ERROR_SURFACE_LOST_KHR => {
              surface.mark_lost();
              return Err(SwapchainError::SurfaceLost);
            }
            _ => { panic!("Could not get surface formats: {:?}", e); }
          }
        }
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

      let (matrix, transform) = match capabilities.current_transform {
        SurfaceTransformFlagsKHR::ROTATE_90 => {
          (
            Matrix4::from_euler_angles(0f32, 0f32, -std::f32::consts::FRAC_PI_2),
            SurfaceTransformFlagsKHR::ROTATE_90
          )
        }
        SurfaceTransformFlagsKHR::ROTATE_180 => {
          (
            Matrix4::from_euler_angles(0f32, 0f32, -std::f32::consts::PI),
            SurfaceTransformFlagsKHR::ROTATE_180
          )
        }
        SurfaceTransformFlagsKHR::ROTATE_270 => {
          (
            Matrix4::from_euler_angles(0f32, 0f32, -std::f32::consts::FRAC_PI_2 * 3f32),
            SurfaceTransformFlagsKHR::ROTATE_270
          )
        }
        SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR => {
          (
            Matrix4::new_nonuniform_scaling(&Vec3::new(-1f32, 1f32, 1f32)),
            SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR
          )
        }
        SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_90 => {
          (
            Matrix4::new_nonuniform_scaling(&Vec3::new(-1f32, 1f32, 1f32)) *
              Matrix4::from_euler_angles(0f32, 0f32, -std::f32::consts::FRAC_PI_2),
            SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_90
          )
        }
        SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_180 => {
          (
            Matrix4::new_nonuniform_scaling(&Vec3::new(-1f32, 1f32, 1f32)) *
              Matrix4::from_euler_angles(0f32, 0f32, -std::f32::consts::PI),
            SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_180
          )
        }
        SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_270 => {
          (
            Matrix4::new_nonuniform_scaling(&Vec3::new(-1f32, 1f32, 1f32)) *
              Matrix4::from_euler_angles(0f32, 0f32, -std::f32::consts::FRAC_PI_2 * 3f32),
            SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_270
          )
        }
        _ => {
          (
            Matrix4::identity(),
            SurfaceTransformFlagsKHR::IDENTITY
          )
        }
      };

      let image_count = VkSwapchain::pick_image_count(&capabilities, 3);

      let swapchain = {
        let old_guard = old_swapchain.map(|sc| {
          sc.set_state(VkSwapchainState::Retired);
          sc.get_handle()
        });

        let surface_handle = surface.get_surface_handle();

        let swapchain_create_info = vk::SwapchainCreateInfoKHR {
          surface: *surface_handle,
          min_image_count: image_count,
          image_format: format.format,
          image_color_space: format.color_space,
          image_extent: extent,
          image_array_layers: 1,
          image_usage: vk::ImageUsageFlags::COLOR_ATTACHMENT,
          present_mode,
          image_sharing_mode: vk::SharingMode::EXCLUSIVE,
          pre_transform: transform,
          composite_alpha: if capabilities.supported_composite_alpha.contains(vk::CompositeAlphaFlagsKHR::OPAQUE) {
            vk::CompositeAlphaFlagsKHR::OPAQUE
          } else {
            vk::CompositeAlphaFlagsKHR::INHERIT
          },
          clipped: vk::TRUE,
          old_swapchain: old_guard.as_ref().map_or(vk::SwapchainKHR::null(), |old_guard| **old_guard),
          ..Default::default()
        };

        swapchain_loader.create_swapchain(&swapchain_create_info, None).map_err(|e| {
          match e {
            vk::Result::ERROR_SURFACE_LOST_KHR => {
              surface.mark_lost();
              SwapchainError::SurfaceLost
            }
            _ => { panic!("Creating swapchain failed {:?}, old swapchain is: {:?}", e, swapchain_create_info.old_swapchain); }
          }
        })?
      };

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

      Ok(Arc::new(VkSwapchain {
        textures,
        views: swapchain_image_views,
        swapchain: Mutex::new(swapchain),
        swapchain_loader,
        instance: device.instance.clone(),
        surface: surface.clone(),
        device: device.clone(),
        vsync,
        state: AtomicCell::new(VkSwapchainState::Okay),
        presented_image: AtomicU32::new(0),
        acquired_image: AtomicU32::new(0),
        transform_matrix: matrix
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
    if formats.len() == 1 && formats[0].format == vk::Format::UNDEFINED {
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
        .next() {
        return *mode;
      }
    }

    *present_modes
      .iter()
      .filter(|&&mode| mode == vk::PresentModeKHR::FIFO)
      .next().expect("No compatible present mode found")
  }

  pub fn get_loader(&self) -> &SwapchainLoader {
    &self.swapchain_loader
  }

  pub fn get_handle(&self) -> MutexGuard<vk::SwapchainKHR> {
    self.swapchain.lock().unwrap()
  }

  pub fn get_textures(&self) -> &[Arc<VkTexture>] {
    &self.textures
  }

  pub fn get_views(&self) -> &[Arc<VkTextureView>] {
    &self.views[..]
  }

  pub fn get_width(&self) -> u32 {
     self.textures.first().unwrap().get_info().width
  }

  pub fn get_height(&self) -> u32 {
    self.textures.first().unwrap().get_info().height
  }

  #[allow(clippy::logic_bug)]
  pub fn prepare_back_buffer(&self, semaphore: &VkSemaphore) -> VkResult<(u32, bool)> {
    while self.presented_image.load(Ordering::SeqCst) != self.acquired_image.load(Ordering::SeqCst) {}
    let result = {
      let swapchain_handle = self.get_handle();
      unsafe { self.swapchain_loader.acquire_next_image(*swapchain_handle, std::u64::MAX, *semaphore.get_handle(), vk::Fence::null()) }
    };
    if let Ok((image, is_optimal)) = result {
      if !is_optimal && false {
        self.set_state(VkSwapchainState::Suboptimal);
      }
      self.acquired_image.store(image, Ordering::SeqCst);
    } else {
      match result.err().unwrap() {
        vk::Result::ERROR_SURFACE_LOST_KHR => {
          self.surface.mark_lost();
          self.set_state(VkSwapchainState::Retired);
        }
        vk::Result::ERROR_OUT_OF_DATE_KHR => {
          #[cfg(target_os = "android")]
            {
              // I guess we can not recreate the SC on OUT_OF_DATE
              self.surface.mark_lost();
            }

          self.set_state(VkSwapchainState::OutOfDate);
        }
        _ => {
          panic!("Unknown error in prepare_back_buffer: {:?}", result.err().unwrap());
        }
      }
    }
    result
  }

  pub(crate) fn set_presented_image(&self, presented_image_index: u32) {
    self.presented_image.store(presented_image_index, Ordering::SeqCst);
  }

  pub fn set_state(&self, state: VkSwapchainState) {
    self.state.store(state);
  }

  pub fn state(&self) -> VkSwapchainState {
    self.state.load()
  }

  pub fn transform(&self) -> &Matrix4 {
    &self.transform_matrix
  }
}

impl Drop for VkSwapchain {
  fn drop(&mut self) {
    let swapchain = self.swapchain.lock().unwrap();
    unsafe {
      self.swapchain_loader.destroy_swapchain(*swapchain, None)
    }
  }
}

impl Swapchain<VkBackend> for VkSwapchain {
  fn recreate(old: &Self, width: u32, height: u32) -> Result<Arc<Self>, SwapchainError> {
    if old.state() == VkSwapchainState::Retired {
      VkSwapchain::new_internal(old.vsync, width, height, &old.device, &old.surface, None)
    } else {
      VkSwapchain::new_internal(old.vsync, width, height, &old.device, &old.surface, Some(&old))
    }
  }

  fn recreate_on_surface(old: &Self, surface: &Arc<VkSurface>, width: u32, height: u32) -> Result<Arc<Self>, SwapchainError> {
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
