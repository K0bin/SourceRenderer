use windows::Win32::Graphics::Dxgi;
use windows::Win32::Graphics::Direct3D12 as D3D12;
use windows::core::Interface;

use smallvec::SmallVec;
use sourcerenderer_core::gpu;

use super::*;

pub struct D3D12Instance {
    factory: Dxgi::IDXGIFactory7,
    adapters: SmallVec<[D3D12Adapter; 2]>
}

impl D3D12Instance {
    pub fn new(debug_layers: bool) -> Option<Self> {
        unsafe {
            let mut debug: Option<D3D12::ID3D12Debug> = None;
            if let Some(debug) = D3D12::D3D12GetDebugInterface(&mut debug).ok().and(debug) {
                debug.EnableDebugLayer();
            }
        }

        let factory: Dxgi::IDXGIFactory7 = unsafe { Dxgi::CreateDXGIFactory2(if debug_layers { Dxgi::DXGI_CREATE_FACTORY_DEBUG  } else { 0 }).ok().unwrap() };

        let mut adapters: SmallVec<[D3D12Adapter; 2]> = SmallVec::<[D3D12Adapter; 2]>::new();
        let mut i = 0u32;
        unsafe {
            while let Ok(adapter) = factory.EnumAdapters1(i) {
                let adapter4 = adapter.cast::<Dxgi::IDXGIAdapter4>().unwrap();
                let adapter = D3D12Adapter::new(adapter4);
                adapters.push(adapter);
                i += 1;
            }
        }

        Some(Self {
            factory,
            adapters
        })
    }
}

impl gpu::Instance<D3D12Backend> for D3D12Instance {
    fn list_adapters(&self) -> &[D3D12Adapter] {
        &self.adapters
    }
}

pub struct D3D12Adapter {
    adapter_type: gpu::AdapterType,
    adapter: Dxgi::IDXGIAdapter4
}

impl D3D12Adapter {
    fn new(adapter: Dxgi::IDXGIAdapter4) -> Self {
        let desc = unsafe {
            let mut desc: Dxgi::DXGI_ADAPTER_DESC3 = std::mem::zeroed();
            adapter.GetDesc3(&mut desc as *mut Dxgi::DXGI_ADAPTER_DESC3);
            desc
        };

        let adapter_type = if desc.DedicatedVideoMemory != 0 {
            gpu::AdapterType::Discrete
        } else if desc.Flags.contains(Dxgi::DXGI_ADAPTER_FLAG3_SOFTWARE) {
            gpu::AdapterType::Software
        } else {
            gpu::AdapterType::Integrated
        };

        Self {
            adapter,
            adapter_type
        }
    }
}

impl gpu::Adapter<D3D12Backend> for D3D12Adapter {
    fn adapter_type(&self) -> gpu::AdapterType {
        self.adapter_type
    }

    fn create_device(&self, surface: &B::Surface) -> D3D12Device {
        D3D12Device::new(&self.adapter)
    }
}
