use nalgebra::Vector3;
use sourcerenderer_core::{Vec2UI, Vec4, graphics::{Backend as GraphicsBackend, Barrier, BindingFrequency, BufferInfo, BufferUsage, CommandBuffer, Device, MemoryUsage, PipelineBinding, ShaderType}};
use sourcerenderer_core::Platform;
use std::sync::Arc;
use std::path::Path;
use std::io::Read;
use sourcerenderer_core::platform::io::IO;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct ShaderScreenToView {
  tile_size: Vec2UI,
  rt_dimensions: Vec2UI,
  z_near: f32,
  z_far: f32
}

pub struct ClusteringPass<B: GraphicsBackend> {
  pipeline: Arc<B::ComputePipeline>,
  clusters_buffer: Arc<B::Buffer>
}

impl<B: GraphicsBackend> ClusteringPass<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>) -> Self {
    let clustering_shader = {
    let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("clustering.comp.spv"))).unwrap();
    let mut bytes: Vec<u8> = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("clustering.comp.spv"))
    };
    let clustering_pipeline = device.create_compute_pipeline(&clustering_shader);
    let buffer = device.create_buffer(&BufferInfo {
        size: std::mem::size_of::<Vec4>() * 2 * 16 * 9 * 24,
        usage: BufferUsage::COMPUTE_SHADER_STORAGE_WRITE | BufferUsage::COMPUTE_SHADER_STORAGE_READ,
    }, MemoryUsage::GpuOnly, Some("Clusters"));

    Self {
      pipeline: clustering_pipeline,
      clusters_buffer: buffer
    }
  }

  pub fn execute(
    &mut self,
    command_buffer: &mut B::CommandBuffer,
    rt_size: Vec2UI,
    near_plane: f32,
    far_plane: f32,
    camera_buffer: &Arc<B::Buffer>
  ) {
    let cluster_count = Vector3::<u32>::new(16, 9, 24);
    let screen_to_view = ShaderScreenToView {
      tile_size: Vec2UI::new(((rt_size.x as f32) / cluster_count.x as f32).ceil() as u32, ((rt_size.y as f32) / cluster_count.y as f32).ceil() as u32),
      rt_dimensions: rt_size,
      z_near: near_plane,
      z_far: far_plane
    };

    let screen_to_view_cbuffer = command_buffer.upload_dynamic_data(&[screen_to_view], BufferUsage::COMPUTE_SHADER_STORAGE_READ);
    command_buffer.barrier(&[
      Barrier::BufferBarrier {
        old_primary_usage: BufferUsage::COMPUTE_SHADER_STORAGE_READ,
        new_primary_usage: BufferUsage::COMPUTE_SHADER_STORAGE_WRITE,
        old_usages: BufferUsage::COMPUTE_SHADER_STORAGE_READ,
        new_usages: BufferUsage::COMPUTE_SHADER_STORAGE_WRITE,
        buffer: &self.clusters_buffer
      },
      Barrier::BufferBarrier {
        old_primary_usage: BufferUsage::COMPUTE_SHADER_CONSTANT,
        new_primary_usage: BufferUsage::COMPUTE_SHADER_STORAGE_READ,
        old_usages: BufferUsage::COMPUTE_SHADER_CONSTANT,
        new_usages: BufferUsage::COMPUTE_SHADER_STORAGE_READ
          | BufferUsage::COMPUTE_SHADER_CONSTANT
          | BufferUsage::VERTEX_SHADER_CONSTANT
          | BufferUsage::FRAGMENT_SHADER_CONSTANT,
        buffer: camera_buffer
      }
    ]);

    command_buffer.set_pipeline(PipelineBinding::Compute(&self.pipeline));
    command_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 0, &self.clusters_buffer);
    command_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 1, &screen_to_view_cbuffer);
    command_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 2, camera_buffer);
    command_buffer.finish_binding();
    command_buffer.dispatch(cluster_count.x, cluster_count.y, cluster_count.z);
  }

  pub fn clusters_buffer(&self) -> &Arc<B::Buffer> {
    &self.clusters_buffer
  }
}
