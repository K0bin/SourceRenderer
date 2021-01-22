use std::io::{Read, Result as IOResult};

pub use crate::lump_data::brush_model::BrushModel;
pub use crate::lump_data::brush_side::BrushSide;
pub use crate::lump_data::disp_info::*;
pub use crate::lump_data::disp_tri::DispTri;
pub use crate::lump_data::disp_vert::DispVert;
pub use crate::lump_data::edge::Edge;
pub use crate::lump_data::face::Face;
pub use crate::lump_data::leaf_brush::LeafBrush;
pub use crate::lump_data::leaf_face::LeafFace;
pub use crate::lump_data::lighting::Lighting;
pub use crate::lump_data::pakfile::PakFile;
pub use crate::lump_data::plane::Plane;
pub use crate::lump_data::surface_edge::SurfaceEdge;
pub use crate::lump_data::texture_data::TextureData;
pub use crate::lump_data::texture_data_string_table::TextureDataStringTable;
pub use crate::lump_data::texture_info::*;
pub use crate::lump_data::texture_string_data::TextureStringData;
pub use crate::lump_data::vertex::Vertex;
pub use crate::lump_data::vertex_normal::VertexNormal;
pub use crate::lump_data::vertex_normal_index::VertexNormalIndex;
pub use crate::lump_data::visibility::Visibility;
pub use crate::game_lumps::GameLumps;
pub use crate::lump_data::entity::Entities;
use crate::lump_data::entity::parse_key_value;

pub use self::brush::Brush;
pub use self::leaf::Leaf;
pub use self::node::Node;

mod brush;
mod node;
mod leaf;
mod edge;
mod face;
mod brush_side;
mod plane;
mod leaf_face;
mod leaf_brush;
mod surface_edge;
mod vertex;
mod vertex_normal;
mod vertex_normal_index;
mod texture_data;
mod texture_info;
mod texture_string_data;
mod texture_data_string_table;
mod brush_model;
mod pakfile;
mod disp_info;
mod disp_vert;
mod disp_tri;
mod lighting;
mod visibility;
pub mod game_lumps;
mod entity;

#[derive(Clone, Copy, Debug)]
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

pub(crate) trait LumpData : Sized{
  fn lump_type() -> LumpType;
  fn lump_type_hdr() -> Option<LumpType>;
  fn element_size(version: i32) -> usize;
  fn read(read: &mut dyn Read, version: i32) -> IOResult<Self>;
}
