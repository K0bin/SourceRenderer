use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;
use std::task::{Poll, Context};
use std::ops::Deref;

use ash::vk;
use ash::version::DeviceV1_0;

use crate::raw::RawVkDevice;

use sourcerenderer_core::graphics::Fence;
use sourcerenderer_core::pool::{Recyclable};

pub struct VkSemaphore {
  semaphore: vk::Semaphore,
  device: Arc<RawVkDevice>
}

impl VkSemaphore {
  pub fn new(device: &Arc<RawVkDevice>) -> Self {
    let _vk_device = &device.device;
    let info = vk::SemaphoreCreateInfo {
      ..Default::default()
    };
    let semaphore = unsafe {
      device.create_semaphore(&info, None)
    }.unwrap();
    return VkSemaphore {
      semaphore,
      device: device.clone()
    };
  }

  pub fn get_handle(&self) -> &vk::Semaphore {
    return &self.semaphore;
  }
}

impl Drop for VkSemaphore {
  fn drop(&mut self) {
    unsafe {
      self.device.destroy_semaphore(self.semaphore, None);
    }
  }
}

pub struct VkFenceInner {
  fence: vk::Fence,
  device: Arc<RawVkDevice>
}

impl VkFenceInner {
  pub fn new(device: &Arc<RawVkDevice>) -> Self {
    let vk_device = &device.device;
    let info = vk::FenceCreateInfo {
      ..Default::default()
    };
    let fence = unsafe { vk_device.create_fence(&info, None).unwrap() };
    unsafe {
      vk_device.reset_fences(&[fence]);
    }
    return Self {
      device: device.clone(),
      fence
    };
  }

  pub fn reset(&self) {
    let vk_device = &self.device.device;
    unsafe {
      vk_device.reset_fences(&[self.fence]);
    }
  }

  pub fn get_handle(&self) -> &vk::Fence {
    return &self.fence;
  }

  pub fn await_signal(&self) {
    let vk_device = &self.device.device;
    unsafe {
      vk_device.wait_for_fences(&[self.fence], true, std::u64::MAX);
    }
  }

  pub fn is_signaled(&self) -> bool {
    let vk_device = &self.device.device;
    unsafe {
      vk_device.get_fence_status(self.fence).unwrap()
    }
  }
}

impl Future for VkFence {
  type Output = ();

  fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
    if self.is_signaled() {
      Poll::Ready(())
    } else {
      Poll::Pending
    }
  }
}

// wrapper type to implement Fence
pub struct VkFence {
  inner: Recyclable<VkFenceInner>
}

impl VkFence {
  pub fn new(inner: Recyclable<VkFenceInner>) -> Self {
    Self {
      inner
    }
  }
}

impl Deref for VkFence {
  type Target = VkFenceInner;

  fn deref(&self) -> &Self::Target {
    &self.inner
  }
}

impl Fence for VkFence {
  fn is_signaled(&self) -> bool {
    self.inner.is_signaled()
  }

  fn await_signal(&self) {
    self.inner.await_signal();
  }
}
