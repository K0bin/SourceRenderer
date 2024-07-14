use std::ops::Deref;

use ash::vk;

pub struct RawInstanceVkDebugUtils {
    pub debug_utils_instance: ash::ext::debug_utils::Instance,
    pub debug_messenger: vk::DebugUtilsMessengerEXT,
}

impl Drop for RawInstanceVkDebugUtils {
    fn drop(&mut self) {
        unsafe {
            self.debug_utils_instance
                .destroy_debug_utils_messenger(self.debug_messenger, None);
        }
    }
}

pub struct RawVkInstance {
    pub debug_utils: Option<RawInstanceVkDebugUtils>,
    pub instance: ash::Instance,
    pub entry: ash::Entry,
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
            std::mem::drop(std::mem::replace(&mut self.debug_utils, None));
            self.instance.destroy_instance(None);
        }
    }
}
