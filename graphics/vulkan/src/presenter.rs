use ash::vk;
use ash::extensions::khr;
use ash::version::DeviceV1_0;

// TODO implement drop
pub struct Presenter {
  surface: vk::SurfaceKHR,
  swapchain: vk::SwapchainKHR,
  swapchain_image_views: Vec<vk::ImageView>
}

pub const SWAPCHAIN_EXT_NAME: &str = "VK_KHR_swapchain";

impl Presenter {
  pub unsafe fn new(physical_device: &vk::PhysicalDevice, device: &ash::Device, surface_ext: khr::Surface, surface: vk::SurfaceKHR, swapchain_ext: khr::Swapchain) -> Presenter {
    let present_modes = surface_ext.get_physical_device_surface_present_modes(*physical_device, surface).unwrap();
    let present_mode = Presenter::pick_present_mode(present_modes);

    let formats = surface_ext.get_physical_device_surface_formats(*physical_device, surface).unwrap();
    let format = Presenter::pick_format(formats);

    let capabilities = surface_ext.get_physical_device_surface_capabilities(*physical_device, surface).unwrap();
    let extent = Presenter::pick_swap_extent(&capabilities);

    let image_count = if capabilities.max_image_count > 0 {
      capabilities.max_image_count
    } else {
      capabilities.min_image_count + 1
    };

    let swapchain_create_info = vk::SwapchainCreateInfoKHR {
      surface: surface,
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

    let swapchain = swapchain_ext.create_swapchain(&swapchain_create_info, None).unwrap();
    let swapchain_images = swapchain_ext.get_swapchain_images(swapchain).unwrap();
    let swapchain_image_views = swapchain_images
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
        device.create_image_view(&info, None).unwrap()
      })
      .collect();

    return Presenter {
      surface: surface,
      swapchain: swapchain,
      swapchain_image_views: swapchain_image_views
    };
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
      panic!("No current extent")
    }
  }

}
