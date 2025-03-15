use std::collections::HashMap;
use std::sync::Arc;
use crate::Mutex;

use log::trace;
use smallvec::SmallVec;

use super::*;
use crate::asset::{
    Asset, AssetData, AssetHandle, AssetLoadPriority, AssetManager, AssetType, AssetWithHandle, MaterialData, MaterialHandle, MaterialValue, MeshData, ModelData, ShaderData, ShaderHandle, TextureData, TextureHandle
};
use crate::graphics::*;

struct DelayedAsset {
    fence: Option<SharedFenceValuePair>,
    asset: AssetWithHandle,
}

pub struct AssetIntegrator {
    device: Arc<crate::graphics::Device>,
    asset_queue: Mutex<Vec<DelayedAsset>>,
    vertex_buffer: AssetBuffer,
    index_buffer: AssetBuffer,
}

impl AssetIntegrator {
    pub(crate) fn new(device: &Arc<crate::graphics::Device>) -> Self {
        let vertex_buffer = AssetBuffer::new(
            device,
            if cfg!(not(target_arch = "wasm32")) { AssetBuffer::SIZE_BIG } else { 64 },
            BufferUsage::VERTEX | BufferUsage::COPY_DST | BufferUsage::STORAGE,
        );
        let index_buffer = AssetBuffer::new(
            device,
            if cfg!(not(target_arch = "wasm32")) { AssetBuffer::SIZE_SMALL } else { 64 },
            BufferUsage::INDEX | BufferUsage::COPY_DST | BufferUsage::STORAGE,
        );

        device.flush_transfers();

        Self {
            device: device.clone(),
            asset_queue: Mutex::new(Vec::new()),
            vertex_buffer,
            index_buffer,
        }
    }

    pub fn integrate<T: Into<AssetHandle>>(
        &self,
        asset_manager: &Arc<AssetManager>,
        shader_manager: &ShaderManager,
        handle: T,
        asset_data: &AssetData,
        priority: AssetLoadPriority
    ) {
        let handle: AssetHandle = handle.into();
        trace!("Integrating asset: {:?} {:?}", asset_data.asset_type(), handle);
        let (asset, fence) = match asset_data {
            AssetData::Texture(texture_data) => {
                let (renderer_texture, fence) = self.integrate_texture(handle.into(), priority, texture_data);
                (Asset::Texture(renderer_texture), fence)
            },
            AssetData::Mesh(mesh_data) => (Asset::Mesh(self.integrate_mesh(mesh_data)), None),
            AssetData::Model(model_data) => (Asset::Model(self.integrate_model(asset_manager, model_data)), None),
            AssetData::Material(material_data) => (Asset::Material(self.integrate_material(asset_manager, material_data)), None),
            AssetData::Shader(shader_data) => (Asset::Shader(self.integrate_shader(asset_manager, shader_manager, handle.into(), shader_data)), None),
            _ => panic!("Asset type is not a renderer asset")
        };

        let mut queue: crate::MutexGuard<'_, Vec<DelayedAsset>> = self.asset_queue.lock().unwrap();
        queue.push(DelayedAsset {
            fence, asset: AssetWithHandle::combine(handle, asset)
        });
    }

    fn integrate_texture(
        &self,
        handle: TextureHandle,
        priority: AssetLoadPriority,
        texture_data: &TextureData
    ) -> (RendererTexture, Option<SharedFenceValuePair>) {
        let (view, fence) = self.upload_texture(handle, texture_data, priority == AssetLoadPriority::Low);
        let bindless_index = if self.device.supports_bindless() {
            self.device.insert_texture_into_bindless_heap(&view)
        } else {
            None
        };
        let renderer_texture = RendererTexture {
            view: view.clone(),
            bindless_index,
        };
        (renderer_texture, fence)
    }

    fn integrate_mesh(
        &self,
        mesh: &MeshData
    ) -> RendererMesh {
        if cfg!(target_arch = "wasm32") {
            // WebGPU can't do multi draw indirect or bindless anyway and likely prefers having single buffer objects per mesh.

            let mut buffer_usage = BufferUsage::VERTEX | BufferUsage::INITIAL_COPY;
            let mut buffer_size = align_up(std::mem::size_of_val(&mesh.vertices[..]), std::mem::size_of::<crate::asset::Vertex>());
            if let Some(indices) = mesh.indices.as_ref() {
                buffer_size += align_up(std::mem::size_of_val(&indices[..]), std::mem::size_of::<u32>());
                buffer_usage |= BufferUsage::INDEX;
            };
            let buffer = AssetBuffer::new(&self.device, buffer_size as u32, buffer_usage);
            let vertex_buffer = buffer.get_slice(std::mem::size_of_val(&mesh.vertices[..]), std::mem::size_of::<crate::asset::Vertex>());
            self.device.init_buffer(
                &mesh.vertices[..],
                vertex_buffer.buffer(),
                vertex_buffer.offset() as u64
            ).unwrap();

            let index_buffer = mesh.indices.as_ref().map(|indices| {
                let ib_slice = buffer.get_slice(
                    std::mem::size_of_val(&indices[..]),
                    std::mem::size_of::<u32>(),
                );
                self.device.init_buffer(
                    &indices,
                    ib_slice.buffer(),
                    ib_slice.offset() as u64,
                ).unwrap();
                ib_slice
            });
            return RendererMesh {
                vertices: vertex_buffer,
                indices: index_buffer,
                parts: mesh.parts.iter().cloned().collect(), // TODO: change base type to boxed slice
                bounding_box: mesh.bounding_box.clone(),
                vertex_count: mesh.vertex_count,
            };
        }

        assert_ne!(mesh.vertex_count, 0);

        let vertex_buffer = self.vertex_buffer.get_slice(
            std::mem::size_of_val(&mesh.vertices[..]),
            std::mem::size_of::<crate::asset::Vertex>(),
        ); // FIXME: hardcoded vertex size
        self.device.init_buffer(
            &mesh.vertices[..],
            vertex_buffer.buffer(),
            vertex_buffer.offset() as u64
        ).unwrap();

        let index_buffer = mesh.indices.as_ref().map(|indices| {
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

        RendererMesh {
            vertices: vertex_buffer,
            indices: index_buffer,
            parts: mesh.parts.iter().cloned().collect(), // TODO: change base type to boxed slice
            bounding_box: mesh.bounding_box.clone(),
            vertex_count: mesh.vertex_count,
        }
    }

    fn upload_texture(
        &self,
        handle: TextureHandle,
        texture: &TextureData,
        do_async: bool,
    ) -> (
        Arc<TextureView>,
        Option<SharedFenceValuePair>
    ) {
        let name = format!("{:?}", handle);
        let gpu_texture = self
            .device
            .create_texture(&texture.info, Some(&name)).unwrap();
        let subresources = texture.info.array_length * texture.info.mip_levels;
        let mut fence = Option::<SharedFenceValuePair>::None;
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
            Some(&name),
        );

        (view, fence)
    }

    fn integrate_material(
        &self,
        asset_manager: &Arc<AssetManager>,
        material: &MaterialData,
    ) -> RendererMaterial {
        let mut properties =
            HashMap::<String, RendererMaterialValue>::with_capacity(material.properties.len());
        for (key, value) in &material.properties {
            match value {
                MaterialValue::Texture(path) => {
                    let texture_handle = asset_manager.get_or_reserve_handle(path, AssetType::Texture);
                    properties.insert(key.to_string(), RendererMaterialValue::Texture(texture_handle.into()));
                }
                MaterialValue::Float(val) => {
                    properties.insert(key.to_string(), RendererMaterialValue::Float(*val));
                }
                MaterialValue::Vec4(val) => {
                    properties.insert(key.to_string(), RendererMaterialValue::Vec4(*val));
                }
            }
        }

        RendererMaterial {
            shader_name: material.shader_name.clone(),
            properties,
        }
    }

    fn integrate_model(
        &self,
        asset_manager: &Arc<AssetManager>,
        model: &ModelData
    ) -> RendererModel {
        let mesh_handle = asset_manager.get_or_reserve_handle(&model.mesh_path, AssetType::Mesh);

        let mut renderer_materials =
            SmallVec::<[MaterialHandle; 16]>::with_capacity(model.material_paths.len());
        for material_path in &model.material_paths {
            let material_handle = asset_manager.get_or_reserve_handle(material_path, AssetType::Material);
            renderer_materials.push(material_handle.into());
        }

        RendererModel::new(mesh_handle.into(), renderer_materials)
    }

    fn integrate_shader(
        &self,
        asset_manager: &Arc<AssetManager>,
        shader_manager: &ShaderManager,
        handle: ShaderHandle,
        shader: &ShaderData
    ) -> RendererShader {
        let name = format!("{:?}", handle);
        let shader = Arc::new(self.device.create_shader(shader, Some(&name)));
        asset_manager.add_asset_with_handle(AssetWithHandle::combine(handle.into(), Asset::Shader(shader.clone())));
        shader_manager.add_shader(asset_manager, handle, &shader);
        shader
    }

    pub(super) fn flush(
        &self,
        asset_manager: &Arc<AssetManager>,
        _shader_manager: &ShaderManager,
    ) {
        let mut retained_delayed_assets = SmallVec::<[DelayedAsset; 2]>::new();
        let mut ready_delayed_assets = SmallVec::<[DelayedAsset; 2]>::new();
        {
            let mut queue = self.asset_queue.lock().unwrap();
            for delayed_asset in queue.drain(..) {
                if let Some(fence) = delayed_asset.fence.as_ref() {
                    if fence.is_signalled() {
                        ready_delayed_assets.push(delayed_asset);
                    } else {
                        retained_delayed_assets.push(delayed_asset);
                    }
                } else {
                    ready_delayed_assets.push(delayed_asset);
                }
            }
            queue.extend(retained_delayed_assets);
        }

        for delayed_asset in ready_delayed_assets.drain(..) {
            asset_manager.add_asset_with_handle(delayed_asset.asset);
        }

        // Make sure the work initializing the resources actually gets submitted
        self.device.flush_transfers();
        self.device.free_completed_transfers();
    }

    #[inline(always)]
    pub(crate) fn bump_frame(&self, context: &GraphicsContext) {
        self.vertex_buffer.bump_frame(context);
        self.index_buffer.bump_frame(context);
    }

    #[inline(always)]
    pub(crate) fn vertex_buffer(&self) -> &Arc<BufferSlice> {
        self.vertex_buffer.buffer()
    }

    #[inline(always)]
    pub(crate) fn index_buffer(&self) -> &Arc<BufferSlice> {
        self.index_buffer.buffer()
    }
}
