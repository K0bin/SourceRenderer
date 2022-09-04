use std::{collections::HashMap, sync::{Arc, Mutex}, hash::{Hash, Hasher}};
use std::sync::Condvar;

use log::info;
use smallvec::SmallVec;
use sourcerenderer_core::{graphics::{Backend, RasterizerInfo, DepthStencilInfo, PrimitiveType, ShaderInputElement, InputAssemblerElement, LogicOp, AttachmentBlendInfo, VertexLayoutInfo, BlendInfo, GraphicsPipelineInfo as ActualGraphicsPipelineInfo, ShaderType, AttachmentInfo, DepthStencilAttachmentRef, OutputAttachmentRef, AttachmentRef, Device, RenderPassInfo, SubpassInfo, Shader}, Platform, atomic_refcell::AtomicRefCell};

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

impl<'a> Hash for GraphicsPipelineInfo<'a> {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.vs.hash(state);
    self.fs.hash(state);
    self.vertex_layout.hash(state);
    self.rasterizer.hash(state);
    self.depth_stencil.hash(state);

    self.blend.alpha_to_coverage_enabled.hash(state);
    self.blend.logic_op_enabled.hash(state);
    self.blend.logic_op.hash(state);
    self.blend.attachments.hash(state);
    let uint_constants: [u32; 4] = unsafe { [
      std::mem::transmute(self.blend.constants[0]),
      std::mem::transmute(self.blend.constants[1]),
      std::mem::transmute(self.blend.constants[2]),
      std::mem::transmute(self.blend.constants[3]),
    ] };
    uint_constants.hash(state);

    self.primitive_type.hash(state);
  }
}

impl<'a> PartialEq<GraphicsPipelineInfo<'a>> for StoredGraphicsPipelineInfo {
  fn eq(&self, other: &GraphicsPipelineInfo<'a>) -> bool {
    self.vs == other.vs
      && self.fs.as_ref().map(|s| s.as_str()) == other.fs
      && self.vertex_layout == other.vertex_layout
      && self.blend == other.blend
      && self.rasterizer == other.rasterizer
      && self.depth_stencil == other.depth_stencil
      && self.primitive_type == other.primitive_type
  }
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

pub struct ShaderManager<P: Platform> {
  device: Arc<<P::GraphicsBackend as Backend>::Device>,
  asset_manager: Arc<AssetManager<P>>,
  inner: Arc<Mutex<ShaderManagerInner<P::GraphicsBackend>>>,
  next_pipeline_handle: PipelineHandle,
  condvar: Arc<Condvar>
}

struct ShaderManagerInner<B: Backend> {
  shaders: HashMap<String, Arc<B::Shader>>,
  remaining_graphics_compilations: HashMap<PipelineHandle, GraphicsCompileTask>,
  remaining_compute_compilations: HashMap<PipelineHandle, String>,
  graphics_pipelines: HashMap<PipelineHandle, GraphicsPipeline<B>>,
  compute_pipelines: HashMap<PipelineHandle, ComputePipeline<B>>,
}

pub type PipelineHandle = u64;

impl<P: Platform> ShaderManager<P> {
  pub fn new(device: &Arc<<P::GraphicsBackend as Backend>::Device>, asset_manager: &Arc<AssetManager<P>>) -> Self {
    Self {
      device: device.clone(),
      asset_manager: asset_manager.clone(),
      inner: Arc::new(Mutex::new(ShaderManagerInner {
        shaders: HashMap::new(),
        remaining_graphics_compilations: HashMap::new(),
        remaining_compute_compilations: HashMap::new(),
        graphics_pipelines: HashMap::new(),
        compute_pipelines: HashMap::new(),
      })),
      next_pipeline_handle: 1u64,
      condvar: Arc::new(Condvar::new())
    }
  }

  pub fn request_graphics_pipeline(&mut self, info: &GraphicsPipelineInfo, renderpass_info: &RenderPassInfo, subpass_index: u32) -> PipelineHandle {
    let handle = self.next_pipeline_handle;
    self.next_pipeline_handle += 1;

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

  pub fn request_compute_pipeline(&mut self, path: &str) -> PipelineHandle {
    let handle = self.next_pipeline_handle;
    self.next_pipeline_handle += 1;

    let mut inner = self.inner.lock().unwrap();
    inner.remaining_compute_compilations.insert(handle, path.to_string());

    self.asset_manager.request_asset(path, AssetType::Shader, AssetLoadPriority::High);

    handle
  }

  pub fn add_shader(&mut self, path: &str, shader_bytecode: Box<[u8]>) {
    let mut graphics_pipelines_using_shader = SmallVec::<[PipelineHandle; 4]>::new();
    let mut compute_pipelines_using_shader = SmallVec::<[PipelineHandle; 4]>::new();
    let mut ready_handles = SmallVec::<[PipelineHandle; 4]>::new();
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
          ready_handles.push(*handle);
        }
      }
      for (handle, c_path) in &inner.remaining_compute_compilations {
        if path == c_path {
          shader_type_opt = Some(ShaderType::ComputeShader);
          ready_handles.push(*handle);
        }
      }
      shader_type = shader_type_opt.unwrap();
      inner.shaders.insert(path.to_string(), self.device.create_shader(shader_type, &shader_bytecode[..], Some(path)));
    }

    if ready_handles.is_empty() {
      return;
    }

    let c_device = self.device.clone();
    let c_inner = self.inner.clone();
    let c_condvar = self.condvar.clone();
    rayon::spawn(move || {
      for handle in ready_handles.drain(..) {
        // It's important that the actual compilation happens outside of the mutex.

        if shader_type == ShaderType::ComputeShader {
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
          continue;
        }

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

      {
        // Storing shader bytecode does nothing but waste memory,
        // Clear it once we're idle.
        /*let mut inner = c_inner.lock().unwrap();
        if inner.remaining_graphics_compilations.is_empty() && inner.remaining_compute_compilations.is_empty() {
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

  pub fn try_get_graphics_pipeline(&self, handle: PipelineHandle) -> Option<Arc<<P::GraphicsBackend as Backend>::GraphicsPipeline>> {
    let inner = self.inner.lock().unwrap();
    inner.graphics_pipelines.get(&handle).map(|p| p.pipeline.clone())
  }

  pub fn get_graphics_pipeline(&self, handle: PipelineHandle) -> Arc<<P::GraphicsBackend as Backend>::GraphicsPipeline> {
    let inner = self.inner.lock().unwrap();
    let pipeline_opt = inner.graphics_pipelines.get(&handle);
    if let Some(pipeline) = pipeline_opt {
      return pipeline.pipeline.clone();
    }
    assert!(inner.remaining_graphics_compilations.contains_key(&handle));
    let inner = self.condvar.wait_while(inner, |inner| !inner.graphics_pipelines.contains_key(&handle)).unwrap();
    inner.graphics_pipelines.get(&handle).unwrap().pipeline.clone()
  }

  pub fn try_get_compute_pipeline(&self, handle: PipelineHandle) -> Option<Arc<<P::GraphicsBackend as Backend>::ComputePipeline>> {
    let inner = self.inner.lock().unwrap();
    inner.compute_pipelines.get(&handle).map(|p| p.pipeline.clone())
  }

  pub fn get_compute_pipeline(&self, handle: PipelineHandle) -> Arc<<P::GraphicsBackend as Backend>::ComputePipeline> {
    let inner = self.inner.lock().unwrap();
    let pipeline_opt = inner.compute_pipelines.get(&handle);
    if let Some(pipeline) = pipeline_opt {
      return pipeline.pipeline.clone();
    }
    assert!(inner.remaining_compute_compilations.contains_key(&handle));
    let inner = self.condvar.wait_while(inner, |inner| !inner.compute_pipelines.contains_key(&handle)).unwrap();
    inner.compute_pipelines.get(&handle).unwrap().pipeline.clone()
  }
}
