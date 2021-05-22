use nalgebra::{Vector3, Vector4};
use sourcerenderer_core::{Vec2UI, Vec4, atomic_refcell::{AtomicRef, AtomicRefCell}, graphics::{Backend as GraphicsBackend, BindingFrequency, BufferUsage, CommandBuffer, Device, InputUsage, MemoryUsage, Output, PassInfo, PassInput, PassType, PipelineBinding, PipelineStage, RenderPassCallbacks, Shader, ShaderType}};
use sourcerenderer_core::{Matrix4, Platform};
use std::{borrow::Borrow, cmp::max, sync::Arc};
use std::path::Path;
use std::io::Read;
use sourcerenderer_core::platform::io::IO;

use crate::renderer::drawable::View;

const PASS_NAME: &str = "Clustering";
pub(super) const OUTPUT_CLUSTERS: &str = "Clusters";

pub(crate) fn build_pass_template<B: GraphicsBackend>() -> PassInfo {
  PassInfo {
    name: PASS_NAME.to_string(),
    pass_type: PassType::Compute {
      inputs: vec![
        PassInput {
          name: super::super::late_latching::OUTPUT_CAMERA.to_string(),
          usage: InputUsage::Storage,
          is_history: false,
          stage: PipelineStage::ComputeShader
        }
      ],
      outputs: vec![
        Output::Buffer {
          name: OUTPUT_CLUSTERS.to_string(),
          format: None,
          size: std::mem::size_of::<Vec4>() as u32 * 2 * 16 * 9 * 24,
          clear: false
        }
      ]
    }
  }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct ShaderScreenToView {
  tile_size: Vector4<u32>,
  rt_dimensions: Vec2UI,
  z_near: f32,
  z_far: f32
}

pub(crate) fn build_pass<P: Platform>(device: &Arc<<P::GraphicsBackend as GraphicsBackend>::Device>, view: &Arc<AtomicRefCell<View>>) -> (String, RenderPassCallbacks<P::GraphicsBackend>) {
  let copy_camera_compute_shader = {
    let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("clustering.comp.spv"))).unwrap();
    let mut bytes: Vec<u8> = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    device.create_shader(ShaderType::ComputeShader, &bytes, Some("clustering.comp.spv"))
  };

  let c_view = view.clone();
  let c_device = device.clone();

  let copy_camera_pipeline = device.create_compute_pipeline(&copy_camera_compute_shader);
  (PASS_NAME.to_string(), RenderPassCallbacks::Regular(
    vec![
      Arc::new(move |command_buffer_a, graph_resources, _frame_counter| {
        let view_ref: AtomicRef<View> = c_view.as_ref().borrow();

        let tile_count = Vector3::<u32>::new(16, 9, 24);
        let size = graph_resources.texture_dimensions(super::geometry::OUTPUT_IMAGE).unwrap();
        let tile_size = max(((size.width as f32) / tile_count.x as f32).ceil() as u32, ((size.height as f32) / tile_count.y as f32).ceil() as u32);
        let screen_to_view = ShaderScreenToView {
          tile_size: Vector4::<u32>::new(0, 0, 0, tile_size),
          rt_dimensions: Vec2UI::new(size.width, size.height),
          z_near: view_ref.near_plane,
          z_far: view_ref.far_plane
        };

        let screen_to_view_cbuffer = c_device.upload_data(&[screen_to_view], MemoryUsage::CpuOnly, BufferUsage::STORAGE);

        let command_buffer = command_buffer_a as &mut <P::GraphicsBackend as GraphicsBackend>::CommandBuffer;
        command_buffer.set_pipeline(PipelineBinding::Compute(&copy_camera_pipeline));
        command_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 0, graph_resources.get_buffer(OUTPUT_CLUSTERS, false).expect("Failed to get graph resource"));
        command_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 1, &screen_to_view_cbuffer);
        command_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 2, graph_resources.get_buffer(super::super::late_latching::OUTPUT_CAMERA, false).expect("Failed to get graph resource"));
        command_buffer.finish_binding();
        command_buffer.dispatch(tile_count.x, tile_count.y, tile_count.z);
      })
    ]
  ))
}
