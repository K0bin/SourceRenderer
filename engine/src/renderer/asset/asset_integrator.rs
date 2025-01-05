use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use smallvec::SmallVec;

use sourcerenderer_core::Platform;

use super::*;
use crate::asset::{
    Asset, AssetData, AssetHandle, AssetLoadPriority, AssetManager, AssetType, AssetWithHandle, MaterialData, MaterialHandle, MaterialValue, MeshData, ModelData, ShaderData, TextureData
};
use crate::graphics::*;

struct DelayedAsset<P: Platform> {
    fence: Option<SharedFenceValuePair<P::GPUBackend>>,
    asset: AssetWithHandle<P>,
}

pub struct AssetIntegrator<P: Platform> {
    device: Arc<crate::graphics::Device<P::GPUBackend>>,
    asset_queue: Mutex<Vec<DelayedAsset<P>>>,
    vertex_buffer: AssetBuffer<P::GPUBackend>,
    index_buffer: AssetBuffer<P::GPUBackend>,
}

impl<P: Platform> AssetIntegrator<P> {
    pub(crate) fn new(device: &Arc<crate::graphics::Device<P::GPUBackend>>) -> Self {

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
            asset_queue: Mutex::new(Vec::new()),
            vertex_buffer,
            index_buffer,
        }
    }

    pub fn integrate(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        shader_manager: &ShaderManager<P>,
        path: &str,
        asset_data: &AssetData,
        priority: AssetLoadPriority
    ) {
        let handle = asset_manager.reserve_handle(path, asset_data.asset_type());

        let (asset, fence) = match asset_data {
            AssetData::Texture(texture_data) => {
                let (renderer_texture, fence) = self.integrate_texture(path, priority, texture_data);
                (Asset::<P>::Texture(renderer_texture), fence)
            },
            AssetData::Mesh(mesh_data) => (Asset::<P>::Mesh(self.integrate_mesh(mesh_data)), None),
            AssetData::Model(model_data) => (Asset::<P>::Model(self.integrate_model(asset_manager, model_data)), None),
            AssetData::Material(material_data) => (Asset::<P>::Material(self.integrate_material(asset_manager, material_data)), None),
            AssetData::Shader(shader_data) => (Asset::<P>::Shader(self.integrate_shader(asset_manager, shader_manager, path, shader_data)), None),
            _ => panic!("Asset type is not a renderer asset")
        };

        let mut queue = self.asset_queue.lock().unwrap();
        queue.push(DelayedAsset {
            fence, asset: AssetWithHandle::combine(handle, asset)
        });
    }

    fn integrate_texture(
        &self,
        path: &str,
        priority: AssetLoadPriority,
        texture_data: &TextureData
    ) -> (RendererTexture<P::GPUBackend>, Option<SharedFenceValuePair<P::GPUBackend>>) {
        let (view, fence) = self.upload_texture(path, texture_data, priority == AssetLoadPriority::Low);
        let bindless_index = if self.device.supports_bindless() {
            self.device.insert_texture_into_bindless_heap(&view)
        } else {
            None
        };
        let renderer_texture: RendererTexture<<P as Platform>::GPUBackend> = RendererTexture {
            view: view.clone(),
            bindless_index,
        };
        (renderer_texture, fence)
    }

    fn integrate_mesh(
        &self,
        mesh: &MeshData
    ) -> RendererMesh<P::GPUBackend> {
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
        path: &str,
        texture: &TextureData,
        do_async: bool,
    ) -> (
        Arc<TextureView<P::GPUBackend>>,
        Option<SharedFenceValuePair<P::GPUBackend>>
    ) {
        let gpu_texture = self
            .device
            .create_texture(&texture.info, Some(path)).unwrap();
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
            Some(path),
        );

        (view, fence)
    }

    fn integrate_material(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        material: &MaterialData,
    ) -> RendererMaterial {
        let mut properties =
            HashMap::<String, RendererMaterialValue>::with_capacity(material.properties.len());
        for (key, value) in &material.properties {
            match value {
                MaterialValue::Texture(path) => {
                    let texture = asset_manager.reserve_handle(path, AssetType::Texture);
                    if let AssetHandle::Texture(texture) = texture {
                        properties.insert(key.to_string(), RendererMaterialValue::Texture(texture));
                    } else {
                        unreachable!();
                    }
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
        asset_manager: &Arc<AssetManager<P>>,
        model: &ModelData
    ) -> RendererModel {
        let handle = asset_manager.reserve_handle(&model.mesh_path, AssetType::Mesh);
        let mesh = if let AssetHandle::Mesh(mesh) = handle {
            mesh
        } else {
            unreachable!()
        };

        let mut renderer_materials =
            SmallVec::<[MaterialHandle; 16]>::with_capacity(model.material_paths.len());
        for material_path in &model.material_paths {
            let material_handle = asset_manager.reserve_handle(material_path, AssetType::Material);
            if let AssetHandle::Material(material_handle) = material_handle {
                renderer_materials.push(material_handle);
            } else {
                unreachable!();
            }
        }

        RendererModel::new(mesh, renderer_materials)
    }

    fn integrate_shader(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        shader_manager: &ShaderManager<P>,
        path: &str,
        shader: &ShaderData
    ) -> RendererShader<P::GPUBackend> {
        let shader = Arc::new(self.device.create_shader(shader, Some(path)));
        shader_manager.add_shader(asset_manager, path, &shader);
        shader
    }

    pub(super) fn flush(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        _shader_manager: &ShaderManager<P>,
    ) {
        let mut retained_delayed_assets = SmallVec::<[DelayedAsset<P>; 2]>::new();
        let mut ready_delayed_assets = SmallVec::<[DelayedAsset<P>; 2]>::new();
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
                    retained_delayed_assets.push(delayed_asset);
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

    pub(crate) fn bump_frame(&self, context: &GraphicsContext<P::GPUBackend>) {
        self.vertex_buffer.bump_frame(context);
        self.index_buffer.bump_frame(context);
    }

    pub(crate) fn vertex_buffer(&self) -> &Arc<BufferSlice<P::GPUBackend>> {
        self.vertex_buffer.buffer()
    }

    pub(crate) fn index_buffer(&self) -> &Arc<BufferSlice<P::GPUBackend>> {
        self.index_buffer.buffer()
    }
}

impl<P: Platform> Drop for AssetIntegrator<P> {
    fn drop(&mut self) {
        // workaround for https://github.com/KhronosGroup/Vulkan-ValidationLayers/issues/3729
        //self.device.wait_for_idle();
    }
}
