use std::collections::HashMap;
use std::collections::hash_map::Iter;
use std::hash::Hash;
use std::sync::Arc;

use smallvec::SmallVec;
use sourcerenderer_core::gpu::{GPUMaybeSend, GPUMaybeSync, SpecConstValue};

use super::{RendererAssetWithHandle, RendererAssetsReadOnly, RendererShader};
use crate::asset::{AssetHandle, AssetLoadPriority, AssetManager, AssetType, ShaderHandle};
use crate::graphics::gpu::Shader as _;
use crate::graphics::{
    GraphicsPipelineInfo as ActualGraphicsPipelineInfo,
    MeshGraphicsPipelineInfo as ActualMeshGraphicsPipelineInfo,
    PipelineShaderStage as ActualPipelineShaderStage,
    RayTracingPipelineInfo as ActualRayTracingPipelineInfo, *,
};
use crate::{Condvar, Mutex};

//
// COMMON
//

pub trait PipelineCompileTask: Clone + Sized {
    type TPipelineHandle: Hash
        + PartialEq
        + Eq
        + Clone
        + Copy
        + From<AssetHandle>
        + Into<AssetHandle>
        + Send
        + Sync
        + std::fmt::Debug;

    type TShaders: GPUMaybeSend;
    type TPipeline: GPUMaybeSend + GPUMaybeSync;

    fn asset_type() -> AssetType;

    fn finished_pipelines<'a>(
        assets: &'a RendererAssetsReadOnly,
    ) -> Iter<'a, Self::TPipelineHandle, CompiledPipeline<Self>>;

    fn name(&self) -> Option<String>;
    fn handle(&self) -> Self::TPipelineHandle;
    fn contains_shader(&self, handle: ShaderHandle) -> Option<ShaderType>;
    fn request_shader_refresh(&self, asset_manager: &Arc<AssetManager>);
    fn can_compile(&self, renderer_assets_read: &RendererAssetsReadOnly<'_>) -> bool;
    fn collect_shaders_for_compilation(
        &self,
        renderer_assets_read: &RendererAssetsReadOnly<'_>,
    ) -> Self::TShaders;
    fn compile(&self, shaders: Self::TShaders, device: &Arc<Device>) -> Arc<Self::TPipeline>;
}

pub struct CompiledPipeline<T: PipelineCompileTask> {
    task: T,
    pub(crate) pipeline: Arc<T::TPipeline>,
}

fn hashmap_clone_key_ref<
    'a,
    TKey: AsRef<TKeyNew> + Hash + PartialEq + Eq,
    TKeyNew: Hash + PartialEq + Eq + ?Sized,
    TValue: Clone,
>(
    hashmap: &'a HashMap<TKey, TValue>,
) -> HashMap<&'a TKeyNew, TValue> {
    let mut borrowed = HashMap::<&'a TKeyNew, TValue>::new();
    for (name, value) in hashmap {
        borrowed.insert(name.as_ref(), value.clone());
    }
    borrowed
}

fn hashmap_clone_key_owned<
    'a: 'b,
    'b,
    TKey: Hash + PartialEq + Eq + ?Sized,
    TKeyNew: From<&'b TKey> + Hash + PartialEq + Eq,
    TValue: Clone,
>(
    hashmap: &HashMap<&'a TKey, TValue>,
) -> HashMap<TKeyNew, TValue> {
    let mut borrowed = HashMap::<TKeyNew, TValue>::new();
    for (name, value) in hashmap {
        borrowed.insert((*name).into(), value.clone());
    }
    borrowed
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
pub struct PathPipelineShaderStage<'a> {
    pub shader_path: &'a str,
    pub spec_consts: Option<&'a HashMap<u32, SpecConstValue>>,
}

impl<'a> PathPipelineShaderStage<'a> {
    pub fn empty_spec_consts(path: &'a str) -> Self {
        Self {
            shader_path: path,
            spec_consts: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GraphicsPipelineInfo<'a> {
    pub vs: PathPipelineShaderStage<'a>,
    pub fs: Option<PathPipelineShaderStage<'a>>,
    pub vertex_layout: VertexLayoutInfo<'a>,
    pub rasterizer: RasterizerInfo,
    pub depth_stencil: DepthStencilInfo,
    pub blend: BlendInfo<'a>,
    pub primitive_type: PrimitiveType,
    pub render_target_formats: &'a [Format],
    pub depth_stencil_format: Format,
}

#[derive(Debug, Clone)]
pub struct HandlePipelineShaderStage {
    shader_handle: ShaderHandle,
    spec_consts: HashMap<u32, SpecConstValue>,
}

#[derive(Debug, Clone)]
pub struct GraphicsCompileTask {
    vs: HandlePipelineShaderStage,
    fs: Option<HandlePipelineShaderStage>,
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

pub struct ArcPipelineShaderStage {
    shader: Arc<Shader>,
    spec_consts: HashMap<u32, SpecConstValue>,
}

impl<'a> Into<ActualPipelineShaderStage<'a>> for &'a ArcPipelineShaderStage {
    fn into(self) -> ActualPipelineShaderStage<'a> {
        ActualPipelineShaderStage {
            shader: &self.shader,
            spec_consts: &self.spec_consts,
        }
    }
}

pub struct GraphicsShaders {
    vs: ArcPipelineShaderStage,
    fs: Option<ArcPipelineShaderStage>,
}

impl PipelineCompileTask for GraphicsCompileTask {
    type TShaders = GraphicsShaders;
    type TPipeline = crate::graphics::GraphicsPipeline;
    type TPipelineHandle = GraphicsPipelineHandle;

    fn asset_type() -> AssetType {
        AssetType::GraphicsPipeline
    }

    fn name(&self) -> Option<String> {
        Some(format!(
            "GraphicsPipeline: VS: {:?}, FS: {:?}",
            &self.vs,
            self.fs.as_ref()
        ))
    }

    fn handle(&self) -> Self::TPipelineHandle {
        self.handle
    }

    fn contains_shader(&self, handle: ShaderHandle) -> Option<ShaderType> {
        if self.vs.shader_handle == handle {
            Some(ShaderType::VertexShader)
        } else if self
            .fs
            .as_ref()
            .map(|fs| fs.shader_handle == handle)
            .unwrap_or(false)
        {
            Some(ShaderType::FragmentShader)
        } else {
            None
        }
    }

    fn can_compile(&self, renderer_assets_read: &RendererAssetsReadOnly<'_>) -> bool {
        renderer_assets_read
            .get_shader(self.vs.shader_handle)
            .is_some()
            && self
                .fs
                .as_ref()
                .map(|fs| renderer_assets_read.get_shader(fs.shader_handle).is_some())
                .unwrap_or(true)
    }

    fn request_shader_refresh(&self, asset_manager: &Arc<AssetManager>) {
        asset_manager
            .request_asset_refresh_by_handle(self.vs.shader_handle, AssetLoadPriority::High);
        if let Some(fs) = self.fs.as_ref() {
            asset_manager
                .request_asset_refresh_by_handle(fs.shader_handle, AssetLoadPriority::High);
        }
    }

    fn collect_shaders_for_compilation(
        &self,
        renderer_assets_read: &RendererAssetsReadOnly<'_>,
    ) -> Self::TShaders {
        GraphicsShaders {
            vs: ArcPipelineShaderStage {
                shader: renderer_assets_read
                    .get_shader(self.vs.shader_handle)
                    .cloned()
                    .unwrap(),
                spec_consts: self.vs.spec_consts.clone(),
            },
            fs: self.fs.as_ref().map(|fs| ArcPipelineShaderStage {
                shader: renderer_assets_read
                    .get_shader(fs.shader_handle)
                    .cloned()
                    .unwrap(),
                spec_consts: fs.spec_consts.clone(),
            }),
        }
    }

    fn finished_pipelines<'a>(
        assets: &'a RendererAssetsReadOnly,
    ) -> Iter<'a, Self::TPipelineHandle, CompiledPipeline<Self>> {
        assets.all_graphics_pipelines()
    }

    fn compile(&self, shaders: Self::TShaders, device: &Arc<Device>) -> Arc<Self::TPipeline> {
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
            vs: (&shaders.vs).into(),
            fs: shaders.fs.as_ref().map(|s| s.into()),
            vertex_layout: input_layout,
            rasterizer: self.rasterizer.clone(),
            depth_stencil: self.depth_stencil.clone(),
            blend: blend_info,
            primitive_type: self.primitive_type,
            render_target_formats: &self.render_target_formats,
            depth_stencil_format: self.depth_stencil_format,
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
    pub ts: Option<PathPipelineShaderStage<'a>>,
    pub ms: PathPipelineShaderStage<'a>,
    pub fs: Option<PathPipelineShaderStage<'a>>,
    pub rasterizer: RasterizerInfo,
    pub depth_stencil: DepthStencilInfo,
    pub blend: BlendInfo<'a>,
    pub render_target_formats: &'a [Format],
    pub depth_stencil_format: Format,
}

#[derive(Debug, Clone)]
pub struct MeshGraphicsCompileTask {
    ts: Option<HandlePipelineShaderStage>,
    ms: HandlePipelineShaderStage,
    fs: Option<HandlePipelineShaderStage>,
    rasterizer: RasterizerInfo,
    depth_stencil: DepthStencilInfo,
    blend: StoredBlendInfo,
    render_target_formats: SmallVec<[Format; 8]>,
    depth_stencil_format: Format,
    is_async: bool,
    handle: MeshGraphicsPipelineHandle,
}

pub struct MeshGraphicsShaders {
    ts: Option<ArcPipelineShaderStage>,
    ms: ArcPipelineShaderStage,
    fs: Option<ArcPipelineShaderStage>,
}

impl PipelineCompileTask for MeshGraphicsCompileTask {
    type TShaders = MeshGraphicsShaders;
    type TPipeline = crate::graphics::MeshGraphicsPipeline;
    type TPipelineHandle = MeshGraphicsPipelineHandle;

    fn asset_type() -> AssetType {
        AssetType::MeshGraphicsPipeline
    }

    fn name(&self) -> Option<String> {
        Some(format!(
            "GraphicsPipeline: TS: {:?}, MS: {:?}, FS: {:?}",
            self.ts.as_ref(),
            &self.ms,
            self.fs.as_ref()
        ))
    }

    fn handle(&self) -> Self::TPipelineHandle {
        self.handle
    }

    fn contains_shader(&self, handle: ShaderHandle) -> Option<ShaderType> {
        if self.ms.shader_handle == handle {
            Some(ShaderType::MeshShader)
        } else if self
            .fs
            .as_ref()
            .map(|fs| fs.shader_handle == handle)
            .unwrap_or(false)
        {
            Some(ShaderType::FragmentShader)
        } else if self
            .ts
            .as_ref()
            .map(|ts| ts.shader_handle == handle)
            .unwrap_or(false)
        {
            Some(ShaderType::TaskShader)
        } else {
            None
        }
    }

    fn can_compile(&self, renderer_assets_read: &RendererAssetsReadOnly<'_>) -> bool {
        renderer_assets_read
            .get_shader(self.ms.shader_handle)
            .is_some()
            && self
                .ts
                .as_ref()
                .map(|ts| renderer_assets_read.get_shader(ts.shader_handle).is_some())
                .unwrap_or(true)
            && self
                .fs
                .as_ref()
                .map(|fs| renderer_assets_read.get_shader(fs.shader_handle).is_some())
                .unwrap_or(true)
    }

    fn request_shader_refresh(&self, asset_manager: &Arc<AssetManager>) {
        asset_manager
            .request_asset_refresh_by_handle(self.ms.shader_handle, AssetLoadPriority::High);
        if let Some(ts) = self.ts.as_ref() {
            asset_manager
                .request_asset_refresh_by_handle(ts.shader_handle, AssetLoadPriority::High);
        }
        if let Some(fs) = self.fs.as_ref() {
            asset_manager
                .request_asset_refresh_by_handle(fs.shader_handle, AssetLoadPriority::High);
        }
    }

    fn collect_shaders_for_compilation(
        &self,
        renderer_assets_read: &RendererAssetsReadOnly<'_>,
    ) -> Self::TShaders {
        MeshGraphicsShaders {
            ts: self.ts.as_ref().map(|ts| ArcPipelineShaderStage {
                shader: renderer_assets_read
                    .get_shader(ts.shader_handle)
                    .cloned()
                    .unwrap(),
                spec_consts: ts.spec_consts.clone(),
            }),
            ms: ArcPipelineShaderStage {
                shader: renderer_assets_read
                    .get_shader(self.ms.shader_handle)
                    .cloned()
                    .unwrap(),
                spec_consts: self.ms.spec_consts.clone(),
            },
            fs: self.fs.as_ref().map(|fs| ArcPipelineShaderStage {
                shader: renderer_assets_read
                    .get_shader(fs.shader_handle)
                    .cloned()
                    .unwrap(),
                spec_consts: fs.spec_consts.clone(),
            }),
        }
    }

    fn finished_pipelines<'a>(
        assets: &'a RendererAssetsReadOnly,
    ) -> Iter<'a, Self::TPipelineHandle, CompiledPipeline<Self>> {
        assets.all_mesh_graphics_pipelines()
    }

    fn compile(&self, shaders: Self::TShaders, device: &Arc<Device>) -> Arc<Self::TPipeline> {
        let blend_info = BlendInfo {
            alpha_to_coverage_enabled: self.blend.alpha_to_coverage_enabled,
            logic_op_enabled: self.blend.logic_op_enabled,
            logic_op: self.blend.logic_op,
            attachments: &self.blend.attachments[..],
            constants: self.blend.constants,
        };

        let info = ActualMeshGraphicsPipelineInfo {
            ts: shaders.ts.as_ref().map(|s| ActualPipelineShaderStage {
                shader: s.shader.as_ref(),
                spec_consts: &s.spec_consts,
            }),
            ms: ActualPipelineShaderStage {
                shader: shaders.ms.shader.as_ref(),
                spec_consts: &shaders.ms.spec_consts,
            },
            fs: shaders.fs.as_ref().map(|s| ActualPipelineShaderStage {
                shader: s.shader.as_ref(),
                spec_consts: &s.spec_consts,
            }),
            rasterizer: self.rasterizer.clone(),
            depth_stencil: self.depth_stencil.clone(),
            blend: blend_info,
            render_target_formats: &self.render_target_formats,
            depth_stencil_format: self.depth_stencil_format,
        };

        device.create_mesh_graphics_pipeline(&info, self.name().as_ref().map(|n| n as &str))
    }
}

//
// COMPUTE
//

#[derive(Debug, Clone)]
pub struct ComputeCompileTask {
    shader: HandlePipelineShaderStage,
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
    type TShaders = ArcPipelineShaderStage;
    type TPipeline = crate::graphics::ComputePipeline;
    type TPipelineHandle = ComputePipelineHandle;

    fn asset_type() -> AssetType {
        AssetType::ComputePipeline
    }

    fn name(&self) -> Option<String> {
        Some(format!("ComputePipeline: {:?}", self.shader.shader_handle))
    }

    fn handle(&self) -> Self::TPipelineHandle {
        self.handle
    }

    fn contains_shader(&self, shader_handle: ShaderHandle) -> Option<ShaderType> {
        if self.shader.shader_handle == shader_handle {
            Some(ShaderType::ComputeShader)
        } else {
            None
        }
    }

    fn request_shader_refresh(&self, asset_manager: &Arc<AssetManager>) {
        asset_manager
            .request_asset_refresh_by_handle(self.shader.shader_handle, AssetLoadPriority::High);
    }

    fn can_compile(&self, renderer_assets_read: &RendererAssetsReadOnly<'_>) -> bool {
        renderer_assets_read
            .get_shader(self.shader.shader_handle)
            .is_some()
    }

    fn collect_shaders_for_compilation(
        &self,
        renderer_assets_read: &RendererAssetsReadOnly<'_>,
    ) -> Self::TShaders {
        ArcPipelineShaderStage {
            shader: renderer_assets_read
                .get_shader(self.shader.shader_handle)
                .cloned()
                .unwrap(),
            spec_consts: self.shader.spec_consts.clone(),
        }
    }

    fn finished_pipelines<'a>(
        assets: &'a RendererAssetsReadOnly,
    ) -> Iter<'a, Self::TPipelineHandle, CompiledPipeline<Self>> {
        assets.all_compute_pipelines()
    }

    fn compile(&self, shader: Self::TShaders, device: &Arc<Device>) -> Arc<Self::TPipeline> {
        device.create_compute_pipeline(
            &ActualPipelineShaderStage {
                shader: &shader.shader,
                spec_consts: &shader.spec_consts,
            },
            self.name().as_ref().map(|n| n as &str),
        )
    }
}

//
// RAY TRACING
//

#[derive(Debug, Clone)]
pub struct RayTracingPipelineInfo<'a> {
    pub ray_gen_shader: PathPipelineShaderStage<'a>,
    pub closest_hit_shaders: &'a [PathPipelineShaderStage<'a>],
    pub any_hit_shaders: &'a [PathPipelineShaderStage<'a>],
    pub miss_shaders: &'a [PathPipelineShaderStage<'a>],
}

#[derive(Debug, Clone)]
pub struct RayTracingCompileTask {
    ray_gen_shader: HandlePipelineShaderStage,
    closest_hit_shaders: SmallVec<[HandlePipelineShaderStage; 4]>,
    any_hit_shaders: SmallVec<[HandlePipelineShaderStage; 4]>,
    miss_shaders: SmallVec<[HandlePipelineShaderStage; 1]>,
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
    ray_gen_shader: ArcPipelineShaderStage,
    closest_hit_shaders: SmallVec<[ArcPipelineShaderStage; 4]>,
    any_hit_shaders: SmallVec<[ArcPipelineShaderStage; 4]>,
    miss_shaders: SmallVec<[ArcPipelineShaderStage; 4]>,
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
        if self.ray_gen_shader.shader_handle == handle {
            return Some(ShaderType::RayGen);
        }
        for shader in &self.closest_hit_shaders {
            if shader.shader_handle == handle {
                return Some(ShaderType::RayClosestHit);
            }
        }
        for shader in &self.miss_shaders {
            if shader.shader_handle == handle {
                return Some(ShaderType::RayMiss);
            }
        }
        None
    }

    fn request_shader_refresh(&self, asset_manager: &Arc<AssetManager>) {
        asset_manager.request_asset_refresh_by_handle(
            self.ray_gen_shader.shader_handle,
            AssetLoadPriority::High,
        );
        for shader in &self.closest_hit_shaders {
            asset_manager
                .request_asset_refresh_by_handle(shader.shader_handle, AssetLoadPriority::High);
        }
        for shader in &self.any_hit_shaders {
            asset_manager
                .request_asset_refresh_by_handle(shader.shader_handle, AssetLoadPriority::High);
        }
        for shader in &self.miss_shaders {
            asset_manager
                .request_asset_refresh_by_handle(shader.shader_handle, AssetLoadPriority::High);
        }
    }

    fn can_compile(&self, renderer_assets_read: &RendererAssetsReadOnly<'_>) -> bool {
        if !renderer_assets_read
            .get_shader(self.ray_gen_shader.shader_handle)
            .is_some()
        {
            return false;
        }
        for shader in &self.closest_hit_shaders {
            if !renderer_assets_read
                .get_shader(shader.shader_handle)
                .is_some()
            {
                return false;
            }
        }
        for shader in &self.any_hit_shaders {
            if !renderer_assets_read
                .get_shader(shader.shader_handle)
                .is_some()
            {
                return false;
            }
        }
        for shader in &self.miss_shaders {
            if !renderer_assets_read
                .get_shader(shader.shader_handle)
                .is_some()
            {
                return false;
            }
        }
        true
    }

    fn collect_shaders_for_compilation(
        &self,
        renderer_assets_read: &RendererAssetsReadOnly<'_>,
    ) -> Self::TShaders {
        Self::TShaders {
            ray_gen_shader: ArcPipelineShaderStage {
                shader: renderer_assets_read
                    .get_shader(self.ray_gen_shader.shader_handle)
                    .cloned()
                    .unwrap(),
                spec_consts: self.ray_gen_shader.spec_consts.clone(),
            },
            closest_hit_shaders: self
                .closest_hit_shaders
                .iter()
                .map(|shader| ArcPipelineShaderStage {
                    shader: renderer_assets_read
                        .get_shader(shader.shader_handle)
                        .cloned()
                        .unwrap(),
                    spec_consts: shader.spec_consts.clone(),
                })
                .collect(),
            any_hit_shaders: self
                .any_hit_shaders
                .iter()
                .map(|shader| ArcPipelineShaderStage {
                    shader: renderer_assets_read
                        .get_shader(shader.shader_handle)
                        .cloned()
                        .unwrap(),
                    spec_consts: shader.spec_consts.clone(),
                })
                .collect(),
            miss_shaders: self
                .miss_shaders
                .iter()
                .map(|shader| ArcPipelineShaderStage {
                    shader: renderer_assets_read
                        .get_shader(shader.shader_handle)
                        .cloned()
                        .unwrap(),
                    spec_consts: shader.spec_consts.clone(),
                })
                .collect(),
        }
    }

    fn finished_pipelines<'a>(
        assets: &'a RendererAssetsReadOnly,
    ) -> Iter<'a, Self::TPipelineHandle, CompiledPipeline<Self>> {
        assets.all_ray_tracing_pipelines()
    }

    fn compile(&self, shaders: Self::TShaders, device: &Arc<Device>) -> Arc<Self::TPipeline> {
        let closest_hit_shader_refs: SmallVec<[ActualPipelineShaderStage; 4]> = shaders
            .closest_hit_shaders
            .iter()
            .map(|s| ActualPipelineShaderStage {
                shader: s.shader.as_ref(),
                spec_consts: &s.spec_consts,
            })
            .collect();
        let any_hit_shader_refs: SmallVec<[ActualPipelineShaderStage; 4]> = shaders
            .any_hit_shaders
            .iter()
            .map(|s| ActualPipelineShaderStage {
                shader: s.shader.as_ref(),
                spec_consts: &s.spec_consts,
            })
            .collect();
        let miss_shaders_refs: SmallVec<[ActualPipelineShaderStage; 1]> = shaders
            .miss_shaders
            .iter()
            .map(|s| ActualPipelineShaderStage {
                shader: s.shader.as_ref(),
                spec_consts: &s.spec_consts,
            })
            .collect();
        let ray_gen_stage = ActualPipelineShaderStage {
            shader: shaders.ray_gen_shader.shader.as_ref(),
            spec_consts: &shaders.ray_gen_shader.spec_consts,
        };
        let info = ActualRayTracingPipelineInfo {
            ray_gen_shader: ray_gen_stage,
            closest_hit_shaders: &closest_hit_shader_refs[..],
            any_hit_shaders: &any_hit_shader_refs[..],
            miss_shaders: &miss_shaders_refs[..],
        };
        device
            .create_raytracing_pipeline(&info, self.name().as_ref().map(|n| n as &str))
            .unwrap()
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
    compiled_unpulled_pipelines: Arc<Mutex<Vec<(T::TPipelineHandle, T, Arc<T::TPipeline>)>>>,
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
    pub fn new(device: &Arc<Device>) -> Self {
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
        assets: &RendererAssetsReadOnly,
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

        let asset_manager = assets.asset_manager();
        let (vs_handle, _) = asset_manager.request_asset(
            info.vs.shader_path,
            AssetType::Shader,
            AssetLoadPriority::Normal,
        );
        let fs_stage = info.fs.as_ref().map(|fs| HandlePipelineShaderStage {
            shader_handle: asset_manager
                .request_asset(fs.shader_path, AssetType::Shader, AssetLoadPriority::Normal)
                .0
                .into(),
            spec_consts: fs.spec_consts.map(|s| (*s).clone()).unwrap_or_default(),
        });

        let handle: GraphicsPipelineHandle = asset_manager
            .reserve_handle_without_path(AssetType::GraphicsPipeline)
            .into();
        {
            let mut remaining = self.graphics.remaining_compilations.lock().unwrap();
            remaining.insert(
                handle,
                GraphicsCompileTask {
                    vs: HandlePipelineShaderStage {
                        shader_handle: vs_handle.into(),
                        spec_consts: info
                            .vs
                            .spec_consts
                            .map(|s| (*s).clone())
                            .unwrap_or_default(),
                    },
                    fs: fs_stage,
                    vertex_layout: stored_input_layout,
                    rasterizer: info.rasterizer.clone(),
                    depth_stencil: info.depth_stencil.clone(),
                    blend: stored_blend,
                    primitive_type: info.primitive_type,
                    render_target_formats: info.render_target_formats.iter().copied().collect(),
                    depth_stencil_format: info.depth_stencil_format,
                    is_async: false,
                    handle,
                },
            );
        }
        self.update_remaining_compilations_type(assets, &self.graphics);
        handle
    }

    pub fn request_mesh_graphics_pipeline(
        &self,
        assets: &RendererAssetsReadOnly,
        info: &MeshGraphicsPipelineInfo,
    ) -> MeshGraphicsPipelineHandle {
        let stored_blend = StoredBlendInfo {
            alpha_to_coverage_enabled: info.blend.alpha_to_coverage_enabled,
            logic_op_enabled: info.blend.logic_op_enabled,
            logic_op: info.blend.logic_op,
            attachments: info.blend.attachments.iter().cloned().collect(),
            constants: info.blend.constants.clone(),
        };

        let asset_manager = assets.asset_manager();
        let ts_handle = info.ts.as_ref().map(|ts| HandlePipelineShaderStage {
            shader_handle: asset_manager
                .request_asset(ts.shader_path, AssetType::Shader, AssetLoadPriority::Normal)
                .0
                .into(),
            spec_consts: ts.spec_consts.map(|s| (*s).clone()).unwrap_or_default(),
        });
        let ms_handle = HandlePipelineShaderStage {
            shader_handle: asset_manager
                .request_asset(
                    info.ms.shader_path,
                    AssetType::Shader,
                    AssetLoadPriority::Normal,
                )
                .0
                .into(),
            spec_consts: info
                .ms
                .spec_consts
                .map(|s| (*s).clone())
                .unwrap_or_default(),
        };
        let fs_handle = info.fs.as_ref().map(|fs| HandlePipelineShaderStage {
            shader_handle: asset_manager
                .request_asset(fs.shader_path, AssetType::Shader, AssetLoadPriority::Normal)
                .0
                .into(),
            spec_consts: fs.spec_consts.map(|s| (*s).clone()).unwrap_or_default(),
        });

        let handle: MeshGraphicsPipelineHandle = asset_manager
            .reserve_handle_without_path(AssetType::MeshGraphicsPipeline)
            .into();
        {
            let mut remaining = self.mesh_graphics.remaining_compilations.lock().unwrap();
            remaining.insert(
                handle,
                MeshGraphicsCompileTask {
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
                },
            );
        }
        self.update_remaining_compilations_type(assets, &self.mesh_graphics);
        handle
    }

    pub fn request_compute_pipeline(
        &self,
        assets: &RendererAssetsReadOnly,
        shader: PathPipelineShaderStage,
    ) -> ComputePipelineHandle {
        let asset_manager = assets.asset_manager();
        let (shader_handle, _) = asset_manager.request_asset(
            shader.shader_path,
            AssetType::Shader,
            AssetLoadPriority::Normal,
        );

        let handle: ComputePipelineHandle = asset_manager
            .reserve_handle_without_path(AssetType::ComputePipeline)
            .into();
        {
            let mut remaining = self.compute.remaining_compilations.lock().unwrap();
            remaining.insert(
                handle,
                ComputeCompileTask {
                    shader: HandlePipelineShaderStage {
                        shader_handle: shader_handle.into(),
                        spec_consts: shader.spec_consts.map(|s| (*s).clone()).unwrap_or_default(),
                    },
                    is_async: false,
                    handle,
                },
            );
        }
        self.update_remaining_compilations_type(assets, &self.compute);
        handle
    }

    pub fn request_ray_tracing_pipeline(
        &self,
        assets: &RendererAssetsReadOnly,
        info: &RayTracingPipelineInfo,
    ) -> RayTracingPipelineHandle {
        let asset_manager = assets.asset_manager();
        let handle: RayTracingPipelineHandle = asset_manager
            .reserve_handle_without_path(AssetType::RayTracingPipeline)
            .into();
        {
            let mut remaining = self.rt.remaining_compilations.lock().unwrap();
            remaining.insert(
                handle,
                RayTracingCompileTask {
                    closest_hit_shaders: info
                        .closest_hit_shaders
                        .iter()
                        .map(|shader| HandlePipelineShaderStage {
                            shader_handle: asset_manager
                                .request_asset(
                                    shader.shader_path,
                                    AssetType::Shader,
                                    AssetLoadPriority::Normal,
                                )
                                .0
                                .into(),
                            spec_consts: shader
                                .spec_consts
                                .map(|s| (*s).clone())
                                .unwrap_or_default(),
                        })
                        .collect(),
                    any_hit_shaders: info
                        .any_hit_shaders
                        .iter()
                        .map(|shader| HandlePipelineShaderStage {
                            shader_handle: asset_manager
                                .request_asset(
                                    shader.shader_path,
                                    AssetType::Shader,
                                    AssetLoadPriority::Normal,
                                )
                                .0
                                .into(),
                            spec_consts: shader
                                .spec_consts
                                .map(|s| (*s).clone())
                                .unwrap_or_default(),
                        })
                        .collect(),
                    miss_shaders: info
                        .miss_shaders
                        .iter()
                        .map(|shader| HandlePipelineShaderStage {
                            shader_handle: asset_manager
                                .request_asset(
                                    shader.shader_path,
                                    AssetType::Shader,
                                    AssetLoadPriority::Normal,
                                )
                                .0
                                .into(),
                            spec_consts: shader
                                .spec_consts
                                .map(|s| (*s).clone())
                                .unwrap_or_default(),
                        })
                        .collect(),
                    ray_gen_shader: HandlePipelineShaderStage {
                        shader_handle: asset_manager
                            .request_asset(
                                info.ray_gen_shader.shader_path,
                                AssetType::Shader,
                                AssetLoadPriority::Normal,
                            )
                            .0
                            .into(),
                        spec_consts: info
                            .ray_gen_shader
                            .spec_consts
                            .map(|s| (*s).clone())
                            .unwrap_or_default(),
                    },
                    is_async: false,
                    handle,
                },
            );
        }
        self.update_remaining_compilations_type(assets, &self.rt);
        handle
    }

    fn queue_pipelines_containing_shader_type<T>(
        &self,
        assets_read: &RendererAssetsReadOnly,
        pipeline_type_manager: &Arc<PipelineTypeManager<T>>,
        handle: ShaderHandle,
        shader: &RendererShader,
    ) where
        T: PipelineCompileTask + 'static,
    {
        let mut remaining_compilations: crate::MutexGuard<'_, HashMap<T::TPipelineHandle, T>> =
            pipeline_type_manager.remaining_compilations.lock().unwrap();
        let finished_pipelines = T::finished_pipelines(assets_read);
        for (pipeline_handle, pipeline) in finished_pipelines {
            let existing_pipeline_match = pipeline.task.contains_shader(handle);
            if let Some(shader_type) = existing_pipeline_match {
                log::info!("Found pipeline that contains shader: {:?}", handle);
                assert!(shader_type == shader.shader_type());
                if !remaining_compilations.contains_key(&pipeline_handle) {
                    let task: T = pipeline.task.clone();
                    remaining_compilations.insert(*pipeline_handle, task);
                }
            }
        }
    }

    fn collect_ready_pipeline_handles<T>(
        &self,
        assets_read: &RendererAssetsReadOnly,
        pipeline_type_manager: &Arc<PipelineTypeManager<T>>,
    ) -> SmallVec<[T; 1]>
    where
        T: PipelineCompileTask + 'static,
    {
        // Go over all pipelines that can be compiled now.
        let mut ready_handles = SmallVec::<[T::TPipelineHandle; 1]>::new();
        let mut remaining_compilations: crate::MutexGuard<'_, HashMap<T::TPipelineHandle, T>> =
            pipeline_type_manager.remaining_compilations.lock().unwrap();
        for (pipeline_handle, task) in remaining_compilations.iter() {
            if task.can_compile(&assets_read) {
                ready_handles.push(*pipeline_handle);
            }
        }

        let ready_tasks: SmallVec<[T; 1]> = ready_handles
            .iter()
            .flat_map(|handle| remaining_compilations.remove(handle))
            .collect();

        ready_tasks
    }

    fn update_remaining_compilations_type<T>(
        &self,
        assets: &RendererAssetsReadOnly,
        pipeline_type_manager: &Arc<PipelineTypeManager<T>>,
    ) -> u32
    where
        T: PipelineCompileTask + Send + 'static,
    {
        let ready_tasks = self.collect_ready_pipeline_handles(assets, pipeline_type_manager);
        if ready_tasks.is_empty() {
            return 0;
        }
        let count = ready_tasks.len() as u32;

        #[cfg(not(target_arch = "wasm32"))]
        self.spawn_compile_task(ready_tasks, assets, pipeline_type_manager);
        #[cfg(target_arch = "wasm32")]
        self.spawn_local_compile_task(ready_tasks, assets, pipeline_type_manager);

        count
    }

    #[allow(unused)]
    fn spawn_compile_task<T>(
        &self,
        mut ready_tasks: SmallVec<[T; 1]>,
        assets: &RendererAssetsReadOnly,
        pipeline_type_manager: &Arc<PipelineTypeManager<T>>,
    ) where
        T: PipelineCompileTask + Send + 'static,
    {
        let task_pool = bevy_tasks::ComputeTaskPool::get();
        for task in ready_tasks.drain(..) {
            let c_device = self.device.clone();
            let c_manager: Arc<PipelineTypeManager<T>> = pipeline_type_manager.clone();
            let c_delayed_pipeline = pipeline_type_manager.compiled_unpulled_pipelines.clone();
            let shaders = task.collect_shaders_for_compilation(assets);
            let handle = task.handle();

            let async_task = crate::tasks::spawn_async_compute(async move {
                crate::autoreleasepool(|| {
                    let pipeline = task.compile(shaders, &c_device);
                    let mut delayed_pipelines = c_delayed_pipeline.lock().unwrap();
                    log::info!("Finished compiling pipeline with handle: {:?}", handle);
                    delayed_pipelines.push((handle, task, pipeline));
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
        pipeline_type_manager: &Arc<PipelineTypeManager<T>>,
    ) where
        T: PipelineCompileTask + Send + 'static,
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
                    log::info!("Finished compiling pipeline with handle: {:?}", handle);
                    delayed_pipelines.push((handle, task, pipeline));
                    c_manager.cond_var.notify_all();
                })
            });
            async_task.detach();
        }
    }

    pub fn queue_pipelines_containing_shader(
        &self,
        assets: &RendererAssetsReadOnly,
        handle: ShaderHandle,
        shader: &RendererShader,
    ) {
        let shader_type = shader.shader_type();
        if shader_type == ShaderType::ComputeShader {
            self.queue_pipelines_containing_shader_type(assets, &self.compute, handle, shader);
            return;
        }

        if shader_type == ShaderType::RayGen
            || shader_type == ShaderType::RayClosestHit
            || shader_type == ShaderType::RayMiss
        {
            self.queue_pipelines_containing_shader_type(assets, &self.rt, handle, shader);
            return;
        }

        if shader_type == ShaderType::FragmentShader {
            self.queue_pipelines_containing_shader_type(assets, &self.graphics, handle, shader);
            self.queue_pipelines_containing_shader_type(
                assets,
                &self.mesh_graphics,
                handle,
                shader,
            );
            return;
        }

        if shader_type == ShaderType::VertexShader
            || shader_type == ShaderType::GeometryShader
            || shader_type == ShaderType::TessellationControlShader
            || shader_type == ShaderType::TessellationEvaluationShader
        {
            self.queue_pipelines_containing_shader_type(assets, &self.graphics, handle, shader);
            return;
        }

        if shader_type == ShaderType::MeshShader || shader_type == ShaderType::TaskShader {
            self.queue_pipelines_containing_shader_type(
                assets,
                &self.mesh_graphics,
                handle,
                shader,
            );
            return;
        }

        panic!("Unhandled shader. {:?}", handle);
    }

    pub fn update_remaining_compilations(&self, assets: &RendererAssetsReadOnly) -> u32 {
        let mut count = 0;
        count += self.update_remaining_compilations_type(assets, &self.graphics);
        count += self.update_remaining_compilations_type(assets, &self.mesh_graphics);
        count += self.update_remaining_compilations_type(assets, &self.compute);
        count += self.update_remaining_compilations_type(assets, &self.rt);
        count
    }

    pub fn pull_finished_pipelines(&self) -> SmallVec<[RendererAssetWithHandle; 2]> {
        let mut assets: SmallVec<[RendererAssetWithHandle; 2]> = SmallVec::new();
        {
            let mut guard = self.graphics.compiled_unpulled_pipelines.lock().unwrap();
            for (handle, task, pipeline) in guard.drain(..) {
                assets.push(RendererAssetWithHandle::GraphicsPipeline(
                    handle,
                    CompiledPipeline { task, pipeline },
                ));
            }
        }
        {
            let mut guard = self
                .mesh_graphics
                .compiled_unpulled_pipelines
                .lock()
                .unwrap();
            for (handle, task, pipeline) in guard.drain(..) {
                assets.push(RendererAssetWithHandle::MeshGraphicsPipeline(
                    handle,
                    CompiledPipeline { task, pipeline },
                ));
            }
        }
        {
            let mut guard = self.compute.compiled_unpulled_pipelines.lock().unwrap();
            for (handle, task, pipeline) in guard.drain(..) {
                assets.push(RendererAssetWithHandle::ComputePipeline(
                    handle,
                    CompiledPipeline { task, pipeline },
                ));
            }
        }
        {
            let mut guard = self.rt.compiled_unpulled_pipelines.lock().unwrap();
            for (handle, task, pipeline) in guard.drain(..) {
                assets.push(RendererAssetWithHandle::RayTracingPipeline(
                    handle,
                    CompiledPipeline { task, pipeline },
                ));
            }
        }
        assets
    }

    pub fn has_remaining_mandatory_compilations(&self) -> bool {
        let has_graphics_compiles = {
            let graphics_remaining = self.graphics.remaining_compilations.lock().unwrap();
            graphics_remaining.iter().any(|(_, t)| !t.is_async)
        };
        let has_mesh_graphics_compiles = {
            let mesh_graphics_remaining = self.mesh_graphics.remaining_compilations.lock().unwrap();
            mesh_graphics_remaining.iter().any(|(_, t)| !t.is_async)
        };
        let has_compute_compiles = {
            let compute_remaining = self.compute.remaining_compilations.lock().unwrap();
            compute_remaining.iter().any(|(_, t)| !t.is_async)
        };
        let has_rt_compiles = {
            let rt_remaining = self.rt.remaining_compilations.lock().unwrap();
            rt_remaining.iter().any(|(_, t)| !t.is_async)
        };
        has_graphics_compiles
            || has_mesh_graphics_compiles
            || has_compute_compiles
            || has_rt_compiles
    }
}
