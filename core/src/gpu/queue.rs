use super::*;

pub struct Submission<'a, B: GPUBackend> {
  pub command_buffers: &'a mut [&'a mut B::CommandBuffer],
  pub wait_fences: &'a [FenceValuePairRef<'a, B>],
  pub signal_fences: &'a [FenceValuePairRef<'a, B>],
  pub acquire_swapchain: Option<&'a B::Swapchain>,
  pub release_swapchain: Option<&'a B::Swapchain>,
}

pub trait Queue<B: GPUBackend> {
  unsafe fn create_command_pool(&self, command_pool_type: CommandPoolType, flags: CommandPoolFlags) -> B::CommandPool;
  unsafe fn submit(&self, submissions: &mut [Submission<B>]);
  unsafe fn present(&self, swapchain: &B::Swapchain);
}
