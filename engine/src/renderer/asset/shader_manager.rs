use std::collections::hash_map::Iter;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use crate::{Mutex, Condvar};

use log::trace;
use smallvec::SmallVec;
use crate::graphics::gpu::Shader as _;

use crate::asset::{
    AssetHandle, AssetLoadPriority, AssetManager, AssetType, ShaderHandle
};
use crate::graphics::*;
use crate::graphics::GraphicsPipelineInfo as ActualGraphicsPipelineInfo;
use crate::graphics::MeshGraphicsPipelineInfo as ActualMeshGraphicsPipelineInfo;
use crate::graphics::RayTracingPipelineInfo as ActualRayTracingPipelineInfo;

use super::{RendererAssetsReadOnly, RendererShader};

//
// COMMON
//

pub trait PipelineCompileTask: Clone {
    type TShaders : Send;
    type TPipelineHandle: Hash + PartialEq + Eq + Clone + Copy + From<AssetHandle> + Into<AssetHandle> + Send + Sync;
    #[cfg(not(target_arch = "wasm32"))]
    type TPipeline : Send + Sync;
    #[cfg(target_arch = "wasm32")]
    type TPipeline;

    fn asset_type() -> AssetType;

    fn finished_pipelines<'a>(assets: &'a RendererAssetsReadOnly) -> Iter<'a, Self::TPipelineHandle, CompiledPipeline<Self>>;

    fn name(&self) -> Option<String>;
    fn handle(&self) -> Self::TPipelineHandle;
    fn contains_shader(&self, handle: ShaderHandle) -> Option<ShaderType>;
    fn request_shader_refresh(&self, asset_manager: &Arc<AssetManager>);
    fn can_compile(
        &self,
        renderer_assets_read: &RendererAssetsReadOnly<'_>,
        loaded_shader_handle: Option<ShaderHandle>
    ) -> bool;
    fn collect_shaders_for_compilation(
        &self,
        renderer_assets_read: &RendererAssetsReadOnly<'_>
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
pub(super) struct StoredVertexLayoutInfo {
    pub(super) shader_inputs: SmallVec<[ShaderInputElement; 4]>,
    pub(super) input_assembler: SmallVec<[InputAssemblerElement; 4]>,
}

impl<'a> PartialEq<VertexLayoutInfo<'a>> for StoredVertexLayoutInfo {
    fn eq(&self, other: &VertexLayoutInfo<'a>) -> bool {
        &self.shader_inputs[..] == other.shader_inputs
            && &self.input_assembler[..] == other.input_assembler
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct StoredBlendInfo {
    pub(super) alpha_to_coverage_enabled: bool,
    pub(super) logic_op_enabled: bool,
    pub(super) logic_op: LogicOp,
    pub(super) attachments: SmallVec<[AttachmentBlendInfo; 4]>,
    pub(super) constants: [f32; 4],
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

#[derive(Debug, Clone)]
pub struct GraphicsCompileTask {
    vs: ShaderHandle,
    fs: Option<ShaderHandle>,
    vertex_layout: StoredVertexLayoutInfo,
    rasterizer: RasterizerInfo,
    depth_stencil: DepthStencilInfo,
    blend: StoredBlendInfo,
    primitive_type: PrimitiveType,
    render_target_formats: SmallVec<[Format; 8]>,
    depth_stencil_format: Format,
    handle: GraphicsPipelineHandle,
    is_async: bool,
}

pub struct GraphicsShaders {
    vs: Arc<Shader>,
    fs: Option<Arc<Shader>>,
}

impl PipelineCompileTask for GraphicsCompileTask {
    type TShaders = GraphicsShaders;
    type TPipeline = crate::graphics::GraphicsPipeline;
    type TPipelineHandle = GraphicsPipelineHandle;

    fn asset_type() -> AssetType {
        AssetType::GraphicsPipeline
    }

    fn name(&self) -> Option<String> {
        Some(format!("GraphicsPipeline: VS: {:?}, FS: {:?}", &self.vs, self.fs.as_ref()))
    }

    fn handle(&self) -> Self::TPipelineHandle {
        self.handle
    }

    fn contains_shader(&self, handle: ShaderHandle) -> Option<ShaderType> {
        if self.vs == handle {
            Some(ShaderType::VertexShader)
        } else if self.fs
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
        (loaded_shader_handle.map_or(false, |s| s == self.vs) || renderer_assets_read.get_shader(self.vs).is_some())
            && self.fs
                .map(|fs| loaded_shader_handle.map_or(false, |s| s == fs) || renderer_assets_read.get_shader(fs).is_some())
                .unwrap_or(true)
    }

    fn request_shader_refresh(&self, asset_manager: &Arc<AssetManager>) {
        asset_manager.request_asset_refresh_by_handle(self.vs, AssetLoadPriority::High);
        if let Some(fs) = self.fs {
            asset_manager.request_asset_refresh_by_handle(fs, AssetLoadPriority::High);
        }
    }

    fn collect_shaders_for_compilation(
        &self,
        renderer_assets_read: &RendererAssetsReadOnly<'_>
    ) -> Self::TShaders {
        GraphicsShaders {
            vs: renderer_assets_read.get_shader(self.vs).cloned().unwrap(),
            fs: self.fs
                .map(|fs| renderer_assets_read.get_shader(fs).cloned().unwrap()),
        }
    }

    fn finished_pipelines<'a>(assets: &'a RendererAssetsReadOnly) -> Iter<'a, Self::TPipelineHandle, CompiledPipeline<Self>> {
        assets.all_graphics_pipelines()
    }

    fn compile(
        &self,
        shaders: Self::TShaders,
        device: &Arc<Device>,
    ) -> Arc<Self::TPipeline> {
        let input_layout = VertexLayoutInfo {
            shader_inputs: &self.vertex_layout.shader_inputs[..],
            input_assembler: &self.vertex_layout.input_assembler[..],
        };

        let blend_info = BlendInfo {
            alpha_to_coverage_enabled: self.blend.alpha_to_coverage_enabled,
            logic_op_enabled: self.blend.logic_op_enabled,
            logic_op: self.blend.logic_op,
            attachments: &self.blend.attachments[..],
            constants: self.blend.constants,
        };

        let info = ActualGraphicsPipelineInfo {
            vs: shaders.vs.as_ref(),
            fs: shaders.fs.as_ref().map(|s| s.as_ref()),
            vertex_layout: input_layout,
            rasterizer: self.rasterizer.clone(),
            depth_stencil: self.depth_stencil.clone(),
            blend: blend_info,
            primitive_type: self.primitive_type,
            render_target_formats: &self.render_target_formats,
            depth_stencil_format: self.depth_stencil_format
        };

        device.create_graphics_pipeline(&info, self.name().as_ref().map(|n| n as &str))
    }
}

//
// GRAPHICS MESH
//

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshGraphicsPipelineHandle(AssetHandle);

impl From<AssetHandle> for MeshGraphicsPipelineHandle {
    fn from(value: AssetHandle) -> Self {
        Self(value)
    }
}

impl Into<AssetHandle> for MeshGraphicsPipelineHandle {
    fn into(self) -> AssetHandle {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct MeshGraphicsPipelineInfo<'a> {
    pub ts: Option<&'a str>,
    pub ms: &'a str,
    pub fs: Option<&'a str>,
    pub rasterizer: RasterizerInfo,
    pub depth_stencil: DepthStencilInfo,
    pub blend: BlendInfo<'a>,
    pub render_target_formats: &'a [Format],
    pub depth_stencil_format: Format
}

#[derive(Debug, Clone)]
pub struct MeshGraphicsCompileTask {
    ts: Option<ShaderHandle>,
    ms: ShaderHandle,
    fs: Option<ShaderHandle>,
    rasterizer: RasterizerInfo,
    depth_stencil: DepthStencilInfo,
    blend: StoredBlendInfo,
    render_target_formats: SmallVec<[Format; 8]>,
    depth_stencil_format: Format,
    is_async: bool,
    handle: MeshGraphicsPipelineHandle,
}

pub struct MeshGraphicsShaders {
    ts: Option<Arc<Shader>>,
    ms: Arc<Shader>,
    fs: Option<Arc<Shader>>,
}

impl PipelineCompileTask for MeshGraphicsCompileTask {
    type TShaders = MeshGraphicsShaders;
    type TPipeline = crate::graphics::MeshGraphicsPipeline;
    type TPipelineHandle = MeshGraphicsPipelineHandle;

    fn asset_type() -> AssetType {
        AssetType::MeshGraphicsPipeline
    }

    fn name(&self) -> Option<String> {
        Some(format!("GraphicsPipeline: TS: {:?}, MS: {:?}, FS: {:?}", self.ts.as_ref(), &self.ms, self.fs.as_ref()))
    }

    fn handle(&self) -> Self::TPipelineHandle {
        self.handle
    }

    fn contains_shader(&self, handle: ShaderHandle) -> Option<ShaderType> {
        if self.ms == handle {
            Some(ShaderType::MeshShader)
        } else if self.fs
            .map(|fs| fs == handle)
            .unwrap_or(false)
        {
            Some(ShaderType::FragmentShader)
        } else if self.ts
            .map(|ts| ts == handle)
            .unwrap_or(false)
        {
            Some(ShaderType::TaskShader)
        } else {
            None
        }
    }

    fn can_compile(
        &self,
        renderer_assets_read: &RendererAssetsReadOnly<'_>,
        loaded_shader_handle: Option<ShaderHandle>,
    ) -> bool {
        (loaded_shader_handle.map_or(false, |s| s == self.ms) || renderer_assets_read.get_shader(self.ms).is_some())
            && self.ts
                .map(|ts| loaded_shader_handle.map_or(false, |s| s == ts) || renderer_assets_read.get_shader(ts).is_some())
                .unwrap_or(true)
            && self.fs
                .map(|fs| loaded_shader_handle.map_or(false, |s| s == fs) || renderer_assets_read.get_shader(fs).is_some())
                .unwrap_or(true)
    }

    fn request_shader_refresh(&self, asset_manager: &Arc<AssetManager>) {
        asset_manager.request_asset_refresh_by_handle(self.ms, AssetLoadPriority::High);
        if let Some(ts) = self.ts {
            asset_manager.request_asset_refresh_by_handle(ts, AssetLoadPriority::High);
        }
        if let Some(fs) = self.fs {
            asset_manager.request_asset_refresh_by_handle(fs, AssetLoadPriority::High);
        }
    }

    fn collect_shaders_for_compilation(
        &self,
        renderer_assets_read: &RendererAssetsReadOnly<'_>
    ) -> Self::TShaders {
        MeshGraphicsShaders {
            ts: self.ts
                .map(|ts| renderer_assets_read.get_shader(ts).cloned().unwrap()),
            ms: renderer_assets_read.get_shader(self.ms).cloned().unwrap(),
            fs: self.fs
                .map(|fs| renderer_assets_read.get_shader(fs).cloned().unwrap()),
        }
    }

    fn finished_pipelines<'a>(assets: &'a RendererAssetsReadOnly) -> Iter<'a, Self::TPipelineHandle, CompiledPipeline<Self>> {
        assets.all_mesh_graphics_pipelines()
    }

    fn compile(
        &self,
        shaders: Self::TShaders,
        device: &Arc<Device>,
    ) -> Arc<Self::TPipeline> {
        let blend_info = BlendInfo {
            alpha_to_coverage_enabled: self.blend.alpha_to_coverage_enabled,
            logic_op_enabled: self.blend.logic_op_enabled,
            logic_op: self.blend.logic_op,
            attachments: &self.blend.attachments[..],
            constants: self.blend.constants,
        };

        let info = ActualMeshGraphicsPipelineInfo {
            ts: shaders.ts.as_ref().map(|s| s.as_ref()),
            ms: shaders.ms.as_ref(),
            fs: shaders.fs.as_ref().map(|s| s.as_ref()),
            rasterizer: self.rasterizer.clone(),
            depth_stencil: self.depth_stencil.clone(),
            blend: blend_info,
            render_target_formats: &self.render_target_formats,
            depth_stencil_format: self.depth_stencil_format
        };

        device.create_mesh_graphics_pipeline(&info, self.name().as_ref().map(|n| n as &str))
    }
}

//
// COMPUTE
//

#[derive(Debug, Clone)]
pub struct ComputeCompileTask {
    shader_handle: ShaderHandle,
    is_async: bool,
    handle: ComputePipelineHandle,
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
    type TPipelineHandle = ComputePipelineHandle;

    fn asset_type() -> AssetType {
        AssetType::ComputePipeline
    }

    fn name(&self) -> Option<String> {
        Some(format!("ComputePipeline: {:?}", self.shader_handle))
    }

    fn handle(&self) -> Self::TPipelineHandle {
        self.handle
    }

    fn contains_shader(&self, shader_handle: ShaderHandle) -> Option<ShaderType> {
        if self.shader_handle == shader_handle {
            Some(ShaderType::ComputeShader)
        } else {
            None
        }
    }

    fn request_shader_refresh(&self, asset_manager: &Arc<AssetManager>) {
        asset_manager.request_asset_refresh_by_handle(self.shader_handle, AssetLoadPriority::High);
    }

    fn can_compile(
        &self,
        renderer_assets_read: &RendererAssetsReadOnly<'_>,
        loaded_shader_handle: Option<ShaderHandle>,
    ) -> bool {
        loaded_shader_handle.map_or(false, |s| s == self.shader_handle) || renderer_assets_read.get_shader(self.shader_handle).is_some()
    }

    fn collect_shaders_for_compilation(
        &self,
        renderer_assets_read: &RendererAssetsReadOnly<'_>
    ) -> Self::TShaders {
        renderer_assets_read.get_shader(self.shader_handle).cloned().unwrap()
    }

    fn finished_pipelines<'a>(assets: &'a RendererAssetsReadOnly) -> Iter<'a, Self::TPipelineHandle, CompiledPipeline<Self>> {
        assets.all_compute_pipelines()
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
    pub any_hit_shaders: &'a [&'a str],
    pub miss_shaders: &'a [&'a str],
}

#[derive(Debug, Clone)]
pub struct RayTracingCompileTask {
    ray_gen_shader: ShaderHandle,
    closest_hit_shaders: SmallVec<[ShaderHandle; 4]>,
    any_hit_shaders: SmallVec<[ShaderHandle; 4]>,
    miss_shaders: SmallVec<[ShaderHandle; 1]>,
    is_async: bool,
    handle: RayTracingPipelineHandle,
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
    ray_gen_shader: Arc<Shader>,
    closest_hit_shaders: SmallVec<[Arc<Shader>; 4]>,
    any_hit_shaders: SmallVec<[Arc<Shader>; 4]>,
    miss_shaders: SmallVec<[Arc<Shader>; 4]>,
}

impl PipelineCompileTask for RayTracingCompileTask {
    type TShaders = RayTracingShaders;
    type TPipeline = crate::graphics::RayTracingPipeline;
    type TPipelineHandle = RayTracingPipelineHandle;

    fn asset_type() -> AssetType {
        AssetType::RayTracingPipeline
    }

    fn name(&self) -> Option<String> {
        None
    }

    fn handle(&self) -> Self::TPipelineHandle {
        self.handle
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
        for shader in &self.any_hit_shaders {
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
        if !loaded_shader_handle.map_or(false, |s| s == self.ray_gen_shader) && !renderer_assets_read.get_shader(self.ray_gen_shader).is_some()
        {
            return false;
        }
        for shader in &self.closest_hit_shaders {
            if !loaded_shader_handle.map_or(false, |s| s == *shader) && !renderer_assets_read.get_shader(*shader).is_some() {
                return false;
            }
        }
        for shader in &self.any_hit_shaders {
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
        renderer_assets_read: &RendererAssetsReadOnly<'_>
    ) -> Self::TShaders {
        Self::TShaders {
            ray_gen_shader: renderer_assets_read.get_shader(self.ray_gen_shader).cloned().unwrap(),
            closest_hit_shaders: self.closest_hit_shaders.iter()
                .map(|shader| renderer_assets_read.get_shader(*shader).cloned().unwrap())
                .collect(),
            any_hit_shaders: self.any_hit_shaders.iter()
                .map(|shader| renderer_assets_read.get_shader(*shader).cloned().unwrap())
                .collect(),
            miss_shaders: self.miss_shaders.iter()
                .map(|shader| renderer_assets_read.get_shader(*shader).cloned().unwrap())
                .collect(),
        }
    }

    fn finished_pipelines<'a>(assets: &'a RendererAssetsReadOnly) -> Iter<'a, Self::TPipelineHandle, CompiledPipeline<Self>> {
        assets.all_ray_tracing_pipelines()
    }

    fn compile(
        &self,
        shaders: Self::TShaders,
        device: &Arc<Device>,
    ) -> Arc<Self::TPipeline> {
        let closest_hit_shader_refs: SmallVec<[&Shader; 4]> =
            shaders.closest_hit_shaders.iter().map(|s| s.as_ref()).collect();
        let any_hit_shader_refs: SmallVec<[&Shader; 4]> =
            shaders.any_hit_shaders.iter().map(|s| s.as_ref()).collect();
        let miss_shaders_refs: SmallVec<[&Shader; 1]> =
            shaders.miss_shaders.iter().map(|s| s.as_ref()).collect();
        let info = ActualRayTracingPipelineInfo {
            ray_gen_shader: &shaders.ray_gen_shader,
            closest_hit_shaders: &closest_hit_shader_refs[..],
            any_hit_shaders: &any_hit_shader_refs[..],
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
    graphics: Arc<PipelineTypeManager<GraphicsCompileTask>>,
    mesh_graphics: Arc<PipelineTypeManager<MeshGraphicsCompileTask>>,
    compute: Arc<PipelineTypeManager<ComputeCompileTask>>,
    rt: Arc<PipelineTypeManager<RayTracingCompileTask>>,
}

struct PipelineTypeManager<T>
where
    T: PipelineCompileTask,
{
    remaining_compilations: Mutex<HashMap<T::TPipelineHandle, T>>,
    cond_var: Condvar,
    compiled_unpulled_pipelines: Arc<Mutex<Vec<(T::TPipelineHandle, Arc<T::TPipeline>)>>>,
}

impl<T> PipelineTypeManager<T>
where
    T: PipelineCompileTask,
{
    fn new() -> Self {
        Self {
            remaining_compilations: Mutex::new(HashMap::new()),
            compiled_unpulled_pipelines: Arc::new(Mutex::new(Vec::new())),
            cond_var: Condvar::new(),
        }
    }
}

impl ShaderManager {
    pub fn new(
        device: &Arc<Device>,
    ) -> Self {
        Self {
            device: device.clone(),
            graphics: Arc::new(PipelineTypeManager::new()),
            mesh_graphics: Arc::new(PipelineTypeManager::new()),
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

        let handle: GraphicsPipelineHandle = asset_manager.reserve_handle_without_path(AssetType::GraphicsPipeline).into();
        let mut remaining = self.graphics.remaining_compilations.lock().unwrap();
        remaining.insert(handle, GraphicsCompileTask {
            vs: vs_handle.into(),
            fs: fs_handle,
            vertex_layout: stored_input_layout,
            rasterizer: info.rasterizer.clone(),
            depth_stencil: info.depth_stencil.clone(),
            blend: stored_blend,
            primitive_type: info.primitive_type,
            render_target_formats: info.render_target_formats.iter().copied().collect(),
            depth_stencil_format: info.depth_stencil_format,
            is_async: false,
            handle,
        });
        handle
    }

    pub fn request_mesh_graphics_pipeline(
        &self,
        asset_manager: &Arc<AssetManager>,
        info: &MeshGraphicsPipelineInfo,
    ) -> MeshGraphicsPipelineHandle {
        let stored_blend = StoredBlendInfo {
            alpha_to_coverage_enabled: info.blend.alpha_to_coverage_enabled,
            logic_op_enabled: info.blend.logic_op_enabled,
            logic_op: info.blend.logic_op,
            attachments: info.blend.attachments.iter().cloned().collect(),
            constants: info.blend.constants.clone(),
        };

        let ts_handle = info.ts.as_ref().map(|ts| asset_manager.request_asset(ts, AssetType::Shader, AssetLoadPriority::Normal).0.into());
        let (ms_handle, _) = asset_manager.request_asset(&info.ms, AssetType::Shader, AssetLoadPriority::Normal);
        let fs_handle = info.fs.as_ref().map(|fs| asset_manager.request_asset(fs, AssetType::Shader, AssetLoadPriority::Normal).0.into());

        let handle: MeshGraphicsPipelineHandle = asset_manager.reserve_handle_without_path(AssetType::MeshGraphicsPipeline).into();
        let mut remaining = self.mesh_graphics.remaining_compilations.lock().unwrap();
        remaining.insert(handle, MeshGraphicsCompileTask {
            ts: ts_handle,
            ms: ms_handle.into(),
            fs: fs_handle,
            rasterizer: info.rasterizer.clone(),
            depth_stencil: info.depth_stencil.clone(),
            blend: stored_blend,
            render_target_formats: info.render_target_formats.iter().copied().collect(),
            depth_stencil_format: info.depth_stencil_format,
            is_async: false,
            handle,
        });
        handle
    }

    pub fn request_compute_pipeline(
        &self,
        asset_manager: &Arc<AssetManager>,
        path: &str) -> ComputePipelineHandle {
        let (shader_handle, _) = asset_manager.request_asset(path, AssetType::Shader, AssetLoadPriority::Normal);

        let handle: ComputePipelineHandle = asset_manager.reserve_handle_without_path(AssetType::ComputePipeline).into();
        let mut remaining = self.compute.remaining_compilations.lock().unwrap();
        remaining.insert(handle, ComputeCompileTask {
            shader_handle: shader_handle.into(),
            is_async: false,
            handle,
        });
        handle
    }

    pub fn request_ray_tracing_pipeline(
        &self,
        asset_manager: &Arc<AssetManager>,
        info: &RayTracingPipelineInfo,
    ) -> RayTracingPipelineHandle {
        let handle: RayTracingPipelineHandle = asset_manager.reserve_handle_without_path(AssetType::RayTracingPipeline).into();
        let mut remaining = self.rt.remaining_compilations.lock().unwrap();
        remaining.insert(handle, RayTracingCompileTask {
            closest_hit_shaders: info.closest_hit_shaders.iter()
                .map(|path| asset_manager.request_asset(path, AssetType::Shader, AssetLoadPriority::Normal).0.into())
                .collect(),
            any_hit_shaders: info.any_hit_shaders.iter()
                .map(|path| asset_manager.request_asset(path, AssetType::Shader, AssetLoadPriority::Normal).0.into())
                .collect(),
            miss_shaders: info.miss_shaders.iter().map(|path| asset_manager.request_asset(path, AssetType::Shader, AssetLoadPriority::Normal).0.into()).collect(),
            ray_gen_shader: asset_manager.request_asset(&info.ray_gen_shader, AssetType::Shader, AssetLoadPriority::Normal).0.into(),
            is_async: false,
            handle,
        });
        handle
    }

    fn collect_ready_pipeline_handles_for_new_shader_handle<T>(
        &self,
        assets_read: &RendererAssetsReadOnly,
        pipeline_type_manager: &Arc<PipelineTypeManager<T>>,
        handle: ShaderHandle,
        shader: &RendererShader
    ) -> SmallVec::<[T; 1]>
    where
        T: PipelineCompileTask + 'static,
    {
        trace!("Integrating shader {:?} {:?}", shader.shader_type(), handle);
        let mut ready_handles = SmallVec::<[T::TPipelineHandle; 1]>::new();

        // Find all pipelines that use this shader and queue new compile tasks for those.
        // This is done because add_shader will get called when a shader has changed on disk, so we need to load
        // all remaining shaders of a pipeline and recompile it.
        let mut remaining_compilations: crate::MutexGuard<'_, HashMap<T::TPipelineHandle, T>> = pipeline_type_manager.remaining_compilations.lock().unwrap();
        let finished_pipelines = T::finished_pipelines(assets_read);
        for (pipeline_handle, pipeline) in finished_pipelines {
            let existing_pipeline_match = pipeline.task.contains_shader(handle);
            if let Some(shader_type) = existing_pipeline_match {
                assert!(shader_type == shader.shader_type());
                if !remaining_compilations.contains_key(&pipeline_handle) {
                    let task: T = pipeline.task.clone();
                    remaining_compilations.insert(*pipeline_handle, task);
                }
            }
        }

        // Go over all pipelines that can be compiled now.
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

        let ready_tasks: SmallVec<[T; 1]> = ready_handles.iter()
            .flat_map(|handle| remaining_compilations.remove(handle))
            .collect();

        ready_tasks
    }

    fn add_shader_type<T>(
        &self,
        assets: &RendererAssetsReadOnly,
        pipeline_type_manager: &Arc<PipelineTypeManager<T>>,
        handle: ShaderHandle,
        shader: &RendererShader
    ) -> bool
    where
        T: PipelineCompileTask + Send + 'static,
    {
        trace!("Integrating shader {:?} {:?}", shader.shader_type(), handle);
        let ready_tasks = self.collect_ready_pipeline_handles_for_new_shader_handle(assets, pipeline_type_manager, handle, shader);

        if ready_tasks.is_empty() {
            trace!("Nothing to do with shader {:?} {:?}", shader.shader_type(), handle);
            return true;
        }

        trace!("Queuing compile tasks for pipelines with {:?} {:?}", shader.shader_type(), handle);
        #[cfg(not(target_arch = "wasm32"))]
        self.spawn_compile_task(ready_tasks, assets, pipeline_type_manager);
        #[cfg(target_arch = "wasm32")]
        self.spawn_local_compile_task(ready_handles, asset_manager, pipeline_type_manager);

        true
    }

    #[allow(unused)]
    fn spawn_compile_task<T>(
        &self,
        mut ready_tasks: SmallVec<[T; 1]>,
        assets: &RendererAssetsReadOnly,
        pipeline_type_manager: &Arc<PipelineTypeManager<T>>
    )
        where T: PipelineCompileTask + Send + 'static
    {
        let task_pool = bevy_tasks::ComputeTaskPool::get();
        for task in ready_tasks.drain(..) {
            let c_device = self.device.clone();
            let c_manager: Arc<PipelineTypeManager<T>> = pipeline_type_manager.clone();
            let c_delayed_pipeline = pipeline_type_manager.compiled_unpulled_pipelines.clone();
            let shaders = task.collect_shaders_for_compilation(assets);
            let handle = task.handle();

            let async_task = task_pool.spawn(async move {
                crate::autoreleasepool(|| {
                    let pipeline = task.compile(shaders, &c_device);
                    let mut delayed_pipelines = c_delayed_pipeline.lock().unwrap();
                    delayed_pipelines.push((handle, pipeline));
                    c_manager.cond_var.notify_all();
                })
            });
            async_task.detach();
        }
    }


    #[allow(unused)]
    fn spawn_local_compile_task<T>(
        &self,
        mut ready_tasks: SmallVec<[T; 1]>,
        assets: &RendererAssetsReadOnly,
        pipeline_type_manager: &Arc<PipelineTypeManager<T>>
    )
        where T: PipelineCompileTask + Send + 'static,
    {
        let task_pool = bevy_tasks::ComputeTaskPool::get();
        for task in ready_tasks.drain(..) {
            let c_device = self.device.clone();
            let c_manager: Arc<PipelineTypeManager<T>> = pipeline_type_manager.clone();
            let c_delayed_pipeline = pipeline_type_manager.compiled_unpulled_pipelines.clone();
            let shaders = task.collect_shaders_for_compilation(assets);
            let handle = task.handle();

            let async_task = task_pool.spawn_local(async move {
                crate::autoreleasepool(|| {
                    let pipeline = task.compile(shaders, &c_device);
                    let mut delayed_pipelines = c_delayed_pipeline.lock().unwrap();
                    delayed_pipelines.push((handle, pipeline));
                    c_manager.cond_var.notify_all();
                })
            });
            async_task.detach();
        }
    }

    pub fn add_shader(&self, assets: &RendererAssetsReadOnly, handle: ShaderHandle, shader: &RendererShader) {
        let shader_type = shader.shader_type();
        if shader_type == ShaderType::ComputeShader {
            self.add_shader_type(assets, &self.compute, handle, shader);
            return;
        }

        if shader_type == ShaderType::RayGen
            || shader_type == ShaderType::RayClosestHit
            || shader_type == ShaderType::RayMiss {
            self.add_shader_type(assets, &self.rt, handle, shader);
            return;
        }

        if shader_type == ShaderType::FragmentShader {
            self.add_shader_type(assets, &self.graphics, handle, shader);
            self.add_shader_type(assets, &self.mesh_graphics, handle, shader);
            return;
        }

        if shader_type == ShaderType::VertexShader
            || shader_type == ShaderType::GeometryShader
            || shader_type == ShaderType::TessellationControlShader
            || shader_type == ShaderType::TessellationEvaluationShader {
            self.add_shader_type(assets, &self.graphics, handle, shader);
            return;
        }

        if shader_type == ShaderType::MeshShader
            || shader_type == ShaderType::TaskShader {
            self.add_shader_type(assets, &self.mesh_graphics, handle, shader);
            return;
        }

        panic!("Unhandled shader. {:?}", handle);
    }

    pub fn has_remaining_mandatory_compilations(&self) -> bool {
        let has_graphics_compiles = {
            let graphics_remaining = self.graphics.remaining_compilations.lock().unwrap();
            graphics_remaining.iter()
                .any(|(_, t)| !t.is_async)
        };
        let has_mesh_graphics_compiles = {
            let mesh_graphics_remaining = self.mesh_graphics.remaining_compilations.lock().unwrap();
            mesh_graphics_remaining.iter()
                .any(|(_, t)| !t.is_async)
        };
        let has_compute_compiles = {
            let compute_remaining = self.compute.remaining_compilations.lock().unwrap();
            compute_remaining.iter()
                .any(|(_, t)| !t.is_async)
        };
        let has_rt_compiles = {
            let rt_remaining = self.rt.remaining_compilations.lock().unwrap();
            rt_remaining.iter().any(|(_, t)| !t.is_async)
        };
        has_graphics_compiles || has_mesh_graphics_compiles || has_compute_compiles || has_rt_compiles
    }
}
