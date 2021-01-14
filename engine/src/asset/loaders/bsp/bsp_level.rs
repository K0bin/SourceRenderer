
use sourcerenderer_core::{Platform, Quaternion, Vec4, Vec2I};
use crate::asset::{AssetLoader, AssetType, Asset, Mesh, Model, AssetManager};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use sourcerenderer_bsp::{Map, Node, Leaf, SurfaceEdge, LeafBrush, LeafFace, Vertex, Face, Edge,
                         Plane, TextureData, TextureInfo, TextureStringData, TextureDataStringTable,
                         BrushModel, DispVert, DispTri, DispInfo, NeighborEdge, DispSubNeighbor,
                         SurfaceFlags};
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
use super::BspLumps;
use std::cell::RefCell;

// REFERENCE
// https://github.com/lewa-j/Unity-Source-Tools/blob/1c5dc0635cdc4c65775d4af2c4449be49639f46b/Assets/Code/Read/SourceBSPLoader.cs#L877
// https://github.com/Metapyziks/VBspViewer/blob/master/Assets/VBspViewer/Scripts/Importing/VBsp/VBspFile.cs#L499
// https://github.com/Metapyziks/SourceUtils/blob/master/SourceUtils.WebExport/Bsp/Geometry.cs
// http://web.archive.org/web/20050426034532/http://www.geocities.com/cofrdrbob/bspformat.html
// https://github.com/toji/webgl-source/blob/a435841a856bb3d43f9783d3d2e7ac1cb63992a5/js/source-bsp.js
// https://github.com/Galaco/kero/blob/dev/LICENSE

pub struct BspLevelLoader {
  map_name_regex: Regex
}

const SCALING_FACTOR: f32 = 0.0236f32;

impl BspLevelLoader {
  pub fn new() -> Self {
    Self {
      map_name_regex: Regex::new(CSGO_MAP_NAME_PATTERN).unwrap()
    }
  }

  fn build_face(&self, temp: &BspLumps, face: &Face, brush_vertices: &mut Vec<crate::Vertex>, brush_indices: &mut HashMap<String, Vec<u32>>) {
    let tex_info = &temp.tex_info[face.texture_info as usize];
    let ignore_flags = SurfaceFlags::NODRAW | SurfaceFlags::LIGHT | SurfaceFlags::SKY | SurfaceFlags::SKY2D | SurfaceFlags::TRIGGER;
    if tex_info.flags.intersects(ignore_flags) {
      return;
    }

    let tex_data = &temp.tex_data[tex_info.texture_data as usize];
    let tex_offset = &temp.tex_data_string_table[tex_data.name_string_table_id as usize];
    let tex_name = temp.tex_string_data.get_string_at(tex_offset.0 as u32).to_str().unwrap().replace('\\', "/").to_lowercase();

    let material_brush_indices = &mut brush_indices.entry(tex_name.clone()).or_default();
    let plane = &temp.planes[face.plane_index as usize];
    let root_vertex = brush_vertices.len() as u32;

    for surf_edge_index in face.first_edge ..face.first_edge  + face.edges_count as i32 {
      let edge_index = temp.surface_edges[surf_edge_index as usize].index;
      let edge = temp.edges[edge_index.abs() as usize];

      // Push the two vertices of the first edge
      let vert_index = edge.vertex_index[if edge_index >= 0 { 0 } else { 1 }];
      let position = temp.vertices[vert_index as usize].position;
      brush_vertices.push(crate::Vertex {
        position: BspLevelLoader::fixup_position(&position),
        normal: BspLevelLoader::fixup_normal(&plane.normal),
        color: Vec3::new(1.0f32, 1.0f32, 1.0f32),
        uv: BspLevelLoader::calculate_uv(&position, &tex_info.texture_vecs_s, &tex_info.texture_vecs_t, &tex_data)
      });

      if surf_edge_index < face.first_edge + 2 {
        continue;
      }
      material_brush_indices.push(root_vertex);
      material_brush_indices.push(brush_vertices.len() as u32 - 1);
      material_brush_indices.push(brush_vertices.len() as u32 - 2);
    }
  }

  fn build_displacement_face(&self,
                             temp: &BspLumps,
                             disp_info: &DispInfo,
                             brush_vertices: &mut Vec<crate::Vertex>,
                             brush_indices: &mut HashMap<String, Vec<u32>>) {
    let face = &temp.faces[disp_info.map_face as usize];
    let tex_info = &temp.tex_info[face.texture_info as usize];
    let ignore_flags = SurfaceFlags::NODRAW | SurfaceFlags::LIGHT | SurfaceFlags::SKY | SurfaceFlags::SKY2D | SurfaceFlags::TRIGGER;
    if tex_info.flags.intersects(ignore_flags) {
      return;
    }

    let tex_data = &temp.tex_data[tex_info.texture_data as usize];
    let tex_offset = &temp.tex_data_string_table[tex_data.name_string_table_id as usize];
    let tex_name = temp.tex_string_data.get_string_at(tex_offset.0 as u32).to_str().unwrap().replace('\\', "/").to_lowercase();
    let plane = &temp.planes[face.plane_index as usize];
    let material_brush_indices = &mut brush_indices.entry(tex_name.clone()).or_default();

    let mut corners = [Vec3::default(); 4];
    let mut corners_uv = [Vec2::default(); 4];
    let mut first_corner = 0;
    let mut first_corner_dist_squared = f32::MAX;
    for surf_edge_index in face.first_edge..face.first_edge + face.edges_count as i32 {

      let edge_index = temp.surface_edges[surf_edge_index as usize].index;
      let edge = temp.edges[edge_index.abs() as usize];
      let vert_index = edge.vertex_index[if edge_index >= 0 { 0 } else { 1 }];
      let position = temp.vertices[vert_index as usize].position;
      let index = (surf_edge_index - face.first_edge) as usize;
      corners[index] = position;
      corners_uv[index] = Self::calculate_uv(&position, &tex_info.texture_vecs_s, &tex_info.texture_vecs_t, &tex_data);

      let dist_squared = (disp_info.start_position - position).magnitude_squared();
      if dist_squared < first_corner_dist_squared {
        first_corner = surf_edge_index - face.first_edge;
        first_corner_dist_squared = dist_squared;
      }
    }

    let subdivisions = 1 << disp_info.power;
    let size = subdivisions + 1;
    let sub_div_mul = 1f32 / (subdivisions as f32);
    for y in 0..subdivisions {
      let old_len = brush_vertices.len() as u32;
      let v = [
        y as f32 * sub_div_mul,
        (y + 1) as f32 * sub_div_mul
      ];
      for x in 0..size {
        let u = x as f32 * sub_div_mul;

        brush_vertices.push(crate::Vertex {
          position: Self::calculate_disp_vert(disp_info.disp_vert_start, x, y, size, &corners, first_corner, &temp.disp_verts),
          normal: Self::fixup_normal(&plane.normal),
          color: Vec3::new(1.0f32, 1.0f32, 1.0f32),
          uv: (corners_uv[0] * (1f32 - u) + corners_uv[3] * u) * (1f32 - v[0]) + (corners_uv[1] * (1f32 - u) + corners_uv[2] * u) * v[0]
        });

        if brush_vertices.len() - old_len as usize >= 3 {
          material_brush_indices.push(brush_vertices.len() as u32 - 3);
          material_brush_indices.push(brush_vertices.len() as u32 - 1);
          material_brush_indices.push(brush_vertices.len() as u32 - 2);
        }

        brush_vertices.push(crate::Vertex {
          position: Self::calculate_disp_vert(disp_info.disp_vert_start, x, y + 1, size, &corners, first_corner, &temp.disp_verts),
          normal: Self::fixup_normal(&plane.normal),
          color: Vec3::new(1.0f32, 1.0f32, 1.0f32),
          uv: (corners_uv[0] * (1f32 - u) + corners_uv[3] * u) * (1f32 - v[1]) + (corners_uv[1] * (1f32 - u) + corners_uv[2] * u) * v[1]
        });

        if brush_vertices.len() - old_len as usize >= 3 {
          material_brush_indices.push(brush_vertices.len() as u32 - 3);
          material_brush_indices.push(brush_vertices.len() as u32 - 2);
          material_brush_indices.push(brush_vertices.len() as u32 - 1);
        }
      }
    }
  }

  fn calculate_disp_uv(x: i32, y: i32, size: i32, face: &Face) -> Vec2 {
    Vec2::new(0f32, 0f32)
  }

  fn calculate_disp_vert(offset: i32, x: i32, y: i32, size: i32, corners: &[Vec3; 4], first_corner: i32, disp_verts: &[DispVert]) -> Vec3 {
    let disp_vert = &disp_verts[(offset + x + y * size) as usize];
    let tx = (x as f32) / ((size - 1) as f32);
    let ty = (y as f32) / ((size - 1) as f32);
    let sx = 1f32 - tx;
    let sy = 1f32 - ty;

    let relevant_corners = [
      corners[((first_corner) & 3) as usize],
      corners[((first_corner + 1) & 3) as usize],
      corners[((first_corner + 2) & 3) as usize],
      corners[((first_corner + 3) & 3) as usize],
    ];
    let origin = ty * (sx * relevant_corners[1] + tx * relevant_corners[2]) + sy * (sx * relevant_corners[0] + tx * relevant_corners[3]);
    Self::fixup_position(&(origin + disp_vert.vec * disp_vert.dist))
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
    let disp_infos = map.read_disp_infos().unwrap();
    let disp_verts = map.read_disp_verts().unwrap();
    let disp_tris = map.read_disp_tris().unwrap();
    let mut pakfile = map.read_pakfile().unwrap();
    let lighting = map.read_lighting().unwrap();
    let visibility = map.read_visibility().unwrap();

    let temp = BspLumps {
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
      tex_data_string_table,
      disp_infos,
      disp_verts,
      disp_tris,
      lighting,
      visibility,
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

      for face in &temp.faces[model.first_face as usize .. (model.first_face + model.num_faces) as usize] {
        if face.displacement_info != -1 {
          let disp_info = &temp.disp_infos[face.displacement_info as usize];
          self.build_displacement_face(&temp, disp_info, &mut brush_vertices, &mut per_material_indices);
        } else {
          self.build_face(&temp, face, &mut brush_vertices, &mut per_material_indices);
        }
      }

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
