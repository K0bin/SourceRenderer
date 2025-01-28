use std::ffi::c_void;
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

bitflags! {
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
  pub struct VkFeatures : u32 {
    const DESCRIPTOR_INDEXING        = 0b1;
    const MEMORY_BUDGET              = 0b10;
    const RAY_TRACING                = 0b1000;
    const ADVANCED_INDIRECT          = 0b10000;
    const MIN_MAX_FILTER             = 0b100000;
    const BARYCENTRICS               = 0b1000000;
    const IMAGE_FORMAT_LIST          = 0b10000000;
    const MAINTENANCE4               = 0b100000000;
    const BDA                        = 0b1000000000;
    const HOST_IMAGE_COPY            = 0b10000000000;
  }
}

pub struct RawVkDevice {
    pub device: ash::Device,
    pub physical_device: vk::PhysicalDevice,
    pub instance: Arc<RawVkInstance>,
    pub debug_utils: Option<ash::ext::debug_utils::Device>,
    pub features: VkFeatures,
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
    pub properties11: vk::PhysicalDeviceVulkan11Properties<'static>,
    pub properties12: vk::PhysicalDeviceVulkan12Properties<'static>,
    pub properties13: vk::PhysicalDeviceVulkan13Properties<'static>,
    pub supported_pipeline_stages: vk::PipelineStageFlags2,
    pub supported_access_flags: vk::AccessFlags2,
    pub host_image_copy: Option<ash::ext::host_image_copy::Device>,
}

unsafe impl Send for RawVkDevice {}
unsafe impl Sync for RawVkDevice {}

pub struct RawVkRTEntries {
    pub acceleration_structure: khr::acceleration_structure::Device,
    pub rt_pipelines: khr::ray_tracing_pipeline::Device,
    pub deferred_operations: khr::deferred_host_operations::Device,
    pub rt_pipeline_properties: vk::PhysicalDeviceRayTracingPipelinePropertiesKHR<'static>,
}

unsafe impl Send for RawVkRTEntries {}
unsafe impl Sync for RawVkRTEntries {}

impl RawVkDevice {
    pub fn new(
        device: ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: Arc<RawVkInstance>,
        features: VkFeatures,
        graphics_queue_info: VkQueueInfo,
        compute_queue_info: Option<VkQueueInfo>,
        transfer_queue_info: Option<VkQueueInfo>,
        graphics_queue: vk::Queue,
        compute_queue: Option<vk::Queue>,
        transfer_queue: Option<vk::Queue>,
    ) -> Self {
        let mut rt_pipeline_properties =
            vk::PhysicalDeviceRayTracingPipelinePropertiesKHR::default();
        let mut properties: vk::PhysicalDeviceProperties2 = Default::default();
        let mut properties11: vk::PhysicalDeviceVulkan11Properties = Default::default();
        let mut properties12: vk::PhysicalDeviceVulkan12Properties = Default::default();
        let mut properties13: vk::PhysicalDeviceVulkan13Properties = Default::default();

        let debug_utils = instance.debug_utils.as_ref().map(|_d| ash::ext::debug_utils::Device::new(&instance.instance, &device));

        properties11.p_next = std::mem::replace(
            &mut properties.p_next,
            &mut properties11
                as *mut vk::PhysicalDeviceVulkan11Properties
                as *mut c_void,
        );

        properties12.p_next = std::mem::replace(
            &mut properties.p_next,
            &mut properties12
                as *mut vk::PhysicalDeviceVulkan12Properties
                as *mut c_void,
        );

        properties13.p_next = std::mem::replace(
            &mut properties.p_next,
            &mut properties13
                as *mut vk::PhysicalDeviceVulkan13Properties
                as *mut c_void,
        );

        if features.contains(VkFeatures::RAY_TRACING) {
            rt_pipeline_properties.p_next = std::mem::replace(
                &mut properties.p_next,
                &mut rt_pipeline_properties
                    as *mut vk::PhysicalDeviceRayTracingPipelinePropertiesKHR
                    as *mut c_void,
            );
        }
        unsafe { instance.get_physical_device_properties2(physical_device, &mut properties) };

        let rt = if features.contains(VkFeatures::RAY_TRACING) {
            Some(RawVkRTEntries {
                acceleration_structure: khr::acceleration_structure::Device::new(&instance, &device),
                rt_pipelines: khr::ray_tracing_pipeline::Device::new(&instance, &device),
                deferred_operations: khr::deferred_host_operations::Device::new(&instance, &device),
                rt_pipeline_properties: unsafe { std::mem::transmute(rt_pipeline_properties) },
            })
        } else {
            None
        };

        let mut d24_props = vk::FormatProperties2::default();
        unsafe {
            instance.get_physical_device_format_properties2(
                physical_device,
                vk::Format::D24_UNORM_S8_UINT,
                &mut d24_props,
            );
        }
        let supports_d24 = d24_props
            .format_properties
            .optimal_tiling_features
            .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT);

        let mut supported_pipeline_stages = vk::PipelineStageFlags2::VERTEX_INPUT
            | vk::PipelineStageFlags2::VERTEX_SHADER
            | vk::PipelineStageFlags2::FRAGMENT_SHADER
            | vk::PipelineStageFlags2::BLIT
            | vk::PipelineStageFlags2::CLEAR
            | vk::PipelineStageFlags2::COPY
            | vk::PipelineStageFlags2::RESOLVE
            | vk::PipelineStageFlags2::HOST
            | vk::PipelineStageFlags2::TRANSFER
            | vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
            | vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
            | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS
            | vk::PipelineStageFlags2::CONDITIONAL_RENDERING_EXT
            | vk::PipelineStageFlags2::DRAW_INDIRECT
            | vk::PipelineStageFlags2::COMPUTE_SHADER;

        let mut supported_access_flags = vk::AccessFlags2::SHADER_READ
            | vk::AccessFlags2::SHADER_WRITE
            | vk::AccessFlags2::INDIRECT_COMMAND_READ
            | vk::AccessFlags2::COLOR_ATTACHMENT_READ
            | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
            | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ
            | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE
            | vk::AccessFlags2::VERTEX_ATTRIBUTE_READ
            | vk::AccessFlags2::TRANSFER_READ
            | vk::AccessFlags2::TRANSFER_WRITE
            | vk::AccessFlags2::SHADER_STORAGE_READ
            | vk::AccessFlags2::SHADER_STORAGE_WRITE
            | vk::AccessFlags2::INDEX_READ
            | vk::AccessFlags2::UNIFORM_READ
            | vk::AccessFlags2::HOST_READ
            | vk::AccessFlags2::HOST_WRITE
            | vk::AccessFlags2::SHADER_SAMPLED_READ;

        if features.contains(VkFeatures::RAY_TRACING) {
            supported_pipeline_stages |= vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR
                | vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR
                | vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_COPY_KHR;

            supported_access_flags |= vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR
                | vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR;
        }

        let host_image_copy =  if features.contains(VkFeatures::HOST_IMAGE_COPY) {
            Some(ash::ext::host_image_copy::Device::new(&instance, &device))
        } else {
            None
        };

        Self {
            device,
            physical_device,
            instance,
            debug_utils,
            features,
            graphics_queue_info,
            compute_queue_info,
            transfer_queue_info,
            graphics_queue: ReentrantMutex::new(graphics_queue),
            compute_queue: compute_queue.map(ReentrantMutex::new),
            transfer_queue: transfer_queue.map(ReentrantMutex::new),
            is_alive: AtomicBool::new(true),
            rt,
            supports_d24,
            properties: properties.properties,
            properties11: unsafe { std::mem::transmute(properties11) },
            properties12: unsafe { std::mem::transmute(properties12) },
            properties13: unsafe { std::mem::transmute(properties13) },
            supported_pipeline_stages,
            supported_access_flags,
            host_image_copy
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
