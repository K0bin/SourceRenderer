use std::{sync::{
    atomic::AtomicU64,
    Arc,
}, ffi::c_void};

use ash::vk;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{self, Device as _};

use super::*;

pub struct VkDevice {
    device: Arc<RawVkDevice>,
    graphics_queue: VkQueue,
    compute_queue: Option<VkQueue>,
    transfer_queue: Option<VkQueue>,
    shared: Arc<VkShared>,
    query_count: AtomicU64,
    memory_type_infos: Vec<gpu::MemoryTypeInfo>,
}

impl VkDevice {
    pub unsafe fn new(
        device: ash::Device,
        instance: &Arc<RawVkInstance>,
        physical_device: vk::PhysicalDevice,
        graphics_queue_info: VkQueueInfo,
        compute_queue_info: Option<VkQueueInfo>,
        transfer_queue_info: Option<VkQueueInfo>,
        features: VkFeatures
    ) -> Self {
        let raw_graphics_queue = unsafe {
            device.get_device_queue(
                graphics_queue_info.queue_family_index as u32,
                graphics_queue_info.queue_index as u32,
            )
        };
        let raw_compute_queue = compute_queue_info.map(|info| unsafe {
            device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32)
        });
        let raw_transfer_queue = transfer_queue_info.map(|info| unsafe {
            device.get_device_queue(info.queue_family_index as u32, info.queue_index as u32)
        });

        let raw = Arc::new(RawVkDevice::new(
            device,
            physical_device,
            instance.clone(),
            features,
            graphics_queue_info,
            compute_queue_info,
            transfer_queue_info,
            raw_graphics_queue,
            raw_compute_queue,
            raw_transfer_queue,
        ));

        let shared = Arc::new(VkShared::new(&raw));

        let graphics_queue =
            { VkQueue::new(graphics_queue_info, VkQueueType::Graphics, &raw, &shared) };

        let compute_queue =
            compute_queue_info.map(|info| VkQueue::new(info, VkQueueType::Compute, &raw, &shared));

        let transfer_queue = transfer_queue_info
            .map(|info| VkQueue::new(info, VkQueueType::Transfer, &raw, &shared));

        // Memory types
        let mut memory_properties = vk::PhysicalDeviceMemoryProperties2::default();
        instance.get_physical_device_memory_properties2(physical_device, &mut memory_properties);

        let memory_type_count = memory_properties.memory_properties.memory_type_count as usize;
        let memory_types = &memory_properties.memory_properties.memory_types[.. memory_type_count];

        let mut memory_type_infos = Vec::<gpu::MemoryTypeInfo>::new();
        for memory_type in memory_types {
            let kind: gpu::MemoryKind = if memory_type.property_flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL) {
                gpu::MemoryKind::VRAM
            } else {
                gpu::MemoryKind::RAM
            };
            let is_cpu_accessible = memory_type.property_flags.contains(vk::MemoryPropertyFlags::HOST_VISIBLE);
            let is_cached = memory_type.property_flags.contains(vk::MemoryPropertyFlags::HOST_CACHED);
            let is_coherent = memory_type.property_flags.contains(vk::MemoryPropertyFlags::HOST_COHERENT);

            let info = gpu::MemoryTypeInfo {
                memory_index: memory_type.heap_index,
                memory_kind: kind, is_cached, is_cpu_accessible,
                is_coherent
            };
            memory_type_infos.push(info);
        }

        VkDevice {
            device: raw,
            graphics_queue,
            compute_queue,
            transfer_queue,
            shared,
            query_count: AtomicU64::new(0),
            memory_type_infos,
        }
    }

    #[inline]
    pub fn inner(&self) -> &Arc<RawVkDevice> {
        &self.device
    }

    #[inline]
    pub fn graphics_queue(&self) -> &VkQueue {
        &self.graphics_queue
    }

    #[inline]
    pub fn compute_queue(&self) -> Option<&VkQueue> {
        self.compute_queue.as_ref()
    }

    #[inline]
    pub fn transfer_queue(&self) -> Option<&VkQueue> {
        self.transfer_queue.as_ref()
    }
}

impl gpu::Device<VkBackend> for VkDevice {
    unsafe fn create_buffer(
        &self,
        info: &gpu::BufferInfo,
        memory_type_index: u32,
        name: Option<&str>,
    ) -> Result<VkBuffer, gpu::OutOfMemoryError> {
        VkBuffer::new(&self.device, ResourceMemory::Dedicated { memory_type_index }, info, name)
    }

    unsafe fn create_shader(
        &self,
        shader: gpu::PackedShader,
        name: Option<&str>,
    ) -> VkShader {
        VkShader::new(&self.device, shader, name)
    }

    unsafe fn create_texture(&self, info: &gpu::TextureInfo, memory_type_index: u32, name: Option<&str>) -> Result<VkTexture, gpu::OutOfMemoryError> {
        VkTexture::new(&self.device, info, ResourceMemory::Dedicated { memory_type_index }, name)
    }

    unsafe fn create_texture_view(
        &self,
        texture: &VkTexture,
        info: &gpu::TextureViewInfo,
        name: Option<&str>,
    ) -> VkTextureView {
        VkTextureView::new(&self.device, texture, info, name)
    }

    unsafe fn create_sampler(&self, info: &gpu::SamplerInfo) -> VkSampler {
        VkSampler::new(&self.device, info)
    }

    unsafe fn create_compute_pipeline(&self, shader: &VkShader, name: Option<&str>) -> VkPipeline {
        VkPipeline::new_compute(&self.device, shader, self.shared.as_ref(), name)
    }

    unsafe fn wait_for_idle(&self) {
        self.device.wait_for_idle();
    }

    unsafe fn create_graphics_pipeline(
        &self,
        info: &gpu::GraphicsPipelineInfo<VkBackend>,
        name: Option<&str>,
    ) -> VkPipeline {
        let shared = &self.shared;
        VkPipeline::new_graphics(&self.device, info, shared, name)
    }

    unsafe fn create_fence(&self, _is_cpu_accessible: bool) -> VkTimelineSemaphore {
        VkTimelineSemaphore::new(&self.device)
    }

    fn graphics_queue(&self) -> &VkQueue {
        &self.graphics_queue
    }

    fn compute_queue(&self) -> Option<&VkQueue> {
        self.compute_queue.as_ref()
    }

    fn transfer_queue(&self) -> Option<&VkQueue> {
        self.transfer_queue.as_ref()
    }

    fn supports_bindless(&self) -> bool {
        self.device
        .features
        .contains(VkFeatures::DESCRIPTOR_INDEXING)
    }

    fn supports_ray_tracing(&self) -> bool {
        self.device.features.contains(VkFeatures::RAY_TRACING)
    }

    fn supports_indirect(&self) -> bool {
        self.device.features.contains(VkFeatures::ADVANCED_INDIRECT)
    }

    fn supports_min_max_filter(&self) -> bool {
        self.device.features.contains(VkFeatures::MIN_MAX_FILTER)
    }

    unsafe fn insert_texture_into_bindless_heap(&self, slot: u32, texture: &VkTextureView) {
        if let Some(bindless_set) = self.shared.bindless_texture_descriptor_set() {
            bindless_set.write_texture_descriptor(slot, texture);
        }
    }

    fn supports_barycentrics(&self) -> bool {
        self.device.features.contains(VkFeatures::BARYCENTRICS)
    }

    unsafe fn memory_infos(&self) -> Vec<gpu::MemoryInfo> {
        let mut memory_infos = Vec::<gpu::MemoryInfo>::new();

        let supports_ext_budget = self.device.features.contains(VkFeatures::MEMORY_BUDGET);

        let mut memory_properties = vk::PhysicalDeviceMemoryProperties2::default();
        let mut budget = vk::PhysicalDeviceMemoryBudgetPropertiesEXT::default();
        if supports_ext_budget {
            memory_properties.p_next = &mut budget as *mut vk::PhysicalDeviceMemoryBudgetPropertiesEXT as *mut c_void;
        }
        self.device.instance.get_physical_device_memory_properties2(self.device.physical_device, &mut memory_properties);

        let heap_count = memory_properties.memory_properties.memory_heap_count as usize;
        let heaps = &memory_properties.memory_properties.memory_heaps[.. heap_count];

        for i in 0..heap_count {
            let heap = &heaps[i];
            let heap_budget = budget.heap_budget[i];
            let heap_usage = budget.heap_usage[i];

            let kind: gpu::MemoryKind = if heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL) {
                gpu::MemoryKind::VRAM
            } else {
                gpu::MemoryKind::RAM
            };

            let info = gpu::MemoryInfo {
                available: if supports_ext_budget { heap_budget - heap_usage } else { heap.size },
                total: if supports_ext_budget { heap_budget } else { heap.size },
                memory_kind: kind
            };
            memory_infos.push(info);
        }
        memory_infos
    }

    unsafe fn memory_type_infos(&self) -> &[gpu::MemoryTypeInfo] {
        &self.memory_type_infos
    }

    unsafe fn create_heap(&self, memory_type_index: u32, size: u64) -> Result<VkMemoryHeap, gpu::OutOfMemoryError> {
        VkMemoryHeap::new(&self.device, memory_type_index, size)
    }

    unsafe fn get_buffer_heap_info(&self, info: &gpu::BufferInfo) -> gpu::ResourceHeapInfo {
        let mut queue_families = SmallVec::<[u32; 3]>::new();
        let mut sharing_mode = vk::SharingMode::EXCLUSIVE;
        if info.sharing_mode == gpu::QueueSharingMode::Concurrent && (self.device.transfer_queue_info.is_some() || self.device.compute_queue_info.is_some()) {
            sharing_mode = vk::SharingMode::CONCURRENT;
            queue_families.push(self.device.graphics_queue_info.queue_family_index as u32);
            if let Some(info) = self.device.transfer_queue_info.as_ref() {
                queue_families.push(info.queue_family_index as u32);
            }
            if let Some(info) = self.device.compute_queue_info.as_ref() {
                queue_families.push(info.queue_family_index as u32);
            }
        }

        let buffer_info = vk::BufferCreateInfo {
            size: info.size as u64,
            usage: buffer_usage_to_vk(
                info.usage,
                self.device.features.contains(VkFeatures::RAY_TRACING),
            ),
            sharing_mode,
            p_queue_family_indices: queue_families.as_ptr(),
            queue_family_index_count: queue_families.len() as u32,
            ..Default::default()
        };

        let mut requirements = vk::MemoryRequirements2::default();
        let mut dedicated_requirements = vk::MemoryDedicatedRequirements::default();
        requirements.p_next = &mut dedicated_requirements as *mut vk::MemoryDedicatedRequirements as *mut c_void;
        let buffer_requirements_info = vk::DeviceBufferMemoryRequirements {
            p_create_info: &buffer_info as *const vk::BufferCreateInfo,
            ..Default::default()
        };
        self.device.get_device_buffer_memory_requirements(&buffer_requirements_info, &mut requirements);

        gpu::ResourceHeapInfo {
            dedicated_allocation_preference: if dedicated_requirements.requires_dedicated_allocation == vk::TRUE {
                gpu::DedicatedAllocationPreference::RequireDedicated
            } else if dedicated_requirements.prefers_dedicated_allocation == vk::TRUE {
                gpu::DedicatedAllocationPreference::PreferDedicated
            } else {
                gpu::DedicatedAllocationPreference::DontCare
            },
            memory_type_mask: requirements.memory_requirements.memory_type_bits,
            alignment: requirements.memory_requirements.alignment,
            size: requirements.memory_requirements.size
        }
    }

    unsafe fn get_texture_heap_info(&self, info: &gpu::TextureInfo) -> gpu::ResourceHeapInfo {
        let mut image_info = vk::ImageCreateInfo {
            flags: vk::ImageCreateFlags::empty(),
            tiling: vk::ImageTiling::OPTIMAL,
            initial_layout: vk::ImageLayout::UNDEFINED,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            usage: texture_usage_to_vk(info.usage),
            image_type: match info.dimension {
                gpu::TextureDimension::Dim1DArray | gpu::TextureDimension::Dim1D => vk::ImageType::TYPE_1D,
                gpu::TextureDimension::Dim2DArray | gpu::TextureDimension::Dim2D
                    | gpu::TextureDimension::Cube | gpu::TextureDimension::CubeArray => vk::ImageType::TYPE_2D,
                gpu::TextureDimension::Dim3D => vk::ImageType::TYPE_3D,
            },
            extent: vk::Extent3D {
                width: info.width.max(1),
                height: info.height.max(1),
                depth: info.depth.max(1),
            },
            format: format_to_vk(info.format, self.device.supports_d24),
            mip_levels: info.mip_levels,
            array_layers: info.array_length,
            samples: samples_to_vk(info.samples),
            ..Default::default()
        };

        debug_assert!(
            info.array_length == 1
                || (info.dimension == gpu::TextureDimension::Dim1DArray
                    || info.dimension == gpu::TextureDimension::Dim2DArray)
        );
        debug_assert!(info.depth == 1 || info.dimension == gpu::TextureDimension::Dim3D);
        debug_assert!(
            info.height == 1
                || (info.dimension == gpu::TextureDimension::Dim2D
                    || info.dimension == gpu::TextureDimension::Dim2DArray
                    || info.dimension == gpu::TextureDimension::Dim3D)
        );

        let mut compatible_formats = SmallVec::<[vk::Format; 2]>::with_capacity(2);
        compatible_formats.push(image_info.format);
        let mut format_list = vk::ImageFormatListCreateInfo {
            view_format_count: compatible_formats.len() as u32,
            p_view_formats: compatible_formats.as_ptr(),
            ..Default::default()
        };
        if info.supports_srgb {
            image_info.flags |= vk::ImageCreateFlags::MUTABLE_FORMAT;
            format_list.p_next = std::mem::replace(
                &mut image_info.p_next,
                &format_list as *const vk::ImageFormatListCreateInfo as *const c_void,
            );
        }

        let mut requirements = vk::MemoryRequirements2::default();
        let mut dedicated_requirements = vk::MemoryDedicatedRequirements::default();
        requirements.p_next = &mut dedicated_requirements as *mut vk::MemoryDedicatedRequirements as *mut c_void;
        let image_requirements_info = vk::DeviceImageMemoryRequirements {
            p_create_info: &image_info as *const vk::ImageCreateInfo,
            ..Default::default()
        };
        self.device.get_device_image_memory_requirements(&image_requirements_info, &mut requirements);

        gpu::ResourceHeapInfo {
            dedicated_allocation_preference: if dedicated_requirements.requires_dedicated_allocation == vk::TRUE {
                gpu::DedicatedAllocationPreference::RequireDedicated
            } else if dedicated_requirements.prefers_dedicated_allocation == vk::TRUE {
                gpu::DedicatedAllocationPreference::PreferDedicated
            } else {
                gpu::DedicatedAllocationPreference::DontCare
            },
            memory_type_mask: requirements.memory_requirements.memory_type_bits,
            alignment: requirements.memory_requirements.alignment.max(self.device.properties.limits.buffer_image_granularity),
            size: requirements.memory_requirements.size
        }
    }

    unsafe fn get_bottom_level_acceleration_structure_size(&self, info: &gpu::BottomLevelAccelerationStructureInfo<VkBackend>) -> gpu::AccelerationStructureSizes {
        VkAccelerationStructure::bottom_level_size(&self.device, info)
    }

    unsafe fn get_top_level_acceleration_structure_size(&self, info: &gpu::TopLevelAccelerationStructureInfo<VkBackend>) -> gpu::AccelerationStructureSizes {
        VkAccelerationStructure::top_level_size(&self.device, info)
    }

    unsafe fn get_raytracing_pipeline_sbt_buffer_size(&self, info: &gpu::RayTracingPipelineInfo<VkBackend>) -> u64 {
        VkPipeline::ray_tracing_buffer_size(&self.device, info, &self.shared)
    }

    unsafe fn create_raytracing_pipeline(&self, info: &gpu::RayTracingPipelineInfo<VkBackend>, sbt_buffer: &VkBuffer, sbt_buffer_offset: u64, name: Option<&str>) -> VkPipeline {
        VkPipeline::new_ray_tracing(&self.device, info, &self.shared, sbt_buffer, sbt_buffer_offset, name)
    }

    fn get_top_level_instances_buffer_size(&self, instances: &[gpu::AccelerationStructureInstance<VkBackend>]) -> u64 {
        (std::mem::size_of::<vk::AccelerationStructureInstanceKHR>() * instances.len()) as u64
    }
}

impl Drop for VkDevice {
    fn drop(&mut self) {
        unsafe {
            self.wait_for_idle();
        }
    }
}

#[derive(Debug)]
pub(crate) struct VulkanMemoryFlags {
    pub(crate) preferred: vk::MemoryPropertyFlags,
    pub(crate) required: vk::MemoryPropertyFlags,
}
