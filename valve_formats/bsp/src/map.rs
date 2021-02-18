use crate::map_header::{MapHeader};
use std::io::{Seek, SeekFrom, Read, Result as IOResult};
use crate::lump_data::{Brush, Node, Leaf, Face,
                       Plane, Edge, BrushSide, LumpData,
                       TextureInfo, LeafFace, LeafBrush,
                       TextureData, SurfaceEdge, Vertex,
                       VertexNormalIndex, VertexNormal,
                       TextureDataStringTable, TextureStringData};
use crate::{LumpType, BrushModel, RawDataRead, PakFile, DispTri, DispInfo, DispVert, Lighting, Visibility, GameLumps, Entities};
use crate::lump_data::game_lumps::StaticPropDict;

pub struct Map<R: Read + Seek> {
  pub name: String,
  header: MapHeader,
  game_lumps: GameLumps,
  reader: R,
}

impl<R: Read + Seek> Map<R> {
  pub fn read(name: &str, mut reader: R) -> IOResult<Map<R>> {
    reader.seek(SeekFrom::Start(0))?;
    let header = MapHeader::read(&mut reader)?;
    let game_lumps = Self::read_game_lump(&header, &mut reader)?;
    Ok(Map {
      name: name.to_owned(),
      header,
      game_lumps,
      reader,
    })
  }

  pub fn read_game_lump(header: &MapHeader, read: &mut R) -> IOResult<GameLumps> {
    let index = LumpType::GameLump as usize;
    let lump = &header.lumps[index];
    read.seek(SeekFrom::Start(lump.file_offset as u64))?;
    GameLumps::read(read)
  }

  pub fn read_brushes(&mut self) -> IOResult<Vec<Brush>> {
    self.read_lump_data()
  }

  pub fn read_nodes(&mut self) -> IOResult<Vec<Node>> {
    self.read_lump_data()
  }

  pub fn read_leafs(&mut self) -> IOResult<Vec<Leaf>> {
    self.read_lump_data()
  }

  pub fn read_brush_sides(&mut self) -> IOResult<Vec<BrushSide>> {
    self.read_lump_data()
  }

  pub fn read_edges(&mut self) -> IOResult<Vec<Edge>> {
    self.read_lump_data()
  }

  pub fn read_faces(&mut self) -> IOResult<Vec<Face>> {
    self.read_lump_data()
  }

  pub fn read_planes(&mut self) -> IOResult<Vec<Plane>> {
    self.read_lump_data()
  }

  pub fn read_leaf_faces(&mut self) -> IOResult<Vec<LeafFace>> {
    self.read_lump_data()
  }

  pub fn read_leaf_brushes(&mut self) -> IOResult<Vec<LeafBrush>> {
    self.read_lump_data()
  }

  pub fn read_surface_edges(&mut self) -> IOResult<Vec<SurfaceEdge>> {
    self.read_lump_data()
  }

  pub fn read_vertices(&mut self) -> IOResult<Vec<Vertex>> {
    self.read_lump_data()
  }

  pub fn read_vertex_normals(&mut self) -> IOResult<Vec<VertexNormal>> {
    self.read_lump_data()
  }

  pub fn read_vertex_normal_indices(&mut self) -> IOResult<Vec<VertexNormalIndex>> {
    self.read_lump_data()
  }

  pub fn read_texture_data(&mut self) -> IOResult<Vec<TextureData>> {
    self.read_lump_data()
  }

  pub fn read_texture_info(&mut self) -> IOResult<Vec<TextureInfo>> {
    self.read_lump_data()
  }

  pub fn read_texture_string_data(&mut self) -> IOResult<TextureStringData> {
    let index = LumpType::TextureStringData as usize;
    let lump = self.header.lumps[index];
    self.reader.seek(SeekFrom::Start(lump.file_offset as u64))?;
    TextureStringData::read(&mut self.reader, lump.file_length as u32)
  }

  pub fn read_texture_data_string_table(&mut self) -> IOResult<Vec<TextureDataStringTable>> {
    self.read_lump_data()
  }

  pub fn read_brush_models(&mut self) -> IOResult<Vec<BrushModel>> {
    self.read_lump_data()
  }

  pub fn read_disp_infos(&mut self) -> IOResult<Vec<DispInfo>> {
    self.read_lump_data()
  }

  pub fn read_disp_tris(&mut self) -> IOResult<Vec<DispTri>> {
    self.read_lump_data()
  }

  pub fn read_disp_verts(&mut self) -> IOResult<Vec<DispVert>> {
    self.read_lump_data()
  }

  pub fn read_lighting(&mut self) -> IOResult<Vec<Lighting>> {
    self.read_lump_data()
  }

  pub fn read_hdr_lighting(&mut self) -> IOResult<Vec<Lighting>> {
    self.read_lump_data()
  }

  pub fn read_pakfile(&mut self) -> IOResult<PakFile> {
    let index = LumpType::PakFile as usize;
    let lump = self.header.lumps[index];
    self.reader.seek(SeekFrom::Start(lump.file_offset as u64))?;
    let data = self.reader.read_data(lump.file_length as usize)?;
    Ok(PakFile::new(data))
  }

  pub fn read_visibility(&mut self) -> IOResult<Visibility> {
    let index = LumpType::Visibility as usize;
    let lump = self.header.lumps[index];
    self.reader.seek(SeekFrom::Start(lump.file_offset as u64))?;
    Visibility::read(&mut self.reader)
  }

  pub fn read_static_props(&mut self) -> IOResult<StaticPropDict> {
    self.game_lumps.read_static_prop_dict(&mut self.reader)
  }

  pub fn read_entities(&mut self) -> IOResult<Entities> {
    let index = LumpType::Entities as usize;
    let lump = self.header.lumps[index];
    self.reader.seek(SeekFrom::Start(lump.file_offset as u64))?;
    Entities::read(&mut self.reader)
  }

  fn read_lump_data<T: LumpData>(&mut self) -> IOResult<Vec<T>> {
    let mut lump_type = T::lump_type();
    let lump_type_hdr = T::lump_type_hdr();
    if let Some(lump_type_hdr) = lump_type_hdr {
      let index = lump_type_hdr as usize;
      let lump = &self.header.lumps[index];
      if lump.file_length != 0 {
        lump_type = lump_type_hdr;
      }
    }

    let index = lump_type as usize;
    let lump = &self.header.lumps[index];
    self.reader.seek(SeekFrom::Start(lump.file_offset as u64))?;

    let element_count = lump.file_length / T::element_size(self.header.version) as i32;
    let mut elements: Vec<T> = Vec::new();
    for _ in 0..element_count {
      let element = T::read(&mut self.reader, self.header.version)?;
      elements.push(element);
    }
    Ok(elements)
  }
}
