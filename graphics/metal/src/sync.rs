use metal;
use metal::foreign_types::ForeignType;
use metal::objc::sel;
use metal::objc::sel_impl;
use metal::objc::runtime::BOOL;

use metal::SharedEventRef;
use objc::msg_send;
use sourcerenderer_core::gpu;

use super::*;

enum MTLEventType {
    Shared(metal::SharedEvent),
    Regular(metal::Event)
}

pub(crate) enum MTLEventTypeRef<'a> {
    Shared(&'a metal::SharedEventRef),
    Regular(&'a metal::EventRef)
}

pub struct MTLFence {
    event: MTLEventType
}

impl MTLFence {
    pub(crate) fn new(device: &metal::DeviceRef, is_cpu_accessible: bool) -> Self {
        let event = if is_cpu_accessible {
            MTLEventType::Shared(device.new_shared_event())
        } else {
            MTLEventType::Regular(device.new_event())
        };
        Self {
            event
        }
    }

    pub(crate) fn handle(&self) -> MTLEventTypeRef {
        match &self.event {
            MTLEventType::Regular(event) => MTLEventTypeRef::Regular(event),
            MTLEventType::Shared(event) => MTLEventTypeRef::Shared(event)
        }
    }

    pub(crate) fn event_handle(&self) -> &metal::EventRef {
        match &self.event {
            MTLEventType::Regular(event) => event,
            MTLEventType::Shared(event) => event
        }

    }
}

impl gpu::Fence for MTLFence {
    unsafe fn value(&self) -> u64 {
        match &self.event {
            MTLEventType::Shared(event) => event.signaled_value(),
            _ => panic!("Fence is not CPU accessible")
        }
    }

    unsafe fn await_value(&self, value: u64) {
        let timeout = u64::MAX;
        match &self.event {
            MTLEventType::Shared(event) => unsafe {
                let _result: BOOL = msg_send![event as &SharedEventRef,
                    waitUntilSignaledValue:value
                        timeoutMS:timeout];
            },
            _ => panic!("Fence is not CPU accessible")
        }
    }
}
