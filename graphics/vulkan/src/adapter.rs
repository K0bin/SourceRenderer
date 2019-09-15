
use std::ffi::{CStr, CString};
use std::cmp::Ordering;
use std::sync::Arc;
use std::f32;

use ash::vk;
use ash::extensions::khr;
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};

use sourcerenderer_core::graphics::Adapter;
use sourcerenderer_core::graphics::Device;
use sourcerenderer_core::graphics::AdapterType;
use sourcerenderer_core::graphics::Surface;
use sourcerenderer_core::graphics::QueueType;
use crate::VkDevice;
use crate::VkInstance;
use crate::VkSurface;
use crate::VkQueue;
use crate::queue::VkQueueInfo;
use ash::extensions::khr::Surface as KhrSurface;

const SWAPCHAIN_EXT_NAME: &str = "VK_KHR_swapchain";

pub struct VkAdapter {
  instance: Arc<VkInstance>,
  physical_device: vk::PhysicalDevice,
  properties: vk::PhysicalDeviceProperties
}

impl VkAdapter {
  pub fn new(instance: Arc<VkInstance>, physical_device: vk::PhysicalDevice) -> Self {
    let properties = unsafe { instance.get_instance().get_physical_device_properties(physical_device) };
    return VkAdapter {
      instance: instance,
      physical_device: physical_device,
      properties: properties
    };
  }

  pub fn get_physical_device_handle(&self) -> &vk::PhysicalDevice {
    return &self.physical_device;
  }

  pub fn get_instance(&self) -> &VkInstance {
    return &self.instance;
  }
}

// Vulkan physical devices are implicitly freed with the instance

impl Adapter for VkAdapter {
  fn create_device(self: Arc<Self>, surface: Arc<Surface>) -> Arc<dyn Device> {
    return unsafe {
      let surface_loader = KhrSurface::new(self.instance.get_entry(), self.instance.get_instance());
      let queue_properties = self.instance.get_instance().get_physical_device_queue_family_properties(self.physical_device);

      let surface_trait_ptr = Arc::into_raw(surface.clone());
      let vk_surface = surface_trait_ptr as *const VkSurface;
      let vk_surface_khr = *(*vk_surface).get_surface_handle();
      Arc::from_raw(vk_surface);

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

      let mut graphics_queue_priorities: Vec<f32> = Vec::new();
      let graphics_queue_info = VkQueueInfo {
        queue_family_index: graphics_queue_family_props.0,
        queue_index: 0,
        queue_type: QueueType::GRAPHICS,
        supports_presentation: surface_loader.get_physical_device_surface_support(self.physical_device, graphics_queue_family_props.0 as u32, vk_surface_khr)
      };
      graphics_queue_priorities.push(1.0f32);

      let mut compute_queue_priority = 1.0f32;
      let compute_queue_info = compute_queue_family_props.map(
        |(index, _)| {
          //There is a separate queue family specifically for compute
          VkQueueInfo {
            queue_family_index: index,
            queue_index: 0,
            queue_type: QueueType::COMPUTE,
            supports_presentation: surface_loader.get_physical_device_surface_support(self.physical_device, index as u32, vk_surface_khr)
          }
        }
      );

      let mut transfer_queue_priority: f32 = 1.0f32;
      let transfer_queue_info = transfer_queue_family_props.map(
        |(index, _)| {
          //There is a separate queue family specifically for transfers
          VkQueueInfo {
            queue_family_index: index,
            queue_index: 0,
            queue_type: QueueType::TRANSFER,
            supports_presentation: surface_loader.get_physical_device_surface_support(self.physical_device, index as u32, vk_surface_khr)
          }
        }
      );

      let mut queue_create_descs: Vec<vk::DeviceQueueCreateInfo> = Vec::new();
      queue_create_descs.push(vk::DeviceQueueCreateInfo {
        queue_family_index: graphics_queue_info.queue_family_index as u32,
        queue_count: graphics_queue_priorities.len() as u32,
        p_queue_priorities: graphics_queue_priorities.as_ptr(),
        ..Default::default()
      });

      if compute_queue_info.is_some() {
        queue_create_descs.push(vk::DeviceQueueCreateInfo {
          queue_family_index: compute_queue_info.unwrap().queue_family_index as u32,
          queue_count: 1,
          p_queue_priorities: &compute_queue_priority as *const f32,
          ..Default::default()
        });
      }

      if transfer_queue_info.is_some() {
        queue_create_descs.push(vk::DeviceQueueCreateInfo {
          queue_family_index: transfer_queue_info.unwrap().queue_family_index as u32,
          queue_count: 1,
          p_queue_priorities: &transfer_queue_priority as *const f32,
          ..Default::default()
        });
      }

      let enabled_features: vk::PhysicalDeviceFeatures = Default::default();
      let extension_names: Vec<&str> = vec!(SWAPCHAIN_EXT_NAME);
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
      let vk_device = self.instance.get_instance().create_device(self.physical_device, &device_create_info, None).unwrap();

      /*let vk_graphics_queue = vk_device.get_device_queue(graphics_queue_info.queue_family_index as u32, graphics_queue_info.queue_index as u32);
      let vk_compute_queue = vk_device.get_device_queue(compute_queue_info.queue_family_index as u32, compute_queue_info.queue_index as u32);
      let vk_transfer_queue = vk_device.get_device_queue(transfer_queue_info.queue_family_index as u32, transfer_queue_info.queue_index as u32);

      let graphics_queue = VkQueue::new(graphics_queue_info, vk_graphics_queue, device.clone());
      let compute_queue = VkQueue::new(compute_queue_info, vk_compute_queue, device.clone());
      let transfer_queue = VkQueue::new(transfer_queue_info, vk_transfer_queue, device.clone());*/

      Arc::new(VkDevice::new(
        self.clone(),
        vk_device,
        graphics_queue_info,
        compute_queue_info,
        transfer_queue_info))
    };
  }

  fn adapter_type(&self) -> AdapterType {
    match self.properties.device_type {
      vk::PhysicalDeviceType::DISCRETE_GPU => AdapterType::DISCRETE,
      vk::PhysicalDeviceType::INTEGRATED_GPU => AdapterType::INTEGRATED,
      vk::PhysicalDeviceType::VIRTUAL_GPU => AdapterType::VIRTUAL,
      vk::PhysicalDeviceType::CPU => AdapterType::SOFTWARE,
      _ => AdapterType::OTHER
    }
  }

  // TODO: find out if presentation is supported
}
