use std::{mem::ManuallyDrop, sync::{atomic::{AtomicBool, Ordering}, Arc}};
use crate::Mutex;

use log::trace;
use sourcerenderer_core::gpu::{self, Device as _};

use super::*;

pub struct Device<B: GPUBackend> {
    device: Arc<B::Device>,
    instance: Arc<Instance<B>>,
    allocator: ManuallyDrop<Arc<MemoryAllocator<B>>>,
    destroyer: ManuallyDrop<Arc<DeferredDestroyer<B>>>,
    buffer_allocator: ManuallyDrop<Arc<BufferAllocator<B>>>,
    bindless_slot_allocator: BindlessSlotAllocator,
    transfer: ManuallyDrop<Transfer<B>>,
    prerendered_frames: u32,
    has_context: AtomicBool,
    graphics_queue: Queue<B>,
    compute_queue: Option<Queue<B>>,
    transfer_queue: Option<Queue<B>>
}

impl<B: GPUBackend> Device<B> {
    pub fn new(device: B::Device, instance: Arc<Instance<B>>) -> Self {
        let device = Arc::new(device);
        let memory_allocator = ManuallyDrop::new(Arc::new(MemoryAllocator::new(&device)));
        let destroyer = ManuallyDrop::new(Arc::new(DeferredDestroyer::new()));
        let buffer_allocator = Arc::new(BufferAllocator::new(&device, &memory_allocator));

        let prerendered_frames = if cfg!(not(target_arch = "wasm32")) {
            3
        } else {
            1 // WebGPU handles synchronization completely.
        };

        Self {
            device: device.clone(),
            instance: instance,
            allocator: memory_allocator.clone(),
            destroyer: destroyer.clone(),
            bindless_slot_allocator: BindlessSlotAllocator::new(BINDLESS_TEXTURE_COUNT),
            transfer: ManuallyDrop::new(Transfer::new(&device, &destroyer, &buffer_allocator)),
            buffer_allocator: ManuallyDrop::new(buffer_allocator),
            prerendered_frames,
            has_context: AtomicBool::new(false),
            graphics_queue: Queue::new(QueueType::Graphics),
            compute_queue: device.compute_queue().map(|_| Queue::new(QueueType::Compute)),
            transfer_queue: device.compute_queue().map(|_| Queue::new(QueueType::Transfer)),
        }
    }

    #[inline(always)]
    pub fn handle(&self) -> &Arc<B::Device> {
        &self.device
    }

    #[inline(always)]
    pub fn instance(&self) -> &Arc<Instance<B>> {
        &self.instance
    }

    #[inline(always)]
    pub(super) fn destroyer(&self) -> &Arc<DeferredDestroyer<B>> {
        &self.destroyer
    }

    #[inline(always)]
    pub fn create_context(&self) -> GraphicsContext<B> {
        trace!("Creating graphics context");
        assert!(!self.has_context.swap(true, Ordering::AcqRel));
        GraphicsContext::new(&self.device, &self.allocator, &self.buffer_allocator, &self.destroyer, self.prerendered_frames)
    }

    #[inline(always)]
    pub fn create_texture(&self, info: &TextureInfo, name: Option<&str>) -> Result<Arc<super::Texture<B>>, OutOfMemoryError> {
        super::Texture::new(&self.device, &self.allocator, &self.destroyer, info, name)
    }

    #[inline(always)]
    pub fn create_texture_view(&self, texture: &Arc<super::Texture<B>>, info: &TextureViewInfo, name: Option<&str>) -> Arc<super::TextureView<B>> {
        super::TextureView::new(&self.device, &self.destroyer, texture, info, name)
    }

    #[inline(always)]
    pub fn create_buffer(&self, info: &BufferInfo, memory_usage: MemoryUsage, name: Option<&str>) -> Result<Arc<BufferSlice<B>>, OutOfMemoryError> {
        self.buffer_allocator.get_slice(info, memory_usage, name)
    }

    #[inline(always)]
    pub fn create_fence(&self) -> super::Fence<B> {
        super::Fence::new(self.device.as_ref(), &self.destroyer)
    }

    #[inline(always)]
    pub fn create_sampler(&self, info: &SamplerInfo) -> super::Sampler<B> {
        super::Sampler::new(&self.device, &self.destroyer, info)
    }

    #[inline(always)]
    pub fn create_shader(&self, shader: &PackedShader, name: Option<&str>) -> B::Shader {
        unsafe { self.device.create_shader(shader, name) }
    }

    #[inline(always)]
    pub fn create_graphics_pipeline(&self, info: &GraphicsPipelineInfo<B>, name: Option<&str>) -> Arc<super::GraphicsPipeline<B>> {
        Arc::new(super::GraphicsPipeline::new(&self.device, &self.destroyer, info, name))
    }

    #[inline(always)]
    pub fn create_compute_pipeline(&self, shader: &B::Shader, name: Option<&str>) -> Arc<super::ComputePipeline<B>> {
        Arc::new(super::ComputePipeline::new(&self.device, &self.destroyer, shader, name))
    }

    #[inline(always)]
    pub fn create_raytracing_pipeline(&self, info: &RayTracingPipelineInfo<B>, name: Option<&str>) -> Result<Arc<super::RayTracingPipeline<B>>, OutOfMemoryError> {
        let pipeline = super::RayTracingPipeline::new(&self.device, &self.destroyer, &self.buffer_allocator, info, name)?;
        Ok(Arc::new(pipeline))
    }

    pub fn upload_data<T>(&self, data: &[T], memory_usage: MemoryUsage, usage: BufferUsage) -> Result<Arc<BufferSlice<B>>, OutOfMemoryError> {
        let required_size = std::mem::size_of_val(data) as u64;
        let size = align_up_64(required_size.max(64), 64);

        let slice = self.buffer_allocator.get_slice(&BufferInfo {
            size,
            usage,
            sharing_mode: QueueSharingMode::Concurrent
        }, memory_usage, None)?;

        unsafe {
            let ptr_void = slice.map(false).unwrap();

            if required_size < size {
                let ptr_u8 = (ptr_void as *mut u8).offset(required_size as isize);
                std::ptr::write_bytes(ptr_u8, 0u8, (size - required_size) as usize);
            }

            if required_size != 0 {
                let ptr = ptr_void as *mut T;
                ptr.copy_from(data.as_ptr(), data.len());
            }
            slice.unmap(true);
        }
        Ok(slice)
    }

    pub fn init_buffer<T>(&self, data: &[T], dst: &Arc<BufferSlice<B>>, dst_offset: u64) -> Result<(), OutOfMemoryError> {
        let data_u8 = into_bytes(data);
        self.transfer.init_buffer(data_u8, dst, dst_offset)?;
        Ok(())
    }

    pub fn init_buffer_box<T>(&self, data: Box<[T]>, dst: &Arc<BufferSlice<B>>, dst_offset: u64) -> Result<(), OutOfMemoryError> {
        let data_u8 = into_bytes_box(data);
        self.transfer.init_buffer_box(data_u8, dst, dst_offset)?;
        Ok(())
    }

    pub fn init_texture_box<T>(
        &self,
        data: Box<[T]>,
        dst: &Arc<super::Texture<B>>,
        mip_level: u32,
        array_layer: u32) -> Result<(), OutOfMemoryError> {
        let data_u8 = into_bytes_box(data);
        let _ = self.transfer.init_texture_box(data_u8, dst, mip_level, array_layer, false)?;
        Ok(())
    }

    pub fn init_texture<T>(
        &self,
        data: &[T],
        dst: &Arc<super::Texture<B>>,
        mip_level: u32,
        array_layer: u32) -> Result<(), OutOfMemoryError> {
        let data_u8 = into_bytes(data);
        let _ = self.transfer.init_texture(data_u8, dst, mip_level, array_layer, false)?;
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
        self.transfer.init_texture_from_buffer(dst, src, mip_level, array_layer, buffer_offset);
    }

    pub fn init_texture_async<T>(
        &self,
        data: &[T],
        dst: &Arc<super::Texture<B>>,
        mip_level: u32,
        array_layer: u32) -> Result<Option<SharedFenceValuePair<B>>, OutOfMemoryError> {
        let data_u8 = into_bytes(data);
        self.transfer.init_texture(&data_u8, dst, mip_level, array_layer, true)
    }

    pub fn init_texture_box_async<T>(
        &self,
        data: Box<[T]>,
        dst: &Arc<super::Texture<B>>,
        mip_level: u32,
        array_layer: u32) -> Result<Option<SharedFenceValuePair<B>>, OutOfMemoryError> {
        let data_u8 = into_bytes_box(data);
        self.transfer.init_texture_box(data_u8, dst, mip_level, array_layer, true)
    }

    pub fn init_texture_from_buffer_async<T>(
        &self,
        dst: &Arc<super::Texture<B>>,
        src: &Arc<BufferSlice<B>>,
        mip_level: u32,
        array_layer: u32,
        buffer_offset: u64
    ) -> Option<SharedFenceValuePair<B>> {
        self.transfer.init_texture_from_buffer_async(dst, src, mip_level, array_layer, buffer_offset)
    }

    #[inline(always)]
    pub fn flush_transfers(&self) {
        self.transfer.flush();
    }

    #[inline(always)]
    pub fn free_completed_transfers(&self) {
        self.transfer.try_free_unused_buffers();
    }

    pub fn insert_texture_into_bindless_heap(&self, texture: &Arc<super::TextureView<B>>) -> Option<BindlessSlot<B>> {
        if !self.supports_bindless() {
            return None;
        }
        let slot = self.bindless_slot_allocator.get_slot(texture);
        if let Some(slot) = slot.as_ref() {
            unsafe {
                self.device.insert_texture_into_bindless_heap(slot.slot(), slot.texture_view().handle());
            }
        }

        slot
    }

    #[inline(always)]
    pub fn supports_indirect(&self) -> bool {
        self.device.supports_indirect()
    }

    #[inline(always)]
    pub fn supports_bindless(&self) -> bool {
        self.device.supports_bindless()
    }

    #[inline(always)]
    pub fn supports_barycentrics(&self) -> bool {
        self.device.supports_barycentrics()
    }

    #[inline(always)]
    pub fn supports_ray_tracing(&self) -> bool {
        self.device.supports_ray_tracing()
    }

    #[inline(always)]
    pub fn supports_min_max_filter(&self) -> bool {
        self.device.supports_min_max_filter()
    }

    pub fn wait_for_idle(&self) {
        self.flush_transfers();
        self.graphics_queue.flush(self.device.graphics_queue());
        self.graphics_queue.wait_for_idle();
        if let Some(queue) = self.compute_queue.as_ref() {
            queue.flush(self.device.compute_queue().unwrap());
            queue.wait_for_idle();
        }
        if let Some(queue) = self.transfer_queue.as_ref() {
            queue.flush(self.device.transfer_queue().unwrap());
            queue.wait_for_idle();
        }

        unsafe {
            self.device.wait_for_idle();
        }
    }

    pub fn submit(&self, queue_type: QueueType, submission: QueueSubmission<B>) {
        let virtual_queue_opt = match queue_type {
            QueueType::Graphics => Some(&self.graphics_queue),
            QueueType::Compute => self.compute_queue.as_ref(),
            QueueType::Transfer => self.transfer_queue.as_ref()
        };

        let virtual_queue = virtual_queue_opt.expect("Device does not support requested queue type.");
        virtual_queue.submit(submission);
    }

    pub fn present(&self, queue_type: QueueType, swapchain: &Arc<Mutex<Swapchain<B>>>, backbuffer: Arc<<B::Swapchain as gpu::Swapchain<B>>::Backbuffer>) {
        let virtual_queue_opt: Option<&Queue<B>> = match queue_type {
            QueueType::Graphics => Some(&self.graphics_queue),
            QueueType::Compute => self.compute_queue.as_ref(),
            QueueType::Transfer => self.transfer_queue.as_ref()
        };

        let virtual_queue = virtual_queue_opt.expect("Device does not support requested queue type.");
        virtual_queue.present(swapchain, backbuffer);
    }

    pub fn flush_all(&self) {
        self.flush(QueueType::Graphics);
        self.flush(QueueType::Compute);
        self.flush(QueueType::Transfer);
    }

    pub fn flush(&self, queue_type: QueueType) {
        self.flush_transfers();

        let (virtual_queue_opt, queue_opt) = match queue_type {
            QueueType::Graphics => (Some(&self.graphics_queue), Some(self.device.graphics_queue())),
            QueueType::Compute => (self.compute_queue.as_ref(), self.device.compute_queue()),
            QueueType::Transfer => (self.transfer_queue.as_ref(), self.device.transfer_queue())
        };

        if virtual_queue_opt.is_none() || queue_opt.is_none() {
            return;
        }

        let virtual_queue = virtual_queue_opt.unwrap();
        let queue = queue_opt.unwrap();

        virtual_queue.flush(queue);
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

#[inline(always)]
fn into_bytes<'a, T>(data: &'a [T]) -> &'a [u8] {
    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * std::mem::size_of::<T>()) }
}

#[inline]
fn into_bytes_box<T>(data: Box<[T]>) -> Box<[u8]> {
    let data_vec = data.into_vec();
    let len = data_vec.len();
    let capacity = data_vec.capacity();
    let ptr = data_vec.as_ptr();
    std::mem::forget(data_vec);
    let data_vec_u8 = unsafe { Vec::from_raw_parts(ptr as *mut u8, len * std::mem::size_of::<T>(), capacity * std::mem::size_of::<T>()) };
    data_vec_u8.into_boxed_slice()
}
