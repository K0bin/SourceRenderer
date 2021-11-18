use std::sync::{Arc, MutexGuard};
use std::future::Future;
use std::pin::Pin;
use std::task::{Poll, Context};
use std::ops::Deref;

use ash::vk;

use crate::raw::RawVkDevice;

use sourcerenderer_core::graphics::Fence;
use sourcerenderer_core::pool::{Recyclable};
use std::hash::Hash;
use std::sync::Mutex;
use crossbeam_utils::atomic::AtomicCell;

pub struct VkSemaphoreInner {
  semaphore: vk::Semaphore,
  device: Arc<RawVkDevice>
}

impl VkSemaphoreInner {
  pub fn new(device: &Arc<RawVkDevice>) -> Self {
    let _vk_device = &device.device;
    let info = vk::SemaphoreCreateInfo {
      ..Default::default()
    };
    let semaphore = unsafe {
      device.create_semaphore(&info, None)
    }.unwrap();
    VkSemaphoreInner {
      semaphore,
      device: device.clone()
    }
  }

  pub fn get_handle(&self) -> &vk::Semaphore {
    &self.semaphore
  }
}

impl Drop for VkSemaphoreInner {
  fn drop(&mut self) {
    unsafe {
      self.device.destroy_semaphore(self.semaphore, None);
    }
  }
}

pub struct VkSemaphore {
  inner: Recyclable<VkSemaphoreInner>
}

impl VkSemaphore {
  pub fn new(inner: Recyclable<VkSemaphoreInner>) -> VkSemaphore {
    Self {
      inner
    }
  }
}

impl Deref for VkSemaphore {
  type Target = VkSemaphoreInner;

  fn deref(&self) -> &Self::Target {
    &self.inner
  }
}

#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum VkFenceState {
  Ready,
  Submitted,
  Signalled
}

pub struct VkFenceInner {
  fence: Mutex<vk::Fence>,
  state: AtomicCell<VkFenceState>,
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
      vk_device.reset_fences(&[fence]).unwrap();
    }
    Self {
      device: device.clone(),
      state: AtomicCell::new(VkFenceState::Ready),
      fence: Mutex::new(fence)
    }
  }

  pub fn reset(&self) {
    let vk_device = &self.device.device;
    self.state.store(VkFenceState::Ready);
    let fence_guard = self.fence.lock().unwrap();
    unsafe {
      vk_device.reset_fences(&[*fence_guard]).unwrap();
    }
  }

  pub fn get_handle(&self) -> MutexGuard<vk::Fence> {
    self.fence.lock().unwrap()
  }

  pub fn await_signal(&self) {
    debug_assert_eq!(self.state.load(), VkFenceState::Submitted);
    let vk_device = &self.device.device;
    let fence_guard = self.fence.lock().unwrap();
    unsafe {
      vk_device.wait_for_fences(&[*fence_guard], true, std::u64::MAX).unwrap();
    }
    self.state.store(VkFenceState::Signalled);
  }

  pub fn is_signalled(&self) -> bool {
    self.state() == VkFenceState::Signalled
  }

  pub fn mark_submitted(&self) {
    debug_assert_eq!(self.state.load(), VkFenceState::Ready);
    self.state.store(VkFenceState::Submitted);
  }

  pub fn state(&self) -> VkFenceState {
    let state = self.state.load();
    if state != VkFenceState::Submitted {
      return state;
    }

    let vk_device = &self.device.device;
    let fence_guard = self.fence.lock().unwrap();
    let is_signalled = unsafe {
      vk_device.get_fence_status(*fence_guard).unwrap()
    };
    if is_signalled {
      self.state.store(VkFenceState::Signalled);
      VkFenceState::Signalled
    } else {
      VkFenceState::Submitted
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
    self.inner.is_signalled()
  }

  fn await_signal(&self) {
    self.inner.await_signal();
  }
}

pub struct VkEvent {
  device: Arc<RawVkDevice>,
  event: vk::Event
}

impl VkEvent {
  pub fn new(device: &Arc<RawVkDevice>) -> Self {
    let event = unsafe {
      device.create_event(&vk::EventCreateInfo {
        flags: vk::EventCreateFlags::empty(),
        ..Default::default()
      }, None)
    }.unwrap();
    Self {
      device: device.clone(),
      event,
    }
  }

  pub fn is_signalled(&self) -> bool {
    unsafe {
      self.device.get_event_status(self.event)
    }.unwrap()
  }

  pub fn reset(&self) {
    unsafe {
      self.device.reset_event(self.event).unwrap();
    }
  }

  pub fn handle(&self) -> &vk::Event {
    &self.event
  }
}

impl Drop for VkEvent {
  fn drop(&mut self) {
    unsafe {
      self.device.destroy_event(self.event, None);
    }
  }
}

