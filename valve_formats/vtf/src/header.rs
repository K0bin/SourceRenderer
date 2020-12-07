use crate::image_format::ImageFormat;
use crate::texture_flags::TextureFlags;
use std::io::{Read, Result as IOResult, Error as IOError, Seek, SeekFrom, ErrorKind};
use crate::read_util::PrimitiveRead;

const SIZE_73: u32 = 80;
const SIZE_72: u32 = 80;
const SIZE_71: u32 = 64;

const EXPECTED_SIGNATURE: u32 = 0x00465456;

pub struct Header {
  /// File signature ("VTF\0"). (or as little-endian integer, 0x00465456)
  pub signature: u32,
  /// version[0].version[1] (currently 7.2).
  pub version: [u32; 2],
  /// Size of the header struct  (16 byte aligned; currently 80 bytes) + size of the resources dictionary (7.3+).
  pub header_size: u32,
  /// Width of the largest mipmap in pixels. Must be a power of 2.
  pub width: u16,
  /// Height of the largest mipmap in pixels. Must be a power of 2.
  pub height: u16,
  /// VTF flags.
  pub flags: TextureFlags,
  /// Number of frames, if animated (1 for no animation).
  pub frames: u16,
  /// First frame in animation (0 based).
  pub first_frame: u16,
  /// reflectivity padding (16 byte alignment).
  pub padding0: [u8; 4],
  /// reflectivity vector.
  pub reflectivity: [f32; 3],
  /// reflectivity padding (8 byte packing).
  pub padding1: [u8; 4],
  /// Bumpmap scale.
  pub bumpmap_scale: f32,
  /// High resolution image format.
  pub high_res_image_format: ImageFormat,
  /// Number of mipmaps.
  pub mipmap_count: u8,
  /// Low resolution image format (always DXT1).
  pub low_res_image_format: ImageFormat,
  /// Low resolution image width.
  pub low_res_image_width: u8,
  /// Low resolution image height.
  pub low_res_image_height: u8,

  // 7.2+
  /// Depth of the largest mipmap in pixels.
  /// Must be a power of 2. Can be 0 or 1 for a 2D texture (v7.2 only).
  pub depth: u16,

  // 7.3+
  /// depth padding (4 byte alignment).
  pub padding2: [u8; 3],
  /// Number of resources this vtf has
  pub num_resources: u32
}

impl Header {
  pub(super) fn check_file<T: Read + Seek>(mut reader: &mut T) -> IOResult<bool> {
    let signature = reader.read_u32()?;
    Ok(signature == EXPECTED_SIGNATURE)
  }


  pub fn read<T: Read + Seek>(mut reader: &mut T) -> IOResult<Self> {
    let signature = reader.read_u32()?;
    if signature != EXPECTED_SIGNATURE {
      return Err(IOError::new(ErrorKind::Other, "File is not a VTF file"));
    }
    let version = [reader.read_u32()?, reader.read_u32()?];
    let header_size = reader.read_u32()?;
    let width = reader.read_u16()?;
    let height = reader.read_u16()?;
    let flags = TextureFlags::from_bits(reader.read_u32()?).unwrap();
    let frames = reader.read_u16()?;
    let first_frame = reader.read_u16()?;
    reader.seek(SeekFrom::Current(4))?;
    let reflectivity = [reader.read_f32()?, reader.read_f32()?, reader.read_f32()?];
    reader.seek(SeekFrom::Current(4))?;
    let bumpmap_scale = reader.read_f32()?;
    let high_res_image_format: ImageFormat = unsafe { std::mem::transmute(reader.read_u32()?) };
    let mipmap_count = reader.read_u8()?;
    let low_res_image_format: ImageFormat =  unsafe { std::mem::transmute(reader.read_u32()?) };
    let low_res_image_width = reader.read_u8()?;
    let low_res_image_height = reader.read_u8()?;

    let depth = if version[0] > 7 || version[0] == 7 && version[1] >= 2 {
      reader.read_u16()?
    } else {
      0u16
    };

    let num_resources = if version[0] > 7 || version[0] == 7 && version[1] >= 3 {
      reader.seek(SeekFrom::Current(3))?;
      reader.read_u32()?
    } else {
      0u32
    };

    Ok(Self {
      signature,
      version,
      header_size,
      width,
      height,
      flags,
      frames,
      first_frame,
      padding0: Default::default(),
      reflectivity,
      padding1: Default::default(),
      bumpmap_scale,
      high_res_image_format,
      mipmap_count,
      low_res_image_format,
      low_res_image_width,
      low_res_image_height,
      depth,
      padding2: Default::default(),
      num_resources
    })
  }
}