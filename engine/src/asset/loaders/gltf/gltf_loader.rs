use std::{collections::HashMap, io::{Cursor, Read, Seek, SeekFrom}, slice, sync::Arc, usize};

use gltf::{Gltf, Material, Node, Primitive, Scene, Semantic, buffer::Source};
use legion::{Entity, World, WorldOptions};
use nalgebra::UnitQuaternion;
use sourcerenderer_core::{Platform, Vec2, Vec3, Vec4};

use crate::{Parent, Transform, asset::{Asset, AssetLoadPriority, AssetLoader, AssetLoaderProgress, AssetManager, Mesh, MeshRange, Model, asset_manager::{AssetFile, AssetLoaderResult}, loaders::BspVertex as Vertex}, math::BoundingBox, renderer::{PointLightComponent, DirectionalLightComponent, StaticRenderableComponent}};

pub struct GltfLoader {}

impl GltfLoader {
  pub fn new() -> Self {
    Self {}
  }

  fn visit_node<P: Platform>(node: &Node, world: &mut World, asset_mgr: &AssetManager<P>, parent_entity: Option<Entity>, gltf_file_name: &str, buffer_cache: &mut HashMap<usize, Vec<u8>>) {
    let (translation, _rotation, scale) = match node.transform() {
      gltf::scene::Transform::Matrix { matrix: _columns_data } => {
        unimplemented!()

        /*let mut matrix = Matrix4::default();
        for i in 0..matrix.len() {
          let column_slice = &columns_data[0];
          matrix.column_mut(i).copy_from_slice(column_slice);
        }
        matrix*/
      },
      gltf::scene::Transform::Decomposed { translation, rotation, scale } =>
        (Vec3::new(translation[0], translation[1], translation[2]),
        Vec4::new(rotation[0], rotation[1], rotation[2], rotation[3]),
        Vec3::new(scale[0], scale[1], scale[2])),
    };
    let entity = world.push((Transform {
      position: translation,
      scale,
      rotation: UnitQuaternion::identity(),
    },));

    {
      let mut entry = world.entry(entity).unwrap();
      if let Some(parent) = parent_entity {
        entry.add_component(Parent(parent));
      }
    }

    if let Some(mesh) = node.mesh() {
      let model_name = node.name().map_or_else(|| node.index().to_string(), |name| name.to_string());
      let mesh_path = gltf_file_name.to_string() + "/mesh/" + &model_name;
      let material_path = gltf_file_name.to_string() + "/material/" + &model_name;

      let mut indices = Vec::<u32>::new();
      let mut vertices = Vec::<Vertex>::new();
      let mut parts = Vec::<MeshRange>::with_capacity(mesh.primitives().len());
      let mut bounding_box = Option::<BoundingBox>::None;
      for primitive in mesh.primitives() {
        let part_start = indices.len();
        GltfLoader::load_primitive(&primitive, asset_mgr, &mut vertices, &mut indices, gltf_file_name, buffer_cache);
        GltfLoader::load_material(&primitive.material(), asset_mgr, &material_path);
        let primitive_bounding_box = primitive.bounding_box();
        if let Some(bounding_box) = &mut bounding_box {
          bounding_box.min.x = f32::min(bounding_box.min.x, primitive_bounding_box.min[0]);
          bounding_box.min.y = f32::min(bounding_box.min.y, primitive_bounding_box.min[1]);
          bounding_box.min.z = f32::min(bounding_box.min.z, primitive_bounding_box.min[2]);
          bounding_box.max.x = f32::max(bounding_box.max.x, primitive_bounding_box.max[0]);
          bounding_box.max.y = f32::max(bounding_box.max.y, primitive_bounding_box.max[1]);
          bounding_box.max.z = f32::max(bounding_box.max.z, primitive_bounding_box.max[2]);
        } else {
          bounding_box = Some(BoundingBox {
            min: Vec3::new(primitive_bounding_box.min[0], primitive_bounding_box.min[1], primitive_bounding_box.min[2]),
            max: Vec3::new(primitive_bounding_box.max[0], primitive_bounding_box.max[1], primitive_bounding_box.max[2]),
          });
        }
        let range = MeshRange {
          start: part_start as u32,
          count: (indices.len() - part_start) as u32
        };
        parts.push(range);
      }

      let vertices_count = vertices.len();
      let vertices_box = vertices.into_boxed_slice();
      let ptr = Box::into_raw(vertices_box);
      let data_ptr = unsafe { slice::from_raw_parts_mut(ptr as *mut u8, vertices_count * std::mem::size_of::<Vertex>()) as *mut [u8] };
      let vertices_data = unsafe { Box::from_raw(data_ptr) };

      let indices_count = indices.len();
      let indices_box = indices.into_boxed_slice();
      let ptr = Box::into_raw(indices_box);
      let data_ptr = unsafe { slice::from_raw_parts_mut(ptr as *mut u8, indices_count * std::mem::size_of::<u32>()) as *mut [u8] };
      let indices_data = unsafe { Box::from_raw(data_ptr) };

      let parts_len = parts.len();

      asset_mgr.add_asset(&mesh_path, Asset::Mesh(Mesh {
        indices: (indices_count > 0).then(|| indices_data),
        vertices: vertices_data,
        bounding_box: bounding_box,
        parts: parts.into_boxed_slice()
      }), AssetLoadPriority::Normal);

      let model_path = gltf_file_name.to_string() + "/model/" + &model_name;
      asset_mgr.add_asset(&model_path, Asset::Model(Model {
        mesh_path: mesh_path.clone(),
        material_paths: vec![material_path; parts_len],
      }), AssetLoadPriority::Normal);

      let mut entry = world.entry(entity).unwrap();
      entry.add_component(StaticRenderableComponent {
        model_path,
        receive_shadows: true,
        cast_shadows: true,
        can_move: false
      });
    };

    if node.skin().is_some() {
      println!("WARNING: skins are not supported. Node name: {:?}", node.name());
    }
    if node.camera().is_some() {
      println!("WARNING: cameras are not supported. Node name: {:?}", node.name());
    }
    if node.weights().is_some() {
      println!("WARNING: weights are not supported. Node name: {:?}", node.name());
    }

    if let Some(light) = node.light() {
      let mut entry = world.entry(entity).unwrap();
      match light.kind() {
        gltf::khr_lights_punctual::Kind::Directional => {
          entry.add_component(DirectionalLightComponent {
            intensity: light.intensity(),
          });
        },
        gltf::khr_lights_punctual::Kind::Point => {
          entry.add_component(PointLightComponent {
            intensity: light.intensity(),
          });
        },
        gltf::khr_lights_punctual::Kind::Spot { .. } => todo!(),
      }
    }

    for child in node.children() {
      GltfLoader::visit_node(&child, world, asset_mgr, Some(entity), gltf_file_name, buffer_cache);
    }
  }

  fn load_scene<P: Platform>(scene: &Scene, asset_mgr: &AssetManager<P>, gltf_file_name: &str) -> World {
    let mut world = World::new(WorldOptions::default());
    let nodes = scene.nodes();
    let mut buffer_cache = HashMap::<usize, Vec<u8>>::new();
    for node in nodes {
      GltfLoader::visit_node(&node, &mut world, asset_mgr, None, gltf_file_name, &mut buffer_cache);
    }
    world
  }

  fn load_primitive<P: Platform>(primitive: &Primitive, asset_mgr: &AssetManager<P>, vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>, gltf_file_name: &str, buffer_cache: &mut HashMap<usize, Vec<u8>>) {
    let index_base = vertices.len() as u32;

    {
      let positions = primitive.get(&Semantic::Positions).unwrap();
      assert!(positions.sparse().is_none());
      let positions_view = positions.view().unwrap();
      let positions_buffer = positions_view.buffer();
      match positions_buffer.source() {
        Source::Bin => {},
        Source::Uri(_) => unimplemented!(),
      }

      let normals = primitive.get(&Semantic::Normals).unwrap();
      assert!(normals.sparse().is_none());
      let normals_view = normals.view().unwrap();
      let normals_buffer = normals_view.buffer();
      match normals_buffer.source() {
        Source::Bin => {},
        Source::Uri(_) => unimplemented!(),
      }

      if !buffer_cache.contains_key(&positions_buffer.index()) {
        let url = format!("{}/buffer/{}", gltf_file_name, positions_buffer.index().to_string());
        println!("Loading: {}", url);
        let mut buffer_file = asset_mgr.load_file(&url).expect("Failed to load buffer");

        let mut data = vec![0u8; positions_buffer.length()];
        buffer_file.read_exact(&mut data).unwrap();
        buffer_cache.insert(positions_buffer.index(), data);
      }

      if !buffer_cache.contains_key(&normals_buffer.index()) {
        let url = format!("{}/buffer/{}", gltf_file_name, normals_buffer.index().to_string());
        println!("Loading: {}", url);
        let mut buffer_file = asset_mgr.load_file(&url).expect("Failed to load buffer");

        let mut data = vec![0u8; normals_buffer.length()];
        buffer_file.read_exact(&mut data).unwrap();
        buffer_cache.insert(normals_buffer.index(), data);
      }

      let positions_buffer_data = buffer_cache.get(&positions_buffer.index()).unwrap();
      let mut positions_buffer_cursor = Cursor::new(positions_buffer_data);
      positions_buffer_cursor.seek(SeekFrom::Start((positions_view.offset() + positions.offset()) as u64)).unwrap();

      let normals_buffer_data = buffer_cache.get(&normals_buffer.index()).unwrap();
      let mut normals_buffer_cursor = Cursor::new(normals_buffer_data);
      normals_buffer_cursor.seek(SeekFrom::Start((normals_view.offset() + normals.offset()) as u64)).unwrap();

      assert_eq!(positions.count(), normals.count());
      for _ in 0..positions.count() {
        let positions_start = positions_buffer_cursor.seek(SeekFrom::Current(0)).unwrap();
        let normals_start = normals_buffer_cursor.seek(SeekFrom::Current(0)).unwrap();

        let mut position_data = vec![0; positions.size()];
        positions_buffer_cursor.read_exact(&mut position_data).unwrap();
        assert_eq!(position_data.len(), std::mem::size_of::<Vec3>());

        let mut normal_data = vec![0; normals.size()];
        normals_buffer_cursor.read_exact(&mut normal_data).unwrap();
        assert_eq!(normal_data.len(), std::mem::size_of::<Vec3>());

        unsafe {
          let position_vec_ptr: *const Vec3 = std::mem::transmute(position_data.as_ptr());
          let normal_vec_ptr: *const Vec3 = std::mem::transmute(normal_data.as_ptr());
          let mut normal = *normal_vec_ptr;
          normal.normalize_mut();
          vertices.push(Vertex {
            position: *position_vec_ptr,
            normal,
            uv: Vec2::new(0f32, 0f32),
            lightmap_uv: Vec2::new(0f32, 0f32),
            alpha: 1.0f32
          });
        }

        if let Some(stride) = positions_view.stride() {
          assert!(stride >= positions.size());
          positions_buffer_cursor.seek(SeekFrom::Start(positions_start + stride as u64)).unwrap();
        }

        if let Some(stride) = normals_view.stride() {
          assert!(stride >= normals.size());
          normals_buffer_cursor.seek(SeekFrom::Start(normals_start + stride as u64)).unwrap();
        }

        assert!(positions_buffer_cursor.seek(SeekFrom::Current(0)).unwrap() <= (positions_view.offset() + positions_view.length()) as u64);
        assert!(normals_buffer_cursor.seek(SeekFrom::Current(0)).unwrap() <= (normals_view.offset() + normals_view.length()) as u64);
      }
    }

    let indices_accessor = primitive.indices();
    if let Some(indices_accessor) = indices_accessor {
      assert!(indices_accessor.sparse().is_none());
      let view = indices_accessor.view().unwrap();
      let buffer = view.buffer();


      if !buffer_cache.contains_key(&buffer.index()) {
        let url = format!("{}/buffer/{}", gltf_file_name, buffer.index().to_string());
        println!("Loading: {}", url);
        let mut buffer_file = asset_mgr.load_file(&url).expect("Failed to load buffer");

        let mut data = vec![0u8; buffer.length()];
        buffer_file.read_exact(&mut data).unwrap();
        buffer_cache.insert(buffer.index(), data);
      }

      let buffer_data = buffer_cache.get(&buffer.index()).unwrap();

      let mut buffer_cursor = Cursor::new(buffer_data);
      buffer_cursor.seek(SeekFrom::Start((view.offset() + indices_accessor.offset()) as u64)).unwrap();

      for _ in 0..indices_accessor.count() {
        let start = buffer_cursor.seek(SeekFrom::Current(0)).unwrap();

        let mut attr_data = vec![0; indices_accessor.size()];
        buffer_cursor.read_exact(&mut attr_data).unwrap();

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
          buffer_cursor.seek(SeekFrom::Start(start + stride as u64)).unwrap();
        }
      }
      assert!(buffer_cursor.seek(SeekFrom::Current(0)).unwrap() <= (view.offset() + view.length()) as u64);
    }
  }

  fn load_material<P: Platform>(material: &Material, asset_mgr: &AssetManager<P>, material_name: &str) {
    let pbr = material.pbr_metallic_roughness();
    let color = pbr.base_color_factor();
    asset_mgr.add_material_color(material_name, Vec4::new(color[0], color[1], color[2], color[3]), pbr.roughness_factor(), pbr.metallic_factor());

    /*let albedo = material.pbr_metallic_roughness().base_color_texture().unwrap();
    let albedo_source = albedo.texture().source().source();
    match albedo_source {
      gltf::image::Source::View { view, .. } => {

      },
      gltf::image::Source::Uri { .. } => unimplemented!(),
    }*/
    //unimplemented!()
  }
}

impl<P: Platform> AssetLoader<P> for GltfLoader {
  fn matches(&self, file: &mut AssetFile<P>) -> bool {
    Gltf::from_reader(file).is_ok()
  }

  fn load(&self, file: AssetFile<P>, manager: &Arc<AssetManager<P>>, _priority: AssetLoadPriority, _progress: &Arc<AssetLoaderProgress>) -> Result<AssetLoaderResult, ()> {
    let path = file.path.clone();
    let gltf = Gltf::from_reader(file).unwrap();

    let scene_prefix = "/scene/";
    let scene_name_start = path.find(scene_prefix);
    if let Some(scene_name_start) = scene_name_start {
      let gltf_name = &path[0..scene_name_start];
      let scene_name = &path[scene_name_start + scene_prefix.len() ..];
      for scene in gltf.scenes() {
        if scene.name().map_or_else(|| scene.index().to_string(), |name| name.to_string()) == scene_name {
          let world = GltfLoader::load_scene(&scene, manager, gltf_name);
          return Ok(AssetLoaderResult {
            level: Some(world),
          });
        }
      }
    }


    unimplemented!()
  }
}

