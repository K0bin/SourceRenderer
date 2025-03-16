use objc2::rc::Retained;

use objc2::runtime::ProtocolObject;
use objc2_metal;

use objc2_metal::{MTLDevice as _, MTLSharedEvent as _};

use sourcerenderer_core::gpu;

enum MTLEventType {
    Shared(Retained<ProtocolObject<dyn objc2_metal::MTLSharedEvent>>),
    Regular(Retained<ProtocolObject<dyn objc2_metal::MTLEvent>>)
}

pub struct MTLFence {
    event: MTLEventType
}

unsafe impl Send for MTLFence {}
unsafe impl Sync for MTLFence {}

impl MTLFence {
    pub(crate) fn new(device: &ProtocolObject<dyn objc2_metal::MTLDevice>, is_cpu_accessible: bool) -> Self {
        let event = if is_cpu_accessible {
            MTLEventType::Shared(device.newSharedEvent().unwrap())
        } else {
            MTLEventType::Regular(device.newEvent().unwrap())
        };
        Self {
            event
        }
    }

    pub(crate) fn event_handle(&self) -> &ProtocolObject<dyn objc2_metal::MTLEvent> {
        match &self.event {
            MTLEventType::Regular(event) => ProtocolObject::from_ref::<ProtocolObject<dyn objc2_metal::MTLEvent>>(event.as_ref()),
            MTLEventType::Shared(event) => ProtocolObject::from_ref::<ProtocolObject<dyn objc2_metal::MTLSharedEvent>>(event.as_ref())
        }
    }

    pub(crate) fn is_shared(&self) -> bool {
        match &self.event {
            MTLEventType::Regular(_) => false,
            MTLEventType::Shared(_) => true
        }
    }

    pub(crate) fn shared_handle(&self) -> &ProtocolObject<dyn objc2_metal::MTLSharedEvent> {
        match &self.event {
            MTLEventType::Regular(_) => panic!(),
            MTLEventType::Shared(event) => event
        }
    }
}

impl gpu::Fence for MTLFence {
    unsafe fn value(&self) -> u64 {
        match &self.event {
            MTLEventType::Shared(event) => event.signaledValue(),
            _ => panic!("Fence is not CPU accessible")
        }
    }

    unsafe fn await_value(&self, value: u64) {
        let timeout = u64::MAX;
        match &self.event {
            MTLEventType::Shared(event) => { event.waitUntilSignaledValue_timeoutMS(value, timeout); },
            _ => panic!("Fence is not CPU accessible")
        }
    }
}
