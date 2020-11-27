use std::sync::{Arc, Mutex};
use crate::renderer::Renderer;
use crossbeam_channel::{Sender, Receiver, TryRecvError};
use crate::renderer::command::RendererCommand;
use std::time::{SystemTime, Duration};
use crate::asset::AssetManager;
use sourcerenderer_core::{Platform, Matrix4, Vec2, Vec3, Quaternion, Vec2UI, Vec2I};
use sourcerenderer_core::graphics::{Backend, ShaderType, SampleCount, RenderPassTextureExtent, Format, PassInfo, PassType, GraphicsSubpassInfo, SubpassOutput, LoadAction, StoreAction, Device, RenderGraphTemplateInfo, GraphicsPipelineInfo, Swapchain, VertexLayoutInfo, InputAssemblerElement, InputRate, ShaderInputElement, FillMode, CullMode, FrontFace, RasterizerInfo, DepthStencilInfo, CompareFunc, StencilInfo, BlendInfo, LogicOp, AttachmentBlendInfo, BufferUsage, CommandBuffer, PipelineBinding, Viewport, Scissor, BindingFrequency, RenderGraphInfo, RenderGraph, DepthStencilOutput, RenderPassCallbacks, PassInput, ComputeOutput, ExternalOutput, ExternalProducerType, ExternalResource};
use std::collections::{HashMap};
use crate::renderer::{DrawableType, View};
use sourcerenderer_core::platform::WindowState;
use nalgebra::{Matrix3};

use crate::renderer::camera::LateLatchCamera;
use crate::renderer::passes;

pub(super) struct RendererInternal<P: Platform> {
  renderer: Arc<Renderer<P>>,
  device: Arc<<P::GraphicsBackend as Backend>::Device>,
  graph: <P::GraphicsBackend as Backend>::RenderGraph,
  swapchain: Arc<<P::GraphicsBackend as Backend>::Swapchain>,
  view: Arc<Mutex<View>>,
  sender: Sender<RendererCommand>,
  receiver: Receiver<RendererCommand>,
  simulation_tick_rate: u32,
  last_tick: SystemTime,
  primary_camera: Arc<LateLatchCamera<P::GraphicsBackend>>
}

impl<P: Platform> RendererInternal<P> {
  pub(super) fn new(
    renderer: &Arc<Renderer<P>>,
    device: &Arc<<P::GraphicsBackend as Backend>::Device>,
    swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
    asset_manager: &Arc<AssetManager<P>>,
    sender: Sender<RendererCommand>,
    receiver: Receiver<RendererCommand>,
    simulation_tick_rate: u32,
    primary_camera: &Arc<LateLatchCamera<P::GraphicsBackend>>) -> Self {

    let renderables = Arc::new(Mutex::new(View::default()));
    let graph = RendererInternal::<P>::build_graph(device, swapchain, asset_manager, &renderables, &primary_camera);

    Self {
      renderer: renderer.clone(),
      device: device.clone(),
      graph,
      swapchain: swapchain.clone(),
      view: renderables,
      sender,
      receiver,
      simulation_tick_rate,
      last_tick: SystemTime::now(),
      primary_camera: primary_camera.clone()
    }
  }

  fn build_graph(
    device: &Arc<<P::GraphicsBackend as Backend>::Device>,
    swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
    asset_manager: &Arc<AssetManager<P>>,
    renderables: &Arc<Mutex<View>>,
    primary_camera: &Arc<LateLatchCamera<P::GraphicsBackend>>)
    -> <P::GraphicsBackend as Backend>::RenderGraph {

    let passes: Vec<PassInfo> = vec![
      passes::late_latching::build_pass_template::<P::GraphicsBackend>(),
      passes::desktop::geometry::build_pass_template::<P::GraphicsBackend>()
    ];

    let external_resources = vec![
      passes::late_latching::external_resource_template()
    ];

    let graph_template = device.create_render_graph_template(&RenderGraphTemplateInfo {
      external_resources,
      passes,
      swapchain_sample_count: swapchain.sample_count(),
      swapchain_format: swapchain.format()
    });

    let mut callbacks: HashMap<String, RenderPassCallbacks<P::GraphicsBackend>> = HashMap::new();
    let (geometry_pass_name, geometry_pass_callback) = passes::desktop::geometry::build_pass::<P>(device, &graph_template, &renderables, &asset_manager);
    callbacks.insert(geometry_pass_name, geometry_pass_callback);

    let (late_latch_pass_name, late_latch_pass_callback) = passes::late_latching::build_pass::<P::GraphicsBackend>(device);
    callbacks.insert(late_latch_pass_name, late_latch_pass_callback);

    let mut external_resources = HashMap::<String, ExternalResource<P::GraphicsBackend>>::new();
    let (camera_resource_name, camera_resource) = passes::late_latching::external_resource(primary_camera);
    external_resources.insert(camera_resource_name, camera_resource);

    let graph = device.create_render_graph(&graph_template, &RenderGraphInfo {
      pass_callbacks: callbacks
    }, swapchain, Some(&external_resources));

    graph
  }

  fn receive_messages(&mut self) {
    let mut guard = self.view.lock().unwrap();

    let message_res = self.receiver.try_recv();
    if let Some(err) = message_res.as_ref().err() {
      if let TryRecvError::Disconnected = err {
        panic!("Rendering channel closed");
      }
    }
    let mut message_opt = message_res.ok();

    while message_opt.is_some() {
      let message = std::mem::replace(&mut message_opt, None).unwrap();
      match message {
        RendererCommand::EndFrame => {
          self.renderer.dec_queued_frames_counter();
          self.last_tick = SystemTime::now();

          for element in &mut guard.elements {
            if let DrawableType::Static { can_move, .. } = &element.drawable_type {
              if !*can_move {
                element.interpolated_transform = element.transform;
              } else {
                element.older_transform = element.old_transform;
                element.old_transform = element.transform;
              }
            }
          }
          guard.older_camera_transform = guard.old_camera_transform;
          guard.old_camera_transform = guard.camera_transform;
          guard.older_camera_fov = guard.old_camera_fov;
          guard.old_camera_fov = guard.camera_fov;
          break;
        }

        RendererCommand::UpdateCameraTransform { camera_transform_mat, fov } => {
          guard.camera_transform = camera_transform_mat;
          guard.camera_fov = fov;
        }

        RendererCommand::UpdateTransform { entity, transform_mat } => {
          let element = guard.elements.iter_mut()
            .find(|r| r.entity == entity);
          // TODO optimize

          if let Some(element) = element {
            element.transform = transform_mat;
          }
        }

        RendererCommand::Register(renderable) => {
          guard.elements.push(renderable);
        }

        RendererCommand::UnregisterStatic(entity) => {
          let index = guard.elements.iter()
            .position(|r| r.entity == entity);

          if let Some(index) = index {
            guard.elements.remove(index);
          }
        }
      }

      let message_res = self.receiver.try_recv();
      if let Some(err) = message_res.as_ref().err() {
        if let TryRecvError::Disconnected = err {
          panic!("Rendering channel closed");
        }
      }
      message_opt = message_res.ok();
    }
  }

  fn interpolate_drawables(&mut self, _swapchain_width: u32, _swapchain_height: u32) {
    let mut guard = self.view.lock().unwrap();

    let now = SystemTime::now();
    let delta = now.duration_since(self.last_tick).unwrap().as_millis() as f32;
    let frac = f32::max(0f32, f32::min(1f32, delta / (1000f32 / self.simulation_tick_rate as f32)));

    for element in &mut guard.elements {
      if let DrawableType::Static { can_move, .. } = &element.drawable_type {
        if *can_move {
          element.interpolated_transform = interpolate_transform_matrix(element.older_transform, element.old_transform, frac);
        }
      }
    }

    guard.interpolated_camera = interpolate_transform_matrix(guard.older_camera_transform, guard.old_camera_transform, frac);
    let (translation, _, _) = deconstruct_transform(guard.interpolated_camera);
    self.primary_camera.update_position(translation);

    guard.interpolated_camera = self.primary_camera.get_camera();
  }

  pub(super) fn render(&mut self) {
    let state = {
      let state_guard = self.renderer.window_state().lock().unwrap();
      state_guard.clone()
    };

    let mut swapchain_width = 0u32;
    let mut swapchain_height = 0u32;

    match state {
      WindowState::Minimized => {
        std::thread::sleep(Duration::new(1, 0));
        return;
      },
      WindowState::FullScreen {
        width, height
      } => {
        swapchain_width = width;
        swapchain_height = height;
      },
      WindowState::Visible {
        width, height, focussed: _focussed
      } => {
        swapchain_width = width;
        swapchain_height = height;
      },
      WindowState::Exited => {
        return;
      }
    }

    self.receive_messages();
    self.interpolate_drawables(swapchain_width, swapchain_height);

    let result = self.graph.render();
    if result.is_err() {
      self.device.wait_for_idle();

      let new_swapchain_result = <P::GraphicsBackend as Backend>::Swapchain::recreate(&self.swapchain, swapchain_width, swapchain_height);
      if new_swapchain_result.is_err() {
        return;
      }
      let new_swapchain = new_swapchain_result.unwrap();
      if new_swapchain.format() != self.swapchain.format() || new_swapchain.sample_count() != self.swapchain.sample_count() {
        panic!("Swapchain format or sample count changed. Can not recreate render graph.");
      }

      let new_graph = <P::GraphicsBackend as Backend>::RenderGraph::recreate(&self.graph, &new_swapchain);
      self.swapchain = new_swapchain;
      self.graph = new_graph;
      let _ = self.graph.render();
    }
  }
}

fn deconstruct_transform(transform_mat: Matrix4) -> (Vec3, Quaternion, Vec3) {
  let scale = Vec3::new(transform_mat.column(0).xyz().magnitude(),
                        transform_mat.column(1).xyz().magnitude(),
                        transform_mat.column(2).xyz().magnitude());
  let translation: Vec3 = transform_mat.column(3).xyz();
  let rotation = Quaternion::from_matrix(&Matrix3::<f32>::from_columns(&[
    transform_mat.column(0).xyz() / scale.x,
    transform_mat.column(1).xyz() / scale.y,
    transform_mat.column(2).xyz() / scale.z
  ]));
  (translation, rotation, scale)
}

fn interpolate_transform_matrix(from: Matrix4, to: Matrix4, frac: f32) -> Matrix4 {
  let (from_position, from_rotation, from_scale) = deconstruct_transform(from);
  let (to_position, to_rotation, to_scale) = deconstruct_transform(to);
  let position = from_position.lerp(&to_position, frac);
  let rotation: Quaternion = Quaternion::from_quaternion(from_rotation.lerp(&to_rotation, frac));
  let scale = from_scale.lerp(&to_scale, frac);

  Matrix4::new_translation(&position)
    * Matrix4::new_rotation(rotation.axis_angle().map_or(Vec3::new(0.0f32, 0.0f32, 0.0f32), |(axis, amount)| *axis * amount))
    * Matrix4::new_nonuniform_scaling(&scale)
}
