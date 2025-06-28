use std::sync::Arc;

use super::*;
use crate::{
    Mutex,
    MutexGuard,
};

pub(super) struct DeferredDestroyer {
    inner: Mutex<DeferredDestroyerInner>,
}

struct DeferredDestroyerInner {
    current_counter: u64,
    allocations: Vec<(u64, MemoryAllocation<active_gpu_backend::Heap>)>,
    textures: Vec<(u64, active_gpu_backend::Texture)>,
    texture_views: Vec<(u64, active_gpu_backend::TextureView)>,
    buffers: Vec<(u64, active_gpu_backend::Buffer)>,
    samplers: Vec<(u64, active_gpu_backend::Sampler)>,
    fences: Vec<(u64, active_gpu_backend::Fence)>,
    acceleration_structures: Vec<(u64, active_gpu_backend::AccelerationStructure)>,
    buffer_slice_refs: Vec<(u64, Arc<BufferSlice>)>,
    graphics_pipelines: Vec<(u64, active_gpu_backend::GraphicsPipeline)>,
    mesh_graphics_pipelines: Vec<(u64, active_gpu_backend::MeshGraphicsPipeline)>,
    compute_pipelines: Vec<(u64, active_gpu_backend::ComputePipeline)>,
    raytracing_pipelines: Vec<(u64, active_gpu_backend::RayTracingPipeline)>,
    buffer_allocations: Vec<(u64, Allocation<BufferAndAllocation>)>,
    query_pools: Vec<(u64, active_gpu_backend::QueryPool)>,
    split_barriers: Vec<(u64, active_gpu_backend::SplitBarrier)>,
}

// TODO: Turn into a union to save memory

impl DeferredDestroyer {
    pub(super) fn new() -> Self {
        Self {
            inner: Mutex::new(DeferredDestroyerInner {
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
                mesh_graphics_pipelines: Vec::new(),
                compute_pipelines: Vec::new(),
                raytracing_pipelines: Vec::new(),
                buffer_allocations: Vec::new(),
                query_pools: Vec::new(),
                split_barriers: Vec::new(),
            }),
        }
    }

    pub(super) fn destroy_allocation(
        &self,
        allocation: MemoryAllocation<active_gpu_backend::Heap>,
    ) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.allocations.push((frame, allocation));
    }

    pub(super) fn destroy_texture(&self, texture: active_gpu_backend::Texture) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.textures.push((frame, texture));
    }

    pub(super) fn destroy_texture_view(&self, texture_view: active_gpu_backend::TextureView) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.texture_views.push((frame, texture_view));
    }

    pub(super) fn destroy_buffer(&self, buffer: active_gpu_backend::Buffer) {
        let mut guard: crate::MutexGuard<'_, DeferredDestroyerInner> = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.buffers.push((frame, buffer));
    }

    pub(super) fn destroy_sampler(&self, sampler: active_gpu_backend::Sampler) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.samplers.push((frame, sampler));
    }

    pub(super) fn destroy_fence(&self, fence: active_gpu_backend::Fence) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.fences.push((frame, fence));
    }

    pub(super) fn destroy_acceleration_structure(
        &self,
        acceleration_structure: active_gpu_backend::AccelerationStructure,
    ) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard
            .acceleration_structures
            .push((frame, acceleration_structure));
    }

    pub(super) fn destroy_graphics_pipeline(&self, pipeline: active_gpu_backend::GraphicsPipeline) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.graphics_pipelines.push((frame, pipeline));
    }

    pub(super) fn destroy_mesh_graphics_pipeline(
        &self,
        pipeline: active_gpu_backend::MeshGraphicsPipeline,
    ) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.mesh_graphics_pipelines.push((frame, pipeline));
    }

    pub(super) fn destroy_compute_pipeline(&self, pipeline: active_gpu_backend::ComputePipeline) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.compute_pipelines.push((frame, pipeline));
    }

    pub(super) fn destroy_raytracing_pipeline(
        &self,
        pipeline: active_gpu_backend::RayTracingPipeline,
    ) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.raytracing_pipelines.push((frame, pipeline));
    }

    pub(super) fn destroy_query_pool(&self, query_pool: active_gpu_backend::QueryPool) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.query_pools.push((frame, query_pool));
    }

    pub(super) fn destroy_buffer_allocation(
        &self,
        buffer_allocation: Allocation<BufferAndAllocation>,
    ) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.buffer_allocations.push((frame, buffer_allocation));
    }

    pub(super) fn destroy_split_barrier(&self, split_barrier: active_gpu_backend::SplitBarrier) {
        let mut guard = self.inner.lock().unwrap();
        let frame = guard.current_counter;
        guard.split_barriers.push((frame, split_barrier));
    }

    pub(super) fn set_counter(&self, counter: u64) {
        let mut guard = self.inner.lock().unwrap();
        assert!(guard.current_counter <= counter);
        guard.current_counter = counter;
    }

    pub(super) fn destroy_unused(&self, counter: u64) {
        let mut guard = self.inner.lock().unwrap();
        Self::destroy_unused_locked(&mut guard, counter);
    }

    pub(super) unsafe fn destroy_all(&self) {
        let mut guard = self.inner.lock().unwrap();
        let counter = guard.current_counter;
        Self::destroy_unused_locked(&mut guard, counter);
    }

    fn destroy_unused_locked(guard: &mut MutexGuard<'_, DeferredDestroyerInner>, counter: u64) {
        assert!(guard.current_counter >= counter);
        guard
            .acceleration_structures
            .retain(|(resource_counter, _)| *resource_counter > counter);
        guard
            .buffer_slice_refs
            .retain(|(resource_counter, _)| *resource_counter > counter);
        guard
            .textures
            .retain(|(resource_counter, _)| *resource_counter > counter);
        guard
            .texture_views
            .retain(|(resource_counter, _)| *resource_counter > counter);
        guard
            .buffers
            .retain(|(resource_counter, _)| *resource_counter > counter);
        guard
            .samplers
            .retain(|(resource_counter, _)| *resource_counter > counter);
        guard
            .fences
            .retain(|(resource_counter, _)| *resource_counter > counter);
        guard
            .allocations
            .retain(|(resource_counter, _)| *resource_counter > counter);
        guard
            .graphics_pipelines
            .retain(|(resource_counter, _)| *resource_counter > counter);
        guard
            .mesh_graphics_pipelines
            .retain(|(resource_counter, _)| *resource_counter > counter);
        guard
            .compute_pipelines
            .retain(|(resource_counter, _)| *resource_counter > counter);
        guard
            .raytracing_pipelines
            .retain(|(resource_counter, _)| *resource_counter > counter);
        guard
            .query_pools
            .retain(|(resource_counter, _)| *resource_counter > counter);
        guard
            .buffer_allocations
            .retain(|(resource_counter, _)| *resource_counter > counter);
        guard
            .split_barriers
            .retain(|(resource_counter, _)| *resource_counter > counter);
    }
}

impl Drop for DeferredDestroyer {
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
        assert!(guard.mesh_graphics_pipelines.is_empty());
        assert!(guard.compute_pipelines.is_empty());
        assert!(guard.raytracing_pipelines.is_empty());
        assert!(guard.query_pools.is_empty());
        assert!(guard.buffer_allocations.is_empty());
    }
}
