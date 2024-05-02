use metal;

use sourcerenderer_core::gpu;

use super::*;

pub struct MTLInstance {
    adapters: Vec<MTLAdapter>
}

impl MTLInstance {
    pub fn new(debug_layer: bool) -> Self {
        if debug_layer && !std::env::var("MTL_DEBUG_LAYER").map(|var| var == "1").unwrap_or_default() {
            println!("Metal debug layer cannot be enable programmatically, use env var MTL_DEBUG_LAYER=1.");
        }

        let devices = metal::Device::all();
        let adapters = devices.into_iter().map(|d| MTLAdapter::new(d)).collect();
        Self {
            adapters
        }
    }
}

impl gpu::Instance<MTLBackend> for MTLInstance {
    fn list_adapters(&self) -> &[MTLAdapter] {
        &self.adapters
    }
}

pub struct MTLAdapter {
    device: metal::Device
}

impl MTLAdapter {
    pub(crate) fn new(device: metal::Device) -> Self {
        Self {
            device
        }
    }
}

impl gpu::Adapter<MTLBackend> for MTLAdapter {
    fn adapter_type(&self) -> gpu::AdapterType {
        gpu::AdapterType::Integrated
    }

    fn create_device(&self, surface: &MTLSurface) -> MTLDevice {
        MTLDevice::new(&self.device, surface)
    }
}
