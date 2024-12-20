use sourcerenderer_core::gpu::GPUBackend;

use crate::{adapter::WebGPUAdapter, buffer::WebGPUBuffer, command::WebGPUCommandBuffer, pipeline::{WebGPUComputePipeline, WebGPUGraphicsPipeline, WebGPUShader}, queue::{WebGPUFence, WebGPUQueue}, sampler::WebGPUSampler, stubs::{WebGPUAccelerationStructure, WebGPUHeap}, surface::WebGPUSurface, swapchain::WebGPUSwapchain, texture::{WebGPUTexture, WebGPUTextureView}, WebGPUInstance};

pub struct WebGPUBackend {

}

impl GPUBackend for WebGPUBackend {
    type Instance = WebGPUInstance;
    type Adapter = WebGPUAdapter;
    type Device;
    type Surface = WebGPUSurface;
    type Swapchain = WebGPUSwapchain;
    type CommandPool;
    type CommandBuffer = WebGPUCommandBuffer;
    type Texture = WebGPUTexture;
    type TextureView = WebGPUTextureView;
    type Sampler = WebGPUSampler;
    type Buffer = WebGPUBuffer;
    type Shader = WebGPUShader;
    type GraphicsPipeline = WebGPUGraphicsPipeline;
    type ComputePipeline = WebGPUComputePipeline;
    type RayTracingPipeline = ();
    type Fence = WebGPUFence;
    type Queue = WebGPUQueue;
    type Heap = WebGPUHeap;
    type AccelerationStructure = WebGPUAccelerationStructure;
}
