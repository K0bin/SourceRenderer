use std::mem::ManuallyDrop;
use std::sync::Arc;
use crossbeam_channel::{unbounded, Receiver, Sender, TryRecvError};
use super::gpu::Fence as _;
use super::*;

pub struct Fence {
    fence: ManuallyDrop<active_gpu_backend::Fence>,
    destroyer: Arc<DeferredDestroyer>,
}

impl Drop for Fence {
    fn drop(&mut self) {
        let fence = unsafe { ManuallyDrop::take(&mut self.fence) };
        self.destroyer.destroy_fence(fence);
    }
}

impl Fence {
    pub(super) fn new(
        device: &active_gpu_backend::Device,
        destroyer: &Arc<DeferredDestroyer>,
    ) -> Self {
        let fence = unsafe { device.create_fence(true) };
        Self {
            fence: ManuallyDrop::new(fence),
            destroyer: destroyer.clone(),
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
    pub sync_before: BarrierSync,
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
    pub sync_before: BarrierSync,
}

impl Clone for SharedFenceValuePair {
    fn clone(&self) -> Self {
        Self {
            fence: self.fence.clone(),
            value: self.value,
            sync_before: self.sync_before,
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
            sync_before: self.sync_before,
        }
    }

    #[inline(always)]
    pub fn as_handle_ref(&self) -> active_gpu_backend::FenceValuePairRef {
        super::gpu::FenceValuePairRef {
            fence: self.fence.handle(),
            value: self.value,
            sync_before: self.sync_before,
        }
    }
}

impl<'a> From<&SharedFenceValuePairRef<'a>> for SharedFenceValuePair {
    fn from(other: &SharedFenceValuePairRef) -> Self {
        Self {
            fence: other.fence.clone(),
            value: other.value,
            sync_before: other.sync_before,
        }
    }
}

pub struct SplitBarrier {
    split_barrier: ManuallyDrop<active_gpu_backend::SplitBarrier>,
    sender: Sender<active_gpu_backend::SplitBarrier>,
    frame: u64,
}

impl Drop for SplitBarrier {
    fn drop(&mut self) {
        let split_barrier = unsafe { ManuallyDrop::take(&mut self.split_barrier) };
        self.sender.send(split_barrier).unwrap();
    }
}

impl SplitBarrier {
    fn new(
        device: &active_gpu_backend::Device,
        sender: &Sender<active_gpu_backend::SplitBarrier>,
        frame: u64,
    ) -> Self {
        let split_barrier = unsafe { device.create_split_barrier() };
        Self {
            split_barrier: ManuallyDrop::new(split_barrier),
            sender: sender.clone(),
            frame
        }
    }

    fn wrap(
        split_barrier: active_gpu_backend::SplitBarrier,
        sender: &Sender<active_gpu_backend::SplitBarrier>,
        frame: u64,
    ) -> Self {
        Self {
            split_barrier: ManuallyDrop::new(split_barrier),
            sender: sender.clone(),
            frame,
        }
    }

    #[inline(always)]
    pub(super) fn handle(&self, frame: u64) -> &active_gpu_backend::SplitBarrier {
        assert_eq!(self.frame, frame);
        &self.split_barrier
    }
}


pub(super) struct SplitBarrierPool {
    device: Arc<active_gpu_backend::Device>,
    sender: Sender<active_gpu_backend::SplitBarrier>,
    receiver: Receiver<active_gpu_backend::SplitBarrier>,
    barriers: Vec<active_gpu_backend::SplitBarrier>,
    frame: u64,
}

impl SplitBarrierPool {
    pub(super) fn new(device: &Arc<active_gpu_backend::Device>) -> Self {
        let (sender, receiver) = unbounded::<active_gpu_backend::SplitBarrier>();
        Self {
            device: device.clone(),
            sender,
            receiver,
            barriers: Vec::new(),
            frame: 0u64,
        }
    }

    pub(super) fn reset(&mut self, frame: u64) {
        self.frame = frame;
        let mut split_barrier_opt = self.receiver.try_recv();
        while split_barrier_opt.is_ok() {
            let split_barrier = split_barrier_opt.unwrap();
            unsafe { self.device.reset_split_barrier(&split_barrier); }
            self.barriers.push(split_barrier);
            split_barrier_opt = self.receiver.try_recv();
        }
        if let Err(TryRecvError::Disconnected) = split_barrier_opt {
            panic!("Split Barrier pool disconnected.");
        }
    }

    pub fn get_split_barrier(&mut self) -> SplitBarrier {
        if let Some(barrier) = self.barriers.pop() {
            return SplitBarrier::wrap(barrier, &self.sender, self.frame);
        }
        SplitBarrier::new(&self.device, &self.sender, self.frame)
    }
}
