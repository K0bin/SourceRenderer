use std::ffi::{CStr, CString};
use std::cmp::Ordering;
use std::sync::Arc;

use ash::vk;
use ash::extensions::khr;
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};

use sourcerenderer_core::graphics::Instance;
use sourcerenderer_core::graphics::Adapter;
use crate::VkAdapter;

pub struct VkInstance {
  instance: ash::Instance,
  entry: ash::Entry
}

impl VkInstance {
  pub fn new(instance_extensions: Vec<&str>, debug_layers: bool) -> Self {
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

    let extension_names_c: Vec<CString> = instance_extensions
      .iter()
      .map(|ext| CString::new(*ext).unwrap())
      .collect();
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

      VkInstance {
        instance: instance,
        entry: entry
      }
    };
  }

  #[inline]
  pub fn get_ash_instance(&self) -> &ash::Instance {
    return &self.instance;
  }

  #[inline]
  pub fn get_entry(&self) -> &ash::Entry {
    return &self.entry;
  }
}

impl Drop for VkInstance {
  fn drop(&mut self) {
    unsafe {
      self.instance.destroy_instance(Option::None);
    }
  }
}

impl Instance for VkInstance {
  fn list_adapters(self: Arc<Self>) -> Vec<Arc<dyn Adapter>> {
    let physical_devices: Vec<vk::PhysicalDevice> = unsafe { self.instance.enumerate_physical_devices().unwrap() };
    let adapters: Vec<Arc<dyn Adapter>> = physical_devices
      .into_iter()
      .map(|phys_dev| Arc::new(VkAdapter::new(self.clone(), phys_dev)) as Arc<dyn Adapter>)
      .collect();

    return adapters;
  }
}
