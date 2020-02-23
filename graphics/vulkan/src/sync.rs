use std::sync::Arc;

use ash::vk;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::Semaphore;
use sourcerenderer_core::graphics::Fence;
use sourcerenderer_core::graphics::Resettable;

use crate::VkDevice;
use crate::raw::RawVkDevice;
use raw::RawVkSemaphore;

pub struct VkSemaphore {
  semaphore: Arc<RawVkSemaphore>
}

impl VkSemaphore {
  pub fn new(device: &Arc<RawVkDevice>) -> Self {
    let vk_device = &device.device;
    let info = vk::SemaphoreCreateInfo {
      ..Default::default()
    };
    return VkSemaphore {
      semaphore: Arc::new(RawVkSemaphore::new(device, &info).unwrap())
    };
  }

  pub fn get_handle(&self) -> &vk::Semaphore {
    return &self.semaphore.semaphore;
  }
}

impl Semaphore for VkSemaphore {

}

impl Resettable for VkSemaphore {
  fn reset(&mut self) {
  }
}

pub struct VkFence {
  fence: vk::Fence,
  device: Arc<RawVkDevice>
}

impl VkFence {
  pub fn new(device: &Arc<RawVkDevice>) -> Self {
    let vk_device = &device.device;
    let info = vk::FenceCreateInfo {
      ..Default::default()
    };
    let fence = unsafe { vk_device.create_fence(&info, None).unwrap() };
    unsafe {
      vk_device.reset_fences(&[fence]);
    }
    return VkFence {
      device: device.clone(),
      fence
    };
  }

  pub fn get_handle(&self) -> &vk::Fence {
    return &self.fence;
  }
}

impl Fence for VkFence {
  fn await(&mut self) {
    let vk_device = &self.device.device;
    unsafe {
      vk_device.wait_for_fences(&[self.fence], true, std::u64::MAX);
    }
  }

  fn is_signaled(&self) -> bool {
    let vk_device = &self.device.device;
    return unsafe {
      vk_device.wait_for_fences(&[self.fence], true, 0).is_ok()
    };
  }
}

impl Resettable for VkFence {
  fn reset(&mut self) {
    let vk_device = &self.device.device;
    unsafe {
      vk_device.reset_fences(&[self.fence]);
    }
  }
}
