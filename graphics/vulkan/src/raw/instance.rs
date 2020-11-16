use std::ops::Deref;

use ash::version::{InstanceV1_0};
use ash::vk;

pub struct RawVkInstance {
  pub entry: ash::Entry,
  pub instance: ash::Instance,
  pub debug_utils_loader: ash::extensions::ext::DebugUtils,
  pub debug_messenger: vk::DebugUtilsMessengerEXT
}

impl Deref for RawVkInstance {
  type Target = ash::Instance;

  fn deref(&self) -> &Self::Target {
    &self.instance
  }
}

impl Drop for RawVkInstance {
  fn drop(&mut self) {
    unsafe {
      self.debug_utils_loader.destroy_debug_utils_messenger(self.debug_messenger, None);
      self.instance.destroy_instance(None);
    }
  }
}
