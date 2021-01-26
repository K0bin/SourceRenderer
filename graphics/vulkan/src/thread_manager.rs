use std::collections::{VecDeque};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::cell::{RefCell, RefMut};
use std::marker::PhantomData;

use thread_local::ThreadLocal;

use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::{Resettable, CommandBufferType, InnerCommandBufferProvider};
use sourcerenderer_core::pool::{Recyclable};


use crate::raw::RawVkDevice;
use crate::VkCommandPool;
use crate::{VkQueue};
use crate::{VkSemaphore, VkFence};
use crate::buffer::BufferAllocator;
use crate::{VkCommandBufferRecorder, VkLifetimeTrackers, VkShared, VkBackend};

pub struct VkThreadManager {
  device: Arc<RawVkDevice>,
  graphics_queue: Arc<VkQueue>,
  compute_queue: Option<Arc<VkQueue>>,
  transfer_queue: Option<Arc<VkQueue>>,
  threads: ThreadLocal<RefCell<VkThreadLocal>>,
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
  frames: Vec<VkFrameLocal>,
  buffer_allocator: Arc<BufferAllocator>,
  frame_counter: u64,
  disable_send_sync: PhantomData<u32>
}

/*
A frame context manages and resets all resources used to render a frame
*/
pub struct VkFrameLocal {
  device: Arc<RawVkDevice>,
  command_pool: VkCommandPool,
  life_time_trackers: VkLifetimeTrackers,
  buffer_allocator: Arc<BufferAllocator>,
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
    return VkThreadManager {
      device: device.clone(),
      threads: ThreadLocal::new(),
      graphics_queue: graphics_queue.clone(),
      compute_queue: compute_queue.clone(),
      transfer_queue: transfer_queue.clone(),
      shared: shared.clone(),
      max_prepared_frames,
      frame_counter: AtomicU64::new(0),
      prepared_frames: Mutex::new(VecDeque::new())
    };
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

  pub fn get_thread_local(&self) -> RefMut<VkThreadLocal> {
    let mut thread_local = self.threads.get_or(|| RefCell::new(VkThreadLocal::new(&self.device, &self.graphics_queue, &self.compute_queue, &self.transfer_queue, self.max_prepared_frames))).borrow_mut();
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

    return VkThreadLocal {
      device: device.clone(),
      frames,
      frame_counter: 0u64,
      buffer_allocator,
      disable_send_sync: PhantomData
    };
  }

  fn set_frame(&mut self, frame: u64) {
    debug_assert!(frame >= self.frame_counter);
    let length = self.frames.len();
    if frame > self.frame_counter && frame >= self.frames.len() as u64 {
      let mut frame_ref = &mut self.frames[frame as usize % length];
      frame_ref.reset();
    }
    self.frame_counter = frame;
  }

  pub fn get_frame_local(&mut self) -> &mut VkFrameLocal {
    let length = self.frames.len();
    let mut frame_local = &mut self.frames[self.frame_counter as usize % length];
    frame_local.set_frame(self.frame_counter);
    frame_local
  }
}

impl VkFrameLocal {
  pub fn new(device: &Arc<RawVkDevice>, graphics_queue: &Arc<VkQueue>, _compute_queue: &Option<Arc<VkQueue>>, _transfer_queue: &Option<Arc<VkQueue>>, buffer_allocator: &Arc<BufferAllocator>) -> Self {
    Self {
      device: device.clone(),
      command_pool: graphics_queue.create_command_pool(buffer_allocator),
      life_time_trackers: VkLifetimeTrackers::new(),
      buffer_allocator: buffer_allocator.clone(),
      frame: 0
    }
  }

  fn set_frame(&mut self, frame: u64) {
    debug_assert!(frame >= self.frame);
    self.frame = frame;
  }

  pub fn get_command_buffer(&mut self, command_buffer_type: CommandBufferType) -> VkCommandBufferRecorder {
    self.command_pool.get_command_buffer(self.frame, command_buffer_type)
  }

  pub fn track_semaphore(&mut self, semaphore: &Arc<Recyclable<VkSemaphore>>) {
    self.life_time_trackers.track_semaphore(semaphore);
  }

  pub fn track_fence(&mut self, fence: &Arc<VkFence>) {
    self.life_time_trackers.track_fence(fence);
  }

  pub fn reset(&mut self) {
    self.life_time_trackers.reset();
    self.command_pool.reset();
  }
}

impl Drop for VkFrameLocal {
  fn drop(&mut self) {
    unsafe { self.device.device_wait_idle(); }
  }
}

impl InnerCommandBufferProvider<VkBackend> for VkThreadManager {
  fn get_inner_command_buffer(&self) -> VkCommandBufferRecorder {
    let mut thread_context = self.get_thread_local();
    let mut frame_context = thread_context.get_frame_local();
    frame_context.get_command_buffer(CommandBufferType::SECONDARY)
  }
}
