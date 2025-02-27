use std::borrow::Borrow;
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::io::SeekFrom;
use std::sync::Arc;
use std::{
    slice,
    usize,
};

use bevy_math::{EulerRot, Quat};
use bevy_tasks::futures_lite::io::Cursor as AsyncCursor;
use bevy_tasks::futures_lite::{AsyncReadExt, AsyncSeekExt};
use std::io::{Cursor, Seek as _, Read as _};
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
    Vec2,
    Vec3,
    Vec4,
};

use crate::asset::asset_manager::AssetFile;
use crate::asset::loaded_level::{LoadedEntityParent, LevelData};
use crate::asset::{
    AssetData, AssetLoadPriority, AssetLoader, AssetLoaderProgress, AssetManager, AssetType, MeshData, MeshRange, ModelData, Vertex, FixedByteSizeCache
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

    async fn visit_node<P: Platform>(
        node: &Node<'_>,
        world: &mut LevelData,
        asset_mgr: &Arc<AssetManager<P>>,
        parent_entity: Option<usize>,
        gltf_file_name: &str,
        buffer_cache: &mut FixedByteSizeCache<String, Box<[u8]>>,
    ) {
        let (translation, rotation, scale) = match node.transform() {
            gltf::scene::Transform::Matrix {
                matrix: _columns_data,
            } => {
                //unimplemented!()

                /*let mut matrix = Matrix4::default();
                for i in 0..matrix.len() {
                  let column_slice = &columns_data[0];
                  matrix.column_mut(i).copy_from_slice(column_slice);
                }
                matrix*/
                (
                    Vec3::new(0.0f32, 0.0f32, 0.0f32),
                    Vec4::new(0.0f32, 0.0f32, 0.0f32, 0.0f32),
                    Vec3::new(1.0f32, 1.0f32, 1.0f32),
                )
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
                    &model_name,
                    &primitive,
                    asset_mgr,
                    &mut vertices,
                    &mut indices,
                    gltf_file_name,
                    buffer_cache,
                ).await;
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

            asset_mgr.add_asset_data(
                &mesh_path,
                AssetData::Mesh(MeshData {
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
            asset_mgr.add_asset_data(
                &model_path,
                AssetData::Model(ModelData {
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
            log::warn!(
                "WARNING: skins are not supported. Node name: {:?}",
                node.name()
            );
        }
        if node.camera().is_some() {
            log::warn!(
                "WARNING: cameras are not supported. Node name: {:?}",
                node.name()
            );
        }
        if node.weights().is_some() {
            log::warn!(
                "WARNING: weights are not supported. Node name: {:?}",
                node.name()
            );
        }

        if let Some(light) = node.light() {
            {
                let transform: &mut Transform = world.get_component_mut(entity).unwrap();
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
            Box::pin(GltfLoader::visit_node(
                &child,
                world,
                asset_mgr,
                Some(entity),
                gltf_file_name,
                buffer_cache,
            )).await;
        }
    }

    async fn load_scene<P: Platform>(
        scene: &Scene<'_>,
        asset_mgr: &Arc<AssetManager<P>>,
        gltf_file_name: &str,
    ) -> LevelData {
        let mut world: LevelData = LevelData::new(4096, 64);
        let mut buffer_cache = FixedByteSizeCache::<String, Box<[u8]>>::new(128 << 20, 3);
        let nodes = scene.nodes();
        for node in nodes {
            GltfLoader::visit_node(
                &node,
                &mut world,
                asset_mgr,
                None,
                gltf_file_name,
                &mut buffer_cache,
            ).await;
        }
        world
    }

    async fn load_primitive<'a, P: Platform>(
        _model_name: &'a str,
        primitive: &'a Primitive<'a>,
        asset_mgr: &'a Arc<AssetManager<P>>,
        vertices: &'a mut Vec<Vertex>,
        indices: &'a mut Vec<u32>,
        gltf_file_name: &'a str,
        buffer_cache: &'a mut FixedByteSizeCache<String, Box<[u8]>>,
    ) {
        async fn load_buffer<'a, P: Platform>(
            gltf_file_name: &str,
            gltf_path: &str,
            asset_mgr: &Arc<AssetManager<P>>,
            buffer_cache: &'a mut FixedByteSizeCache<String, Box<[u8]>>,
            view: &View<'_>,
        ) {
            let uri: String;
            match view.buffer().source() {
                Source::Bin => {
                    uri = format!(
                        "{}/buffer/{}-{}",
                        gltf_file_name,
                        view.offset(),
                        view.length()
                    );
                }
                Source::Uri(gltf_uri) => {
                    uri = if let Some(last_slash_pos) = gltf_path.find('/') {
                        format!("{}/{}", &gltf_path[..last_slash_pos], &gltf_uri)
                    } else {
                        gltf_uri.to_string()
                    };
                }
            }

            if !buffer_cache.contains_key(&uri) {
                let file = asset_mgr.load_file(&uri).await.expect("Failed to load buffer");
                let file_data = file.data.into_inner();
                buffer_cache.insert(uri.clone(), file_data);
            }
        }

        fn load_buffer_from_cache<'a>(
            gltf_file_name: &str,
            gltf_path: &str,
            buffer_cache: &'a FixedByteSizeCache<String, Box<[u8]>>,
            view: &View<'_>,
        ) -> &'a [u8] {
            let uri: String;
            let offset: usize;
            match view.buffer().source() {
                Source::Bin => {
                    offset = 0;
                    uri = format!(
                        "{}/buffer/{}-{}",
                        gltf_file_name,
                        view.offset(),
                        view.length()
                    );
                }
                Source::Uri(gltf_uri) => {
                    offset = view.offset();
                    uri = if let Some(last_slash_pos) = gltf_path.find('/') {
                        format!("{}/{}", &gltf_path[..last_slash_pos], &gltf_uri)
                    } else {
                        gltf_uri.to_string()
                    };
                }
            }

            let file = buffer_cache.get(&uri).unwrap();
            &file[offset..(offset + view.length())]
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
            load_buffer(
                gltf_file_name,
                gltf_path,
                asset_mgr,
                buffer_cache,
                &positions_view,
            ).await;

            let normals = primitive.get(&Semantic::Normals).unwrap();
            assert!(normals.sparse().is_none());
            let normals_view = normals.view().unwrap();
            load_buffer(
                gltf_file_name,
                gltf_path,
                asset_mgr,
                buffer_cache,
                &normals_view,
            ).await;

            let texcoords = primitive.get(&Semantic::TexCoords(0)).unwrap();
            assert!(texcoords.sparse().is_none());
            let texcoords_view = texcoords.view().unwrap();
            load_buffer(
                gltf_file_name,
                gltf_path,
                asset_mgr,
                buffer_cache,
                &texcoords_view,
            ).await;


            let positions_data = load_buffer_from_cache(gltf_file_name, gltf_path, buffer_cache, &positions_view);
            let mut positions_buffer_cursor = Cursor::new(positions_data);
            let positions_stride = if let Some(stride) = positions_view.stride() {
                stride
            } else {
                positions.size()
            };

            let normals_data = load_buffer_from_cache(gltf_file_name, gltf_path, buffer_cache, &normals_view);
            let mut normals_buffer_cursor = Cursor::new(normals_data);
            let normals_stride = if let Some(stride) = normals_view.stride() {
                stride
            } else {
                normals.size()
            };

            let texcoords_data = load_buffer_from_cache(gltf_file_name, gltf_path, buffer_cache, &texcoords_view);
            let mut texcoords_buffer_cursor = Cursor::new(texcoords_data);
            let texcoords_stride = if let Some(stride) = texcoords_view.stride() {
                stride
            } else {
                texcoords.size()
            };

            positions_buffer_cursor
                .seek(SeekFrom::Start(positions.offset() as u64))
                .unwrap();
            normals_buffer_cursor
                .seek(SeekFrom::Start(normals.offset() as u64))
                .unwrap();
            texcoords_buffer_cursor
                .seek(SeekFrom::Start(texcoords.offset() as u64))
                .unwrap();

            assert_eq!(positions.count(), normals.count());
            for i in 0..positions.count() {
                positions_buffer_cursor
                    .seek(SeekFrom::Start(
                        positions.offset() as u64 + (i * positions_stride) as u64,
                    ))
                    .unwrap();
                let mut position_data = [0u8; 12];
                assert!(positions.size() <= position_data.len());
                assert_eq!(positions.size(), std::mem::size_of::<Vec3>());
                positions_buffer_cursor
                    .read_exact(&mut position_data[..positions.size()])
                    .unwrap();

                normals_buffer_cursor
                    .seek(SeekFrom::Start(
                        normals.offset() as u64 + (i * normals_stride) as u64,
                    ))
                    .unwrap();
                let mut normal_data = [0u8; 12];
                assert!(normals.size() <= normal_data.len());
                assert_eq!(normals.size(), std::mem::size_of::<Vec3>());
                normals_buffer_cursor.read_exact(&mut normal_data[..normals.size()])
                    .unwrap();

                texcoords_buffer_cursor
                    .seek(SeekFrom::Start(
                        texcoords.offset() as u64 + (i * texcoords_stride) as u64,
                    ))
                    .unwrap();
                let mut texcoords_data = [0u8; 8];
                assert!(texcoords.size() <= texcoords_data.len());
                assert_eq!(texcoords.size(), std::mem::size_of::<Vec2>());
                texcoords_buffer_cursor
                    .read_exact(&mut texcoords_data[..texcoords.size()])
                    .unwrap();

                let position_raw: Vec3 = unsafe { std::mem::transmute_copy(&position_data) };
                let normal_raw: Vec3 = unsafe { std::mem::transmute_copy(&normal_data) };
                let position = fixup_vec(&position_raw);
                let normal = fixup_vec(&normal_raw).normalize();
                let tex_coord: Vec2 = unsafe { std::mem::transmute_copy(&texcoords_data) };
                assert_eq!(std::mem::size_of::<Vertex>(), 36);
                vertices.push(Vertex {
                    position,
                    normal,
                    tex_coord,
                    color: [255, 255, 255, 255],
                });

                debug_assert!(
                    positions_buffer_cursor.seek(SeekFrom::Current(0)).unwrap()
                        <= (positions_view.offset() + positions_view.length()) as u64
                );
                debug_assert!(
                    normals_buffer_cursor.seek(SeekFrom::Current(0)).unwrap()
                        <= (normals_view.offset() + normals_view.length()) as u64
                );
                debug_assert!(
                    texcoords_buffer_cursor.seek(SeekFrom::Current(0)).unwrap()
                        <= (texcoords_view.offset() + texcoords_view.length()) as u64
                );
            }
        }

        let indices_accessor = primitive.indices();
        if let Some(indices_accessor) = indices_accessor {
            assert!(indices_accessor.sparse().is_none());
            let view = indices_accessor.view().unwrap();
            load_buffer(
                gltf_file_name,
                gltf_path,
                asset_mgr,
                buffer_cache,
                &view,
            ).await;

            let data = load_buffer_from_cache(gltf_file_name, gltf_path, buffer_cache, &view);
            let mut buffer_cursor = Cursor::new(&data);
            buffer_cursor
                .seek(SeekFrom::Start(indices_accessor.offset() as u64))
                .unwrap();

            for _ in 0..indices_accessor.count() {
                let start = buffer_cursor.seek(SeekFrom::Current(0)).unwrap();

                let mut attr_data = [0u8; 8];
                assert!(indices_accessor.size() <= attr_data.len());
                buffer_cursor.read_exact(&mut attr_data[..indices_accessor.size()]).unwrap();
                assert!(indices_accessor.size() <= std::mem::size_of::<u32>());

                if indices_accessor.size() == 4 {
                    let index: u32 = unsafe { std::mem::transmute_copy(&attr_data) };
                    indices.push(index + index_base);
                } else if indices_accessor.size() == 2 {
                    let index: u16 = unsafe { std::mem::transmute_copy(&attr_data) };
                    indices.push(index as u32 + index_base);
                } else {
                    unimplemented!();
                }

                if let Some(stride) = view.stride() {
                    assert!(stride > indices_accessor.size());
                    buffer_cursor
                        .seek(SeekFrom::Start(start + stride as u64))
                        .unwrap();
                }
            }
            assert!(
                buffer_cursor.seek(SeekFrom::Current(0)).unwrap()
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
                    } => {
                        if let Some(last_slash_pos) = gltf_path.find('/') {
                            format!("{}/{}", &gltf_path[..last_slash_pos], &uri)
                        } else {
                            uri.to_string()
                        }
                    },
                }
            });

        if let Some(albedo_path) = albedo_path {
            asset_mgr.request_asset(&albedo_path, AssetType::Material, AssetLoadPriority::Low);
            asset_mgr.add_material_data(
                &material_path,
                &albedo_path,
                pbr.roughness_factor(),
                pbr.metallic_factor(),
            );
        } else {
            let color = pbr.base_color_factor();
            asset_mgr.add_material_data_color(
                &material_path,
                Vec4::new(color[0], color[1], color[2], color[3]),
                pbr.roughness_factor(),
                pbr.metallic_factor(),
            );
        }
        material_path
    }
}

impl<P: Platform> AssetLoader<P> for GltfLoader {
    fn matches(&self, file: &mut AssetFile) -> bool {
        (file.path.contains("gltf") || file.path.contains("glb"))
            && file.path.contains("/scene/")
            && Gltf::from_reader(file).is_ok()
    }

    async fn load(
        &self,
        file: AssetFile,
        manager: &Arc<AssetManager<P>>,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<(), ()> {
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
        let scene_name_start_opt = path.find(scene_prefix);
        if scene_name_start_opt.is_none() {
            return Ok(());
        }
        let scene_name_start = scene_name_start_opt.unwrap();
        let gltf_name = &path[0..scene_name_start];

        for scene in gltf.scenes() {
            let scene_name_or_fallback: String;
            if let Some(scene_name) = scene.name() {
                scene_name_or_fallback = format!("{}/scene/{}", gltf_name, scene_name);
            } else if gltf.scenes().len() > 1 || scene_name_start + scene_prefix.len() < path.len() {
                scene_name_or_fallback = format!("{}/scene/{}", gltf_name, scene.index());
            } else {
                scene_name_or_fallback = format!("{}/scene/", gltf_name);
            }

            if &path == &scene_name_or_fallback {
                let world = GltfLoader::load_scene(&scene, manager, gltf_name).await;
                manager.add_asset_data_with_progress(&scene_name_or_fallback, AssetData::Level(world), Some(progress), priority);
            }
        }
        Ok(())
    }
}

// glTF uses a right-handed coordinate system. glTF defines +Y as up, +Z as forward, and -X as right; the front of a glTF asset faces +Z.
// We use a left-handed coordinate system with +Y as up, +Z as forward and +X as right. => flip X
fn fixup_vec(vec: &Vec3) -> Vec3 {
    let mut new_vec = vec.clone();
    new_vec.x = -new_vec.x;
    return new_vec;
}
