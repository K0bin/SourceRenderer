use std::sync::Arc;

use ash::vk;
use ash::extensions::khr::Swapchain as SwapchainLoader;
use ash::Device;
use crate::ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::{Swapchain, TextureInfo, SampleCount};
use sourcerenderer_core::graphics::Texture;
use sourcerenderer_core::graphics::Format;

use crate::VkInstance;
use crate::VkSurface;
use crate::VkDevice;
use crate::raw::{RawVkInstance, RawVkDevice};
use crate::VkAdapter;
use crate::VkTexture;
use crate::VkSemaphore;
use crate::VkBackend;
use crate::VkQueue;
use std::cmp::{min, max};
use texture::VkTextureView;
use vk_mem::ffi::VkResult_VK_ERROR_OUT_OF_DATE_KHR;
use ash::prelude::VkResult;

pub struct VkSwapchain {
  textures: Vec<Arc<VkTexture>>,
  views: Vec<Arc<VkTextureView>>,
  swap_chain: vk::SwapchainKHR,
  swap_chain_loader: SwapchainLoader,
  instance: Arc<RawVkInstance>,
  surface: Arc<VkSurface>,
  device: Arc<RawVkDevice>,
  width: u32,
  height: u32,
  vsync: bool
}

impl VkSwapchain {
  fn new_internal(vsync: bool, width: u32, height: u32, device: &Arc<RawVkDevice>, surface: &Arc<VkSurface>, old_swapchain: Option<&vk::SwapchainKHR>) -> Self {
    let vk_device = &device.device;
    let instance = &device.instance;

    return unsafe {
      let physical_device = device.physical_device;
      let present_modes = surface.get_present_modes(&physical_device);
      let present_mode = VkSwapchain::pick_present_mode(present_modes);
      let swap_chain_loader = SwapchainLoader::new(&instance.instance, vk_device);

      let capabilities = surface.get_capabilities(&physical_device);
      let formats = surface.get_formats(&physical_device);
      let format = VkSwapchain::pick_format(&formats);

      let (width, height) = VkSwapchain::pick_extent(&capabilities, width, height);
      let extent = vk::Extent2D {
        width,
        height
      };

      let image_count = VkSwapchain::pick_image_count(&capabilities, 3);

      let swap_chain_create_info = vk::SwapchainCreateInfoKHR {
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
        composite_alpha: vk::CompositeAlphaFlagsKHR::OPAQUE,
        clipped: vk::TRUE,
        old_swapchain: old_swapchain.map_or(vk::SwapchainKHR::null(), |swap_chain| *swap_chain),
        ..Default::default()
      };

      let swap_chain = swap_chain_loader.create_swapchain(&swap_chain_create_info, None).unwrap();
      let swap_chain_images = swap_chain_loader.get_swapchain_images(swap_chain).unwrap();
      let textures: Vec<Arc<VkTexture>> = swap_chain_images
        .iter()
        .map(|image|
          Arc::new(VkTexture::from_image(device, *image, TextureInfo {
            format: Format::BGRA8UNorm,
            width,
            height,
            array_length: 1u32,
            mip_levels: 1u32,
            depth: 1u32,
            samples: SampleCount::Samples1
          })))
        .collect();

      let swap_chain_image_views: Vec<Arc<VkTextureView>> = textures
        .iter()
        .map(|texture| {
          Arc::new(VkTextureView::new_render_target_view(device, texture))
        })
        .collect();

      VkSwapchain {
        textures,
        views: swap_chain_image_views,
        swap_chain,
        swap_chain_loader,
        instance: device.instance.clone(),
        surface: surface.clone(),
        device: device.clone(),
        width,
        height,
        vsync
      }
    }
  }

  pub fn new(vsync: bool, width: u32, height: u32, device: &Arc<RawVkDevice>, surface: &Arc<VkSurface>) -> Self {
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
        .filter(|&format| format.format == vk::Format::B8G8R8A8_UNORM && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR)
        .nth(0).expect("No compatible format found")
    }
  }

  pub fn pick_image_count(capabilities: &vk::SurfaceCapabilitiesKHR, preferred: u32) -> u32 {
    let mut image_count = max(capabilities.min_image_count + 1, preferred);
    if capabilities.max_image_count != 0 {
      image_count = min(capabilities.max_image_count, image_count);
    }
    image_count
  }

  unsafe fn pick_present_mode(present_modes: Vec<vk::PresentModeKHR>) -> vk::PresentModeKHR {
    return *present_modes
      .iter()
      .filter(|&&mode| mode == vk::PresentModeKHR::FIFO)
      .nth(0).expect("No compatible present mode found");
  }

  pub fn get_loader(&self) -> &SwapchainLoader {
    return &self.swap_chain_loader;
  }

  pub fn get_handle(&self) -> &vk::SwapchainKHR {
    return &self.swap_chain;
  }

  pub fn get_textures(&self) -> &[Arc<VkTexture>] {
    &self.textures
  }

  pub fn get_views(&self) -> &[Arc<VkTextureView>] {
    return &self.views[..];
  }

  pub fn get_width(&self) -> u32 {
    return self.width;
  }

  pub fn get_height(&self) -> u32 {
    return self.height;
  }

  pub fn prepare_back_buffer(&self, semaphore: &VkSemaphore) -> VkResult<(u32, bool)> {
    unsafe { self.swap_chain_loader.acquire_next_image(self.swap_chain, std::u64::MAX, *semaphore.get_handle(), vk::Fence::null()) }
  }
}

impl Drop for VkSwapchain {
  fn drop(&mut self) {
    unsafe {
      self.swap_chain_loader.destroy_swapchain(self.swap_chain, None)
    }
  }
}

impl Swapchain for VkSwapchain {
  fn recreate(old: &Self) -> Self {
    VkSwapchain::new_internal(old.vsync, old.width, old.height, &old.device, &old.surface, Some(&old.swap_chain))
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
