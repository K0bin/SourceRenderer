use map_header::{MapHeader};
use std::io::{Read, Error, ErrorKind, Seek, SeekFrom, BufReader};
use std::fs::File;
use lump_data::{LumpType, Brush, Node, Leaf, Face, Plane, Edge, BrushSide, LumpData};
use std::ops::DerefMut;
use std::boxed::{Box};
use std::io::Result as IOResult;

pub struct Map {
  pub name: String,
  header: MapHeader,
  reader: BufReader<File>,
}

impl Map {
  pub fn read(name: String, mut reader: BufReader<File>) -> IOResult<Map> {
    let header = MapHeader::read(&mut reader)?;
    return Ok(Map {
      name,
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
    self.read_lump_data::<Leaf>()
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
