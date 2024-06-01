use std::collections::HashMap;
use std::sync::Arc;

use smallvec::SmallVec;
use sourcerenderer_core::gpu::GPUBackend;

use sourcerenderer_core::{
    Platform,
    Vec4,
};

use super::asset_buffer::{
    AssetBuffer,
    AssetBufferSlice,
};
use super::shader_manager::ShaderManager;
use crate::asset::{
    Asset,
    AssetLoadPriority,
    AssetManager,
    Material,
    MaterialValue,
    Mesh,
    MeshRange,
    Model,
    Texture,
};
use crate::graphics::*;
use crate::math::BoundingBox;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MeshHandle {
    index: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MaterialHandle {
    index: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureHandle {
    index: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelHandle {
    index: u64,
}

impl IndexHandle for MeshHandle {
    fn new(index: u64) -> Self {
        Self { index }
    }
}
impl IndexHandle for MaterialHandle {
    fn new(index: u64) -> Self {
        Self { index }
    }
}
impl IndexHandle for TextureHandle {
    fn new(index: u64) -> Self {
        Self { index }
    }
}
impl IndexHandle for ModelHandle {
    fn new(index: u64) -> Self {
        Self { index }
    }
}

pub struct RendererTexture<B: GPUBackend> {
    pub(super) view: Arc<TextureView<B>>,
    pub(super) bindless_index: Option<BindlessSlot<B>>,
}

impl<B: GPUBackend> PartialEq for RendererTexture<B> {
    fn eq(&self, other: &Self) -> bool {
        self.view == other.view
    }
}
impl<B: GPUBackend> Eq for RendererTexture<B> {}

pub struct RendererMaterial {
    pub(super) properties: HashMap<String, RendererMaterialValue>,
    pub(super) shader_name: String, // TODO reference actual shader
}

impl Clone for RendererMaterial {
    fn clone(&self) -> Self {
        Self {
            properties: self.properties.clone(),
            shader_name: self.shader_name.clone(),
        }
    }
}

pub enum RendererMaterialValue {
    Float(f32),
    Vec4(Vec4),
    Texture(TextureHandle),
}

impl PartialEq for RendererMaterialValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Float(l0), Self::Float(r0)) => (l0 * 100f32) as u32 == (r0 * 100f32) as u32,
            (Self::Vec4(l0), Self::Vec4(r0)) => {
                (l0.x * 100f32) as u32 == (r0.x * 100f32) as u32
                    && (l0.y * 100f32) as u32 == (r0.y * 100f32) as u32
                    && (l0.z * 100f32) as u32 == (r0.z * 100f32) as u32
                    && (l0.w * 100f32) as u32 == (r0.w * 100f32) as u32
            }
            (Self::Texture(l0), Self::Texture(r0)) => l0 == r0,
            _ => false,
        }
    }
}

impl Eq for RendererMaterialValue {}

impl Clone for RendererMaterialValue {
    fn clone(&self) -> Self {
        match self {
            Self::Float(val) => Self::Float(*val),
            Self::Vec4(val) => Self::Vec4(*val),
            Self::Texture(tex) => Self::Texture(*tex),
        }
    }
}

impl PartialOrd for RendererMaterialValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RendererMaterialValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (RendererMaterialValue::Float(val1), RendererMaterialValue::Float(val2)) => {
                ((val1 * 100f32) as u32).cmp(&((val2 * 100f32) as u32))
            }
            (RendererMaterialValue::Float(_), RendererMaterialValue::Texture(_)) => {
                std::cmp::Ordering::Less
            }
            (RendererMaterialValue::Float(_), RendererMaterialValue::Vec4(_)) => {
                std::cmp::Ordering::Less
            }
            (RendererMaterialValue::Texture(_), RendererMaterialValue::Float(_)) => {
                std::cmp::Ordering::Greater
            }
            (RendererMaterialValue::Texture(_), RendererMaterialValue::Vec4(_)) => {
                std::cmp::Ordering::Greater
            }
            (RendererMaterialValue::Texture(tex1), RendererMaterialValue::Texture(tex2)) => {
                tex1.cmp(&tex2)
            }
            (RendererMaterialValue::Vec4(val1), RendererMaterialValue::Vec4(val2)) => {
                ((val1.x * 100f32) as u32)
                    .cmp(&((val2.x * 100f32) as u32))
                    .then(((val1.y * 100f32) as u32).cmp(&((val2.y * 100f32) as u32)))
                    .then(((val1.z * 100f32) as u32).cmp(&((val2.z * 100f32) as u32)))
                    .then(((val1.w * 100f32) as u32).cmp(&((val2.w * 100f32) as u32)))
            }
            (RendererMaterialValue::Vec4(_), RendererMaterialValue::Texture(_)) => {
                std::cmp::Ordering::Less
            }
            (RendererMaterialValue::Vec4(_), RendererMaterialValue::Float(_)) => {
                std::cmp::Ordering::Greater
            }
        }
    }
}

impl PartialEq for RendererMaterial {
    fn eq(&self, other: &Self) -> bool {
        if self.shader_name != other.shader_name {
            return false;
        }
        for (key, value) in self.properties.iter() {
            if other.properties.get(key) != Some(value) {
                return false;
            }
        }
        true
    }
}

impl RendererMaterial {
    pub fn new_pbr(albedo_texture: TextureHandle) -> Self {
        let mut props = HashMap::new();
        props.insert(
            "albedo".to_string(),
            RendererMaterialValue::Texture(albedo_texture),
        );
        Self {
            shader_name: "pbr".to_string(),
            properties: props,
        }
    }

    pub fn new_pbr_color(color: Vec4) -> Self {
        let mut props = HashMap::new();
        props.insert("albedo".to_string(), RendererMaterialValue::Vec4(color));
        Self {
            shader_name: "pbr".to_string(),
            properties: props,
        }
    }

    pub fn get(&self, key: &str) -> Option<&RendererMaterialValue> {
        self.properties.get(key)
    }
}

impl Eq for RendererMaterial {}

impl PartialOrd for RendererMaterial {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RendererMaterial {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let mut last_result = self
            .shader_name
            .cmp(&other.shader_name)
            .then(self.properties.len().cmp(&other.properties.len()));

        if last_result != std::cmp::Ordering::Equal {
            return last_result;
        }

        for (key, value) in &self.properties {
            let other_val = other.properties.get(key);
            if let Some(other_val) = other_val {
                last_result = value.cmp(other_val);
                if last_result != std::cmp::Ordering::Equal {
                    return last_result;
                }
            }
        }
        std::cmp::Ordering::Equal
    }
}

pub struct RendererModel {
    mesh: MeshHandle,
    materials: SmallVec<[MaterialHandle; 16]>,
}

impl RendererModel {
    pub fn new(mesh: MeshHandle, materials: SmallVec<[MaterialHandle; 16]>) -> Self {
        Self {
            mesh: mesh,
            materials,
        }
    }

    pub fn mesh_handle(&self) -> MeshHandle {
        self.mesh
    }

    pub fn material_handles(&self) -> &[MaterialHandle] {
        &self.materials
    }
}

pub struct RendererMesh<B: GPUBackend> {
    pub vertices: AssetBufferSlice<B>,
    pub indices: Option<AssetBufferSlice<B>>,
    pub parts: Box<[MeshRange]>,
    pub bounding_box: Option<BoundingBox>,
    pub vertex_count: u32,
}

struct DelayedAsset<B: GPUBackend> {
    fence: SharedFenceValuePair<B>,
    path: String,
    asset: DelayedAssetType<B>,
}
enum DelayedAssetType<B: GPUBackend> {
    TextureView(Arc<TextureView<B>>),
}

trait IndexHandle {
    fn new(index: u64) -> Self;
}

struct HandleMap<THandle, TValue>
where
    THandle: std::hash::Hash + PartialEq + Eq + Copy + IndexHandle,
{
    path_to_handle: HashMap<String, THandle>,
    handle_to_val: HashMap<THandle, TValue>,
    next_handle_index: u64,
}

impl<THandle, TValue> HandleMap<THandle, TValue>
where
    THandle: std::hash::Hash + PartialEq + Eq + Copy + IndexHandle,
{
    fn new() -> Self {
        Self {
            path_to_handle: HashMap::new(),
            handle_to_val: HashMap::new(),
            next_handle_index: 1u64,
        }
    }

    fn get_handle(&self, path: &str) -> Option<THandle> {
        self.path_to_handle.get(path).copied()
    }

    fn get_or_create_handle(&mut self, path: &str) -> THandle {
        if let Some(handle) = self.path_to_handle.get(path) {
            return *handle;
        }
        self.create_handle(path)
    }

    fn get_value(&self, handle: THandle) -> Option<&TValue> {
        self.handle_to_val.get(&handle)
    }

    fn contains(&self, handle: THandle) -> bool {
        self.handle_to_val.contains_key(&handle)
    }

    fn create_handle(&mut self, path: &str) -> THandle {
        let handle = THandle::new(self.next_handle_index);
        self.next_handle_index += 1;
        self.path_to_handle.insert(path.to_string(), handle);
        handle
    }

    fn insert(&mut self, path: &str, value: TValue) -> THandle {
        if let Some(existing_handle) = self.path_to_handle.get(path) {
            self.handle_to_val.insert(*existing_handle, value);
            return *existing_handle;
        }
        let handle = self.create_handle(path);
        self.handle_to_val.insert(handle, value);
        handle
    }

    fn remove_handle(&mut self, handle: THandle) {
        let path = self
            .path_to_handle
            .iter()
            .find(|(_path, h)| handle == **h)
            .map(|(path, _handle)| path)
            .unwrap()
            .clone();
        self.path_to_handle.remove(&path);
        // TODO: consider either just keeping the path_to_handle map entry because we never reuse handles anyway
        // or add another HashMap that does THandle->Path
        self.handle_to_val.remove(&handle);
    }

    pub fn len(&self) -> usize {
        self.handle_to_val.len()
    }
}

pub struct RendererAssets<P: Platform> {
    device: Arc<crate::graphics::Device<P::GPUBackend>>,
    models: HandleMap<ModelHandle, RendererModel>,
    meshes: HandleMap<MeshHandle, RendererMesh<P::GPUBackend>>,
    materials: HandleMap<MaterialHandle, RendererMaterial>,
    textures: HandleMap<TextureHandle, RendererTexture<P::GPUBackend>>,
    zero_texture: RendererTexture<P::GPUBackend>,
    zero_texture_black: RendererTexture<P::GPUBackend>,
    placeholder_material: RendererMaterial,
    delayed_assets: Vec<DelayedAsset<P::GPUBackend>>,
    vertex_buffer: AssetBuffer<P::GPUBackend>,
    index_buffer: AssetBuffer<P::GPUBackend>,
}

impl<P: Platform> RendererAssets<P> {
    pub(super) fn new(device: &Arc<crate::graphics::Device<P::GPUBackend>>) -> Self {
        let zero_data = [255u8; 16];
        let zero_texture = device.create_texture(
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RGBA8UNorm,
                width: 2,
                height: 2,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::SAMPLED | TextureUsage::INITIAL_COPY,
                supports_srgb: false,
            },
            Some("AssetManagerZeroTexture"),
        ).unwrap();
        device.init_texture(&zero_data, &zero_texture, 0, 0).unwrap();
        let zero_view = device.create_texture_view(
            &zero_texture,
            &TextureViewInfo::default(),
            Some("AssetManagerZeroTextureView"),
        );
        let zero_index = if device.supports_bindless() {
            device.insert_texture_into_bindless_heap(&zero_view)
        } else {
            None
        };
        let zero_rtexture = RendererTexture {
            view: zero_view,
            bindless_index: zero_index,
        };

        let zero_data_black = [
            0u8, 0u8, 0u8, 255u8, 0u8, 0u8, 0u8, 255u8, 0u8, 0u8, 0u8, 255u8, 0u8, 0u8, 0u8, 255u8,
        ];
        let zero_texture_black = device.create_texture(
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RGBA8UNorm,
                width: 2,
                height: 2,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::SAMPLED | TextureUsage::COPY_DST,
                supports_srgb: false,
            },
            Some("AssetManagerZeroTextureBlack"),
        ).unwrap();
        device.init_texture(&zero_data_black, &zero_texture_black, 0, 0).unwrap();
        let zero_view_black = device.create_texture_view(
            &zero_texture_black,
            &TextureViewInfo::default(),
            Some("AssetManagerZeroTextureBlackView"),
        );
        let zero_black_index = if device.supports_bindless() {
            device.insert_texture_into_bindless_heap(&zero_view_black)
        } else {
            None
        };
        let zero_rtexture_black = RendererTexture {
            view: zero_view_black,
            bindless_index: zero_black_index,
        };
        let placeholder_material =
            RendererMaterial::new_pbr_color(Vec4::new(1f32, 1f32, 1f32, 1f32));

        let vertex_buffer = AssetBuffer::<P::GPUBackend>::new(
            device,
            AssetBuffer::<P::GPUBackend>::SIZE_BIG,
            BufferUsage::VERTEX | BufferUsage::COPY_DST | BufferUsage::STORAGE,
        );
        let index_buffer = AssetBuffer::<P::GPUBackend>::new(
            device,
            AssetBuffer::<P::GPUBackend>::SIZE_SMALL,
            BufferUsage::INDEX | BufferUsage::COPY_DST | BufferUsage::STORAGE,
        );

        device.flush_transfers();

        Self {
            device: device.clone(),
            models: HandleMap::new(),
            meshes: HandleMap::new(),
            materials: HandleMap::new(),
            textures: HandleMap::new(),
            zero_texture: zero_rtexture,
            zero_texture_black: zero_rtexture_black,
            placeholder_material,
            delayed_assets: Vec::new(),
            vertex_buffer,
            index_buffer,
        }
    }

    pub fn integrate_texture(
        &mut self,
        texture_path: &str,
        texture: &Arc<TextureView<P::GPUBackend>>,
    ) -> TextureHandle {
        let bindless_index = if self.device.supports_bindless() {
            if texture == &self.zero_texture.view {
                None
            } else if texture == &self.zero_texture_black.view {
                None
            } else {
                self.device.insert_texture_into_bindless_heap(&texture)
            }
        } else {
            None
        };
        let renderer_texture: RendererTexture<<P as Platform>::GPUBackend> = RendererTexture {
            view: texture.clone(),
            bindless_index,
        };
        self.textures.insert(&texture_path, renderer_texture)
    }

    pub fn integrate_mesh(&mut self, mesh_path: &str, mesh: Mesh) -> MeshHandle {
        assert_ne!(mesh.vertex_count, 0);

        let vertex_buffer = self.vertex_buffer.get_slice(
            std::mem::size_of_val(&mesh.vertices[..]),
            std::mem::size_of::<crate::renderer::Vertex>(),
        ); // FIXME: hardcoded vertex size
        self.device.init_buffer(
            &mesh.vertices[..],
            vertex_buffer.buffer(),
            vertex_buffer.offset() as u64
        ).unwrap();

        let index_buffer = mesh.indices.map(|indices| {
            let buffer = self.index_buffer.get_slice(
                std::mem::size_of_val(&indices[..]),
                std::mem::size_of::<u32>(),
            );
            self.device.init_buffer(
                &indices,
                buffer.buffer(),
                buffer.offset() as u64,
            ).unwrap();
            buffer
        });

        let mesh = RendererMesh {
            vertices: vertex_buffer,
            indices: index_buffer,
            parts: mesh.parts.iter().cloned().collect(), // TODO: change base type to boxed slice
            bounding_box: mesh.bounding_box,
            vertex_count: mesh.vertex_count,
        };
        self.meshes.insert(mesh_path, mesh)
    }

    pub fn upload_texture(
        &mut self,
        texture_path: &str,
        texture: Texture,
        do_async: bool,
    ) -> (
        Arc<TextureView<P::GPUBackend>>,
        Option<SharedFenceValuePair<P::GPUBackend>>
    ) {
        let gpu_texture = self
            .device
            .create_texture(&texture.info, Some(texture_path)).unwrap();
        let subresources = texture.info.array_length * texture.info.mip_levels;
        let mut fence = Option::<SharedFenceValuePair<P::GPUBackend>>::None;
        for subresource in 0..subresources {
            let mip_level = subresource % texture.info.mip_levels;
            let array_index = subresource / texture.info.mip_levels;
            if do_async {
                fence = self.device.init_texture_async(
                    &texture.data[subresource as usize][..],
                    &gpu_texture,
                    mip_level,
                    array_index
                ).unwrap();
            } else {
                self.device
                    .init_texture(&texture.data[subresource as usize][..], &gpu_texture, mip_level, array_index).unwrap();
            }
        }
        let view = self.device.create_texture_view(
            &gpu_texture,
            &TextureViewInfo {
                base_mip_level: 0,
                mip_level_length: texture.info.mip_levels,
                base_array_layer: 0,
                array_layer_length: 1,
                format: None,
            },
            Some(texture_path),
        );

        (view, fence)
    }

    pub fn integrate_material(
        &mut self,
        material_path: &str,
        material: &Material,
    ) -> MaterialHandle {
        let mut properties =
            HashMap::<String, RendererMaterialValue>::with_capacity(material.properties.len());
        for (key, value) in &material.properties {
            match value {
                MaterialValue::Texture(path) => {
                    let texture = self.textures.get_or_create_handle(path);
                    properties.insert(key.to_string(), RendererMaterialValue::Texture(texture));
                }

                MaterialValue::Float(val) => {
                    properties.insert(key.to_string(), RendererMaterialValue::Float(*val));
                }

                MaterialValue::Vec4(val) => {
                    properties.insert(key.to_string(), RendererMaterialValue::Vec4(*val));
                }
            }
        }

        let renderer_material = RendererMaterial {
            shader_name: material.shader_name.clone(),
            properties,
        };

        self.materials.insert(material_path, renderer_material)
    }

    pub fn integrate_model(&mut self, model_path: &str, model: &Model) -> ModelHandle {
        let mesh = self.meshes.get_or_create_handle(&model.mesh_path);

        let mut renderer_materials =
            SmallVec::<[MaterialHandle; 16]>::with_capacity(model.material_paths.len());
        for material_path in &model.material_paths {
            let material_handle = self.materials.get_or_create_handle(material_path);
            renderer_materials.push(material_handle.clone());
        }

        let renderer_model = RendererModel::new(mesh, renderer_materials);
        self.models.insert(model_path, renderer_model)
    }

    pub fn get_or_create_model_handle(&mut self, path: &str) -> ModelHandle {
        self.models.get_or_create_handle(path)
    }

    pub fn get_model(&self, handle: ModelHandle) -> Option<&RendererModel> {
        self.models.get_value(handle)
    }

    pub fn has_model(&self, handle: ModelHandle) -> bool {
        self.models.contains(handle)
    }

    pub fn get_mesh(&self, handle: MeshHandle) -> Option<&RendererMesh<P::GPUBackend>> {
        self.meshes.get_value(handle)
    }

    pub fn get_or_create_texture_handle(&mut self, path: &str) -> TextureHandle {
        self.textures.get_or_create_handle(path)
    }

    pub fn get_material(&self, handle: MaterialHandle) -> &RendererMaterial {
        self.materials
            .get_value(handle)
            .unwrap_or(&self.placeholder_material)
    }

    pub fn get_texture(&self, handle: TextureHandle) -> &RendererTexture<P::GPUBackend> {
        self.textures
            .get_value(handle)
            .unwrap_or_else(|| &self.zero_texture)
    }

    pub fn placeholder_texture(&self) -> &RendererTexture<P::GPUBackend> {
        &self.zero_texture
    }

    pub fn placeholder_black(&self) -> &RendererTexture<P::GPUBackend> {
        &self.zero_texture_black
    }

    pub fn is_empty(&self) -> bool {
        self.models.len() == 0
            && self.meshes.len() == 0
            && self.materials.len() == 0
            && self.textures.len() == 0
    }

    pub(super) fn receive_assets(
        &mut self,
        asset_manager: &AssetManager<P>,
        shader_manager: &mut ShaderManager<P>,
    ) {
        let mut retained_delayed_assets = Vec::<DelayedAsset<P::GPUBackend>>::new();
        let mut ready_delayed_assets = Vec::<DelayedAsset<P::GPUBackend>>::new();
        for delayed_asset in self.delayed_assets.drain(..) {
            if delayed_asset.fence.is_signalled() {
                ready_delayed_assets.push(delayed_asset);
            } else {
                retained_delayed_assets.push(delayed_asset);
            }
        }
        self.delayed_assets.extend(retained_delayed_assets);

        for delayed_asset in ready_delayed_assets.drain(..) {
            match &delayed_asset.asset {
                DelayedAssetType::TextureView(view) => {
                    self.integrate_texture(&delayed_asset.path, view);
                }
            }
        }

        let mut asset_opt = asset_manager.receive_render_asset();
        while asset_opt.is_some() {
            let asset = asset_opt.unwrap();
            match asset.asset {
                Asset::Material(material) => {
                    self.integrate_material(&asset.path, &material);
                }
                Asset::Model(model) => {
                    self.integrate_model(&asset.path, &model);
                }
                Asset::Mesh(mesh) => {
                    self.integrate_mesh(&asset.path, mesh);
                }
                Asset::Texture(texture) => {
                    let do_async = asset.priority == AssetLoadPriority::Low;
                    let (view, fence) = self.upload_texture(&asset.path, texture, do_async);
                    if let Some(fence) = fence {
                        self.delayed_assets.push(DelayedAsset {
                            fence,
                            path: asset.path.to_string(),
                            asset: DelayedAssetType::TextureView(view),
                        });
                    } else {
                        self.integrate_texture(&asset.path, &view);
                    }
                }
                Asset::Shader(shader) => {
                    shader_manager.add_shader(&asset.path, shader);
                }
                _ => unimplemented!(),
            }
            asset_opt = asset_manager.receive_render_asset();
        }

        // Make sure the work initializing the resources actually gets submitted
        self.device.flush_transfers();
        self.device.free_completed_transfers();
    }

    pub fn bump_frame(&self, context: &GraphicsContext<P::GPUBackend>) {
        self.vertex_buffer.bump_frame(context);
        self.index_buffer.bump_frame(context);
    }

    pub fn vertex_buffer(&self) -> &Arc<BufferSlice<P::GPUBackend>> {
        self.vertex_buffer.buffer()
    }

    pub fn index_buffer(&self) -> &Arc<BufferSlice<P::GPUBackend>> {
        self.index_buffer.buffer()
    }
}

impl<P: Platform> Drop for RendererAssets<P> {
    fn drop(&mut self) {
        // workaround for https://github.com/KhronosGroup/Vulkan-ValidationLayers/issues/3729
        //self.device.wait_for_idle();
    }
}
