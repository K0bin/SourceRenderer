use nalgebra::Vector3;
use sourcerenderer_core::{Vec3, graphics::{Backend as GraphicsBackend, Barrier, BindingFrequency, BufferInfo, BufferUsage, CommandBuffer, Device, MemoryUsage, PipelineBinding, ShaderType, BarrierSync, BarrierAccess, WHOLE_BUFFER}};
use sourcerenderer_core::Platform;
use std::sync::Arc;
use std::path::Path;
use std::io::Read;
use sourcerenderer_core::platform::io::IO;

use crate::renderer::{RendererScene, renderer_resources::{RendererResources, HistoryResourceEntry}};

use super::clustering::ClusteringPass;

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
  light_binning_pipeline: Arc<B::ComputePipeline>
}

impl<B: GraphicsBackend> LightBinningPass<B> {
  pub const LIGHT_BINNING_BUFFER_NAME: &'static str = "binned_lights";

  pub fn new<P: Platform>(device: &Arc<B::Device>, barriers: &mut RendererResources<B>) -> Self {
    let shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("light_binning.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("light_binning.comp.spv"))
    };
    let pipeline = device.create_compute_pipeline(&shader);

    barriers.create_buffer(Self::LIGHT_BINNING_BUFFER_NAME, &BufferInfo {
      size: std::mem::size_of::<u32>() * 16 * 9 * 24,
      usage: BufferUsage::STORAGE | BufferUsage::CONSTANT,
    }, MemoryUsage::GpuOnly, false);

    Self {
      light_binning_pipeline: pipeline
    }
  }

  pub fn execute(&mut self, cmd_buffer: &mut B::CommandBuffer, scene: &RendererScene<B>, camera_buffer: &Arc<B::Buffer>, barriers: &mut RendererResources<B>) {
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

    let light_info_buffer = cmd_buffer.upload_dynamic_data(&[setup_info], BufferUsage::STORAGE);
    let point_lights_buffer = cmd_buffer.upload_dynamic_data(&point_lights[..], BufferUsage::STORAGE);

    cmd_buffer.barrier(&[
      Barrier::BufferBarrier {
        old_sync: BarrierSync::COMPUTE_SHADER,
        new_sync: BarrierSync::COMPUTE_SHADER | BarrierSync::VERTEX_SHADER | BarrierSync::FRAGMENT_SHADER,
        old_access: BarrierAccess::STORAGE_WRITE,
        new_access: BarrierAccess::CONSTANT_READ | BarrierAccess::STORAGE_READ,
        buffer: camera_buffer,
      }
    ]);

    let light_bitmask_buffer = barriers.access_buffer(cmd_buffer, Self::LIGHT_BINNING_BUFFER_NAME, BarrierSync::COMPUTE_SHADER, BarrierAccess::STORAGE_READ | BarrierAccess::STORAGE_WRITE, HistoryResourceEntry::Current);
    let clusters_buffer = barriers.access_buffer(cmd_buffer, ClusteringPass::<B>::CLUSTERS_BUFFER_NAME, BarrierSync::COMPUTE_SHADER, BarrierAccess::STORAGE_READ, HistoryResourceEntry::Current);

    cmd_buffer.set_pipeline(PipelineBinding::Compute(&self.light_binning_pipeline));
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 0, camera_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 1, &*clusters_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 2, &light_info_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 3, &point_lights_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 4, &*light_bitmask_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.finish_binding();
    cmd_buffer.dispatch((cluster_count.x * cluster_count.y * cluster_count.z + 63) / 64, 1, 1);
    cmd_buffer.end_label();
  }
}