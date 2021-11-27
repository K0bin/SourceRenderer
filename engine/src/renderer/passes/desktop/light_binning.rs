use nalgebra::Vector3;
use sourcerenderer_core::{Vec3, graphics::{Backend as GraphicsBackend, Barrier, BindingFrequency, BufferInfo, BufferUsage, CommandBuffer, Device, MemoryUsage, PipelineBinding, ShaderType}};
use sourcerenderer_core::Platform;
use std::sync::Arc;
use std::path::Path;
use std::io::Read;
use sourcerenderer_core::platform::io::IO;

use crate::renderer::RendererScene;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SetupInfo {
  cluster_count: u32,
  point_light_count: u32
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CullingPointLight {
  position: Vec3,
  radius: f32
}

const LIGHT_CUTOFF: f32 = 0.05f32;

pub struct LightBinningPass<B: GraphicsBackend> {
  light_bitmask_buffer: Arc<B::Buffer>,
  light_binning_pipeline: Arc<B::ComputePipeline>
}

impl<B: GraphicsBackend> LightBinningPass<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>) -> Self {
    let buffer = device.create_buffer(&BufferInfo {
      size: std::mem::size_of::<u32>() * 16 * 9 * 24,
      usage: BufferUsage::COMPUTE_SHADER_STORAGE_WRITE | BufferUsage::FRAGMENT_SHADER_STORAGE_READ | BufferUsage::FRAGMENT_SHADER_CONSTANT
    }, MemoryUsage::GpuOnly, Some("LightBitmaskBuffer"));

    let shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("light_binning.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("light_binning.comp.spv"))
    };  
    let pipeline = device.create_compute_pipeline(&shader);

    Self {
      light_bitmask_buffer: buffer,
      light_binning_pipeline: pipeline
    }
  }

  pub fn execute(&mut self, cmd_buffer: &mut B::CommandBuffer, scene: &RendererScene<B>, clusters_buffer: &Arc<B::Buffer>, camera_buffer: &Arc<B::Buffer>) {
    cmd_buffer.begin_label("Light binning");
    let cluster_count = Vector3::<u32>::new(16, 9, 24);
    let setup_info = SetupInfo {
      point_light_count: scene.point_lights().len() as u32,
      cluster_count: cluster_count.x * cluster_count.y * cluster_count.z
    };
    let point_lights: Vec<CullingPointLight> = scene.point_lights().iter().map(|l| CullingPointLight {
      position: l.position,
      radius: (l.intensity / LIGHT_CUTOFF).sqrt()
    }).collect();

    let light_info_buffer = cmd_buffer.upload_dynamic_data(&[setup_info], BufferUsage::COMPUTE_SHADER_STORAGE_READ);
    let point_lights_buffer = cmd_buffer.upload_dynamic_data(&point_lights[..], BufferUsage::COMPUTE_SHADER_STORAGE_READ);

    cmd_buffer.barrier(&[
      Barrier::BufferBarrier {
        old_primary_usage: BufferUsage::COMPUTE_SHADER_STORAGE_WRITE,
        new_primary_usage: BufferUsage::COMPUTE_SHADER_CONSTANT,
        old_usages: BufferUsage::COMPUTE_SHADER_STORAGE_WRITE,
        new_usages: BufferUsage::COMPUTE_SHADER_CONSTANT | BufferUsage::VERTEX_SHADER_CONSTANT | BufferUsage::FRAGMENT_SHADER_CONSTANT
          | BufferUsage::COMPUTE_SHADER_STORAGE_READ | BufferUsage::VERTEX_SHADER_STORAGE_READ | BufferUsage::FRAGMENT_SHADER_STORAGE_READ,
        buffer: camera_buffer,
      },
      Barrier::BufferBarrier {
        old_primary_usage: BufferUsage::COMPUTE_SHADER_STORAGE_WRITE,
        new_primary_usage: BufferUsage::COMPUTE_SHADER_STORAGE_READ,
        old_usages: BufferUsage::COMPUTE_SHADER_STORAGE_WRITE,
        new_usages: BufferUsage::COMPUTE_SHADER_STORAGE_READ,
        buffer: clusters_buffer,
      },
      Barrier::BufferBarrier {
        old_primary_usage: BufferUsage::FRAGMENT_SHADER_STORAGE_READ,
        new_primary_usage: BufferUsage::COMPUTE_SHADER_STORAGE_WRITE,
        old_usages: BufferUsage::FRAGMENT_SHADER_STORAGE_READ,
        new_usages: BufferUsage::COMPUTE_SHADER_STORAGE_WRITE,
        buffer: &self.light_bitmask_buffer,
      }
    ]);

    cmd_buffer.set_pipeline(PipelineBinding::Compute(&self.light_binning_pipeline));
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 0, camera_buffer);
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 1, clusters_buffer);
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 2, &light_info_buffer);
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 3, &point_lights_buffer);
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 4, &self.light_bitmask_buffer);
    cmd_buffer.finish_binding();
    cmd_buffer.dispatch((cluster_count.x * cluster_count.y * cluster_count.z + 63) / 64, 1, 1);
    cmd_buffer.end_label();
  }

  pub fn light_bitmask_buffer(&self) -> &Arc<B::Buffer> {
    &self.light_bitmask_buffer
  }
}