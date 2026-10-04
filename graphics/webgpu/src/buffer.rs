use sourcerenderer_core::gpu;
use std::marker::PhantomData;
use std::{
    cell::RefCell,
    hash::Hash,
};
use std::cell::Cell;
use std::ffi::c_void;
use web_sys::{GpuBuffer, GpuBufferDescriptor, GpuDevice};
use sourcerenderer_core::gpu::BufferCpuAccess;

pub struct WebGPUBuffer {
    device: GpuDevice,
    buffer: GpuBuffer,
    rust_memory: RefCell<Option<Box<[u8]>>>,
    ptr: Cell<*mut c_void>,
    mappable: bool,
    info: gpu::BufferInfo,
    _p: PhantomData<*const std::ffi::c_void>,
}

impl PartialEq for WebGPUBuffer {
    fn eq(&self, other: &Self) -> bool {
        self.buffer == other.buffer
    }
}

impl Eq for WebGPUBuffer {}

impl Hash for WebGPUBuffer {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let buffer = &self.buffer;
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
        // Besides that, map() is async and the buffer cannot be used by the GPU while it is mapped.
        // Tons of fun to work around...

        let mut usage = 0u32;
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
        if !info.usage.gpu_writable()
            && !mappable
            && !info.usage.contains(gpu::BufferUsage::INITIAL_COPY)
        {
            panic!(
                "The buffer is useless because it can neither be written on the CPU nor the GPU."
            );
        }
        if info.usage.gpu_writable() && !info.usage.gpu_readable() && !mappable {
            panic!(
                "The buffer is useless because it can only be written on the GPU but the contents cannot be read anywhere."
            );
        }

        let ptr: *mut c_void = std::ptr::null_mut();
        let rust_memory: Option<Box<[u8]>> = None;
        if mappable {
            // GpuQueue::writeBuffer requires GpuUsage::COPY_DST
            usage |= web_sys::gpu_buffer_usage::COPY_DST;
        }

        let descriptor = GpuBufferDescriptor::new(info.size as u32, usage);
        if let Some(name) = name {
            descriptor.set_label(name);
        }
        let buffer = device.create_buffer(&descriptor).map_err(|e| {
            log::error!("Failed to create buffer: {:?}", e);
            ()
        })?;

        Ok(Self {
            device: device.clone(),
            buffer,
            rust_memory: RefCell::new(rust_memory),
            ptr: Cell::new(ptr),
            mappable,
            info: info.clone(),
            _p: PhantomData,
        })
    }

    #[inline(always)]
    pub(crate) fn handle(&self) -> &GpuBuffer {
        &self.buffer
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
        self.buffer.destroy();
    }
}

impl gpu::Buffer for WebGPUBuffer {
    fn info(&self) -> &gpu::BufferInfo {
        &self.info
    }

    fn cpu_access(&self) -> BufferCpuAccess {
        BufferCpuAccess::DeviceWrite
    }

    fn map_ptr(&self) -> Option<*mut c_void> {
        if !self.mappable {
            return None;
        }
        debug_assert!(self.mappable);
        if cfg!(debug_assertions) {
            if self.info.size < 4096 {
                log::warn!("{}\n{}", "Mapping buffers requires keeping a copy of the buffer around in WASM memory.",
                "This is not worth the overhead for small buffers. Try using Device::copy_to_buffer instead.");
            }
        }
        let mut rust_mem = self.rust_memory.borrow_mut();
        if rust_mem.is_none() {
            let rust_memory_vec = vec![0u8; self.info.size as usize];
            let mut rust_memory_box = rust_memory_vec.into_boxed_slice();
            let ptr = rust_memory_box.as_mut_ptr() as *mut c_void;
            self.ptr.replace(ptr);
            *rust_mem = Some(rust_memory_box)
        }

        Some(self.ptr.get())
    }

    unsafe fn invalidate(
        &self,
        _offset: u64,
        _length: u64,
    ) {
        log::warn!("Reading buffer contents written by the GPU would need a ton of hacks in WebGPU. That's not implemented.");
    }

    unsafe fn flush(&self, offset: u64, length: u64) {
        let rust_mem = self.rust_memory.borrow();
        if rust_mem.is_none() {
            return;
        }
        if cfg!(debug_assertions) {
            if length < 1024 {
                log::warn!("Flushing small range. Try using Device::copy_to_buffer instead.");
            }
        }
        let memory = rust_mem.as_ref().unwrap();
        debug_assert!(offset + length <= self.info.size);
        debug_assert!((memory.len() as u64) >= length);

        debug_assert_ne!((self.buffer.usage() & web_sys::gpu_buffer_usage::COPY_DST), 0);
        self.device
            .queue()
            .write_buffer_with_u32_and_u8_slice(&self.buffer, offset as u32, &memory[offset as usize..(offset + length) as usize])
            .unwrap();
    }
}
