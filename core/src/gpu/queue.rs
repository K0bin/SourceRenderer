use super::*;

pub struct Submission<'a, B: GPUBackend> {
  pub command_buffers: &'a [&'a B::CommandBuffer],
  pub wait_fences: &'a [FenceValuePairRef<'a, B>],
  pub signal_fences: &'a [FenceValuePairRef<'a, B>],
  pub acquire_swapchain: Option<(&'a B::Swapchain, &'a <B::Swapchain as Swapchain<B>>::Backbuffer)>,
  pub release_swapchain: Option<(&'a B::Swapchain, &'a <B::Swapchain as Swapchain<B>>::Backbuffer)>,
}

pub trait Queue<B: GPUBackend> {
  unsafe fn create_command_pool(&self, command_pool_type: CommandPoolType, flags: CommandPoolFlags) -> B::CommandPool;
  unsafe fn submit(&self, submissions: &[Submission<B>]);
  unsafe fn present(&self, swapchain: &mut B::Swapchain, backbuffer_key: &<B::Swapchain as Swapchain<B>>::Backbuffer);
}
