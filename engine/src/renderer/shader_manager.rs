use std::{collections::HashMap, sync::{Arc, Mutex}, hash::Hash};
use std::sync::Condvar;

use smallvec::SmallVec;
use sourcerenderer_core::{graphics::{Backend, RasterizerInfo, DepthStencilInfo, PrimitiveType, ShaderInputElement, InputAssemblerElement, LogicOp, AttachmentBlendInfo, VertexLayoutInfo, BlendInfo, GraphicsPipelineInfo as ActualGraphicsPipelineInfo, ShaderType, AttachmentInfo, DepthStencilAttachmentRef, OutputAttachmentRef, AttachmentRef, Device, RenderPassInfo, SubpassInfo, RayTracingPipelineInfo as ActualRayTracingPipelineInfo}, Platform};

use crate::asset::{AssetManager, AssetType, AssetLoadPriority};

#[derive(Debug, Hash, Eq, PartialEq, Clone)]
struct StoredVertexLayoutInfo {
  pub shader_inputs: SmallVec<[ShaderInputElement; 4]>,
  pub input_assembler: SmallVec<[InputAssemblerElement; 4]>
}

impl<'a> PartialEq<VertexLayoutInfo<'a>> for StoredVertexLayoutInfo {
  fn eq(&self, other: &VertexLayoutInfo<'a>) -> bool {
    &self.shader_inputs[..] == other.shader_inputs
      && &self.input_assembler[..] == other.input_assembler
  }
}

#[derive(Debug, Clone, PartialEq)]
struct StoredBlendInfo {
  pub alpha_to_coverage_enabled: bool,
  pub logic_op_enabled: bool,
  pub logic_op: LogicOp,
  pub attachments: SmallVec<[AttachmentBlendInfo; 4]>,
  pub constants: [f32; 4]
}

impl<'a> PartialEq<BlendInfo<'a>> for StoredBlendInfo {
  fn eq(&self, other: &BlendInfo<'a>) -> bool {
    self.alpha_to_coverage_enabled == other.alpha_to_coverage_enabled
      && self.logic_op_enabled == other.logic_op_enabled
      && self.logic_op == other.logic_op
      && &self.attachments[..] == other.attachments
      && self.constants == other.constants
  }
}

#[derive(Debug, Clone)]
struct StoredGraphicsPipelineInfo {
  pub vs: String,
  pub fs: Option<String>,
  pub vertex_layout: StoredVertexLayoutInfo,
  pub rasterizer: RasterizerInfo,
  pub depth_stencil: DepthStencilInfo,
  pub blend: StoredBlendInfo,
  pub primitive_type: PrimitiveType
}

#[derive(Debug, Clone)]
pub struct GraphicsPipelineInfo<'a> {
  pub vs: &'a str,
  pub fs: Option<&'a str>,
  pub vertex_layout: VertexLayoutInfo<'a>,
  pub rasterizer: RasterizerInfo,
  pub depth_stencil: DepthStencilInfo,
  pub blend: BlendInfo<'a>,
  pub primitive_type: PrimitiveType
}

struct StoredGraphicsPipeline<B: Backend> {
  info: StoredGraphicsPipelineInfo,
  pipeline: Arc<B::GraphicsPipeline>,
}

#[derive(Debug, Clone)]
pub struct StoredSubpassInfo {
  pub input_attachments: SmallVec<[AttachmentRef; 4]>,
  pub output_color_attachments: SmallVec<[OutputAttachmentRef; 4]>,
  pub depth_stencil_attachment: Option<DepthStencilAttachmentRef>
}

#[derive(Debug, Clone)]
struct StoredRenderPassInfo {
  attachments: SmallVec<[AttachmentInfo; 4]>,
  subpasses: SmallVec<[StoredSubpassInfo; 4]>
}

#[derive(Debug, Clone)]
struct GraphicsCompileTask {
  info: StoredGraphicsPipelineInfo,
  renderpass: StoredRenderPassInfo,
  subpass: u32
}

struct GraphicsPipeline<B: Backend> {
  task: GraphicsCompileTask,
  pipeline: Arc<B::GraphicsPipeline>
}

struct ComputePipeline<B: Backend> {
  path: String,
  pipeline: Arc<B::ComputePipeline>
}

struct RayTracingPipeline<B: Backend> {
  task: StoredRayTracingPipelineInfo,
  pipeline: Arc<B::RayTracingPipeline>
}

#[derive(Debug, Clone)]
pub struct RayTracingPipelineInfo<'a> {
  pub ray_gen_shader: &'a str,
  pub closest_hit_shaders: &'a [&'a str],
  pub miss_shaders: &'a [&'a str],
}

#[derive(Debug, Clone)]
struct StoredRayTracingPipelineInfo {
  ray_gen_shader: String,
  closest_hit_shaders: SmallVec<[String; 4]>,
  miss_shaders: SmallVec<[String; 1]>
}

pub struct ShaderManager<P: Platform> {
  device: Arc<<P::GraphicsBackend as Backend>::Device>,
  asset_manager: Arc<AssetManager<P>>,
  inner: Arc<Mutex<ShaderManagerInner<P::GraphicsBackend>>>,
  next_pipeline_handle_index: u64,
  condvar: Arc<Condvar>
}

struct ShaderManagerInner<B: Backend> {
  shaders: HashMap<String, Arc<B::Shader>>,
  remaining_graphics_compilations: HashMap<GraphicsPipelineHandle, GraphicsCompileTask>,
  remaining_compute_compilations: HashMap<ComputePipelineHandle, String>,
  remaining_rt_compilations: HashMap<RayTracingPipelineHandle, StoredRayTracingPipelineInfo>,
  graphics_pipelines: HashMap<GraphicsPipelineHandle, GraphicsPipeline<B>>,
  compute_pipelines: HashMap<ComputePipelineHandle, ComputePipeline<B>>,
  rt_pipelines: HashMap<RayTracingPipelineHandle, RayTracingPipeline<B>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GraphicsPipelineHandle {
  index: u64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ComputePipelineHandle {
  index: u64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RayTracingPipelineHandle {
  index: u64
}

impl<P: Platform> ShaderManager<P> {
  pub fn new(device: &Arc<<P::GraphicsBackend as Backend>::Device>, asset_manager: &Arc<AssetManager<P>>) -> Self {
    Self {
      device: device.clone(),
      asset_manager: asset_manager.clone(),
      inner: Arc::new(Mutex::new(ShaderManagerInner {
        shaders: HashMap::new(),
        remaining_graphics_compilations: HashMap::new(),
        remaining_compute_compilations: HashMap::new(),
        remaining_rt_compilations: HashMap::new(),
        graphics_pipelines: HashMap::new(),
        compute_pipelines: HashMap::new(),
        rt_pipelines: HashMap::new(),
      })),
      next_pipeline_handle_index: 1u64,
      condvar: Arc::new(Condvar::new())
    }
  }

  pub fn request_graphics_pipeline(&mut self, info: &GraphicsPipelineInfo, renderpass_info: &RenderPassInfo, subpass_index: u32) -> GraphicsPipelineHandle {
    let handle = GraphicsPipelineHandle { index: self.next_pipeline_handle_index };
    self.next_pipeline_handle_index += 1;

    let stored_input_layout = StoredVertexLayoutInfo {
      shader_inputs: info.vertex_layout.shader_inputs.iter().cloned().collect(),
      input_assembler: info.vertex_layout.input_assembler.iter().cloned().collect(),
    };

    let stored_blend = StoredBlendInfo {
      alpha_to_coverage_enabled: info.blend.alpha_to_coverage_enabled,
      logic_op_enabled: info.blend.logic_op_enabled,
      logic_op: info.blend.logic_op,
      attachments: info.blend.attachments.iter().cloned().collect(),
      constants: info.blend.constants.clone()
    };

    let stored = StoredGraphicsPipelineInfo {
      vs: info.vs.to_string(),
      fs: info.fs.map(|s| s.to_string()),
      vertex_layout: stored_input_layout,
      rasterizer: info.rasterizer.clone(),
      depth_stencil: info.depth_stencil.clone(),
      blend: stored_blend,
      primitive_type: info.primitive_type
    };

    let rp = StoredRenderPassInfo {
      attachments: renderpass_info.attachments.iter().cloned().collect(),
      subpasses: renderpass_info.subpasses.iter().map(|subpass|
        StoredSubpassInfo {
          input_attachments: subpass.input_attachments.iter().cloned().collect(),
          output_color_attachments: subpass.output_color_attachments.iter().cloned().collect(),
          depth_stencil_attachment: subpass.depth_stencil_attachment.clone()
        }
      ).collect(),
    };

    let mut inner = self.inner.lock().unwrap();
    inner.remaining_graphics_compilations.insert(handle, GraphicsCompileTask {
      info: stored,
      renderpass: rp,
      subpass: subpass_index
    });

    self.asset_manager.request_asset(info.vs, AssetType::Shader, AssetLoadPriority::High);
    if let Some(fs) = info.fs.as_ref() {
      self.asset_manager.request_asset(fs, AssetType::Shader, AssetLoadPriority::High);
    }

    handle
  }

  pub fn request_compute_pipeline(&mut self, path: &str) -> ComputePipelineHandle {
    let handle = ComputePipelineHandle { index: self.next_pipeline_handle_index };
    self.next_pipeline_handle_index += 1;

    let mut inner = self.inner.lock().unwrap();
    inner.remaining_compute_compilations.insert(handle, path.to_string());

    self.asset_manager.request_asset(path, AssetType::Shader, AssetLoadPriority::High);

    handle
  }

  pub fn request_ray_tracing_pipeline(&mut self, info: &RayTracingPipelineInfo) -> RayTracingPipelineHandle {
    let handle = RayTracingPipelineHandle { index: self.next_pipeline_handle_index };
    self.next_pipeline_handle_index += 1;

    let stored = StoredRayTracingPipelineInfo {
      closest_hit_shaders: info.closest_hit_shaders.iter().map(|s| s.to_string()).collect(),
      miss_shaders: info.miss_shaders.iter().map(|s| s.to_string()).collect(),
      ray_gen_shader: info.ray_gen_shader.to_string()
    };

    let mut inner = self.inner.lock().unwrap();
    inner.remaining_rt_compilations.insert(handle, stored);

    for shader in info.closest_hit_shaders {
      self.asset_manager.request_asset(shader, AssetType::Shader, AssetLoadPriority::High);
    }
    for shader in info.miss_shaders {
      self.asset_manager.request_asset(shader, AssetType::Shader, AssetLoadPriority::High);
    }
    self.asset_manager.request_asset(info.ray_gen_shader, AssetType::Shader, AssetLoadPriority::High);

    handle
  }

  pub fn add_shader(&mut self, path: &str, shader_bytecode: Box<[u8]>) {
    let mut graphics_pipelines_using_shader = SmallVec::<[GraphicsPipelineHandle; 1]>::new();
    let mut compute_pipelines_using_shader = SmallVec::<[ComputePipelineHandle; 1]>::new();
    let mut rt_pipelines_using_shader = SmallVec::<[RayTracingPipelineHandle; 1]>::new();
    let mut ready_graphics_handles = SmallVec::<[GraphicsPipelineHandle; 1]>::new();
    let mut ready_compute_handles = SmallVec::<[ComputePipelineHandle; 1]>::new();
    let mut ready_rt_handles = SmallVec::<[RayTracingPipelineHandle; 1]>::new();
    let shader_type: ShaderType;

    {
      let mut shader_type_opt = Option::<ShaderType>::None;
      let mut inner = self.inner.lock().unwrap();

      {
        // Find all pipelines that use this shader and queue new compile tasks for those.
        // This is done because add_shader will get called when a shader has changed on disk, so we need to load
        // all remaining shaders of a pipeline and recompile it.

        for (handle, pipeline) in &inner.graphics_pipelines {
          let c = &pipeline.task;
          if c.info.vs == path {
            shader_type_opt = Some(ShaderType::VertexShader);
          } else if c.info.fs.as_ref().map(|fs| fs == path).unwrap_or_default() {
            shader_type_opt = Some(ShaderType::FragmentShader);
          } else {
            continue;
          }
          graphics_pipelines_using_shader.push(*handle);
        }
        if shader_type_opt.is_none() {
          for (handle, pipeline) in &inner.compute_pipelines {
            let c_path = &pipeline.path;
            if c_path == path {
              shader_type_opt = Some(ShaderType::ComputeShader);
            } else {
              continue;
            }
            compute_pipelines_using_shader.push(*handle);
          }
        }
        if shader_type_opt.is_none() {
          for (handle, pipeline) in &inner.rt_pipelines {
            let c = &pipeline.task;
            if path == &c.ray_gen_shader {
              shader_type_opt = Some(ShaderType::RayGen);
            } else if c.closest_hit_shaders.iter().any(|s| s == path) {
              shader_type_opt = Some(ShaderType::RayClosestHit);
            } else if c.miss_shaders.iter().any(|s| s == path) {
              shader_type_opt = Some(ShaderType::RayMiss);
            } else {
              continue;
            }
            rt_pipelines_using_shader.push(*handle);
          }
        }
        for handle in graphics_pipelines_using_shader {
          let task = inner.graphics_pipelines.get(&handle).unwrap().task.clone();
          match shader_type_opt.unwrap() {
            ShaderType::VertexShader => {
              if let Some(fs) = task.info.fs.as_ref() {
                self.asset_manager.request_asset(fs, AssetType::Shader, AssetLoadPriority::Low);
              }
            },
            ShaderType::FragmentShader => { self.asset_manager.request_asset(&task.info.vs, AssetType::Shader, AssetLoadPriority::Low); },
            _ => unreachable!()
          }
          inner.remaining_graphics_compilations.insert(handle, task);
        }
        for handle in compute_pipelines_using_shader {
          let task = inner.compute_pipelines.get(&handle).unwrap().clone().path.clone();
          inner.remaining_compute_compilations.insert(handle, task);
        }
        for handle in rt_pipelines_using_shader {
          let task = inner.rt_pipelines.get(&handle).unwrap().task.clone();
          if &task.ray_gen_shader != path {
            self.asset_manager.request_asset(&task.ray_gen_shader, AssetType::Shader, AssetLoadPriority::Low);
          }
          for shader in task.closest_hit_shaders {
            if &shader == path {
              continue;
            }
            self.asset_manager.request_asset(&shader, AssetType::Shader, AssetLoadPriority::Low);
          }
          for shader in task.miss_shaders {
            if &shader == path {
              continue;
            }
            self.asset_manager.request_asset(&shader, AssetType::Shader, AssetLoadPriority::Low);
          }
        }
      }

      // Find all remaining compilations using the new shader and queue pipeline compilations for those.
      for (handle, c) in &inner.remaining_graphics_compilations {
        if c.info.vs == path {
          assert!(shader_type_opt.is_none() || shader_type_opt.unwrap() == ShaderType::VertexShader);
          shader_type_opt = Some(ShaderType::VertexShader);
        } else if c.info.fs.as_ref().map(|fs| fs == path).unwrap_or_default() {
          assert!(shader_type_opt.is_none() || shader_type_opt.unwrap() == ShaderType::FragmentShader);
          shader_type_opt = Some(ShaderType::FragmentShader);
        } else {
          continue;
        }

        if (inner.shaders.contains_key(&c.info.vs) || shader_type_opt.unwrap() == ShaderType::VertexShader)
          && (c.info.fs.as_ref().map(|fs| inner.shaders.contains_key(fs)).unwrap_or(true) || shader_type_opt.unwrap() == ShaderType::FragmentShader) {
          ready_graphics_handles.push(*handle);
        }
      }
      for (handle, c_path) in &inner.remaining_compute_compilations {
        if path == c_path {
          shader_type_opt = Some(ShaderType::ComputeShader);
          ready_compute_handles.push(*handle);
        }
      }
      for (handle, c) in &inner.remaining_rt_compilations {
        if path == &c.ray_gen_shader {
          assert!(shader_type_opt.is_none() || shader_type_opt.unwrap() == ShaderType::RayClosestHit);
          shader_type_opt = Some(ShaderType::RayGen);
        } else if c.closest_hit_shaders.iter().any(|s| s == path) {
          assert!(shader_type_opt.is_none() || shader_type_opt.unwrap() == ShaderType::RayClosestHit);
          shader_type_opt = Some(ShaderType::RayClosestHit);
        } else if c.miss_shaders.iter().any(|s| s == path) {
          assert!(shader_type_opt.is_none() || shader_type_opt.unwrap() == ShaderType::RayMiss);
          shader_type_opt = Some(ShaderType::RayMiss);
        } else {
          continue;
        }
        if (&c.ray_gen_shader == path || inner.shaders.contains_key(&c.ray_gen_shader))
          && (c.closest_hit_shaders.iter().all(|s| s == path || inner.shaders.contains_key(s)))
          && (c.miss_shaders.iter().all(|s| s == path || inner.shaders.contains_key(s))) {
          ready_rt_handles.push(*handle);
        }
      }
      shader_type = shader_type_opt.unwrap();
      inner.shaders.insert(path.to_string(), self.device.create_shader(shader_type, &shader_bytecode[..], Some(path)));
    }

    if ready_graphics_handles.is_empty() && ready_compute_handles.is_empty() && ready_rt_handles.is_empty() {
      return;
    }

    let c_device = self.device.clone();
    let c_inner = self.inner.clone();
    let c_condvar = self.condvar.clone();
    rayon::spawn(move || {
      for handle in ready_graphics_handles.drain(..) {
        // It's important that the actual compilation happens outside of the mutex.

        let (task, vs, fs) = {
          let mut inner = c_inner.lock().unwrap();
          let task = inner.remaining_graphics_compilations
            .remove(&handle)
            .unwrap();
          let vs = inner.shaders
            .get(&task.info.vs)
            .cloned()
            .unwrap();
          let fs = task.info.fs
            .as_ref()
            .map(|fs|
              inner.shaders
                .get(fs)
                .cloned()
                .unwrap()
            );
          (task, vs, fs)
        };

        let subpasses: SmallVec<[SubpassInfo; 4]> = task.renderpass.subpasses.iter().map(|s| SubpassInfo {
          input_attachments: &s.input_attachments[..],
          output_color_attachments: &s.output_color_attachments[..],
          depth_stencil_attachment: s.depth_stencil_attachment.clone()
        }).collect();

        let rp = RenderPassInfo {
          attachments: &task.renderpass.attachments[..],
          subpasses: &subpasses[..]
        };

        let input_layout = VertexLayoutInfo {
          shader_inputs: &task.info.vertex_layout.shader_inputs[..],
          input_assembler: &task.info.vertex_layout.input_assembler[..],
        };

        let blend_info = BlendInfo {
          alpha_to_coverage_enabled: task.info.blend.alpha_to_coverage_enabled,
          logic_op_enabled: task.info.blend.logic_op_enabled,
          logic_op: task.info.blend.logic_op,
          attachments: &task.info.blend.attachments[..],
          constants: task.info.blend.constants,
        };

        let info = ActualGraphicsPipelineInfo {
          vs: &vs,
          fs: fs.as_ref(),
          vertex_layout: input_layout,
          rasterizer: task.info.rasterizer.clone(),
          depth_stencil: task.info.depth_stencil.clone(),
          blend: blend_info,
          primitive_type: task.info.primitive_type,
        };

        let pipeline = c_device.create_graphics_pipeline(&info, &rp, task.subpass, None);
        std::mem::drop(info);
        std::mem::drop(rp);
        std::mem::drop(subpasses);
        let mut inner = c_inner.lock().unwrap();
        if let Some(existing_pipeline) = inner.graphics_pipelines.get_mut(&handle) {
          existing_pipeline.pipeline = pipeline;
        } else {
          inner.graphics_pipelines.insert(handle, GraphicsPipeline::<P::GraphicsBackend> {
            pipeline,
            task
          });
        }
      }

      for handle in ready_compute_handles.drain(..) {
        // It's important that the actual compilation happens outside of the mutex.

        let (path, shader) = {
          let mut inner = c_inner.lock().unwrap();
          let path = inner.remaining_compute_compilations.remove(&handle).unwrap();
          let shader = inner.shaders.get(&path).cloned().unwrap();
          (path, shader)
        };
        let pipeline = c_device.create_compute_pipeline(&shader, None);
        let mut inner = c_inner.lock().unwrap();
        if let Some(existing_pipeline) = inner.compute_pipelines.get_mut(&handle) {
          existing_pipeline.pipeline = pipeline;
        } else {
          inner.compute_pipelines.insert(handle, ComputePipeline::<P::GraphicsBackend> {
            path,
            pipeline,
          });
        }
      }

      for handle in ready_rt_handles {
        let task: StoredRayTracingPipelineInfo;
        let ray_gen_shader: Arc<<P::GraphicsBackend as Backend>::Shader>;
        let mut closest_hit_shaders = SmallVec::<[Arc<<P::GraphicsBackend as Backend>::Shader>; 4]>::new();
        let mut miss_shaders = SmallVec::<[Arc<<P::GraphicsBackend as Backend>::Shader>; 1]>::new();
        {
          let mut inner = c_inner.lock().unwrap();
          task = inner.remaining_rt_compilations
            .remove(&handle)
            .unwrap();
          ray_gen_shader = inner.shaders.get(&task.ray_gen_shader).cloned().unwrap();
          for handle in &task.closest_hit_shaders {
            closest_hit_shaders.push(inner.shaders.get(handle).cloned().unwrap());
          }
          for handle in &task.miss_shaders {
            miss_shaders.push(inner.shaders.get(handle).cloned().unwrap());
          }
        }
        let closest_hit_shader_refs: SmallVec<[&Arc<<P::GraphicsBackend as Backend>::Shader>; 4]> = closest_hit_shaders.iter().map(|s| s).collect();
        let miss_shaders_refs: SmallVec<[&Arc<<P::GraphicsBackend as Backend>::Shader>; 1]> = miss_shaders.iter().map(|s| s).collect();
        let info = ActualRayTracingPipelineInfo::<P::GraphicsBackend> {
          ray_gen_shader: &ray_gen_shader,
          closest_hit_shaders: &closest_hit_shader_refs[..],
          miss_shaders: &miss_shaders_refs[..],
        };
        let pipeline = c_device.create_raytracing_pipeline(&info);
        let mut inner = c_inner.lock().unwrap();
        if let Some(existing_pipeline) = inner.rt_pipelines.get_mut(&handle) {
          existing_pipeline.pipeline = pipeline;
        } else {
          inner.rt_pipelines.insert(handle, RayTracingPipeline::<P::GraphicsBackend> {
            pipeline,
            task
          });
        }
      }

      {
        // Storing shader bytecode does nothing but waste memory,
        // Clear it once we're idle.
        /*let mut inner = c_inner.lock().unwrap();
        if inner.remaining_graphics_compilations.is_empty() && inner.remaining_compute_compilations.is_empty() && inner.remaining_rt_compilations.is_empty() {
          // TODO: Unloading it breaks reloading in the Asset Manager
          for shader_path in inner.shaders.keys() {
            c_asset_manager.notify_unloaded(shader_path);
          }
          inner.shaders.clear();
        }*/
      }
      c_condvar.notify_all();
    });
  }

  pub fn try_get_graphics_pipeline(&self, handle: GraphicsPipelineHandle) -> Option<Arc<<P::GraphicsBackend as Backend>::GraphicsPipeline>> {
    let inner = self.inner.lock().unwrap();
    inner.graphics_pipelines.get(&handle).map(|p| p.pipeline.clone())
  }

  pub fn get_graphics_pipeline(&self, handle: GraphicsPipelineHandle) -> Arc<<P::GraphicsBackend as Backend>::GraphicsPipeline> {
    let inner = self.inner.lock().unwrap();
    let pipeline_opt = inner.graphics_pipelines.get(&handle);
    if let Some(pipeline) = pipeline_opt {
      return pipeline.pipeline.clone();
    }
    assert!(inner.remaining_graphics_compilations.contains_key(&handle));
    let inner = self.condvar.wait_while(inner, |inner| !inner.graphics_pipelines.contains_key(&handle)).unwrap();
    inner.graphics_pipelines.get(&handle).unwrap().pipeline.clone()
  }

  pub fn try_get_compute_pipeline(&self, handle: ComputePipelineHandle) -> Option<Arc<<P::GraphicsBackend as Backend>::ComputePipeline>> {
    let inner = self.inner.lock().unwrap();
    inner.compute_pipelines.get(&handle).map(|p| p.pipeline.clone())
  }

  pub fn get_compute_pipeline(&self, handle: ComputePipelineHandle) -> Arc<<P::GraphicsBackend as Backend>::ComputePipeline> {
    let inner = self.inner.lock().unwrap();
    let pipeline_opt = inner.compute_pipelines.get(&handle);
    if let Some(pipeline) = pipeline_opt {
      return pipeline.pipeline.clone();
    }
    assert!(inner.remaining_compute_compilations.contains_key(&handle));
    let inner = self.condvar.wait_while(inner, |inner| !inner.compute_pipelines.contains_key(&handle)).unwrap();
    inner.compute_pipelines.get(&handle).unwrap().pipeline.clone()
  }

  pub fn try_get_ray_tracing_pipeline(&self, handle: RayTracingPipelineHandle) -> Option<Arc<<P::GraphicsBackend as Backend>::RayTracingPipeline>> {
    let inner = self.inner.lock().unwrap();
    inner.rt_pipelines.get(&handle).map(|p| p.pipeline.clone())
  }

  pub fn get_ray_tracing_pipeline(&self, handle: RayTracingPipelineHandle) -> Arc<<P::GraphicsBackend as Backend>::RayTracingPipeline> {
    let inner = self.inner.lock().unwrap();
    let pipeline_opt = inner.rt_pipelines.get(&handle);
    if let Some(pipeline) = pipeline_opt {
      return pipeline.pipeline.clone();
    }
    assert!(inner.remaining_rt_compilations.contains_key(&handle));
    let inner = self.condvar.wait_while(inner, |inner| !inner.rt_pipelines.contains_key(&handle)).unwrap();
    inner.rt_pipelines.get(&handle).unwrap().pipeline.clone()
  }
}
