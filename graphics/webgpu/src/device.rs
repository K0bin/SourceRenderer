use smallvec::{SmallVec, smallvec};
use sourcerenderer_core::gpu;
use web_sys::{GpuAdapter, GpuDevice};

use crate::{WebGPUBackend, WebGPUBuffer, WebGPUComputePipeline, WebGPUFence, WebGPUGraphicsPipeline, WebGPUHeap, WebGPUQueue, WebGPUSampler, WebGPUShader, WebGPUShared, WebGPUTexture, WebGPUTextureView};

pub struct WebGPUDevice {
    device: GpuDevice,
    shared: WebGPUShared,
    memory_infos: [gpu::MemoryTypeInfo; 3],
    queue: WebGPUQueue
}

unsafe impl Send for WebGPUDevice {}
unsafe impl Sync for WebGPUDevice {}

impl WebGPUDevice {
    pub fn new(device: GpuDevice) -> Self {
        let memory_infos: [gpu::MemoryTypeInfo; 3] = [
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
                memory_index: 1,
                memory_kind: gpu::MemoryKind::VRAM
            }
        ];

        let shared = WebGPUShared::new(&device);
        let queue = WebGPUQueue::new(&device);

        Self {
            device,
            shared,
            memory_infos,
            queue
        }
    }

    pub fn handle(&self) -> &GpuDevice {
        &self.device
    }
}

impl gpu::Device<WebGPUBackend> for WebGPUDevice {
    unsafe fn create_buffer(&self, info: &gpu::BufferInfo, memory_type_index: u32, name: Option<&str>) -> Result<WebGPUBuffer, gpu::OutOfMemoryError> {
        let mem = &self.memory_infos[memory_type_index as usize];
        WebGPUBuffer::new(&self.device, info, mem.is_cpu_accessible, name).map_err(|_e| gpu::OutOfMemoryError {})
    }


    unsafe fn create_texture(&self, info: &gpu::TextureInfo, _memory_type_index: u32, name: Option<&str>) -> Result<WebGPUTexture, gpu::OutOfMemoryError> {
        WebGPUTexture::new(&self.device, info, name).map_err(|_e| gpu::OutOfMemoryError {})
    }

    unsafe fn create_shader(&self, shader: &gpu::PackedShader, name: Option<&str>) -> WebGPUShader {
        WebGPUShader::new(&self.device, shader, name)
    }

    unsafe fn create_texture_view(&self, texture: &WebGPUTexture, info: &gpu::TextureViewInfo, name: Option<&str>) -> WebGPUTextureView {
        WebGPUTextureView::new(&self.device, texture, info, name).unwrap()
    }

    unsafe fn create_compute_pipeline(&self, shader: &WebGPUShader, name: Option<&str>) -> WebGPUComputePipeline {
        WebGPUComputePipeline::new(&self.device, shader, name).unwrap()
    }

    unsafe fn create_sampler(&self, info: &gpu::SamplerInfo) -> WebGPUSampler {
        WebGPUSampler::new(&self.device, info, None).unwrap()
    }

    unsafe fn create_graphics_pipeline(&self, info: &gpu::GraphicsPipelineInfo<WebGPUBackend>, name: Option<&str>) -> WebGPUGraphicsPipeline {
        WebGPUGraphicsPipeline::new(&self.device, info, &self.shared, name).unwrap()
    }

    unsafe fn wait_for_idle(&self) {}

    unsafe fn create_fence(&self, _is_cpu_accessible: bool) -> WebGPUFence {
        WebGPUFence::new(&self.device)
    }

    unsafe fn memory_infos(&self) -> Vec<gpu::MemoryInfo> {
        /*
            TODO: Implement rudimentary memory tracking by having a fixed number and change it in WebGPUTexture, WebGPUBuffer and WebGPUHeap.
            Increase it in the constructor and decrease it in the destructor.
         */
        vec![gpu::MemoryInfo {
            available: (u32::MAX as u64) / 2u64, total: (u32::MAX as u64) / 2u64, memory_kind: gpu::MemoryKind::RAM
        }, gpu::MemoryInfo {
            available: (u32::MAX as u64) / 2u64, total: (u32::MAX as u64) / 2u64, memory_kind: gpu::MemoryKind::VRAM
        }]
    }

    unsafe fn memory_type_infos(&self) -> &[gpu::MemoryTypeInfo] {
        &self.memory_infos
    }

    unsafe fn create_heap(&self, memory_type_index: u32, size: u64) -> Result<WebGPUHeap, gpu::OutOfMemoryError> {
        let mem = &self.memory_infos[memory_type_index as usize];
        Ok(WebGPUHeap::new(&self.device, memory_type_index, size, mem.is_cpu_accessible))
    }

    unsafe fn get_buffer_heap_info(&self, info: &gpu::BufferInfo) -> gpu::ResourceHeapInfo {
        gpu::ResourceHeapInfo {
            dedicated_allocation_preference: gpu::DedicatedAllocationPreference::PreferDedicated,
            memory_type_mask: 1 | (1 << 1) | (1 << 2),
            alignment: 4,
            size: info.size,
        }
    }

    unsafe fn get_texture_heap_info(&self, info: &gpu::TextureInfo) -> gpu::ResourceHeapInfo {
        gpu::ResourceHeapInfo {
            dedicated_allocation_preference: gpu::DedicatedAllocationPreference::PreferDedicated,
            memory_type_mask: 1 << 2,
            alignment: 4,
            size: (info.width * info.height * info.array_length * (4 * 4)) as u64, // TODO: We just assume RGBA Float32, make this take the format into account properly
        }
    }

    unsafe fn insert_texture_into_bindless_heap(&self, _slot: u32, _texture: &WebGPUTextureView) {
        panic!("WebGPU does not support bindless textures");
    }

    fn graphics_queue(&self) -> &WebGPUQueue {
        &self.queue
    }

    fn compute_queue(&self) -> Option<&WebGPUQueue> {
        None
    }

    fn transfer_queue(&self) -> Option<&WebGPUQueue> {
        None
    }

    fn supports_bindless(&self) -> bool {
        false
    }

    fn supports_ray_tracing(&self) -> bool {
        false
    }

    fn supports_indirect(&self) -> bool {
        true
    }

    fn supports_min_max_filter(&self) -> bool {
        false
    }

    fn supports_barycentrics(&self) -> bool {
        false
    }

    unsafe fn get_bottom_level_acceleration_structure_size(&self, _info: &gpu::BottomLevelAccelerationStructureInfo<WebGPUBackend>) -> gpu::AccelerationStructureSizes {
        panic!("WebGPU does not support bindless")
    }

    unsafe fn get_top_level_acceleration_structure_size(&self, _info: &gpu::TopLevelAccelerationStructureInfo<WebGPUBackend>) -> gpu::AccelerationStructureSizes {
        panic!("WebGPU does not support bindless")
    }

    fn get_top_level_instances_buffer_size(&self, _instances: &[gpu::AccelerationStructureInstance<WebGPUBackend>]) -> u64 {
        panic!("WebGPU does not support bindless")
    }

    unsafe fn get_raytracing_pipeline_sbt_buffer_size(&self, _info: &gpu::RayTracingPipelineInfo<WebGPUBackend>) -> u64 {
        panic!("WebGPU does not support bindless")
    }

    unsafe fn create_raytracing_pipeline(&self, _info: &gpu::RayTracingPipelineInfo<WebGPUBackend>, _sbt_buffer: &WebGPUBuffer, _sbt_buffer_offset: u64, _name: Option<&str>) -> () {
        panic!("WebGPU does not support bindless")
    }
}
