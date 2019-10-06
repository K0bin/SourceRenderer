use std::sync::Arc;

use ash::vk;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::Semaphore;
use sourcerenderer_core::graphics::Fence;

use crate::VkDevice;

pub struct VkSemaphore {
  semaphore: vk::Semaphore,
  device: Arc<VkDevice>
}

impl VkSemaphore {
  pub fn new(device: Arc<VkDevice>) -> Self {
    let vk_device = device.get_ash_device();
    let info = vk::SemaphoreCreateInfo {
      ..Default::default()
    };
    let semaphore = unsafe { vk_device.create_semaphore(&info, None).unwrap() };
    return VkSemaphore {
      device: device,
      semaphore: semaphore
    };
  }

  pub fn get_handle(&self) -> &vk::Semaphore {
    return &self.semaphore;
  }
}

impl Drop for VkSemaphore {
  fn drop(&mut self) {
    unsafe {
      let vk_device = self.device.get_ash_device();
      vk_device.destroy_semaphore(self.semaphore, None);
    }
  }
}

impl Semaphore for VkSemaphore {

}
