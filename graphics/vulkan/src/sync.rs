use std::sync::{Arc, MutexGuard};
use std::future::Future;
use std::pin::Pin;
use std::task::{Poll, Context};
use std::ops::Deref;

use ash::vk;
use ash::version::DeviceV1_0;

use crate::raw::RawVkDevice;

use sourcerenderer_core::graphics::Fence;
use sourcerenderer_core::pool::{Recyclable};
use std::hash::{Hash, Hasher};
use ash::vk::Handle;
use crate::ash::version::InstanceV1_0;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

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
  fence: Mutex<vk::Fence>,
  is_signalled: AtomicBool,
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
      is_signalled: AtomicBool::new(false),
      fence: Mutex::new(fence)
    };
  }

  pub fn reset(&self) {
    let vk_device = &self.device.device;
    self.is_signalled.store(false, Ordering::SeqCst);
    let fence_guard = self.fence.lock().unwrap();
    unsafe {
      vk_device.reset_fences(&[*fence_guard]);
    }
  }

  pub fn get_handle(&self) -> MutexGuard<vk::Fence> {
    return self.fence.lock().unwrap();
  }

  pub fn await_signal(&self) {
    let vk_device = &self.device.device;
    let fence_guard = self.fence.lock().unwrap();
    unsafe {
      vk_device.wait_for_fences(&[*fence_guard], true, std::u64::MAX);
    }
  }

  pub fn is_signalled(&self) -> bool {
    if self.is_signalled.load(Ordering::SeqCst) {
      return true;
    }

    let vk_device = &self.device.device;
    let fence_guard = self.fence.lock().unwrap();
    let is_signalled = unsafe {
      vk_device.get_fence_status(*fence_guard).unwrap()
    };
    if is_signalled {
      self.is_signalled.store(true, Ordering::SeqCst);
    }
    is_signalled
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
    self.inner.is_signalled()
  }

  fn await_signal(&self) {
    self.inner.await_signal();
  }
}
