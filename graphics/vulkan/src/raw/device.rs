use std::ffi::c_void;
use std::sync::Arc;
use std::ops::Deref;
use std::sync::atomic::AtomicBool;
use parking_lot::{ReentrantMutex, ReentrantMutexGuard};

use ash::vk;
use ash::extensions::khr;

use crate::raw::RawVkInstance;
use crate::queue::VkQueueInfo;

bitflags! {
  pub struct VkFeatures : u32 {
    const DESCRIPTOR_INDEXING        = 0b1;
    const DEDICATED_ALLOCATION       = 0b10;
    const DESCRIPTOR_TEMPLATE        = 0b100;
    const RAY_TRACING                = 0b1000;
    const ADVANCED_INDIRECT          = 0b10000;
  }
}

pub struct RawVkDevice {
  pub device: ash::Device,
  pub allocator: vk_mem::Allocator,
  pub physical_device: vk::PhysicalDevice,
  pub instance: Arc<RawVkInstance>,
  pub features: VkFeatures,
  pub graphics_queue_info: VkQueueInfo,
  pub compute_queue_info: Option<VkQueueInfo>,
  pub transfer_queue_info: Option<VkQueueInfo>,
  pub is_alive: AtomicBool,
  pub graphics_queue: ReentrantMutex<vk::Queue>,
  pub compute_queue: Option<ReentrantMutex<vk::Queue>>,
  pub transfer_queue: Option<ReentrantMutex<vk::Queue>>,
  pub rt: Option<RawVkRTEntries>,
  pub indirect_count: Option<ash::extensions::khr::DrawIndirectCount>,
  pub supports_d24: bool,
  pub timeline_semaphores: ash::extensions::khr::TimelineSemaphore,
  pub synchronization2: ash::extensions::khr::Synchronization2,
}

pub struct RawVkRTEntries {
  pub acceleration_structure: khr::AccelerationStructure,
  pub rt_pipelines: khr::RayTracingPipeline,
  pub deferred_operations: khr::DeferredHostOperations,
  pub bda: khr::BufferDeviceAddress,
  pub rt_pipeline_properties: vk::PhysicalDeviceRayTracingPipelinePropertiesKHR
}

unsafe impl Send for RawVkRTEntries {}
unsafe impl Sync for RawVkRTEntries {}

impl RawVkDevice {
  pub fn new(
    device: ash::Device,
    allocator: vk_mem::Allocator,
    physical_device: vk::PhysicalDevice,
    instance: Arc<RawVkInstance>,
    features: VkFeatures,
    graphics_queue_info: VkQueueInfo,
    compute_queue_info: Option<VkQueueInfo>,
    transfer_queue_info: Option<VkQueueInfo>,
    graphics_queue: vk::Queue,
    compute_queue: Option<vk::Queue>,
    transfer_queue: Option<vk::Queue>
  ) -> Self {
    let mut rt_pipeline_properties = vk::PhysicalDeviceRayTracingPipelinePropertiesKHR::default();
    let mut properties: vk::PhysicalDeviceProperties2 = Default::default();
    if features.contains(VkFeatures::RAY_TRACING) {
      rt_pipeline_properties.p_next = std::mem::replace(&mut properties.p_next, &mut rt_pipeline_properties as *mut vk::PhysicalDeviceRayTracingPipelinePropertiesKHR as *mut c_void);
    }
    unsafe { instance.get_physical_device_properties2(physical_device, &mut properties) };

    let rt = if features.contains(VkFeatures::RAY_TRACING) {
      Some(RawVkRTEntries {
        acceleration_structure: khr::AccelerationStructure::new(&instance, &device),
        rt_pipelines: khr::RayTracingPipeline::new(&instance, &device),
        deferred_operations: khr::DeferredHostOperations::new(&instance, &device),
        bda: khr::BufferDeviceAddress::new(&instance, &device),
        rt_pipeline_properties
      })
    } else {
      None
    };

    let mut d24_props = vk::FormatProperties2::default();
    unsafe {
      instance.get_physical_device_format_properties2(physical_device, vk::Format::D24_UNORM_S8_UINT, &mut d24_props);
    }
    let supports_d24 = d24_props.format_properties.optimal_tiling_features.contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT);

    let indirect_count = features.contains(VkFeatures::ADVANCED_INDIRECT).then(|| {
      ash::extensions::khr::DrawIndirectCount::new(&instance, &device)
    });

    let timeline_semaphores = ash::extensions::khr::TimelineSemaphore::new(&instance, &device);
    let synchronization2 = ash::extensions::khr::Synchronization2::new(&instance, &device);

    Self {
      device,
      allocator,
      physical_device,
      instance,
      features,
      graphics_queue_info,
      compute_queue_info,
      transfer_queue_info,
      graphics_queue: ReentrantMutex::new(graphics_queue),
      compute_queue: compute_queue.map(|queue| ReentrantMutex::new(queue)),
      transfer_queue: transfer_queue.map(|queue| ReentrantMutex::new(queue)),
      is_alive: AtomicBool::new(true),
      rt,
      indirect_count,
      supports_d24: supports_d24,
      timeline_semaphores,
      synchronization2,
    }
  }

  pub fn graphics_queue(&self) -> ReentrantMutexGuard<vk::Queue> {
    self.graphics_queue.lock()
  }

  pub fn compute_queue(&self) -> Option<ReentrantMutexGuard<vk::Queue>> {
    self.compute_queue.as_ref().map(|queue| queue.lock())
  }

  pub fn transfer_queue(&self) -> Option<ReentrantMutexGuard<vk::Queue>> {
    self.transfer_queue.as_ref().map(|queue| queue.lock())
  }

  pub fn wait_for_idle(&self) {
    let _graphics_queue_lock = self.graphics_queue();
    let _compute_queue_lock = self.compute_queue();
    let _transfer_queue_lock = self.transfer_queue();
    unsafe { self.device.device_wait_idle().unwrap(); }
  }
}

impl Deref for RawVkDevice {
  type Target = ash::Device;

  fn deref(&self) -> &Self::Target {
    &self.device
  }
}

impl Drop for RawVkDevice {
  fn drop(&mut self) {
    unsafe {
      self.allocator.destroy();
      self.device.destroy_device(None);
    }
  }
}
