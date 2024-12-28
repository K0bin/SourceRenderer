use std::collections::HashMap;
use std::io::SeekFrom;
use std::sync::Arc;
use std::{
    slice,
    usize,
};

use bevy_math::{EulerRot, Quat};
use bevy_tasks::futures_lite::io::Cursor;
use bevy_tasks::futures_lite::{AsyncReadExt, AsyncSeekExt};
use bevy_transform::components::Transform;
use gltf::buffer::{
    Source,
    View,
};
use gltf::material::AlphaMode;
use gltf::texture::WrappingMode;
use gltf::{
    Gltf,
    Material,
    Node,
    Primitive,
    Scene,
    Semantic,
};
use log::warn;
use sourcerenderer_core::{
    Platform,
    Quaternion,
    Vec2,
    Vec3,
    Vec4,
};

use crate::asset::asset_manager::{
    AssetFile,
    DirectlyLoadedAsset,
};
use crate::asset::loaded_level::{LoadedEntityParent, LoadedLevel};
use crate::asset::{
    Asset, AssetLoadPriority, AssetLoader, AssetLoaderAsync, AssetLoaderProgress, AssetManager, AssetType, Mesh, MeshRange, Model, Vertex
};
use crate::math::BoundingBox;
use crate::renderer::{
    DirectionalLightComponent,
    PointLightComponent,
    StaticRenderableComponent,
};

pub struct GltfLoader {}

impl GltfLoader {
    pub fn new() -> Self {
        Self {}
    }

    fn visit_node<P: Platform>(
        node: &Node,
        world: &mut LoadedLevel,
        asset_mgr: &Arc<AssetManager<P>>,
        parent_entity: Option<usize>,
        gltf_file_name: &str,
        buffer_cache: &mut HashMap<String, Vec<u8>>,
    ) {
        let (translation, rotation, scale) = match node.transform() {
            gltf::scene::Transform::Matrix {
                matrix: _columns_data,
            } => {
                unimplemented!()

                /*let mut matrix = Matrix4::default();
                for i in 0..matrix.len() {
                  let column_slice = &columns_data[0];
                  matrix.column_mut(i).copy_from_slice(column_slice);
                }
                matrix*/
            }
            gltf::scene::Transform::Decomposed {
                translation,
                rotation,
                scale,
            } => (
                Vec3::new(translation[0], translation[1], translation[2]),
                Vec4::new(rotation[0], rotation[1], rotation[2], rotation[3]),
                Vec3::new(scale[0], scale[1], scale[2]),
            ),
        };

        let fixed_position = fixup_vec(&translation);
        let fixed_rotation = Vec4::new(rotation.x, rotation.y, rotation.z, rotation.w);
        let rot_quat = Quat::from_vec4(fixed_rotation).normalize();
        let euler_angles = rot_quat.to_euler(EulerRot::XYZ);
        let rot_quat = Quat::from_euler(EulerRot::XYZ, euler_angles.0, -euler_angles.1, -euler_angles.2);
        let entity = world.push_entity(3);
        world.push_component(entity, Transform {
            translation: fixed_position,
            scale,
            rotation: rot_quat,
        });
        if let Some(parent) = parent_entity {
            world.push_component(entity, LoadedEntityParent(parent));
        }

        if let Some(mesh) = node.mesh() {
            let model_name = node
                .name()
                .map_or_else(|| node.index().to_string(), |name| name.to_string());
            let mut mesh_path = gltf_file_name.to_string();
            mesh_path += "/mesh/";
            mesh_path += &model_name;

            let mut indices = Vec::<u32>::new();
            let mut vertices = Vec::<Vertex>::new();
            let mut parts = Vec::<MeshRange>::with_capacity(mesh.primitives().len());
            let mut bounding_box = Option::<BoundingBox>::None;
            let mut materials = Vec::<String>::new();
            for primitive in mesh.primitives() {
                let part_start = indices.len();
                GltfLoader::load_primitive(
                    &primitive,
                    asset_mgr,
                    &mut vertices,
                    &mut indices,
                    gltf_file_name,
                    buffer_cache,
                );
                let material_path =
                    GltfLoader::load_material(&primitive.material(), asset_mgr, gltf_file_name);
                materials.push(material_path);
                let primitive_bounding_box = primitive.bounding_box();
                if let Some(bounding_box) = &mut bounding_box {
                    bounding_box.min.x =
                        f32::min(bounding_box.min.x, primitive_bounding_box.min[0]);
                    bounding_box.min.y =
                        f32::min(bounding_box.min.y, primitive_bounding_box.min[1]);
                    bounding_box.min.z =
                        f32::min(bounding_box.min.z, primitive_bounding_box.min[2]);
                    bounding_box.max.x =
                        f32::max(bounding_box.max.x, primitive_bounding_box.max[0]);
                    bounding_box.max.y =
                        f32::max(bounding_box.max.y, primitive_bounding_box.max[1]);
                    bounding_box.max.z =
                        f32::max(bounding_box.max.z, primitive_bounding_box.max[2]);
                } else {
                    bounding_box = Some(BoundingBox::new(
                        Vec3::new(
                            primitive_bounding_box.min[0],
                            primitive_bounding_box.min[1],
                            primitive_bounding_box.min[2],
                        ),
                        Vec3::new(
                            primitive_bounding_box.max[0],
                            primitive_bounding_box.max[1],
                            primitive_bounding_box.max[2],
                        ),
                    ));
                }
                let range = MeshRange {
                    start: part_start as u32,
                    count: (indices.len() - part_start) as u32,
                };
                parts.push(range);
            }
            indices.reverse();
            for part in &mut parts {
                part.start = indices.len() as u32 - part.start - part.count;
            }

            let vertices_count = vertices.len();
            let vertices_box = vertices.into_boxed_slice();
            let size_old = std::mem::size_of_val(vertices_box.as_ref());
            let ptr = Box::into_raw(vertices_box);
            let data_ptr = unsafe {
                slice::from_raw_parts_mut(
                    ptr as *mut u8,
                    vertices_count * std::mem::size_of::<Vertex>(),
                ) as *mut [u8]
            };
            let vertices_data = unsafe { Box::from_raw(data_ptr) };
            assert_eq!(size_old, std::mem::size_of_val(vertices_data.as_ref()));

            let indices_count = indices.len();
            let indices_box = indices.into_boxed_slice();
            let size_old = std::mem::size_of_val(indices_box.as_ref());
            let ptr = Box::into_raw(indices_box);
            let data_ptr = unsafe {
                slice::from_raw_parts_mut(
                    ptr as *mut u8,
                    indices_count * std::mem::size_of::<u32>(),
                ) as *mut [u8]
            };
            let indices_data = unsafe { Box::from_raw(data_ptr) };
            assert_eq!(size_old, std::mem::size_of_val(indices_data.as_ref()));

            if let Some(bounding_box) = bounding_box.as_mut() {
                // Right hand -> left hand coordinate system conversion
                let bb_min_x = bounding_box.min.x;
                bounding_box.min.x = -bounding_box.max.x;
                bounding_box.max.x = -bb_min_x;
            }

            asset_mgr.add_asset(
                &mesh_path,
                Asset::Mesh(Mesh {
                    indices: (indices_count > 0).then(|| indices_data),
                    vertices: vertices_data,
                    bounding_box: bounding_box,
                    parts: parts.into_boxed_slice(),
                    vertex_count: vertices_count as u32,
                }),
                AssetLoadPriority::Normal,
            );

            let mut model_path = gltf_file_name.to_string();
            model_path += "/model/";
            model_path += &model_name;
            asset_mgr.add_asset(
                &model_path,
                Asset::Model(Model {
                    mesh_path: mesh_path.clone(),
                    material_paths: materials,
                }),
                AssetLoadPriority::Normal,
            );

            world.push_component(entity, StaticRenderableComponent {
                model_path,
                receive_shadows: true,
                cast_shadows: true,
                can_move: false,
            });
        };

        if node.skin().is_some() {
            println!(
                "WARNING: skins are not supported. Node name: {:?}",
                node.name()
            );
        }
        if node.camera().is_some() {
            println!(
                "WARNING: cameras are not supported. Node name: {:?}",
                node.name()
            );
        }
        if node.weights().is_some() {
            println!(
                "WARNING: weights are not supported. Node name: {:?}",
                node.name()
            );
        }

        if let Some(light) = node.light() {
            println!("light is dir");
            {
                let mut transform: &mut Transform = world.get_component_mut(entity).unwrap();
                let mut coords = Vec4::from(transform.rotation);
                coords.z = -coords.z;
                transform.rotation = Quat::from_vec4(coords).normalize();
            }
            match light.kind() {
                gltf::khr_lights_punctual::Kind::Directional => {
                    world.push_component(entity, DirectionalLightComponent {
                        intensity: light.intensity() * 685f32, // Blender exports as W/m2, we need lux
                    });
                }
                gltf::khr_lights_punctual::Kind::Point => {
                    world.push_component(entity, PointLightComponent {
                        intensity: light.intensity(),
                    });
                }
                gltf::khr_lights_punctual::Kind::Spot { .. } => todo!(),
            }
        }

        for child in node.children() {
            GltfLoader::visit_node(
                &child,
                world,
                asset_mgr,
                Some(entity),
                gltf_file_name,
                buffer_cache,
            );
        }
    }

    fn load_scene<P: Platform>(
        scene: &Scene,
        asset_mgr: &Arc<AssetManager<P>>,
        gltf_file_name: &str,
    ) -> LoadedLevel {
        let mut world = LoadedLevel::new(4096, 64);
        let mut buffer_cache = HashMap::<String, Vec<u8>>::new();
        let nodes = scene.nodes();
        for node in nodes {
            GltfLoader::visit_node(
                &node,
                &mut world,
                asset_mgr,
                None,
                gltf_file_name,
                &mut buffer_cache,
            );
        }
        world
    }

    async fn load_primitive<'a, P: Platform>(
        primitive: &'a Primitive<'a>,
        asset_mgr: &'a Arc<AssetManager<P>>,
        vertices: &'a mut Vec<Vertex>,
        indices: &'a mut Vec<u32>,
        gltf_file_name: &'a str,
        buffer_cache: &'a mut HashMap<String, Vec<u8>>,
    ) {
        async fn load_buffer<'a, P: Platform>(
            gltf_file_name: &str,
            gltf_path: &str,
            asset_mgr: &Arc<AssetManager<P>>,
            buffer_cache: &'a mut HashMap<String, Vec<u8>>,
            view: &View<'_>,
        ) -> Vec<u8> {
            let mut data = vec![0u8; view.length()];
            match view.buffer().source() {
                Source::Bin => {
                    let url = format!(
                        "{}/buffer/{}-{}",
                        gltf_file_name,
                        view.offset(),
                        view.length()
                    );
                    let mut file = asset_mgr.load_file(&url).await.expect("Failed to load buffer");
                    let _ = file.read_exact(&mut data).await.unwrap();
                }
                Source::Uri(uri) => {
                    let url = gltf_path.to_string() + uri;
                    if let Some(cached_data) = buffer_cache.get_mut(&url) {
                        data.copy_from_slice(
                            &cached_data[view.offset()..(view.offset() + view.length())],
                        );
                    } else {
                        let mut file = asset_mgr.load_file(&url).await.expect("Failed to load buffer");
                        let start = file.seek(SeekFrom::Current(0)).await.unwrap();
                        let mut file_data =
                            vec![0u8; (file.seek(SeekFrom::End(0)).await.unwrap() - start) as usize];
                        let _ = file.seek(SeekFrom::Start(start)).await.unwrap();
                        let _ = file.read_exact(&mut file_data).await.unwrap();
                        data.copy_from_slice(&file_data[view.offset()..(view.offset() + view.length())]);
                        buffer_cache.insert(url.clone(), file_data);
                    }
                }
            };
            data
        }

        let index_base = vertices.len() as u32;
        let gltf_path = if let Some(last_slash) = gltf_file_name.rfind('/') {
            &gltf_file_name[..last_slash + 1]
        } else {
            gltf_file_name
        };

        {
            let positions = primitive.get(&Semantic::Positions).unwrap();
            assert!(positions.sparse().is_none());
            let positions_view = positions.view().unwrap();
            let positions_data = load_buffer(
                gltf_file_name,
                gltf_path,
                asset_mgr,
                buffer_cache,
                &positions_view,
            ).await;
            let mut positions_buffer_cursor = Cursor::new(&positions_data[..]);
            let positions_stride = if let Some(stride) = positions_view.stride() {
                stride
            } else {
                positions.size()
            };

            let normals = primitive.get(&Semantic::Normals).unwrap();
            assert!(normals.sparse().is_none());
            let normals_view = normals.view().unwrap();
            let same_buffer = match (
                positions_view.buffer().source(),
                normals_view.buffer().source(),
            ) {
                (Source::Bin, Source::Bin) => true,
                (Source::Uri(uri1), Source::Uri(uri2)) => uri1 == uri2,
                _ => false,
            };
            let normals_data: Vec<u8>;
            let mut normals_buffer_cursor = if same_buffer
                && normals_view.offset() == positions_view.offset()
                && normals_view.length() == positions_view.length()
                && normals_view.stride() == positions_view.stride()
            {
                Cursor::new(&positions_data[..])
            } else {
                normals_data = load_buffer(
                    gltf_file_name,
                    gltf_path,
                    asset_mgr,
                    buffer_cache,
                    &normals_view,
                ).await;
                Cursor::new(&normals_data[..])
            };
            let normals_stride = if let Some(stride) = normals_view.stride() {
                stride
            } else {
                normals.size()
            };

            let texcoords = primitive.get(&Semantic::TexCoords(0)).unwrap();
            assert!(texcoords.sparse().is_none());
            let texcoords_view = texcoords.view().unwrap();
            let same_buffer = match (
                positions_view.buffer().source(),
                texcoords_view.buffer().source(),
            ) {
                (Source::Bin, Source::Bin) => true,
                (Source::Uri(uri1), Source::Uri(uri2)) => uri1 == uri2,
                _ => false,
            };
            let texcoords_data: Vec<u8>;
            let mut texcoords_buffer_cursor = if same_buffer
                && texcoords_view.offset() == positions_view.offset()
                && texcoords_view.length() == positions_view.length()
                && texcoords_view.stride() == positions_view.stride()
            {
                Cursor::new(&positions_data[..])
            } else {
                texcoords_data = load_buffer(
                    gltf_file_name,
                    gltf_path,
                    asset_mgr,
                    buffer_cache,
                    &texcoords_view,
                ).await;
                Cursor::new(&texcoords_data[..])
            };
            let texcoords_stride = if let Some(stride) = texcoords_view.stride() {
                stride
            } else {
                texcoords.size()
            };

            positions_buffer_cursor
                .seek(SeekFrom::Start(positions.offset() as u64))
                .await
                .unwrap();
            normals_buffer_cursor
                .seek(SeekFrom::Start(normals.offset() as u64))
                .await
                .unwrap();
            texcoords_buffer_cursor
                .seek(SeekFrom::Start(texcoords.offset() as u64))
                .await
                .unwrap();

            assert_eq!(positions.count(), normals.count());
            for i in 0..positions.count() {
                positions_buffer_cursor
                    .seek(SeekFrom::Start(
                        positions.offset() as u64 + (i * positions_stride) as u64,
                    ))
                    .await
                    .unwrap();
                let mut position_data = vec![0; positions.size()];
                positions_buffer_cursor
                    .read_exact(&mut position_data)
                    .await
                    .unwrap();
                assert_eq!(position_data.len(), std::mem::size_of::<Vec3>());

                normals_buffer_cursor
                    .seek(SeekFrom::Start(
                        normals.offset() as u64 + (i * normals_stride) as u64,
                    ))
                    .await
                    .unwrap();
                let mut normal_data = vec![0; normals.size()];
                normals_buffer_cursor.read_exact(&mut normal_data)
                    .await
                    .unwrap();
                assert_eq!(normal_data.len(), std::mem::size_of::<Vec3>());

                texcoords_buffer_cursor
                    .seek(SeekFrom::Start(
                        texcoords.offset() as u64 + (i * texcoords_stride) as u64,
                    ))
                    .await
                    .unwrap();
                let mut texcoords_data = vec![0; texcoords.size()];
                texcoords_buffer_cursor
                    .read_exact(&mut texcoords_data)
                    .await
                    .unwrap();
                assert_eq!(texcoords_data.len(), std::mem::size_of::<Vec2>());

                unsafe {
                    let position_vec_ptr: *const Vec3 = std::mem::transmute(position_data.as_ptr());
                    let normal_vec_ptr: *const Vec3 = std::mem::transmute(normal_data.as_ptr());
                    let texcoord_vec_ptr: *const Vec2 =
                        std::mem::transmute(texcoords_data.as_ptr());
                    let position = fixup_vec(&*position_vec_ptr);
                    let mut normal = fixup_vec(&*normal_vec_ptr);
                    normal = normal.normalize();
                    vertices.push(Vertex {
                        position,
                        normal,
                        tex_coord: *texcoord_vec_ptr,
                        color: [255, 255, 255, 255],
                    });
                }

                debug_assert!(
                    positions_buffer_cursor.seek(SeekFrom::Current(0)).await.unwrap()
                        <= (positions_view.offset() + positions_view.length()) as u64
                );
                debug_assert!(
                    normals_buffer_cursor.seek(SeekFrom::Current(0)).await.unwrap()
                        <= (normals_view.offset() + normals_view.length()) as u64
                );
                debug_assert!(
                    texcoords_buffer_cursor.seek(SeekFrom::Current(0)).await.unwrap()
                        <= (texcoords_view.offset() + texcoords_view.length()) as u64
                );
            }
        }

        let indices_accessor = primitive.indices();
        if let Some(indices_accessor) = indices_accessor {
            assert!(indices_accessor.sparse().is_none());
            let view = indices_accessor.view().unwrap();
            let data = load_buffer(gltf_file_name, gltf_path, asset_mgr, buffer_cache, &view).await;
            let mut buffer_cursor = Cursor::new(&data);
            buffer_cursor
                .seek(SeekFrom::Start(indices_accessor.offset() as u64))
                .await
                .unwrap();

            for _ in 0..indices_accessor.count() {
                let start = buffer_cursor.seek(SeekFrom::Current(0)).await.unwrap();

                let mut attr_data = vec![0; indices_accessor.size()];
                buffer_cursor.read_exact(&mut attr_data).await.unwrap();

                assert!(indices_accessor.size() <= std::mem::size_of::<u32>());

                unsafe {
                    if indices_accessor.size() == 4 {
                        let index_ptr: *const u32 = std::mem::transmute(attr_data.as_ptr());
                        indices.push(*index_ptr + index_base);
                    } else if indices_accessor.size() == 2 {
                        let index_ptr: *const u16 = std::mem::transmute(attr_data.as_ptr());
                        indices.push(*index_ptr as u32 + index_base);
                    } else {
                        unimplemented!();
                    }
                }

                if let Some(stride) = view.stride() {
                    assert!(stride > indices_accessor.size());
                    buffer_cursor
                        .seek(SeekFrom::Start(start + stride as u64))
                        .await
                        .unwrap();
                }
            }
            assert!(
                buffer_cursor.seek(SeekFrom::Current(0)).await.unwrap()
                    <= (view.offset() + view.length()) as u64
            );
        }
    }

    fn load_material<P: Platform>(
        material: &Material,
        asset_mgr: &Arc<AssetManager<P>>,
        gltf_file_name: &str,
    ) -> String {
        let gltf_path = if let Some(last_slash) = gltf_file_name.rfind('/') {
            &gltf_file_name[..last_slash + 1]
        } else {
            gltf_file_name
        };
        let material_path = format!(
            "{}/material/{}",
            gltf_file_name.to_string(),
            material
                .index()
                .map_or_else(|| "default".to_string(), |index| index.to_string())
        );

        let pbr = material.pbr_metallic_roughness();
        if material.double_sided() {
            //warn!("Double sided materials are not supported, material path: {}", material_path);
        }
        if material.alpha_mode() != AlphaMode::Opaque {
            //warn!("Unsupported alpha mode, alpha mode: {:?}, material path: {}", material.alpha_mode(), material_path);
        }

        let albedo_info = pbr.base_color_texture();
        let albedo_path = albedo_info
            .and_then(|albedo| {
                if albedo.tex_coord() == 0 {
                    Some(albedo)
                } else {
                    warn!("Found non zero texcoord for texture: {}", &material_path);
                    None
                }
            })
            .map(|albedo| {
                if albedo.texture().sampler().wrap_s() != WrappingMode::Repeat
                    || albedo.texture().sampler().wrap_t() != WrappingMode::Repeat
                {
                    warn!(
                        "Texture uses non-repeat wrap mode: s: {:?}, t: {:?}",
                        albedo.texture().sampler().wrap_s(),
                        albedo.texture().sampler().wrap_t()
                    );
                }
                let albedo_source = albedo.texture().source().source();
                match albedo_source {
                    gltf::image::Source::View { view, mime_type } => {
                        let mime_parts: Vec<&str> = mime_type.split('/').collect();
                        let file_type = mime_parts[1].to_lowercase();
                        format!(
                            "{}/texture/{}-{}.{}",
                            gltf_file_name,
                            view.offset(),
                            view.length(),
                            &file_type
                        )
                    }
                    gltf::image::Source::Uri {
                        uri,
                        mime_type: _mime_type,
                    } => gltf_path.to_string() + uri,
                }
            });

        if let Some(albedo_path) = albedo_path {
            asset_mgr.request_asset(&albedo_path, AssetType::Material, AssetLoadPriority::Low);
            asset_mgr.add_material(
                &material_path,
                &albedo_path,
                pbr.roughness_factor(),
                pbr.metallic_factor(),
            );
        } else {
            let color = pbr.base_color_factor();
            asset_mgr.add_material_color(
                &material_path,
                Vec4::new(color[0], color[1], color[2], color[3]),
                pbr.roughness_factor(),
                pbr.metallic_factor(),
            );
        }
        material_path
    }
}

impl<P: Platform> AssetLoaderAsync<P> for GltfLoader {
    fn matches(&self, file: &mut AssetFile) -> bool {
        (file.path.contains("gltf") || file.path.contains("glb"))
            && file.path.contains("/scene/")
            && Gltf::from_reader(file).is_ok()
    }

    async fn load(
        &self,
        file: AssetFile,
        manager: &Arc<AssetManager<P>>,
        _priority: AssetLoadPriority,
        _progress: &Arc<AssetLoaderProgress>,
    ) -> Result<DirectlyLoadedAsset, ()> {
        let path = file.path.clone();
        let gltf = Gltf::from_reader(file).unwrap();
        const PUNCTUAL_LIGHT_EXTENSION: &'static str = "KHR_lights_punctual";
        for extension in gltf.extensions_required() {
            if extension != PUNCTUAL_LIGHT_EXTENSION {
                log::warn!("GLTF file requires unsupported extension: {}", extension)
            }
        }
        for extension in gltf.extensions_used() {
            if extension != PUNCTUAL_LIGHT_EXTENSION {
                log::warn!("GLTF file uses unsupported extension: {}", extension)
            }
        }

        let scene_prefix = "/scene/";
        let scene_name_start = path.find(scene_prefix);
        if let Some(scene_name_start) = scene_name_start {
            let gltf_name = &path[0..scene_name_start];
            let scene_name = &path[scene_name_start + scene_prefix.len()..];
            for scene in gltf.scenes() {
                if scene
                    .name()
                    .map_or_else(|| scene.index().to_string(), |name| name.to_string())
                    == scene_name
                {
                    let world = GltfLoader::load_scene(&scene, manager, gltf_name);
                    return Ok(DirectlyLoadedAsset::Level(world));
                }
            }
        }

        unimplemented!()
    }
}

// glTF uses a right-handed coordinate system. glTF defines +Y as up, +Z as forward, and -X as right; the front of a glTF asset faces +Z.
// We use a left-handed coordinate system with +Y as up, +Z as forward and +X as right. => flip X
fn fixup_vec(vec: &Vec3) -> Vec3 {
    let mut new_vec = vec.clone();
    new_vec.x = -new_vec.x;
    return new_vec;
}
