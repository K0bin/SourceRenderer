
use std::ffi::{CStr, CString, c_void};

use std::sync::Arc;
use std::f32;

use std::os::raw::c_char;

use ash::vk;

use sourcerenderer_core::graphics::Adapter;

use sourcerenderer_core::graphics::AdapterType;

use crate::VkDevice;

use crate::VkSurface;

use crate::bindless::BINDLESS_TEXTURE_COUNT;
use crate::queue::VkQueueInfo;
use crate::VkBackend;
use crate::raw::*;
use ash::extensions::khr::Surface as KhrSurface;

const SWAPCHAIN_EXT_NAME: &str = "VK_KHR_swapchain";
const GET_DEDICATED_MEMORY_REQUIREMENTS2_EXT_NAME: &str = "VK_KHR_get_memory_requirements2";
const DEDICATED_ALLOCATION_EXT_NAME: &str = "VK_KHR_dedicated_allocation";
const DESCRIPTOR_UPDATE_TEMPLATE_EXT_NAME: &str = "VK_KHR_descriptor_update_template";
const SHADER_NON_SEMANTIC_INFO_EXT_NAME: &str = "VK_KHR_shader_non_semantic_info";
const DESCRIPTOR_INDEXING_EXT_NAME: &str = "VK_EXT_descriptor_indexing";
const ACCELERATION_STRUCTURE_EXT_NAME: &str = "VK_KHR_acceleration_structure";
const BUFFER_DEVICE_ADDRESS_EXT_NAME: &str = "VK_KHR_buffer_device_address";
const DEFERRED_HOST_OPERATIONS_EXT_NAME: &str = "VK_KHR_deferred_host_operations";
const RAY_TRACING_PIPELINE_EXT_NAME: &str = "VK_KHR_ray_tracing_pipeline";
const RAY_QUERY_EXT_NAME: &str = "VK_KHR_ray_query";
const PIPELINE_LIBRARY_EXT_NAME: &str = "VK_KHR_pipeline_library";
const SPIRV_1_4_EXT_NAME: &str = "VK_KHR_spirv_1_4";
const SHADER_FLOAT_CONTROLS_EXT_NAME: &str = "VK_KHR_shader_float_controls";
const DRAW_INDIRECT_COUNT_EXT_NAME: &str = "VK_KHR_draw_indirect_count";
const TIMELINE_SEMAPHORE_EXT_NAME: &str = "VK_KHR_timeline_semaphore";
const SYNCHRONIZATION2_EXT_NAME: &str = "VK_KHR_synchronization2";


bitflags! {
  pub struct VkAdapterExtensionSupport: u32 {
    const NONE                       = 0b0;
    const SWAPCHAIN                  = 0b1;
    const DEDICATED_ALLOCATION       = 0b10;
    const GET_MEMORY_PROPERTIES2     = 0b100;
    const DESCRIPTOR_UPDATE_TEMPLATE = 0b1000;
    const SHADER_NON_SEMANTIC_INFO   = 0b10000;
    const DESCRIPTOR_INDEXING        = 0b100000;
    const ACCELERATION_STRUCTURE     = 0b1000000;
    const BUFFER_DEVICE_ADDRESS      = 0b10000000;
    const DEFERRED_HOST_OPERATIONS   = 0b100000000;
    const RAY_TRACING_PIPELINE       = 0b1000000000;
    const RAY_QUERY                  = 0b10000000000;
    const PIPELINE_LIBRARY           = 0b100000000000;
    const SPIRV_1_4                  = 0b1000000000000;
    const SHADER_FLOAT_CONTROLS      = 0b10000000000000;
    const DRAW_INDIRECT_COUNT        = 0b100000000000000;
    const TIMELINE_SEMAPHORE         = 0b1000000000000000;
    const SYNCHRONIZATION2           = 0b10000000000000000;
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
        DESCRIPTOR_INDEXING_EXT_NAME => { VkAdapterExtensionSupport::DESCRIPTOR_INDEXING },
        ACCELERATION_STRUCTURE_EXT_NAME => { VkAdapterExtensionSupport::ACCELERATION_STRUCTURE },
        PIPELINE_LIBRARY_EXT_NAME => { VkAdapterExtensionSupport::PIPELINE_LIBRARY },
        BUFFER_DEVICE_ADDRESS_EXT_NAME => { VkAdapterExtensionSupport::BUFFER_DEVICE_ADDRESS },
        RAY_QUERY_EXT_NAME => { VkAdapterExtensionSupport::RAY_QUERY },
        RAY_TRACING_PIPELINE_EXT_NAME => { VkAdapterExtensionSupport::RAY_TRACING_PIPELINE },
        DEFERRED_HOST_OPERATIONS_EXT_NAME => { VkAdapterExtensionSupport::DEFERRED_HOST_OPERATIONS },
        SPIRV_1_4_EXT_NAME => { VkAdapterExtensionSupport::SPIRV_1_4 },
        SHADER_FLOAT_CONTROLS_EXT_NAME => { VkAdapterExtensionSupport::SHADER_FLOAT_CONTROLS },
        DRAW_INDIRECT_COUNT_EXT_NAME => { VkAdapterExtensionSupport::DRAW_INDIRECT_COUNT },
        TIMELINE_SEMAPHORE_EXT_NAME => { VkAdapterExtensionSupport::TIMELINE_SEMAPHORE },
        SYNCHRONIZATION2_EXT_NAME => { VkAdapterExtensionSupport::SYNCHRONIZATION2 },
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

  pub fn physical_device_handle(&self) -> &vk::PhysicalDevice {
    &self.physical_device
  }

  pub fn raw_instance(&self) -> &Arc<RawVkInstance> { &self.instance }
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
        supports_presentation: surface_loader.get_physical_device_surface_support(self.physical_device, graphics_queue_family_props.0 as u32, *surface.surface_handle()).unwrap_or(false)
      };

      let compute_queue_info = compute_queue_family_props.map(
        |(index, _)| {
          //There is a separate queue family specifically for compute
          VkQueueInfo {
            queue_family_index: index,
            queue_index: 0,
            supports_presentation: surface_loader.get_physical_device_surface_support(self.physical_device, index as u32, *surface.surface_handle()).unwrap_or(false)
          }
        }
      );

      let transfer_queue_info = transfer_queue_family_props.map(
        |(index, _)| {
          //There is a separate queue family specifically for transfers
          VkQueueInfo {
            queue_family_index: index,
            queue_index: 0,
            supports_presentation: surface_loader.get_physical_device_surface_support(self.physical_device, index as u32, *surface.surface_handle()).unwrap_or(false)
          }
        }
      );

      let priority = 1.0f32;
      let mut queue_create_descs = vec![
        vk::DeviceQueueCreateInfo {
          queue_family_index: graphics_queue_info.queue_family_index as u32,
          queue_count: 1,
          p_queue_priorities: &priority as *const f32,
          ..Default::default()
        }
      ];

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

      let mut features = VkFeatures::empty();

      let mut supported_features: vk::PhysicalDeviceFeatures2 = Default::default();
      let mut properties: vk::PhysicalDeviceProperties2 = Default::default();
      let mut supported_descriptor_indexing_features = vk::PhysicalDeviceDescriptorIndexingFeaturesEXT::default();
      let mut descriptor_indexing_properties = vk::PhysicalDeviceDescriptorIndexingPropertiesEXT::default();
      let mut supported_acceleration_structure_features = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default();
      let mut supported_rt_pipeline_features = vk::PhysicalDeviceRayTracingPipelineFeaturesKHR::default();
      let mut supported_bda_features = vk::PhysicalDeviceBufferDeviceAddressFeaturesKHR::default();
      if self.extensions.intersects(VkAdapterExtensionSupport::DESCRIPTOR_INDEXING) {
        supported_descriptor_indexing_features.p_next = std::mem::replace(&mut supported_features.p_next, &mut supported_descriptor_indexing_features as *mut vk::PhysicalDeviceDescriptorIndexingFeaturesEXT as *mut c_void);
        descriptor_indexing_properties.p_next = std::mem::replace(&mut properties.p_next, &mut descriptor_indexing_properties as *mut vk::PhysicalDeviceDescriptorIndexingPropertiesEXT as *mut c_void);
      }
      if self.extensions.intersects(VkAdapterExtensionSupport::ACCELERATION_STRUCTURE) {
        supported_acceleration_structure_features.p_next = std::mem::replace(&mut supported_features.p_next, &mut supported_acceleration_structure_features as *mut vk::PhysicalDeviceAccelerationStructureFeaturesKHR as *mut c_void);
      }
      if self.extensions.intersects(VkAdapterExtensionSupport::RAY_TRACING_PIPELINE) {
        supported_rt_pipeline_features.p_next = std::mem::replace(&mut supported_features.p_next, &mut supported_rt_pipeline_features as *mut vk::PhysicalDeviceRayTracingPipelineFeaturesKHR as *mut c_void);
      }
      if self.extensions.intersects(VkAdapterExtensionSupport::BUFFER_DEVICE_ADDRESS) {
        supported_bda_features.p_next = std::mem::replace(&mut supported_features.p_next, &mut supported_bda_features as *mut vk::PhysicalDeviceBufferDeviceAddressFeaturesKHR as *mut c_void);
      }


      self.instance.get_physical_device_features2(self.physical_device, &mut supported_features);
      self.instance.get_physical_device_properties2(self.physical_device, &mut properties);

      let mut enabled_features: vk::PhysicalDeviceFeatures = Default::default();
      let mut descriptor_indexing_features = vk::PhysicalDeviceDescriptorIndexingFeaturesEXT::default();
      let mut acceleration_structure_features = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default();
      let mut rt_pipeline_features = vk::PhysicalDeviceRayTracingPipelineFeaturesKHR::default();
      let mut bda_features = vk::PhysicalDeviceBufferDeviceAddressFeaturesKHR::default();
      let mut extension_names: Vec<&str> = vec!(SWAPCHAIN_EXT_NAME);
      let mut device_creation_pnext: *mut c_void = std::ptr::null_mut();

      if self.extensions.intersects(VkAdapterExtensionSupport::GET_MEMORY_PROPERTIES2) && self.extensions.intersects(VkAdapterExtensionSupport::DEDICATED_ALLOCATION) {
        extension_names.push(GET_DEDICATED_MEMORY_REQUIREMENTS2_EXT_NAME);
        extension_names.push(DEDICATED_ALLOCATION_EXT_NAME);
        features |= VkFeatures::DEDICATED_ALLOCATION;
      }

      if self.extensions.intersects(VkAdapterExtensionSupport::DESCRIPTOR_UPDATE_TEMPLATE) {
        extension_names.push(DESCRIPTOR_UPDATE_TEMPLATE_EXT_NAME);
        features |= VkFeatures::DESCRIPTOR_TEMPLATE;
      }

      if self.instance.debug_utils.is_some() && self.extensions.intersects(VkAdapterExtensionSupport::SHADER_NON_SEMANTIC_INFO) {
        extension_names.push(SHADER_NON_SEMANTIC_INFO_EXT_NAME);
      }

      let supports_descriptor_indexing = self.extensions.intersects(VkAdapterExtensionSupport::DESCRIPTOR_INDEXING)
        && supported_descriptor_indexing_features.shader_sampled_image_array_non_uniform_indexing == vk::TRUE
        && supported_descriptor_indexing_features.descriptor_binding_sampled_image_update_after_bind == vk::TRUE
        && supported_descriptor_indexing_features.descriptor_binding_variable_descriptor_count == vk::TRUE
        && supported_descriptor_indexing_features.runtime_descriptor_array == vk::TRUE
        && supported_descriptor_indexing_features.descriptor_binding_partially_bound == vk::TRUE
        && supported_descriptor_indexing_features.descriptor_binding_update_unused_while_pending == vk::TRUE
        && descriptor_indexing_properties.shader_sampled_image_array_non_uniform_indexing_native == vk::TRUE
        && descriptor_indexing_properties.max_descriptor_set_update_after_bind_sampled_images > BINDLESS_TEXTURE_COUNT;

      let supports_bda = self.extensions.contains(VkAdapterExtensionSupport::BUFFER_DEVICE_ADDRESS)
        && supported_bda_features.buffer_device_address == vk::TRUE;

      let supports_indirect = self.extensions.contains(VkAdapterExtensionSupport::DRAW_INDIRECT_COUNT)
        && supported_features.features.draw_indirect_first_instance == vk::TRUE
        && supported_features.features.multi_draw_indirect == vk::TRUE && supports_bda;

      let supports_rt = supports_descriptor_indexing
        && self.extensions.contains(
          VkAdapterExtensionSupport::ACCELERATION_STRUCTURE
          | VkAdapterExtensionSupport::RAY_TRACING_PIPELINE
          | VkAdapterExtensionSupport::DEFERRED_HOST_OPERATIONS
          | VkAdapterExtensionSupport::SPIRV_1_4
          | VkAdapterExtensionSupport::SHADER_FLOAT_CONTROLS)
        && supported_acceleration_structure_features.acceleration_structure == vk::TRUE
        && supported_rt_pipeline_features.ray_tracing_pipeline == vk::TRUE
        && supports_bda;

      if supports_descriptor_indexing {
        println!("Bindless supported.");
        extension_names.push(DESCRIPTOR_INDEXING_EXT_NAME);
        descriptor_indexing_features.p_next = std::mem::replace(&mut device_creation_pnext, &mut descriptor_indexing_features as *mut vk::PhysicalDeviceDescriptorIndexingFeaturesEXT as *mut c_void);
        descriptor_indexing_features.shader_sampled_image_array_non_uniform_indexing = vk::TRUE;
        descriptor_indexing_features.descriptor_binding_sampled_image_update_after_bind = vk::TRUE;
        descriptor_indexing_features.descriptor_binding_variable_descriptor_count = vk::TRUE;
        descriptor_indexing_features.runtime_descriptor_array = vk::TRUE;
        descriptor_indexing_features.descriptor_binding_partially_bound = vk::TRUE;
        descriptor_indexing_features.descriptor_binding_update_unused_while_pending = vk::TRUE;
        features |= VkFeatures::DESCRIPTOR_INDEXING;
      }

      if supports_rt {
        println!("Ray tracing supported.");
        extension_names.push(DEFERRED_HOST_OPERATIONS_EXT_NAME);
        extension_names.push(ACCELERATION_STRUCTURE_EXT_NAME);
        extension_names.push(RAY_TRACING_PIPELINE_EXT_NAME);
        extension_names.push(PIPELINE_LIBRARY_EXT_NAME);
        extension_names.push(SPIRV_1_4_EXT_NAME);
        extension_names.push(SHADER_FLOAT_CONTROLS_EXT_NAME);

        features |= VkFeatures::RAY_TRACING;
        acceleration_structure_features.acceleration_structure = vk::TRUE;
        rt_pipeline_features.ray_tracing_pipeline = vk::TRUE;
        acceleration_structure_features.p_next = std::mem::replace(&mut device_creation_pnext, &mut acceleration_structure_features as *mut vk::PhysicalDeviceAccelerationStructureFeaturesKHR as *mut c_void);
        rt_pipeline_features.p_next = std::mem::replace(&mut device_creation_pnext, &mut rt_pipeline_features as *mut vk::PhysicalDeviceRayTracingPipelineFeaturesKHR as *mut c_void);
      }

      if supports_indirect {
        println!("GPU driven rendering supported.");
        extension_names.push(DRAW_INDIRECT_COUNT_EXT_NAME);
        features |= VkFeatures::ADVANCED_INDIRECT;
        enabled_features.draw_indirect_first_instance = vk::TRUE;
        enabled_features.multi_draw_indirect = vk::TRUE;
      }

      if supports_bda && supports_rt {
        extension_names.push(BUFFER_DEVICE_ADDRESS_EXT_NAME);
        bda_features.buffer_device_address = vk::TRUE;
        bda_features.p_next = std::mem::replace(&mut device_creation_pnext, &mut bda_features as *mut vk::PhysicalDeviceBufferDeviceAddressFeaturesKHR as *mut c_void);
      }

      if !self.extensions.contains(VkAdapterExtensionSupport::TIMELINE_SEMAPHORE) || !self.extensions.contains(VkAdapterExtensionSupport::SYNCHRONIZATION2) {
        panic!("Timeline semaphores or sync2 unsupported. Update your driver!");
      }
      extension_names.push(TIMELINE_SEMAPHORE_EXT_NAME);
      extension_names.push(SYNCHRONIZATION2_EXT_NAME);

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
        p_next: device_creation_pnext,
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
        features,
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
