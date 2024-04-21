use windows::Win32;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Foundation::WAIT_OBJECT_0;
use windows::Win32::Graphics::Direct3D as D3D;
use windows::Win32::Graphics::Dxgi;
use windows::Win32::Graphics::Direct3D12 as D3D12;
use windows::Win32::Security::SECURITY_ATTRIBUTES;
use windows::Win32::System::Threading::CreateEventExA;

use sourcerenderer_core::gpu;
use windows::Win32::System::Threading::WaitForSingleObject;
use windows::Win32::System::Threading::CREATE_EVENT;
use windows::Win32::System::Threading::INFINITE;

use super::*;

pub struct D3D12Fence {
    fence: D3D12::ID3D12Fence1
}

impl D3D12Fence {
    pub(crate) fn new(device: &D3D12::ID3D12Device12) -> Self {
        let fence = unsafe { device.CreateFence(0u64, D3D12::D3D12_FENCE_FLAG_NONE).unwrap() };
        Self {
            fence
        }
    }

    pub(crate) fn handle(&self) -> &D3D12::ID3D12Fence1 {
        &self.fence
    }
}

impl gpu::Fence for D3D12Fence {
    unsafe fn value(&self) -> u64 {
        unsafe { self.fence.GetCompletedValue() }
    }

    unsafe fn await_value(&self, value: u64) {
        unsafe {
            const SYNCHRONIZE: u32 = 0x00100000;
            let event = CreateEventExA(None, None, CREATE_EVENT(0u32), SYNCHRONIZE).unwrap();
            self.fence.SetEventOnCompletion(value, event).unwrap();
            let result = WaitForSingleObject(event, INFINITE);
            assert_eq!(result, WAIT_OBJECT_0);
            CloseHandle(event);
        }
    }
}
