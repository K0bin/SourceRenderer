use std::io::{Read, Error};
use byteorder::{ReadBytesExt, LittleEndian};

#[derive(Copy, Clone)]
pub struct Lump {
    pub file_offset: i32,
    pub file_length: i32,
    pub version: i32,
    pub four_cc: i32
}

#[derive(FromPrimitive)]
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
        LUMP_PROPHULLVERTS = 24,
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

pub fn read_lump(reader: &mut Read) -> Result<Lump, Error> {
    let file_offset = reader.read_i32::<LittleEndian>();
    if file_offset.is_err() {
        return Err(file_offset.err().unwrap());
    }
    let file_length = reader.read_i32::<LittleEndian>();
    if file_length.is_err() {
        return Err(file_length.err().unwrap());
    }
    let version = reader.read_i32::<LittleEndian>();
    if version.is_err() {
        return Err(version.err().unwrap());
    }
    let four_cc = reader.read_i32::<LittleEndian>();
    if four_cc.is_err() {
        return Err(four_cc.err().unwrap());
    }

    return Ok(Lump {
        file_offset: file_offset.unwrap(),
        file_length: file_length.unwrap(),
        version: version.unwrap(),
        four_cc: four_cc.unwrap()
    });
}
