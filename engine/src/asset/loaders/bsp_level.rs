
use sourcerenderer_core::{Platform, Quaternion};
use crate::asset::{AssetLoader, AssetType, Asset, Mesh, Model};
use std::io::{BufReader};
use std::fs::File;
use std::path::Path;
use sourcerenderer_bsp::{Map, Node, Leaf, SurfaceEdge, LeafBrush, LeafFace, Vertex, Face, Edge};
use std::sync::Mutex;
use std::collections::HashMap;
use sourcerenderer_core::{Vec3, Vec2};
use crate::asset::asset_manager::{AssetLoaderResult, AssetFile, AssetFileData, AssetLoaderContext, MeshRange, LoadedAsset};
use sourcerenderer_core::graphics::{Device, MemoryUsage, BufferUsage};
use legion::world::SubWorld;
use legion::{World, WorldOptions};
use crate::renderer::StaticRenderableComponent;
use crate::Transform;
use nalgebra::{UnitQuaternion, Unit};

pub struct BspLevelLoader {
}

struct BspTemp {
  map: Map,
  map_name: String,
  leafs: Vec<Leaf>,
  nodes: Vec<Node>,
  leaf_faces: Vec<LeafFace>,
  leaf_brushes: Vec<LeafBrush>,
  surface_edges: Vec<SurfaceEdge>,
  vertices: Vec<Vertex>,
  faces: Vec<Face>,
  edges: Vec<Edge>
}

impl BspLevelLoader {
  pub fn new() -> Self {
    Self {}
  }

  fn read_node(&self, node: &Node, temp: &BspTemp, brush_vertices: &mut Vec<crate::Vertex>, brush_indices: &mut Vec<u32>) {
    let left_child = node.children[0];
    self.read_child(left_child, temp, brush_vertices, brush_indices);
    let right_child = node.children[1];
    self.read_child(right_child, temp, brush_vertices, brush_indices);
  }

  fn read_child(&self, index: i32, temp: &BspTemp, brush_vertices: &mut Vec<crate::Vertex>, brush_indices: &mut Vec<u32>) {
    if index < 0 {
      self.read_leaf(&temp.leafs[(-1 - index) as usize], temp, brush_vertices, brush_indices);
    } else {
      self.read_node(&temp.nodes[index as usize], temp, brush_vertices, brush_indices);
    };
  }

  fn read_leaf(&self, leaf: &Leaf, temp: &BspTemp, brush_vertices: &mut Vec<crate::Vertex>, brush_indices: &mut Vec<u32>) {
    for leaf_face_index in leaf.first_leaf_face as u32 .. leaf.first_leaf_face as u32 + leaf.leaf_faces_count as u32 {
      let face_index = temp.leaf_faces[leaf_face_index as usize].index;
      let face = &temp.faces[face_index as usize];

      let mut face_vertices: HashMap<u16, u32> = HashMap::new(); // Just to make sure that there's no duplicates
      let mut root_vertex = 0u16;
      for surf_edge_index in face.first_edge .. face.first_edge + face.edges_count as i32 {
        let edge_index = temp.surface_edges[surf_edge_index as usize].index;
        let edge = temp.edges[edge_index.abs() as usize];

        // Push the two vertices of the first edge
        if surf_edge_index == face.first_edge {
          if !face_vertices.contains_key(&edge.vertex_index[if edge_index > 0 { 0 } else { 1 }]) {
            root_vertex = edge.vertex_index[if edge_index > 0 { 0 } else { 1 }];
            let position = temp.vertices[root_vertex as usize].position.clone();
            let vertex = crate::Vertex {
              position,
              normal: Vec3::new(1.0f32, 0.0f32, 0.0f32),
              color: Vec3::new(1.0f32, 1.0f32, 1.0f32),
              uv: Vec2::new(0.0f32, 0.0f32)
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
              position,
              normal: Vec3::new(1.0f32, 0.0f32, 0.0f32),
              color: Vec3::new(1.0f32, 1.0f32, 1.0f32),
              uv: Vec2::new(0.0f32, 0.0f32)
            };
            face_vertices.insert(edge.vertex_index[i], brush_vertices.len() as u32);
            brush_vertices.push(vertex);
          }
        }

        // Push indices
        brush_indices.push(face_vertices[&root_vertex]);
        if edge_index < 0 {
          brush_indices.push(face_vertices[&edge.vertex_index[1]]);
          brush_indices.push(face_vertices[&edge.vertex_index[0]]);
        } else {
          brush_indices.push(face_vertices[&edge.vertex_index[0]]);
          brush_indices.push(face_vertices[&edge.vertex_index[1]]);
        }
      }
    }
  }
}

impl<P: Platform> AssetLoader<P> for BspLevelLoader {
  fn matches(&self, file: &AssetFile) -> bool {
    true
  }

  fn load(&self, asset_file: AssetFile, context: &AssetLoaderContext<P>) -> Result<AssetLoaderResult<P>, ()> {
    let name = Path::new(&asset_file.path).file_name().unwrap().to_str().unwrap();
    let file = match asset_file.data {
      AssetFileData::File(file) => file,
      _ => unreachable!()
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
      edges
    };

    let mut brush_vertices = Vec::<crate::Vertex>::new();
    let mut brush_indices = Vec::<u32>::new();

    let root = temp.nodes.first().unwrap();
    self.read_node(root, &temp, &mut brush_vertices, &mut brush_indices);

    let vertex_buffer_temp = context.graphics_device.upload_data_slice(&brush_vertices, MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
    let index_buffer_temp = context.graphics_device.upload_data_slice(&brush_indices, MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
    let vertex_buffer = context.graphics_device.create_buffer(std::mem::size_of::<crate::Vertex>() * brush_vertices.len(), MemoryUsage::GpuOnly, BufferUsage::COPY_DST | BufferUsage::VERTEX);
    let index_buffer = context.graphics_device.create_buffer(std::mem::size_of::<u32>() * brush_indices.len(), MemoryUsage::GpuOnly, BufferUsage::COPY_DST | BufferUsage::INDEX);
    context.graphics_device.init_buffer(&vertex_buffer_temp, &vertex_buffer);
    context.graphics_device.init_buffer(&index_buffer_temp, &index_buffer);

    let mesh = Mesh {
      vertices: vertex_buffer,
      indices: Some(index_buffer),
      parts: vec![MeshRange {
        start: 0,
        count: brush_indices.len() as u32
      }]
    };

    let model = Model {
      mesh_path: "brushes_mesh".to_string(),
      material_paths: vec!["BLANK_MATERIAL".to_string()]
    };

    let mut world = World::new(WorldOptions::default());
    world.push(
      (StaticRenderableComponent {
        model_path: "brushes_model".to_string(),
        receive_shadows: true,
        cast_shadows: true,
        can_move: false
      },
      Transform {
        position: Vec3::new(0.0f32, 0.0f32, 0.0f32),
        //rotation: Quaternion::identity(),
        scale: Vec3::new(1.0f32, 1.0f32, 1.0f32),
        rotation: Quaternion::from_axis_angle(&Unit::new_unchecked(Vec3::new(1.0f32, 0.0f32, 0.0f32)), std::f32::consts::FRAC_PI_2),
        //scale: Vec3::new(42.35f32, 42.35f32, 42.35f32),
      })
    );

    Ok(AssetLoaderResult {
      assets: vec![
        LoadedAsset {
          path: "brushes_mesh".to_string(),
          asset: Asset::Mesh(mesh)
        },
        LoadedAsset {
          path: "brushes_model".to_string(),
          asset: Asset::Model(model)
        },
        LoadedAsset {
          path: asset_file.path.clone(),
          asset: Asset::Level(world)
        }
      ],
      requests: Vec::new()
    })
  }
}
