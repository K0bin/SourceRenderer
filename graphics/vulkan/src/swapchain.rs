use std::sync::Arc;

use ash::vk;
use ash::extensions::khr::Swapchain as SwapchainLoader;
use ash::Device;
use crate::ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::Swapchain;
use sourcerenderer_core::graphics::SwapchainInfo;
use sourcerenderer_core::graphics::Queue;
use sourcerenderer_core::graphics::Texture;
use sourcerenderer_core::graphics::Format;
use sourcerenderer_core::graphics::Semaphore;

use crate::VkInstance;
use crate::VkSurface;
use crate::VkDevice;
use crate::raw::{RawVkInstance, RawVkDevice};
use crate::VkAdapter;
use crate::VkTexture;
use crate::VkSemaphore;
use crate::VkBackend;
use crate::VkQueue;

pub struct VkSwapchain {
  textures: Vec<Arc<VkTexture>>,
  images: Vec<vk::Image>,
  views: Vec<vk::ImageView>,
  swap_chain: vk::SwapchainKHR,
  swap_chain_loader: SwapchainLoader,
  instance: Arc<RawVkInstance>,
  surface: Arc<VkSurface>,
  device: Arc<RawVkDevice>,
  width: u32,
  height: u32
}

impl VkSwapchain {
  pub fn new(info: SwapchainInfo, device: &VkDevice, surface: &Arc<VkSurface>) -> Self {
    let device_inner = device.get_inner().clone();
    let vk_device = &device_inner.device;
    let instance = &device_inner.instance;

    return unsafe {
      let surface_loader = surface.get_surface_loader();
      let surface_handle = *surface.get_surface_handle();
      let physical_device = device.get_inner().physical_device;
      let present_modes = surface_loader.get_physical_device_surface_present_modes(physical_device, surface_handle).unwrap();
      let present_mode = VkSwapchain::pick_present_mode(present_modes);
      let swap_chain_loader = SwapchainLoader::new(&instance.instance, vk_device);

      let formats = surface_loader.get_physical_device_surface_formats(physical_device, surface_handle).unwrap();
      let format = VkSwapchain::pick_format(formats);

      let capabilities = surface_loader.get_physical_device_surface_capabilities(physical_device, surface_handle).unwrap();
      let extent = VkSwapchain::pick_swap_extent(&capabilities);

      let image_count = if capabilities.max_image_count > 0 {
        capabilities.max_image_count
      } else {
        capabilities.min_image_count + 1
      };

      let swap_chain_create_info = vk::SwapchainCreateInfoKHR {
        surface: surface_handle,
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
        old_swapchain: vk::SwapchainKHR::null(),
        ..Default::default()
      };

      let swap_chain = swap_chain_loader.create_swapchain(&swap_chain_create_info, None).unwrap();
      let swap_chain_images = swap_chain_loader.get_swapchain_images(swap_chain).unwrap();
      let textures: Vec<Arc<VkTexture>> = swap_chain_images
        .iter()
        .map(|image| {
          Arc::new(VkTexture::from_image(&device_inner, *image, Format::BGRA8UNorm, info.width, info.height, 1u32, 1u32, 1u32))
        })
        .collect();

      let swap_chain_image_views: Vec<vk::ImageView> = swap_chain_images
        .iter()
        .map(|image| {
          let info = vk::ImageViewCreateInfo {
            image: *image,
            view_type: vk::ImageViewType::TYPE_2D,
            format: format.format,
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
          vk_device.create_image_view(&info, None).unwrap()
        })
        .collect();

      VkSwapchain {
        textures,
        images: swap_chain_images,
        views: swap_chain_image_views,
        swap_chain,
        swap_chain_loader,
        instance: device.get_inner().instance.clone(),
        surface: surface.clone(),
        device: device_inner,
        width: info.width,
        height: info.height
      }
    }
  }

  unsafe fn pick_present_mode(present_modes: Vec<vk::PresentModeKHR>) -> vk::PresentModeKHR {
    return *present_modes
      .iter()
      .filter(|&&mode| mode == vk::PresentModeKHR::FIFO)
      .nth(0).expect("No compatible present mode found");
  }

  unsafe fn pick_format(formats: Vec<vk::SurfaceFormatKHR>) -> vk::SurfaceFormatKHR {
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

  unsafe fn pick_swap_extent(capabilities: &vk::SurfaceCapabilitiesKHR) -> vk::Extent2D {
    return if capabilities.current_extent.width != u32::max_value() {
      capabilities.current_extent
    } else {
      // TODO: pick an extent and check min/max
      panic!("No current extent")
    }
  }

  pub fn get_loader(&self) -> &SwapchainLoader {
    return &self.swap_chain_loader;
  }

  pub fn get_handle(&self) -> &vk::SwapchainKHR {
    return &self.swap_chain;
  }

  pub fn get_images(&self) -> &[vk::Image] {
    return &self.images[..];
  }

  pub fn get_views(&self) -> &[vk::ImageView] {
    return &self.views[..];
  }

  pub fn get_width(&self) -> u32 {
    return self.width;
  }

  pub fn get_height(&self) -> u32 {
    return self.height;
  }
}

impl Drop for VkSwapchain {
  fn drop(&mut self) {
    unsafe {
      self.swap_chain_loader.destroy_swapchain(self.swap_chain, None)
    }
  }
}

impl Swapchain<VkBackend> for VkSwapchain {
  fn prepare_back_buffer(&mut self, semaphore: &VkSemaphore) -> (Arc<VkTexture>, u32) {
    let (index, optimal) = unsafe { self.swap_chain_loader.acquire_next_image(self.swap_chain, std::u64::MAX, *semaphore.get_handle(), vk::Fence::null()) }.unwrap();
    let back_buffer = self.textures.get(index as usize).unwrap().clone();
    return (back_buffer, index);
  }
}
