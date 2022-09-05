use nalgebra::Vector3;
use sourcerenderer_core::{Vec2UI, Vec4, graphics::{Backend as GraphicsBackend, BindingFrequency, BufferInfo, BufferUsage, CommandBuffer, MemoryUsage, PipelineBinding, BarrierSync, BarrierAccess, WHOLE_BUFFER, Buffer}};
use sourcerenderer_core::Platform;
use std::sync::Arc;

use crate::renderer::{drawable::View, renderer_resources::{RendererResources, HistoryResourceEntry}, shader_manager::{ComputePipelineHandle, ShaderManager}};

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct ShaderScreenToView {
  tile_size: Vec2UI,
  rt_dimensions: Vec2UI,
  z_near: f32,
  z_far: f32
}

pub struct ClusteringPass {
  pipeline: ComputePipelineHandle
}

impl ClusteringPass {
  pub const CLUSTERS_BUFFER_NAME: &'static str = "clusters";

  pub fn new<P: Platform>(barriers: &mut RendererResources<P::GraphicsBackend>, shader_manager: &mut ShaderManager<P>) -> Self {
    let pipeline = shader_manager.request_compute_pipeline("shaders/clustering.comp.spv");

    barriers.create_buffer(Self::CLUSTERS_BUFFER_NAME, &BufferInfo {
      size: std::mem::size_of::<Vec4>() * 2 * 16 * 9 * 24,
      usage: BufferUsage::STORAGE,
  }, MemoryUsage::VRAM, false);

    Self {
      pipeline,
    }
  }

  pub fn execute<P: Platform>(
    &mut self,
    command_buffer: &mut <P::GraphicsBackend as GraphicsBackend>::CommandBuffer,
    rt_size: Vec2UI,
    view_ref: &View,
    camera_buffer: &Arc<<P::GraphicsBackend as GraphicsBackend>::Buffer>,
    barriers: &mut RendererResources<P::GraphicsBackend>,
    shader_manager: &ShaderManager<P>
  ) {
    command_buffer.begin_label("Clustering pass");

    let cluster_count = Vector3::<u32>::new(16, 9, 24);
    let screen_to_view = ShaderScreenToView {
      tile_size: Vec2UI::new(((rt_size.x as f32) / cluster_count.x as f32).ceil() as u32, ((rt_size.y as f32) / cluster_count.y as f32).ceil() as u32),
      rt_dimensions: rt_size,
      z_near: view_ref.near_plane,
      z_far: view_ref.far_plane
    };

    let screen_to_view_cbuffer = command_buffer.upload_dynamic_data(&[screen_to_view], BufferUsage::STORAGE);
    let clusters_buffer = barriers.access_buffer(command_buffer, Self::CLUSTERS_BUFFER_NAME, BarrierSync::COMPUTE_SHADER, BarrierAccess::STORAGE_WRITE, HistoryResourceEntry::Current);
    debug_assert!(clusters_buffer.info().size as u32 >= cluster_count.x * cluster_count.y * cluster_count.z * 2 * std::mem::size_of::<Vec4>() as u32);
    debug_assert_eq!(cluster_count.x % 8, 0);
    debug_assert_eq!(cluster_count.y % 1, 0);
    debug_assert_eq!(cluster_count.z % 8, 0); // Ensure the cluster count fits with the work group size
    let pipeline = shader_manager.get_compute_pipeline(self.pipeline);
    command_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));
    command_buffer.bind_storage_buffer(BindingFrequency::VeryFrequent, 0, &*clusters_buffer, 0, WHOLE_BUFFER);
    command_buffer.bind_storage_buffer(BindingFrequency::VeryFrequent, 1, &screen_to_view_cbuffer, 0, WHOLE_BUFFER);
    command_buffer.bind_uniform_buffer(BindingFrequency::VeryFrequent, 2, camera_buffer, 0, WHOLE_BUFFER);
    command_buffer.finish_binding();
    command_buffer.dispatch((cluster_count.x + 7) / 8, cluster_count.y, (cluster_count.z + 7) / 8);

    command_buffer.end_label();
  }

  pub fn cluster_count(&self) -> Vector3<u32> {
    Vector3::<u32>::new(16, 9, 24)
  }
}
