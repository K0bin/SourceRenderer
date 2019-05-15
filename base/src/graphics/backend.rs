use graphics::Instance;
use graphics::Adapter;
use graphics::Device;
use graphics::Surface;
use graphics::CommandPool;
use graphics::CommandBuffer;
use graphics::Queue;
use graphics::Shader;
use graphics::PipelineInfo;
use graphics::Pipeline;
use graphics::Texture;
use graphics::Buffer;
use graphics::RenderPassLayout;
use graphics::RenderPass;
use graphics::RenderTargetView;
use graphics::Swapchain;

pub trait Backend: 'static + Sized {
  type Instance: Instance<Self>;
  type Adapter: Adapter<Self>;
  type Device: Device<Self>;
  type Surface: Surface<Self>;
  type Swapchain: Swapchain<Self>;
  type CommandPool: CommandPool<Self>;
  type CommandBuffer: CommandBuffer<Self>;
  type Queue: Queue<Self>;
  type Texture: Texture<Self>;
  type Buffer: Buffer<Self>;
  type Shader: Shader<Self>;
  type Pipeline: Pipeline<Self>;
  type RenderPassLayout: RenderPassLayout<Self>;
  type RenderPass: RenderPass<Self>;
  type RenderTargetView: RenderTargetView<Self>;
}
