use std::ffi::{CStr, CString};
use std::cmp::Ordering;
use std::sync::Arc;

use ash::vk;
use ash::extensions::khr;
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};

use sourcerenderer_core::graphics::Instance;
use sourcerenderer_core::graphics::Adapter;
use crate::VkAdapter;
use crate::VkBackend;
use raw::RawVkInstance;
use std::mem::ManuallyDrop;
use std::os::raw::{c_char, c_void};

//const DEBUG_EXT_NAME =

pub struct VkInstance {
  raw: ManuallyDrop<Arc<RawVkInstance>>,
  debug_utils_loader: ManuallyDrop<ash::extensions::ext::DebugUtils>,
  debug_messenger: vk::DebugUtilsMessengerEXT
}

impl VkInstance {
  pub fn new(instance_extensions: &[&str], debug_layers: bool) -> Self {
    let entry: ash::Entry = ash::Entry::new().unwrap();

    let app_name = CString::new("CS:GO").unwrap();
    let app_name_ptr = app_name.as_ptr();
    let engine_name = CString::new("SourceRenderer").unwrap();
    let engine_name_ptr = engine_name.as_ptr();

    let mut layer_names_c: Vec<CString> = Vec::new();
    if debug_layers {
      layer_names_c.push(CString::new("VK_LAYER_KHRONOS_validation").unwrap());
    }
    let layer_names_ptr: Vec<*const i8> = layer_names_c
      .iter()
      .map(|raw_name| raw_name.as_ptr())
      .collect();

    let mut extension_names_c: Vec<CString> = instance_extensions
      .iter()
      .map(|ext| CString::new(*ext).unwrap())
      .collect();
    extension_names_c.push(CString::from(ash::extensions::ext::DebugUtils::name()));
    let extension_names_ptr: Vec<*const i8> = extension_names_c
      .iter()
      .map(|ext_c| ext_c.as_ptr())
      .collect();

    let app_info = vk::ApplicationInfo {
      api_version: vk_make_version!(1, 1, 126),
      application_version: vk_make_version!(0, 0, 1),
      engine_version: vk_make_version!(0, 0, 1),
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

    return unsafe {
      let instance = entry.create_instance(&instance_create_info, None).unwrap();

      let debug_utils_loader = ash::extensions::ext::DebugUtils::new(&entry, &instance);
      let debug_messenger = debug_utils_loader.create_debug_utils_messenger(&vk::DebugUtilsMessengerCreateInfoEXT {
        flags: vk::DebugUtilsMessengerCreateFlagsEXT::empty(),
        message_severity: vk::DebugUtilsMessageSeverityFlagsEXT::all(),
        message_type: vk::DebugUtilsMessageTypeFlagsEXT::all(),
        pfn_user_callback: Some(VkInstance::debug_callback),
        p_user_data: std::ptr::null_mut(),
        ..Default::default()
      }, None).unwrap();

      VkInstance {
        debug_utils_loader: ManuallyDrop::new(debug_utils_loader),
        debug_messenger,
        raw: ManuallyDrop::new(Arc::new(RawVkInstance {
          entry,
          instance
        }))
      }
    };
  }

  pub fn get_raw(&self) -> &Arc<RawVkInstance> {
    return &self.raw;
  }

  unsafe extern "system" fn debug_callback(
                    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
                    message_types: vk::DebugUtilsMessageTypeFlagsEXT,
                    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
                    p_user_data: *mut c_void,
                    ) -> vk::Bool32 {
    let callback_data_opt = unsafe {
       p_callback_data.as_ref()
    };
    if callback_data_opt.is_none() {
      return vk::FALSE;
    }
    let callback_data = callback_data_opt.unwrap();

    println!("VK DEBUG: {:?}", CStr::from_ptr(callback_data.p_message));
    vk::FALSE
  }
}

impl Drop for VkInstance {
  fn drop(&mut self) {
    unsafe {
      self.debug_utils_loader.destroy_debug_utils_messenger(self.debug_messenger, None);
      ManuallyDrop::drop(&mut self.debug_utils_loader);
      ManuallyDrop::drop(&mut self.raw);
    }
  }
}

impl Instance<VkBackend> for VkInstance {
  fn list_adapters(self: Arc<Self>) -> Vec<Arc<VkAdapter>> {
    let physical_devices: Vec<vk::PhysicalDevice> = unsafe { self.raw.instance.enumerate_physical_devices().unwrap() };
    let instance_ref: &Arc<RawVkInstance> = &self.raw;
    let adapters: Vec<Arc<VkAdapter>> = physical_devices
      .into_iter()
      .map(|phys_dev| Arc::new(VkAdapter::new(instance_ref.clone(), phys_dev)) as Arc<VkAdapter>).collect();

    return adapters;
  }
}
