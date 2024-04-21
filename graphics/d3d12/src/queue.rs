use sourcerenderer_core::gpu::ResourceHeapInfo;
use windows::core::GUID;
use windows::Win32::Graphics::Direct3D as D3D;
use windows::Win32::Graphics::Dxgi;
use windows::Win32::Graphics::Direct3D12 as D3D12;
use windows::core::Interface;

use sourcerenderer_core::gpu;

use super::*;

pub struct D3D12Queue {
    queue: D3D12::ID3D12CommandQueue
}

impl D3D12Queue {
    pub(crate) fn new(device: &D3D12::ID3D12Device12, queue_type: gpu::QueueType, creator_id: &GUID) -> Self {
        let desc = D3D12::D3D12_COMMAND_QUEUE_DESC {
            Type: match queue_type {
                gpu::QueueType::Graphics => D3D12::D3D12_COMMAND_LIST_TYPE_DIRECT,
                gpu::QueueType::Compute => D3D12::D3D12_COMMAND_LIST_TYPE_COMPUTE,
                gpu::QueueType::Transfer => D3D12::D3D12_COMMAND_LIST_TYPE_COPY,
            },
            Priority: if queue_type != gpu::QueueType::Transfer { D3D12::D3D12_COMMAND_QUEUE_PRIORITY_HIGH.0 } else { D3D12::D3D12_COMMAND_QUEUE_PRIORITY_NORMAL.0 },
            Flags: D3D12::D3D12_COMMAND_QUEUE_FLAG_NONE,
            NodeMask: 0,
        };
        let queue: D3D12::ID3D12CommandQueue = unsafe {
            device.CreateCommandQueue1(&desc as *const D3D12::D3D12_COMMAND_QUEUE_DESC, creator_id as *const GUID)
        }.unwrap();
        Self {
            queue
        }
    }
}

impl gpu::Queue<D3D12Backend> for D3D12Queue {
    unsafe fn create_command_pool(&self, command_pool_type: gpu::CommandPoolType, flags: gpu::CommandPoolFlags) -> D3D12CommandPool {
        let mut device_opt: Option::<D3D12::ID3D12Device12> = None;
        self.queue.GetDevice(&mut device_opt as *mut Option<D3D12::ID3D12Device12>).unwrap();
        let device = device_opt.unwrap();
        D3D12CommandPool::new(&device, command_pool_type, flags)
    }

    unsafe fn submit(&self, submissions: &[gpu::Submission<D3D12Backend>]) {
        todo!()
    }

    unsafe fn present(&self, swapchain: &D3D12Swapchain) {
        todo!()
    }
}
