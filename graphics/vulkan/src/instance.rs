use std::{
    ffi::{
        CStr,
        CString,
    },
    os::raw::{
        c_char,
        c_void,
    },
    sync::Arc,
};

use ash::vk;
use sourcerenderer_core::gpu;

use super::*;

pub struct VkInstance {
    raw: Arc<RawVkInstance>,
    adapters: Vec<VkAdapter>,
}

impl VkInstance {
    pub fn new(instance_extensions: &[&str], debug_layers: bool) -> Self {
        let entry: ash::Entry = unsafe { ash::Entry::load().unwrap() };

        let extensions = entry.enumerate_instance_extension_properties(None).unwrap();
        let layers = entry.enumerate_instance_layer_properties().unwrap();
        let mut supports_khronos_validation = false;
        let mut supports_debug_utils = false;
        for layer in &layers {
            let name = unsafe { CStr::from_ptr(&layer.layer_name as *const c_char) };
            match name.to_str().unwrap() {
                "VK_LAYER_KHRONOS_validation" => {
                    supports_khronos_validation = true;
                }
                _ => {}
            }
        }
        for extension in &extensions {
            let name = unsafe { CStr::from_ptr(&extension.extension_name as *const c_char) };
            let debug_utils_name = ash::extensions::ext::DebugUtils::name();
            if name == debug_utils_name {
                supports_debug_utils = true;
            }
        }

        let app_name = CString::new("Dreieck").unwrap();
        let app_name_ptr = app_name.as_ptr();
        let engine_name = CString::new("Dreieck").unwrap();
        let engine_name_ptr = engine_name.as_ptr();

        let mut layer_names_c: Vec<CString> = Vec::new();
        /* The layers are loaded in the order they are listed in this array,
         * with the first array element being the closest to the application,
         * and the last array element being the closest to the driver.
         */

        if debug_layers {
            if supports_khronos_validation {
                layer_names_c.push(CString::new("VK_LAYER_KHRONOS_validation").unwrap());
            } else {
                println!("Validation layers not installed");
            }
        }

        if cfg!(target_os = "android") {
            println!("Activating synchronization2 and timeline semaphore fallback layers");
            layer_names_c.push(CString::new("VK_LAYER_KHRONOS_synchronization2").unwrap());
            layer_names_c.push(CString::new("VK_LAYER_KHRONOS_timeline_semaphore").unwrap());
        }

        let layer_names_ptr: Vec<*const c_char> = layer_names_c
            .iter()
            .map(|raw_name| raw_name.as_ptr())
            .collect();

        let mut extension_names_c: Vec<CString> = instance_extensions
            .iter()
            .map(|ext| CString::new(*ext).unwrap())
            .collect();
        if supports_debug_utils {
            extension_names_c.push(CString::from(ash::extensions::ext::DebugUtils::name()));
        } else {
            println!("Vulkan debug utils are unsupported");
        }
        let extension_names_ptr: Vec<*const c_char> = extension_names_c
            .iter()
            .map(|ext_c| ext_c.as_ptr())
            .collect();

        let app_info = vk::ApplicationInfo {
            api_version: vk::make_api_version(0, 1, 3, 280),
            application_version: vk::make_api_version(0, 0, 0, 1),
            engine_version: vk::make_api_version(0, 0, 0, 1),
            p_application_name: app_name_ptr,
            p_engine_name: engine_name_ptr,
            ..Default::default()
        };

        let instance_create_info = vk::InstanceCreateInfo {
            p_application_info: &app_info,
            pp_enabled_layer_names: layer_names_ptr.as_ptr(),
            enabled_layer_count: layer_names_ptr.len() as u32,
            pp_enabled_extension_names: extension_names_ptr.as_ptr(),
            enabled_extension_count: extension_names_ptr.len() as u32,
            ..Default::default()
        };

        unsafe {
            let instance = entry.create_instance(&instance_create_info, None).unwrap();

            let debug_utils = if supports_debug_utils {
                let debug_utils_loader = ash::extensions::ext::DebugUtils::new(&entry, &instance);
                let debug_messenger = debug_utils_loader
                    .create_debug_utils_messenger(
                        &vk::DebugUtilsMessengerCreateInfoEXT {
                            flags: vk::DebugUtilsMessengerCreateFlagsEXT::empty(),
                            message_severity: vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE
                                | vk::DebugUtilsMessageSeverityFlagsEXT::INFO
                                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                                | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
                            message_type: vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                            pfn_user_callback: Some(VkInstance::debug_callback),
                            p_user_data: std::ptr::null_mut(),
                            ..Default::default()
                        },
                        None,
                    )
                    .unwrap();
                Some(RawVkDebugUtils {
                    debug_messenger,
                    debug_utils_loader,
                })
            } else {
                None
            };

            let raw = Arc::new(RawVkInstance {
                entry,
                instance,
                debug_utils,
            });

            let physical_devices: Vec<vk::PhysicalDevice> =
                raw.instance.enumerate_physical_devices().unwrap();
            let adapters: Vec<VkAdapter> = physical_devices
                .into_iter()
                .map(|phys_dev| VkAdapter::new(&raw, phys_dev))
                .collect();

            VkInstance { raw, adapters }
        }
    }

    pub fn raw(&self) -> &Arc<RawVkInstance> {
        &self.raw
    }

    unsafe extern "system" fn debug_callback(
        message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
        message_types: vk::DebugUtilsMessageTypeFlagsEXT,
        p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
        _p_user_data: *mut c_void,
    ) -> vk::Bool32 {
        let callback_data_opt = p_callback_data.as_ref();
        if callback_data_opt.is_none() {
            return vk::FALSE;
        }
        let callback_data = callback_data_opt.unwrap();

        if message_severity == vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE {
            return vk::FALSE;
        }

        if callback_data.message_id_number == 688222058 {
            // False positive about setting the viewport & scissor for ray tracing pipelines
            return vk::FALSE;
        }

        if message_severity != vk::DebugUtilsMessageSeverityFlagsEXT::INFO || message_severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
            println!(
                "VK: {:?} - {:?}: {:?}",
                message_severity,
                message_types,
                CStr::from_ptr(callback_data.p_message)
            );
        } else {
            println!(
                "VK: {:?} - {:?}: {:?}",
                message_severity,
                message_types,
                CStr::from_ptr(callback_data.p_message)
            );
        }
        vk::FALSE
    }
}

impl gpu::Instance<VkBackend> for VkInstance {
    fn list_adapters(&self) -> &[VkAdapter] {
        &self.adapters
    }
}
