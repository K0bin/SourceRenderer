use super::*;

use std::hash::Hash;

// WANT https://github.com/rust-lang/rust/issues/44265
pub trait GPUBackend: 'static + Sized {
  type Instance: Instance<Self> + Send + Sync;
  type Adapter: Adapter<Self> + Send + Sync;
  type Device: Device<Self> + Send + Sync;
  type Surface: Send + Sync + PartialEq + Eq;
  type Swapchain: Swapchain<Self> + Send + Sync;
  type CommandPool: CommandPool<Self>;
  type CommandBuffer: CommandBuffer<Self>;
  type Texture: Send + Sync + PartialEq + Eq;
  type TextureView: Send + Sync + PartialEq + Eq;
  type Sampler: Send + Sync;
  type Buffer: Buffer + Send + Sync;
  type Shader: Shader + Hash + Eq + PartialEq + Send + Sync;
  type GraphicsPipeline: Send + Sync;
  type ComputePipeline: ComputePipeline + Send + Sync;
  type RayTracingPipeline: Send + Sync;
  type Fence : Fence + Send + Sync;
  type Queue : Queue<Self> + Send + Sync;
  //type QueryRange : Send + Sync;
  //type AccelerationStructure : AccelerationStructure + Send + Sync;
  type WSIFence : WSIFence + Send + Sync;
//  type DescriptorHeap : DescriptorHeap<Self> + Send + Sync;
}