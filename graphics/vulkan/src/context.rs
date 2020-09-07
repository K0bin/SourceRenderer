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
use buffer::{BufferAllocator, VkBufferSlice};
use descriptor::VkDescriptorSetLayout;
use pipeline::VkPipelineLayout;
use transfer::VkTransfer;
use ::{VkTexture, VkRenderPass};
use VkFrameBuffer;
use texture::VkTextureView;
use std::marker::PhantomData;

pub struct VkShared {
  pipelines: RwLock<HashMap<u64, Arc<VkPipeline>>>,
  semaphores: Pool<VkSemaphore>,
  fences: Pool<VkFence>,
  buffers: BufferAllocator, // consider per thread
  descriptor_set_layouts: RwLock<HashMap<u64, Arc<VkDescriptorSetLayout>>>,
  pipeline_layouts: RwLock<HashMap<u64, Arc<VkPipelineLayout>>>
}

pub struct VkThreadContextManager {
  device: Arc<RawVkDevice>,
  graphics_queue: Arc<VkQueue>,
  compute_queue: Option<Arc<VkQueue>>,
  transfer_queue: Option<Arc<VkQueue>>,
  threads: ThreadLocal<RefCell<VkThreadContext>>,
  shared: Arc<VkShared>,
  max_prepared_frames: u32,
  frame_counter: AtomicU64,
  prepared_frames: Mutex<VecDeque<VkFrame>>
}

/*
A thread context manages frame contexts for a thread
*/
pub struct VkThreadContext {
  device: Arc<RawVkDevice>,
  frames: Vec<RefCell<VkFrameContext>>,
  buffer_allocator: Arc<BufferAllocator>,
  frame_counter: u64,
  disable_send_sync: PhantomData<u32>
}

/*
A frame context manages and resets all resources used to render a frame
*/
pub struct VkFrameContext {
  device: Arc<RawVkDevice>,
  command_pool: VkCommandPool,
  life_time_trackers: VkLifetimeTrackers,
  buffer_allocator: Arc<BufferAllocator>
}

pub struct VkFrame {
  counter: u64,
  fence: Arc<Recyclable<VkFence>>
}

impl VkThreadContextManager {
  pub fn new(device: &Arc<RawVkDevice>,
             graphics_queue: &Arc<VkQueue>,
             compute_queue: &Option<Arc<VkQueue>>,
             transfer_queue: &Option<Arc<VkQueue>>,
             shared: &Arc<VkShared>,
             max_prepared_frames: u32) -> Self {
    return VkThreadContextManager {
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
        frame.fence.await();
        frame.fence.reset();
      }
    }
  }

  pub fn get_shared(&self) -> &Arc<VkShared> {
    &self.shared
  }

  pub fn get_thread_context(&self) -> RefMut<VkThreadContext> {
    let mut context = self.threads.get_or(|| RefCell::new(VkThreadContext::new(&self.device, &self.graphics_queue, &self.compute_queue, &self.transfer_queue, self.max_prepared_frames))).borrow_mut();
    context.mark_used(self.frame_counter.load(Ordering::SeqCst));
    context
  }

  pub fn end_frame(&self, fence: &Arc<Recyclable<VkFence>>) {
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
}

impl VkThreadContext {
  fn new(device: &Arc<RawVkDevice>,
         graphics_queue: &Arc<VkQueue>,
         compute_queue: &Option<Arc<VkQueue>>,
         transfer_queue: &Option<Arc<VkQueue>>,
         max_prepared_frames: u32) -> Self {
    let buffer_allocator = Arc::new(BufferAllocator::new(device));

    let mut frames: Vec<RefCell<VkFrameContext>> = Vec::new();
    for _ in 0..max_prepared_frames {
      frames.push(RefCell::new(VkFrameContext::new(device, graphics_queue, compute_queue, transfer_queue, &buffer_allocator)))
    }

    return VkThreadContext {
      device: device.clone(),
      frames,
      frame_counter: 0u64,
      buffer_allocator,
      disable_send_sync: PhantomData
    };
  }

  fn mark_used(&mut self, frame: u64) {
    debug_assert!(frame >= self.frame_counter);
    if frame > self.frame_counter && frame >= self.frames.len() as u64 {
      let mut frame_ref = self.frames[frame as usize % self.frames.len()].borrow_mut();
      frame_ref.reset();
    }
    self.frame_counter = frame;
  }

  pub fn get_frame_context(&self) -> RefMut<VkFrameContext> {
    self.frames[self.frame_counter as usize % self.frames.len()].borrow_mut()
  }
}

impl VkFrameContext {
  pub fn new(device: &Arc<RawVkDevice>, graphics_queue: &Arc<VkQueue>, compute_queue: &Option<Arc<VkQueue>>, transfer_queue: &Option<Arc<VkQueue>>, buffer_allocator: &Arc<BufferAllocator>) -> Self {
    Self {
      device: device.clone(),
      command_pool: graphics_queue.create_command_pool(buffer_allocator),
      life_time_trackers: VkLifetimeTrackers::new(),
      buffer_allocator: buffer_allocator.clone()
    }
  }

  pub fn get_command_pool(&mut self) -> &mut VkCommandPool {
    &mut self.command_pool
  }

  pub fn track_semaphore(&mut self, semaphore: &Arc<Recyclable<VkSemaphore>>) {
    self.life_time_trackers.track_semaphore(semaphore);
  }

  pub fn track_fence(&mut self, fence: &Arc<Recyclable<VkFence>>) {
    self.life_time_trackers.track_fence(fence);
  }

  pub fn reset(&mut self) {
    self.life_time_trackers.reset();
    self.command_pool.reset();
  }
}

impl VkShared {
  pub fn new(device: &Arc<RawVkDevice>) -> Self {
    let semaphores_device_clone = device.clone();
    let fences_device_clone = device.clone();
    Self {
      pipelines: RwLock::new(HashMap::new()),
      semaphores: Pool::new(Box::new(move ||
        VkSemaphore::new(&semaphores_device_clone)
      )),
      fences: Pool::new(Box::new(move ||
        VkFence::new(&fences_device_clone)
      )),
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
  pub(crate) fn get_semaphore(&self) -> Arc<Recyclable<VkSemaphore>> {
    Arc::new(self.semaphores.get())
  }

  #[inline]
  pub(crate) fn get_fence(&self) -> Arc<Recyclable<VkFence>> {
    Arc::new(self.fences.get())
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

pub struct VkLifetimeTrackers {
  semaphores: Vec<Arc<Recyclable<VkSemaphore>>>,
  fences: Vec<Arc<Recyclable<VkFence>>>,
  buffers: Vec<Arc<VkBufferSlice>>,
  textures: Vec<Arc<VkTexture>>,
  texture_views: Vec<Arc<VkTextureView>>,
  render_passes: Vec<Arc<VkRenderPass>>,
  frame_buffers: Vec<Arc<VkFrameBuffer>>
}

impl VkLifetimeTrackers {
  pub(crate) fn new() -> Self {
    Self {
      semaphores: Vec::new(),
      fences: Vec::new(),
      buffers: Vec::new(),
      textures: Vec::new(),
      texture_views: Vec::new(),
      render_passes: Vec::new(),
      frame_buffers: Vec::new()
    }
  }

  pub(crate) fn reset(&mut self) {
    self.semaphores.clear();
    for fence in &self.fences {
      fence.reset();
    }
    self.fences.clear();
    self.buffers.clear();
    self.textures.clear();
    self.texture_views.clear();
    self.render_passes.clear();
    self.frame_buffers.clear();
  }

  pub(crate) fn track_semaphore(&mut self, semaphore: &Arc<Recyclable<VkSemaphore>>) {
    self.semaphores.push(semaphore.clone());
  }

  pub(crate) fn track_fence(&mut self, fence: &Arc<Recyclable<VkFence>>) {
    self.fences.push(fence.clone());
  }

  pub(crate) fn track_buffer(&mut self, buffer: &Arc<VkBufferSlice>) {
    self.buffers.push(buffer.clone());
  }

  pub(crate) fn track_texture(&mut self, texture: &Arc<VkTexture>) {
    self.textures.push(texture.clone());
  }

  pub(crate) fn track_render_pass(&mut self, render_pass: &Arc<VkRenderPass>) {
    self.render_passes.push(render_pass.clone());
  }

  pub(crate) fn track_frame_buffer(&mut self, frame_buffer: &Arc<VkFrameBuffer>) {
    self.frame_buffers.push(frame_buffer.clone());
  }

  pub(crate) fn track_texture_view(&mut self, texture_view: &Arc<VkTextureView>) {
    self.texture_views.push(texture_view.clone());
  }
}

impl Drop for VkFrameContext {
  fn drop(&mut self) {
    unsafe { self.device.device_wait_idle(); }
  }
}
