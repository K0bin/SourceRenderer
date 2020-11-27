pub use self::brush::Brush;
pub use self::node::Node;
pub use self::leaf::Leaf;
pub use lump_data::edge::Edge;
pub use lump_data::brush_side::BrushSide;
pub use lump_data::face::Face;
pub use lump_data::plane::Plane;
pub use lump_data::leaf_face::LeafFace;
pub use lump_data::leaf_brush::LeafBrush;
pub use lump_data::surface_edge::SurfaceEdge;
pub use lump_data::vertex::Vertex;
pub use lump_data::vertex_normal::VertexNormal;
pub use lump_data::vertex_normal_index::VertexNormalIndex;

use std::io::{Read, Result as IOResult};

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
  fn element_size(version: i32) -> usize;
  fn read(read: &mut dyn Read, version: i32) -> IOResult<Self>;
}
