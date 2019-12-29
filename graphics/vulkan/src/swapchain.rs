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
use crate::VkAdapter;
use crate::VkTexture;
use crate::VkSemaphore;
use crate::VkBackend;
use crate::VkQueue;

pub struct VkSwapchain {
  textures: Vec<Arc<VkTexture>>,
  semaphores: Vec<Arc<VkSemaphore>>,
  swapchain: vk::SwapchainKHR,
  swapchain_loader: SwapchainLoader,
  device: Arc<VkDevice>
}

impl VkSwapchain {
  pub fn new(info: SwapchainInfo, device: Arc<VkDevice>, surface: Arc<VkSurface>) -> Self {
    let vk_device = device.get_ash_device();
    let adapter = device.get_adapter();
    let instance = adapter.get_instance();

    return unsafe {
      let surface_loader = surface.get_surface_loader();
      let surface_handle = *surface.get_surface_handle();
      let physical_device = *adapter.get_physical_device_handle();
      let present_modes = surface_loader.get_physical_device_surface_present_modes(physical_device, surface_handle).unwrap();
      let present_mode = VkSwapchain::pick_present_mode(present_modes);
      let swapchain_loader = SwapchainLoader::new(instance.get_ash_instance(), vk_device);

      let formats = surface_loader.get_physical_device_surface_formats(physical_device, surface_handle).unwrap();
      let format = VkSwapchain::pick_format(formats);

      let capabilities = surface_loader.get_physical_device_surface_capabilities(physical_device, surface_handle).unwrap();
      let extent = VkSwapchain::pick_swap_extent(&capabilities);

      let image_count = if capabilities.max_image_count > 0 {
        capabilities.max_image_count
      } else {
        capabilities.min_image_count + 1
      };

      let swapchain_create_info = vk::SwapchainCreateInfoKHR {
        surface: surface_handle,
        min_image_count: image_count,
        image_format: format.format,
        image_color_space: format.color_space,
        image_extent: extent,
        image_array_layers: 1,
        image_usage: vk::ImageUsageFlags::COLOR_ATTACHMENT,
        present_mode: present_mode,
        image_sharing_mode: vk::SharingMode::EXCLUSIVE,
        pre_transform: capabilities.current_transform,
        composite_alpha: vk::CompositeAlphaFlagsKHR::OPAQUE,
        clipped: vk::TRUE,
        old_swapchain: vk::SwapchainKHR::null(),
        ..Default::default()
      };

    let swapchain = swapchain_loader.create_swapchain(&swapchain_create_info, None).unwrap();
    let swapchain_images = swapchain_loader.get_swapchain_images(swapchain).unwrap();
    let textures: Vec<Arc<VkTexture>> = swapchain_images
      .iter()
      .map(|image| {
        Arc::new(VkTexture::from_image(device.clone(), *image, Format::BGRA8UNorm, info.width, info.height, 1u32, 1u32, 1u32))
      })
      .collect();

    let semaphores: Vec<Arc<VkSemaphore>> = textures
      .iter()
      .map(|image| {
        Arc::new(VkSemaphore::new(device.clone()))
      })
      .collect();
    /*let swapchain_image_views: Vec<vk::ImageView> = swapchain_images
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
      .collect();*/

      VkSwapchain {
        textures: textures,
        semaphores: semaphores,
        swapchain: swapchain,
        swapchain_loader: swapchain_loader,
        device: device
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
}

impl Drop for VkSwapchain {
  fn drop(&mut self) {
    let vk_device = self.device.get_ash_device();
    unsafe {
      /*for image_view in &self.image_views {
          vk_device.destroy_image_view(*image_view, None);
      }*/
      println!("DESTORY SC");
      self.swapchain_loader.destroy_swapchain(self.swapchain, None);
    }
  }
}

impl Swapchain<VkBackend> for VkSwapchain {
  fn recreate(&mut self, info: SwapchainInfo) {

  }

  fn start_frame(&self, index: u32) -> (Arc<dyn Semaphore>, Arc<VkTexture>) {
    let semaphore = self.semaphores[index as usize].clone();
    unsafe { self.swapchain_loader.acquire_next_image(self.swapchain, std::u64::MAX, *semaphore.get_handle(), vk::Fence::null()); }
    let back_buffer = self.textures[index as usize].clone();;
    return (semaphore, back_buffer);
  }

  fn present(&self, queue: Arc<VkQueue>) {

  }
}
