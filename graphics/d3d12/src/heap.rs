use windows::Win32::Graphics::Direct3D as D3D;
use windows::Win32::Graphics::Dxgi;
use windows::Win32::Graphics::Direct3D12 as D3D12;
use windows::core::Interface;

use sourcerenderer_core::gpu;

use super::*;

pub(crate) enum ResourceMemory<'a> {
    Dedicated {
        heap_type: D3D12::D3D12_HEAP_TYPE
    },
    Suballocated {
        memory: &'a D3D12Heap,
        offset: u64
    }
}

pub struct D3D12Heap {
    heap: D3D12::ID3D12Heap1,
    device: D3D12::ID3D12Device12,
    memory_type_index: u32,
    heap_type: D3D12::D3D12_HEAP_TYPE
}

impl D3D12Heap {
    pub(crate) fn new(device: &D3D12::ID3D12Device12, memory_type_index: u32, size: u64) -> Result<Self, gpu::OutOfMemoryError> {
        let heap_type = memory_type_index_to_memory_heap(memory_type_index);
        let heap_properties = D3D12::D3D12_HEAP_PROPERTIES {
            Type: heap_type,
            CPUPageProperty: D3D12::D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: D3D12::D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 0,
            VisibleNodeMask: 0,
        };

        let mut alignment = D3D12::D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT;
        let mut flags = D3D12::D3D12_HEAP_FLAG_ALLOW_ALL_BUFFERS_AND_TEXTURES;
        flags |= D3D12::D3D12_HEAP_FLAG_CREATE_NOT_ZEROED;
        if heap_type == D3D12::D3D12_HEAP_TYPE_DEFAULT {
            flags |= D3D12::D3D12_HEAP_FLAG_ALLOW_SHADER_ATOMICS;
            alignment = D3D12::D3D12_DEFAULT_MSAA_RESOURCE_PLACEMENT_ALIGNMENT;
        }

        let mut desc = D3D12::D3D12_HEAP_DESC {
            SizeInBytes: size,
            Properties: heap_properties,
            Alignment: alignment as u64,
            Flags: flags,
        };

        let mut heap_opt: Option<D3D12::ID3D12Heap1> = None;
        unsafe {
            device.CreateHeap1(&desc as *const D3D12::D3D12_HEAP_DESC, None, &mut heap_opt as *mut Option<D3D12::ID3D12Heap1>).map_err(|_e| gpu::OutOfMemoryError {})?
        }
        let heap = heap_opt.ok_or(gpu::OutOfMemoryError {})?;
        Ok(Self {
            heap,
            device: device.clone(),
            memory_type_index,
            heap_type
        })
    }

    pub(crate) fn handle(&self) -> &D3D12::ID3D12Heap1 {
        &self.heap
    }

    pub(crate) fn heap_type(&self) -> D3D12::D3D12_HEAP_TYPE {
        self.heap_type
    }
}

impl gpu::Heap<D3D12Backend> for D3D12Heap {
    fn memory_type_index(&self) -> u32 {
        self.memory_type_index
    }

    unsafe fn create_buffer(&self, info: &gpu::BufferInfo, offset: u64, name: Option<&str>) -> Result<D3D12Buffer, gpu::OutOfMemoryError> {
        D3D12Buffer::new(&self.device, ResourceMemory::Suballocated { memory: self, offset }, info, name)
    }

    unsafe fn create_texture(&self, info: &gpu::TextureInfo, offset: u64, name: Option<&str>) -> Result<D3D12Texture, gpu::OutOfMemoryError> {
        D3D12Texture::new(&self.device, ResourceMemory::Suballocated { memory: self, offset }, info, name)
    }
}

