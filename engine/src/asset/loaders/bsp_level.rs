
use sourcerenderer_core::{Platform, Quaternion, Vec4};
use crate::asset::{AssetLoader, AssetType, Asset, Mesh, Model, AssetManager};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use sourcerenderer_bsp::{Map, Node, Leaf, SurfaceEdge, LeafBrush, LeafFace, Vertex, Face, Edge, Plane, TextureData, TextureInfo, TextureStringData, TextureDataStringTable, BrushModel};
use std::sync::Mutex;
use std::collections::HashMap;
use sourcerenderer_core::{Vec3, Vec2};
use crate::asset::asset_manager::{AssetLoaderResult, AssetFile, AssetFileData, MeshRange, LoadedAsset, AssetLoaderProgress};
use sourcerenderer_core::graphics::{Device, MemoryUsage, BufferUsage};
use legion::world::SubWorld;
use legion::{World, WorldOptions};
use crate::renderer::StaticRenderableComponent;
use crate::Transform;
use nalgebra::{UnitQuaternion, Unit};
use regex::Regex;
use crate::asset::loaders::csgo_loader::CSGO_MAP_NAME_PATTERN;
use std::io::BufReader;
use std::io::Cursor;
use std::collections::HashSet;
use crate::asset::loaders::vpk_container::new_vpk_container;
use crate::asset::loaders::PakFileContainer;

pub struct BspLevelLoader {
  map_name_regex: Regex
}

struct BspTemp {
  map: Map<BufReader<File>>,
  map_name: String,
  leafs: Vec<Leaf>,
  nodes: Vec<Node>,
  leaf_faces: Vec<LeafFace>,
  leaf_brushes: Vec<LeafBrush>,
  surface_edges: Vec<SurfaceEdge>,
  vertices: Vec<Vertex>,
  faces: Vec<Face>,
  edges: Vec<Edge>,
  planes: Vec<Plane>,
  tex_data: Vec<TextureData>,
  tex_info: Vec<TextureInfo>,
  tex_string_data: TextureStringData,
  tex_data_string_table: Vec<TextureDataStringTable>
}

const SCALING_FACTOR: f32 = 0.0236f32;

impl BspLevelLoader {
  pub fn new() -> Self {
    Self {
      map_name_regex: Regex::new(CSGO_MAP_NAME_PATTERN).unwrap()
    }
  }

  fn read_node(&self, node: &Node, temp: &BspTemp, brush_vertices: &mut Vec<crate::Vertex>, brush_indices: &mut HashMap<String, Vec<u32>>) {
    let left_child = node.children[0];
    self.read_child(left_child, temp, brush_vertices, brush_indices);
    let right_child = node.children[1];
    self.read_child(right_child, temp, brush_vertices, brush_indices);
  }

  fn read_child(&self, index: i32, temp: &BspTemp, brush_vertices: &mut Vec<crate::Vertex>, brush_indices: &mut HashMap<String, Vec<u32>>) {
    if index < 0 {
      self.read_leaf(&temp.leafs[(-1 - index) as usize], temp, brush_vertices, brush_indices);
    } else {
      self.read_node(&temp.nodes[index as usize], temp, brush_vertices, brush_indices);
    };
  }

  fn read_leaf(&self, leaf: &Leaf, temp: &BspTemp, brush_vertices: &mut Vec<crate::Vertex>, brush_indices: &mut HashMap<String, Vec<u32>>) {
    for leaf_face_index in leaf.first_leaf_face as u32 .. leaf.first_leaf_face as u32 + leaf.leaf_faces_count as u32 {
      let face_index = temp.leaf_faces[leaf_face_index as usize].index;
      let face = &temp.faces[face_index as usize];
      let plane = &temp.planes[face.plane_index as usize];
      let tex_info = &temp.tex_info[face.texture_info as usize];
      let tex_data = &temp.tex_data[tex_info.texture_data as usize];
      let tex_offset = &temp.tex_data_string_table[tex_data.name_string_table_id as usize];
      let tex_name = temp.tex_string_data.get_string_at(tex_offset.0 as u32).to_str().unwrap().replace('\\', "/").to_lowercase();

      let mut face_vertices: HashMap<u16, u32> = HashMap::new(); // Just to make sure that there's no duplicates
      let mut root_vertex = 0u16;
      for surf_edge_index in face.first_edge .. face.first_edge + face.edges_count as i32 {
        let edge_index = temp.surface_edges[surf_edge_index as usize].index;
        let edge = temp.edges[edge_index.abs() as usize];

        // Push the two vertices of the first edge
        if surf_edge_index == face.first_edge {
          if !face_vertices.contains_key(&edge.vertex_index[if edge_index > 0 { 0 } else { 1 }]) {
            root_vertex = edge.vertex_index[if edge_index > 0 { 0 } else { 1 }];
            let position = temp.vertices[root_vertex as usize].position;
            let mut vertex = crate::Vertex {
              position: BspLevelLoader::fixup_position(&position),
              normal: BspLevelLoader::fixup_normal(&plane.normal),
              color: Vec3::new(1.0f32, 1.0f32, 1.0f32),
              uv: BspLevelLoader::calculate_uv(&position, &tex_info.texture_vecs_s, &tex_info.texture_vecs_t, &tex_data)
            };
            face_vertices.insert(root_vertex, brush_vertices.len() as u32);
            brush_vertices.push(vertex);
          }
          continue;
        }

        // Edge must not be connected to the root vertex
        if edge.vertex_index[0] == root_vertex || edge.vertex_index[1] == root_vertex {
          continue;
        }

        // Edge is on opposite side of the first edge => push the vertices
        for i in 0..2 {
          if !face_vertices.contains_key(&edge.vertex_index[i]) {
            let position = temp.vertices[edge.vertex_index[i] as usize].position;
            let vertex = crate::Vertex {
              position: BspLevelLoader::fixup_position(&position),
              normal: BspLevelLoader::fixup_normal(&plane.normal),
              color: Vec3::new(1.0f32, 1.0f32, 1.0f32),
              uv: BspLevelLoader::calculate_uv(&position, &tex_info.texture_vecs_s, &tex_info.texture_vecs_t, &tex_data)
            };
            face_vertices.insert(edge.vertex_index[i], brush_vertices.len() as u32);
            brush_vertices.push(vertex);
          }
        }

        // Push indices
        let material_brush_indices = &mut brush_indices.entry(tex_name.clone()).or_default();
        material_brush_indices.push(face_vertices[&root_vertex]);
        if edge_index < 0 {
          material_brush_indices.push(face_vertices[&edge.vertex_index[0]]);
          material_brush_indices.push(face_vertices[&edge.vertex_index[1]]);
        } else {
          material_brush_indices.push(face_vertices[&edge.vertex_index[1]]);
          material_brush_indices.push(face_vertices[&edge.vertex_index[0]]);
        }
      }
    }
  }

  fn calculate_uv(position: &Vec3, texture_vecs_s: &Vec4, texture_vecs_t: &Vec4, tex_data: &TextureData) -> Vec2 {
    let pos4 = Vec4::new(position.x, position.y, position.z, 1.0f32);
    Vec2::new(
      pos4.dot(texture_vecs_s) / tex_data.width as f32,
      pos4.dot(texture_vecs_t) / tex_data.height as f32
    )
  }

  fn fixup_position(position: &Vec3) -> Vec3 {
    Vec3::new(position.x, position.z, -position.y) * SCALING_FACTOR
  }

  fn fixup_normal(normal: &Vec3) -> Vec3 {
    Vec3::new(-normal.x, -normal.z, normal.y)
  }
}

impl<P: Platform> AssetLoader<P> for BspLevelLoader {
  fn matches(&self, file: &mut AssetFile) -> bool {
    let file_name = Path::new(&file.path).file_name();
    file_name.and_then(|file_name| file_name.to_str()).map_or(false, |file_name| self.map_name_regex.is_match(file_name))
  }

  fn load(&self, asset_file: AssetFile, manager: &AssetManager<P>, progress: &Arc<AssetLoaderProgress>) -> Result<AssetLoaderResult, ()> {
    let name = Path::new(&asset_file.path).file_name().unwrap().to_str().unwrap();
    let path = asset_file.path.clone();
    let file = match asset_file.data {
      AssetFileData::File(file) => file,
      _ => unreachable!("hi")
    };
    let buf_reader = BufReader::new(file);
    let mut map = Map::read(name, buf_reader).unwrap();
    let leafs = map.read_leafs().unwrap();
    let nodes = map.read_nodes().unwrap();
    let faces = map.read_faces().unwrap();
    let leaf_faces = map.read_leaf_faces().unwrap();
    let leaf_brushes = map.read_leaf_brushes().unwrap();
    let edges = map.read_edges().unwrap();
    let surface_edges = map.read_surface_edges().unwrap();
    let vertices = map.read_vertices().unwrap();
    let planes = map.read_planes().unwrap();
    let tex_data = map.read_texture_data().unwrap();
    let tex_info = map.read_texture_info().unwrap();
    let tex_string_data = map.read_texture_string_data().unwrap();
    let tex_data_string_table = map.read_texture_data_string_table().unwrap();
    let brush_models = map.read_brush_models().unwrap();
    let mut pakfile = map.read_pakfile().unwrap();

    let temp = BspTemp {
      map,
      map_name: name.to_string(),
      nodes,
      leafs,
      leaf_brushes,
      leaf_faces,
      surface_edges,
      vertices,
      faces,
      edges,
      planes,
      tex_data,
      tex_info,
      tex_string_data,
      tex_data_string_table
    };

    let pakfile_container = Box::new(PakFileContainer::new(pakfile));

    let mut brush_vertices = Vec::<crate::Vertex>::new();
    let mut brush_indices = Vec::<u32>::new();
    let mut mesh_ranges = Vec::<MeshRange>::new();

    let mut per_material_indices = HashMap::<String, Vec<u32>>::new();
    let mut per_model_range_offsets = Vec::<(usize, usize)>::new();
    let mut per_model_materials = Vec::<Vec<String>>::new();
    let mut materials_to_load = HashSet::<String>::new();

    for model in &brush_models {
      let root = &temp.nodes[model.head_node as usize];
      self.read_node(root, &temp, &mut brush_vertices, &mut per_material_indices);
      let mut materials = Vec::<String>::new();
      let ranges_start = mesh_ranges.len();
      'materials: for (material, indices) in per_material_indices.drain() {
        if indices.is_empty() {
          continue 'materials;
        }

        let material_path = "materials/".to_string() + material.as_str() + ".vmt";
        materials_to_load.insert(material_path.clone());

        let offset = brush_indices.len();
        brush_indices.extend_from_slice(&indices);
        let count = brush_indices.len() - offset;

        materials.push(material_path);
        mesh_ranges.push(MeshRange {
          start: offset as u32,
          count: count as u32
        });
      }
      per_model_materials.push(materials);
      per_model_range_offsets.push((ranges_start, mesh_ranges.len() - ranges_start));
    }

    let vertex_buffer_temp = manager.graphics_device().upload_data_slice(&brush_vertices, MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
    let index_buffer_temp = manager.graphics_device().upload_data_slice(&brush_indices, MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
    let vertex_buffer = manager.graphics_device().create_buffer(std::mem::size_of::<crate::Vertex>() * brush_vertices.len(), MemoryUsage::GpuOnly, BufferUsage::COPY_DST | BufferUsage::VERTEX);
    let index_buffer = manager.graphics_device().create_buffer(std::mem::size_of::<u32>() * brush_indices.len(), MemoryUsage::GpuOnly, BufferUsage::COPY_DST | BufferUsage::INDEX);
    manager.graphics_device().init_buffer(&vertex_buffer_temp, &vertex_buffer);
    manager.graphics_device().init_buffer(&index_buffer_temp, &index_buffer);

    let mut world = World::new(WorldOptions::default());
    for (index, (ranges_start, ranges_count)) in per_model_range_offsets.iter().enumerate() {
      let mesh = Arc::new(Mesh {
        vertices: vertex_buffer.clone(),
        indices: Some(index_buffer.clone()),
        parts: mesh_ranges[*ranges_start .. *ranges_start + ranges_count].to_vec()
      });
      let mesh_name = format!("brushes_mesh_{}", index);

      manager.add_asset(&mesh_name, Asset::Mesh(mesh));

      let model_name = format!("brushes_model_{}", index);
      let model = Arc::new(Model {
        mesh_path: mesh_name,
        material_paths: per_model_materials[index].clone()
      });
      manager.add_asset(&model_name, Asset::Model(model));

      world.push(
        (StaticRenderableComponent {
          model_path: model_name,
          receive_shadows: true,
          cast_shadows: true,
          can_move: false
        },
         Transform {
           position: brush_models[index].origin,
           scale: Vec3::new(1.0f32, 1.0f32, 1.0f32),
           rotation: Quaternion::identity(),
         })
      );
    }

    for material in materials_to_load {
      manager.request_asset_with_progress(&material, AssetType::Material, Some(progress));
    }

    manager.add_container(pakfile_container);

    Ok(AssetLoaderResult {
      level: Some(world)
    })
  }
}
