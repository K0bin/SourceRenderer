pub use self::brush::Brush;

use self::brush::BRUSH_SIZE;

use std::io::{Read, Error};

mod brush;

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
    DisplacementMultiblend = 63
}

pub enum LumpData {
    Brushes(Box<Vec<Brush>>)
}

pub fn read_lump_data(reader: &mut Read, lumpType: LumpType, size: i32) -> Result<LumpData, Error> {
    match lumpType {
        LumpType::Brushes => {
            let elementCount = size / i32::from(BRUSH_SIZE);
            let mut elements: Box<Vec<Brush>> = Box::new(Vec::new());
            for i in 0..elementCount {
                let brush = Brush::read(reader);
                if brush.is_err() {
                    return Err(brush.err().unwrap());
                }
                elements.push(brush.unwrap());
            }
            return Ok(LumpData::Brushes(elements));
        }
        _ => unimplemented!()
    }
}
