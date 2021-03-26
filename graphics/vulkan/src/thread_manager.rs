use std::{borrow::Borrow, collections::{VecDeque}};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::cell::{RefCell, RefMut};
use std::marker::PhantomData;

use thread_local::ThreadLocal;

use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::Resettable;
use sourcerenderer_core::pool::Recyclable;


use crate::{command::VkInnerCommandBufferInfo, raw::RawVkDevice};
use crate::VkCommandPool;
use crate::{VkQueue};
use crate::{VkSemaphore, VkFence};
use crate::buffer::BufferAllocator;
use crate::{VkCommandBufferRecorder, VkLifetimeTrackers, VkShared};

pub struct VkThreadManager {
  device: Arc<RawVkDevice>,
  graphics_queue: Arc<VkQueue>,
  compute_queue: Option<Arc<VkQueue>>,
  transfer_queue: Option<Arc<VkQueue>>,
  threads: ThreadLocal<VkThreadLocal>,
  shared: Arc<VkShared>,
  max_prepared_frames: u32,
  frame_counter: AtomicU64,
  prepared_frames: Mutex<VecDeque<VkFrame>>
}

/*
A thread context manages frame contexts for a thread
*/
pub struct VkThreadLocal {
  device: Arc<RawVkDevice>,
  buffer_allocator: Arc<BufferAllocator>,
  frame_counter: RefCell<u64>,
  frames: Vec<VkFrameLocal>,
  disable_sync: PhantomData<*const u32>
}
unsafe impl Send for VkThreadLocal {}

/*
A frame context manages and resets all resources used to render a frame
*/
pub struct VkFrameLocal {
  device: Arc<RawVkDevice>,
  buffer_allocator: Arc<BufferAllocator>,
  inner: RefCell<VkFrameLocalInner>,
  disable_sync: PhantomData<*const u32>
}
unsafe impl Send for VkFrameLocal {}

struct VkFrameLocalInner {
  command_pool: VkCommandPool,
  life_time_trackers: VkLifetimeTrackers,
  frame: u64
}

pub struct VkFrame {
  counter: u64,
  fence: Arc<VkFence>
}

impl VkThreadManager {
  pub fn new(device: &Arc<RawVkDevice>,
             graphics_queue: &Arc<VkQueue>,
             compute_queue: &Option<Arc<VkQueue>>,
             transfer_queue: &Option<Arc<VkQueue>>,
             shared: &Arc<VkShared>,
             max_prepared_frames: u32) -> Self {
    VkThreadManager {
      device: device.clone(),
      threads: ThreadLocal::new(),
      graphics_queue: graphics_queue.clone(),
      compute_queue: compute_queue.clone(),
      transfer_queue: transfer_queue.clone(),
      shared: shared.clone(),
      max_prepared_frames,
      frame_counter: AtomicU64::new(0),
      prepared_frames: Mutex::new(VecDeque::new())
    }
  }

  pub fn begin_frame(&self) {
    let mut guard = self.prepared_frames.lock().unwrap();
    if guard.len() >= self.max_prepared_frames as usize {
      if let Some(frame) = guard.pop_front() {
        frame.fence.await_signal();
        frame.fence.reset();
      }
    }
  }

  pub fn get_shared(&self) -> &Arc<VkShared> {
    &self.shared
  }

  pub fn get_thread_local(&self) -> &VkThreadLocal {
    let thread_local = self.threads.get_or(|| VkThreadLocal::new(&self.device, &self.graphics_queue, &self.compute_queue, &self.transfer_queue, self.max_prepared_frames));
    thread_local.set_frame(self.frame_counter.load(Ordering::SeqCst));
    thread_local
  }

  pub fn end_frame(&self, fence: &Arc<VkFence>) {
    let counter = self.frame_counter.fetch_add(1, Ordering::SeqCst);
    let mut guard = self.prepared_frames.lock().unwrap();
    guard.push_back(VkFrame {
      counter,
      fence: fence.clone()
    });
  }

  #[inline]
  pub fn get_frame_counter(&self) -> u64 {
    self.frame_counter.load(Ordering::SeqCst)
  }

  #[inline]
  pub fn shared(&self) -> &Arc<VkShared> {
    &self.shared
  }
}

impl VkThreadLocal {
  fn new(device: &Arc<RawVkDevice>,
         graphics_queue: &Arc<VkQueue>,
         compute_queue: &Option<Arc<VkQueue>>,
         transfer_queue: &Option<Arc<VkQueue>>,
         max_prepared_frames: u32) -> Self {
    let buffer_allocator = Arc::new(BufferAllocator::new(device));

    let mut frames: Vec<VkFrameLocal> = Vec::new();
    for _ in 0..max_prepared_frames {
      frames.push(VkFrameLocal::new(device, graphics_queue, compute_queue, transfer_queue, &buffer_allocator))
    }

    VkThreadLocal {
      device: device.clone(),
      frames,
      frame_counter: RefCell::new(0u64),
      buffer_allocator,
      disable_sync: PhantomData
    }
  }

  fn set_frame(&self, frame: u64) {
    let mut frame_counter = self.frame_counter.borrow_mut();
    debug_assert!(frame >= *frame_counter);
    let length = self.frames.len();
    if frame > *frame_counter && frame >= self.frames.len() as u64 {
      let frame_ref = &self.frames[frame as usize % length];
      frame_ref.reset();
    }
    *frame_counter = frame;
  }

  pub fn get_frame_local(&self) -> &VkFrameLocal {
    let frame_counter = self.frame_counter.borrow();
    let length = self.frames.len();
    let frame_local = &self.frames[*frame_counter as usize % length];
    frame_local.set_frame(*frame_counter);
    frame_local
  }
}

impl VkFrameLocal {
  pub fn new(device: &Arc<RawVkDevice>, graphics_queue: &Arc<VkQueue>, _compute_queue: &Option<Arc<VkQueue>>, _transfer_queue: &Option<Arc<VkQueue>>, buffer_allocator: &Arc<BufferAllocator>) -> Self {
    Self {
      device: device.clone(),
      buffer_allocator: buffer_allocator.clone(),
      inner: RefCell::new(VkFrameLocalInner {
        command_pool: graphics_queue.create_command_pool(buffer_allocator),
        life_time_trackers: VkLifetimeTrackers::new(),
        frame: 0
      }),
      disable_sync: PhantomData
    }
  }

  fn set_frame(&self, frame: u64) {
    let mut inner = self.inner.borrow_mut();
    debug_assert!(frame >= inner.frame);
    inner.frame = frame;
  }

  pub fn get_command_buffer(&self) -> VkCommandBufferRecorder {
    let mut inner = self.inner.borrow_mut();
    let frame = inner.frame;
    inner.command_pool.get_command_buffer(frame)
  }

  pub fn get_inner_command_buffer(&self, inner_info: Option<&VkInnerCommandBufferInfo>) -> VkCommandBufferRecorder {
    let mut inner = self.inner.borrow_mut();
    let frame = inner.frame;
    inner.command_pool.get_inner_command_buffer(frame, inner_info)
  }

  pub fn track_semaphore(&self, semaphore: &Arc<Recyclable<VkSemaphore>>) {
    let mut inner = self.inner.borrow_mut();
    inner.life_time_trackers.track_semaphore(semaphore);
  }

  pub fn track_fence(&self, fence: &Arc<VkFence>) {
    let mut inner = self.inner.borrow_mut();
    inner.life_time_trackers.track_fence(fence);
  }

  pub fn reset(&self) {
    let mut inner = self.inner.borrow_mut();
    inner.life_time_trackers.reset();
    inner.command_pool.reset();
  }
}

impl Drop for VkFrameLocal {
  fn drop(&mut self) {
    unsafe { self.device.device_wait_idle().unwrap(); }
  }
}
