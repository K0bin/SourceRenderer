use super::*;

use std::hash::Hash;

#[cfg(not(feature = "single_thread_gpu_api"))]
pub trait GPUBackend: 'static + Sized {
  type Instance: Instance<Self> + Send + Sync;
  type Adapter: Adapter<Self> + Send + Sync;
  type Device: Device<Self> + Send + Sync;
  type Surface: Send + Sync + PartialEq + Eq; // TODO: Add trait with associated type and to_transferable function that returns the offscreen canvas JsValue;
  type Swapchain: Swapchain<Self> + Send + Sync;
  type CommandPool: CommandPool<Self> + Send;
  type CommandBuffer: CommandBuffer<Self> + Send;
  type Texture: Texture + PartialEq;
  type TextureView: TextureView + PartialEq;
  type Sampler: Send + Sync;
  type Buffer: Buffer + Send + Sync + PartialEq;
  type Shader: Shader + Hash + Eq + PartialEq + Send + Sync;
  type GraphicsPipeline: Send + Sync;
  type MeshGraphicsPipeline: Send + Sync;
  type ComputePipeline: ComputePipeline + Send + Sync;
  type RayTracingPipeline: Send + Sync;
  type Fence : Fence + Send + Sync;
  type Queue : Queue<Self> + Send + Sync;
  type Heap : Heap<Self>;
  type QueryPool : QueryPool + Send + Sync;
  type AccelerationStructure : AccelerationStructure + Send + Sync;

  fn name() -> &'static str;
}

#[cfg(feature = "single_thread_gpu_api")]
pub trait GPUBackend: 'static + Sized {
  type Instance: Instance<Self>;
  type Adapter: Adapter<Self>;
  type Device: Device<Self>;
  type Surface: PartialEq + Eq;
  type Swapchain: Swapchain<Self>;
  type CommandPool: CommandPool<Self>;
  type CommandBuffer: CommandBuffer<Self>;
  type Texture: Texture + PartialEq;
  type TextureView: TextureView + PartialEq;
  type Sampler;
  type Buffer: Buffer + PartialEq;
  type Shader: Shader + Hash + Eq + PartialEq;
  type GraphicsPipeline;
  type MeshGraphicsPipeline;
  type ComputePipeline: ComputePipeline;
  type RayTracingPipeline;
  type Fence : Fence;
  type Queue : Queue<Self>;
  type Heap : Heap<Self>;
  type QueryPool : QueryPool;
  type AccelerationStructure : AccelerationStructure;

  fn name() -> &'static str;
}
