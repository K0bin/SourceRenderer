use super::*;

pub trait Fence {
    unsafe fn value(&self) -> u64;
    unsafe fn await_value(&self, value: u64);
}

pub struct FenceValuePairRef<'a, B: GPUBackend> {
    pub fence: &'a B::Fence,
    pub value: u64,
    pub sync_before: BarrierSync
}
