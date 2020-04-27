use std::collections::{HashMap, VecDeque};
use std::thread::ThreadId;
use std::sync::{Arc, Mutex};
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use thread_local::ThreadLocal;

use crossbeam_queue::SegQueue;

use crate::VkDevice;
use crate::raw::RawVkDevice;
use crate::VkCommandPool;
use sourcerenderer_core::graphics::{Device, Resettable};
use std::cell::{RefCell, RefMut};
use ::{VkPipeline, VkQueue};
use ash::version::DeviceV1_0;
use sourcerenderer_core::pool::{Pool, Recyclable};
use ::{VkSemaphore, VkFence};
use ash::prelude::VkResult;
use buffer::BufferAllocator;
use descriptor::VkDescriptorSetLayout;
use pipeline::VkPipelineLayout;
use transfer::VkTransfer;

pub struct VkShared {
  pipelines: RwLock<HashMap<u64, Arc<VkPipeline>>>,
  semaphores: Pool<VkSemaphore>,
  fences: Pool<VkFence>,
  buffers: BufferAllocator, // consider per thread
  descriptor_set_layouts: RwLock<HashMap<u64, Arc<VkDescriptorSetLayout>>>,
  pipeline_layouts: RwLock<HashMap<u64, Arc<VkPipelineLayout>>>
}

pub struct VkGraphicsContext {
  device: Arc<RawVkDevice>,
  graphics_queue: Arc<VkQueue>,
  compute_queue: Option<Arc<VkQueue>>,
  transfer_queue: Option<Arc<VkQueue>>,
  threads: ThreadLocal<RefCell<VkThreadContext>>,
  shared: Arc<VkShared>,
  max_prepared_frames: u32,
  frame_counter: AtomicU64,
  prepared_frames: Mutex<VecDeque<VkFrame>>,
  transfer: VkTransfer
}

/*
A thread context manages frame contexts for a thread
*/
pub struct VkThreadContext {
  device: Arc<RawVkDevice>,
  frames: Vec<RefCell<VkFrameContext>>,
  frame_counter: u64,
  max_prepared_frames: u32
}

/*
A frame context manages and resets all resources used to render a frame
*/
pub struct VkFrameContext {
  device: Arc<RawVkDevice>,
  command_pool: VkCommandPool,
  life_time_trackers: FrameLifeTimeTrackers
}

pub struct VkFrame {
  counter: u64,
  fence: Recyclable<VkFence>
}

impl VkGraphicsContext {
  pub fn new(device: &Arc<RawVkDevice>,
             graphics_queue: &Arc<VkQueue>,
             compute_queue: &Option<Arc<VkQueue>>,
             transfer_queue: &Option<Arc<VkQueue>>,
             shared: &Arc<VkShared>,
             max_prepared_frames: u32) -> Self {
    return VkGraphicsContext {
      device: device.clone(),
      threads: ThreadLocal::new(),
      graphics_queue: graphics_queue.clone(),
      compute_queue: compute_queue.clone(),
      transfer_queue: transfer_queue.clone(),
      shared: shared.clone(),
      max_prepared_frames,
      frame_counter: AtomicU64::new(0),
      prepared_frames: Mutex::new(VecDeque::new()),
      transfer: VkTransfer::new(device, graphics_queue, transfer_queue, shared)
    };
  }

  pub fn get_shared(&self) -> &Arc<VkShared> {
    &self.shared
  }

  pub fn get_thread_context(&self) -> RefMut<VkThreadContext> {
    let mut context = self.threads.get_or(|| RefCell::new(VkThreadContext::new(&self.device, &self.graphics_queue, &self.compute_queue, &self.transfer_queue, self.max_prepared_frames))).borrow_mut();
    context.mark_used(self.frame_counter.load(Ordering::SeqCst));
    context
  }

  pub fn inc_frame_counter(&self, fence: Recyclable<VkFence>) {
    let counter = self.frame_counter.fetch_add(1, Ordering::SeqCst);
    let mut guard = self.prepared_frames.lock().unwrap();
    if guard.len() >= self.max_prepared_frames as usize {
      if let Some(frame) = guard.pop_back() {
        frame.fence.await();
        frame.fence.reset();
      }
    }
    guard.push_back(VkFrame {
      counter,
      fence
    });
  }

  #[inline]
  pub fn get_frame_counter(&self) -> u64 {
    self.frame_counter.load(Ordering::SeqCst)
  }

  #[inline]
  pub(crate) fn get_transfer(&self) -> &VkTransfer {
    &self.transfer
  }
}

impl VkThreadContext {
  fn new(device: &Arc<RawVkDevice>,
         graphics_queue: &Arc<VkQueue>,
         compute_queue: &Option<Arc<VkQueue>>,
         transfer_queue: &Option<Arc<VkQueue>>,
         max_prepared_frames: u32) -> Self {
    let mut frames: Vec<RefCell<VkFrameContext>> = Vec::new();
    for i in 0..max_prepared_frames {
      frames.push(RefCell::new(VkFrameContext::new(device, graphics_queue, compute_queue, transfer_queue)))
    }

    return VkThreadContext {
      device: device.clone(),
      frames,
      frame_counter: 0u64,
      max_prepared_frames
    };
  }

  fn mark_used(&mut self, frame: u64) {
    if frame > self.frame_counter && frame >= self.frames.len() as u64 {
      let mut frame_ref = self.frames[(frame as usize - (self.frames.len() - 1)) % self.frames.len()].borrow_mut();
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
        semaphores: Vec::new(),
        fences: Vec::new()
      }
    }
  }

  pub fn get_command_pool(&mut self) -> &mut VkCommandPool {
    &mut self.command_pool
  }

  pub fn track_semaphore(&mut self, semaphore: Recyclable<VkSemaphore>) {
    self.life_time_trackers.semaphores.push(semaphore);
  }

  pub fn track_fence(&mut self, fence: Recyclable<VkFence>) {
    self.life_time_trackers.fences.push(fence);
  }

  pub fn reset(&mut self) {
    self.life_time_trackers.semaphores.clear();
    for fence in &self.life_time_trackers.fences {
      fence.reset();
    }
    self.life_time_trackers.fences.clear();
    self.command_pool.reset();
  }
}

impl VkShared {
  pub fn new(device: &Arc<RawVkDevice>) -> Self {
    let semaphores_device_clone = device.clone();
    let fences_device_clone = device.clone();
    Self {
      pipelines: RwLock::new(HashMap::new()),
      semaphores: Pool::new(Box::new(move || {
        VkSemaphore::new(&semaphores_device_clone)
      })),
      fences: Pool::new(Box::new(move || {
        VkFence::new(&fences_device_clone)
      })),
      buffers: BufferAllocator::new(device),
      descriptor_set_layouts: RwLock::new(HashMap::new()),
      pipeline_layouts: RwLock::new(HashMap::new())
    }
  }

  #[inline]
  pub(crate) fn get_pipelines(&self) -> &RwLock<HashMap<u64, Arc<VkPipeline>>> {
    &self.pipelines
  }

  #[inline]
  pub(crate) fn get_semaphore(&self) -> Recyclable<VkSemaphore> {
    self.semaphores.get()
  }

  #[inline]
  pub(crate) fn get_fence(&self) -> Recyclable<VkFence> {
    self.fences.get()
  }

  #[inline]
  pub(crate) fn get_descriptor_set_layouts(&self) -> &RwLock<HashMap<u64, Arc<VkDescriptorSetLayout>>> {
    &self.descriptor_set_layouts
  }

  #[inline]
  pub(crate) fn get_pipeline_layouts(&self) -> &RwLock<HashMap<u64, Arc<VkPipelineLayout>>> {
    &self.pipeline_layouts
  }

  #[inline]
  pub(crate) fn get_buffer_allocator(&self) -> &BufferAllocator {
    &self.buffers
  }
}

pub struct FrameLifeTimeTrackers {
  pub semaphores: Vec<Recyclable<VkSemaphore>>,
  pub fences: Vec<Recyclable<VkFence>>
}

impl Drop for VkFrameContext {
  fn drop(&mut self) {
    unsafe { self.device.device_wait_idle(); }
  }
}
