use std::{sync::{Arc, atomic::{AtomicBool, Ordering}}, mem::ManuallyDrop};

use sourcerenderer_core::gpu::*;

use super::*;

struct GPUDevice<B: GPUBackend> {
    device: Arc<B::Device>,
    allocator: ManuallyDrop<Arc<MemoryAllocator<B>>>,
    destroyer: ManuallyDrop<Arc<DeferredDestroyer<B>>>,
    buffer_allocator: ManuallyDrop<BufferAllocator<B>>,
    transfer: ManuallyDrop<Transfer<B>>,
    prerendered_frames: u32,
    has_context: AtomicBool
}

impl<B: GPUBackend> GPUDevice<B> {
    pub fn create_context(&self) -> GraphicsContext<B> {
        assert!(!self.has_context.swap(true, Ordering::AcqRel));
        GraphicsContext::new(&self.device, &self.allocator, &self.destroyer, self.prerendered_frames)
    }

    pub fn create_texture(&self, info: &TextureInfo, name: Option<&str>) -> Result<Arc<super::Texture<B>>, OutOfMemoryError> {
        super::Texture::new(&self.device, &self.allocator, &self.destroyer, info, name)
    }

    pub fn create_texture_view(&self, info: &TextureViewInfo, texture: &Arc<super::Texture<B>>, name: Option<&str>) -> super::TextureView<B> {
        super::TextureView::new(&self.device, &self.destroyer, texture, info, name)
    }

    pub fn create_buffer(&self, info: &BufferInfo, memory_usage: MemoryUsage, name: Option<&str>) -> Result<Arc<BufferSlice<B>>, OutOfMemoryError> {
        self.buffer_allocator.get_slice(info, memory_usage, name)
    }

    pub fn create_fence(&self) -> super::Fence<B> {
        super::Fence::new(self.device.as_ref(), &self.destroyer)
    }

    pub fn create_sampler(&self, info: &SamplerInfo) -> super::Sampler<B> {
        super::Sampler::new(&self.device, &self.destroyer, info)
    }

    pub fn upload_data<T>(&self, data: &[T], memory_usage: MemoryUsage, usage: BufferUsage) -> Result<Arc<BufferSlice<B>>, OutOfMemoryError> {
        let slice = self.buffer_allocator.get_slice(&BufferInfo {
            size: std::mem::size_of_val(data) as u64,
            usage,
            sharing_mode: QueueSharingMode::Concurrent
        }, memory_usage, None)?;

        unsafe {
            let ptr = slice.map(false).unwrap();
            std::ptr::copy(data.as_ptr(), ptr as *mut T, data.len());
            slice.unmap(true);

        }
        Ok(slice)
    }

    pub fn init_buffer<T>(&self, data: &[T], dst: &Arc<BufferSlice<B>>) -> Result<(), OutOfMemoryError> {
        let slice = self.upload_data(data, MemoryUsage::MainMemoryWriteCombined, BufferUsage::COPY_SRC)?;
        self.transfer.init_buffer(&slice, dst, 0, 0, WHOLE_BUFFER);
        Ok(())
    }

    pub fn init_texture<T>(
        &self,
        data: &[T],
        dst: &Arc<super::Texture<B>>,
        mip_level: u32,
        array_layer: u32) -> Result<(), OutOfMemoryError> {
        let slice = self.upload_data(data, MemoryUsage::MainMemoryWriteCombined, BufferUsage::COPY_SRC)?;
        self.transfer.init_texture(dst, &slice, mip_level, array_layer, 0);
        Ok(())
    }

    pub fn init_texture_from_buffer<T>(
        &self,
        dst: &Arc<super::Texture<B>>,
        src: &Arc<BufferSlice<B>>,
        mip_level: u32,
        array_layer: u32,
        buffer_offset: u64
    ) {
        self.transfer.init_texture(dst, src, mip_level, array_layer, buffer_offset);
    }

    pub fn init_texture_async<T>(
        &self,
        data: &[T],
        dst: &Arc<super::Texture<B>>,
        mip_level: u32,
        array_layer: u32) -> Result<Option<SharedFenceValuePair<B>>, OutOfMemoryError> {
        let slice = self.upload_data(data, MemoryUsage::MainMemoryWriteCombined, BufferUsage::COPY_SRC)?;
        Ok(self.transfer.init_texture_async(dst, &slice, mip_level, array_layer, 0))
    }

    pub fn init_texture_from_buffer_async<T>(
        &self,
        dst: &Arc<super::Texture<B>>,
        src: &Arc<BufferSlice<B>>,
        mip_level: u32,
        array_layer: u32,
        buffer_offset: u64
    ) -> Option<SharedFenceValuePair<B>> {
        self.transfer.init_texture_async(dst, src, mip_level, array_layer, buffer_offset)
    }

    fn flush_transfers(&self) {
        self.transfer.flush();
    }

    fn free_completed_transfers(&self) {
        self.transfer.try_free_unused_buffers();
    }
}

impl<B: GPUBackend> Drop for GPUDevice<B> {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.buffer_allocator);
            ManuallyDrop::drop(&mut self.transfer);
            self.device.wait_for_idle();
            self.destroyer.destroy_unused(u64::MAX);
            ManuallyDrop::drop(&mut self.destroyer);
            ManuallyDrop::drop(&mut self.allocator);
        }
    }
}
