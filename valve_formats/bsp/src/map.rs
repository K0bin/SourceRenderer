use map_header::{MapHeader};
use std::io::{Seek, SeekFrom, Read};
use std::fs::File;
use lump_data::{Brush, Node, Leaf, Face, Plane, Edge, BrushSide, LumpData};


use std::io::Result as IOResult;
use ::{LeafFace, LeafBrush};
use ::{SurfaceEdge, VertexNormal};
use ::{Vertex, VertexNormalIndex};

pub struct Map<R: Read + Seek> {
  pub name: String,
  header: MapHeader,
  reader: R,
}

impl<R: Read + Seek> Map<R> {
  pub fn read(name: &str, mut reader: R) -> IOResult<Map<R>> {
    reader.seek(SeekFrom::Start(0));
    let header = MapHeader::read(&mut reader)?;
    return Ok(Map {
      name: name.to_owned(),
      header,
      reader,
    });
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

  fn read_lump_data<T: LumpData>(&mut self) -> IOResult<Vec<T>> {
    let index = T::lump_type() as usize;
    let lump = self.header.lumps[index];
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
