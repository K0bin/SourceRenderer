use super::*;
use crate::Mutex;
use bytemuck::{BoxBytes, Pod, box_bytes_of, cast_slice};
use std::mem::ManuallyDrop;
use std::sync::Arc;

pub struct Device {
    device: Arc<active_gpu_backend::Device>,
    instance: Arc<Instance>,
    allocator: ManuallyDrop<Arc<MemoryAllocator>>,
    destroyer: ManuallyDrop<Arc<DeferredDestroyer>>,
    buffer_allocator: ManuallyDrop<Arc<BufferAllocator>>,
    bindless_slot_allocator: BindlessSlotAllocator,
    transfer: ManuallyDrop<Transfer>,
    prerendered_frames: u32,
    graphics_queue: Mutex<Queue>,
    compute_queue: Option<Mutex<Queue>>,
    transfer_queue: Option<Mutex<Queue>>,
}

impl Device {
    pub fn new(device: active_gpu_backend::Device, instance: Arc<Instance>) -> Self {
        let device = Arc::new(device);
        let memory_allocator = ManuallyDrop::new(Arc::new(MemoryAllocator::new(&device)));
        let destroyer = ManuallyDrop::new(Arc::new(DeferredDestroyer::new()));
        let buffer_allocator =
            Arc::new(BufferAllocator::new(&device, &memory_allocator, &destroyer));

        let prerendered_frames = if cfg!(not(target_arch = "wasm32")) {
            3
        } else {
            // WebGPU handles synchronization completely.
            // WebGPU also prevents us from writing to buffers that are in-flight because we
            // have to use GpuQueue.writeBuffer due to async and WebGPU buffer usage rules.
            // GpuQueue.writeBuffer happens on the GPU timeline so it only impacts work submitted after it.
            1
        };

        let graphics_queue = {
            let fence = Fence::new(&device, &destroyer);
            Mutex::new(Queue::new(&destroyer, QueueType::Graphics, fence))
        };
        let compute_queue = device.compute_queue().map(|_| {
            let fence = Fence::new(&device, &destroyer);
            Mutex::new(Queue::new(&destroyer, QueueType::Compute, fence))
        });
        let transfer_queue = device.transfer_queue().map(|_| {
            let fence = Fence::new(&device, &destroyer);
            Mutex::new(Queue::new(&destroyer, QueueType::Transfer, fence))
        });

        Self {
            device: device.clone(),
            instance,
            allocator: memory_allocator.clone(),
            destroyer: destroyer.clone(),
            bindless_slot_allocator: BindlessSlotAllocator::new(BINDLESS_TEXTURE_COUNT),
            transfer: ManuallyDrop::new(Transfer::new(&device, &buffer_allocator)),
            buffer_allocator: ManuallyDrop::new(buffer_allocator),
            prerendered_frames,
            graphics_queue,
            compute_queue,
            transfer_queue,
        }
    }

    #[inline(always)]
    pub fn handle(&self) -> &Arc<active_gpu_backend::Device> {
        &self.device
    }

    #[inline(always)]
    pub fn instance(&self) -> &Arc<Instance> {
        &self.instance
    }

    #[inline(always)]
    pub(super) fn destroyer(&self) -> &Arc<DeferredDestroyer> {
        &self.destroyer
    }

    #[inline(always)]
    pub fn create_context(self: &Arc<Self>) -> GraphicsContext {
        log::trace!("Creating graphics context");
        GraphicsContext::new(
            self,
            &self.allocator,
            &self.buffer_allocator,
            &self.destroyer,
            self.prerendered_frames,
        )
    }

    #[inline(always)]
    pub fn create_texture(
        &self,
        info: &TextureInfo,
        name: Option<&str>,
    ) -> Result<Arc<super::Texture>, OutOfMemoryError> {
        super::Texture::new(&self.device, &self.allocator, &self.destroyer, info, name)
    }

    #[inline(always)]
    pub fn create_texture_view(
        &self,
        texture: &Arc<super::Texture>,
        info: &TextureViewInfo,
        name: Option<&str>,
    ) -> Arc<super::TextureView> {
        super::TextureView::new(&self.device, &self.destroyer, texture, info, name)
    }

    #[inline(always)]
    pub fn create_buffer(
        &self,
        info: &BufferInfo,
        memory_usage: MemoryUsage,
        name: Option<&str>,
    ) -> Result<Arc<BufferSlice>, OutOfMemoryError> {
        self.buffer_allocator.get_slice(info, memory_usage, name)
    }

    #[inline(always)]
    pub fn create_fence(&self) -> super::Fence {
        super::Fence::new(self.device.as_ref(), &self.destroyer)
    }

    #[inline(always)]
    pub fn create_sampler(&self, info: &SamplerInfo, name: Option<&str>) -> super::Sampler {
        super::Sampler::new(&self.device, &self.destroyer, info, name)
    }

    #[inline(always)]
    pub fn create_shader(
        &self,
        shader: &PackedShader,
        name: Option<&str>,
    ) -> active_gpu_backend::Shader {
        self.device.create_shader(shader, name)
    }

    #[inline(always)]
    pub fn create_graphics_pipeline(
        &self,
        info: &active_gpu_backend::GraphicsPipelineInfo,
        name: Option<&str>,
    ) -> Arc<super::GraphicsPipeline> {
        Arc::new(super::GraphicsPipeline::new(
            &self.device,
            &self.destroyer,
            info,
            name,
        ))
    }

    #[inline(always)]
    pub fn create_mesh_graphics_pipeline(
        &self,
        info: &active_gpu_backend::MeshGraphicsPipelineInfo,
        name: Option<&str>,
    ) -> Arc<super::MeshGraphicsPipeline> {
        Arc::new(super::MeshGraphicsPipeline::new(
            &self.device,
            &self.destroyer,
            info,
            name,
        ))
    }

    #[inline(always)]
    pub fn create_compute_pipeline(
        &self,
        shader: PipelineShaderStage,
        name: Option<&str>,
    ) -> Arc<super::ComputePipeline> {
        Arc::new(super::ComputePipeline::new(
            &self.device,
            &self.destroyer,
            shader,
            name,
        ))
    }

    #[inline(always)]
    pub fn create_raytracing_pipeline(
        &self,
        info: &active_gpu_backend::RayTracingPipelineInfo,
        name: Option<&str>,
    ) -> Result<Arc<super::RayTracingPipeline>, OutOfMemoryError> {
        let pipeline = super::RayTracingPipeline::new(
            &self.device,
            &self.destroyer,
            &self.buffer_allocator,
            info,
            name,
        )?;
        Ok(Arc::new(pipeline))
    }

    pub fn upload_data<T: Pod>(
        &self,
        data: &[T],
        memory_usage: MemoryUsage,
        usage: BufferUsage,
    ) -> Result<Arc<BufferSlice>, OutOfMemoryError> {
        let required_size = std::mem::size_of_val(data);
        let size = align_up(required_size.max(64), 64);

        let slice = self.buffer_allocator.get_slice(
            &BufferInfo {
                size: size as u64,
                usage,
                sharing_mode: QueueSharingMode::Concurrent,
            },
            memory_usage,
            None,
        )?;

        slice.write(self.handle(), data, 0);
        Ok(slice)
    }

    pub fn init_buffer<T: Pod>(
        &self,
        data: &[T],
        dst: &Arc<BufferSlice>,
        dst_offset: u64,
    ) -> Result<(), OutOfMemoryError> {
        let data_u8: &[u8] = cast_slice(data);
        self.transfer.init_buffer(data_u8, dst, dst_offset)?;
        Ok(())
    }

    pub fn init_buffer_box<T: Pod>(
        &self,
        data: Box<[T]>,
        dst: &Arc<BufferSlice>,
        dst_offset: u64,
    ) -> Result<(), OutOfMemoryError> {
        let data_u8: BoxBytes = box_bytes_of(data);
        self.transfer.init_buffer_box(data_u8, dst, dst_offset)?;
        Ok(())
    }

    pub fn init_texture_box<T: Pod>(
        &self,
        data: Box<[T]>,
        dst: &Arc<super::Texture>,
        mip_level: u32,
        array_layer: u32,
    ) -> Result<(), OutOfMemoryError> {
        let data_u8: BoxBytes = box_bytes_of(data);
        let _ =
            self.transfer
                .init_texture_box(self, data_u8, dst, mip_level, array_layer, false)?;
        Ok(())
    }

    pub fn init_texture<T: Pod>(
        &self,
        data: &[T],
        dst: &Arc<super::Texture>,
        mip_level: u32,
        array_layer: u32,
    ) -> Result<(), OutOfMemoryError> {
        let data_u8: &[u8] = cast_slice(data);
        let _ = self
            .transfer
            .init_texture(self, data_u8, dst, mip_level, array_layer, false)?;
        Ok(())
    }

    pub fn init_texture_from_buffer(
        &self,
        dst: &Arc<super::Texture>,
        src: &Arc<BufferSlice>,
        mip_level: u32,
        array_layer: u32,
        buffer_offset: u64,
    ) {
        self.transfer
            .init_texture_from_buffer(dst, src, mip_level, array_layer, buffer_offset);
    }

    pub fn init_texture_async<T: Pod>(
        &self,
        data: &[T],
        dst: &Arc<super::Texture>,
        mip_level: u32,
        array_layer: u32,
    ) -> Result<Option<QueueFenceValue>, OutOfMemoryError> {
        let data_u8: &[u8] = cast_slice(data);
        self.transfer
            .init_texture(self, &data_u8, dst, mip_level, array_layer, true)
    }

    pub fn init_texture_box_async<T: Pod>(
        &self,
        data: Box<[T]>,
        dst: &Arc<super::Texture>,
        mip_level: u32,
        array_layer: u32,
    ) -> Result<Option<QueueFenceValue>, OutOfMemoryError> {
        let data_u8: BoxBytes = box_bytes_of(data);
        self.transfer
            .init_texture_box(self, data_u8, dst, mip_level, array_layer, true)
    }

    pub fn init_texture_from_buffer_async(
        &self,
        dst: &Arc<super::Texture>,
        src: &Arc<BufferSlice>,
        mip_level: u32,
        array_layer: u32,
        buffer_offset: u64,
    ) -> Option<QueueFenceValue> {
        self.transfer.init_texture_from_buffer_async(
            &self,
            dst,
            src,
            mip_level,
            array_layer,
            buffer_offset,
        )
    }

    #[inline(always)]
    pub fn submit_transfers(&self) {
        self.transfer.submit(self);
    }

    #[inline(always)]
    pub fn free_completed_transfers(&self) {
        self.transfer.try_free_unused_buffers(&self);
    }

    pub fn insert_texture_into_bindless_heap(
        &self,
        texture: &Arc<super::TextureView>,
    ) -> Option<BindlessSlot> {
        if !self.supports_bindless() {
            return None;
        }
        let slot = self.bindless_slot_allocator.get_slot(texture);
        if let Some(slot) = slot.as_ref() {
            unsafe {
                self.device
                    .insert_texture_into_bindless_heap(slot.slot(), slot.texture_view().handle());
            }
        }

        slot
    }

    #[inline(always)]
    pub fn supports_indirect_first_instance(&self) -> bool {
        self.device.supports_indirect_first_instance()
    }

    #[inline(always)]
    pub fn supports_indirect_count(&self) -> bool {
        self.device.supports_indirect_count()
    }

    #[inline(always)]
    pub fn supports_indirect_count_mesh_shader(&self) -> bool {
        self.device.supports_indirect_count_mesh_shader()
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
    pub fn supports_ray_tracing_pipeline(&self) -> bool {
        self.device.supports_ray_tracing_pipeline()
    }

    #[inline(always)]
    pub fn supports_ray_tracing_query(&self) -> bool {
        self.device.supports_ray_tracing_query()
    }

    #[inline(always)]
    pub fn supports_min_max_filter(&self) -> bool {
        self.device.supports_min_max_filter()
    }

    pub fn block_until_idle(&self) {
        log::warn!("Block until idle.");
        self.flush();
        self.graphics_queue.lock().unwrap().wait_for_idle();
        if let Some(queue) = self.compute_queue.as_ref() {
            queue.lock().unwrap().wait_for_idle();
        }
        if let Some(queue) = self.transfer_queue.as_ref() {
            queue.lock().unwrap().wait_for_idle();
        }

        unsafe {
            self.device.block_until_idle();
        }
    }

    pub fn queue_next_counter(&self, queue_type: QueueType) -> u64 {
        let queue = match queue_type {
            QueueType::Graphics => &self.graphics_queue,
            QueueType::Compute => self
                .compute_queue
                .as_ref()
                .expect("Device does not support requested queue type."),
            QueueType::Transfer => self
                .transfer_queue
                .as_ref()
                .expect("Device does not support requested queue type."),
        };
        queue.lock().unwrap().next_counter()
    }

    pub fn await_queue_counter(&self, queue_type: QueueType, value: u64) {
        let queue = match queue_type {
            QueueType::Graphics => &self.graphics_queue,
            QueueType::Compute => self
                .compute_queue
                .as_ref()
                .expect("Device does not support requested queue type."),
            QueueType::Transfer => self
                .transfer_queue
                .as_ref()
                .expect("Device does not support requested queue type."),
        };
        queue.lock().unwrap().await_counter(value);
    }

    pub fn submitted_queue_counter(&self, queue_type: QueueType) -> u64 {
        let queue = match queue_type {
            QueueType::Graphics => &self.graphics_queue,
            QueueType::Compute => self
                .compute_queue
                .as_ref()
                .expect("Device does not support requested queue type."),
            QueueType::Transfer => self
                .transfer_queue
                .as_ref()
                .expect("Device does not support requested queue type."),
        };
        queue.lock().unwrap().submitted_counter()
    }

    pub fn completed_queue_counter(&self, queue_type: QueueType) -> u64 {
        let queue = match queue_type {
            QueueType::Graphics => &self.graphics_queue,
            QueueType::Compute => self
                .compute_queue
                .as_ref()
                .expect("Device does not support requested queue type."),
            QueueType::Transfer => self
                .transfer_queue
                .as_ref()
                .expect("Device does not support requested queue type."),
        };
        queue.lock().unwrap().completed_counter()
    }

    pub fn wait_for(
        &self,
        queue_type: QueueType,
        wait_for_queue: QueueType,
        counter: u64,
        wait_before: BarrierSync,
    ) {
        if queue_type == wait_for_queue {
            panic!("Cannot wait for same queue.");
        }

        let wait_for_queue_lock = match wait_for_queue {
            QueueType::Graphics => self.graphics_queue.lock().unwrap(),
            QueueType::Compute => self
                .compute_queue
                .as_ref()
                .expect("Cannot wait for compute queue, queue is not supported.")
                .lock()
                .unwrap(),
            QueueType::Transfer => self
                .transfer_queue
                .as_ref()
                .expect("Cannot wait for transfer queue, queue is not supported.")
                .lock()
                .unwrap(),
        };
        let fence = wait_for_queue_lock.fence();

        let mut queue = match queue_type {
            QueueType::Graphics => self.graphics_queue.lock().unwrap(),
            QueueType::Compute => self
                .compute_queue
                .as_ref()
                .expect("Device does not support requested queue type.")
                .lock()
                .unwrap(),
            QueueType::Transfer => self
                .transfer_queue
                .as_ref()
                .expect("Device does not support requested queue type.")
                .lock()
                .unwrap(),
        };

        queue.wait_for(fence, counter, wait_before);
    }

    pub fn submit(&self, queue_type: QueueType, cmd_buffer: FinishedCommandBuffer) -> u64 {
        let mut queue = match queue_type {
            QueueType::Graphics => self.graphics_queue.lock().unwrap(),
            QueueType::Compute => self
                .compute_queue
                .as_ref()
                .expect("Device does not support requested queue type.")
                .lock()
                .unwrap(),
            QueueType::Transfer => self
                .transfer_queue
                .as_ref()
                .expect("Device does not support requested queue type.")
                .lock()
                .unwrap(),
        };
        queue.submit(cmd_buffer);
        queue.next_counter()
    }

    pub fn submit_counter_bump(&self, queue_type: QueueType) -> u64 {
        let queue_opt = match queue_type {
            QueueType::Graphics => Some(&self.graphics_queue),
            QueueType::Compute => self.compute_queue.as_ref(),
            QueueType::Transfer => self.transfer_queue.as_ref(),
        };

        if let Some(queue) = queue_opt {
            queue.lock().unwrap().submit_counter_bump()
        } else {
            0u64
        }
    }

    pub fn present(
        &self,
        queue_type: QueueType,
        swapchain: &Arc<Mutex<Swapchain>>,
        backbuffer: Arc<active_gpu_backend::Backbuffer>,
    ) {
        self.flush();

        let (queue_opt, api_queue_opt) = match queue_type {
            QueueType::Graphics => (
                Some(&self.graphics_queue),
                Some(self.device.graphics_queue()),
            ),
            QueueType::Compute => (self.compute_queue.as_ref(), self.device.compute_queue()),
            QueueType::Transfer => (self.transfer_queue.as_ref(), self.device.transfer_queue()),
        };

        if queue_opt.is_none() || api_queue_opt.is_none() {
            panic!("Device does not support requested queue type.");
        }

        let api_queue = api_queue_opt.unwrap();
        let mut queue = queue_opt.unwrap().lock().unwrap();
        queue.present(swapchain, backbuffer, api_queue);
    }

    pub fn wait_for_backbuffer(
        &self,
        queue_type: QueueType,
        swapchain: &Arc<Mutex<Swapchain>>,
        backbuffer: &Arc<active_gpu_backend::Backbuffer>,
    ) {
        let queue_opt = match queue_type {
            QueueType::Graphics => Some(&self.graphics_queue),
            QueueType::Compute => self.compute_queue.as_ref(),
            QueueType::Transfer => self.transfer_queue.as_ref(),
        };

        if queue_opt.is_none() {
            panic!("Device does not support requested queue type.");
        }

        let mut queue = queue_opt.unwrap().lock().unwrap();
        queue.acquire_swapchain(swapchain, backbuffer);
    }

    pub fn signal_backbuffer(
        &self,
        queue_type: QueueType,
        swapchain: &Arc<Mutex<Swapchain>>,
        backbuffer: &Arc<active_gpu_backend::Backbuffer>,
    ) {
        let queue_opt = match queue_type {
            QueueType::Graphics => Some(&self.graphics_queue),
            QueueType::Compute => self.compute_queue.as_ref(),
            QueueType::Transfer => self.transfer_queue.as_ref(),
        };

        if queue_opt.is_none() {
            panic!("Device does not support requested queue type.");
        }

        let mut queue = queue_opt.unwrap().lock().unwrap();
        queue.release_swapchain(swapchain, backbuffer);
    }

    pub fn flush(&self) -> u64 {
        self.transfer.submit(self);

        let graphics_counter = self.flush_queue(QueueType::Graphics);
        let compute_counter = self.flush_queue(QueueType::Compute);
        let transfer_counter = self.flush_queue(QueueType::Transfer);

        graphics_counter.max(compute_counter.max(transfer_counter))
    }

    fn flush_queue(&self, queue_type: QueueType) -> u64 {
        let (queue_opt, api_queue_opt) = match queue_type {
            QueueType::Graphics => (
                Some(&self.graphics_queue),
                Some(self.device.graphics_queue()),
            ),
            QueueType::Compute => (self.compute_queue.as_ref(), self.device.compute_queue()),
            QueueType::Transfer => (self.transfer_queue.as_ref(), self.device.transfer_queue()),
        };

        if queue_opt.is_none() || api_queue_opt.is_none() {
            return 0u64;
        }

        let api_queue = api_queue_opt.unwrap();
        let mut queue = queue_opt.unwrap().lock().unwrap();

        queue.submit_counter_bump();
        queue.flush(api_queue)
    }

    pub fn has_queue(&self, queue_type: QueueType) -> bool {
        match queue_type {
            QueueType::Graphics => true,
            QueueType::Compute => self.device.compute_queue().is_some(),
            QueueType::Transfer => self.device.transfer_queue().is_some(),
        }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.transfer);
            self.device.block_until_idle();
            ManuallyDrop::drop(&mut self.buffer_allocator);
            ManuallyDrop::drop(&mut self.allocator);
            self.destroyer.destroy_all();
            ManuallyDrop::drop(&mut self.destroyer);
        }
    }
}
