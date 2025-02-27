use std::sync::Arc;

use metal::{self, MTLRegion};
use smallvec::{smallvec, SmallVec};

use sourcerenderer_core::{align_up_32, gpu::{self, DedicatedAllocationPreference, Texture as _, TextureLayout}};

use super::*;

pub struct MTLDevice {
    device: metal::Device,
    memory_type_infos: SmallVec<[gpu::MemoryTypeInfo; 3]>,
    graphics_queue: MTLQueue,
    compute_queue: MTLQueue,
    transfer_queue: MTLQueue,
    shared: Arc<MTLShared>,
}

impl MTLDevice {
    pub(crate) fn new(device: &metal::DeviceRef, _surface: &MTLSurface) -> Self {
        // We basically have to set up memory types similar to a device with a discrete GPU
        // despite the fact that almost all devices supported by Metal are UMA devices.
        // Metals weird rules for the StorageMode force us to do this.
        let mut infos: SmallVec<[gpu::MemoryTypeInfo; 3]> = smallvec![
            gpu::MemoryTypeInfo {
                is_cached: false,
                is_coherent: true,
                is_cpu_accessible: true,
                memory_index: 0,
                memory_kind: gpu::MemoryKind::RAM
            },
            gpu::MemoryTypeInfo {
                is_cached: true,
                is_coherent: true,
                is_cpu_accessible: true,
                memory_index: 0,
                memory_kind: gpu::MemoryKind::RAM
            },
            gpu::MemoryTypeInfo {
                is_cached: false,
                is_coherent: false,
                is_cpu_accessible: false,
                memory_index: 0,
                memory_kind: gpu::MemoryKind::VRAM
            }
        ];

        if !device.has_unified_memory() {
            infos[2].memory_index = 1;
        }

        let bindless = MTLBindlessArgumentBuffer::new(&device, gpu::BINDLESS_TEXTURE_COUNT as usize);
        let shared = Arc::new(MTLShared::new(device, bindless));

        Self {
            device: device.to_owned(),
            memory_type_infos: infos,
            graphics_queue: MTLQueue::new(device, &shared),
            compute_queue: MTLQueue::new(device, &shared),
            transfer_queue: MTLQueue::new(device, &shared),
            shared
        }
    }

    pub(crate) fn resource_options_for_memory_type(&self, memory_type_index: u32) -> metal::MTLResourceOptions {
        let memory_type = &self.memory_type_infos[memory_type_index as usize];
        let mut options = metal::MTLResourceOptions::empty();
        if memory_type.is_cpu_accessible {
            options |= metal::MTLResourceOptions::StorageModeShared;
            if memory_type.is_cached {
                options |= metal::MTLResourceOptions::CPUCacheModeDefaultCache;
            } else {
                options |= metal::MTLResourceOptions::CPUCacheModeWriteCombined;
            }
        } else {
            options |= metal::MTLResourceOptions::StorageModePrivate;
        }
        options
    }

    pub fn handle(&self) -> &metal::DeviceRef {
        &self.device
    }
}

impl gpu::Device<MTLBackend> for MTLDevice {
    unsafe fn memory_type_infos(&self) -> &[gpu::MemoryTypeInfo] {
        &self.memory_type_infos
    }

    unsafe fn memory_infos(&self) -> Vec<gpu::MemoryInfo> {
        let total = self.device.recommended_max_working_set_size();
        let used = self.device.current_allocated_size();

        if self.device.has_unified_memory() {
            vec![
                gpu::MemoryInfo {
                    available: total - used.min(total),
                    total: total,
                    memory_kind: gpu::MemoryKind::RAM
                }
            ]
        } else {
            vec![
                gpu::MemoryInfo {
                    available: u64::MAX,
                    total: u64::MAX,
                    memory_kind: gpu::MemoryKind::RAM
                },
                gpu::MemoryInfo {
                    available: total - used.min(total),
                    total: total,
                    memory_kind: gpu::MemoryKind::RAM
                }
            ]
        }
    }

    unsafe fn create_heap(&self, memory_type_index: u32, size: u64) -> Result<MTLHeap, gpu::OutOfMemoryError> {
        let memory_type = &self.memory_type_infos[memory_type_index as usize];

        let is_apple_gpu = self.device.supports_family(metal::MTLGPUFamily::Apple7);
        if !is_apple_gpu && memory_type.is_cpu_accessible {
            return Err(gpu::OutOfMemoryError {});
        }

        let options = self.resource_options_for_memory_type(memory_type_index);

        MTLHeap::new(
            &self.device,
            &self.shared,
            size,
            memory_type_index,
            memory_type.is_cached,
            memory_type.memory_kind,
            options
        )
    }

    unsafe fn create_buffer(&self, info: &gpu::BufferInfo, memory_type_index: u32, name: Option<&str>) -> Result<MTLBuffer, gpu::OutOfMemoryError> {
        MTLBuffer::new(
            ResourceMemory::Dedicated {
                device: &self.device,
                options: self.resource_options_for_memory_type(memory_type_index)
            },
            info,
            name
        )
    }

    unsafe fn get_buffer_heap_info(&self, info: &gpu::BufferInfo) -> gpu::ResourceHeapInfo {
        let options = MTLBuffer::resource_options(info);
        let size_and_align = self.device.heap_buffer_size_and_align(info.size, options);

        /*
        For devices with Apple silicon, you can create a heap with either the MTLStorageMode.private or the MTLStorageMode.shared storage mode.
        However, you can only create heaps with private storage on macOS devices without Apple silicon.
        */

        let is_apple_gpu = self.device.supports_family(metal::MTLGPUFamily::Apple7);
        let is_uma = self.device.has_unified_memory();

        let mut memory_type_mask = if !is_uma { 1 | 1 << 1 | 1 << 2 } else { 1 | 1 << 1 };
        if info.usage.contains(gpu::BufferUsage::ACCELERATION_STRUCTURE) {
            // Acceleration structures must be private
            memory_type_mask = 1 << 2;
        }

        gpu::ResourceHeapInfo {
            dedicated_allocation_preference: if !is_apple_gpu || info.usage.gpu_writable() {
                DedicatedAllocationPreference::RequireDedicated
            } else {
                DedicatedAllocationPreference::DontCare
            },
            memory_type_mask,
            alignment: size_and_align.align,
            size: size_and_align.size,
        }
    }

    unsafe fn create_texture(&self, info: &gpu::TextureInfo, memory_type_index: u32, name: Option<&str>) -> Result<MTLTexture, gpu::OutOfMemoryError> {
        MTLTexture::new(
            ResourceMemory::Dedicated {
                device: &self.device,
                options: self.resource_options_for_memory_type(memory_type_index)
            },
            info,
            name
        )
    }

    unsafe fn create_shader(&self, shader: &gpu::PackedShader, name: Option<&str>) -> MTLShader {
        MTLShader::new(&self.device, shader, name)
    }

    unsafe fn create_texture_view(&self, texture: &MTLTexture, info: &gpu::TextureViewInfo, name: Option<&str>) -> MTLTextureView {
        MTLTextureView::new(texture, info, name)
    }

    unsafe fn create_compute_pipeline(&self, shader: &MTLShader, name: Option<&str>) -> MTLComputePipeline {
        MTLComputePipeline::new(&self.device, shader, name)
    }

    unsafe fn create_sampler(&self, info: &gpu::SamplerInfo) -> MTLSampler {
        MTLSampler::new(&self.device, info)
    }

    unsafe fn create_graphics_pipeline(&self, info: &gpu::GraphicsPipelineInfo<MTLBackend>, name: Option<&str>) -> MTLGraphicsPipeline {
        MTLGraphicsPipeline::new(&self.device, info, name)
    }

    unsafe fn wait_for_idle(&self) {
        self.transfer_queue.wait_for_idle();
        self.compute_queue.wait_for_idle();
        self.graphics_queue.wait_for_idle();
    }

    unsafe fn create_fence(&self, is_cpu_accessible: bool) -> MTLFence {
        MTLFence::new(&self.device, is_cpu_accessible)
    }

    unsafe fn get_texture_heap_info(&self, info: &gpu::TextureInfo) -> gpu::ResourceHeapInfo {
        let descriptor = MTLTexture::descriptor(info);
        let size_and_align = self.device.heap_texture_size_and_align(&descriptor);

        /*
        For devices with Apple silicon, you can create a heap with either the MTLStorageMode.private or the MTLStorageMode.shared storage mode.
        However, you can only create heaps with private storage on macOS devices without Apple silicon.
        */

        let is_apple_gpu = self.device.supports_family(metal::MTLGPUFamily::Apple7);
        let is_uma = self.device.has_unified_memory();
        gpu::ResourceHeapInfo {
            dedicated_allocation_preference: if !is_apple_gpu || info.usage.gpu_writable() {
                DedicatedAllocationPreference::RequireDedicated
            } else {
                DedicatedAllocationPreference::DontCare
            },
            memory_type_mask: if !is_uma { 1 | 1 << 1 | 1 << 2 } else { 1 | 1 << 1 },
            alignment: size_and_align.align,
            size: size_and_align.size,
        }
    }

    unsafe fn insert_texture_into_bindless_heap(&self, slot: u32, texture: &MTLTextureView) {
        self.shared.bindless.insert(texture, slot);
    }

    fn graphics_queue(&self) -> &MTLQueue {
        &self.graphics_queue
    }

    fn compute_queue(&self) -> Option<&MTLQueue> {
        Some(&self.compute_queue)
    }

    fn transfer_queue(&self) -> Option<&MTLQueue> {
        Some(&self.transfer_queue)
    }

    fn supports_bindless(&self) -> bool {
        true
    }

    fn supports_ray_tracing(&self) -> bool {
        self.device.supports_raytracing()
    }

    fn supports_indirect(&self) -> bool {
        true
    }

    fn supports_min_max_filter(&self) -> bool {
        false
    }

    fn supports_barycentrics(&self) -> bool {
        self.device.supports_shader_barycentric_coordinates()
    }

    unsafe fn get_bottom_level_acceleration_structure_size(&self, info: &gpu::BottomLevelAccelerationStructureInfo<MTLBackend>) -> gpu::AccelerationStructureSizes {
        MTLAccelerationStructure::bottom_level_size(&self.device, info)
    }

    unsafe fn get_top_level_acceleration_structure_size(&self, info: &gpu::TopLevelAccelerationStructureInfo<MTLBackend>) -> gpu::AccelerationStructureSizes {
        MTLAccelerationStructure::top_level_size(&self.device, &self.shared, info)
    }

    fn get_top_level_instances_buffer_size(&self, instances: &[gpu::AccelerationStructureInstance<MTLBackend>]) -> u64 {
        (instances.len() * std::mem::size_of::<metal::MTLAccelerationStructureUserIDInstanceDescriptor>()) as u64
    }

    unsafe fn get_raytracing_pipeline_sbt_buffer_size(&self, _info: &gpu::RayTracingPipelineInfo<MTLBackend>) -> u64 {
        panic!("The Metal backend does not support RT pipelines.")
    }

    unsafe fn create_raytracing_pipeline(&self, _info: &gpu::RayTracingPipelineInfo<MTLBackend>, _sbt_buffer: &MTLBuffer, _sbt_buffer_offset: u64, _name: Option<&str>) -> MTLRayTracingPipeline {
        panic!("The Metal backend does not support RT pipelines.")
    }

    unsafe fn transition_texture(&self, _dst: &MTLTexture, _transition: &gpu::CPUTextureTransition<'_, MTLBackend>) {}

    unsafe fn copy_to_texture(&self, src: *const std::ffi::c_void, dst: &MTLTexture, _texture_layout: TextureLayout, region: &gpu::MemoryTextureCopyRegion) {
        let mtl_region = MTLRegion::new_3d(region.texture_offset.x as u64, region.texture_offset.y as u64, region.texture_offset.z as u64,
            region.texture_extent.x as u64, region.texture_extent.y as u64, region.texture_extent.z as u64);

        let format = dst.info().format;
        let row_pitch = if region.row_pitch != 0 {
            region.row_pitch
        } else {
            (align_up_32(region.texture_extent.x, format.block_size().x) / format.block_size().x * format.element_size()) as u64
        };
        let slice_pitch = if region.slice_pitch != 0 {
            region.slice_pitch
        } else {
            (align_up_32(region.texture_extent.y, format.block_size().y) / format.block_size().y) as u64 * row_pitch
        };

        dst.handle().replace_region_in_slice(
            mtl_region,
            region.texture_subresource.mip_level as u64,
            region.texture_subresource.array_layer as u64,
            src, row_pitch, slice_pitch
        );
    }
}
