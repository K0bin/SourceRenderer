use sourcerenderer_core::gpu::{self, Heap, OutOfMemoryError};
use web_sys::GpuDevice;

use crate::{buffer::WebGPUBuffer, texture::WebGPUTexture, WebGPUBackend};

pub struct WebGPUAccelerationStructure {}

impl gpu::AccelerationStructure for WebGPUAccelerationStructure {}

pub struct WebGPUHeap {
    device: GpuDevice,
    memory_type_index: u32,
    mappable: bool
}

unsafe impl Send for WebGPUHeap {}
unsafe impl Sync for WebGPUHeap {}

impl Heap<WebGPUBackend> for WebGPUHeap {
    fn memory_type_index(&self) -> u32 {
        self.memory_type_index
    }

    unsafe fn create_buffer(&self, info: &gpu::BufferInfo, _offset: u64, name: Option<&str>) -> Result<WebGPUBuffer, gpu::OutOfMemoryError> {
        WebGPUBuffer::new(&self.device, info, self.mappable, name).map_err(|_| OutOfMemoryError {})
    }

    unsafe fn create_texture(&self, info: &gpu::TextureInfo, _offset: u64, name: Option<&str>) -> Result<WebGPUTexture, gpu::OutOfMemoryError> {
        WebGPUTexture::new(&self.device, info, name).map_err(|_| OutOfMemoryError {})
    }
}