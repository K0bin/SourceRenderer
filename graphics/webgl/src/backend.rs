use crate::{WebGLAdapter, WebGLBuffer, WebGLCommandBuffer, WebGLCommandSubmission, WebGLComputePipeline, WebGLDevice, WebGLFence, WebGLGraphicsPipeline, WebGLInstance, WebGLShader, WebGLSurface, WebGLSwapchain, WebGLTexture, WebGLTextureView, command::WebGLQueue, sync::WebGLSemaphore, texture::WebGLSampler, rt::WebGLAccelerationStructureStub};

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum WebGLBackend {}

impl Backend for WebGLBackend {
  type Instance = WebGLInstance;
  type Adapter = WebGLAdapter;
  type Device = WebGLDevice;
  type Surface = WebGLSurface;
  type Swapchain = WebGLSwapchain;
  type CommandBuffer = WebGLCommandBuffer;
  type CommandBufferSubmission = WebGLCommandSubmission;
  type Texture = WebGLTexture;
  type TextureView = WebGLTextureView;
  type Buffer = WebGLBuffer;
  type Shader = WebGLShader;
  type GraphicsPipeline = WebGLGraphicsPipeline;
  type ComputePipeline = WebGLComputePipeline;
  type Fence = WebGLFence;
  type Semaphore = WebGLSemaphore;
  type Sampler = WebGLSampler;
  type Queue = WebGLQueue;
  type QueryRange = ();
  type AccelerationStructure = WebGLAccelerationStructureStub;
  type RayTracingPipeline = ();
}
