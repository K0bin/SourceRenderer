use crate::header::Header;
use std::io::{Read, Seek, Result as IOResult, SeekFrom};
use crate::thumbnail::Thumbnail;
use std::collections::HashMap;
use io_util::{PrimitiveRead, RawDataRead};
use crate::image_format::{is_image_format_supported, calculate_image_size};
use crate::{MipMap, Face, Slice};
use std::cmp::max;
use crate::Frame;

pub struct VtfTexture<R: Read + Seek> {
  reader: R,
  header: Header,
  resource_offsets: HashMap<Resource, u32>,
  thumbnail: Option<Thumbnail>
}

#[derive(Hash, PartialEq, Eq, Copy, Clone, Debug)]
pub enum Resource {
  Unknwon,
  Thumbnail,
  Image,
  CRC
}

impl<R: Read + Seek> VtfTexture<R> {
  pub fn check_file<T: Read + Seek>(reader: &mut T) -> IOResult<bool> {
    Header::check_file(reader)
  }

  pub fn new(mut reader: R) -> IOResult<Self> {
    let header = Header::read(&mut reader)?;
    let resource_offsets = Self::read_resource_offsets(&mut reader, &header)?;
    let thumbnail = resource_offsets.get(&Resource::Thumbnail).and_then(|offset| {
      reader.seek(SeekFrom::Start(*offset as u64)).ok()?;
      let size = calculate_image_size(header.low_res_image_width as u32, header.low_res_image_height as u32, 1, header.low_res_image_format) as usize;
      let buffer = reader.read_data(size).ok()?;
      Some(Thumbnail {
        data: buffer,
        width: header.low_res_image_width as u32,
        height: header.low_res_image_height as u32,
        format: header.low_res_image_format
      })
    });
    Ok(Self {
      reader,
      header,
      resource_offsets,
      thumbnail
    })
  }

  fn calculate_mip_offset(&self, level: u32) -> Option<u64> {
    if level > self.header.mipmap_count as u32 {
      return None;
    }
    let mut offset = *self.resource_offsets.get(&Resource::Image).unwrap() as u64;

    // I'm sure there's a way to simply calculate that with math but this works just fine
    for level in 0 .. level {
      let reversed_level = self.header.mipmap_count as u32 - 1 - level;

      let level_width = max(1, self.header.width >> reversed_level) as u32;
      let level_height = max(1, self.header.height >> reversed_level) as u32;
      let level_image_size = calculate_image_size(level_width, level_height, 1, self.header.high_res_image_format) as u64;
      let frames_count = self.header.frames as u64;
      let faces_count = 1u64; // TODO
      let slices_count = max(1, self.header.depth as u64); // does this perhaps scale with the mip level in some cases?
      offset += level_image_size * frames_count * faces_count * slices_count;
    }
    Some(offset)
  }

  pub fn header(&self) -> &Header {
    &self.header
  }

  pub fn read_mip_map(&mut self, level: u32) -> Option<MipMap> {
    let offset = self.calculate_mip_offset(level)?;
    self.reader.seek(SeekFrom::Start(offset)).ok()?;

    let reversed_level = self.header.mipmap_count as u32 - 1 - level;
    let level_width = max(1, self.header.width >> reversed_level) as u32;
    let level_height = max(1, self.header.height >> reversed_level) as u32;
    let level_image_size = calculate_image_size(level_width, level_height, 1, self.header.high_res_image_format);

    let frames_count = self.header.frames;
    let faces_count = 1; // TODO
    let slices_count = max(1, self.header.depth); // does this perhaps scale with the mip level in some cases?

    let mut frames = Vec::<Frame>::with_capacity(frames_count as usize);
    for _frame in 0..frames_count {
      let mut faces = Vec::<Face>::with_capacity(faces_count as usize);
      for _face in 0..faces_count {
        let mut slices = Vec::<Slice>::with_capacity(slices_count as usize);
        for _slice in 0..slices_count {
          let data = self.reader.read_data(level_image_size as usize).ok()?;
          slices.push(Slice {
            data
          });
        }
        faces.push(Face {
          slices
        });
      }
      frames.push(Frame {
        faces
      });
    }

    Some(MipMap {
      frames,
      format: self.header.high_res_image_format,
      width: level_width,
      height: level_height
    })
  }

  fn read_resource_offsets(reader: &mut R, header: &Header) -> IOResult<HashMap<Resource, u32>> {
    let has_thumbnail = header.low_res_image_width != 0
      && header.low_res_image_height != 0
      && is_image_format_supported(header.low_res_image_format)
      && calculate_image_size(header.low_res_image_width as u32, header.low_res_image_height as u32, 1, header.low_res_image_format) > 0;
    let mut resource_offsets = HashMap::<Resource, u32>::new();
    if header.version[0] > 7 || header.version[0] == 7 && header.version[1] >= 3 {
      for _ in 0 .. header.num_resources {
        let a = reader.read_u8()?;
        let b = reader.read_u8()?;
        let c = reader.read_u8()?;
        let d = reader.read_u8()?;
        let value = reader.read_u32()?;
        let id = (a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24);
        let resource = match id {
          0x01 => Resource::Thumbnail,
          0x30 => Resource::Image,
          _ => Resource::Unknwon
        };

        if resource != Resource::Unknwon {
          resource_offsets.insert(resource, value);
        }
      }
    } else {
      if has_thumbnail {
        resource_offsets.insert(Resource::Thumbnail, header.header_size);
      }
      resource_offsets.insert(Resource::Image, header.header_size + calculate_image_size(header.low_res_image_width as u32, header.low_res_image_height as u32, 1, header.low_res_image_format));
    }

    Ok(resource_offsets)
  }
}