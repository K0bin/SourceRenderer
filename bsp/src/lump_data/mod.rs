pub use self::brush::Brush;
pub use self::node::Node;
pub use self::leaf::Leaf;

use self::brush::BRUSH_SIZE;
use self::node::NODE_SIZE;
use self::leaf::LEAF_SIZE;
use self::leaf::LEAF_SIZE_LE19;

use std::io::{Read, Result};
use lump_data::edge::{Edge, EDGE_SIZE};
use lump_data::brush_side::BrushSide;
use lump_data::face::{Face, FACE_SIZE};
use lump_data::plane::{Plane, PLANE_SIZE};
use lump_data::LumpType::BrushSides;

mod brush;
mod node;
mod leaf;
mod edge;
mod face;
mod brush_side;
mod plane;

#[derive(FromPrimitive, Clone, Copy, Debug)]
#[repr(u8)]
pub enum LumpType {
  Entities = 0,
  Planes = 1,
  TextureData = 2,
  Vertices = 3,
  Visibility = 4,
  Nodes = 5,
  TextureInfo = 6,
  Faces = 7,
  Lighting = 8,
  Occlusion = 9,
  Leafs = 10,
  FaceIds = 11,
  Edges = 12,
  SurfaceEdges = 13,
  Models = 14,
  WorldLights = 15,
  LeafFaces = 16,
  LeafBrushes = 17,
  Brushes = 18,
  BrushSides = 19,
  Areas = 20,
  AreaPortals = 21,
  PropCollisions = 22,
  PropHulls = 23,
  PropHullVerts = 24,
  PropTriangles = 25,
  DisplacementInfo = 26,
  OriginalFaces = 27,
  PhysicsDisplacement = 28,
  PhysicsCollision = 29,
  VertexNormals = 30,
  VertexNormalIndices = 31,
  DisplacementLightmapAlphas = 32,
  DisplacementVertices = 33,
  DisplacementLightmapSamplePositions = 34,
  GameLump = 35,
  LeafWaterData = 36,
  Primitives = 37,
  PrimitiveVertices = 38,
  PrimitiveIndices = 39,
  PakFile = 40,
  ClipPortalVertices = 41,
  Cubemaps = 42,
  TextureStringData = 43,
  TextureDataStringTable = 44,
  Overlays = 45,
  LeafsMinimumDistanceToWater = 46,
  FaceMakroTextureInfo = 47,
  DisplacementTriangles = 48,
  PropBlob = 49,
  WaterOverlays = 50,
  LeafAmbientIndexHDR = 51,
  LeafAmbientIndex = 52,
  LightingHDR = 53,
  WorldlightsHDR = 54,
  LeafAmbientLightingHDR = 55,
  LeafAmbientLighting = 56,
  XzipPakFile = 57,
  FacesHDR = 58,
  MapFlags = 59,
  OverlayFades = 60,
  OverlaySystemSettings = 61,
  PhysicsLevel = 62,
  DisplacementMultiblend = 63,
}

pub enum LumpData {
  Brushes(Box<Vec<Brush>>),
  BrushSides(Box<Vec<BrushSide>>),
  Edges(Box<Vec<Edge>>),
  Faces(Box<Vec<Face>>),
  Leafs(Box<Vec<Leaf>>),
  Nodes(Box<Vec<Node>>),
  Planes(Box<Vec<Plane>>),
}

pub fn read_lump_data(reader: &mut dyn Read, lump_type: LumpType, size: i32, version: i32) -> Result<LumpData> {
  match lump_type {
    LumpType::Brushes => {
      let element_count = size / BRUSH_SIZE as i32;
      let mut elements: Box<Vec<Brush>> = Box::new(Vec::new());
      for _ in 0..element_count {
        let element = Brush::read(reader)?;
        elements.push(element);
      }
      Ok(LumpData::Brushes(elements))
    }

    LumpType::Nodes => {
      let element_count = size / NODE_SIZE as i32;
      let mut elements: Box<Vec<Node>> = Box::new(Vec::new());
      for _ in 0..element_count {
        let element = Node::read(reader)?;
        elements.push(element);
      }
      Ok(LumpData::Nodes(elements))
    }

    LumpType::Leafs => {
      let mut element_size = LEAF_SIZE as i32;
      if version <= 19 {
        element_size = LEAF_SIZE_LE19 as i32;
      }
      let element_count = size / element_size;
      let mut elements: Box<Vec<Leaf>> = Box::new(Vec::new());
      for _ in 0..element_count {
        let element = Leaf::read(reader, version)?;
        elements.push(element);
      }
      Ok(LumpData::Leafs(elements))
    }

    LumpType::BrushSides => {
      let mut element_count = size / BRUSH_SIZE as i32;
      let mut elements: Box<Vec<BrushSide>> = Box::new(Vec::new());
      for _ in 0..element_count {
        let element = BrushSide::read(reader)?;
        elements.push(element);
      }
      Ok(LumpData::BrushSides(elements))
    }

    LumpType::Edges => {
      let mut element_count = size / EDGE_SIZE as i32;
      let mut elements: Box<Vec<Edge>> = Box::new(Vec::new());
      for _ in 0..element_count {
        let element = Edge::read(reader)?;
        elements.push(element);
      }
      Ok(LumpData::Edges(elements))
    }

    LumpType::Faces => {
      let mut element_count = size / FACE_SIZE as i32;
      let mut elements: Box<Vec<Face>> = Box::new(Vec::new());
      for _ in 0..element_count {
        let element = Face::read(reader)?;
        elements.push(element);
      }
      Ok(LumpData::Faces(elements))
    }

    LumpType::Planes => {
      let mut element_count = size / PLANE_SIZE as i32;
      let mut elements: Box<Vec<Plane>> = Box::new(Vec::new());
      for _ in 0..element_count {
        let element = Plane::read(reader)?;
        elements.push(element);
      }
      Ok(LumpData::Planes(elements))
    }

    _ => unimplemented!()
  }
}
