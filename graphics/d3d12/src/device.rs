use sourcerenderer_core::gpu::ResourceHeapInfo;
use windows::core::GUID;
use windows::Win32::Graphics::Direct3D as D3D;
use windows::Win32::Graphics::Dxgi;
use windows::Win32::Graphics::Direct3D12 as D3D12;
use windows::core::Interface;

use sourcerenderer_core::gpu;

use super::*;

pub(crate) fn memory_type_index_to_memory_heap(memory_type_index: u32) -> D3D12::D3D12_HEAP_TYPE {
    match memory_type_index {
        0 => D3D12::D3D12_HEAP_TYPE_DEFAULT,
        1 => D3D12::D3D12_HEAP_TYPE_UPLOAD,
        2 => D3D12::D3D12_HEAP_TYPE_READBACK,
        3 => D3D12::D3D12_HEAP_TYPE_GPU_UPLOAD,
    }
}

pub struct D3D12Device {
    device: D3D12::ID3D12Device12,
    adapter: Dxgi::IDXGIAdapter4,
    memory_types: [gpu::MemoryTypeInfo; 4],
    creator_id: GUID,
    graphics_queue: D3D12Queue,
    compute_queue: D3D12Queue,
    transfer_queue: D3D12Queue,

    // Descriptor Heaps
    src_descriptor_heap: D3D12DescriptorHeap,
    src_sampler_descriptor_heap: D3D12DescriptorHeap,
    src_rtv_descriptor_heap: D3D12DescriptorHeap,
    src_dsv_descriptor_heap: D3D12DescriptorHeap,
    gpu_descriptor_heap: D3D12DescriptorHeap,
    gpu_sampler_descriptor_heap: D3D12DescriptorHeap,
}

impl D3D12Device {
    pub(crate) fn new(adapter: &Dxgi::IDXGIAdapter4) -> Self {
        let mut device_opt: Option<D3D12::ID3D12Device12> = None;
        unsafe {
            D3D12::D3D12CreateDevice(adapter, D3D::D3D_FEATURE_LEVEL_12_2, &mut device_opt as *mut Option<D3D12::ID3D12Device12>).unwrap();
        }
        let device = device_opt.unwrap();

        let memory_types = [
            gpu::MemoryTypeInfo {
                is_cached: false,
                memory_index: 0,
                memory_kind: gpu::MemoryKind::VRAM,
                is_cpu_accessible: false,
                is_coherent: false
            },
            gpu::MemoryTypeInfo {
                is_cached: false,
                memory_kind: gpu::MemoryKind::RAM,
                memory_index: 1,
                is_cpu_accessible: true,
                is_coherent: true
            },
            gpu::MemoryTypeInfo {
                is_cached: true,
                memory_kind: gpu::MemoryKind::RAM,
                memory_index: 1,
                is_cpu_accessible: true,
                is_coherent: true
            },
            gpu::MemoryTypeInfo {
                is_cached: false,
                memory_kind: gpu::MemoryKind::VRAM,
                memory_index: 0,
                is_cpu_accessible: true,
                is_coherent: true
            },
        ];

        let creator_id = GUID::new().unwrap();

        let graphics_queue = D3D12Queue::new(&device, gpu::QueueType::Graphics, &creator_id);
        let compute_queue = D3D12Queue::new(&device, gpu::QueueType::Compute, &creator_id);
        let transfer_queue = D3D12Queue::new(&device, gpu::QueueType::Transfer, &creator_id);

        let mut descriptor_heap_desc = D3D12::D3D12_DESCRIPTOR_HEAP_DESC {
            Type: D3D12::D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            NumDescriptors: 2048,
            Flags: D3D12::D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
            NodeMask: 0,
        };
        let src_descriptor_heap = D3D12DescriptorHeap::new(&device, D3D12::D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, false, 2048);
        let src_sampler_descriptor_heap = D3D12DescriptorHeap::new(&device, D3D12::D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER, false, 32);
        let src_rtv_descriptor_heap = D3D12DescriptorHeap::new(&device, D3D12::D3D12_DESCRIPTOR_HEAP_TYPE_RTV, false, 128);
        let src_dsv_descriptor_heap = D3D12DescriptorHeap::new(&device, D3D12::D3D12_DESCRIPTOR_HEAP_TYPE_DSV, false, 128);
        let gpu_descriptor_heap = D3D12DescriptorHeap::new(&device, D3D12::D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, true, 128);
        let gpu_sampler_descriptor_heap = D3D12DescriptorHeap::new(&device, D3D12::D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER, true, 32);

        Self {
            device,
            adapter: adapter.clone(),
            memory_types,
            creator_id,
            graphics_queue,
            compute_queue,
            transfer_queue,

            src_descriptor_heap,
            src_sampler_descriptor_heap,
            src_rtv_descriptor_heap,
            src_dsv_descriptor_heap,
            gpu_descriptor_heap,
            gpu_sampler_descriptor_heap,
        }
    }
}

impl gpu::Device<D3D12Backend> for D3D12Device {
    unsafe fn memory_type_infos(&self) -> &[gpu::MemoryTypeInfo] {
        &self.memory_types
    }

    unsafe fn memory_infos(&self) -> Vec<gpu::MemoryInfo> {
        let local_memory_info = unsafe {
            let mut memory_info = std::mem::zeroed::<Dxgi::DXGI_QUERY_VIDEO_MEMORY_INFO>();
            self.adapter.QueryVideoMemoryInfo(0, Dxgi::DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut memory_info as *mut Dxgi::DXGI_QUERY_VIDEO_MEMORY_INFO)
                .expect("Failed to determine memory");
            memory_info
        };
        let system_memory_info = unsafe {
            let mut memory_info = std::mem::zeroed::<Dxgi::DXGI_QUERY_VIDEO_MEMORY_INFO>();
            self.adapter.QueryVideoMemoryInfo(0, Dxgi::DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL, &mut memory_info as *mut Dxgi::DXGI_QUERY_VIDEO_MEMORY_INFO)
                .expect("Failed to determine memory");
            memory_info
        };

        let mut memory_infos = Vec::<gpu::MemoryInfo>::new();
        memory_infos.push(gpu::MemoryInfo {
            memory_kind: gpu::MemoryKind::VRAM,
            available: local_memory_info.Budget - local_memory_info.CurrentUsage,
            total: local_memory_info.Budget
        });
        memory_infos.push(gpu::MemoryInfo {
            memory_kind: gpu::MemoryKind::RAM,
            available: system_memory_info.Budget - system_memory_info.CurrentUsage,
            total: system_memory_info.Budget
        });
        memory_infos
    }

    unsafe fn create_heap(&self, memory_type_index: u32, size: u64) -> Result<D3D12Heap, gpu::OutOfMemoryError> {
        D3D12Heap::new(&self.device, memory_type_index, size)
    }

    unsafe fn create_buffer(&self, info: &gpu::BufferInfo, memory_type_index: u32, name: Option<&str>) -> Result<D3D12Buffer, gpu::OutOfMemoryError> {
        let heap_type = memory_type_index_to_memory_heap(memory_type_index);
        D3D12Buffer::new(&self.device, ResourceMemory::Dedicated { heap_type }, info, name)
    }

    unsafe fn create_texture(&self, info: &gpu::TextureInfo, memory_type_index: u32, name: Option<&str>) -> Result<D3D12Texture, gpu::OutOfMemoryError> {
        let heap_type = memory_type_index_to_memory_heap(memory_type_index);
        D3D12Texture::new(&self.device, ResourceMemory::Dedicated { heap_type }, info, name)
    }

    unsafe fn create_texture_view(&self, texture: &D3D12Texture, info: &gpu::TextureViewInfo, name: Option<&str>) -> D3D12TextureView {
        let descriptor = self.src_descriptor_heap.get_new_descriptor();
        //D3D12TextureView::new()
    }

    fn graphics_queue(&self) -> &D3D12Queue {
        &self.graphics_queue
    }

    fn compute_queue(&self) -> Option<&D3D12Queue> {
        Some(&self.compute_queue)
    }

    fn transfer_queue(&self) -> Option<&D3D12Queue> {
        Some(&self.transfer_queue)
    }
}
