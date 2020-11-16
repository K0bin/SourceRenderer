
use sourcerenderer_core::Platform;
use crate::asset::{AssetLoader, AssetType, Asset};
use std::io::{BufReader};
use std::fs::File;
use std::path::Path;
use sourcerenderer_bsp::{Map, Node, Leaf, SurfaceEdge, LeafBrush, LeafFace, Vertex, Face, Edge};
use std::sync::Mutex;
use std::collections::HashMap;
use sourcerenderer_core::{Vec3, Vec2};

pub struct BspLevelLoader {
  map: Mutex<Map>,
  map_name: String,
}

struct BspTemp {
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
  pub fn new(path: &str) -> std::io::Result<Self> {
    let map_name = Path::new(path).file_name().unwrap().to_str().unwrap().to_string();
    let buf_reader = BufReader::new(File::open(path)?);
    let map = Map::read(map_name.as_str(), buf_reader)?;

    Ok(Self {
      map: Mutex::new(map),
      map_name,
    })
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
    for leaf_face_index in leaf.first_leaf_face .. leaf.first_leaf_face + leaf.leaf_faces_count {
      let face_index = temp.leaf_faces[leaf_face_index as usize].index;
      let face = &temp.faces[face_index as usize];

      let mut face_vertices: HashMap<u16, u32> = HashMap::new(); // Just to make sure that there's no duplicates
      let mut root_vertex = 0u16;
      for surf_edge_index in face.first_edge .. face.first_edge + face.edges_count as i32 {
        let edge_index = temp.surface_edges[surf_edge_index as usize].index;
        let edge = temp.edges[edge_index as usize];

        // Push the two vertices of the first edge
        if surf_edge_index == face.first_edge {
          if !face_vertices.contains_key(&edge.vertex_index[if edge_index > 0 { 0 } else { 1 }]) {
            root_vertex = edge.vertex_index[if edge_index > 0 { 0 } else { 1 }];
            let position = temp.vertices[root_vertex as usize].position.clone();
            let vertex = crate::Vertex {
              position,
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
  fn matches(&self, path: &str, asset_type: AssetType) -> bool {
    match asset_type {
      AssetType::Level => Path::new(path).file_name().unwrap() == self.map_name.as_str(),
      _ => false
    }
  }

  fn load(&self, _path: &str, asset_type: AssetType) -> Option<Asset<P>> {
    if asset_type == AssetType::Model {
      let mut map_guard = self.map.lock().unwrap();
      let leafs = map_guard.read_leafs().ok()?;
      let nodes = map_guard.read_nodes().ok()?;
      let faces = map_guard.read_faces().ok()?;
      let leaf_faces = map_guard.read_leaf_faces().ok()?;
      let leaf_brushes = map_guard.read_leaf_brushes().ok()?;
      let edges = map_guard.read_edges().ok()?;
      let surface_edges = map_guard.read_surface_edges().ok()?;
      let vertices = map_guard.read_vertices().ok()?;
      let temp = BspTemp {
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
    } else {

    }

    None
  }
}
