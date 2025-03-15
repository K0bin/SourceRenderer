use std::{mem::ManuallyDrop, sync::Arc};

use super::*;

use sourcerenderer_core::gpu::Fence as _;

pub struct Fence {
    fence: ManuallyDrop<active_gpu_backend::Fence>,
    destroyer: Arc<DeferredDestroyer>
}

impl Drop for Fence {
    fn drop(&mut self) {
        let fence = unsafe { ManuallyDrop::take(&mut self.fence) };
        self.destroyer.destroy_fence(fence);
    }
}

impl Fence {
    pub(super) fn new(device: &active_gpu_backend::Device, destoryer: &Arc<DeferredDestroyer>) -> Self {
        let fence = unsafe {device.create_fence(true) };
        Self {
            fence: ManuallyDrop::new(fence),
            destroyer: destoryer.clone()
        }
    }

    #[inline(always)]
    pub fn value(&self) -> u64 {
        unsafe { self.fence.value() }
    }

    #[inline(always)]
    pub fn await_value(&self, value: u64) {
        unsafe {
            self.fence.await_value(value);
        }
    }

    #[inline(always)]
    pub(super) fn handle(&self) -> &active_gpu_backend::Fence {
        &self.fence
    }
}

pub struct SharedFenceValuePairRef<'a> {
    pub fence: &'a Arc<super::Fence>,
    pub value: u64,
    pub sync_before: BarrierSync
}

impl<'a> SharedFenceValuePairRef<'a> {
    #[inline(always)]
    pub unsafe fn is_signalled(&self) -> bool {
        self.fence.value() >= self.value
    }

    #[inline(always)]
    pub unsafe fn await_signal(&self) {
        self.fence.await_value(self.value);
    }
}

pub struct SharedFenceValuePair {
    pub fence: Arc<super::Fence>,
    pub value: u64,
    pub sync_before: BarrierSync
}

impl Clone for SharedFenceValuePair {
    fn clone(&self) -> Self {
        Self {
            fence: self.fence.clone(),
            value: self.value,
            sync_before: self.sync_before
        }
    }
}

impl SharedFenceValuePair {
    #[inline(always)]
    pub fn is_signalled(&self) -> bool {
        self.fence.value() >= self.value
    }

    #[inline(always)]
    pub fn await_signal(&self) {
        self.fence.await_value(self.value);
    }

    #[inline(always)]
    pub fn as_ref(&self) -> SharedFenceValuePairRef {
        SharedFenceValuePairRef {
            fence: &self.fence,
            value: self.value,
            sync_before: self.sync_before
        }
    }

    #[inline(always)]
    pub fn as_handle_ref(&self) -> active_gpu_backend::FenceValuePairRef {
        sourcerenderer_core::gpu::FenceValuePairRef {
            fence: self.fence.handle(),
            value: self.value,
            sync_before: self.sync_before
        }
    }
}

impl<'a> From<&SharedFenceValuePairRef<'a>> for SharedFenceValuePair {
    fn from(other: &SharedFenceValuePairRef) -> Self {
        Self {
            fence: other.fence.clone(),
            value: other.value,
            sync_before: other.sync_before
        }
    }
}
