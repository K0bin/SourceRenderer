use std::{cell::{Ref, RefCell}, hash::Hash, sync::atomic::AtomicBool};

use sourcerenderer_core::gpu::{Buffer, BufferInfo, BufferUsage};

use web_sys::{js_sys::Uint8Array, GpuBuffer, GpuBufferDescriptor, GpuDevice};

pub struct WebGPUBuffer {
    device: GpuDevice,
    buffer: RefCell<GpuBuffer>,
    descriptor: GpuBufferDescriptor,
    rust_memory: RefCell<Box<[u8]>>,
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
    pub fn new(device: &GpuDevice, info: &BufferInfo, name: Option<&str>) -> Result<Self, ()> {
        let mut usage = 0u32;
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
        if info.usage.contains(BufferUsage::COPY_SRC) {
            usage |= web_sys::gpu_buffer_usage::COPY_SRC;
        }
        if info.usage.intersects(BufferUsage::COPY_DST | BufferUsage::INITIAL_COPY) {
            usage |= web_sys::gpu_buffer_usage::COPY_DST;
        }
        if info.usage.contains(BufferUsage::STORAGE) {
            usage |= web_sys::gpu_buffer_usage::STORAGE;
        }
        usage |= web_sys::gpu_buffer_usage::MAP_READ | web_sys::gpu_buffer_usage::MAP_WRITE;

        let descriptor = GpuBufferDescriptor::new(info.size as f64, usage);
        if let Some(name) = name {
            descriptor.set_label(name);
        }
        let buffer = device.create_buffer(&descriptor).map_err(|_| ())?;
        descriptor.set_mapped_at_creation(true);
        let mut rust_memory_vec = Vec::with_capacity(info.size as usize);
        rust_memory_vec.resize(info.size as usize, 0);
        Ok(Self {
            device: device.clone(),
            buffer: RefCell::new(buffer),
            descriptor,
            rust_memory: RefCell::new(rust_memory_vec.into_boxed_slice()),
            info: info.clone(),
            mapped: AtomicBool::new(false)
        })

    }
}

impl Buffer for WebGPUBuffer {
    fn info(&self) -> &BufferInfo {
        &self.info
    }

    unsafe fn map(&self, offset: u64, mut length: u64, invalidate: bool) -> Option<*mut std::ffi::c_void> {
        let buffer = self.buffer.borrow_mut();
        let mut memory = self.rust_memory.borrow_mut();
        let was_mapped = self.mapped.swap(true, std::sync::atomic::Ordering::Acquire);
        let webgpu_mapped = buffer.map_state() != web_sys::GpuBufferMapState::Mapped;
        assert_eq!(webgpu_mapped, was_mapped);
        length = length.min(self.info.size);
        if !webgpu_mapped {
            buffer.map_async_with_u32_and_u32(if invalidate { web_sys::gpu_map_mode::READ } else { web_sys::gpu_map_mode::WRITE }, offset as u32, length as u32);
        }
        if invalidate {
            assert!(!was_mapped);
            if buffer.map_state() != web_sys::GpuBufferMapState::Mapped {
                return None;
            }
            let mapped_range = buffer.get_mapped_range().ok()?;
            let uint8_array = Uint8Array::new(&mapped_range);
            unsafe {
                uint8_array.raw_copy_to_ptr(std::mem::transmute(memory.as_ptr()));
            }
        }
        unsafe { Some(std::mem::transmute(memory.as_mut_ptr())) }
    }

    unsafe fn unmap(&self, offset: u64, mut length: u64, flush: bool) {
        let mut buffer = self.buffer.borrow_mut();
        let memory = self.rust_memory.borrow();
        length = length.min(self.info.size);
        if flush {
            if buffer.map_state() != web_sys::GpuBufferMapState::Mapped && offset == 0 && length == self.info.size {
                *buffer = self.device.create_buffer(&self.descriptor).unwrap();
            }
            assert!(buffer.map_state() == web_sys::GpuBufferMapState::Mapped);
            let mapped_range = buffer.get_mapped_range().unwrap();
            let uint8_array = Uint8Array::new(&mapped_range);
            uint8_array.copy_from(memory.as_ref());
        }
        buffer.unmap();
        self.mapped.store(false, std::sync::atomic::Ordering::Release);
    }
}
