use sourcerenderer_core::gpu;
use std::marker::PhantomData;
use std::{
    cell::{Ref, RefCell},
    hash::Hash,
};
use std::ffi::c_void;
use web_sys::{GpuBuffer, GpuBufferDescriptor, GpuDevice};
use sourcerenderer_core::gpu::BufferCpuAccess;

pub struct WebGPUBuffer {
    device: GpuDevice,
    buffer: RefCell<GpuBuffer>,
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

        if (usage & web_sys::gpu_buffer_usage::MAP_WRITE) == 0 && mappable {
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
            buffer: RefCell::new(buffer),
            mappable,
            info: info.clone(),
            _p: PhantomData,
        })
    }

    #[inline(always)]
    pub(crate) fn handle(&self) -> Ref<'_, GpuBuffer> {
        self.buffer.borrow()
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

    fn cpu_access(&self) -> BufferCpuAccess {
        BufferCpuAccess::DeviceWrite
    }

    fn map_ptr(&self) -> Option<*mut c_void> {
        None
    }

    unsafe fn invalidate(
        &self,
        _offset: u64,
        _length: u64,
    ) {}

    unsafe fn flush(&self, _offset: u64, _length: u64) {}
}
