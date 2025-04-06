use std::{
    f32,
    ffi::{
        c_void,
        CStr,
        CString,
    },
    os::raw::c_char,
    sync::{atomic::AtomicBool, Arc},
};

use ash::{
    khr::surface::Instance as KhrSurface,
    vk,
};
use parking_lot::lock_api::ReentrantMutex;
use sourcerenderer_core::gpu;

use super::*;

use bitflags::bitflags;

const SWAPCHAIN_EXT_NAME: &str = "VK_KHR_swapchain";
const MEMORY_BUDGET_EXT_NAME: &str = "VK_EXT_memory_budget";
const ACCELERATION_STRUCTURE_EXT_NAME: &str = "VK_KHR_acceleration_structure";
const DEFERRED_HOST_OPERATIONS_EXT_NAME: &str = "VK_KHR_deferred_host_operations";
const RAY_TRACING_PIPELINE_EXT_NAME: &str = "VK_KHR_ray_tracing_pipeline";
const RAY_QUERY_EXT_NAME: &str = "VK_KHR_ray_query";
const PIPELINE_LIBRARY_EXT_NAME: &str = "VK_KHR_pipeline_library";
const HOST_IMAGE_COPY_EXT_NAME: &str = "VK_EXT_host_image_copy";
const BARYCENTRICS_EXT_NAME: &str = "VK_KHR_fragment_shader_barycentric";
const MESH_SHADER_EXT_NAME: &str = "VK_EXT_msh_shader";

bitflags! {
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
  pub struct VkAdapterExtensionSupport: u32 {
    const NONE                       = 0b0;
    const SWAPCHAIN                  = 0b1;
    const MEMORY_BUDGET              = 0b10;
    const ACCELERATION_STRUCTURE     = 0b1000000;
    const DEFERRED_HOST_OPERATIONS   = 0b100000000;
    const RAY_TRACING_PIPELINE       = 0b1000000000;
    const RAY_QUERY                  = 0b10000000000;
    const PIPELINE_LIBRARY           = 0b100000000000;
    const HOST_IMAGE_COPY            = 0b1000000000000;
    const MESH_SHADER                = 0b10000000000000;
    const BARYCENTRICS               = 0b1000000000000000000;
  }
}

#[repr(C)]
#[derive(Debug)]
struct VkPhysicalDeviceFragmentShaderBarycentricFeaturesNV {
    s_type: vk::StructureType,
    p_next: *mut c_void,
    fragment_shader_barycentric: vk::Bool32,
}

impl Default for VkPhysicalDeviceFragmentShaderBarycentricFeaturesNV {
    fn default() -> Self {
        Self {
            s_type: vk::StructureType::PHYSICAL_DEVICE_FRAGMENT_SHADER_BARYCENTRIC_FEATURES_NV,
            p_next: std::ptr::null_mut(),
            fragment_shader_barycentric: vk::FALSE,
        }
    }
}

pub struct VkAdapter {
    instance: Arc<RawVkInstance>,
    physical_device: vk::PhysicalDevice,
    properties: vk::PhysicalDeviceProperties,
}

impl VkAdapter {
    pub fn new(instance: &Arc<RawVkInstance>, physical_device: vk::PhysicalDevice) -> Self {
        let properties = unsafe {
            instance
                .instance
                .get_physical_device_properties(physical_device)
        };

        VkAdapter {
            instance: instance.clone(),
            physical_device,
            properties,
        }
    }

    #[inline(always)]
    pub fn physical_device_handle(&self) -> vk::PhysicalDevice {
        self.physical_device
    }

    #[inline(always)]
    pub fn raw_instance(&self) -> &Arc<RawVkInstance> {
        &self.instance
    }
}

// Vulkan physical devices are implicitly freed with the instance

pub(crate) const BINDLESS_TEXTURE_COUNT: u32 = gpu::BINDLESS_TEXTURE_COUNT;

impl gpu::Adapter<VkBackend> for VkAdapter {
    unsafe fn create_device(&self, surface: &VkSurface) -> VkDevice {
        let mut extensions = VkAdapterExtensionSupport::NONE;
        let supported_extensions = self.instance
                .instance
                .enumerate_device_extension_properties(self.physical_device).unwrap();
        for ref prop in supported_extensions {
            let name_ptr = &prop.extension_name as *const c_char;
            let c_char_ptr = name_ptr as *const c_char;
            let name_res = unsafe { CStr::from_ptr(c_char_ptr) }.to_str();
            let name = name_res.unwrap();
            extensions |= match name {
                SWAPCHAIN_EXT_NAME => VkAdapterExtensionSupport::SWAPCHAIN,
                MEMORY_BUDGET_EXT_NAME => VkAdapterExtensionSupport::MEMORY_BUDGET,
                ACCELERATION_STRUCTURE_EXT_NAME => VkAdapterExtensionSupport::ACCELERATION_STRUCTURE,
                PIPELINE_LIBRARY_EXT_NAME => VkAdapterExtensionSupport::PIPELINE_LIBRARY,
                RAY_QUERY_EXT_NAME => VkAdapterExtensionSupport::RAY_QUERY,
                RAY_TRACING_PIPELINE_EXT_NAME => VkAdapterExtensionSupport::RAY_TRACING_PIPELINE,
                DEFERRED_HOST_OPERATIONS_EXT_NAME => VkAdapterExtensionSupport::DEFERRED_HOST_OPERATIONS,
                BARYCENTRICS_EXT_NAME => VkAdapterExtensionSupport::BARYCENTRICS,
                MESH_SHADER_EXT_NAME => VkAdapterExtensionSupport::MESH_SHADER,
                HOST_IMAGE_COPY_EXT_NAME => VkAdapterExtensionSupport::HOST_IMAGE_COPY,
                _ => VkAdapterExtensionSupport::NONE,
            };
        }

        let surface_loader = KhrSurface::new(&self.instance.entry, &self.instance.instance);
        let queue_properties = self
            .instance
            .instance
            .get_physical_device_queue_family_properties(self.physical_device);

        let graphics_queue_family_props = queue_properties
            .iter()
            .enumerate()
            .find(|(_, queue_props)| {
                queue_props.queue_count > 0
                    && queue_props.queue_flags & vk::QueueFlags::GRAPHICS
                        == vk::QueueFlags::GRAPHICS
            })
            .expect("Vulkan device has no graphics queue");

        let compute_queue_family_props =
            queue_properties
                .iter()
                .enumerate()
                .find(|(_index, queue_props)| {
                    queue_props.queue_count > 0
                        && queue_props.queue_flags & vk::QueueFlags::COMPUTE
                            == vk::QueueFlags::COMPUTE
                        && queue_props.queue_flags & vk::QueueFlags::GRAPHICS
                            != vk::QueueFlags::GRAPHICS
                });

        let transfer_queue_family_props =
            queue_properties
                .iter()
                .enumerate()
                .find(|(_index, queue_props)| {
                    queue_props.queue_count > 0
                        && queue_props.queue_flags & vk::QueueFlags::TRANSFER
                            == vk::QueueFlags::TRANSFER
                        && queue_props.queue_flags & vk::QueueFlags::COMPUTE
                            != vk::QueueFlags::COMPUTE
                        && queue_props.queue_flags & vk::QueueFlags::GRAPHICS
                            != vk::QueueFlags::GRAPHICS
                });

        let graphics_queue_info = VkQueueInfo {
            queue_family_index: graphics_queue_family_props.0,
            supports_presentation: surface_loader
                .get_physical_device_surface_support(
                    self.physical_device,
                    graphics_queue_family_props.0 as u32,
                    surface.surface_handle(),
                )
                .unwrap_or(false),
        };

        let compute_queue_info = compute_queue_family_props.map(|(index, _)| {
            //There is a separate queue family specifically for compute
            VkQueueInfo {
                queue_family_index: index,
                supports_presentation: surface_loader
                    .get_physical_device_surface_support(
                        self.physical_device,
                        index as u32,
                        surface.surface_handle(),
                    )
                    .unwrap_or(false),
            }
        });

        let transfer_queue_info = transfer_queue_family_props.map(|(index, _)| {
            //There is a separate queue family specifically for transfers
            VkQueueInfo {
                queue_family_index: index,
                supports_presentation: surface_loader
                    .get_physical_device_surface_support(
                        self.physical_device,
                        index as u32,
                        surface.surface_handle(),
                    )
                    .unwrap_or(false),
            }
        });

        let priority = 1.0f32;
        let mut queue_create_descs = vec![vk::DeviceQueueCreateInfo {
            queue_family_index: graphics_queue_info.queue_family_index as u32,
            queue_count: 1,
            p_queue_priorities: &priority as *const f32,
            ..Default::default()
        }];

        if let Some(compute_queue_info) = compute_queue_info {
            queue_create_descs.push(vk::DeviceQueueCreateInfo {
                queue_family_index: compute_queue_info.queue_family_index as u32,
                queue_count: 1,
                p_queue_priorities: &priority as *const f32,
                ..Default::default()
            });
        }

        let lower_priority = 0.1f32;
        if let Some(transfer_queue_info) = transfer_queue_info {
            queue_create_descs.push(vk::DeviceQueueCreateInfo {
                queue_family_index: transfer_queue_info.queue_family_index as u32,
                queue_count: 1,
                p_queue_priorities: &lower_priority as *const f32,
                ..Default::default()
            });
        }

        let mut supported_features: vk::PhysicalDeviceFeatures2 = Default::default();
        let mut supported_features_11: vk::PhysicalDeviceVulkan11Features = Default::default();
        let mut supported_features_12: vk::PhysicalDeviceVulkan12Features = Default::default();
        let mut supported_features_13: vk::PhysicalDeviceVulkan13Features = Default::default();
        let mut properties: vk::PhysicalDeviceProperties2 = Default::default();
        let mut properties_11: vk::PhysicalDeviceVulkan11Properties = Default::default();
        let mut properties_12: vk::PhysicalDeviceVulkan12Properties = Default::default();
        let mut properties_13: vk::PhysicalDeviceVulkan13Properties = Default::default();
        let mut properties_rt_pipeline =
            vk::PhysicalDeviceRayTracingPipelinePropertiesKHR::default();
        let mut properties_host_image_copy =
            vk::PhysicalDeviceHostImageCopyPropertiesEXT::default();
        let mut supported_features_acceleration_structure =
            vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default();
        let mut properties_acceleration_structure =
            vk::PhysicalDeviceAccelerationStructurePropertiesKHR::default();
        let mut supported_features_rt_pipeline =
            vk::PhysicalDeviceRayTracingPipelineFeaturesKHR::default();
        let mut supported_features_rt_query =
            vk::PhysicalDeviceRayQueryFeaturesKHR::default();
        let mut supported_features_barycentrics =
            VkPhysicalDeviceFragmentShaderBarycentricFeaturesNV::default();
        let mut supported_features_host_image_copy =
            vk::PhysicalDeviceHostImageCopyFeaturesEXT::default();
        let mut supported_features_mesh_shader =
            vk::PhysicalDeviceMeshShaderFeaturesEXT::default();
        let mut properties_mesh_shader =
            vk::PhysicalDeviceMeshShaderPropertiesEXT::default();

        supported_features_11.p_next = std::mem::replace(
            &mut supported_features.p_next,
            &mut supported_features_11 as *mut vk::PhysicalDeviceVulkan11Features
                as *mut c_void,
        );

        supported_features_12.p_next = std::mem::replace(
            &mut supported_features.p_next,
            &mut supported_features_12 as *mut vk::PhysicalDeviceVulkan12Features
                as *mut c_void,
        );

        supported_features_13.p_next = std::mem::replace(
            &mut supported_features.p_next,
            &mut supported_features_13 as *mut vk::PhysicalDeviceVulkan13Features
                as *mut c_void,
        );

        properties_11.p_next = std::mem::replace(
            &mut properties.p_next,
            &mut properties_11 as *mut vk::PhysicalDeviceVulkan11Properties
                as *mut c_void,
        );

        properties_12.p_next = std::mem::replace(
            &mut properties.p_next,
            &mut properties_12 as *mut vk::PhysicalDeviceVulkan12Properties
                as *mut c_void,
        );

        properties_13.p_next = std::mem::replace(
            &mut properties.p_next,
            &mut properties_13 as *mut vk::PhysicalDeviceVulkan13Properties
                as *mut c_void,
        );

        if extensions
            .intersects(VkAdapterExtensionSupport::ACCELERATION_STRUCTURE)
        {
            properties_acceleration_structure.p_next = std::mem::replace(
                &mut properties.p_next,
                &mut properties_acceleration_structure
                    as *mut vk::PhysicalDeviceAccelerationStructurePropertiesKHR
                    as *mut c_void,
            );

            supported_features_acceleration_structure.p_next = std::mem::replace(
                &mut supported_features.p_next,
                &mut supported_features_acceleration_structure
                    as *mut vk::PhysicalDeviceAccelerationStructureFeaturesKHR
                    as *mut c_void,
            );
        }
        if extensions
            .intersects(VkAdapterExtensionSupport::RAY_TRACING_PIPELINE)
        {
            properties_rt_pipeline.p_next = std::mem::replace(
                &mut properties.p_next,
                &mut properties_rt_pipeline
                    as *mut vk::PhysicalDeviceRayTracingPipelinePropertiesKHR
                    as *mut c_void,
            );

            supported_features_rt_pipeline.p_next = std::mem::replace(
                &mut supported_features.p_next,
                &mut supported_features_rt_pipeline
                    as *mut vk::PhysicalDeviceRayTracingPipelineFeaturesKHR
                    as *mut c_void,
            );
        }
        if extensions
            .intersects(VkAdapterExtensionSupport::RAY_QUERY)
        {
            supported_features_rt_query.p_next = std::mem::replace(
                &mut supported_features.p_next,
                &mut supported_features_rt_query
                    as *mut vk::PhysicalDeviceRayQueryFeaturesKHR
                    as *mut c_void,
            );
        }

        if !supported_features
            .features
            .shader_storage_image_write_without_format
            == vk::TRUE
        {
            panic!("Your Vulkan driver is not capable of running this application. ShaderStorageImageWriteWithoutFormat is a required feature!");
        }

        if !supported_features_13.dynamic_rendering == vk::TRUE {
            panic!("Your Vulkan driver is not capable of running this application. Dynamic rendering is a required feature!");
        }

        if !supported_features_12.host_query_reset == vk::TRUE {
            panic!("Your Vulkan driver is not capable of running this application. Host query reset is a required feature!");
        }

        if extensions
            .intersects(VkAdapterExtensionSupport::BARYCENTRICS)
        {
            supported_features_barycentrics.p_next = std::mem::replace(
                &mut supported_features.p_next,
                &mut supported_features_barycentrics
                    as *mut VkPhysicalDeviceFragmentShaderBarycentricFeaturesNV
                    as *mut c_void,
            );
        }

        if extensions.intersects(VkAdapterExtensionSupport::HOST_IMAGE_COPY) {
            supported_features_host_image_copy.p_next = std::mem::replace(
                &mut supported_features.p_next,
                &mut supported_features_host_image_copy
                    as *mut vk::PhysicalDeviceHostImageCopyFeaturesEXT
                    as *mut c_void,
            );
            properties_host_image_copy.p_next = std::mem::replace(
                &mut properties.p_next,
                &mut properties_host_image_copy
                    as *mut vk::PhysicalDeviceHostImageCopyPropertiesEXT
                    as *mut c_void,
            );
        }

        if extensions
            .intersects(VkAdapterExtensionSupport::MESH_SHADER)
        {
            supported_features_mesh_shader.p_next = std::mem::replace(
                &mut supported_features.p_next,
                &mut supported_features_mesh_shader
                    as *mut vk::PhysicalDeviceMeshShaderFeaturesEXT
                    as *mut c_void,
            );
            properties_mesh_shader.p_next = std::mem::replace(
                &mut properties.p_next,
                &mut properties_mesh_shader
                    as *mut vk::PhysicalDeviceMeshShaderPropertiesEXT
                    as *mut c_void,
            );
        }

        self.instance
            .get_physical_device_features2(self.physical_device, &mut supported_features);
        self.instance
            .get_physical_device_properties2(self.physical_device, &mut properties);

        let mut enabled_features: vk::PhysicalDeviceFeatures2 = Default::default();
        let mut enabled_features_11: vk::PhysicalDeviceVulkan11Features = Default::default();
        let mut enabled_features_12: vk::PhysicalDeviceVulkan12Features = Default::default();
        let mut enabled_features_13: vk::PhysicalDeviceVulkan13Features = Default::default();

        let mut features_acceleration_structure =
            vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default();
        let mut features_rt_pipeline =
            vk::PhysicalDeviceRayTracingPipelineFeaturesKHR::default();
        let mut features_rt_query =
            vk::PhysicalDeviceRayQueryFeaturesKHR::default();
        let mut features_barycentrics =
            vk::PhysicalDeviceFragmentShaderBarycentricFeaturesKHR::default();
        let mut features_host_image_copy = vk::PhysicalDeviceHostImageCopyFeaturesEXT::default();
        let mut features_mesh_shader = vk::PhysicalDeviceMeshShaderFeaturesEXT::default();
        let mut extension_names: Vec<&str> = vec![SWAPCHAIN_EXT_NAME];

        enabled_features.features.shader_storage_image_write_without_format = vk::TRUE;
        enabled_features.features.sampler_anisotropy = vk::TRUE;
        enabled_features_12.host_query_reset = vk::TRUE;
        enabled_features_13.dynamic_rendering = vk::TRUE;

        enabled_features_11.p_next = std::mem::replace(
            &mut enabled_features.p_next,
            &mut enabled_features_11 as *mut vk::PhysicalDeviceVulkan11Features
                as *mut c_void,
        );

        enabled_features_12.p_next = std::mem::replace(
            &mut enabled_features.p_next,
            &mut enabled_features_12 as *mut vk::PhysicalDeviceVulkan12Features
                as *mut c_void,
        );

        enabled_features_13.p_next = std::mem::replace(
            &mut enabled_features.p_next,
            &mut enabled_features_13 as *mut vk::PhysicalDeviceVulkan13Features
                as *mut c_void,
        );

        if supported_features.features.shader_int16 == vk::TRUE
            && supported_features_11.storage_buffer16_bit_access == vk::TRUE
        {
            enabled_features_11.storage_buffer16_bit_access = vk::TRUE;
            enabled_features.features.shader_int16 = vk::TRUE;
        }

        if extensions
            .intersects(VkAdapterExtensionSupport::MEMORY_BUDGET)
        {
            extension_names.push(MEMORY_BUDGET_EXT_NAME);
        }

        let supports_descriptor_indexing = supported_features_12
                .shader_sampled_image_array_non_uniform_indexing
                == vk::TRUE
            && supported_features_12
                .descriptor_binding_sampled_image_update_after_bind
                == vk::TRUE
            && supported_features_12
                .descriptor_binding_variable_descriptor_count
                == vk::TRUE
            && supported_features_12.runtime_descriptor_array == vk::TRUE
            && supported_features_12.descriptor_binding_partially_bound
                == vk::TRUE
            && supported_features_12
                .descriptor_binding_update_unused_while_pending
                == vk::TRUE
            && properties_12
                .shader_sampled_image_array_non_uniform_indexing_native
                == vk::TRUE
            && properties_12
                .max_descriptor_set_update_after_bind_sampled_images
                > BINDLESS_TEXTURE_COUNT;

        let supports_bda = supported_features_12.buffer_device_address == vk::TRUE;

        let supports_rt_pipeline = extensions.contains(
            VkAdapterExtensionSupport::ACCELERATION_STRUCTURE
                | VkAdapterExtensionSupport::RAY_TRACING_PIPELINE
                | VkAdapterExtensionSupport::PIPELINE_LIBRARY
            )
            && supported_features_acceleration_structure.acceleration_structure == vk::TRUE
            && supported_features_rt_pipeline.ray_tracing_pipeline == vk::TRUE
            && supports_bda;

        let supports_rt_query = supports_descriptor_indexing
            && extensions.contains(
                VkAdapterExtensionSupport::ACCELERATION_STRUCTURE
                    | VkAdapterExtensionSupport::RAY_QUERY
            )
            && supported_features_rt_query.ray_query == vk::TRUE
            && supports_bda;

        if supports_descriptor_indexing {
            println!("Bindless supported.");
            enabled_features_12.shader_sampled_image_array_non_uniform_indexing =
                vk::TRUE;
            enabled_features_12.descriptor_binding_sampled_image_update_after_bind =
                vk::TRUE;
            enabled_features_12.descriptor_binding_variable_descriptor_count =
                vk::TRUE;
            enabled_features_12.runtime_descriptor_array = vk::TRUE;
            enabled_features_12.descriptor_binding_partially_bound = vk::TRUE;
            enabled_features_12.descriptor_binding_update_unused_while_pending =
                vk::TRUE;
            enabled_features_12.descriptor_indexing = vk::TRUE;
        }

        if supports_rt_pipeline || supports_rt_query {
            extension_names.push(ACCELERATION_STRUCTURE_EXT_NAME);
            enabled_features_12.buffer_device_address = vk::TRUE;
            features_acceleration_structure.acceleration_structure = vk::TRUE;
            features_acceleration_structure.p_next = std::mem::replace(
                &mut enabled_features.p_next,
                &mut features_acceleration_structure
                    as *mut vk::PhysicalDeviceAccelerationStructureFeaturesKHR
                    as *mut c_void,
            );
        }

        if supports_rt_pipeline {
            println!("Ray tracing pipelines supported.");
            extension_names.push(RAY_TRACING_PIPELINE_EXT_NAME);
            if extensions.contains(VkAdapterExtensionSupport::DEFERRED_HOST_OPERATIONS) {
                extension_names.push(DEFERRED_HOST_OPERATIONS_EXT_NAME);
            }
            extension_names.push(PIPELINE_LIBRARY_EXT_NAME);
            features_rt_pipeline.ray_tracing_pipeline = vk::TRUE;
            features_rt_pipeline.p_next = std::mem::replace(
                &mut enabled_features.p_next,
                &mut features_rt_pipeline
                    as *mut vk::PhysicalDeviceRayTracingPipelineFeaturesKHR
                    as *mut c_void,
            );
        }

        if supports_rt_query {
            println!("Ray tracing queries supported.");
            extension_names.push(RAY_QUERY_EXT_NAME);
            features_rt_query.ray_query = vk::TRUE;
            features_rt_query.p_next = std::mem::replace(
                &mut enabled_features.p_next,
                &mut features_rt_query
                    as *mut vk::PhysicalDeviceRayQueryFeaturesKHR
                    as *mut c_void,
            );
        }

        enabled_features.features.draw_indirect_first_instance = supported_features.features.draw_indirect_first_instance;
        enabled_features.features.multi_draw_indirect = supported_features.features.multi_draw_indirect;
        enabled_features_12.draw_indirect_count = supported_features_12.draw_indirect_count;

        let supports_filter_min_max = supported_features_12.sampler_filter_minmax == vk::TRUE && properties_12.filter_minmax_single_component_formats == vk::TRUE;
        if supports_filter_min_max {
            enabled_features_12.sampler_filter_minmax = vk::TRUE;
        }

        if supported_features_13.synchronization2 != vk::TRUE
            || supported_features_12.timeline_semaphore != vk::TRUE
        {
            panic!("Timeline semaphores or sync2 unsupported. Update your driver!");
        }
        enabled_features_12.timeline_semaphore = vk::TRUE;
        enabled_features_13.synchronization2 = vk::TRUE;

        if supported_features_barycentrics.fragment_shader_barycentric == vk::TRUE {
            println!("Barycentrics supported.");
            features_barycentrics.fragment_shader_barycentric = vk::TRUE;
            features_barycentrics.p_next = std::mem::replace(
                &mut enabled_features.p_next,
                &mut features_barycentrics
                    as *mut vk::PhysicalDeviceFragmentShaderBarycentricFeaturesKHR
                    as *mut c_void,
            );
            extension_names.push(BARYCENTRICS_EXT_NAME);
            enabled_features.features.geometry_shader = vk::TRUE; // Unfortunately this is necessary for gl_PrimitiveId
        }

        if supported_features_13.maintenance4 == vk::TRUE {
            enabled_features_13.maintenance4 = vk::TRUE;
        }

        if supported_features_host_image_copy.host_image_copy == vk::TRUE {
            extension_names.push(HOST_IMAGE_COPY_EXT_NAME);
            features_host_image_copy.host_image_copy = vk::TRUE;
            features_host_image_copy.p_next = std::mem::replace(
                &mut enabled_features.p_next,
                &mut features_host_image_copy
                    as *mut vk::PhysicalDeviceHostImageCopyFeaturesEXT
                    as *mut c_void,
            );
        }

        if supported_features_mesh_shader.mesh_shader == vk::TRUE
            && supported_features_mesh_shader.task_shader == vk::TRUE {
            extension_names.push(MESH_SHADER_EXT_NAME);
            features_mesh_shader.mesh_shader = vk::TRUE;
            features_mesh_shader.task_shader = vk::TRUE;
            features_mesh_shader.p_next = std::mem::replace(
                &mut enabled_features.p_next,
                &mut features_mesh_shader
                    as *mut vk::PhysicalDeviceMeshShaderFeaturesEXT
                    as *mut c_void,
            );
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
            p_enabled_features: std::ptr::null(),
            pp_enabled_extension_names: extension_names_ptr.as_ptr(),
            enabled_extension_count: extension_names_c.len() as u32,
            p_next: &enabled_features as &vk::PhysicalDeviceFeatures2 as *const vk::PhysicalDeviceFeatures2 as *const c_void,
            ..Default::default()
        };
        let vk_device = self
            .instance
            .instance
            .create_device(self.physical_device, &device_create_info, None)
            .unwrap();


        let graphics_queue = unsafe {
            vk_device.get_device_queue(
                graphics_queue_info.queue_family_index as u32,
                0,
            )
        };
        let compute_queue = compute_queue_info.map(|info| unsafe {
            vk_device.get_device_queue(info.queue_family_index as u32, 0)
        });
        let transfer_queue = transfer_queue_info.map(|info| unsafe {
            vk_device.get_device_queue(info.queue_family_index as u32, 0)
        });

        let mut d24_props = vk::FormatProperties2::default();
        unsafe {
            self.instance.get_physical_device_format_properties2(
                self.physical_device,
                vk::Format::D24_UNORM_S8_UINT,
                &mut d24_props,
            );
        }
        let supports_d24 = d24_props
            .format_properties
            .optimal_tiling_features
            .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT);

        let mut memory_properties = vk::PhysicalDeviceMemoryProperties2::default();
        self.instance.get_physical_device_memory_properties2(self.physical_device, &mut memory_properties);

        let mut supported_shader_stages = vk::ShaderStageFlags::COMPUTE
            | vk::ShaderStageFlags::VERTEX
            | vk::ShaderStageFlags::FRAGMENT;

        let mut supported_pipeline_stages = vk::PipelineStageFlags2::VERTEX_INPUT
            | vk::PipelineStageFlags2::VERTEX_SHADER
            | vk::PipelineStageFlags2::FRAGMENT_SHADER
            | vk::PipelineStageFlags2::BLIT
            | vk::PipelineStageFlags2::CLEAR
            | vk::PipelineStageFlags2::COPY
            | vk::PipelineStageFlags2::RESOLVE
            | vk::PipelineStageFlags2::HOST
            | vk::PipelineStageFlags2::TRANSFER
            | vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
            | vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
            | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS
            | vk::PipelineStageFlags2::CONDITIONAL_RENDERING_EXT
            | vk::PipelineStageFlags2::DRAW_INDIRECT
            | vk::PipelineStageFlags2::COMPUTE_SHADER;

        let mut supported_access_flags = vk::AccessFlags2::SHADER_READ
            | vk::AccessFlags2::SHADER_WRITE
            | vk::AccessFlags2::INDIRECT_COMMAND_READ
            | vk::AccessFlags2::COLOR_ATTACHMENT_READ
            | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
            | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ
            | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE
            | vk::AccessFlags2::VERTEX_ATTRIBUTE_READ
            | vk::AccessFlags2::TRANSFER_READ
            | vk::AccessFlags2::TRANSFER_WRITE
            | vk::AccessFlags2::SHADER_STORAGE_READ
            | vk::AccessFlags2::SHADER_STORAGE_WRITE
            | vk::AccessFlags2::INDEX_READ
            | vk::AccessFlags2::UNIFORM_READ
            | vk::AccessFlags2::HOST_READ
            | vk::AccessFlags2::HOST_WRITE
            | vk::AccessFlags2::SHADER_SAMPLED_READ;

        if supports_rt_pipeline || supports_rt_query {
            supported_pipeline_stages |= vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR
                | vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_COPY_KHR;

            supported_access_flags |= vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR
                | vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR;
        }

        if supports_rt_pipeline {
            supported_shader_stages |= vk::ShaderStageFlags::RAYGEN_KHR
                | vk::ShaderStageFlags::INTERSECTION_KHR
                | vk::ShaderStageFlags::ANY_HIT_KHR
                | vk::ShaderStageFlags::CLOSEST_HIT_KHR
                | vk::ShaderStageFlags::MISS_KHR;
            supported_pipeline_stages |= vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR;
        }

        if features_mesh_shader.mesh_shader == vk::TRUE {
            supported_shader_stages |= vk::ShaderStageFlags::MESH_EXT;
            supported_pipeline_stages |= vk::PipelineStageFlags2::MESH_SHADER_EXT;
        }

        if features_mesh_shader.task_shader == vk::TRUE {
            supported_shader_stages |= vk::ShaderStageFlags::TASK_EXT;
            supported_pipeline_stages |= vk::PipelineStageFlags2::TASK_SHADER_EXT;
        }

        let rt = if supports_rt_pipeline || supports_rt_query {
            Some(RawVkRTEntries {
                rt_pipelines: if supports_rt_pipeline {
                    Some(ash::khr::ray_tracing_pipeline::Device::new(&self.instance, &vk_device))
                } else { None },
                deferred_operations: if extensions.contains(VkAdapterExtensionSupport::DEFERRED_HOST_OPERATIONS) {
                    Some(ash::khr::deferred_host_operations::Device::new(&self.instance, &vk_device))
                } else { None },
                acceleration_structure: ash::khr::acceleration_structure::Device::new(&self.instance, &vk_device),
                features_acceleration_structure:  std::mem::transmute(features_acceleration_structure),
                properties_acceleration_structure:  std::mem::transmute(properties_acceleration_structure),
                properties_rt_pipeline: std::mem::transmute(properties_rt_pipeline),
                features_rt_pipeline: std::mem::transmute(features_rt_pipeline),
                rt_query: supports_rt_query
            })
        } else { None };

        let debug_utils = self.instance.debug_utils.as_ref()
            .map(|_d| ash::ext::debug_utils::Device::new(&self.instance.instance, &vk_device));

        let mut host_image_copy = Option::<RawVkHostImageCopyEntries>::None;
        if features_host_image_copy.host_image_copy == vk::TRUE {
            host_image_copy = Some(
                RawVkHostImageCopyEntries {
                    host_image_copy: ash::ext::host_image_copy::Device::new(&self.instance, &vk_device),
                    properties_host_image_copy: std::mem::transmute(properties_host_image_copy),
                }
            );
        }

        let mut mesh_shader = Option::<RawVkMeshShaderEntries>::None;
        if features_mesh_shader.mesh_shader == vk::TRUE {
            mesh_shader = Some(
                RawVkMeshShaderEntries {
                    mesh_shader: ash::ext::mesh_shader::Device::new(&self.instance, &vk_device),
                    features_mesh_shader: std::mem::transmute(features_mesh_shader),
                    properties_mesh_shader: std::mem::transmute(properties_mesh_shader),
                }
            );
        }

        let raw = Arc::new(RawVkDevice {
            device: vk_device,
            physical_device: self.physical_device,
            instance: self.instance.clone(),
            debug_utils,
            graphics_queue_info,
            compute_queue_info,
            transfer_queue_info,
            is_alive: AtomicBool::new(true),
            graphics_queue: ReentrantMutex::new(graphics_queue),
            compute_queue: compute_queue.map(|queue| ReentrantMutex::new(queue)),
            transfer_queue: transfer_queue.map(|queue| ReentrantMutex::new(queue)),
            rt,
            supports_d24,
            properties: properties.properties,
            properties_11,
            properties_12,
            properties_13,
            features_barycentrics,
            features: enabled_features.features,
            features_11: enabled_features_11,
            features_12: enabled_features_12,
            features_13: enabled_features_13,
            memory_properties: memory_properties.memory_properties,
            feature_memory_budget: extensions.intersects(VkAdapterExtensionSupport::MEMORY_BUDGET),
            supported_shader_stages,
            supported_pipeline_stages,
            supported_access_flags,
            host_image_copy,
            mesh_shader,
        });

        VkDevice::new(
            raw,
            graphics_queue_info,
            compute_queue_info,
            transfer_queue_info
        )
    }

    #[inline(always)]
    fn adapter_type(&self) -> gpu::AdapterType {
        match self.properties.device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => gpu::AdapterType::Discrete,
            vk::PhysicalDeviceType::INTEGRATED_GPU => gpu::AdapterType::Integrated,
            vk::PhysicalDeviceType::VIRTUAL_GPU => gpu::AdapterType::Virtual,
            vk::PhysicalDeviceType::CPU => gpu::AdapterType::Software,
            _ => gpu::AdapterType::Other,
        }
    }

    // TODO: find out if presentation is supported
}
