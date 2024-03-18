use std::ffi::c_void;
use std::hash::{Hash, Hasher};

use windows::Win32::Graphics::Direct3D as D3D;
use windows::Win32::Graphics::Dxgi;
use windows::Win32::Graphics::Direct3D12 as D3D12;
use windows::core::Interface;

use sourcerenderer_core::gpu;

use super::*;

pub struct D3D12Buffer {
    buffer: D3D12::ID3D12Resource2,
    info: gpu::BufferInfo,
    ptr: Option<*mut c_void>
}

impl D3D12Buffer {
    pub(crate) fn new(device: &D3D12::ID3D12Device12, memory: ResourceMemory, info: gpu::BufferInfo, name: Option<&str>) -> Result<Self, gpu::OutOfMemoryError> {
        let mut flags = D3D12::D3D12_RESOURCE_FLAG_NONE;
        flags |= D3D12::D3D12_RESOURCE_FLAG_DENY_SHADER_RESOURCE;
        if info.usage.contains(gpu::BufferUsage::STORAGE) {
            flags |= D3D12::D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
        }

        let mut desc = D3D12::D3D12_RESOURCE_DESC1 {
            Dimension: D3D12::D3D12_RESOURCE_DIMENSION_BUFFER,
            Alignment: D3D12::D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT as u64,
            Width: info.size,
            Height: 1u32,
            DepthOrArraySize: 1u16,
            MipLevels: 1u16,
            Format: Dxgi::Common::DXGI_FORMAT_UNKNOWN,
            SampleDesc: Dxgi::Common::DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12::D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: flags,
            SamplerFeedbackMipRegion: D3D12::D3D12_MIP_REGION { Width: 0u32, Height: 0u32, Depth: 0u32 },
        };

        let mut map = false;

        let mut resource_opt: Option<D3D12::ID3D12Resource2> = None;
        match memory {
            ResourceMemory::Dedicated { heap_type } => {
                let heap_properties = D3D12::D3D12_HEAP_PROPERTIES {
                    Type: heap_type,
                    CPUPageProperty: D3D12::D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
                    MemoryPoolPreference: D3D12::D3D12_MEMORY_POOL_UNKNOWN,
                    CreationNodeMask: 0,
                    VisibleNodeMask: 0,
                };

                let mut flags: D3D12::D3D12_HEAP_FLAGS = D3D12::D3D12_HEAP_FLAG_NONE;
                flags |= D3D12::D3D12_HEAP_FLAG_CREATE_NOT_ZEROED;
                if heap_type == D3D12::D3D12_HEAP_TYPE_DEFAULT {
                    flags |= D3D12::D3D12_HEAP_FLAG_ALLOW_SHADER_ATOMICS;
                } else {
                    map = true;
                }

                unsafe {
                    let protected = Option::<&D3D12::ID3D12ProtectedResourceSession>::None;
                    device.CreateCommittedResource3(
                        &heap_properties as *const D3D12::D3D12_HEAP_PROPERTIES,
                        flags,
                        &desc as *const D3D12::D3D12_RESOURCE_DESC1,
                        D3D12::D3D12_BARRIER_LAYOUT_COMMON,
                        None,
                        protected,
                        None,
                        &mut resource_opt as *mut Option<D3D12::ID3D12Resource2>
                    )
                }
            },
            ResourceMemory::Suballocated { memory: heap, offset } => {
                map = heap.heap_type() != D3D12::D3D12_HEAP_TYPE_DEFAULT;

                unsafe {
                    device.CreatePlacedResource2(
                        heap.handle(), offset,
                        &desc as *const D3D12::D3D12_RESOURCE_DESC1,
                        D3D12::D3D12_BARRIER_LAYOUT_COMMON,
                        None,
                        None,
                        &mut resource_opt as *mut Option<D3D12::ID3D12Resource2>
                    )
                }
            }
        }.map_err(|_e| gpu::OutOfMemoryError {})?;

        let resource = resource_opt.unwrap();

        let ptr = if map {
            unsafe {
                let mut ptr_opt = Option::<*mut *mut c_void>::None;
                resource.Map(0, None, ptr_opt).map_err(|_e| gpu::OutOfMemoryError {})?;
                ptr_opt.map(|ptr| *ptr)
            }
        } else {
            None
        };

        Ok(Self {
            buffer: resource,
            info: info.clone(),
            ptr
        })
    }
}

impl Drop for D3D12Buffer {
    fn drop(&mut self) {
        unsafe {
            self.buffer.Unmap(0, None);
        }
    }
}

impl gpu::Buffer for D3D12Buffer {
    fn info(&self) -> &gpu::BufferInfo {
        &self.info
    }

    unsafe fn map(&self, offset: u64, length: u64, invalidate: bool) -> Option<*mut c_void> {
        self.ptr.map(|ptr| ptr.byte_offset(offset as isize))
    }

    unsafe fn unmap(&self, offset: u64, length: u64, flush: bool) {
    }
}

unsafe impl Send for D3D12Buffer {}
unsafe impl Sync for D3D12Buffer {}

impl PartialEq<D3D12Buffer> for D3D12Buffer {
    fn eq(&self, other: &D3D12Buffer) -> bool {
        self.buffer == other.buffer
    }
}

impl Eq for D3D12Buffer {}

impl Hash for D3D12Buffer {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.buffer.as_raw().hash(state)
    }
}
