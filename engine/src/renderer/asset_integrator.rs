use std::collections::HashMap;
use std::sync::Arc;

use smallvec::SmallVec;

use sourcerenderer_core::{
    Platform,
    Vec4,
};

use super::asset_buffer::AssetBuffer;
use super::shader_manager::ShaderManager;
use crate::asset::{
    Asset, AssetData, AssetHandle, AssetLoadPriority, AssetManager, AssetType, AssetWithHandle, MaterialData, MaterialHandle, MaterialValue, MeshData, ModelData, TextureData, TextureHandle
};
use crate::graphics::*;

struct DelayedAsset<P: Platform> {
    fence: Option<SharedFenceValuePair<P::GPUBackend>>,
    asset: AssetWithHandle<P>,
}

pub struct AssetIntegrator<P: Platform> {
    device: Arc<crate::graphics::Device<P::GPUBackend>>,
    asset_queue: Vec<DelayedAsset<P>>,
    vertex_buffer: AssetBuffer<P::GPUBackend>,
    index_buffer: AssetBuffer<P::GPUBackend>,
}

impl<P: Platform> AssetIntegrator<P> {
    pub(crate) fn new(device: &Arc<crate::graphics::Device<P::GPUBackend>>) -> Self {
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
            asset_queue: Vec::new(),
            vertex_buffer,
            index_buffer,
        }
    }

    pub fn integrate(
        &mut self,
        asset_manager: &Arc<AssetManager<P>>,
        path: &str,
        asset_data: AssetData,
        priority: AssetLoadPriority
    ) {
        let handle = asset_manager.reserve_handle(path, asset_data.asset_type());

        let (asset, fence) = match &asset_data {
            AssetData::Texture(texture_data) => {
                let (renderer_texture, fence) = self.integrate_texture(path, priority, texture_data);
                (Asset::<P>::Texture(renderer_texture), fence)
            },
            AssetData::Mesh(mesh_data) => (Asset::<P>::Mesh(self.integrate_mesh(mesh_data)), None),
            AssetData::Model(model_data) => (Asset::<P>::Model(self.integrate_model(asset_manager, model_data)), None),
            AssetData::Material(material_data) => (Asset::<P>::Material(self.integrate_material(asset_manager, material_data)), None),
            AssetData::Shader(packed_shader) => unimplemented!(),
            _ => panic!("Asset type is not a renderer asset")
        };

        self.asset_queue.push(DelayedAsset {
            fence, asset: AssetWithHandle::combine(handle, asset)
        });
    }

    fn finish_integrating_texture_aync(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        handle: TextureHandle,
        texture: Arc<TextureView<<P as Platform>::GPUBackend>>
    ) {
        let bindless_index: Option<BindlessSlot<<P as Platform>::GPUBackend>> = if self.device.supports_bindless() {
            self.device.insert_texture_into_bindless_heap(&texture)
        } else {
            None
        };
        let renderer_texture: RendererTexture<<P as Platform>::GPUBackend> = RendererTexture {
            view: texture.clone(),
            bindless_index,
        };
        asset_manager.add_asset_with_handle( AssetWithHandle::Texture(handle, renderer_texture));
    }

    pub fn integrate_texture(
        &mut self,
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

    pub fn integrate_mesh(
        &mut self,
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

        RendererMesh {
            vertices: vertex_buffer,
            indices: index_buffer,
            parts: mesh.parts.iter().cloned().collect(), // TODO: change base type to boxed slice
            bounding_box: mesh.bounding_box,
            vertex_count: mesh.vertex_count,
        }
    }

    pub fn upload_texture(
        &mut self,
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

    pub fn integrate_material(
        &mut self,
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

    pub fn integrate_model(
        &mut self,
        asset_manager: &Arc<AssetManager<P>>,
        model: &ModelData
    ) -> RendererModel {
        let mesh = asset_manager.reserve_handle(&model.mesh_path, AssetType::Mesh);

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

    pub(super) fn flush(
        &mut self,
        asset_manager: &Arc<AssetManager<P>>,
        shader_manager: &mut ShaderManager<P>,
    ) {
        let mut retained_delayed_assets = SmallVec::<[DelayedAsset<P>; 2]>::new();
        let mut ready_delayed_assets = SmallVec::<[DelayedAsset<P>; 2]>::new();
        for delayed_asset in self.asset_queue.drain(..) {
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
        self.asset_queue.extend(retained_delayed_assets);

        for delayed_asset in ready_delayed_assets.drain(..) {
            asset_manager.add_asset_with_handle(delayed_asset.asset);
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

impl<P: Platform> Drop for AssetIntegrator<P> {
    fn drop(&mut self) {
        // workaround for https://github.com/KhronosGroup/Vulkan-ValidationLayers/issues/3729
        //self.device.wait_for_idle();
    }
}
