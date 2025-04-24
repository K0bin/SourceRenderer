use std::{
    cell::{Ref, RefCell},
    hash::Hash,
};

use sourcerenderer_core::gpu;

use web_sys::{js_sys::Uint8Array, GpuBuffer, GpuBufferDescriptor, GpuDevice};

pub(crate) const PREFER_DISCARD_OVER_QUEUE_WRITE: bool = false;

pub struct WebGPUBuffer {
    device: GpuDevice,
    buffer: RefCell<GpuBuffer>,
    readback_buffer: Option<RefCell<GpuBuffer>>,
    descriptor: GpuBufferDescriptor,
    rust_memory: RefCell<Option<Box<[u8]>>>,
    retained_memory_limit: u64,
    mappable: bool,
    info: gpu::BufferInfo,
}

impl PartialEq for WebGPUBuffer {
    fn eq(&self, other: &Self) -> bool {
        self.buffer == other.buffer
    }
}

impl Eq for WebGPUBuffer {}

impl Hash for WebGPUBuffer {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let buffer = self.buffer.borrow();
        Self::handle_as_usize(&buffer).hash(state);
    }
}

impl WebGPUBuffer {
    pub(crate) fn new(
        device: &GpuDevice,
        info: &gpu::BufferInfo,
        mappable: bool,
        name: Option<&str>,
    ) -> Result<Self, ()> {
        // If usage contains MAP_WRITE, it must not contain any other usage flags besides COPY_SRC.
        // If usage contains MAP_READ, it must not contain any other usage flags besides COPY_DST.
        // Besides that map() is async and the buffer can not be used by the GPU while it is mapped.
        // Tons of fun to work around...

        let mut usage = 0u32;
        let mut retained_rust_memory_limit = 0u64;
        if info.usage.contains(gpu::BufferUsage::VERTEX) {
            usage |= web_sys::gpu_buffer_usage::VERTEX;
        }
        if info.usage.contains(gpu::BufferUsage::INDEX) {
            usage |= web_sys::gpu_buffer_usage::INDEX;
        }
        if info.usage.contains(gpu::BufferUsage::INDIRECT) {
            usage |= web_sys::gpu_buffer_usage::INDIRECT;
        }
        if info.usage.contains(gpu::BufferUsage::CONSTANT) {
            usage |= web_sys::gpu_buffer_usage::UNIFORM;
        }
        if info.usage.contains(gpu::BufferUsage::STORAGE) {
            usage |= web_sys::gpu_buffer_usage::STORAGE;
        }
        if info.usage.contains(gpu::BufferUsage::COPY_SRC) {
            usage |= web_sys::gpu_buffer_usage::COPY_SRC;
        }
        if info
            .usage
            .intersects(gpu::BufferUsage::COPY_DST | gpu::BufferUsage::INITIAL_COPY)
        {
            usage |= web_sys::gpu_buffer_usage::COPY_DST;
        }
        if info.usage.contains(gpu::BufferUsage::COPY_DST) {
            usage |= web_sys::gpu_buffer_usage::QUERY_RESOLVE;
        }
        if info.usage == gpu::BufferUsage::COPY_DST && mappable {
            usage = web_sys::gpu_buffer_usage::COPY_DST | web_sys::gpu_buffer_usage::MAP_READ;
        }
        if info.usage == gpu::BufferUsage::CONSTANT && mappable {
            // Allocating new Rust memory for every single map operation is too slow.
            retained_rust_memory_limit = 256;
        }
        if !info.usage.gpu_writable()
            && !mappable
            && !info.usage.contains(gpu::BufferUsage::INITIAL_COPY)
        {
            panic!(
                "The buffer is useless because it can neither be written on the CPU nor the GPU."
            );
        }
        if info.usage.gpu_writable() && !info.usage.gpu_readable() && !mappable {
            panic!("The buffer is useless because it can only be written on the GPU but the contents cannot be read anywhere.");
        }

        retained_rust_memory_limit = retained_rust_memory_limit.min(info.size);
        let retain_entire_buffer = retained_rust_memory_limit == info.size;
        if (usage & web_sys::gpu_buffer_usage::MAP_WRITE) == 0
            && mappable
            && (info.usage.gpu_writable()
                || !retain_entire_buffer
                || !PREFER_DISCARD_OVER_QUEUE_WRITE)
        {
            // GpuQueue::writeBuffer requires GpuUsage::COPY_DST
            usage |= web_sys::gpu_buffer_usage::COPY_DST;
        }
        let rust_memory = if retained_rust_memory_limit != 0 {
            let mut rust_memory_vec = Vec::with_capacity(info.size as usize);
            rust_memory_vec.resize(retained_rust_memory_limit as usize, 0);
            Some(rust_memory_vec.into_boxed_slice())
        } else {
            Option::<Box<[u8]>>::None
        };

        let descriptor = GpuBufferDescriptor::new(info.size as f64, usage);
        if let Some(name) = name {
            descriptor.set_label(name);
        }
        let mapped_at_creation = mappable
            && !info.usage.gpu_writable()
            && info.usage.contains(gpu::BufferUsage::INITIAL_COPY);
        assert!(!mapped_at_creation || info.size % 4 == 0);
        // Mapping at creation would mean we'd have to guarantee it gets unmapped to make it usable on the GPU which would involve lots of tracking.
        // We'll only do it for buffers with INITIAL_COPY and just assume those will get mapped & unmapped at least once before they get used on the GPU.
        descriptor.set_mapped_at_creation(false);
        let buffer = device.create_buffer(&descriptor).map_err(|e| {
            log::error!("Failed to create buffer: {:?}", e);
            ()
        })?;

        let readback_buffer = if info.usage.gpu_writable()
            && mappable
            && usage != (web_sys::gpu_buffer_usage::COPY_DST | web_sys::gpu_buffer_usage::MAP_READ)
        {
            // WebGPU does not allow USAGE_MAP_READ with anything except USAGE_COPY_DST.
            // So we have to keep a second buffer around and copy to that at the end of every command buffer.
            let readback_descriptor = GpuBufferDescriptor::new(
                info.size as f64,
                web_sys::gpu_buffer_usage::COPY_DST | web_sys::gpu_buffer_usage::MAP_READ,
            );
            if let Some(name) = name {
                readback_descriptor.set_label(&format!("{}_readback", name));
            }
            readback_descriptor.set_mapped_at_creation(true);
            Some(RefCell::new(
                device.create_buffer(&readback_descriptor).map_err(|e| {
                    log::error!("Failed to create buffer: {:?}", e);
                    ()
                })?,
            ))
        } else {
            None
        };

        Ok(Self {
            device: device.clone(),
            buffer: RefCell::new(buffer),
            readback_buffer,
            descriptor,
            rust_memory: RefCell::new(rust_memory),
            mappable,
            retained_memory_limit: retained_rust_memory_limit,
            info: info.clone(),
        })
    }

    #[inline(always)]
    pub(crate) fn handle(&self) -> Ref<GpuBuffer> {
        self.buffer.borrow()
    }

    #[inline(always)]
    pub(crate) fn readback_handle(&self) -> Option<Ref<GpuBuffer>> {
        self.readback_buffer.as_ref().map(|b| b.borrow())
    }

    #[inline(always)]
    pub(crate) fn is_mappable(&self) -> bool {
        self.mappable
    }

    #[inline(always)]
    pub(crate) fn handle_as_usize(handle: &GpuBuffer) -> usize {
        unsafe { std::mem::transmute(handle as *const GpuBuffer) }
    }
}

impl Drop for WebGPUBuffer {
    fn drop(&mut self) {
        let buffer = self.buffer.borrow();
        buffer.destroy();
    }
}

impl gpu::Buffer for WebGPUBuffer {
    fn info(&self) -> &gpu::BufferInfo {
        &self.info
    }

    unsafe fn map(
        &self,
        offset: u64,
        mut length: u64,
        invalidate: bool,
    ) -> Option<*mut std::ffi::c_void> {
        if !self.mappable {
            return None;
        }
        if !invalidate && !self.info.usage.gpu_readable() {
            log::warn!("Mapping a GPU-writeonly buffer (so probably mapping for reading) without invalidating will cause issues.");
        }

        length = length.min(self.info.size - offset);
        debug_assert!(offset + length <= self.info.size);

        let mut memory_opt: std::cell::RefMut<'_, Option<Box<[u8]>>> =
            self.rust_memory.borrow_mut();
        let retained_memory_size = if let Some(memory) = memory_opt.as_mut() {
            memory.len() as u64
        } else {
            0u64
        };
        if retained_memory_size < length {
            if cfg!(debug_assertions) {
                log::trace!("Creating new memory copy of buffer because current one is too small ({:?} bytes). Requested by map operation: {:?} bytes. Buffer size: {:?} bytes, buffer usage: {:?}", retained_memory_size, length, self.info.size, self.info.usage);
            }
            let mut memory_vec =
                Vec::<u8>::with_capacity(length.max(self.retained_memory_limit) as usize);
            unsafe {
                memory_vec.set_len(length.max(self.retained_memory_limit) as usize);
            }
            *memory_opt = Some(memory_vec.into_boxed_slice());
        }
        let memory = memory_opt.as_mut().unwrap();
        let entire_buffer_was_already_mapped = retained_memory_size == self.info.size;
        let entire_buffer_mapped = (memory.len() as u64) >= self.info.size;

        let memory_slice = if entire_buffer_mapped {
            &mut memory[offset as usize..offset as usize + length as usize]
        } else {
            &mut memory[..length as usize]
        };

        if invalidate {
            let mut use_readback_buffer = false;
            if let Some(readback_buffer) = self.readback_buffer.as_ref() {
                let buffer = readback_buffer.borrow_mut();
                if (&*buffer).map_state() == web_sys::GpuBufferMapState::Mapped {
                    let mapped_range = buffer.get_mapped_range().unwrap();
                    let uint8_array = Uint8Array::new_with_byte_offset_and_length(
                        &mapped_range,
                        offset as u32,
                        length as u32,
                    );
                    uint8_array.copy_to(memory_slice);
                    use_readback_buffer = true;
                } else if self.info.usage.gpu_writable() {
                    panic!("Cannot read back. Buffer either wasn't mapped after writing or is not ready yet.");
                }
            }

            if !use_readback_buffer {
                let buffer = self.buffer.borrow_mut();
                if (&*buffer).map_state() == web_sys::GpuBufferMapState::Mapped {
                    let mapped_range = buffer.get_mapped_range().unwrap();
                    let uint8_array = Uint8Array::new_with_byte_offset_and_length(
                        &mapped_range,
                        offset as u32,
                        length as u32,
                    );
                    uint8_array.copy_to(memory_slice);
                } else if !entire_buffer_was_already_mapped {
                    panic!(
                        "Cannot read back. Read only buffer was not entirely retained in memory."
                    );
                }
            }
        }

        Some(memory_slice.as_mut_ptr() as *mut std::ffi::c_void)
    }

    unsafe fn unmap(&self, offset: u64, mut length: u64, flush: bool) {
        let mut memory_opt: std::cell::RefMut<'_, Option<Box<[u8]>>> =
            self.rust_memory.borrow_mut();
        if memory_opt.is_none() {
            assert!(self.mappable);
            // Buffer wasn't mapped
            return;
        }
        if !flush && !self.info.usage.gpu_writable() {
            log::warn!("Mapping a GPU-readonly buffer (so probably mapped for writing) without flushing will cause issues.");
        }

        let retain_entire_buffer = self.retained_memory_limit == self.info.size;
        let memory = memory_opt.as_mut().unwrap();

        if flush {
            let mut buffer = self.buffer.borrow_mut();
            length = length.min(self.info.size - offset);
            assert!(offset + length <= self.info.size);
            assert!((memory.len() as u64) >= length);

            let entire_buffer_mapped = (memory.len() as u64) >= self.info.size;

            let memory_slice = if entire_buffer_mapped {
                &memory[offset as usize..offset as usize + length as usize]
            } else {
                &memory[..length as usize]
            };

            let map_directly = buffer.map_state() == web_sys::GpuBufferMapState::Mapped
                || ((PREFER_DISCARD_OVER_QUEUE_WRITE
                    || (buffer.usage() & web_sys::gpu_buffer_usage::COPY_DST) == 0)
                    && ((!self.info.usage.gpu_writable() && retain_entire_buffer) // the buffer can only be written on the CPU so the contents of the rust memory always mirror the buffer contents
                            || (offset == 0 && length == self.info.size))); // Replace the entire buffer with one that's mapped at creation. Map at creation can be set without USAGE_MAP_*.
            if map_directly {
                if buffer.map_state() != web_sys::GpuBufferMapState::Mapped {
                    // Create a new buffer that's mapped at creation
                    *buffer = self.device.create_buffer(&self.descriptor).unwrap();
                    if cfg!(debug_assertions) {
                        log::info!(
                            "Discarding buffer! Buffer size: {:?}, buffer usage: {:?}",
                            self.info.size,
                            self.info.usage
                        );
                    }
                } else {
                    if cfg!(debug_assertions) {
                        log::info!("Using directly mapped buffer without discard! Buffer size: {:?}, buffer usage: {:?}", self.info.size, self.info.usage);
                    }
                }
                assert!(buffer.map_state() == web_sys::GpuBufferMapState::Mapped);
                let mapped_range = buffer.get_mapped_range().unwrap();
                let uint8_array = Uint8Array::new_with_byte_offset_and_length(
                    &mapped_range,
                    offset as u32,
                    length as u32,
                );
                uint8_array.copy_from(memory_slice);
                buffer.unmap();
            } else {
                assert!((buffer.usage() & web_sys::gpu_buffer_usage::COPY_DST) != 0);
                self.device
                    .queue()
                    .write_buffer_with_u32_and_u8_slice(&buffer, offset as u32, memory_slice)
                    .unwrap();
            }
            if let Some(readback_buffer) = self.readback_buffer.as_ref() {
                if PREFER_DISCARD_OVER_QUEUE_WRITE && offset == 0 && length == self.info.size {
                    let mut readback_buffer_mut = readback_buffer.borrow_mut();
                    let readback_descriptor = GpuBufferDescriptor::new(
                        self.info.size as f64,
                        web_sys::gpu_buffer_usage::COPY_DST | web_sys::gpu_buffer_usage::MAP_READ,
                    );
                    readback_descriptor.set_label(&readback_buffer_mut.label());
                    readback_descriptor.set_mapped_at_creation(true);
                    *readback_buffer_mut = self
                        .device
                        .create_buffer(&readback_descriptor)
                        .map_err(|e| {
                            log::error!("Failed to create buffer: {:?}", e);
                            ()
                        })
                        .unwrap();
                    assert!(readback_buffer_mut.map_state() == web_sys::GpuBufferMapState::Mapped);
                    let mapped_range = readback_buffer_mut.get_mapped_range().unwrap();
                    let uint8_array = Uint8Array::new_with_byte_offset_and_length(
                        &mapped_range,
                        0,
                        self.info.size as u32,
                    );
                    uint8_array.copy_from(memory_slice);
                    readback_buffer_mut.unmap();
                } else {
                    let readback_buffer = readback_buffer.borrow();
                    self.device
                        .queue()
                        .write_buffer_with_u32_and_u8_slice(
                            &readback_buffer,
                            offset as u32,
                            memory_slice,
                        )
                        .unwrap();
                }
            }
        }
        if (memory.len() as u64) > self.retained_memory_limit {
            if cfg!(debug_assertions) {
                log::trace!("Removing memory copy of buffer ({:?} bytes) because it exceeds limit ({:?} bytes). Buffer size: {:?} bytes, buffer usage: {:?}", memory.len(), self.retained_memory_limit, self.info.size, self.info.usage);
            }
            // Free mapping copy
            *memory_opt = None;
        }
    }
}
