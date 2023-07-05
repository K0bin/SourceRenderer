use super::*;

pub struct Submission<'a, B: GPUBackend> {
  pub command_buffers: &'a [&'a B::CommandBuffer],
  pub wait_fences: &'a [FenceRef<'a, B>],
  pub signal_fences: &'a [FenceRef<'a, B>],
}

pub trait Queue<B: GPUBackend> {
  unsafe fn create_command_pool(&self, command_pool_type: CommandPoolType) -> B::CommandPool;
  unsafe fn submit(&self, submissions: &[Submission<B>]);
  unsafe fn present(&self, swapchain: &B::Swapchain, wait_fence: &B::WSIFence);
}
