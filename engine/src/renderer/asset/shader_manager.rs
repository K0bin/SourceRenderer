use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::Arc;
use crate::{Mutex, Condvar};

use log::trace;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::Shader as _;

use crate::asset::{
    Asset, AssetHandle, AssetLoadPriority, AssetManager, AssetRef, AssetType, AssetWithHandle, ShaderHandle
};
use crate::graphics::*;
use crate::graphics::GraphicsPipelineInfo as ActualGraphicsPipelineInfo;
use crate::graphics::RayTracingPipelineInfo as ActualRayTracingPipelineInfo;

use super::{RendererAssetsReadOnly, RendererShader};

//
// COMMON
//

pub trait PipelineCompileTask: Send + Sync + Clone {
    type TShaders;
    type TPipeline: Send + Sync;

    fn asset_type() -> AssetType;
    fn pipeline_from_asset_ref<'a>(asset: AssetRef<'a>) -> &'a CompiledPipeline<Self>;
    fn pipeline_into_asset(self, pipeline: Arc<Self::TPipeline>) -> Asset;
    fn get_task(pipeline: &CompiledPipeline<Self>) -> &Self {
        &pipeline.task
    }

    fn name(&self) -> Option<String>;
    fn contains_shader(&self, handle: ShaderHandle) -> Option<ShaderType>;
    fn request_shader_refresh(&self, asset_manager: &Arc<AssetManager>);
    fn can_compile(
        &self,
        renderer_assets_read: &RendererAssetsReadOnly<'_>,
        loaded_shader_handle: Option<ShaderHandle>
    ) -> bool;
    fn collect_shaders_for_compilation(
        &self,
        renderer_assets_read: RendererAssetsReadOnly<'_>
    ) -> Self::TShaders;
    fn compile(
        &self,
        shaders: Self::TShaders,
        device: &Arc<Device>,
    ) -> Arc<Self::TPipeline>;
}

pub struct CompiledPipeline<T: PipelineCompileTask> {
    task: T,
    pub(crate) pipeline: Arc<T::TPipeline>,
}

//
// GRAPHICS
//

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GraphicsPipelineHandle(AssetHandle);

impl From<AssetHandle> for GraphicsPipelineHandle {
    fn from(value: AssetHandle) -> Self {
        Self(value)
    }
}

impl Into<AssetHandle> for GraphicsPipelineHandle {
    fn into(self) -> AssetHandle {
        self.0
    }
}

#[derive(Debug, Hash, Eq, PartialEq, Clone)]
struct StoredVertexLayoutInfo {
    pub shader_inputs: SmallVec<[ShaderInputElement; 4]>,
    pub input_assembler: SmallVec<[InputAssemblerElement; 4]>,
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
    pub constants: [f32; 4],
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
    pub vs: ShaderHandle,
    pub fs: Option<ShaderHandle>,
    pub vertex_layout: StoredVertexLayoutInfo,
    pub rasterizer: RasterizerInfo,
    pub depth_stencil: DepthStencilInfo,
    pub blend: StoredBlendInfo,
    pub primitive_type: PrimitiveType,
    pub render_target_formats: SmallVec<[Format; 8]>,
    pub depth_stencil_format: Format
}

#[derive(Debug, Clone)]
pub struct GraphicsPipelineInfo<'a> {
    pub vs: &'a str,
    pub fs: Option<&'a str>,
    pub vertex_layout: VertexLayoutInfo<'a>,
    pub rasterizer: RasterizerInfo,
    pub depth_stencil: DepthStencilInfo,
    pub blend: BlendInfo<'a>,
    pub primitive_type: PrimitiveType,
    pub render_target_formats: &'a [Format],
    pub depth_stencil_format: Format
}

#[derive(Debug)]
pub struct GraphicsCompileTask {
    info: StoredGraphicsPipelineInfo,
    is_async: bool,
    _p: PhantomData<Device>,
}

impl Clone for GraphicsCompileTask {
    fn clone(&self) -> Self {
        Self {
            info: self.info.clone(),
            is_async: self.is_async,
            _p: PhantomData
        }
    }
}

pub struct GraphicsShaders {
    vs: Arc<Shader>,
    fs: Option<Arc<Shader>>,
}

impl PipelineCompileTask for GraphicsCompileTask {
    type TShaders = GraphicsShaders;
    type TPipeline = crate::graphics::GraphicsPipeline;

    fn asset_type() -> AssetType {
        AssetType::GraphicsPipeline
    }
    fn pipeline_from_asset_ref<'a>(asset: AssetRef<'a>) -> &'a CompiledPipeline<Self> {
        if let AssetRef::GraphicsPipeline(pipeline) = asset {
            pipeline
        } else {
            panic!("Asset has wrong type")
        }
    }

    fn name(&self) -> Option<String> {
        Some(format!("GraphicsPipeline: VS: {:?}, FS: {:?}", &self.info.vs, self.info.fs.as_ref()))
    }

    fn pipeline_into_asset(self, pipeline: Arc<Self::TPipeline>) -> Asset {
        Asset::GraphicsPipeline(CompiledPipeline { task: self, pipeline })
    }

    fn contains_shader(&self, handle: ShaderHandle) -> Option<ShaderType> {
        if self.info.vs == handle {
            Some(ShaderType::VertexShader)
        } else if self
            .info
            .fs
            .map(|fs| fs == handle)
            .unwrap_or(false)
        {
            Some(ShaderType::FragmentShader)
        } else {
            None
        }
    }

    fn can_compile(
        &self,
        renderer_assets_read: &RendererAssetsReadOnly<'_>,
        loaded_shader_handle: Option<ShaderHandle>,
    ) -> bool {
        (loaded_shader_handle.map_or(false, |s| s == self.info.vs) || renderer_assets_read.get_shader(self.info.vs).is_some())
            && self
                .info
                .fs
                .map(|fs| loaded_shader_handle.map_or(false, |s| s == fs) || renderer_assets_read.get_shader(fs).is_some())
                .unwrap_or(true)
    }

    fn request_shader_refresh(&self, asset_manager: &Arc<AssetManager>) {
        asset_manager.request_asset_refresh_by_handle(self.info.vs, AssetLoadPriority::High);
        if let Some(fs) = self.info.fs {
            asset_manager.request_asset_refresh_by_handle(fs, AssetLoadPriority::High);
        }
    }

    fn collect_shaders_for_compilation(
        &self,
        renderer_assets_read: RendererAssetsReadOnly<'_>
    ) -> Self::TShaders {
        GraphicsShaders {
            vs: renderer_assets_read.get_shader(self.info.vs).cloned().unwrap(),
            fs: self
                .info
                .fs
                .map(|fs| renderer_assets_read.get_shader(fs).cloned().unwrap()),
        }
    }

    fn compile(
        &self,
        shaders: Self::TShaders,
        device: &Arc<Device>,
    ) -> Arc<Self::TPipeline> {
        let input_layout = VertexLayoutInfo {
            shader_inputs: &self.info.vertex_layout.shader_inputs[..],
            input_assembler: &self.info.vertex_layout.input_assembler[..],
        };

        let blend_info = BlendInfo {
            alpha_to_coverage_enabled: self.info.blend.alpha_to_coverage_enabled,
            logic_op_enabled: self.info.blend.logic_op_enabled,
            logic_op: self.info.blend.logic_op,
            attachments: &self.info.blend.attachments[..],
            constants: self.info.blend.constants,
        };

        let info = ActualGraphicsPipelineInfo {
            vs: shaders.vs.as_ref(),
            fs: shaders.fs.as_ref().map(|s| s.as_ref()),
            vertex_layout: input_layout,
            rasterizer: self.info.rasterizer.clone(),
            depth_stencil: self.info.depth_stencil.clone(),
            blend: blend_info,
            primitive_type: self.info.primitive_type,
            render_target_formats: &self.info.render_target_formats,
            depth_stencil_format: self.info.depth_stencil_format
        };

        device.create_graphics_pipeline(&info, self.name().as_ref().map(|n| n as &str))
    }
}

//
// COMPUTE
//

pub struct ComputeCompileTask {
    handle: ShaderHandle,
    is_async: bool,
    _p: PhantomData<Device>,
}

impl Clone for ComputeCompileTask {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle,
            is_async: self.is_async,
            _p: PhantomData
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ComputePipelineHandle(AssetHandle);

impl From<AssetHandle> for ComputePipelineHandle {
    fn from(value: AssetHandle) -> Self {
        Self(value)
    }
}

impl Into<AssetHandle> for ComputePipelineHandle {
    fn into(self) -> AssetHandle {
        self.0
    }
}

impl PipelineCompileTask for ComputeCompileTask {
    type TShaders = Arc<Shader>;
    type TPipeline = crate::graphics::ComputePipeline;

    fn asset_type() -> AssetType {
        AssetType::ComputePipeline
    }
    fn pipeline_from_asset_ref<'a>(asset: AssetRef<'a>) -> &'a CompiledPipeline<Self> {
        if let AssetRef::ComputePipeline(pipeline) = asset {
            pipeline
        } else {
            panic!("Asset has wrong type")
        }
    }

    fn pipeline_into_asset(self, pipeline: Arc<Self::TPipeline>) -> Asset {
        Asset::ComputePipeline(CompiledPipeline { task: self, pipeline })
    }

    fn name(&self) -> Option<String> {
        Some(format!("ComputePipeline: {:?}", self.handle))
    }

    fn contains_shader(&self, handle: ShaderHandle) -> Option<ShaderType> {
        if self.handle == handle {
            Some(ShaderType::ComputeShader)
        } else {
            None
        }
    }

    fn request_shader_refresh(&self, asset_manager: &Arc<AssetManager>) {
        asset_manager.request_asset_refresh_by_handle(self.handle, AssetLoadPriority::High);
    }

    fn can_compile(
        &self,
        renderer_assets_read: &RendererAssetsReadOnly<'_>,
        loaded_shader_handle: Option<ShaderHandle>,
    ) -> bool {
        loaded_shader_handle.map_or(false, |s| s == self.handle) || renderer_assets_read.get(self.handle).is_some()
    }

    fn collect_shaders_for_compilation(
        &self,
        renderer_assets_read: RendererAssetsReadOnly<'_>
    ) -> Self::TShaders {
        renderer_assets_read.get_shader(self.handle).cloned().unwrap()
    }

    fn compile(
        &self,
        shader: Self::TShaders,
        device: &Arc<Device>,
    ) -> Arc<Self::TPipeline> {
        device.create_compute_pipeline(&shader, self.name().as_ref().map(|n| n as &str))
    }
}

//
// RAY TRACING
//

#[derive(Debug, Clone)]
pub struct RayTracingPipelineInfo<'a> {
    pub ray_gen_shader: &'a str,
    pub closest_hit_shaders: &'a [&'a str],
    pub miss_shaders: &'a [&'a str],
}

#[derive(Debug)]
pub struct StoredRayTracingPipelineInfo {
    ray_gen_shader: ShaderHandle,
    closest_hit_shaders: SmallVec<[ShaderHandle; 4]>,
    miss_shaders: SmallVec<[ShaderHandle; 1]>,
    is_async: bool,
    _p: PhantomData<Device>,
}

impl Clone for StoredRayTracingPipelineInfo {
    fn clone(&self) -> Self {
        Self {
            ray_gen_shader: self.ray_gen_shader.clone(),
            closest_hit_shaders: self.closest_hit_shaders.clone(),
            miss_shaders: self.miss_shaders.clone(),
            is_async: self.is_async,
            _p: PhantomData
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RayTracingPipelineHandle(AssetHandle);

impl From<AssetHandle> for RayTracingPipelineHandle {
    fn from(value: AssetHandle) -> Self {
        Self(value)
    }
}

impl Into<AssetHandle> for RayTracingPipelineHandle {
    fn into(self) -> AssetHandle {
        self.0
    }
}

pub struct RayTracingShaders {
    pub ray_gen_shader: Arc<Shader>,
    pub closest_hit_shaders: SmallVec<[Arc<Shader>; 4]>,
    pub miss_shaders: SmallVec<[Arc<Shader>; 4]>,
}

impl PipelineCompileTask for StoredRayTracingPipelineInfo {
    type TShaders = RayTracingShaders;
    type TPipeline = crate::graphics::RayTracingPipeline;

    fn asset_type() -> AssetType {
        AssetType::RayTracingPipeline
    }
    fn pipeline_from_asset_ref<'a>(asset: AssetRef<'a>) -> &'a CompiledPipeline<Self> {
        if let AssetRef::RayTracingPipeline(pipeline) = asset {
            pipeline
        } else {
            panic!("Asset has wrong type")
        }
    }

    fn pipeline_into_asset(self, pipeline: Arc<Self::TPipeline>) -> Asset {
        Asset::RayTracingPipeline(CompiledPipeline { task: self, pipeline })
    }

    fn name(&self) -> Option<String> {
        None
    }

    fn contains_shader(&self, handle: ShaderHandle) -> Option<ShaderType> {
        if self.ray_gen_shader == handle {
            return Some(ShaderType::RayGen);
        }
        for shader in &self.closest_hit_shaders {
            if *shader == handle {
                return Some(ShaderType::RayClosestHit);
            }
        }
        for shader in &self.miss_shaders {
            if *shader == handle {
                return Some(ShaderType::RayMiss);
            }
        }
        None
    }

    fn request_shader_refresh(&self, asset_manager: &Arc<AssetManager>) {
        asset_manager.request_asset_refresh_by_handle(
            self.ray_gen_shader,
            AssetLoadPriority::High,
        );
        for shader in &self.closest_hit_shaders {
            asset_manager.request_asset_refresh_by_handle(*shader, AssetLoadPriority::High);
        }
        for shader in &self.miss_shaders {
            asset_manager.request_asset_refresh_by_handle(*shader, AssetLoadPriority::High);
        }
    }

    fn can_compile(
        &self,
        renderer_assets_read: &RendererAssetsReadOnly<'_>,
        loaded_shader_handle: Option<ShaderHandle>,
    ) -> bool {
        if !loaded_shader_handle.map_or(false, |s| s == self.ray_gen_shader) && !renderer_assets_read.get(self.ray_gen_shader).is_some()
        {
            return false;
        }
        for shader in &self.closest_hit_shaders {
            if !loaded_shader_handle.map_or(false, |s| s == *shader) && !renderer_assets_read.get_shader(*shader).is_some() {
                return false;
            }
        }
        for shader in &self.miss_shaders {
            if !loaded_shader_handle.map_or(false, |s| s == *shader) && !renderer_assets_read.get_shader(*shader).is_some() {
                return false;
            }
        }
        true
    }

    fn collect_shaders_for_compilation(
        &self,
        renderer_assets_read: RendererAssetsReadOnly<'_>
    ) -> Self::TShaders {
        Self::TShaders {
            ray_gen_shader: renderer_assets_read.get_shader(self.ray_gen_shader).cloned().unwrap(),
            closest_hit_shaders: self
                .closest_hit_shaders
                .iter()
                .map(|shader| renderer_assets_read.get_shader(*shader).cloned().unwrap())
                .collect(),
            miss_shaders: self
                .miss_shaders
                .iter()
                .map(|shader| renderer_assets_read.get_shader(*shader).cloned().unwrap())
                .collect(),
        }
    }

    fn compile(
        &self,
        shaders: Self::TShaders,
        device: &Arc<Device>,
    ) -> Arc<Self::TPipeline> {
        let closest_hit_shader_refs: SmallVec<[&Shader; 4]> =
            shaders.closest_hit_shaders.iter().map(|s| s.as_ref()).collect();
        let miss_shaders_refs: SmallVec<[&Shader; 1]> =
            shaders.miss_shaders.iter().map(|s| s.as_ref()).collect();
        let info = ActualRayTracingPipelineInfo {
            ray_gen_shader: &shaders.ray_gen_shader,
            closest_hit_shaders: &closest_hit_shader_refs[..],
            miss_shaders: &miss_shaders_refs[..],
        };
        device.create_raytracing_pipeline(&info, self.name().as_ref().map(|n| n as &str)).unwrap()
    }
}

//
// BASE
//

pub struct ShaderManager {
    device: Arc<Device>,
    graphics: Arc<PipelineTypeManager<GraphicsPipelineHandle, GraphicsCompileTask>>,
    compute: Arc<PipelineTypeManager<ComputePipelineHandle, ComputeCompileTask>>,
    rt: Arc<
        PipelineTypeManager<RayTracingPipelineHandle, StoredRayTracingPipelineInfo>,
    >
}

struct PipelineTypeManager<THandle, T>
where
    THandle: Hash + PartialEq + Eq + Clone + Copy + Send + Sync + From<AssetHandle>,
    T: PipelineCompileTask,
{
    remaining_compilations: Mutex<HashMap<THandle, T>>,
    cond_var: Condvar,
}

impl<THandle, T> PipelineTypeManager<THandle, T>
where
    THandle: Hash + PartialEq + Eq + Clone + Copy + Send + Sync + From<AssetHandle>,
    T: PipelineCompileTask,
{
    fn new() -> Self {
        Self {
            remaining_compilations: Mutex::new(HashMap::new()),
            cond_var: Condvar::new(),
        }
    }
}

impl Drop for ShaderManager {
    fn drop(&mut self) {
        log::warn!("Dropping ShaderManager");
    }
}

impl ShaderManager {
    pub fn new(
        device: &Arc<Device>,
    ) -> Self {
        Self {
            device: device.clone(),
            graphics: Arc::new(PipelineTypeManager::new()),
            compute: Arc::new(PipelineTypeManager::new()),
            rt: Arc::new(PipelineTypeManager::new()),
        }
    }

    pub fn request_graphics_pipeline(
        &self,
        asset_manager: &Arc<AssetManager>,
        info: &GraphicsPipelineInfo,
    ) -> GraphicsPipelineHandle {
        let stored_input_layout = StoredVertexLayoutInfo {
            shader_inputs: info.vertex_layout.shader_inputs.iter().cloned().collect(),
            input_assembler: info.vertex_layout.input_assembler.iter().cloned().collect(),
        };

        let stored_blend = StoredBlendInfo {
            alpha_to_coverage_enabled: info.blend.alpha_to_coverage_enabled,
            logic_op_enabled: info.blend.logic_op_enabled,
            logic_op: info.blend.logic_op,
            attachments: info.blend.attachments.iter().cloned().collect(),
            constants: info.blend.constants.clone(),
        };

        let (vs_handle, _) = asset_manager.request_asset(&info.vs, AssetType::Shader, AssetLoadPriority::Normal);
        let fs_handle = info.fs.as_ref().map(|fs| asset_manager.request_asset(fs, AssetType::Shader, AssetLoadPriority::Normal).0.into());

        let stored = StoredGraphicsPipelineInfo {
            vs: vs_handle.into(),
            fs: fs_handle,
            vertex_layout: stored_input_layout,
            rasterizer: info.rasterizer.clone(),
            depth_stencil: info.depth_stencil.clone(),
            blend: stored_blend,
            primitive_type: info.primitive_type,
            render_target_formats: info.render_target_formats.iter().copied().collect(),
            depth_stencil_format: info.depth_stencil_format
        };

        self.request_pipeline_internal(
            asset_manager,
            &self.graphics,
            GraphicsCompileTask {
                info: stored,
                is_async: false,
                _p: PhantomData,
            },
        )
    }

    pub fn request_compute_pipeline(
        &self,
        asset_manager: &Arc<AssetManager>,
        path: &str) -> ComputePipelineHandle {
        let (shader_handle, _) = asset_manager.request_asset(path, AssetType::Shader, AssetLoadPriority::Normal);
        self.request_pipeline_internal(
            asset_manager,
            &self.compute,
            ComputeCompileTask {
                handle: shader_handle.into(),
                is_async: false,
                _p: PhantomData,
            },
        )
    }

    pub fn request_ray_tracing_pipeline(
        &self,
        asset_manager: &Arc<AssetManager>,
        info: &RayTracingPipelineInfo,
    ) -> RayTracingPipelineHandle {
        self.request_pipeline_internal(
            asset_manager,
            &self.rt,
            StoredRayTracingPipelineInfo {
                closest_hit_shaders: info
                    .closest_hit_shaders
                    .iter()
                    .map(|path| asset_manager.request_asset(path, AssetType::Shader, AssetLoadPriority::Normal).0.into())
                    .collect(),
                miss_shaders: info.miss_shaders.iter().map(|path| asset_manager.request_asset(path, AssetType::Shader, AssetLoadPriority::Normal).0.into()).collect(),
                ray_gen_shader: asset_manager.request_asset(&info.ray_gen_shader, AssetType::Shader, AssetLoadPriority::Normal).0.into(),
                is_async: false,
                _p: PhantomData,
            },
        )
    }

    fn request_pipeline_internal<T, THandle>(
        &self,
        asset_manager: &Arc<AssetManager>,
        pipeline_type_manager: &Arc<PipelineTypeManager<THandle, T>>,
        task: T,
    ) -> THandle
    where
        THandle: Hash + PartialEq + Eq + Clone + Copy + Send + Sync + From<AssetHandle>,
        T: PipelineCompileTask,
    {
        let handle: THandle = asset_manager.reserve_handle_without_path(T::asset_type()).into();
        let mut remaining = pipeline_type_manager.remaining_compilations.lock().unwrap();
        remaining.insert(handle, task);
        handle
    }

    fn add_shader_type<THandle, T>(
        &self,
        asset_manager: &Arc<AssetManager>,
        pipeline_type_manager: &Arc<PipelineTypeManager<THandle, T>>,
        handle: ShaderHandle,
        shader: &RendererShader
    ) -> bool
    where
        THandle: Hash + PartialEq + Eq + Clone + Copy + Send + Sync + From<AssetHandle> + Into<AssetHandle> + 'static,
        T: PipelineCompileTask + 'static,
    {
        {
            trace!("Integrating shader {:?} {:?}", shader.shader_type(), handle);
            let mut ready_handles = SmallVec::<[THandle; 1]>::new();
            {

                // Find all pipelines that use this shader and queue new compile tasks for those.
                // This is done because add_shader will get called when a shader has changed on disk, so we need to load
                // all remaining shaders of a pipeline and recompile it.

                let assets_read = asset_manager.read_renderer_assets();
                let mut remaining_compilations: crate::MutexGuard<'_, HashMap<THandle, T>> = pipeline_type_manager.remaining_compilations.lock().unwrap();
                let compiled_pipeline_handles = assets_read.all_pipeline_handles(T::asset_type());
                for pipeline_handle in compiled_pipeline_handles {
                    let asset_ref = assets_read.get(pipeline_handle).unwrap();
                    let pipeline: &CompiledPipeline<T> = T::pipeline_from_asset_ref(asset_ref);
                    let existing_pipeline_match = pipeline.task.contains_shader(handle);
                    if let Some(shader_type) = existing_pipeline_match {
                        assert!(shader_type == shader.shader_type());
                        let typed_handle: THandle = pipeline_handle.into();
                        if !remaining_compilations.contains_key(&typed_handle) {
                            let task: T = pipeline.task.clone();
                            remaining_compilations.insert(typed_handle, task);
                        }
                    }
                }

                for (pipeline_handle, task) in remaining_compilations.iter() {
                    let remaining_compile_match = task.contains_shader(handle);
                    if let Some(shader_type) = remaining_compile_match {
                        trace!("Found pipeline that contains shader {:?} {:?}. Testing if its ready to compile.", shader.shader_type(), handle);
                        assert!(shader_type == shader.shader_type());
                        if task.can_compile(&assets_read, Some(handle)) {
                            trace!("Pipeline that contains shader {:?} {:?} is ready to compile.", shader.shader_type(), handle);
                            ready_handles.push(*pipeline_handle);
                        }
                    }
                }
            }

            if ready_handles.is_empty() {
                trace!("Nothing to do with shader {:?} {:?}", shader.shader_type(), handle);
                return true;
            }

            trace!("Queuing compile tasks for pipelines with {:?} {:?}", shader.shader_type(), handle);
            let c_device = self.device.clone();
            let c_manager: Arc<PipelineTypeManager<THandle, T>> = pipeline_type_manager.clone();
            let c_asset_manager = asset_manager.clone();
            c_manager.cond_var.notify_all();
            let task_pool = bevy_tasks::ComputeTaskPool::get();
            let task = task_pool.spawn(async move {
                for handle in ready_handles.drain(..) {
                    let task: T;
                    let shaders: T::TShaders;

                    let assets_read = c_asset_manager.read_renderer_assets();
                    {
                        let mut remaining_compilations = c_manager.remaining_compilations.lock().unwrap();
                        let task_opt = remaining_compilations.remove(&handle);
                        if task_opt.is_none() {
                            continue;
                        }
                        task = task_opt.unwrap();
                        shaders = task.collect_shaders_for_compilation(assets_read);
                    };
                    let pipeline: Arc<<T as PipelineCompileTask>::TPipeline> = task.compile(shaders, &c_device);
                    let generic_handle: AssetHandle = handle.into();
                    c_asset_manager.add_asset_with_handle(AssetWithHandle::combine(generic_handle, T::pipeline_into_asset(task, pipeline)));
                }
                c_manager.cond_var.notify_all();
            });
            task.detach();
            true
        }
    }

    pub fn add_shader(&self, asset_manager: &Arc<AssetManager>, handle: ShaderHandle, shader: &RendererShader) {
        if !match shader.shader_type() {
            ShaderType::ComputeShader => self.add_shader_type(asset_manager, &self.compute, handle, shader),
            ShaderType::RayGen | ShaderType::RayClosestHit | ShaderType::RayMiss => self.add_shader_type(asset_manager, &self.rt, handle, shader),
            ShaderType::FragmentShader | ShaderType::VertexShader | ShaderType::GeometryShader | ShaderType::TessellationControlShader | ShaderType::TessellationEvaluationShader =>
                self.add_shader_type(asset_manager, &self.graphics, handle, shader),
        } {
            panic!("Unhandled shader. {:?}", handle);
        }
    }

    pub fn has_remaining_mandatory_compilations(&self) -> bool {
        let has_graphics_compiles = {
            let graphics_remaining = self.graphics.remaining_compilations.lock().unwrap();
            graphics_remaining
                .iter()
                .any(|(_, t)| !t.is_async)
        };
        let has_compute_compiles = {
            let compute_remaining = self.compute.remaining_compilations.lock().unwrap();
            compute_remaining
                .iter()
                .any(|(_, t)| !t.is_async)
        };
        let has_rt_compiles = {
            let rt_remaining = self.rt.remaining_compilations.lock().unwrap();
            rt_remaining.iter().any(|(_, t)| !t.is_async)
        };
        has_graphics_compiles || has_compute_compiles || has_rt_compiles
    }
}
