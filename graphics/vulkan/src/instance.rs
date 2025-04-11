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

const SURFACE_WAYLAND_EXT_NAME: &str = "VK_KHR_wayland_surface";
const SURFACE_EXT_NAME: &str = "VK_KHR_surface";
const SURFACE_XCB_EXT_NAME: &str = "VK_KHR_xcb_surface";
const SURFACE_XLIB_EXT_NAME: &str = "VK_KHR_xlib_surface";
const SURFACE_ANDROID_EXT_NAME: &str = "VK_KHR_android_surface";
const SURFACE_WIN32_EXT_NAME: &str = "VK_KHR_win32_surface";
const DEBUG_UTILS_EXT_NAME: &str = "VK_EXT_debug_utils";
const VALIDATION_LAYER_NAME: &str = "VK_LAYER_KHRONOS_validation";

impl VkInstance {
    pub fn new(debug_layers: bool) -> Self {
        let entry: ash::Entry = unsafe { ash::Entry::load().unwrap() };

        /* The layers are loaded in the order they are listed in this array,
         * with the first array element being the closest to the application,
         * and the last array element being the closest to the driver.
         */
        let mut enabled_layers: Vec<&str> = Vec::new();
        let supported_layers = unsafe { entry.enumerate_instance_layer_properties() }.unwrap();
        let mut supports_khronos_validation = false;
        for layer in &supported_layers {
            let name_c = unsafe { CStr::from_ptr(&layer.layer_name as *const c_char) };
            let name_res = name_c.to_str();
            if name_res.is_err() {
                continue;
            }
            let name = name_res.unwrap();
            match name {
                VALIDATION_LAYER_NAME => {
                    if debug_layers && !supports_khronos_validation {
                        enabled_layers.push(VALIDATION_LAYER_NAME);
                        supports_khronos_validation = true;
                    }
                }
                _ => {}
            }
        }

        let enabled_layers_c: Vec<CString> = enabled_layers
            .iter()
            .map(|layer| CString::new(*layer).unwrap())
            .collect();
        let enabled_layers_c_ptr: Vec<*const c_char> = enabled_layers_c
            .iter()
            .map(|layer_c| layer_c.as_ptr())
            .collect();

        if debug_layers && !supports_khronos_validation {
            println!("Validation layers not installed");
        }

        let supported_extensions = unsafe { entry.enumerate_instance_extension_properties(None) }.unwrap();
        let surface_extensions = [
            SURFACE_WAYLAND_EXT_NAME,
            SURFACE_XCB_EXT_NAME,
            SURFACE_XLIB_EXT_NAME,
            SURFACE_ANDROID_EXT_NAME,
            SURFACE_WIN32_EXT_NAME,
        ];
        let mut supports_surface_extension = false;
        let mut supports_platform_surface_extension = false;
        let mut supports_debug_utils = false;
        let mut enabled_extensions = Vec::<&str>::new();

        'ext_loop: for extension in &supported_extensions {
            let name_c = unsafe { CStr::from_ptr(&extension.extension_name as *const c_char) };
            let name_res = name_c.to_str();
            if name_res.is_err() {
                continue 'ext_loop;
            }
            let name = name_res.unwrap();

            match name {
                DEBUG_UTILS_EXT_NAME => {
                    enabled_extensions.push(DEBUG_UTILS_EXT_NAME);
                    supports_debug_utils = true;
                    continue 'ext_loop;
                }
                SURFACE_EXT_NAME => {
                    enabled_extensions.push(SURFACE_EXT_NAME);
                    supports_surface_extension = true;
                    continue 'ext_loop;
                }
                _ => {}
            }

            if !supports_platform_surface_extension {
                'surface_ext_loop: for surface_extension in surface_extensions {
                    if surface_extension == name {
                        enabled_extensions.push(surface_extension);
                        supports_platform_surface_extension = true;
                        break 'surface_ext_loop;
                    }
                }
            }
        }

        if !supports_surface_extension || !supports_platform_surface_extension {
            panic!("The Vulkan instance doesn't support the surface or swapchain or the required platform surface extension.")
        }

        let enabled_extensions_c: Vec<CString> = enabled_extensions
            .iter()
            .map(|ext| CString::new(*ext).unwrap())
            .collect();
        let enabled_extensions_c_ptrs: Vec<*const c_char> = enabled_extensions_c
            .iter()
            .map(|ext_c| ext_c.as_ptr())
            .collect();


        let app_name = CString::new("Dreieck").unwrap();
        let engine_name = CString::new("Dreieck").unwrap();

        let app_info = vk::ApplicationInfo {
            api_version: vk::make_api_version(0, 1, 3, 280),
            application_version: vk::make_api_version(0, 0, 0, 1),
            engine_version: vk::make_api_version(0, 0, 0, 1),
            p_application_name: app_name.as_ptr(),
            p_engine_name: engine_name.as_ptr(),
            ..Default::default()
        };

        let instance_create_info = vk::InstanceCreateInfo {
            p_application_info: &app_info,
            pp_enabled_layer_names: enabled_layers_c_ptr.as_ptr(),
            enabled_layer_count: enabled_layers_c_ptr.len() as u32,
            pp_enabled_extension_names: enabled_extensions_c_ptrs.as_ptr(),
            enabled_extension_count: enabled_extensions_c_ptrs.len() as u32,
            ..Default::default()
        };

        unsafe {
            let instance = entry.create_instance(&instance_create_info, None).unwrap();

            let debug_utils = if supports_debug_utils {
                let debug_utils_instance = ash::ext::debug_utils::Instance::new(&entry, &instance);
                let debug_messenger = debug_utils_instance
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
                Some(RawInstanceVkDebugUtils {
                    debug_messenger,
                    debug_utils_instance,
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
