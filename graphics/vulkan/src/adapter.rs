use std::{
    f32,
    ffi::{
        c_void,
        CStr,
        CString,
    },
    os::raw::c_char,
    sync::Arc,
};

use ash::{
    extensions::khr::Surface as KhrSurface,
    vk,
};
use sourcerenderer_core::gpu::*;

use super::*;

const SWAPCHAIN_EXT_NAME: &str = "VK_KHR_swapchain";
const MEMORY_BUDGET_EXT_NAME: &str = "VK_EXT_memory_budget";
const ACCELERATION_STRUCTURE_EXT_NAME: &str = "VK_KHR_acceleration_structure";
const DEFERRED_HOST_OPERATIONS_EXT_NAME: &str = "VK_KHR_deferred_host_operations";
const RAY_TRACING_PIPELINE_EXT_NAME: &str = "VK_KHR_ray_tracing_pipeline";
const RAY_QUERY_EXT_NAME: &str = "VK_KHR_ray_query";
const PIPELINE_LIBRARY_EXT_NAME: &str = "VK_KHR_pipeline_library";
const BARYCENTRICS_EXT_NAME: &str = "VK_NV_fragment_shader_barycentric"; // TODO: Use VK_KHR_fragment_shader_barycentric

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
    extensions: VkAdapterExtensionSupport,
}

impl VkAdapter {
    pub fn new(instance: &Arc<RawVkInstance>, physical_device: vk::PhysicalDevice) -> Self {
        let properties = unsafe {
            instance
                .instance
                .get_physical_device_properties(physical_device)
        };

        let mut extensions = VkAdapterExtensionSupport::NONE;

        let supported_extensions = unsafe {
            instance
                .instance
                .enumerate_device_extension_properties(physical_device)
        }
        .unwrap();
        for ref prop in supported_extensions {
            let name_ptr = &prop.extension_name as *const c_char;
            let c_char_ptr = name_ptr as *const c_char;
            let name_res = unsafe { CStr::from_ptr(c_char_ptr) }.to_str();
            let name = name_res.unwrap();
            extensions |= match name {
                SWAPCHAIN_EXT_NAME => VkAdapterExtensionSupport::SWAPCHAIN,
                MEMORY_BUDGET_EXT_NAME => VkAdapterExtensionSupport::MEMORY_BUDGET,
                ACCELERATION_STRUCTURE_EXT_NAME => {
                    VkAdapterExtensionSupport::ACCELERATION_STRUCTURE
                }
                PIPELINE_LIBRARY_EXT_NAME => VkAdapterExtensionSupport::PIPELINE_LIBRARY,
                RAY_QUERY_EXT_NAME => VkAdapterExtensionSupport::RAY_QUERY,
                RAY_TRACING_PIPELINE_EXT_NAME => VkAdapterExtensionSupport::RAY_TRACING_PIPELINE,
                DEFERRED_HOST_OPERATIONS_EXT_NAME => {
                    VkAdapterExtensionSupport::DEFERRED_HOST_OPERATIONS
                }
                BARYCENTRICS_EXT_NAME => VkAdapterExtensionSupport::BARYCENTRICS,
                _ => VkAdapterExtensionSupport::NONE,
            };
        }

        VkAdapter {
            instance: instance.clone(),
            physical_device,
            properties,
            extensions,
        }
    }

    pub fn physical_device_handle(&self) -> vk::PhysicalDevice {
        self.physical_device
    }

    pub fn raw_instance(&self) -> &Arc<RawVkInstance> {
        &self.instance
    }
}

// Vulkan physical devices are implicitly freed with the instance

pub(crate) const BINDLESS_TEXTURE_COUNT: u32 = 500_000;

impl Adapter<VkBackend> for VkAdapter {
    fn create_device(&self, surface: &VkSurface) -> VkDevice {
        return unsafe {
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
                queue_index: 0,
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
                    queue_index: 0,
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
                    queue_index: 0,
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
            let mut supported_descriptor_indexing_features =
                vk::PhysicalDeviceDescriptorIndexingFeaturesEXT::default();
            let mut descriptor_indexing_properties =
                vk::PhysicalDeviceDescriptorIndexingPropertiesEXT::default();
            let mut supported_acceleration_structure_features =
                vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default();
            let mut supported_rt_pipeline_features =
                vk::PhysicalDeviceRayTracingPipelineFeaturesKHR::default();
            let mut supported_bda_features =
                vk::PhysicalDeviceBufferDeviceAddressFeaturesKHR::default();
            let mut supported_timeline_semaphore_features =
                vk::PhysicalDeviceTimelineSemaphoreFeatures::default();
            let mut supported_sync2_features =
                vk::PhysicalDeviceSynchronization2Features::default();
            let mut filter_min_max_properties =
                vk::PhysicalDeviceSamplerFilterMinmaxProperties::default();
            let mut supported_barycentrics_features =
                VkPhysicalDeviceFragmentShaderBarycentricFeaturesNV::default();
            let mut supported_16bit_features = vk::PhysicalDevice16BitStorageFeatures::default();
            let mut supported_maintenance4_features = vk::PhysicalDeviceMaintenance4Features::default();

            supported_16bit_features.p_next = std::mem::replace(
                &mut supported_features.p_next,
                &mut supported_16bit_features as *mut vk::PhysicalDevice16BitStorageFeatures
                    as *mut c_void,
            );


            supported_descriptor_indexing_features.p_next = std::mem::replace(
                &mut supported_features.p_next,
                &mut supported_descriptor_indexing_features
                    as *mut vk::PhysicalDeviceDescriptorIndexingFeaturesEXT
                    as *mut c_void,
            );
            descriptor_indexing_properties.p_next = std::mem::replace(
                &mut properties.p_next,
                &mut descriptor_indexing_properties
                    as *mut vk::PhysicalDeviceDescriptorIndexingPropertiesEXT
                    as *mut c_void,
            );
            if self
                .extensions
                .intersects(VkAdapterExtensionSupport::ACCELERATION_STRUCTURE)
            {
                supported_acceleration_structure_features.p_next = std::mem::replace(
                    &mut supported_features.p_next,
                    &mut supported_acceleration_structure_features
                        as *mut vk::PhysicalDeviceAccelerationStructureFeaturesKHR
                        as *mut c_void,
                );
            }
            if self
                .extensions
                .intersects(VkAdapterExtensionSupport::RAY_TRACING_PIPELINE)
            {
                supported_rt_pipeline_features.p_next = std::mem::replace(
                    &mut supported_features.p_next,
                    &mut supported_rt_pipeline_features
                        as *mut vk::PhysicalDeviceRayTracingPipelineFeaturesKHR
                        as *mut c_void,
                );
            }

            supported_bda_features.p_next = std::mem::replace(
                &mut supported_features.p_next,
                &mut supported_bda_features
                    as *mut vk::PhysicalDeviceBufferDeviceAddressFeaturesKHR
                    as *mut c_void,
            );


            filter_min_max_properties.p_next = std::mem::replace(
                &mut properties.p_next,
                &mut filter_min_max_properties
                    as *mut vk::PhysicalDeviceSamplerFilterMinmaxProperties
                    as *mut c_void,
            );

            if !supported_features
                .features
                .shader_storage_image_write_without_format
                == vk::TRUE
            {
                panic!("Your Vulkan driver is not capable of running this application. ShaderStorageImageWriteWithoutFormat is a required feature!");
            }

            if self
                .extensions
                .intersects(VkAdapterExtensionSupport::BARYCENTRICS)
            {
                supported_barycentrics_features.p_next = std::mem::replace(
                    &mut supported_features.p_next,
                    &mut supported_barycentrics_features
                        as *mut VkPhysicalDeviceFragmentShaderBarycentricFeaturesNV
                        as *mut c_void,
                );
            }

            supported_timeline_semaphore_features.p_next = std::mem::replace(
                &mut supported_features.p_next,
                &mut supported_timeline_semaphore_features
                    as *mut vk::PhysicalDeviceTimelineSemaphoreFeatures
                    as *mut c_void,
            );
            supported_sync2_features.p_next = std::mem::replace(
                &mut supported_features.p_next,
                &mut supported_sync2_features as *mut vk::PhysicalDeviceSynchronization2Features
                    as *mut c_void,
            );

            supported_maintenance4_features.p_next = std::mem::replace(
                &mut supported_features.p_next,
                &mut supported_maintenance4_features
                    as *mut vk::PhysicalDeviceMaintenance4Features
                    as *mut c_void,
            );

            self.instance
                .get_physical_device_features2(self.physical_device, &mut supported_features);
            self.instance
                .get_physical_device_properties2(self.physical_device, &mut properties);

            let mut enabled_features: vk::PhysicalDeviceFeatures = Default::default();
            let mut descriptor_indexing_features =
                vk::PhysicalDeviceDescriptorIndexingFeaturesEXT::default();
            let mut acceleration_structure_features =
                vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default();
            let mut rt_pipeline_features =
                vk::PhysicalDeviceRayTracingPipelineFeaturesKHR::default();
            let mut bda_features = vk::PhysicalDeviceBufferDeviceAddressFeaturesKHR::default();
            let mut timeline_semaphore_features =
                vk::PhysicalDeviceTimelineSemaphoreFeatures::default();
            let mut sync2_features = vk::PhysicalDeviceSynchronization2Features::default();
            let mut barycentrics_features =
                VkPhysicalDeviceFragmentShaderBarycentricFeaturesNV::default();
            let mut sixteen_bit_features = vk::PhysicalDevice16BitStorageFeatures::default();
            let mut extension_names: Vec<&str> = vec![SWAPCHAIN_EXT_NAME];
            let mut maintenance4_features = vk::PhysicalDeviceMaintenance4Features::default();
            let mut device_creation_pnext: *mut c_void = std::ptr::null_mut();

            enabled_features.shader_storage_image_write_without_format = vk::TRUE;

            if supported_features.features.shader_int16 == vk::TRUE
                && supported_16bit_features.storage_buffer16_bit_access == vk::TRUE
            {
                sixteen_bit_features.storage_buffer16_bit_access = vk::TRUE;
                enabled_features.shader_int16 = vk::TRUE;
            }

            if self
                .extensions
                .intersects(VkAdapterExtensionSupport::MEMORY_BUDGET)
            {
                extension_names.push(MEMORY_BUDGET_EXT_NAME);
                features |= VkFeatures::MEMORY_BUDGET;
            }

            let supports_descriptor_indexing = supported_descriptor_indexing_features
                    .shader_sampled_image_array_non_uniform_indexing
                    == vk::TRUE
                && supported_descriptor_indexing_features
                    .descriptor_binding_sampled_image_update_after_bind
                    == vk::TRUE
                && supported_descriptor_indexing_features
                    .descriptor_binding_variable_descriptor_count
                    == vk::TRUE
                && supported_descriptor_indexing_features.runtime_descriptor_array == vk::TRUE
                && supported_descriptor_indexing_features.descriptor_binding_partially_bound
                    == vk::TRUE
                && supported_descriptor_indexing_features
                    .descriptor_binding_update_unused_while_pending
                    == vk::TRUE
                && descriptor_indexing_properties
                    .shader_sampled_image_array_non_uniform_indexing_native
                    == vk::TRUE
                && descriptor_indexing_properties
                    .max_descriptor_set_update_after_bind_sampled_images
                    > BINDLESS_TEXTURE_COUNT;

            let supports_bda = supported_bda_features.buffer_device_address == vk::TRUE;

            let supports_indirect = supported_features.features.draw_indirect_first_instance == vk::TRUE
                && supported_features.features.multi_draw_indirect == vk::TRUE
                && supports_bda;

            let supports_rt = supports_descriptor_indexing
                && self.extensions.contains(
                    VkAdapterExtensionSupport::ACCELERATION_STRUCTURE
                        | VkAdapterExtensionSupport::RAY_TRACING_PIPELINE
                        | VkAdapterExtensionSupport::DEFERRED_HOST_OPERATIONS,
                )
                && supported_acceleration_structure_features.acceleration_structure == vk::TRUE
                && supported_rt_pipeline_features.ray_tracing_pipeline == vk::TRUE
                && supports_bda;

            if supports_descriptor_indexing {
                println!("Bindless supported.");
                descriptor_indexing_features.p_next = std::mem::replace(
                    &mut device_creation_pnext,
                    &mut descriptor_indexing_features
                        as *mut vk::PhysicalDeviceDescriptorIndexingFeaturesEXT
                        as *mut c_void,
                );
                descriptor_indexing_features.shader_sampled_image_array_non_uniform_indexing =
                    vk::TRUE;
                descriptor_indexing_features.descriptor_binding_sampled_image_update_after_bind =
                    vk::TRUE;
                descriptor_indexing_features.descriptor_binding_variable_descriptor_count =
                    vk::TRUE;
                descriptor_indexing_features.runtime_descriptor_array = vk::TRUE;
                descriptor_indexing_features.descriptor_binding_partially_bound = vk::TRUE;
                descriptor_indexing_features.descriptor_binding_update_unused_while_pending =
                    vk::TRUE;
                features |= VkFeatures::DESCRIPTOR_INDEXING;
            }

            if supports_rt {
                println!("Ray tracing supported.");
                extension_names.push(DEFERRED_HOST_OPERATIONS_EXT_NAME);
                extension_names.push(ACCELERATION_STRUCTURE_EXT_NAME);
                extension_names.push(RAY_TRACING_PIPELINE_EXT_NAME);
                extension_names.push(PIPELINE_LIBRARY_EXT_NAME);

                features |= VkFeatures::RAY_TRACING;
                acceleration_structure_features.acceleration_structure = vk::TRUE;
                rt_pipeline_features.ray_tracing_pipeline = vk::TRUE;
                acceleration_structure_features.p_next = std::mem::replace(
                    &mut device_creation_pnext,
                    &mut acceleration_structure_features
                        as *mut vk::PhysicalDeviceAccelerationStructureFeaturesKHR
                        as *mut c_void,
                );
                rt_pipeline_features.p_next = std::mem::replace(
                    &mut device_creation_pnext,
                    &mut rt_pipeline_features
                        as *mut vk::PhysicalDeviceRayTracingPipelineFeaturesKHR
                        as *mut c_void,
                );
            }

            if supports_indirect {
                println!("GPU driven rendering supported.");
                features |= VkFeatures::ADVANCED_INDIRECT;
                enabled_features.draw_indirect_first_instance = vk::TRUE;
                enabled_features.multi_draw_indirect = vk::TRUE;
            }

            if supports_bda && supports_rt {
                bda_features.buffer_device_address = vk::TRUE;
                bda_features.p_next = std::mem::replace(
                    &mut device_creation_pnext,
                    &mut bda_features as *mut vk::PhysicalDeviceBufferDeviceAddressFeaturesKHR
                        as *mut c_void,
                );
            }

            let supports_filter_min_max = filter_min_max_properties.filter_minmax_single_component_formats == vk::TRUE;
            if supports_filter_min_max {
                features |= VkFeatures::MIN_MAX_FILTER;
            }

            if supported_sync2_features.synchronization2 != vk::TRUE
                || supported_timeline_semaphore_features.timeline_semaphore != vk::TRUE
            {
                panic!("Timeline semaphores or sync2 unsupported. Update your driver!");
            }
            timeline_semaphore_features.timeline_semaphore = vk::TRUE;
            sync2_features.synchronization2 = vk::TRUE;
            timeline_semaphore_features.p_next = std::mem::replace(
                &mut device_creation_pnext,
                &mut timeline_semaphore_features as *mut vk::PhysicalDeviceTimelineSemaphoreFeatures
                    as *mut c_void,
            );
            sync2_features.p_next = std::mem::replace(
                &mut device_creation_pnext,
                &mut sync2_features as *mut vk::PhysicalDeviceSynchronization2Features
                    as *mut c_void,
            );

            if supported_barycentrics_features.fragment_shader_barycentric == vk::TRUE {
                println!("Barycentrics supported.");
                barycentrics_features.fragment_shader_barycentric = vk::TRUE;
                barycentrics_features.p_next = std::mem::replace(
                    &mut device_creation_pnext,
                    &mut barycentrics_features
                        as *mut VkPhysicalDeviceFragmentShaderBarycentricFeaturesNV
                        as *mut c_void,
                );
                extension_names.push(BARYCENTRICS_EXT_NAME);
                features |= VkFeatures::BARYCENTRICS;
                enabled_features.geometry_shader = vk::TRUE; // Unfortunately this is necessary for gl_PrimitiveId
            }

            sixteen_bit_features.p_next = std::mem::replace(
                &mut device_creation_pnext,
                &mut sixteen_bit_features as *mut vk::PhysicalDevice16BitStorageFeatures
                    as *mut c_void,
            );

            if supported_maintenance4_features.maintenance4 == vk::TRUE {
                maintenance4_features.maintenance4 = vk::TRUE;
                maintenance4_features.p_next = std::mem::replace(
                    &mut device_creation_pnext,
                    &mut maintenance4_features as *mut vk::PhysicalDeviceMaintenance4Features
                        as *mut c_void,
                );

                features |= VkFeatures::MAINTENANCE4;
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
                p_next: device_creation_pnext,
                pp_enabled_layer_names: std::ptr::null(),
                enabled_layer_count: 0,
                ..Default::default()
            };
            let vk_device = self
                .instance
                .instance
                .create_device(self.physical_device, &device_create_info, None)
                .unwrap();

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
                max_image_count,
            )
        };
    }

    fn adapter_type(&self) -> AdapterType {
        match self.properties.device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => AdapterType::Discrete,
            vk::PhysicalDeviceType::INTEGRATED_GPU => AdapterType::Integrated,
            vk::PhysicalDeviceType::VIRTUAL_GPU => AdapterType::Virtual,
            vk::PhysicalDeviceType::CPU => AdapterType::Software,
            _ => AdapterType::Other,
        }
    }

    // TODO: find out if presentation is supported
}
