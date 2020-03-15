use std::collections::HashMap;
use std::thread::ThreadId;
use std::sync::{Arc, Mutex};
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use thread_local::ThreadLocal;

use crate::VkDevice;
use crate::raw::RawVkDevice;
use crate::VkCommandPool;
use sourcerenderer_core::graphics::{Device, Queue, Resettable};
use std::cell::{RefCell, RefMut};
use ::{VkPipeline, VkQueue};
use ash::version::DeviceV1_0;
use sourcerenderer_core::pool::{Pool, Recyclable};
use VkSemaphore;

pub struct VkSharedCaches {
  pub pipelines: RwLock<HashMap<u64, VkPipeline>>,
  pub semaphores: Pool<VkSemaphore>
}

pub struct VkGraphicsContext {
  device: Arc<RawVkDevice>,
  graphics_queue: Arc<VkQueue>,
  compute_queue: Option<Arc<VkQueue>>,
  transfer_queue: Option<Arc<VkQueue>>,
  threads: ThreadLocal<RefCell<VkThreadContext>>,
  caches: Arc<VkSharedCaches>,
  frame_counter: AtomicU64
}

/*
A thread context manages frame contexts for a thread
*/
pub struct VkThreadContext {
  device: Arc<RawVkDevice>,
  frames: Vec<RefCell<VkFrameContext>>,
  frame_counter: u64
}

/*
A frame context manages and resets all resources used to render a frame
*/
pub struct VkFrameContext {
  device: Arc<RawVkDevice>,
  command_pool: VkCommandPool,
  life_time_trackers: FrameLifeTimeTrackers
}

impl VkGraphicsContext {
  pub fn new(device: &Arc<RawVkDevice>,
             graphics_queue: &Arc<VkQueue>,
             compute_queue: &Option<Arc<VkQueue>>,
             transfer_queue: &Option<Arc<VkQueue>>,
             caches: &Arc<VkSharedCaches>) -> Self {
    return VkGraphicsContext {
      device: device.clone(),
      threads: ThreadLocal::new(),
      graphics_queue: graphics_queue.clone(),
      compute_queue: compute_queue.clone(),
      transfer_queue: transfer_queue.clone(),
      caches: caches.clone(),
      frame_counter: AtomicU64::new(0)
    };
  }

  pub fn get_caches(&self) -> &Arc<VkSharedCaches> {
    &self.caches
  }

  pub fn get_thread_context(&self) -> RefMut<VkThreadContext> {
    let mut context = self.threads.get_or(|| RefCell::new(VkThreadContext::new(&self.device, &self.graphics_queue, &self.compute_queue, &self.transfer_queue))).borrow_mut();
    context.mark_used(self.frame_counter.load(Ordering::SeqCst));
    context
  }

  pub fn inc_frame_counter(&self) {
    self.frame_counter.fetch_add(1, Ordering::SeqCst);
  }
}

impl VkThreadContext {
  fn new(device: &Arc<RawVkDevice>, graphics_queue: &Arc<VkQueue>, compute_queue: &Option<Arc<VkQueue>>, transfer_queue: &Option<Arc<VkQueue>>) -> Self {
    let mut frames: Vec<RefCell<VkFrameContext>> = Vec::new();
    for i in 0..2 {
      frames.push(RefCell::new(VkFrameContext::new(device, graphics_queue, compute_queue, transfer_queue)))
    }

    return VkThreadContext {
      device: device.clone(),
      frames,
      frame_counter: 0u64
    };
  }

  fn mark_used(&mut self, frame: u64) {
    if frame > self.frame_counter {
      let mut frame_ref = self.frames[(frame as usize - 1) % self.frames.len()].borrow_mut();
      frame_ref.reset();
      self.frame_counter = frame;
    }
  }

  pub fn get_frame_context(&self) -> RefMut<VkFrameContext> {
    self.frames[self.frame_counter as usize % self.frames.len()].borrow_mut()
  }
}

impl VkFrameContext {
  pub fn new(device: &Arc<RawVkDevice>, graphics_queue: &Arc<VkQueue>, compute_queue: &Option<Arc<VkQueue>>, transfer_queue: &Option<Arc<VkQueue>>) -> Self {
    Self {
      device: device.clone(),
      command_pool: graphics_queue.create_command_pool(),
      life_time_trackers: FrameLifeTimeTrackers {
        semaphores: Vec::new()
      }
    }
  }

  pub fn get_command_pool(&mut self) -> &mut VkCommandPool {
    &mut self.command_pool
  }

  pub fn track_semaphore(&mut self, semaphore: Recyclable<VkSemaphore>) {
    self.life_time_trackers.semaphores.push(semaphore);
  }

  pub fn reset(&mut self) {
    self.life_time_trackers.semaphores.clear();
    self.command_pool.reset();
  }
}

impl VkSharedCaches {
  pub fn new(device: &Arc<RawVkDevice>) -> Self {
    let device_clone = device.clone();
    Self {
      pipelines: RwLock::new(HashMap::new()),
      semaphores: Pool::new(16, Box::new(move || {
        VkSemaphore::new(&device_clone)
      }))
    }
  }

  pub fn get_pipelines(&self) -> &RwLock<HashMap<u64, VkPipeline>> {
    &self.pipelines
  }

  pub fn get_semaphore(&self) -> Recyclable<VkSemaphore> {
    self.semaphores.get()
  }
}

pub struct FrameLifeTimeTrackers {
  pub semaphores: Vec<Recyclable<VkSemaphore>>
}
