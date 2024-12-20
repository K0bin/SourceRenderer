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

use smallvec::SmallVec;
use sourcerenderer_core::gpu::PackedShader;
use sourcerenderer_core::Platform;

use crate::asset::{
    AssetLoadPriority,
    AssetManager,
    AssetType,
};
use crate::graphics::*;
use crate::graphics::GraphicsPipelineInfo as ActualGraphicsPipelineInfo;
use crate::graphics::RayTracingPipelineInfo as ActualRayTracingPipelineInfo;

//
// COMMON
//

trait PipelineCompileTask<P: Platform>: Send + Sync + Clone {
    type TShaders;
    type TPipeline: Send + Sync;

    fn contains_shader(&self, loaded_shader_path: &str) -> Option<ShaderType>;
    fn request_shaders(&self, asset_manager: &Arc<AssetManager<P>>);
    fn request_remaining_shaders(
        &self,
        loaded_shader_path: &str,
        shaders: &HashMap<String, Arc<<P::GPUBackend as GPUBackend>::Shader>>,
        asset_manager: &Arc<AssetManager<P>>,
    );
    fn can_compile(
        &self,
        loaded_shader_path: Option<&str>,
        shaders: &HashMap<String, Arc<<P::GPUBackend as GPUBackend>::Shader>>,
    ) -> bool;
    fn collect_shaders_for_compilation(
        &self,
        shaders: &HashMap<String, Arc<<P::GPUBackend as GPUBackend>::Shader>>,
    ) -> Self::TShaders;
    fn compile(
        &self,
        shaders: Self::TShaders,
        device: &Arc<Device<P::GPUBackend>>,
    ) -> Arc<Self::TPipeline>;
    fn is_async(&self) -> bool;
    fn set_async(&mut self);
}

struct CompiledPipeline<P: Platform, T: PipelineCompileTask<P>> {
    task: T,
    pipeline: Arc<T::TPipeline>,
}

trait IndexHandle {
    fn new(index: u64) -> Self;
}

//
// GRAPHICS
//

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GraphicsPipelineHandle {
    index: u64,
}

impl IndexHandle for GraphicsPipelineHandle {
    fn new(index: u64) -> Self {
        Self { index }
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
struct GraphicsCompileTask<P: Platform> {
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

struct GraphicsShaders<B: GPUBackend> {
    vs: Arc<B::Shader>,
    fs: Option<Arc<B::Shader>>,
}

impl<P: Platform> PipelineCompileTask<P> for GraphicsCompileTask<P> {
    type TShaders = GraphicsShaders<P::GPUBackend>;
    type TPipeline = crate::graphics::GraphicsPipeline<P::GPUBackend>;

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
        loaded_shader_path: Option<&str>,
        shaders: &HashMap<String, Arc<<<P as Platform>::GPUBackend as GPUBackend>::Shader>>,
    ) -> bool {
        (loaded_shader_path.map_or(false, |s| s == &self.info.vs) || shaders.contains_key(&self.info.vs))
            && self
                .info
                .fs
                .as_ref()
                .map(|fs| loaded_shader_path.map_or(false, |s| s == fs) || shaders.contains_key(fs))
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
        loaded_shader_path: &str,
        shaders: &HashMap<String, Arc<<<P as Platform>::GPUBackend as GPUBackend>::Shader>>,
        asset_manager: &Arc<AssetManager<P>>,
    ) {
        if &self.info.vs != loaded_shader_path && !shaders.contains_key(&self.info.vs) {
            asset_manager.request_asset(&self.info.vs, AssetType::Shader, AssetLoadPriority::High);
        }
        if let Some(fs) = self.info.fs.as_ref() {
            if fs != loaded_shader_path && !shaders.contains_key(fs) {
                asset_manager.request_asset(fs, AssetType::Shader, AssetLoadPriority::High);
            }
        }
    }

    fn collect_shaders_for_compilation(
        &self,
        shaders: &HashMap<String, Arc<<P::GPUBackend as GPUBackend>::Shader>>,
    ) -> Self::TShaders {
        GraphicsShaders {
            vs: shaders.get(&self.info.vs).cloned().unwrap(),
            fs: self
                .info
                .fs
                .as_ref()
                .map(|fs| shaders.get(fs).cloned().unwrap()),
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

struct ComputeCompileTask<P: Platform> {
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
pub struct ComputePipelineHandle {
    index: u64,
}

impl IndexHandle for ComputePipelineHandle {
    fn new(index: u64) -> Self {
        Self { index }
    }
}

impl<P: Platform> PipelineCompileTask<P> for ComputeCompileTask<P> {
    type TShaders = Arc<<P::GPUBackend as GPUBackend>::Shader>;
    type TPipeline = crate::graphics::ComputePipeline<P::GPUBackend>;

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
        _loaded_shader_path: &str,
        _shaders: &HashMap<String, Arc<<<P as Platform>::GPUBackend as GPUBackend>::Shader>>,
        _asset_manager: &Arc<AssetManager<P>>,
    ) {
    }

    fn can_compile(
        &self,
        loaded_shader_path: Option<&str>,
        shaders: &HashMap<String, Arc<<<P as Platform>::GPUBackend as GPUBackend>::Shader>>,
    ) -> bool {
        loaded_shader_path.map_or(false, |s| s == &self.path) || shaders.contains_key(&self.path)
    }

    fn collect_shaders_for_compilation(
        &self,
        shaders: &HashMap<String, Arc<<<P as Platform>::GPUBackend as GPUBackend>::Shader>>,
    ) -> Self::TShaders {
        shaders.get(&self.path).cloned().unwrap()
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
struct StoredRayTracingPipelineInfo<P: Platform> {
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
pub struct RayTracingPipelineHandle {
    index: u64,
}

impl IndexHandle for RayTracingPipelineHandle {
    fn new(index: u64) -> Self {
        Self { index }
    }
}

struct RayTracingShaders<B: GPUBackend> {
    pub ray_gen_shader: Arc<B::Shader>,
    pub closest_hit_shaders: SmallVec<[Arc<B::Shader>; 4]>,
    pub miss_shaders: SmallVec<[Arc<B::Shader>; 4]>,
}

impl<P: Platform> PipelineCompileTask<P> for StoredRayTracingPipelineInfo<P> {
    type TShaders = RayTracingShaders<P::GPUBackend>;
    type TPipeline = crate::graphics::RayTracingPipeline<P::GPUBackend>;

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
        loaded_shader_path: &str,
        shaders: &HashMap<String, Arc<<<P as Platform>::GPUBackend as GPUBackend>::Shader>>,
        asset_manager: &Arc<AssetManager<P>>,
    ) {
        if loaded_shader_path != &self.ray_gen_shader && !shaders.contains_key(&self.ray_gen_shader)
        {
            asset_manager.request_asset(
                &self.ray_gen_shader,
                AssetType::Shader,
                AssetLoadPriority::High,
            );
        }
        for shader in &self.closest_hit_shaders {
            if loaded_shader_path != shader && !shaders.contains_key(shader) {
                asset_manager.request_asset(shader, AssetType::Shader, AssetLoadPriority::High);
            }
        }
        for shader in &self.miss_shaders {
            if loaded_shader_path != shader && !shaders.contains_key(shader) {
                asset_manager.request_asset(shader, AssetType::Shader, AssetLoadPriority::High);
            }
        }
    }

    fn can_compile(
        &self,
        loaded_shader_path: Option<&str>,
        shaders: &HashMap<String, Arc<<<P as Platform>::GPUBackend as GPUBackend>::Shader>>,
    ) -> bool {
        if !loaded_shader_path.map_or(false, |s| s == &self.ray_gen_shader) && !shaders.contains_key(&self.ray_gen_shader)
        {
            return false;
        }
        for shader in &self.closest_hit_shaders {
            if !loaded_shader_path.map_or(false, |s| s == shader) && !shaders.contains_key(shader) {
                return false;
            }
        }
        for shader in &self.miss_shaders {
            if !loaded_shader_path.map_or(false, |s| s == shader) && !shaders.contains_key(shader) {
                return false;
            }
        }
        true
    }

    fn collect_shaders_for_compilation(
        &self,
        shaders: &HashMap<String, Arc<<<P as Platform>::GPUBackend as GPUBackend>::Shader>>,
    ) -> Self::TShaders {
        Self::TShaders {
            ray_gen_shader: shaders.get(&self.ray_gen_shader).cloned().unwrap(),
            closest_hit_shaders: self
                .closest_hit_shaders
                .iter()
                .map(|shader| shaders.get(shader).cloned().unwrap())
                .collect(),
            miss_shaders: self
                .miss_shaders
                .iter()
                .map(|shader| shaders.get(shader).cloned().unwrap())
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
    asset_manager: Arc<AssetManager<P>>,
    graphics: Arc<PipelineTypeManager<P, GraphicsPipelineHandle, GraphicsCompileTask<P>>>,
    compute: Arc<PipelineTypeManager<P, ComputePipelineHandle, ComputeCompileTask<P>>>,
    rt: Arc<
        PipelineTypeManager<P, RayTracingPipelineHandle, StoredRayTracingPipelineInfo<P>>,
    >,
    next_pipeline_handle_index: u64
}

struct PipelineTypeManager<P, THandle, T>
where
    P: Platform,
    THandle: IndexHandle + Hash + PartialEq + Eq + Clone + Copy + Send + Sync,
    T: PipelineCompileTask<P>,
{
    inner: Mutex<PipelineTypeManagerInner<P, THandle, T>>,
    cond_var: Condvar
}

struct PipelineTypeManagerInner<P, THandle, T>
where
    P: Platform,
    THandle: IndexHandle + Hash + PartialEq + Eq + Clone + Copy + Send + Sync,
    T: PipelineCompileTask<P>,
{
    next_handle_index: u64,
    shaders: HashMap<String, Arc<<P::GPUBackend as GPUBackend>::Shader>>,
    compiled_pipelines: HashMap<THandle, CompiledPipeline<P, T>>,
    remaining_compilations: HashMap<THandle, T>,
}

impl<P, THandle, T> PipelineTypeManager<P, THandle, T>
where
    P: Platform,
    THandle: IndexHandle + Hash + PartialEq + Eq + Clone + Copy + Send + Sync,
    T: PipelineCompileTask<P>,
{
    fn new() -> Self {
        Self {
            inner: Mutex::new(PipelineTypeManagerInner {
                next_handle_index: 1u64,
                shaders: HashMap::new(),
                compiled_pipelines: HashMap::new(),
                remaining_compilations: HashMap::new(),
            }),
            cond_var: Condvar::new()
        }
    }
}

impl<P: Platform> ShaderManager<P> {
    pub fn new(
        device: &Arc<Device<P::GPUBackend>>,
        asset_manager: &Arc<AssetManager<P>>,
    ) -> Self {
        Self {
            device: device.clone(),
            asset_manager: asset_manager.clone(),
            graphics: Arc::new(PipelineTypeManager::new()),
            compute: Arc::new(PipelineTypeManager::new()),
            rt: Arc::new(PipelineTypeManager::new()),
            next_pipeline_handle_index: 1u64
        }
    }

    pub fn request_graphics_pipeline(
        &mut self,
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
            &self.graphics,
            GraphicsCompileTask::<P> {
                info: stored,
                is_async: false,
                _p: PhantomData,
            },
        )
    }

    pub fn request_compute_pipeline(&mut self, path: &str) -> ComputePipelineHandle {
        self.request_pipeline_internal(
            &self.compute,
            ComputeCompileTask::<P> {
                path: path.to_string(),
                is_async: false,
                _p: PhantomData,
            },
        )
    }

    pub fn request_ray_tracing_pipeline(
        &mut self,
        info: &RayTracingPipelineInfo,
    ) -> RayTracingPipelineHandle {
        self.request_pipeline_internal(
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
        pipeline_type_manager: &Arc<PipelineTypeManager<P, THandle, T>>,
        task: T,
    ) -> THandle
    where
        THandle: IndexHandle + Hash + PartialEq + Eq + Clone + Copy + Send + Sync,
        T: PipelineCompileTask<P>,
    {
        let mut inner = pipeline_type_manager.inner.lock().unwrap();
        let handle = THandle::new(inner.next_handle_index);
        inner.next_handle_index += 1;
        task.request_shaders(&self.asset_manager);
        inner.remaining_compilations.insert(handle, task);
        handle
    }

    fn add_shader_type<THandle, T>(
        &self,
        pipeline_type_manager: &Arc<PipelineTypeManager<P, THandle, T>>,
        path: &str,
        shader: PackedShader
    ) -> bool
    where
        THandle: IndexHandle + Hash + PartialEq + Eq + Clone + Copy + Send + Sync + 'static,
        T: PipelineCompileTask<P> + 'static,
    {
        {
            println!("Integrating shader {:?}", path);
            let mut ready_handles = SmallVec::<[THandle; 1]>::new();
            let mut found = false;
            {
                let mut inner = pipeline_type_manager.inner.lock().unwrap();

                // Find all pipelines that use this shader and queue new compile tasks for those.
                // This is done because add_shader will get called when a shader has changed on disk, so we need to load
                // all remaining shaders of a pipeline and recompile it.

                let mut tasks_to_add: SmallVec<[(THandle, T); 1]> = SmallVec::new();

                for (handle, pipeline) in &inner.compiled_pipelines {
                    let existing_pipeline_match = pipeline.task.contains_shader(path);
                    if let Some(shader_type) = existing_pipeline_match {
                        assert!(shader_type  == shader.shader_type);
                        found = true;
                        pipeline.task.request_remaining_shaders(
                            path,
                            &inner.shaders,
                            &self.asset_manager,
                        );
                        if !inner.remaining_compilations.contains_key(handle) {
                            let mut task: T = pipeline.task.clone();
                            task.set_async();
                            tasks_to_add.push((handle.clone(), task));
                        }
                    }
                }

                for (handle, task) in &inner.remaining_compilations {
                    let remaining_compile_match = task.contains_shader(path);
                    if let Some(shader_type) = remaining_compile_match {
                        assert!(shader_type  == shader.shader_type);
                        found = true;
                        if task.can_compile(Some(path), &inner.shaders) {
                            ready_handles.push(*handle);
                        }
                    }
                }

                if found {
                    let shader =
                        self.device
                            .create_shader(shader, Some(path));
                    inner.shaders.insert(path.to_string(), Arc::new(shader));

                    for (handle, task) in tasks_to_add.drain(..) {
                        inner.remaining_compilations.insert(handle, task);
                    }
                } else {
                    return false;
                }
            }

            if ready_handles.is_empty() {
                return true;
            }

            let c_device = self.device.clone();
            let c_manager = pipeline_type_manager.clone();
            c_manager.cond_var.notify_all();
            let task_pool = bevy_tasks::ComputeTaskPool::get();
            task_pool.spawn(async move {
                for handle in ready_handles.drain(..) {
                    let task: T;
                    let shaders: T::TShaders;

                    {
                        let mut inner = c_manager.inner.lock().unwrap();
                        task = inner.remaining_compilations.remove(&handle).unwrap();
                        shaders = task.collect_shaders_for_compilation(&inner.shaders);
                    };
                    let pipeline = task.compile(shaders, &c_device);
                    {
                        let mut inner = c_manager.inner.lock().unwrap();
                        if let Some(existing_pipeline) = inner.compiled_pipelines.get_mut(&handle) {
                            existing_pipeline.pipeline = pipeline;
                        } else {
                            inner
                                .compiled_pipelines
                                .insert(handle, CompiledPipeline::<P, T> { pipeline, task });
                        }
                    }
                }
                c_manager.cond_var.notify_all();
            });
            true
        }
    }

    pub fn add_shader(&mut self, path: &str, shader: PackedShader) {
        if !match shader.shader_type {
            ShaderType::ComputeShader => self.add_shader_type(&self.compute, path, shader),
            ShaderType::RayGen | ShaderType::RayClosestHit | ShaderType::RayMiss => self.add_shader_type(&self.rt, path, shader),
            ShaderType::FragmentShader | ShaderType::VertexShader | ShaderType::GeometryShader | ShaderType::TessellationControlShader | ShaderType::TessellationEvaluationShader =>
                self.add_shader_type(&self.graphics, path, shader),
        } {
            panic!("Unhandled shader. {}", path);
        }
    }

    pub fn has_remaining_mandatory_compilations(&self) -> bool {
        let has_graphics_compiles = {
            let graphics = self.graphics.inner.lock().unwrap();
            graphics
                .remaining_compilations
                .iter()
                .any(|(_, t)| !t.is_async)
        };
        let has_compute_compiles = {
            let compute = self.compute.inner.lock().unwrap();
            compute
                .remaining_compilations
                .iter()
                .any(|(_, t)| !t.is_async)
        };
        let has_rt_compiles = {
            let rt = self.rt.inner.lock().unwrap();
            rt.remaining_compilations.iter().any(|(_, t)| !t.is_async)
        };
        has_graphics_compiles || has_compute_compiles || has_rt_compiles
    }

    fn try_get_pipeline_internal<T, THandle>(
        &self,
        pipeline_type_manager: &Arc<PipelineTypeManager<P, THandle, T>>,
        handle: THandle,
    ) -> Option<Arc<T::TPipeline>>
    where
        THandle: IndexHandle + Hash + PartialEq + Eq + Clone + Copy + Send + Sync,
        T: PipelineCompileTask<P>,
    {
        let inner = pipeline_type_manager.inner.lock().unwrap();
        inner
            .compiled_pipelines
            .get(&handle)
            .map(|p| p.pipeline.clone())
    }

    fn get_pipeline_internal<T, THandle>(
        &self,
        pipeline_type_manager: &Arc<PipelineTypeManager<P, THandle, T>>,
        handle: THandle,
    ) -> Arc<T::TPipeline>
    where
        THandle: IndexHandle + Hash + PartialEq + Eq + Clone + Copy + Send + Sync,
        T: PipelineCompileTask<P>,
    {
        let inner: std::sync::MutexGuard<'_, PipelineTypeManagerInner<P, THandle, T>> = pipeline_type_manager.inner.lock().unwrap();
        let pipeline_opt = inner.compiled_pipelines.get(&handle);
        if let Some(pipeline) = pipeline_opt {
            return pipeline.pipeline.clone();
        }
        let inner = pipeline_type_manager
            .cond_var
            .wait_while(inner, |inner| {
                !inner.compiled_pipelines.contains_key(&handle)
            })
            .unwrap();
        inner
            .compiled_pipelines
            .get(&handle)
            .unwrap()
            .pipeline
            .clone()
    }

    pub fn try_get_graphics_pipeline(
        &self,
        handle: GraphicsPipelineHandle,
    ) -> Option<Arc<crate::graphics::GraphicsPipeline<P::GPUBackend>>> {
        self.try_get_pipeline_internal(&self.graphics, handle)
    }

    pub fn get_graphics_pipeline(
        &self,
        handle: GraphicsPipelineHandle,
    ) -> Arc<crate::graphics::GraphicsPipeline<P::GPUBackend>> {
        self.get_pipeline_internal(&self.graphics, handle)
    }

    pub fn try_get_compute_pipeline(
        &self,
        handle: ComputePipelineHandle,
    ) -> Option<Arc<crate::graphics::ComputePipeline<P::GPUBackend>>> {
        self.try_get_pipeline_internal(&self.compute, handle)
    }

    pub fn get_compute_pipeline(
        &self,
        handle: ComputePipelineHandle,
    ) -> Arc<crate::graphics::ComputePipeline<P::GPUBackend>> {
        self.get_pipeline_internal(&self.compute, handle)
    }

    pub fn try_get_ray_tracing_pipeline(
        &self,
        handle: RayTracingPipelineHandle,
    ) -> Option<Arc<crate::graphics::RayTracingPipeline<P::GPUBackend>>> {
        self.try_get_pipeline_internal(&self.rt, handle)
    }

    pub fn get_ray_tracing_pipeline(
        &self,
        handle: RayTracingPipelineHandle,
    ) -> Arc<crate::graphics::RayTracingPipeline<P::GPUBackend>> {
        self.get_pipeline_internal(&self.rt, handle)
    }
}
