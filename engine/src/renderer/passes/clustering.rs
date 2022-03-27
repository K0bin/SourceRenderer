use nalgebra::Vector3;
use sourcerenderer_core::{Vec2UI, Vec4, graphics::{Backend as GraphicsBackend, BindingFrequency, BufferInfo, BufferUsage, CommandBuffer, Device, MemoryUsage, PipelineBinding, ShaderType, BarrierSync, BarrierAccess, WHOLE_BUFFER}, atomic_refcell::AtomicRef};
use sourcerenderer_core::Platform;
use std::sync::Arc;
use std::path::Path;
use std::io::Read;
use sourcerenderer_core::platform::io::IO;

use crate::renderer::{drawable::View, renderer_resources::{RendererResources, HistoryResourceEntry}};

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct ShaderScreenToView {
  tile_size: Vec2UI,
  rt_dimensions: Vec2UI,
  z_near: f32,
  z_far: f32
}

pub struct ClusteringPass<B: GraphicsBackend> {
  pipeline: Arc<B::ComputePipeline>
}

impl<B: GraphicsBackend> ClusteringPass<B> {
  pub const CLUSTERS_BUFFER_NAME: &'static str = "clusters";

  pub fn new<P: Platform>(device: &Arc<B::Device>, barriers: &mut RendererResources<B>) -> Self {
    let clustering_shader = {
    let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("clustering.comp.spv"))).unwrap();
    let mut bytes: Vec<u8> = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("clustering.comp.spv"))
    };
    let clustering_pipeline = device.create_compute_pipeline(&clustering_shader);

    barriers.create_buffer(Self::CLUSTERS_BUFFER_NAME, &BufferInfo {
      size: std::mem::size_of::<Vec4>() * 2 * 16 * 9 * 24,
      usage: BufferUsage::STORAGE,
  }, MemoryUsage::GpuOnly, false);

    Self {
      pipeline: clustering_pipeline,
    }
  }

  pub fn execute(
    &mut self,
    command_buffer: &mut B::CommandBuffer,
    rt_size: Vec2UI,
    view_ref: &AtomicRef<View>,
    camera_buffer: &Arc<B::Buffer>,
    barriers: &mut RendererResources<B>
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
    command_buffer.set_pipeline(PipelineBinding::Compute(&self.pipeline));
    command_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 0, &*clusters_buffer, 0, WHOLE_BUFFER);
    command_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 1, &screen_to_view_cbuffer, 0, WHOLE_BUFFER);
    command_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 2, camera_buffer, 0, WHOLE_BUFFER);
    command_buffer.finish_binding();
    command_buffer.dispatch(cluster_count.x, cluster_count.y, cluster_count.z);

    command_buffer.end_label();
  }
}
