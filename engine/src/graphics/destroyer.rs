use std::sync::Arc;
use crate::Mutex;

use sourcerenderer_core::gpu::*;

use super::*;

pub(super) struct DeferredDestroyer<B: GPUBackend> {
    inner: Mutex<DeferredDestroyerInner<B>>
}

struct DeferredDestroyerInner<B: GPUBackend> {
    current_counter: u64,
    allocations: Vec<(u64, MemoryAllocation<B::Heap>)>,
    textures: Vec<(u64, B::Texture)>,
    texture_views: Vec<(u64, B::TextureView)>,
    buffers: Vec<(u64, B::Buffer)>,
    samplers: Vec<(u64, B::Sampler)>,
    fences: Vec<(u64, B::Fence)>,
    acceleration_structures: Vec<(u64, B::AccelerationStructure)>,
    buffer_slice_refs: Vec<(u64, Arc<BufferSlice<B>>)>,
    graphics_pipelines: Vec<(u64, B::GraphicsPipeline)>,
    compute_pipelines: Vec<(u64, B::ComputePipeline)>,
    raytracing_pipelines: Vec<(u64, B::RayTracingPipeline)>,
}

// TODO: Turn into a union to save memory

impl<B: GPUBackend> DeferredDestroyer<B> {
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(
                DeferredDestroyerInner {
                    current_counter: 0u64,
                    allocations: Vec::new(),
                    textures: Vec::new(),
                    texture_views: Vec::new(),
                    buffers: Vec::new(),
                    samplers: Vec::new(),
                    fences: Vec::new(),
                    acceleration_structures: Vec::new(),
                    buffer_slice_refs: Vec::new(),
                    graphics_pipelines: Vec::new(),
                    compute_pipelines: Vec::new(),
                    raytracing_pipelines: Vec::new()
                }
            )
        }
    }

    pub fn destroy_allocation(&self, allocation: MemoryAllocation<B::Heap>) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.allocations.push((frame, allocation));
    }

    pub fn destroy_texture(&self, texture: B::Texture) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.textures.push((frame, texture));
    }

    pub fn destroy_texture_view(&self, texture_view: B::TextureView) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.texture_views.push((frame, texture_view));
    }

    pub fn destroy_buffer(&self, buffer: B::Buffer) {
        let mut guard: crate::MutexGuard<'_, DeferredDestroyerInner<B>> = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.buffers.push((frame, buffer));
    }

    pub fn destroy_sampler(&self, sampler: B::Sampler) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.samplers.push((frame, sampler));
    }

    pub fn destroy_fence(&self, fence: B::Fence) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.fences.push((frame, fence));
    }

    pub fn destroy_acceleration_structure(&self, acceleration_structure: B::AccelerationStructure) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.acceleration_structures.push((frame, acceleration_structure));
    }

    pub fn destroy_buffer_slice_ref(&self, buffer_slice_ref: Arc<BufferSlice<B>>) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.buffer_slice_refs.push((frame, buffer_slice_ref));
    }

    pub fn destroy_graphics_pipeline(&self, pipeline: B::GraphicsPipeline) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.graphics_pipelines.push((frame, pipeline));
    }

    pub fn destroy_compute_pipeline(&self, pipeline: B::ComputePipeline) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.compute_pipelines.push((frame, pipeline));
    }

    pub fn destroy_raytracing_pipeline(&self, pipeline: B::RayTracingPipeline) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.raytracing_pipelines.push((frame, pipeline));
    }

    pub fn set_counter(&self, counter: u64) {
        let mut guard = self.inner.lock().unwrap();
        guard.current_counter = counter;
    }

    pub fn destroy_unused(&self, counter: u64) {
        let mut guard = self.inner.lock().unwrap();
        guard.acceleration_structures.retain(|(resource_counter, _)| *resource_counter > counter);
        guard.buffer_slice_refs.retain(|(resource_counter, _)| *resource_counter > counter);
        guard.textures.retain(|(resource_counter, _)| *resource_counter > counter);
        guard.texture_views.retain(|(resource_counter, _)| *resource_counter > counter);
        guard.buffers.retain(|(resource_counter, _)| *resource_counter > counter);
        guard.samplers.retain(|(resource_counter, _)| *resource_counter > counter);
        guard.fences.retain(|(resource_counter, _)| *resource_counter > counter);
        guard.allocations.retain(|(resource_counter, _)| *resource_counter > counter);
        guard.graphics_pipelines.retain(|(resource_counter, _)| *resource_counter > counter);
        guard.compute_pipelines.retain(|(resource_counter, _)| *resource_counter > counter);
        guard.raytracing_pipelines.retain(|(resource_counter, _)| *resource_counter > counter);
    }
}

impl<B: GPUBackend> Drop for DeferredDestroyer<B> {
    fn drop(&mut self) {
        let guard = self.inner.lock().unwrap();
        assert!(guard.acceleration_structures.is_empty());
        assert!(guard.buffer_slice_refs.is_empty());
        assert!(guard.textures.is_empty());
        assert!(guard.texture_views.is_empty());
        assert!(guard.buffers.is_empty());
        assert!(guard.samplers.is_empty());
        assert!(guard.fences.is_empty());
        assert!(guard.allocations.is_empty());
        assert!(guard.graphics_pipelines.is_empty());
        assert!(guard.compute_pipelines.is_empty());
        assert!(guard.raytracing_pipelines.is_empty());
    }
}
