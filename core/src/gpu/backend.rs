use super::{send_sync_bounds::GPUMaybeSend, *};

use std::hash::Hash;

pub trait GPUBackend: 'static + Sized {
    type Instance: Instance<Self> + GPUMaybeSend + GPUMaybeSync;
    type Adapter: Adapter<Self> + GPUMaybeSend + GPUMaybeSync;
    type Device: Device<Self> + GPUMaybeSend + GPUMaybeSync;
    type Surface: Surface<Self> + GPUMaybeSend + GPUMaybeSync + PartialEq + Eq;
    type Swapchain: Swapchain<Self> + GPUMaybeSend + GPUMaybeSync;
    type CommandPool: CommandPool<Self> + GPUMaybeSend;
    type CommandBuffer: CommandBuffer<Self> + GPUMaybeSend;
    type Texture: Texture + PartialEq;
    type TextureView: TextureView + PartialEq;
    type Sampler: GPUMaybeSend + GPUMaybeSync;
    type Buffer: Buffer + GPUMaybeSend + GPUMaybeSync + PartialEq;
    type Shader: Shader + Hash + Eq + PartialEq + GPUMaybeSend + GPUMaybeSync;
    type GraphicsPipeline: GPUMaybeSend + GPUMaybeSync;
    type MeshGraphicsPipeline: GPUMaybeSend + GPUMaybeSync;
    type ComputePipeline: ComputePipeline + GPUMaybeSend + GPUMaybeSync;
    type RayTracingPipeline: GPUMaybeSend + GPUMaybeSync;
    type Fence: Fence + GPUMaybeSend + GPUMaybeSync;
    type Queue: Queue<Self> + GPUMaybeSend + GPUMaybeSync;
    type Heap: Heap<Self>;
    type QueryPool: QueryPool + GPUMaybeSend + GPUMaybeSync;
    type AccelerationStructure: AccelerationStructure + GPUMaybeSend + GPUMaybeSync;

    fn name() -> &'static str;
}
