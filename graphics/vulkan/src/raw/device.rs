use std::ops::Deref;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use ash::khr;
use ash::vk;
use parking_lot::{
    ReentrantMutex,
    ReentrantMutexGuard,
};

use crate::VkQueueInfo;
use crate::raw::RawVkInstance;

pub struct RawVkDevice {
    pub device: ash::Device,
    pub physical_device: vk::PhysicalDevice,
    pub instance: Arc<RawVkInstance>,
    pub debug_utils: Option<ash::ext::debug_utils::Device>,
    pub graphics_queue_info: VkQueueInfo,
    pub compute_queue_info: Option<VkQueueInfo>,
    pub transfer_queue_info: Option<VkQueueInfo>,
    pub is_alive: AtomicBool,
    pub graphics_queue: ReentrantMutex<vk::Queue>,
    pub compute_queue: Option<ReentrantMutex<vk::Queue>>,
    pub transfer_queue: Option<ReentrantMutex<vk::Queue>>,
    pub rt: Option<RawVkRTEntries>,
    pub supports_d24: bool,
    pub properties: vk::PhysicalDeviceProperties,
    pub properties_11: vk::PhysicalDeviceVulkan11Properties<'static>,
    pub properties_12: vk::PhysicalDeviceVulkan12Properties<'static>,
    pub properties_13: vk::PhysicalDeviceVulkan13Properties<'static>,
    pub features: vk::PhysicalDeviceFeatures,
    pub features_11: vk::PhysicalDeviceVulkan11Features<'static>,
    pub features_12: vk::PhysicalDeviceVulkan12Features<'static>,
    pub features_13: vk::PhysicalDeviceVulkan13Features<'static>,
    pub memory_properties: vk::PhysicalDeviceMemoryProperties,
    pub features_barycentrics: vk::PhysicalDeviceFragmentShaderBarycentricFeaturesKHR<'static>,
    pub feature_memory_budget: bool,
    pub supported_shader_stages: vk::ShaderStageFlags,
    pub supported_pipeline_stages: vk::PipelineStageFlags2,
    pub supported_access_flags: vk::AccessFlags2,
    pub host_image_copy: Option<RawVkHostImageCopyEntries>,
    pub mesh_shader: Option<RawVkMeshShaderEntries>,
}

unsafe impl Send for RawVkDevice {}
unsafe impl Sync for RawVkDevice {}

pub struct RawVkRTEntries {
    pub acceleration_structure: khr::acceleration_structure::Device,
    pub rt_pipelines: Option<khr::ray_tracing_pipeline::Device>,
    pub deferred_operations: Option<khr::deferred_host_operations::Device>,
    pub properties_acceleration_structure: vk::PhysicalDeviceAccelerationStructurePropertiesKHR<'static>,
    pub features_acceleration_structure: vk::PhysicalDeviceAccelerationStructureFeaturesKHR<'static>,
    pub properties_rt_pipeline: vk::PhysicalDeviceRayTracingPipelinePropertiesKHR<'static>,
    pub features_rt_pipeline: vk::PhysicalDeviceRayTracingPipelineFeaturesKHR<'static>,
    pub rt_query: bool,
}

unsafe impl Send for RawVkRTEntries {}
unsafe impl Sync for RawVkRTEntries {}

pub struct RawVkHostImageCopyEntries {
    pub host_image_copy: ash::ext::host_image_copy::Device,
    pub properties_host_image_copy: vk::PhysicalDeviceHostImageCopyPropertiesEXT<'static>,
}

pub struct RawVkMeshShaderEntries {
    pub mesh_shader: ash::ext::mesh_shader::Device,
    pub features_mesh_shader: vk::PhysicalDeviceMeshShaderFeaturesEXT<'static>,
    pub properties_mesh_shader: vk::PhysicalDeviceMeshShaderPropertiesEXT<'static>,
}

impl RawVkDevice {
    #[inline(always)]
    pub fn graphics_queue(&self) -> ReentrantMutexGuard<vk::Queue> {
        self.graphics_queue.lock()
    }

    #[inline(always)]
    pub fn compute_queue(&self) -> Option<ReentrantMutexGuard<vk::Queue>> {
        self.compute_queue.as_ref().map(|queue| queue.lock())
    }

    #[inline(always)]
    pub fn transfer_queue(&self) -> Option<ReentrantMutexGuard<vk::Queue>> {
        self.transfer_queue.as_ref().map(|queue| queue.lock())
    }

    pub fn wait_for_idle(&self) {
        let _graphics_queue_lock = self.graphics_queue();
        let _compute_queue_lock = self.compute_queue();
        let _transfer_queue_lock = self.transfer_queue();
        unsafe {
            self.device.device_wait_idle().unwrap();
        }
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
            self.device.destroy_device(None);
        }
    }
}
