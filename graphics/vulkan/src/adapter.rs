
use std::ffi::{CStr, CString};
use std::cmp::Ordering;
use std::sync::Arc;
use std::f32;
use std::slice;
use std::os::raw::c_char;

use ash::vk;
use ash::extensions::khr;
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};

use sourcerenderer_core::graphics::Adapter;
use sourcerenderer_core::graphics::Device;
use sourcerenderer_core::graphics::AdapterType;
use sourcerenderer_core::graphics::Surface;
use crate::VkDevice;
use crate::VkInstance;
use crate::VkSurface;
use crate::VkQueue;
use crate::queue::VkQueueInfo;
use crate::VkBackend;
use crate::raw::*;
use ash::extensions::khr::Surface as KhrSurface;

const SWAPCHAIN_EXT_NAME: &str = "VK_KHR_swapchain";
const GET_DEDICATED_MEMORY_REQUIREMENTS2_EXT_NAME: &str = "VK_KHR_get_memory_requirements2";
const DEDICATED_ALLOCATION_EXT_NAME: &str = "VK_KHR_dedicated_allocation";
const DESCRIPTOR_UPDATE_TEMPLATE_EXT_NAME: &str = "VK_KHR_descriptor_update_template";


bitflags! {
  pub struct VkAdapterExtensionSupport: u32 {
    const NONE = 0;
    const SWAPCHAIN = 0b_1;
    const DEDICATED_ALLOCATION = 0b_10;
    const GET_MEMORY_PROPERTIES2 = 0b_100;
    const DESCRIPTOR_UPDATE_TEMPLATE = 0b1000;
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
      let name_ptr = &prop.extension_name as *const i8;
      let c_char_ptr = name_ptr as *const c_char;
      let name_res = unsafe { CStr::from_ptr(c_char_ptr) }.to_str();
      let name = name_res.unwrap();
      extensions |= match name {
        SWAPCHAIN_EXT_NAME => { VkAdapterExtensionSupport::SWAPCHAIN },
        DEDICATED_ALLOCATION_EXT_NAME => { VkAdapterExtensionSupport::DEDICATED_ALLOCATION },
        GET_DEDICATED_MEMORY_REQUIREMENTS2_EXT_NAME => { VkAdapterExtensionSupport::GET_MEMORY_PROPERTIES2 },
        DESCRIPTOR_UPDATE_TEMPLATE_EXT_NAME => { VkAdapterExtensionSupport::DESCRIPTOR_UPDATE_TEMPLATE },
        _ => VkAdapterExtensionSupport::NONE
      };
    }

    return VkAdapter {
      instance,
      physical_device,
      properties,
      extensions
    };
  }

  pub fn get_physical_device_handle(&self) -> &vk::PhysicalDevice {
    return &self.physical_device;
  }

  pub fn get_raw_instance(&self) -> &Arc<RawVkInstance> { return &self.instance; }
}

// Vulkan physical devices are implicitly freed with the instance

impl Adapter<VkBackend> for VkAdapter {
  fn create_device(&self, surface: &VkSurface) -> VkDevice {
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
        .find(|(index, queue_props)|
          queue_props.queue_count > 0
          && queue_props.queue_flags & vk::QueueFlags::COMPUTE  == vk::QueueFlags::COMPUTE
          && queue_props.queue_flags & vk::QueueFlags::GRAPHICS != vk::QueueFlags::GRAPHICS
        );

      let transfer_queue_family_props = queue_properties
        .iter()
        .enumerate()
        .find(|(index, queue_props)|
          queue_props.queue_count > 0
          && queue_props.queue_flags & vk::QueueFlags::TRANSFER == vk::QueueFlags::TRANSFER
          && queue_props.queue_flags & vk::QueueFlags::COMPUTE  != vk::QueueFlags::COMPUTE
          && queue_props.queue_flags & vk::QueueFlags::GRAPHICS != vk::QueueFlags::GRAPHICS
        );

      let graphics_queue_info = VkQueueInfo {
        queue_family_index: graphics_queue_family_props.0,
        queue_index: 0,
        supports_presentation: surface_loader.get_physical_device_surface_support(self.physical_device, graphics_queue_family_props.0 as u32, *surface.get_surface_handle())
      };

      let compute_queue_info = compute_queue_family_props.map(
        |(index, _)| {
          //There is a separate queue family specifically for compute
          VkQueueInfo {
            queue_family_index: index,
            queue_index: 0,
            supports_presentation: surface_loader.get_physical_device_surface_support(self.physical_device, index as u32, *surface.get_surface_handle())
          }
        }
      );

      let transfer_queue_info = transfer_queue_family_props.map(
        |(index, _)| {
          //There is a separate queue family specifically for transfers
          VkQueueInfo {
            queue_family_index: index,
            queue_index: 0,
            supports_presentation: surface_loader.get_physical_device_surface_support(self.physical_device, index as u32, *surface.get_surface_handle())
          }
        }
      );

      let mut queue_create_descs: Vec<vk::DeviceQueueCreateInfo> = Vec::new();
      queue_create_descs.push(vk::DeviceQueueCreateInfo {
        queue_family_index: graphics_queue_info.queue_family_index as u32,
        queue_count: 1,
        p_queue_priorities: &1.0f32 as *const f32,
        ..Default::default()
      });

      if compute_queue_info.is_some() {
        queue_create_descs.push(vk::DeviceQueueCreateInfo {
          queue_family_index: compute_queue_info.unwrap().queue_family_index as u32,
          queue_count: 1,
          p_queue_priorities: &1.0f32 as *const f32,
          ..Default::default()
        });
      }

      if transfer_queue_info.is_some() {
        queue_create_descs.push(vk::DeviceQueueCreateInfo {
          queue_family_index: transfer_queue_info.unwrap().queue_family_index as u32,
          queue_count: 1,
          p_queue_priorities: &1.0f32 as *const f32,
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

      let extension_names_c: Vec<CString> = extension_names
        .iter()
        .map(|ext| CString::new(*ext).unwrap())
        .collect();
      let extension_names_ptr: Vec<*const i8> = extension_names_c
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

      /*let vk_graphics_queue = vk_device.get_device_queue(graphics_queue_info.queue_family_index as u32, graphics_queue_info.queue_index as u32);
      let vk_compute_queue = vk_device.get_device_queue(compute_queue_info.queue_family_index as u32, compute_queue_info.queue_index as u32);
      let vk_transfer_queue = vk_device.get_device_queue(transfer_queue_info.queue_family_index as u32, transfer_queue_info.queue_index as u32);

      let graphics_queue = VkQueue::new(graphics_queue_info, vk_graphics_queue, device.clone());
      let compute_queue = VkQueue::new(compute_queue_info, vk_compute_queue, device.clone());
      let transfer_queue = VkQueue::new(transfer_queue_info, vk_transfer_queue, device.clone());*/

      VkDevice::new(
        vk_device,
        &self.instance,
        self.physical_device,
        graphics_queue_info,
        compute_queue_info,
        transfer_queue_info,
        self.extensions)
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
