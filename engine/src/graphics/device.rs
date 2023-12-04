use std::{sync::{Arc, atomic::{AtomicBool, Ordering}}, mem::ManuallyDrop};

use sourcerenderer_core::gpu::*;
use sourcerenderer_core::gpu::Device as GPUDevice;
use sourcerenderer_core::gpu::RayTracingPipelineInfo;

use super::*;

pub struct Device<B: GPUBackend> {
    device: Arc<B::Device>,
    allocator: ManuallyDrop<Arc<MemoryAllocator<B>>>,
    destroyer: ManuallyDrop<Arc<DeferredDestroyer<B>>>,
    buffer_allocator: ManuallyDrop<Arc<BufferAllocator<B>>>,
    bindless_slot_allocator: BindlessSlotAllocator,
    transfer: ManuallyDrop<Transfer<B>>,
    prerendered_frames: u32,
    has_context: AtomicBool
}

impl<B: GPUBackend> Device<B> {
    pub fn new(device: B::Device) -> Self {
        let device = Arc::new(device);
        let memory_allocator = ManuallyDrop::new(Arc::new(MemoryAllocator::new(&device)));
        let destroyer = ManuallyDrop::new(Arc::new(DeferredDestroyer::new()));
        Self {
            device: device.clone(),
            allocator: memory_allocator.clone(),
            destroyer: destroyer.clone(),
            buffer_allocator: ManuallyDrop::new(Arc::new(BufferAllocator::new(&device, &memory_allocator))),
            bindless_slot_allocator: BindlessSlotAllocator::new(500_000),
            transfer: ManuallyDrop::new(Transfer::new(&device, &destroyer)),
            prerendered_frames: 3,
            has_context: AtomicBool::new(false)
        }
    }

    pub(super) fn handle(&self) -> &Arc<B::Device> {
        &self.device
    }

    pub(super) fn destroyer(&self) -> &Arc<DeferredDestroyer<B>> {
        &self.destroyer
    }

    pub fn create_context(&self) -> GraphicsContext<B> {
        assert!(!self.has_context.swap(true, Ordering::AcqRel));
        GraphicsContext::new(&self.device, &self.allocator, &self.buffer_allocator, &self.destroyer, self.prerendered_frames)
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

    pub fn create_graphics_pipeline(&self, info: &GraphicsPipelineInfo<B>, renderpass_info: &RenderPassInfo, subpass: u32, name: Option<&str>) -> Arc<B::GraphicsPipeline> {
        // TODO: Create pipeline wrapper to defer destruction
        unsafe {
            Arc::new(self.device.create_graphics_pipeline(info, renderpass_info, subpass, name))
        }
    }

    pub fn create_compute_pipeline(&self, shader: &B::Shader, name: Option<&str>) -> Arc<B::ComputePipeline> {
        // TODO: Create pipeline wrapper to defer destruction
        unsafe {
            Arc::new(self.device.create_compute_pipeline(shader, name))
        }
    }

    pub fn create_raytracing_pipeline(&self, info: &RayTracingPipelineInfo<B>, name: Option<&str>) -> Result<Arc<B::RayTracingPipeline>, OutOfMemoryError> {
        // TODO: Create pipeline wrapper to defer destruction & hold reference to buffer slice!
        unsafe {
            let sbt_buffer_size = self.device.get_raytracing_pipeline_sbt_buffer_size(info);
            let sbt_buffer_slice = self.buffer_allocator.get_slice(&BufferInfo {
                size: sbt_buffer_size,
                usage: BufferUsage::SHADER_BINDING_TABLE,
                sharing_mode: QueueSharingMode::Exclusive
            }, MemoryUsage::GPUMemory, name)?;
            Ok(Arc::new(self.device.create_raytracing_pipeline(info, sbt_buffer_slice.handle(), sbt_buffer_slice.offset())))
        }
    }

    pub fn upload_data<T>(&self, data: &[T], memory_usage: MemoryUsage, usage: BufferUsage) -> Result<Arc<BufferSlice<B>>, OutOfMemoryError> {
        let slice = self.buffer_allocator.get_slice(&BufferInfo {
            size: std::mem::size_of_val(data) as u64,
            usage,
            sharing_mode: QueueSharingMode::Concurrent
        }, memory_usage, None)?;

        unsafe {
            let ptr: *mut std::ffi::c_void = slice.map(false).unwrap();
            std::ptr::copy(data.as_ptr(), ptr as *mut T, data.len());
            slice.unmap(true);

        }
        Ok(slice)
    }

    pub fn init_buffer<T>(&self, data: &[T], dst: &Arc<BufferSlice<B>>, dst_offset: u32) -> Result<(), OutOfMemoryError> {
        let slice = self.upload_data(data, MemoryUsage::MainMemoryWriteCombined, BufferUsage::COPY_SRC)?;
        self.transfer.init_buffer(&slice, dst, 0, dst_offset, WHOLE_BUFFER);
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

    pub fn flush_transfers(&self) {
        self.transfer.flush();
    }

    pub fn free_completed_transfers(&self) {
        self.transfer.try_free_unused_buffers();
    }

    pub fn insert_texture_into_bindless_heap(&self, texture: &Arc<super::TextureView<B>>) -> Option<BindlessSlot<B>> {
        self.bindless_slot_allocator.get_slot(texture)
    }
}

impl<B: GPUBackend> Drop for Device<B> {
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
