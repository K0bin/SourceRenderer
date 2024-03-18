use sourcerenderer_core::gpu::ResourceHeapInfo;
use windows::Win32::Graphics::Direct3D as D3D;
use windows::Win32::Graphics::Dxgi;
use windows::Win32::Graphics::Direct3D12 as D3D12;
use windows::core::Interface;

use sourcerenderer_core::gpu;

use super::*;

pub struct D3D12Device {
    device: D3D12::ID3D12Device12,
    adapter: Dxgi::IDXGIAdapter4,
    memory_types: [gpu::MemoryTypeInfo; 4]
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

        Self {
            device,
            adapter: adapter.clone(),
            memory_types
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
        let heap_type = match memory_type_index {
            0 => D3D12::D3D12_HEAP_TYPE_DEFAULT,
            1 => D3D12::D3D12_HEAP_TYPE_UPLOAD,
            2 => D3D12::D3D12_HEAP_TYPE_READBACK,
            3 => D3D12::D3D12_HEAP_TYPE_GPU_UPLOAD,
        };
        D3D12Heap::new(&self.device, heap_type, size)
    }
}
