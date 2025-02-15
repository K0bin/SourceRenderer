use std::{cell::{Ref, RefCell}, hash::Hash, sync::atomic::AtomicBool};

use log::{error, warn};
use sourcerenderer_core::gpu::{Buffer, BufferInfo, BufferUsage};

use web_sys::{js_sys::Uint8Array, GpuBuffer, GpuBufferDescriptor, GpuDevice};

const PREFER_DISCARD_OVER_QUEUE_WRITE: bool = false;

pub struct WebGPUBuffer {
    device: GpuDevice,
    buffer: RefCell<GpuBuffer>,
    descriptor: GpuBufferDescriptor,
    rust_memory: RefCell<Option<Box<[u8]>>>,
    mappable: bool,
    keep_rust_memory: bool,
    info: BufferInfo,
    mapped: AtomicBool
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
        let ptr_val: usize = unsafe { std::mem::transmute(buffer.as_ref() as *const GpuBuffer) };
        ptr_val.hash(state);
    }
}

unsafe impl Send for WebGPUBuffer {}
unsafe impl Sync for WebGPUBuffer {}

impl WebGPUBuffer {
    pub fn new(device: &GpuDevice, info: &BufferInfo, mappable: bool, name: Option<&str>) -> Result<Self, ()> {
        // If usage contains MAP_WRITE, it must not contain any other usage flags besides COPY_SRC.
        // If usage contains MAP_READ, it must not contain any other usage flags besides COPY_DST.
        // Besides that map() is async and the buffer can not be used by the GPU while it is mapped.
        // Tons of fun to work around...

        let mut usage = 0u32;
        let mut keep_rust_memory = false;
        if info.usage.contains(BufferUsage::VERTEX) {
            usage |= web_sys::gpu_buffer_usage::VERTEX;
        }
        if info.usage.contains(BufferUsage::INDEX) {
            usage |= web_sys::gpu_buffer_usage::INDEX;
        }
        if info.usage.contains(BufferUsage::INDIRECT) {
            usage |= web_sys::gpu_buffer_usage::INDIRECT;
        }
        if info.usage.contains(BufferUsage::CONSTANT) {
            usage |= web_sys::gpu_buffer_usage::UNIFORM;
        }
        if info.usage.contains(BufferUsage::STORAGE) {
            usage |= web_sys::gpu_buffer_usage::STORAGE;
        }
        if info.usage.contains(BufferUsage::COPY_SRC) {
            usage |= web_sys::gpu_buffer_usage::COPY_SRC;
        }
        if info.usage.intersects(BufferUsage::COPY_DST | BufferUsage::INITIAL_COPY) {
            usage |= web_sys::gpu_buffer_usage::COPY_DST;
        }
        if info.usage == BufferUsage::COPY_DST && mappable {
            usage = web_sys::gpu_buffer_usage::COPY_DST | web_sys::gpu_buffer_usage::MAP_READ;
        }
        if info.usage == BufferUsage::COPY_SRC && mappable {
            usage = web_sys::gpu_buffer_usage::COPY_SRC | web_sys::gpu_buffer_usage::MAP_WRITE;
            keep_rust_memory = true;
        }
        if info.usage == BufferUsage::CONSTANT && mappable {
            // The transient allocator creates large bump allocated constant buffers
            // that get used in a hot path.
            keep_rust_memory = true;
        }
        if !info.usage.gpu_writable() && !mappable && !info.usage.contains(BufferUsage::INITIAL_COPY) {
            panic!("The buffer is useless because it can neither be written on the CPU nor the GPU.");
        }
        if info.usage.gpu_writable() && !info.usage.gpu_readable() && !mappable {
            panic!("The buffer is useless because it can only be written on the GPU but the contents cannot be read anywhere.");
        }
        if (usage & web_sys::gpu_buffer_usage::MAP_WRITE) == 0 && mappable && (info.usage.gpu_writable() || !keep_rust_memory || !PREFER_DISCARD_OVER_QUEUE_WRITE) {
            // GpuQueue::writeBuffer requires GpuUsage::COPY_DST
            usage |= web_sys::gpu_buffer_usage::COPY_DST;
        } else if (usage & web_sys::gpu_buffer_usage::MAP_WRITE) != 0 && mappable && !PREFER_DISCARD_OVER_QUEUE_WRITE {
            assert!(keep_rust_memory);
        }

        let rust_memory = if keep_rust_memory {
            let mut rust_memory_vec = Vec::with_capacity(info.size as usize);
            rust_memory_vec.resize(info.size as usize, 0);
            Some(rust_memory_vec.into_boxed_slice())
        } else {
            Option::<Box<[u8]>>::None
        };

        let descriptor = GpuBufferDescriptor::new(info.size as f64, usage);
        if let Some(name) = name {
            descriptor.set_label(name);
        }
        let buffer = device.create_buffer(&descriptor).map_err(|_| ())?;
        descriptor.set_mapped_at_creation(mappable);
        Ok(Self {
            device: device.clone(),
            buffer: RefCell::new(buffer),
            descriptor,
            rust_memory: RefCell::new(rust_memory),
            mappable,
            keep_rust_memory,
            info: info.clone(),
            mapped: AtomicBool::new(false)
        })
    }

    pub fn handle(&self) -> Ref<GpuBuffer> {
        self.buffer.borrow()
    }
}

impl Drop for WebGPUBuffer {
    fn drop(&mut self) {
        let buffer = self.buffer.borrow();
        buffer.destroy();
    }
}

impl Buffer for WebGPUBuffer {
    fn info(&self) -> &BufferInfo {
        &self.info
    }

    unsafe fn map(&self, offset: u64, mut length: u64, invalidate: bool) -> Option<*mut std::ffi::c_void> {
        if !self.mappable {
            return None;
        }
        length = length.min(self.info.size - offset);
        assert_eq!(offset % 8, 0);
        assert_eq!(length % 4, 0);

        let mut memory_opt: std::cell::RefMut<'_, Option<Box<[u8]>>> = self.rust_memory.borrow_mut();
        if (&*memory_opt).is_none() {
            assert!(!self.keep_rust_memory);
            let mut memory_vec = Vec::with_capacity(self.info.size as usize);
            unsafe { memory_vec.set_len(self.info.size as usize); }
            *memory_opt = Some(memory_vec.into_boxed_slice());
        }
        let memory = memory_opt.as_mut().unwrap();

        if invalidate && (self.info.usage.gpu_writable() || !self.keep_rust_memory) {
            panic!("Reading back data from the GPU will require more workarounds");
        }
        unsafe { Some(std::mem::transmute(memory.as_mut_ptr().byte_offset(offset as isize))) }
    }

    unsafe fn unmap(&self, offset: u64, mut length: u64, flush: bool) {
        let mut memory_opt: std::cell::RefMut<'_, Option<Box<[u8]>>> = self.rust_memory.borrow_mut();
        if memory_opt.is_none() {
            assert!(self.mappable);
            // Buffer wasn't mapped
            return;
        }
        let memory = memory_opt.as_mut().unwrap();

        let mut buffer = self.buffer.borrow_mut();
        length = length.min(self.info.size - offset);
        assert_eq!(offset % 8, 0);
        assert_eq!(length % 4, 0);

        if flush {
            let map_directly =
                (&*buffer).map_state() == web_sys::GpuBufferMapState::Mapped
                || self.info.usage == BufferUsage::COPY_SRC // the buffer can only be written on the CPU so the contents of the rust memory always mirror the buffer contents
                || (
                    PREFER_DISCARD_OVER_QUEUE_WRITE
                    && (
                            (!self.info.usage.gpu_writable() && self.keep_rust_memory) // the buffer can only be written on the CPU so the contents of the rust memory always mirror the buffer contents
                            || (offset == 0 && length == self.info.size)
                        )
                    ); // Replace the entire buffer with one that's mapped at creation. Map at creation can be set without USAGE_MAP_*.
            if map_directly {
                if buffer.map_state() != web_sys::GpuBufferMapState::Mapped {
                    // Create a new buffer that's mapped at creation
                    buffer.destroy();
                    *buffer = self.device.create_buffer(&self.descriptor).unwrap();
                } else {
                    log::info!("Using directly mapped buffer without discard!");
                }
                assert!(buffer.map_state() == web_sys::GpuBufferMapState::Mapped);
                let mapped_range = buffer.get_mapped_range().unwrap();
                let uint8_array = Uint8Array::new_with_byte_offset_and_length(&mapped_range, offset as u32, length as u32);
                uint8_array.copy_from(&memory[offset as usize .. offset as usize + length as usize]);
                buffer.unmap();
            } else {
                self.device.queue().write_buffer_with_u32_and_u8_slice(
                    &buffer,
                    offset as u32,
                    &memory[offset as usize .. offset as usize + length as usize]
                ).unwrap();
            }
        }
        if !self.keep_rust_memory {
            // Free mapping copy
            *memory_opt = None;
        }
        self.mapped.store(false, std::sync::atomic::Ordering::Release);
    }
}
