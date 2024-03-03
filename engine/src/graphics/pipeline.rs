use std::mem::ManuallyDrop;
use std::sync::Arc;
use sourcerenderer_core::gpu::{self, ComputePipeline as _};
use super::*;

pub struct GraphicsPipeline<B: GPUBackend> {
    pipeline: ManuallyDrop<B::GraphicsPipeline>,
    destroyer: Arc<DeferredDestroyer<B>>
}

impl<B: GPUBackend> GraphicsPipeline<B> {
    pub(super) fn new(device: &Arc<B::Device>, destroyer: &Arc<DeferredDestroyer<B>>, info: &GraphicsPipelineInfo<B>, render_pass_info: &gpu::RenderPassInfo, subpass: u32, name: Option<&str>) -> Self {
        let pipeline = unsafe {
            device.create_graphics_pipeline(info, render_pass_info, subpass, name)
        };
        Self {
            pipeline: ManuallyDrop::new(pipeline),
            destroyer: destroyer.clone()
        }
    }

    pub fn handle(&self) -> &B::GraphicsPipeline {
        &*self.pipeline
    }
}

impl<B: GPUBackend> Drop for GraphicsPipeline<B> {
    fn drop(&mut self) {
        let pipeline = unsafe { ManuallyDrop::take(&mut self.pipeline) };
        self.destroyer.destroy_graphics_pipeline(pipeline);
    }
}

pub struct ComputePipeline<B: GPUBackend> {
    pipeline: ManuallyDrop<B::ComputePipeline>,
    destroyer: Arc<DeferredDestroyer<B>>
}

impl<B: GPUBackend> ComputePipeline<B> {
    pub(super) fn new(device: &Arc<B::Device>, destroyer: &Arc<DeferredDestroyer<B>>, shader: &B::Shader, name: Option<&str>) -> Self {
        let pipeline = unsafe {
            device.create_compute_pipeline(shader, name)
        };
        Self {
            pipeline: ManuallyDrop::new(pipeline),
            destroyer: destroyer.clone()
        }
    }

    pub fn handle(&self) -> &B::ComputePipeline {
        &*self.pipeline
    }

    pub fn binding_info(&self, set: BindingFrequency, slot: u32) -> Option<BindingInfo> {
        (*self.pipeline).binding_info(set, slot)
    }
}

impl<B: GPUBackend> Drop for ComputePipeline<B> {
    fn drop(&mut self) {
        let pipeline = unsafe { ManuallyDrop::take(&mut self.pipeline) };
        self.destroyer.destroy_compute_pipeline(pipeline);
    }
}

pub struct RayTracingPipeline<B: GPUBackend> {
    pipeline: ManuallyDrop<B::RayTracingPipeline>,
    destroyer: Arc<DeferredDestroyer<B>>,
    sbt: Arc<BufferSlice<B>>
}

impl<B: GPUBackend> RayTracingPipeline<B> {
    pub(super) fn new(device: &Arc<B::Device>, destroyer: &Arc<DeferredDestroyer<B>>, buffer_allocator: &BufferAllocator<B>, info: &gpu::RayTracingPipelineInfo<B>, name: Option<&str>) -> Result<Self, OutOfMemoryError> {
        let sbt_size = unsafe { device.get_raytracing_pipeline_sbt_buffer_size(info) };
        let sbt = buffer_allocator.get_slice(&BufferInfo {
            size: sbt_size,
            usage: gpu::BufferUsage::SHADER_BINDING_TABLE,
            sharing_mode: QueueSharingMode::Exclusive
        }, MemoryUsage::MappableGPUMemory, None)?;
        // TODO: Name SBT
        // TODO: Handle systems without rebar

        let pipeline = unsafe {
            device.create_raytracing_pipeline(info, sbt.handle(), sbt.offset())
        };
        Ok(Self {
            pipeline: ManuallyDrop::new(pipeline),
            destroyer: destroyer.clone(),
            sbt
        })
    }

    pub fn handle(&self) -> &B::RayTracingPipeline {
        &*self.pipeline
    }

    pub fn sbt(&self) -> &BufferSlice<B> {
        &self.sbt
    }
}

impl<B: GPUBackend> Drop for RayTracingPipeline<B> {
    fn drop(&mut self) {
        let pipeline = unsafe { ManuallyDrop::take(&mut self.pipeline) };
        self.destroyer.destroy_raytracing_pipeline(pipeline);
    }
}

