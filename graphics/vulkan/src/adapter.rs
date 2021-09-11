
use std::ffi::{CStr, CString};

use std::sync::Arc;
use std::f32;

use std::os::raw::c_char;

use ash::vk;

use sourcerenderer_core::graphics::Adapter;

use sourcerenderer_core::graphics::AdapterType;

use crate::VkDevice;

use crate::VkSurface;

use crate::queue::VkQueueInfo;
use crate::VkBackend;
use crate::raw::*;
use ash::extensions::khr::Surface as KhrSurface;

const SWAPCHAIN_EXT_NAME: &str = "VK_KHR_swapchain";
const GET_DEDICATED_MEMORY_REQUIREMENTS2_EXT_NAME: &str = "VK_KHR_get_memory_requirements2";
const DEDICATED_ALLOCATION_EXT_NAME: &str = "VK_KHR_dedicated_allocation";
const DESCRIPTOR_UPDATE_TEMPLATE_EXT_NAME: &str = "VK_KHR_descriptor_update_template";
const SHADER_NON_SEMANTIC_INFO_EXT_NAME: &str = "VK_KHR_shader_non_semantic_info";


bitflags! {
  pub struct VkAdapterExtensionSupport: u32 {
    const NONE                       = 0b0;
    const SWAPCHAIN                  = 0b1;
    const DEDICATED_ALLOCATION       = 0b10;
    const GET_MEMORY_PROPERTIES2     = 0b100;
    const DESCRIPTOR_UPDATE_TEMPLATE = 0b1000;
    const SHADER_NON_SEMANTIC_INFO   = 0b10000;
  }
}

pub struct VkAdapter {
  instance: Arc<RawVkInstance>,
  physical_device: vk::PhysicalDevice,
  properties: vk::PhysicalDeviceProperties,
  extensions: VkAdapterExtensionSupport
}

impl VkAdapter {
  pub fn new(instance: Arc<RawVkInstance>, physical_device: vk::PhysicalDevice) -> Self {
    let properties = unsafe { instance.instance.get_physical_device_properties(physical_device) };

    let mut extensions = VkAdapterExtensionSupport::NONE;

    let supported_extensions = unsafe { instance.instance.enumerate_device_extension_properties(physical_device) }.unwrap();
    for ref prop in supported_extensions {
      let name_ptr = &prop.extension_name as *const c_char;
      let c_char_ptr = name_ptr as *const c_char;
      let name_res = unsafe { CStr::from_ptr(c_char_ptr) }.to_str();
      let name = name_res.unwrap();
      extensions |= match name {
        SWAPCHAIN_EXT_NAME => { VkAdapterExtensionSupport::SWAPCHAIN },
        DEDICATED_ALLOCATION_EXT_NAME => { VkAdapterExtensionSupport::DEDICATED_ALLOCATION },
        GET_DEDICATED_MEMORY_REQUIREMENTS2_EXT_NAME => { VkAdapterExtensionSupport::GET_MEMORY_PROPERTIES2 },
        DESCRIPTOR_UPDATE_TEMPLATE_EXT_NAME => { VkAdapterExtensionSupport::DESCRIPTOR_UPDATE_TEMPLATE },
        SHADER_NON_SEMANTIC_INFO_EXT_NAME => { VkAdapterExtensionSupport::SHADER_NON_SEMANTIC_INFO },
        _ => VkAdapterExtensionSupport::NONE
      };
    }

    VkAdapter {
      instance,
      physical_device,
      properties,
      extensions
    }
  }

  pub fn get_physical_device_handle(&self) -> &vk::PhysicalDevice {
    &self.physical_device
  }

  pub fn get_raw_instance(&self) -> &Arc<RawVkInstance> { &self.instance }
}

// Vulkan physical devices are implicitly freed with the instance

impl Adapter<VkBackend> for VkAdapter {
  fn create_device(&self, surface: &Arc<VkSurface>) -> VkDevice {
    return unsafe {
      let surface_loader = KhrSurface::new(&self.instance.entry, &self.instance.instance);
      let queue_properties = self.instance.instance.get_physical_device_queue_family_properties(self.physical_device);

      let graphics_queue_family_props = queue_properties
        .iter()
        .enumerate()
        .find(|(_, queue_props)|
          queue_props.queue_count > 0
          && queue_props.queue_flags & vk::QueueFlags::GRAPHICS == vk::QueueFlags::GRAPHICS
        )
        .expect("Vulkan device has no graphics queue");

      let compute_queue_family_props = queue_properties
        .iter()
        .enumerate()
        .find(|(_index, queue_props)|
          queue_props.queue_count > 0
          && queue_props.queue_flags & vk::QueueFlags::COMPUTE  == vk::QueueFlags::COMPUTE
          && queue_props.queue_flags & vk::QueueFlags::GRAPHICS != vk::QueueFlags::GRAPHICS
        );

      let transfer_queue_family_props = queue_properties
        .iter()
        .enumerate()
        .find(|(_index, queue_props)|
          queue_props.queue_count > 0
          && queue_props.queue_flags & vk::QueueFlags::TRANSFER == vk::QueueFlags::TRANSFER
          && queue_props.queue_flags & vk::QueueFlags::COMPUTE  != vk::QueueFlags::COMPUTE
          && queue_props.queue_flags & vk::QueueFlags::GRAPHICS != vk::QueueFlags::GRAPHICS
        );

      let graphics_queue_info = VkQueueInfo {
        queue_family_index: graphics_queue_family_props.0,
        queue_index: 0,
        supports_presentation: surface_loader.get_physical_device_surface_support(self.physical_device, graphics_queue_family_props.0 as u32, *surface.get_surface_handle()).unwrap_or(false)
      };

      let compute_queue_info = compute_queue_family_props.map(
        |(index, _)| {
          //There is a separate queue family specifically for compute
          VkQueueInfo {
            queue_family_index: index,
            queue_index: 0,
            supports_presentation: surface_loader.get_physical_device_surface_support(self.physical_device, index as u32, *surface.get_surface_handle()).unwrap_or(false)
          }
        }
      );

      let transfer_queue_info = transfer_queue_family_props.map(
        |(index, _)| {
          //There is a separate queue family specifically for transfers
          VkQueueInfo {
            queue_family_index: index,
            queue_index: 0,
            supports_presentation: surface_loader.get_physical_device_surface_support(self.physical_device, index as u32, *surface.get_surface_handle()).unwrap_or(false)
          }
        }
      );

      let priority = 1.0f32;
      let mut queue_create_descs: Vec<vk::DeviceQueueCreateInfo> = Vec::new();
      queue_create_descs.push(vk::DeviceQueueCreateInfo {
        queue_family_index: graphics_queue_info.queue_family_index as u32,
        queue_count: 1,
        p_queue_priorities: &priority as *const f32,
        ..Default::default()
      });

      if let Some(compute_queue_info) = compute_queue_info {
        queue_create_descs.push(vk::DeviceQueueCreateInfo {
          queue_family_index: compute_queue_info.queue_family_index as u32,
          queue_count: 1,
          p_queue_priorities: &priority as *const f32,
          ..Default::default()
        });
      }

      if let Some(transfer_queue_info) = transfer_queue_info {
        queue_create_descs.push(vk::DeviceQueueCreateInfo {
          queue_family_index: transfer_queue_info.queue_family_index as u32,
          queue_count: 1,
          p_queue_priorities: &priority as *const f32,
          ..Default::default()
        });
      }

      let enabled_features: vk::PhysicalDeviceFeatures = Default::default();
      let mut extension_names: Vec<&str> = vec!(SWAPCHAIN_EXT_NAME);

      if self.extensions.intersects(VkAdapterExtensionSupport::DEDICATED_ALLOCATION) {
        extension_names.push(DEDICATED_ALLOCATION_EXT_NAME);
      }

      if self.extensions.intersects(VkAdapterExtensionSupport::GET_MEMORY_PROPERTIES2) {
        extension_names.push(GET_DEDICATED_MEMORY_REQUIREMENTS2_EXT_NAME);
      }

      if self.extensions.intersects(VkAdapterExtensionSupport::DESCRIPTOR_UPDATE_TEMPLATE) {
        extension_names.push(DESCRIPTOR_UPDATE_TEMPLATE_EXT_NAME);
      }

      if self.instance.debug_utils.is_some() && self.extensions.intersects(VkAdapterExtensionSupport::SHADER_NON_SEMANTIC_INFO) {
        extension_names.push(SHADER_NON_SEMANTIC_INFO_EXT_NAME);
      }

      let extension_names_c: Vec<CString> = extension_names
        .iter()
        .map(|ext| CString::new(*ext).unwrap())
        .collect();
      let extension_names_ptr: Vec<*const c_char> = extension_names_c
        .iter()
        .map(|ext_c| ext_c.as_ptr())
        .collect();

      let device_create_info = vk::DeviceCreateInfo {
        p_queue_create_infos: queue_create_descs.as_ptr(),
        queue_create_info_count: queue_create_descs.len() as u32,
        p_enabled_features: &enabled_features,
        pp_enabled_extension_names: extension_names_ptr.as_ptr(),
        enabled_extension_count: extension_names_c.len() as u32,
        ..Default::default()
      };
      let vk_device = self.instance.instance.create_device(self.physical_device, &device_create_info, None).unwrap();

      let capabilities = surface.get_capabilities(&self.physical_device).unwrap();
      let mut max_image_count = capabilities.max_image_count;
      if max_image_count == 0 {
        max_image_count = 99; // whatever
      }


      VkDevice::new(
        vk_device,
        &self.instance,
        self.physical_device,
        graphics_queue_info,
        compute_queue_info,
        transfer_queue_info,
        self.extensions,
        max_image_count)
    };
  }

  fn adapter_type(&self) -> AdapterType {
    match self.properties.device_type {
      vk::PhysicalDeviceType::DISCRETE_GPU => AdapterType::Discrete,
      vk::PhysicalDeviceType::INTEGRATED_GPU => AdapterType::Integrated,
      vk::PhysicalDeviceType::VIRTUAL_GPU => AdapterType::Virtual,
      vk::PhysicalDeviceType::CPU => AdapterType::Software,
      _ => AdapterType::Other
    }
  }

  // TODO: find out if presentation is supported
}
