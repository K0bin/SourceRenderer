use std::mem::ManuallyDrop;
use std::sync::Arc;

use super::gpu::ComputePipeline as _;
use super::*;

pub struct GraphicsPipeline {
    pipeline: ManuallyDrop<active_gpu_backend::GraphicsPipeline>,
    destroyer: Arc<DeferredDestroyer>,
}

impl GraphicsPipeline {
    pub(super) fn new(
        device: &Arc<active_gpu_backend::Device>,
        destroyer: &Arc<DeferredDestroyer>,
        info: &active_gpu_backend::GraphicsPipelineInfo,
        name: Option<&str>,
    ) -> Self {
        let pipeline = unsafe { device.create_graphics_pipeline(info, name) };
        Self {
            pipeline: ManuallyDrop::new(pipeline),
            destroyer: destroyer.clone(),
        }
    }

    #[inline(always)]
    pub fn handle(&self) -> &active_gpu_backend::GraphicsPipeline {
        &*self.pipeline
    }
}

impl Drop for GraphicsPipeline {
    fn drop(&mut self) {
        let pipeline = unsafe { ManuallyDrop::take(&mut self.pipeline) };
        self.destroyer.destroy_graphics_pipeline(pipeline);
    }
}

pub struct MeshGraphicsPipeline {
    pipeline: ManuallyDrop<active_gpu_backend::MeshGraphicsPipeline>,
    destroyer: Arc<DeferredDestroyer>,
}

impl MeshGraphicsPipeline {
    pub(super) fn new(
        device: &Arc<active_gpu_backend::Device>,
        destroyer: &Arc<DeferredDestroyer>,
        info: &active_gpu_backend::MeshGraphicsPipelineInfo,
        name: Option<&str>,
    ) -> Self {
        let pipeline = unsafe { device.create_mesh_graphics_pipeline(info, name) };
        Self {
            pipeline: ManuallyDrop::new(pipeline),
            destroyer: destroyer.clone(),
        }
    }

    #[inline(always)]
    pub fn handle(&self) -> &active_gpu_backend::MeshGraphicsPipeline {
        &*self.pipeline
    }
}

impl Drop for MeshGraphicsPipeline {
    fn drop(&mut self) {
        let pipeline = unsafe { ManuallyDrop::take(&mut self.pipeline) };
        self.destroyer.destroy_mesh_graphics_pipeline(pipeline);
    }
}

pub struct ComputePipeline {
    pipeline: ManuallyDrop<active_gpu_backend::ComputePipeline>,
    destroyer: Arc<DeferredDestroyer>,
}

impl ComputePipeline {
    pub(super) fn new(
        device: &Arc<active_gpu_backend::Device>,
        destroyer: &Arc<DeferredDestroyer>,
        shader: &active_gpu_backend::Shader,
        name: Option<&str>,
    ) -> Self {
        let pipeline = unsafe { device.create_compute_pipeline(shader, name) };
        Self {
            pipeline: ManuallyDrop::new(pipeline),
            destroyer: destroyer.clone(),
        }
    }

    #[inline(always)]
    pub fn handle(&self) -> &active_gpu_backend::ComputePipeline {
        &*self.pipeline
    }

    #[inline(always)]
    pub fn binding_info(&self, set: BindingFrequency, slot: u32) -> Option<BindingInfo> {
        (*self.pipeline).binding_info(set, slot)
    }
}

impl Drop for ComputePipeline {
    fn drop(&mut self) {
        let pipeline = unsafe { ManuallyDrop::take(&mut self.pipeline) };
        self.destroyer.destroy_compute_pipeline(pipeline);
    }
}

pub struct RayTracingPipeline {
    pipeline: ManuallyDrop<active_gpu_backend::RayTracingPipeline>,
    destroyer: Arc<DeferredDestroyer>,
    sbt: Arc<BufferSlice>,
}

impl RayTracingPipeline {
    pub(super) fn new(
        device: &Arc<active_gpu_backend::Device>,
        destroyer: &Arc<DeferredDestroyer>,
        buffer_allocator: &BufferAllocator,
        info: &active_gpu_backend::RayTracingPipelineInfo,
        name: Option<&str>,
    ) -> Result<Self, OutOfMemoryError> {
        let sbt_size = unsafe { device.get_raytracing_pipeline_sbt_buffer_size(info) };
        let sbt = buffer_allocator.get_slice(
            &BufferInfo {
                size: sbt_size,
                usage: BufferUsage::SHADER_BINDING_TABLE,
                sharing_mode: QueueSharingMode::Exclusive,
            },
            MemoryUsage::MappableGPUMemory,
            None,
        )?;
        // TODO: Name SBT
        // TODO: Handle systems without rebar

        let pipeline =
            unsafe { device.create_raytracing_pipeline(info, sbt.handle(), sbt.offset(), name) };
        Ok(Self {
            pipeline: ManuallyDrop::new(pipeline),
            destroyer: destroyer.clone(),
            sbt,
        })
    }

    #[inline(always)]
    pub fn handle(&self) -> &active_gpu_backend::RayTracingPipeline {
        &*self.pipeline
    }

    #[inline(always)]
    pub fn sbt(&self) -> &BufferSlice {
        &self.sbt
    }
}

impl Drop for RayTracingPipeline {
    fn drop(&mut self) {
        let pipeline = unsafe { ManuallyDrop::take(&mut self.pipeline) };
        self.destroyer.destroy_raytracing_pipeline(pipeline);
    }
}
