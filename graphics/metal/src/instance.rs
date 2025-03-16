use log::warn;
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_metal;

use sourcerenderer_core::gpu;

use super::*;

pub struct MTLInstance {
    adapters: Vec<MTLAdapter>
}

unsafe impl Send for MTLInstance {}
unsafe impl Sync for MTLInstance {}

impl MTLInstance {
    pub fn new(debug_layer: bool) -> Self {
        if debug_layer && !std::env::var("MTL_DEBUG_LAYER").map(|var| var == "1").unwrap_or_default() {
            warn!("Metal debug layer cannot be enable programmatically, use env var MTL_DEBUG_LAYER=1. \"man MetalValidation\" for more info.");
        }

        let devices = objc2_metal::MTLCopyAllDevices();
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
    device: Retained<ProtocolObject<dyn objc2_metal::MTLDevice>>
}

unsafe impl Send for MTLAdapter {}
unsafe impl Sync for MTLAdapter {}

impl MTLAdapter {
    pub(crate) fn new(device: Retained<ProtocolObject<dyn objc2_metal::MTLDevice>>) -> Self {
        Self {
            device
        }
    }
}

impl gpu::Adapter<MTLBackend> for MTLAdapter {
    fn adapter_type(&self) -> gpu::AdapterType {
        gpu::AdapterType::Integrated
    }

    unsafe fn create_device(&self, surface: &MTLSurface) -> MTLDevice {
        MTLDevice::new(&self.device, surface)
    }
}
