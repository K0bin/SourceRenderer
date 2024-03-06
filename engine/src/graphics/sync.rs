use std::{mem::ManuallyDrop, sync::Arc};

use sourcerenderer_core::gpu::{*, Fence as GPUFence};

use super::*;

pub struct Fence<B: GPUBackend> {
    fence: ManuallyDrop<B::Fence>,
    destroyer: Arc<DeferredDestroyer<B>>
}

impl<B: GPUBackend> Drop for Fence<B> {
    fn drop(&mut self) {
        let fence = unsafe { ManuallyDrop::take(&mut self.fence) };
        self.destroyer.destroy_fence(fence);
    }
}

impl<B: GPUBackend> Fence<B> {
    pub(super) fn new(device: &B::Device, destoryer: &Arc<DeferredDestroyer<B>>) -> Self {
        let fence = unsafe {device.create_fence() };
        Self {
            fence: ManuallyDrop::new(fence),
            destroyer: destoryer.clone()
        }
    }

    pub fn value(&self) -> u64 {
        unsafe { self.fence.value() }
    }

    pub fn await_value(&self, value: u64) {
        unsafe {
            self.fence.await_value(value);
        }
    }

    pub(super) fn handle(&self) -> &B::Fence {
        &self.fence
    }
}

pub struct SharedFenceValuePairRef<'a, B: GPUBackend> {
    pub fence: &'a Arc<super::Fence<B>>,
    pub value: u64,
    pub sync_before: BarrierSync
}

impl<'a, B: GPUBackend> SharedFenceValuePairRef<'a, B> {
    pub unsafe fn is_signalled(&self) -> bool {
        self.fence.value() >= self.value
    }

    pub unsafe fn await_signal(&self) {
        self.fence.await_value(self.value);
    }
}

pub struct SharedFenceValuePair<B: GPUBackend> {
    pub fence: Arc<super::Fence<B>>,
    pub value: u64,
    pub sync_before: BarrierSync
}

impl<B: GPUBackend> Clone for SharedFenceValuePair<B> {
    fn clone(&self) -> Self {
        Self {
            fence: self.fence.clone(),
            value: self.value,
            sync_before: self.sync_before
        }
    }
}

impl<B: GPUBackend> SharedFenceValuePair<B> {
    pub fn is_signalled(&self) -> bool {
        self.fence.value() >= self.value
    }

    pub fn await_signal(&self) {
        self.fence.await_value(self.value);
    }

    pub fn as_ref(&self) -> SharedFenceValuePairRef<B> {
        SharedFenceValuePairRef {
            fence: &self.fence,
            value: self.value,
            sync_before: self.sync_before
        }
    }

    pub fn as_handle_ref(&self) -> sourcerenderer_core::gpu::FenceValuePairRef<B> {
        sourcerenderer_core::gpu::FenceValuePairRef {
            fence: self.fence.handle(),
            value: self.value,
            sync_before: self.sync_before
        }
    }
}

impl<'a, B: GPUBackend> From<&SharedFenceValuePairRef<'a, B>> for SharedFenceValuePair<B> {
    fn from(other: &SharedFenceValuePairRef<B>) -> Self {
        Self {
            fence: other.fence.clone(),
            value: other.value,
            sync_before: other.sync_before
        }
    }
}
