use std::collections::hash_map::Values;
use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{
    Arc,
    Condvar,
    Mutex,
};

use log::trace;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{PackedShader, Shader as _};
use sourcerenderer_core::{Platform, PlatformPhantomData};

use crate::asset::{
    Asset, AssetHandle, AssetLoadPriority, AssetManager, AssetRef, AssetType, AssetWithHandle, IndexHandle
};
use crate::graphics::*;
use crate::graphics::GraphicsPipelineInfo as ActualGraphicsPipelineInfo;
use crate::graphics::RayTracingPipelineInfo as ActualRayTracingPipelineInfo;

use super::{RendererAssetsReadOnly, RendererGraphicsPipeline, RendererShader};

//
// COMMON
//

pub trait PipelineCompileTask<P: Platform>: Send + Sync + Clone {
    type TShaders;
    type TPipeline: Send + Sync;

    fn asset_type() -> AssetType;
    fn pipeline_from_asset_ref<'a>(asset: AssetRef<'a, P>) -> &'a CompiledPipeline<P, Self>;
    fn pipeline_into_asset(self, pipeline: Arc<Self::TPipeline>) -> Asset<P>;
    fn get_task(pipeline: &CompiledPipeline<P, Self>) -> &Self {
        &pipeline.task
    }

    fn contains_shader(&self, loaded_shader_path: &str) -> Option<ShaderType>;
    fn request_shaders(&self, asset_manager: &Arc<AssetManager<P>>);
    fn request_remaining_shaders(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        loaded_shader_path: &str,
    );
    fn can_compile(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        loaded_shader_path: Option<&str>,
    ) -> bool;
    fn collect_shaders_for_compilation(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
    ) -> Self::TShaders;
    fn compile(
        &self,
        shaders: Self::TShaders,
        device: &Arc<Device<P::GPUBackend>>,
    ) -> Arc<Self::TPipeline>;
    fn is_async(&self) -> bool;
    fn set_async(&mut self);
}

pub struct CompiledPipeline<P: Platform, T: PipelineCompileTask<P>> {
    task: T,
    pub(crate) pipeline: Arc<T::TPipeline>,
}

//
// GRAPHICS
//

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GraphicsPipelineHandle(u64);

impl From<AssetHandle> for GraphicsPipelineHandle {
    fn from(value: AssetHandle) -> Self {
        if let AssetHandle::GraphicsPipeline(handle) = value {
            handle
        } else {
            panic!("Incorrect asset type")
        }
    }
}

impl Into<AssetHandle> for GraphicsPipelineHandle {
    fn into(self) -> AssetHandle {
        AssetHandle::GraphicsPipeline(self)
    }
}

impl IndexHandle for GraphicsPipelineHandle {
    fn new(index: u64) -> Self {
        Self(index)
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
    pub vs: String,
    pub fs: Option<String>,
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

struct StoredGraphicsPipeline<B: GPUBackend> {
    info: StoredGraphicsPipelineInfo,
    pipeline: Arc<B::GraphicsPipeline>,
}

#[derive(Debug)]
pub struct GraphicsCompileTask<P: Platform> {
    info: StoredGraphicsPipelineInfo,
    is_async: bool,
    _p: PhantomData<<P::GPUBackend as GPUBackend>::Device>,
}

impl<P: Platform> Clone for GraphicsCompileTask<P> {
    fn clone(&self) -> Self {
        Self {
            info: self.info.clone(),
            is_async: self.is_async,
            _p: PhantomData
        }
    }
}

struct GraphicsPipeline<P: Platform> {
    task: GraphicsCompileTask<P>,
    pipeline: Arc<<P::GPUBackend as GPUBackend>::GraphicsPipeline>,
}

pub struct GraphicsShaders<B: GPUBackend> {
    vs: Arc<B::Shader>,
    fs: Option<Arc<B::Shader>>,
}

impl<P: Platform> PipelineCompileTask<P> for GraphicsCompileTask<P> {
    type TShaders = GraphicsShaders<P::GPUBackend>;
    type TPipeline = crate::graphics::GraphicsPipeline<P::GPUBackend>;

    fn asset_type() -> AssetType {
        AssetType::GraphicsPipeline
    }
    fn pipeline_from_asset_ref<'a>(asset: AssetRef<'a, P>) -> &'a CompiledPipeline<P, Self> {
        if let AssetRef::<P>::GraphicsPipeline(pipeline) = asset {
            pipeline
        } else {
            panic!("Asset has wrong type")
        }
    }

    fn pipeline_into_asset(self, pipeline: Arc<Self::TPipeline>) -> Asset<P> {
        Asset::<P>::GraphicsPipeline(CompiledPipeline { task: self, pipeline })
    }

    fn contains_shader(&self, loaded_shader_path: &str) -> Option<ShaderType> {
        if &self.info.vs == loaded_shader_path {
            Some(ShaderType::VertexShader)
        } else if self
            .info
            .fs
            .as_ref()
            .map(|fs| loaded_shader_path == fs)
            .unwrap_or(false)
        {
            Some(ShaderType::FragmentShader)
        } else {
            None
        }
    }

    fn can_compile(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        loaded_shader_path: Option<&str>,
    ) -> bool {
        let asset_read = asset_manager.read_renderer_assets();
        (loaded_shader_path.map_or(false, |s| s == &self.info.vs) || asset_read.contains_shader_by_path(&self.info.vs))
            && self
                .info
                .fs
                .as_ref()
                .map(|fs| loaded_shader_path.map_or(false, |s| s == fs) || asset_read.contains_shader_by_path(fs))
                .unwrap_or(true)
    }

    fn request_shaders(&self, asset_manager: &Arc<AssetManager<P>>) {
        asset_manager.request_asset(&self.info.vs, AssetType::Shader, AssetLoadPriority::High);
        if let Some(fs) = self.info.fs.as_ref() {
            asset_manager.request_asset(fs, AssetType::Shader, AssetLoadPriority::High);
        }
    }

    fn request_remaining_shaders(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        loaded_shader_path: &str,
    ) {
        let asset_read = asset_manager.read_renderer_assets();
        if &self.info.vs != loaded_shader_path && !asset_read.contains_shader_by_path(&self.info.vs) {
            asset_manager.request_asset(&self.info.vs, AssetType::Shader, AssetLoadPriority::High);
        }
        if let Some(fs) = self.info.fs.as_ref() {
            if fs != loaded_shader_path && !asset_read.contains_shader_by_path(fs) {
                asset_manager.request_asset(fs, AssetType::Shader, AssetLoadPriority::High);
            }
        }
    }

    fn collect_shaders_for_compilation(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
    ) -> Self::TShaders {
        let asset_read = asset_manager.read_renderer_assets();
        GraphicsShaders {
            vs: asset_read.get_shader_by_path(&self.info.vs).cloned().unwrap(),
            fs: self
                .info
                .fs
                .as_ref()
                .map(|fs| asset_read.get_shader_by_path(fs).cloned().unwrap()),
        }
    }

    fn compile(
        &self,
        shaders: Self::TShaders,
        device: &Arc<Device<P::GPUBackend>>,
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

        device.create_graphics_pipeline(&info, None)
    }

    fn is_async(&self) -> bool {
        self.is_async
    }

    fn set_async(&mut self) {
        self.is_async = true;
    }
}

//
// COMPUTE
//

struct ComputePipeline<B: GPUBackend> {
    path: String,
    pipeline: Arc<B::ComputePipeline>,
}

pub struct ComputeCompileTask<P: Platform> {
    path: String,
    is_async: bool,
    _p: PhantomData<<P::GPUBackend as GPUBackend>::Device>,
}

impl<P: Platform> Clone for ComputeCompileTask<P> {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            is_async: self.is_async,
            _p: PhantomData
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ComputePipelineHandle(u64);

impl IndexHandle for ComputePipelineHandle {
    fn new(index: u64) -> Self {
        Self(index)
    }
}

impl From<AssetHandle> for ComputePipelineHandle {
    fn from(value: AssetHandle) -> Self {
        if let AssetHandle::ComputePipeline(handle) = value {
            handle
        } else {
            panic!("Incorrect asset type")
        }
    }
}

impl Into<AssetHandle> for ComputePipelineHandle {
    fn into(self) -> AssetHandle {
        AssetHandle::ComputePipeline(self)
    }
}

impl<P: Platform> PipelineCompileTask<P> for ComputeCompileTask<P> {
    type TShaders = Arc<<P::GPUBackend as GPUBackend>::Shader>;
    type TPipeline = crate::graphics::ComputePipeline<P::GPUBackend>;

    fn asset_type() -> AssetType {
        AssetType::ComputePipeline
    }
    fn pipeline_from_asset_ref<'a>(asset: AssetRef<'a, P>) -> &'a CompiledPipeline<P, Self> {
        if let AssetRef::<P>::ComputePipeline(pipeline) = asset {
            pipeline
        } else {
            panic!("Asset has wrong type")
        }
    }

    fn pipeline_into_asset(self, pipeline: Arc<Self::TPipeline>) -> Asset<P> {
        Asset::<P>::ComputePipeline(CompiledPipeline { task: self, pipeline })
    }

    fn contains_shader(&self, loaded_shader_path: &str) -> Option<ShaderType> {
        if self.path == loaded_shader_path {
            Some(ShaderType::ComputeShader)
        } else {
            None
        }
    }

    fn request_shaders(&self, asset_manager: &Arc<AssetManager<P>>) {
        asset_manager.request_asset(&self.path, AssetType::Shader, AssetLoadPriority::High);
    }

    fn request_remaining_shaders(
        &self,
        _asset_manager: &Arc<AssetManager<P>>,
        _loaded_shader_path: &str,
    ) {
    }

    fn can_compile(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        loaded_shader_path: Option<&str>,
    ) -> bool {
        let asset_read = asset_manager.read_renderer_assets();
        loaded_shader_path.map_or(false, |s| s == &self.path) || asset_read.contains_shader_by_path(&self.path)
    }

    fn collect_shaders_for_compilation(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
    ) -> Self::TShaders {
        let asset_read = asset_manager.read_renderer_assets();
        asset_read.get_shader_by_path(&self.path).cloned().unwrap()
    }

    fn compile(
        &self,
        shader: Self::TShaders,
        device: &Arc<Device<P::GPUBackend>>,
    ) -> Arc<Self::TPipeline> {
        device.create_compute_pipeline(&shader, None)
    }

    fn is_async(&self) -> bool {
        self.is_async
    }

    fn set_async(&mut self) {
        self.is_async = true;
    }
}

//
// RAY TRACING
//

struct RayTracingPipeline<P: Platform> {
    task: StoredRayTracingPipelineInfo<P>,
    pipeline: Arc<crate::graphics::RayTracingPipeline<P::GPUBackend>>,
}

#[derive(Debug, Clone)]
pub struct RayTracingPipelineInfo<'a> {
    pub ray_gen_shader: &'a str,
    pub closest_hit_shaders: &'a [&'a str],
    pub miss_shaders: &'a [&'a str],
}

#[derive(Debug)]
pub struct StoredRayTracingPipelineInfo<P: Platform> {
    ray_gen_shader: String,
    closest_hit_shaders: SmallVec<[String; 4]>,
    miss_shaders: SmallVec<[String; 1]>,
    is_async: bool,
    _p: PhantomData<<P::GPUBackend as GPUBackend>::Device>,
}

impl<P: Platform> Clone for StoredRayTracingPipelineInfo<P> {
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
pub struct RayTracingPipelineHandle(u64);

impl IndexHandle for RayTracingPipelineHandle {
    fn new(index: u64) -> Self {
        Self(index)
    }
}

impl From<AssetHandle> for RayTracingPipelineHandle {
    fn from(value: AssetHandle) -> Self {
        if let AssetHandle::RayTracingPipeline(handle) = value {
            handle
        } else {
            panic!("Incorrect asset type")
        }
    }
}

impl Into<AssetHandle> for RayTracingPipelineHandle {
    fn into(self) -> AssetHandle {
        AssetHandle::RayTracingPipeline(self)
    }
}

pub struct RayTracingShaders<B: GPUBackend> {
    pub ray_gen_shader: Arc<B::Shader>,
    pub closest_hit_shaders: SmallVec<[Arc<B::Shader>; 4]>,
    pub miss_shaders: SmallVec<[Arc<B::Shader>; 4]>,
}

impl<P: Platform> PipelineCompileTask<P> for StoredRayTracingPipelineInfo<P> {
    type TShaders = RayTracingShaders<P::GPUBackend>;
    type TPipeline = crate::graphics::RayTracingPipeline<P::GPUBackend>;

    fn asset_type() -> AssetType {
        AssetType::RayTracingPipeline
    }
    fn pipeline_from_asset_ref<'a>(asset: AssetRef<'a, P>) -> &'a CompiledPipeline<P, Self> {
        if let AssetRef::<P>::RayTracingPipeline(pipeline) = asset {
            pipeline
        } else {
            panic!("Asset has wrong type")
        }
    }

    fn pipeline_into_asset(self, pipeline: Arc<Self::TPipeline>) -> Asset<P> {
        Asset::<P>::RayTracingPipeline(CompiledPipeline { task: self, pipeline })
    }

    fn contains_shader(&self, loaded_shader_path: &str) -> Option<ShaderType> {
        if &self.ray_gen_shader == loaded_shader_path {
            return Some(ShaderType::RayGen);
        }
        for shader in &self.closest_hit_shaders {
            if shader == loaded_shader_path {
                return Some(ShaderType::RayClosestHit);
            }
        }
        for shader in &self.miss_shaders {
            if shader == loaded_shader_path {
                return Some(ShaderType::RayMiss);
            }
        }
        None
    }

    fn request_shaders(&self, asset_manager: &Arc<AssetManager<P>>) {
        asset_manager.request_asset(
            &self.ray_gen_shader,
            AssetType::Shader,
            AssetLoadPriority::High,
        );
        for shader in &self.closest_hit_shaders {
            asset_manager.request_asset(shader, AssetType::Shader, AssetLoadPriority::High);
        }
        for shader in &self.miss_shaders {
            asset_manager.request_asset(shader, AssetType::Shader, AssetLoadPriority::High);
        }
    }

    fn request_remaining_shaders(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        loaded_shader_path: &str,
    ) {
        let asset_read = asset_manager.read_renderer_assets();
        if loaded_shader_path != &self.ray_gen_shader && !asset_read.contains_shader_by_path(&self.ray_gen_shader)
        {
            asset_manager.request_asset(
                &self.ray_gen_shader,
                AssetType::Shader,
                AssetLoadPriority::High,
            );
        }
        for shader in &self.closest_hit_shaders {
            if loaded_shader_path != shader && !asset_read.contains_shader_by_path(shader) {
                asset_manager.request_asset(shader, AssetType::Shader, AssetLoadPriority::High);
            }
        }
        for shader in &self.miss_shaders {
            if loaded_shader_path != shader && !asset_read.contains_shader_by_path(shader) {
                asset_manager.request_asset(shader, AssetType::Shader, AssetLoadPriority::High);
            }
        }
    }

    fn can_compile(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        loaded_shader_path: Option<&str>,
    ) -> bool {
        let asset_read = asset_manager.read_renderer_assets();
        if !loaded_shader_path.map_or(false, |s| s == &self.ray_gen_shader) && !asset_read.contains_shader_by_path(&self.ray_gen_shader)
        {
            return false;
        }
        for shader in &self.closest_hit_shaders {
            if !loaded_shader_path.map_or(false, |s| s == shader) && !asset_read.contains_shader_by_path(shader) {
                return false;
            }
        }
        for shader in &self.miss_shaders {
            if !loaded_shader_path.map_or(false, |s| s == shader) && !asset_read.contains_shader_by_path(shader) {
                return false;
            }
        }
        true
    }

    fn collect_shaders_for_compilation(
        &self,
        asset_manager: &Arc<AssetManager<P>>
    ) -> Self::TShaders {
        let asset_read: RendererAssetsReadOnly<'_, P> = asset_manager.read_renderer_assets();
        Self::TShaders {
            ray_gen_shader: asset_read.get_shader_by_path(&self.ray_gen_shader).cloned().unwrap(),
            closest_hit_shaders: self
                .closest_hit_shaders
                .iter()
                .map(|shader| asset_read.get_shader_by_path(shader).cloned().unwrap())
                .collect(),
            miss_shaders: self
                .miss_shaders
                .iter()
                .map(|shader| asset_read.get_shader_by_path(shader).cloned().unwrap())
                .collect(),
        }
    }

    fn compile(
        &self,
        shaders: Self::TShaders,
        device: &Arc<Device<P::GPUBackend>>,
    ) -> Arc<Self::TPipeline> {
        let closest_hit_shader_refs: SmallVec<[&<P::GPUBackend as GPUBackend>::Shader; 4]> =
            shaders.closest_hit_shaders.iter().map(|s| s.as_ref()).collect();
        let miss_shaders_refs: SmallVec<[&<P::GPUBackend as GPUBackend>::Shader; 1]> =
            shaders.miss_shaders.iter().map(|s| s.as_ref()).collect();
        let info = ActualRayTracingPipelineInfo::<P::GPUBackend> {
            ray_gen_shader: &shaders.ray_gen_shader,
            closest_hit_shaders: &closest_hit_shader_refs[..],
            miss_shaders: &miss_shaders_refs[..],
        };
        device.create_raytracing_pipeline(&info, None).unwrap()
    }

    fn is_async(&self) -> bool {
        self.is_async
    }

    fn set_async(&mut self) {
        self.is_async = true;
    }
}

//
// BASE
//

pub struct ShaderManager<P: Platform> {
    device: Arc<Device<P::GPUBackend>>,
    graphics: Arc<PipelineTypeManager<P, GraphicsPipelineHandle, GraphicsCompileTask<P>>>,
    compute: Arc<PipelineTypeManager<P, ComputePipelineHandle, ComputeCompileTask<P>>>,
    rt: Arc<
        PipelineTypeManager<P, RayTracingPipelineHandle, StoredRayTracingPipelineInfo<P>>,
    >
}

struct PipelineTypeManager<P, THandle, T>
where
    P: Platform,
    THandle: IndexHandle + Hash + PartialEq + Eq + Clone + Copy + Send + Sync + From<AssetHandle>,
    T: PipelineCompileTask<P>,
{
    remaining_compilations: Mutex<HashMap<THandle, T>>,
    cond_var: Condvar,
    _platform: PlatformPhantomData<P>
}

impl<P, THandle, T> PipelineTypeManager<P, THandle, T>
where
    P: Platform,
    THandle: IndexHandle + Hash + PartialEq + Eq + Clone + Copy + Send + Sync + From<AssetHandle>,
    T: PipelineCompileTask<P>,
{
    fn new() -> Self {
        Self {
            remaining_compilations: Mutex::new(HashMap::new()),
            cond_var: Condvar::new(),
            _platform: Default::default()
        }
    }
}

impl<P: Platform> ShaderManager<P> {
    pub fn new(
        device: &Arc<Device<P::GPUBackend>>,
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
        asset_manager: &Arc<AssetManager<P>>,
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

        let stored = StoredGraphicsPipelineInfo {
            vs: info.vs.to_string(),
            fs: info.fs.map(|s| s.to_string()),
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
            GraphicsCompileTask::<P> {
                info: stored,
                is_async: false,
                _p: PhantomData,
            },
        )
    }

    pub fn request_compute_pipeline(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        path: &str) -> ComputePipelineHandle {
        self.request_pipeline_internal(
            asset_manager,
            &self.compute,
            ComputeCompileTask::<P> {
                path: path.to_string(),
                is_async: false,
                _p: PhantomData,
            },
        )
    }

    pub fn request_ray_tracing_pipeline(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        info: &RayTracingPipelineInfo,
    ) -> RayTracingPipelineHandle {
        self.request_pipeline_internal(
            asset_manager,
            &self.rt,
            StoredRayTracingPipelineInfo::<P> {
                closest_hit_shaders: info
                    .closest_hit_shaders
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                miss_shaders: info.miss_shaders.iter().map(|s| s.to_string()).collect(),
                ray_gen_shader: info.ray_gen_shader.to_string(),
                is_async: false,
                _p: PhantomData,
            },
        )
    }

    fn request_pipeline_internal<T, THandle>(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        pipeline_type_manager: &Arc<PipelineTypeManager<P, THandle, T>>,
        task: T,
    ) -> THandle
    where
        THandle: IndexHandle + Hash + PartialEq + Eq + Clone + Copy + Send + Sync + From<AssetHandle>,
        T: PipelineCompileTask<P>,
    {
        let handle: THandle = asset_manager.reserve_handle_without_path(T::asset_type()).into();
        task.request_shaders(asset_manager);
        let mut remaining = pipeline_type_manager.remaining_compilations.lock().unwrap();
        remaining.insert(handle, task);
        handle
    }

    fn add_shader_type<THandle, T>(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        pipeline_type_manager: &Arc<PipelineTypeManager<P, THandle, T>>,
        path: &str,
        shader: &RendererShader<P::GPUBackend>
    ) -> bool
    where
        THandle: IndexHandle + Hash + PartialEq + Eq + Clone + Copy + Send + Sync + From<AssetHandle> + Into<AssetHandle> + 'static,
        T: PipelineCompileTask<P> + 'static,
    {
        {
            trace!("Integrating shader {:?} {}", shader.shader_type(), path);
            let mut ready_handles = SmallVec::<[THandle; 1]>::new();
            {

                // Find all pipelines that use this shader and queue new compile tasks for those.
                // This is done because add_shader will get called when a shader has changed on disk, so we need to load
                // all remaining shaders of a pipeline and recompile it.

                let mut remaining_compilations: std::sync::MutexGuard<'_, HashMap<THandle, T>> = pipeline_type_manager.remaining_compilations.lock().unwrap();
                let assets_read = asset_manager.read_renderer_assets();
                let compiled_pipeline_handles = assets_read.all_pipeline_handles(T::asset_type());
                for handle in compiled_pipeline_handles {
                    let asset_ref = assets_read.get(handle).unwrap();
                    let pipeline: &CompiledPipeline<P, T> = T::pipeline_from_asset_ref(asset_ref);
                    let existing_pipeline_match = pipeline.task.contains_shader(path);
                    if let Some(shader_type) = existing_pipeline_match {
                        trace!("Found pipeline that contains shader {:?} {}. Queing remaining shaders if necessary.", shader.shader_type(), path);
                        assert!(shader_type  == shader.shader_type());
                        pipeline.task.request_remaining_shaders(
                            asset_manager,
                            path,
                        );
                        let typed_handle: THandle = handle.into();
                        if !remaining_compilations.contains_key(&typed_handle) {
                            let mut task: T = pipeline.task.clone();
                            task.set_async();
                            remaining_compilations.insert(typed_handle, task);
                        }
                    }
                }

                for (handle, task) in remaining_compilations.iter() {
                    let remaining_compile_match = task.contains_shader(path);
                    if let Some(shader_type) = remaining_compile_match {
                        trace!("Found pipeline that contains shader {:?} {}. Testing if its ready to compile.", shader.shader_type(), path);
                        assert!(shader_type == shader.shader_type());
                        if task.can_compile(asset_manager, Some(path)) {
                            trace!("Pipeline that contains shader {:?} {} is ready to compile.", shader.shader_type(), path);
                            ready_handles.push(*handle);
                        }
                    }
                }
            }

            if ready_handles.is_empty() {
                trace!("Nothing to do with shader {:?} {}", shader.shader_type(), path);
                return true;
            }

            trace!("Queuing compile tasks for pipelines with {:?} {}", shader.shader_type(), path);
            let c_device = self.device.clone();
            let c_manager: Arc<PipelineTypeManager<P, THandle, T>> = pipeline_type_manager.clone();
            let c_asset_manager = asset_manager.clone();
            c_manager.cond_var.notify_all();
            let task_pool = bevy_tasks::ComputeTaskPool::get();
            let task = task_pool.spawn(async move {
                for handle in ready_handles.drain(..) {
                    let task: T;
                    let shaders: T::TShaders;

                    {
                        let mut remaining_compilations = c_manager.remaining_compilations.lock().unwrap();
                        task = remaining_compilations.remove(&handle).unwrap();
                        shaders = task.collect_shaders_for_compilation(&c_asset_manager);
                    };
                    let pipeline: Arc<<T as PipelineCompileTask<P>>::TPipeline> = task.compile(shaders, &c_device);
                    let generic_handle: AssetHandle = handle.into();
                    c_asset_manager.add_asset_with_handle(AssetWithHandle::combine(generic_handle, T::pipeline_into_asset(task, pipeline)));
                }
                c_manager.cond_var.notify_all();
            });
            task.detach();
            true
        }
    }

    pub fn add_shader(&self, asset_manager: &Arc<AssetManager<P>>, path: &str, shader: &RendererShader<P::GPUBackend>) {
        if !match shader.shader_type() {
            ShaderType::ComputeShader => self.add_shader_type(asset_manager, &self.compute, path, shader),
            ShaderType::RayGen | ShaderType::RayClosestHit | ShaderType::RayMiss => self.add_shader_type(asset_manager, &self.rt, path, shader),
            ShaderType::FragmentShader | ShaderType::VertexShader | ShaderType::GeometryShader | ShaderType::TessellationControlShader | ShaderType::TessellationEvaluationShader =>
                self.add_shader_type(asset_manager, &self.graphics, path, shader),
        } {
            panic!("Unhandled shader. {}", path);
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
