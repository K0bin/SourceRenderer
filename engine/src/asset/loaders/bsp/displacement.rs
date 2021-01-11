use sourcerenderer_bsp::{NeighborOrientation, DispInfo, DispVert, NeighborEdge, DispSubNeighbor, NeighborCorner, NeighborSpan};
use sourcerenderer_core::{Vec2, Vec3, Vec2I};
use std::cell::RefCell;
use std::collections::HashMap;
use crate::asset::loaders::bsp::bsp_lumps::BspLumps;
use nalgebra::base::coordinates::X;

/* Ported from https://github.com/Metapyziks/SourceUtils/blob/e64dd0bdffc4d60e348b24c748fc785f96f3563f/SourceUtils/ValveBsp/Displacement.cs
and I have no idea why it does anything it does.

MIT License

Copyright (c) 2016 James King

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
 */

pub struct Neighbor<'a> {
  relative_orientation: NeighborOrientation,
  relative_min: Vec2,
  relative_max: Vec2,
  disp_index: u32,
  disps: &'a RefCell<HashMap<u32, Displacement<'a>>>,
  bsp: &'a BspLumps
}

impl<'a> Neighbor<'a> {
  fn new(disps: &'a RefCell<HashMap<u32, Displacement<'a>>>, bsp_lumps: &'a BspLumps, orig: &Displacement, disp_index: u32, orientation: NeighborOrientation, min: Vec2, max: Vec2) -> Self {
    if orientation != NeighborOrientation::Unknown {
      return Self {
        disps,
        disp_index,
        relative_orientation: orientation,
        relative_min: min,
        relative_max: max,
        bsp: bsp_lumps
      };
    }

    let disps_ref = disps.borrow();
    let disp = disps_ref.get(&disp_index).unwrap();

    // Temp hack until I figure out how to correctly offset orientation
    let mut test_pos = (min + max) * 0.5f32;

    if test_pos.x < 0f32 { test_pos.x = 0f32; }
    else if test_pos.x > 1f32 { test_pos.x = 1f32; }
    if test_pos.y < 0f32 { test_pos.y = 0f32; }
    else if test_pos.y > 1f32 { test_pos.y = 1f32; }

    let orig_sample = orig.get_inner_position(&test_pos);
    let mut best_dist_squared = f32::INFINITY;
    let mut best_orientation = NeighborOrientation::CounterClockwise0;

    for i in 0..4 {
      let relative_orientation: NeighborOrientation = unsafe { std::mem::transmute(i as u8) };
      let sample = Self::get_position_raw(disp, &test_pos, &min, &max, &relative_orientation);
      let dist_squared = (sample - orig_sample).magnitude_squared();

      if dist_squared < best_dist_squared {
        best_dist_squared = dist_squared;
        best_orientation = relative_orientation;
      }
    }

    Self {
      disps,
      disp_index,
      relative_orientation: best_orientation,
      relative_min: min,
      relative_max: max,
      bsp: bsp_lumps
    }
  }

  fn get_position(&self, pos: &Vec2) -> Vec3 {
    {
      let disps_ref = self.disps.borrow();
      let disp_opt = disps_ref.get(&self.disp_index);
      if disp_opt.is_some() {
        return Self::get_position_raw(disp_opt.unwrap(), pos, &self.relative_min, &self.relative_max, &self.relative_orientation);
      }
    }

    {
      let disp = Displacement::new(self.disps, self.disp_index, self.bsp);
      let mut disps_mut = self.disps.borrow_mut();
      disps_mut.insert(self.disp_index, disp);
    }
    self.get_position(pos)
  }

  fn contains(&self, relative_pos: &Vec2) -> bool {
    relative_pos.x >= self.relative_min.x && relative_pos.y >= self.relative_min.y
    && relative_pos.x <= self.relative_min.x && relative_pos.y <= self.relative_max.y
  }

  fn get_position_raw(disp: &Displacement, relative_pos: &Vec2, relative_min: &Vec2, relative_max: &Vec2, relative_orientation: &NeighborOrientation) -> Vec3 {
    let temp = Vec2::new(
      (relative_pos.x - relative_min.x) / (relative_max.x - relative_min.x),
      (relative_pos.y - relative_min.y) / (relative_max.y - relative_min.y),
    );

    let pos = match relative_orientation {
      NeighborOrientation::CounterClockwise90 => {
        Vec2::new(temp.y, 1f32 - temp.x)
      }
      NeighborOrientation::CounterClockwise180 => {
        Vec2::new(1f32 - temp.x, 1f32 - temp.y)
      }
      NeighborOrientation::CounterClockWise270 => {
        Vec2::new(1f32 - relative_pos.y, relative_pos.x)
      }
      NeighborOrientation::Unknown | NeighborOrientation::CounterClockwise0 => {
        temp
      }
    };

    disp.get_inner_position(&pos)
  }
}

pub(super) struct Displacement<'a> {
  bsp: &'a BspLumps,
  index: u32,
  disp_info: &'a DispInfo,
  disps: &'a RefCell<HashMap<u32, Displacement<'a>>>,
  corners: [Vec3; 4],
  first_corner: u32,
  min: Vec2I,
  max: Vec2I,

  positions: Vec<Vec3>,
  alphas: Vec<f32>,
  neighbors: Vec<Neighbor<'a>>,
  normal: Vec3
}

impl<'a> Displacement<'a> {
  pub(super) fn new(disps: &'a RefCell<HashMap<u32, Displacement<'a>>>, disp_index: u32, bsp: &'a BspLumps) -> Displacement<'a> {
    let disp_info = &bsp.disp_infos[disp_index as usize];
    let subdivisions = 1 << disp_info.power;

    let mut min = Vec2I::new(0, 0);
    let mut max = Vec2I::new(subdivisions, subdivisions);

    let mut corners: [Vec3; 4] = [Vec3::default(); 4];
    let mut first_corner_dist_squared = f32::MAX;
    let mut first_corner = 0;

    let face = &bsp.faces[disp_info.map_face as usize];
    let normal = bsp.planes[face.plane_index as usize].normal;

    for i in 0..4 {
      let surf_edge_index = face.first_edge + i;
      let surf_edge = &bsp.surface_edges[surf_edge_index as usize];
      let edge = &bsp.edges[surf_edge.index.abs() as usize];
      let pos = &bsp.vertices[if surf_edge.index >= 0 { 0 } else { 1 }].position;

      corners[i as usize] = *pos;
      let dist_squared = (disp_info.start_position - *pos).magnitude_squared();
      if dist_squared < first_corner_dist_squared {
        first_corner_dist_squared = dist_squared;
        first_corner = i;
      }
    }

    let mut disp = Displacement {
      bsp,
      index: disp_index,
      disp_info,
      disps,
      corners,
      first_corner: first_corner as u32,
      min,
      max,
      positions: Vec::new(),
      alphas: Vec::new(),
      neighbors: Vec::new(),
      normal
    };

    // neighbors
    let mut neighbors = Vec::<Neighbor>::new();
    for i in 0..4 {
      let edge_neighbors = &disp_info.edge_neighbors[i];
      for sub_i in 0..2 {
        if edge_neighbors.sub_neighbors[sub_i].is_valid() {
          let edge: NeighborEdge = unsafe { std::mem::transmute(i as u8) };
          Self::add_edge_neighbor(&mut neighbors, edge, &disp, &edge_neighbors.sub_neighbors[sub_i], disps, bsp);
        }
      }
    }
    for i in 0usize .. 4usize {
      let corner_neighbors = &disp_info.corner_neighbors[i];
      for corner_neighbor_index in corner_neighbors.corner_neighbor_indices() {
        let corner: NeighborCorner = unsafe { std::mem::transmute(i as u8 )};
        Self::add_corner_neighbor(&mut neighbors, corner, &disp, *corner_neighbor_index as u32, disps, bsp);
      }
    }
    disp.neighbors = neighbors;

    let size = subdivisions as usize + 1;
    let mut positions = Vec::with_capacity(size * size);
    let mut alphas = Vec::with_capacity(size * size);

    const ALPHA_MUL: f32 = 1f32 / 255f32;

    for y in 0..size {
      for x in 0..size {
        let index = x + y * size;
        let vert = &bsp.disp_verts[disp_info.disp_vert_start as usize + index];

        let tx = x as f32 / (size as f32 - 1f32);
        let ty = y as f32 / (size as f32 - 1f32);
        let sx = 1f32 - tx;
        let sy = 1f32 - ty;

        let corners: [Vec3; 4] = [
          disp.corners[(first_corner as usize) & 3],
          disp.corners[(1 + first_corner as usize) & 3],
          disp.corners[(2 + first_corner as usize) & 3],
          disp.corners[(3 + first_corner as usize) & 3],
        ];

        let test = sx * disp.corners[1];
        let origin = ty * (sx * corners[1] + tx * corners[2]) + sy * (sx * corners[0] + tx * corners[3]);
        positions.push(origin + vert.vec * vert.dist);
        alphas.push(vert.alpha * ALPHA_MUL);
      }
    }
    disp.positions = positions;
    disp.alphas = alphas;

    disp
  }

  fn add_edge_neighbor(neighbors: &mut Vec<Neighbor<'a>>, edge: NeighborEdge, orig: &Displacement, sub_neighbor: &DispSubNeighbor, disps: &'a RefCell<HashMap<u32, Displacement<'a>>>, bsp: &'a BspLumps) {
    let mut min = Vec2::new(0f32, 0f32);
    let mut size = 1f32;

    if sub_neighbor.span != NeighborSpan::CornerToCorner || sub_neighbor.neighbor_span != NeighborSpan::CornerToCorner {
      // TODO
      return;
    }

    match edge {
      NeighborEdge::Left => { min.x -= size; }
      NeighborEdge::Bottom => { min.y -= size; }
      NeighborEdge::Right => { min.x += 1f32; }
      NeighborEdge::Top => { min.y += 1f32; }
    }
    if neighbors.iter().any(|x| x.disp_index == sub_neighbor.neighbor_index as u32) {
      return;
    }
    neighbors.push(Neighbor::new(disps, bsp, orig, sub_neighbor.neighbor_index as u32, sub_neighbor.neighbor_orientation, min, min + Vec2::new(size, size)));
  }

  fn add_corner_neighbor(neighbors: &mut Vec<Neighbor<'a>>, corner: NeighborCorner, orig: &Displacement, disp_index: u32, disps: &'a RefCell<HashMap<u32, Displacement<'a>>>, bsp: &'a BspLumps ) {
    let mut min = Vec2::new(0f32, 0f32);
    let size = 1f32;

    match corner {
      NeighborCorner::LowerLeft => {
        min.x -= size;
        min.y -= size;
      }
      NeighborCorner::LowerRight => {
        min.x += 1f32;
        min.y -= size;
      }
      NeighborCorner::UpperLeft => {
        min.x -= size;
        min.y += 1f32;
      }
      NeighborCorner::UpperRight => {
        min.x += 1f32;
        min.y += 1f32;
      }
    }
    if neighbors.iter().any(|x| x.disp_index == disp_index) {
      return;
    }
    neighbors.push(Neighbor::new(disps, bsp, orig, disp_index, NeighborOrientation::Unknown, min, min + Vec2::new(size, size)));
  }

  fn get_inner_position(&self, pos: &Vec2) -> Vec3 {
    let subdivisions = self.subdivisions();
    let temp = *pos * (subdivisions as f32);

    let vecs = [
      Vec2I::new(
        temp.x.floor() as i32,
        temp.y.floor() as i32,
      ),
      Vec2I::new(
        temp.x.ceil() as i32,
        temp.y.ceil() as i32,
      )
    ];

    let s00 = self.get_inner_position_int(&vecs[0]);

    if vecs[0] == vecs[1] {
      return s00;
    }

    let s10 = if vecs[0].x == vecs[1].x { s00 } else { self.get_inner_position_int(&Vec2I::new(vecs[1].x, vecs[0].y)) };
    let s01 = if vecs[0].y == vecs[1].y { s00 } else { self.get_inner_position_int(&Vec2I::new(vecs[0].x, vecs[1].y)) };
    let s11 = if vecs[0].x == vecs[1].x {
      s01
    } else if vecs[0].y == vecs[1].y {
      s10
    } else {
      self.get_inner_position_int(&vecs[1])
    };

    let tx = temp.x - vecs[0].x as f32;
    let sx = 1f32 - tx;
    let ty = temp.y - vecs[0].y as f32;
    let sy = 1f32 - ty;

    (s00 * sx + s10 * tx) * sy + (s01 * sx + s11 * tx) * ty
  }

  fn get_inner_position_int(&self, pos: &Vec2I) -> Vec3 {
    let size = self.size();

    let mut temp = *pos;
    if temp.x < self.min.x { temp.x = self.min.x; }
    else if temp.x > self.max.x { temp.x = self.max.x; }
    if temp.y < self.min.y { temp.y = self.min.y; }
    else if temp.y > self.max.y { temp.y = self.max.y; }

    self.positions[(temp.x + temp.y * size) as usize]
  }

  pub fn contains(&self, pos: &Vec2I) -> bool {
    pos.x >= self.min.x && pos.y >= self.min.y && pos.x <= self.max.x && pos.y <= self.max.y
  }

  pub fn get_corners(&self) -> &[Vec3; 4] {
    &self.corners
  }

  pub fn get_position(&self, pos: &Vec2I) -> Vec3 {
    return self.get_inner_position_int(pos); // TEMP

    let subdivisions = 1 << self.disp_info.power;

    let mut total = 0;
    let mut sum = Vec3::default();

    if self.contains(pos) {
      sum += self.get_inner_position_int(pos);
      total += 1;
    }

    let relative_pos = Vec2::new(pos.x as f32 / subdivisions as f32, pos.y as f32 / subdivisions as f32);
    for neighbor in &self.neighbors {
      if neighbor.contains(&relative_pos) {
        sum += neighbor.get_position(&relative_pos);
        total += 1;
      }
    }

    if total == 0 {
      self.get_inner_position_int(pos)
    } else {
      sum * (1f32 / total as f32)
    }
  }

  pub fn get_normal(&self, pos: &Vec2I) -> Vec3 {
    return self.normal; // TEMP

    let mut kernel = [Vec3::default(); 9];

    for i in 0..3 {
      for j in 0..3 {
        kernel[i + j * 3] = self.get_position(&Vec2I::new(pos.x + i as i32 - 1, pos.y + j as i32 - i as i32));
      }
    }

    let mut sum = Vec3::default();

    for i in 0..2 {
      for j in 0..2 {
        let a = kernel[i + 0 + (j + 0) * 3];
        let b = kernel[i + 1 + (j + 0) * 3];
        let c = kernel[i + 0 + (j + 1) * 3];
        let d = kernel[i + 1 + (j + 1) * 3];

        let abd = (b - a).cross(&(d - a));
        let adc = (d - a).cross(&(c - d));

        sum += abd + adc;
      }
    }

    let normal = sum.normalize();

    if normal.x.is_nan() || normal.y.is_nan() || normal.z.is_nan() {
      self.normal
    } else {
      normal
    }
  }

  pub fn subdivisions(&self) -> i32 {
    1 << self.disp_info.power
  }

  pub fn size(&self) -> i32 {
    self.subdivisions() + 1
  }
}