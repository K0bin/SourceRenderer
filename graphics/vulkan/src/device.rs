use std::ffi::c_void;
use std::pin::Pin;
use std::sync::Arc;

use ash::vk;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{
    self,
    Device as _,
};

use super::*;

pub struct VkDevice {
    device: Arc<RawVkDevice>,
    graphics_queue: VkQueue,
    compute_queue: Option<VkQueue>,
    transfer_queue: Option<VkQueue>,
    shared: Arc<VkShared>,
    memory_type_infos: Vec<gpu::MemoryTypeInfo>,
}

impl VkDevice {
    pub unsafe fn new(
        device: Arc<RawVkDevice>,
        graphics_queue_info: VkQueueInfo,
        compute_queue_info: Option<VkQueueInfo>,
        transfer_queue_info: Option<VkQueueInfo>,
    ) -> Self {
        let shared = Arc::new(VkShared::new(&device));

        let graphics_queue =
            { VkQueue::new(graphics_queue_info, VkQueueType::Graphics, &device, &shared) };

        let compute_queue = compute_queue_info
            .map(|info| VkQueue::new(info, VkQueueType::Compute, &device, &shared));

        let transfer_queue = transfer_queue_info
            .map(|info| VkQueue::new(info, VkQueueType::Transfer, &device, &shared));

        // Memory types
        let memory_type_count = device.memory_properties.memory_type_count as usize;
        let memory_types = &device.memory_properties.memory_types[..memory_type_count];

        let mut memory_type_infos = Vec::<gpu::MemoryTypeInfo>::new();
        for memory_type in memory_types {
            let kind: gpu::MemoryKind = if memory_type
                .property_flags
                .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
            {
                gpu::MemoryKind::VRAM
            } else {
                gpu::MemoryKind::RAM
            };
            let is_cpu_accessible = memory_type
                .property_flags
                .contains(vk::MemoryPropertyFlags::HOST_VISIBLE);
            let is_cached = memory_type
                .property_flags
                .contains(vk::MemoryPropertyFlags::HOST_CACHED);
            let is_coherent = memory_type
                .property_flags
                .contains(vk::MemoryPropertyFlags::HOST_COHERENT);

            let info = gpu::MemoryTypeInfo {
                memory_index: memory_type.heap_index,
                memory_kind: kind,
                is_cached,
                is_cpu_accessible,
                is_coherent,
            };
            memory_type_infos.push(info);
        }

        VkDevice {
            device,
            graphics_queue,
            compute_queue,
            transfer_queue,
            shared,
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
        VkBuffer::new(
            &self.device,
            ResourceMemory::Dedicated { memory_type_index },
            info,
            name,
        )
    }

    unsafe fn create_shader(&self, shader: &gpu::PackedShader, name: Option<&str>) -> VkShader {
        VkShader::new(&self.device, shader, name)
    }

    unsafe fn create_texture(
        &self,
        info: &gpu::TextureInfo,
        memory_type_index: u32,
        name: Option<&str>,
    ) -> Result<VkTexture, gpu::OutOfMemoryError> {
        VkTexture::new(
            &self.device,
            info,
            ResourceMemory::Dedicated { memory_type_index },
            name,
        )
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

    unsafe fn create_mesh_graphics_pipeline(
        &self,
        info: &gpu::MeshGraphicsPipelineInfo<VkBackend>,
        name: Option<&str>,
    ) -> VkPipeline {
        let shared = &self.shared;
        VkPipeline::new_mesh_graphics(&self.device, info, shared, name)
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
        self.device.features_12.descriptor_indexing == vk::TRUE
    }

    fn supports_ray_tracing_pipeline(&self) -> bool {
        self.device
            .rt
            .as_ref()
            .map(|rt| rt.rt_pipelines.is_some())
            .unwrap_or_default()
    }

    fn supports_ray_tracing_query(&self) -> bool {
        self.device
            .rt
            .as_ref()
            .map(|rt| rt.rt_query)
            .unwrap_or_default()
    }

    fn supports_indirect_count(&self) -> bool {
        self.device.features.draw_indirect_first_instance == vk::TRUE
            && self.device.features.multi_draw_indirect == vk::TRUE
            && self.device.features_12.draw_indirect_count == vk::TRUE
    }

    fn supports_indirect_first_instance(&self) -> bool {
        self.device.features.draw_indirect_first_instance == vk::TRUE
    }

    fn supports_indirect_count_mesh_shader(&self) -> bool {
        self.supports_indirect_count()
    }

    fn supports_min_max_filter(&self) -> bool {
        self.device.features_12.sampler_filter_minmax == vk::TRUE
    }

    unsafe fn insert_texture_into_bindless_heap(&self, slot: u32, texture: &VkTextureView) {
        if let Some(bindless_set) = self.shared.bindless_texture_descriptor_set() {
            bindless_set.write_texture_descriptor(slot, texture);
        }
    }

    fn supports_barycentrics(&self) -> bool {
        self.device
            .features_barycentrics
            .fragment_shader_barycentric
            == vk::TRUE
    }

    fn supports_mesh_shader(&self) -> bool {
        self.device.mesh_shader.is_some()
    }

    unsafe fn memory_infos(&self) -> Vec<gpu::MemoryInfo> {
        let mut memory_infos = Vec::<gpu::MemoryInfo>::new();

        let supports_ext_budget = self.device.feature_memory_budget;

        let mut memory_properties = vk::PhysicalDeviceMemoryProperties2::default();
        let mut budget = vk::PhysicalDeviceMemoryBudgetPropertiesEXT::default();
        if supports_ext_budget {
            memory_properties.p_next =
                &mut budget as *mut vk::PhysicalDeviceMemoryBudgetPropertiesEXT as *mut c_void;
        }
        self.device.instance.get_physical_device_memory_properties2(
            self.device.physical_device,
            &mut memory_properties,
        );

        let heap_count = memory_properties.memory_properties.memory_heap_count as usize;
        let heaps = &memory_properties.memory_properties.memory_heaps[..heap_count];

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
                available: if supports_ext_budget {
                    heap_budget - heap_usage
                } else {
                    heap.size
                },
                total: if supports_ext_budget {
                    heap_budget
                } else {
                    heap.size
                },
                memory_kind: kind,
            };
            memory_infos.push(info);
        }
        memory_infos
    }

    unsafe fn memory_type_infos(&self) -> &[gpu::MemoryTypeInfo] {
        &self.memory_type_infos
    }

    unsafe fn create_heap(
        &self,
        memory_type_index: u32,
        size: u64,
    ) -> Result<VkMemoryHeap, gpu::OutOfMemoryError> {
        VkMemoryHeap::new(&self.device, memory_type_index, size)
    }

    unsafe fn get_buffer_heap_info(&self, info: &gpu::BufferInfo) -> gpu::ResourceHeapInfo {
        let mut queue_families = SmallVec::<[u32; 3]>::new();
        let mut sharing_mode = vk::SharingMode::EXCLUSIVE;
        if info.sharing_mode == gpu::QueueSharingMode::Concurrent
            && (self.device.transfer_queue_info.is_some()
                || self.device.compute_queue_info.is_some())
        {
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
            usage: buffer_usage_to_vk(info.usage, self.device.rt.is_some()),
            sharing_mode,
            p_queue_family_indices: queue_families.as_ptr(),
            queue_family_index_count: queue_families.len() as u32,
            ..Default::default()
        };

        let mut requirements = vk::MemoryRequirements2::default();
        let mut dedicated_requirements = vk::MemoryDedicatedRequirements::default();
        requirements.p_next =
            &mut dedicated_requirements as *mut vk::MemoryDedicatedRequirements as *mut c_void;
        let buffer_requirements_info = vk::DeviceBufferMemoryRequirements {
            p_create_info: &buffer_info as *const vk::BufferCreateInfo,
            ..Default::default()
        };
        self.device
            .get_device_buffer_memory_requirements(&buffer_requirements_info, &mut requirements);

        let mut alignment = requirements.memory_requirements.alignment;
        alignment = alignment.max(self.device.properties.limits.min_memory_map_alignment as u64);
        alignment = alignment.max(self.device.properties.limits.non_coherent_atom_size as u64);
        alignment = alignment.max(self.device.properties.limits.buffer_image_granularity as u64);
        if info
            .usage
            .contains(gpu::BufferUsage::COPY_DST | gpu::BufferUsage::COPY_SRC)
        {
            alignment = alignment.max(
                self.device
                    .properties
                    .limits
                    .min_uniform_buffer_offset_alignment,
            );
        }
        if info.usage.contains(gpu::BufferUsage::CONSTANT) {
            alignment = alignment.max(
                self.device
                    .properties
                    .limits
                    .min_uniform_buffer_offset_alignment,
            );
        }
        if info.usage.contains(gpu::BufferUsage::STORAGE) {
            alignment = alignment.max(
                self.device
                    .properties
                    .limits
                    .min_storage_buffer_offset_alignment,
            );
        }

        gpu::ResourceHeapInfo {
            dedicated_allocation_preference: if dedicated_requirements.requires_dedicated_allocation
                == vk::TRUE
            {
                gpu::DedicatedAllocationPreference::RequireDedicated
            } else if dedicated_requirements.prefers_dedicated_allocation == vk::TRUE {
                gpu::DedicatedAllocationPreference::PreferDedicated
            } else {
                gpu::DedicatedAllocationPreference::DontCare
            },
            memory_type_mask: requirements.memory_requirements.memory_type_bits,
            alignment,
            size: requirements.memory_requirements.size,
        }
    }

    unsafe fn get_texture_heap_info(&self, info: &gpu::TextureInfo) -> gpu::ResourceHeapInfo {
        let mut create_info_collection = VkImageCreateInfoCollection::default();
        let mut pinned = Pin::new(&mut create_info_collection);
        VkTexture::build_create_info(&self.device, pinned.as_mut(), info);

        let mut requirements = vk::MemoryRequirements2::default();
        let mut dedicated_requirements = vk::MemoryDedicatedRequirements::default();
        requirements.p_next =
            &mut dedicated_requirements as *mut vk::MemoryDedicatedRequirements as *mut c_void;
        let image_requirements_info = vk::DeviceImageMemoryRequirements {
            p_create_info: &pinned.create_info as *const vk::ImageCreateInfo,
            ..Default::default()
        };
        self.device
            .get_device_image_memory_requirements(&image_requirements_info, &mut requirements);

        let result = gpu::ResourceHeapInfo {
            dedicated_allocation_preference: if dedicated_requirements.requires_dedicated_allocation
                == vk::TRUE
            {
                gpu::DedicatedAllocationPreference::RequireDedicated
            } else if dedicated_requirements.prefers_dedicated_allocation == vk::TRUE {
                gpu::DedicatedAllocationPreference::PreferDedicated
            } else {
                gpu::DedicatedAllocationPreference::DontCare
            },
            memory_type_mask: requirements.memory_requirements.memory_type_bits,
            alignment: requirements
                .memory_requirements
                .alignment
                .max(self.device.properties.limits.buffer_image_granularity),
            size: requirements.memory_requirements.size,
        };

        if pinned
            .create_info
            .usage
            .contains(vk::ImageUsageFlags::HOST_TRANSFER_EXT)
            && self
                .device
                .host_image_copy
                .as_ref()
                .unwrap()
                .properties_host_image_copy
                .identical_memory_type_requirements
                == vk::FALSE
        {
            // Analyze the memory types & heaps we'd get without HOST_TRANSFER
            pinned.create_info.usage |= vk::ImageUsageFlags::TRANSFER_DST;
            pinned.create_info.usage &= !vk::ImageUsageFlags::HOST_TRANSFER_EXT;
            self.device
                .get_device_image_memory_requirements(&image_requirements_info, &mut requirements);

            // Out of all memory types, that the resource supports, find the largest DEVICE_LOCAL one.
            let mut memory_type_mask_without_host_image_copy =
                requirements.memory_requirements.memory_type_bits;
            let mut preferred_memory_type_index = Option::<usize>::None;
            while memory_type_mask_without_host_image_copy != 0 {
                let bit_pos = memory_type_mask_without_host_image_copy.trailing_zeros();

                let memory_type = &self.device.memory_properties.memory_types[bit_pos as usize];
                let memory_heap =
                    &self.device.memory_properties.memory_heaps[memory_type.heap_index as usize];

                if let Some(preferred_memory_type_index) = &mut preferred_memory_type_index {
                    let preferred_memory_type =
                        &self.device.memory_properties.memory_types[*preferred_memory_type_index];
                    let preferred_memory_heap = &self.device.memory_properties.memory_heaps
                        [preferred_memory_type.heap_index as usize];

                    let memory_type_device_local = memory_type
                        .property_flags
                        .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL);
                    let preferred_memory_type_device_local = preferred_memory_type
                        .property_flags
                        .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL);

                    if memory_heap.size > preferred_memory_heap.size
                        && preferred_memory_type_device_local == memory_type_device_local
                    {
                        // The heap of this memory type has the same "device locality" as the heap of the preferred memory type.
                        // So prefer this one instead.
                        *preferred_memory_type_index = bit_pos as usize;
                    } else if !preferred_memory_type_device_local && memory_type_device_local {
                        // This memory type of the preferred heap is not DEVICE_LOCAL but this one is.
                        // So prefer this one instead.
                        *preferred_memory_type_index = bit_pos as usize;
                    }
                } else {
                    // We don't have a preferred memory type yet, just pick this one.
                    preferred_memory_type_index = Some(bit_pos as usize)
                };

                let bit_mask = 1 << bit_pos;
                memory_type_mask_without_host_image_copy &= !bit_mask;
            }

            let preferred_memory_type =
                &self.device.memory_properties.memory_types[preferred_memory_type_index.unwrap()];
            let preferred_memory_heap = &self.device.memory_properties.memory_heaps
                [preferred_memory_type.heap_index as usize];
            let preferred_memory_type_device_local = preferred_memory_type
                .property_flags
                .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL);
            let mut memory_type_mask_with_host_image_copy =
                requirements.memory_requirements.memory_type_bits;
            let mut found_acceptable_heap = false;
            // Find a memory type whose heap matches the size and "device locality" of the preferred memory types heap.
            // This is meant to avoid the 256 MiB BAR heap on non ReBar systems.
            while memory_type_mask_with_host_image_copy != 0 {
                let bit_pos = memory_type_mask_with_host_image_copy.trailing_zeros();

                let memory_type = &self.device.memory_properties.memory_types[bit_pos as usize];
                let memory_heap =
                    &self.device.memory_properties.memory_heaps[memory_type.heap_index as usize];

                let memory_type_device_local = memory_type
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL);

                if (preferred_memory_type_device_local == memory_type_device_local)
                    && memory_heap.size >= preferred_memory_heap.size
                {
                    found_acceptable_heap = true;
                    break;
                }

                let bit_mask = 1 << bit_pos;
                memory_type_mask_with_host_image_copy &= !bit_mask;
            }

            if !found_acceptable_heap {
                let result_without_host_image_copy = gpu::ResourceHeapInfo {
                    dedicated_allocation_preference: if dedicated_requirements
                        .requires_dedicated_allocation
                        == vk::TRUE
                    {
                        gpu::DedicatedAllocationPreference::RequireDedicated
                    } else if dedicated_requirements.prefers_dedicated_allocation == vk::TRUE {
                        gpu::DedicatedAllocationPreference::PreferDedicated
                    } else {
                        gpu::DedicatedAllocationPreference::DontCare
                    },
                    memory_type_mask: requirements.memory_requirements.memory_type_bits,
                    alignment: requirements
                        .memory_requirements
                        .alignment
                        .max(self.device.properties.limits.buffer_image_granularity),
                    size: requirements.memory_requirements.size,
                };

                log::info!("Fitting memory types with HOST_TRANSFER are not acceptable. Falling back to regular GPU copies.\nWith HOST_TRANSFER: {:?}\nWithout HOST_TRANSFER: {:?}", &result, &result_without_host_image_copy);
                return result_without_host_image_copy;
            }
        }
        result
    }

    unsafe fn get_bottom_level_acceleration_structure_size(
        &self,
        info: &gpu::BottomLevelAccelerationStructureInfo<VkBackend>,
    ) -> gpu::AccelerationStructureSizes {
        VkAccelerationStructure::bottom_level_size(&self.device, info)
    }

    unsafe fn get_top_level_acceleration_structure_size(
        &self,
        info: &gpu::TopLevelAccelerationStructureInfo<VkBackend>,
    ) -> gpu::AccelerationStructureSizes {
        VkAccelerationStructure::top_level_size(&self.device, info)
    }

    unsafe fn get_raytracing_pipeline_sbt_buffer_size(
        &self,
        info: &gpu::RayTracingPipelineInfo<VkBackend>,
    ) -> u64 {
        VkPipeline::ray_tracing_buffer_size(&self.device, info, &self.shared)
    }

    unsafe fn create_raytracing_pipeline(
        &self,
        info: &gpu::RayTracingPipelineInfo<VkBackend>,
        sbt_buffer: &VkBuffer,
        sbt_buffer_offset: u64,
        name: Option<&str>,
    ) -> VkPipeline {
        VkPipeline::new_ray_tracing(
            &self.device,
            info,
            &self.shared,
            sbt_buffer,
            sbt_buffer_offset,
            name,
        )
    }

    fn get_top_level_instances_buffer_size(
        &self,
        instances: &[gpu::AccelerationStructureInstance<VkBackend>],
    ) -> u64 {
        (std::mem::size_of::<vk::AccelerationStructureInstanceKHR>() * instances.len()) as u64
    }

    unsafe fn transition_texture(
        &self,
        dst: &VkTexture,
        transition: &gpu::CPUTextureTransition<'_, VkBackend>,
    ) {
        let host_img_copy = self.device.host_image_copy.as_ref().unwrap();
        host_img_copy
            .host_image_copy
            .transition_image_layout(&[vk::HostImageLayoutTransitionInfoEXT {
                image: dst.handle(),
                old_layout: texture_layout_to_image_layout(transition.old_layout),
                new_layout: texture_layout_to_image_layout(transition.new_layout),
                subresource_range: vk::ImageSubresourceRange {
                    aspect_mask: aspect_mask_from_format(dst.info().format),
                    base_mip_level: 0,
                    level_count: dst.info().mip_levels,
                    base_array_layer: 0,
                    layer_count: dst.info().array_length,
                },
                ..Default::default()
            }])
            .unwrap();
    }

    unsafe fn copy_to_texture(
        &self,
        src: *const c_void,
        dst: &VkTexture,
        texture_layout: gpu::TextureLayout,
        region: &gpu::MemoryTextureCopyRegion,
    ) {
        let host_img_copy = self.device.host_image_copy.as_ref().unwrap();
        let format = dst.info().format;
        let texels_width = if region.row_pitch != 0 {
            (region.row_pitch as u32) * format.block_size().x / format.element_size()
        } else {
            0
        };
        let texels_height = if region.slice_pitch != 0 {
            (region.slice_pitch as u32) / texels_width * format.block_size().y
                / format.element_size()
        } else {
            0
        };

        let region = vk::MemoryToImageCopyEXT {
            p_host_pointer: src,
            memory_row_length: texels_width,
            memory_image_height: texels_height,
            image_subresource: texture_subresource_to_vk_layers(
                &region.texture_subresource,
                format,
                1,
            ),
            image_offset: vk::Offset3D {
                x: region.texture_offset.x as i32,
                y: region.texture_offset.y as i32,
                z: region.texture_offset.z as i32,
            },
            image_extent: vk::Extent3D {
                width: region.texture_extent.x,
                height: region.texture_extent.y,
                depth: region.texture_extent.z,
            },
            ..Default::default()
        };

        host_img_copy
            .host_image_copy
            .copy_memory_to_image(&vk::CopyMemoryToImageInfoEXT {
                flags: vk::HostImageCopyFlagsEXT::empty(),
                dst_image: dst.handle(),
                dst_image_layout: texture_layout_to_image_layout(texture_layout),
                p_regions: &region as *const vk::MemoryToImageCopyEXT,
                region_count: 1,
                ..Default::default()
            })
            .unwrap();
    }

    unsafe fn create_query_pool(&self, count: u32) -> VkQueryPool {
        VkQueryPool::new(&self.device, vk::QueryType::OCCLUSION, count)
    }

    unsafe fn create_split_barrier(&self) -> VkEvent {
        VkEvent::new(&self.device)
    }

    unsafe fn reset_split_barrier(&self, split_barrier: &VkEvent) {
        self.device.reset_event(split_barrier.handle()).unwrap();
    }
}

impl Drop for VkDevice {
    fn drop(&mut self) {
        unsafe {
            self.wait_for_idle();
        }
    }
}
