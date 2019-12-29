use std::io::{Read, Error};
use byteorder::{ReadBytesExt, LittleEndian};
use lump_data::brush::BrushContents;

pub const LEAF_SIZE_LE19: u8 = 56;
pub const LEAF_SIZE: u8 = 32;

#[derive(Copy, Clone, Debug, Default)]
pub struct ColorRGBExp32 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub exponent: i8
}

#[derive(Copy, Clone, Debug, Default)]
pub struct CompressedLightCube {
    pub color: [ColorRGBExp32; 6]
}

#[derive(Copy, Clone, Debug, Default)]
pub struct Leaf {
    pub contents: BrushContents,
    pub cluster: i16,
    pub area: i16,
    pub flags: i16,
    pub mins: [i16; 3],
    pub maxs: [i16; 3],
    pub first_leaf_face: u16,
    pub leaf_faces_count: u16,
    pub first_leaf_brush: u16,
    pub leaf_brushes_count: u16,
    pub leaf_water_data_id: i16,
    pub ambient_lighting: CompressedLightCube,
    pub padding: i16
}

impl ColorRGBExp32 {
    pub fn read(reader: &mut Read) -> Result<ColorRGBExp32, Error> {
        let r = reader.read_u8();
        if r.is_err() {
            return Err(r.err().unwrap());
        }
        let g = reader.read_u8();
        if g.is_err() {
            return Err(g.err().unwrap());
        }
        let b = reader.read_u8();
        reader.read_u8();
        if b.is_err() {
            return Err(b.err().unwrap());
        }
        let exponent = reader.read_i8();
        if exponent.is_err() {
            return Err(exponent.err().unwrap());
        }
        return Ok(ColorRGBExp32 {
            r: r.unwrap(),
            g: g.unwrap(),
            b: b.unwrap(),
            exponent: exponent.unwrap()
        });
    }
}

impl CompressedLightCube {
    pub fn read(reader: &mut Read) -> Result<CompressedLightCube, Error> {
        let mut colors: [ColorRGBExp32; 6] = [Default::default(); 6];
        for i in 0..6 {
            let color = ColorRGBExp32::read(reader);
            if (color.is_err()) {
                return Err(color.err().unwrap());
            }
            colors[i] = color.unwrap();
        }
        return Ok(CompressedLightCube {
            color: colors
        });
    }
}

impl Leaf {
    pub fn read(reader: &mut Read, version: i32) -> Result<Leaf, Error> {
        let contents = reader.read_u32::<LittleEndian>();
        if contents.is_err() {
            return Err(contents.err().unwrap());
        }
        let cluster = reader.read_i16::<LittleEndian>();
        if cluster.is_err() {
            return Err(cluster.err().unwrap());
        }
        let area_flags_res = reader.read_u16::<LittleEndian>();
        if area_flags_res.is_err() {
            return Err(area_flags_res.err().unwrap());
        }
        let area_flags = area_flags_res.unwrap();
        let area: i16 = ((area_flags & 0b1111_1111_1000_0000) >> 7) as i16;
        let flags: i16 = (area_flags & 0b0000_0000_0111_1111) as i16;

        let mut mins: [i16; 3] = [0; 3];
        for i in 0..mins.len() {
            let min = reader.read_i16::<LittleEndian>();
            if min.is_err() {
                return Err(min.err().unwrap());
            }
            mins[i] = min.unwrap();
        }

        let mut maxs: [i16; 3] = [0; 3];
        for i in 0..maxs.len() {
            let max = reader.read_i16::<LittleEndian>();
            if max.is_err() {
                return Err(max.err().unwrap());
            }
            maxs[i] = max.unwrap();
        }

        let first_leaf_face = reader.read_u16::<LittleEndian>();
        if first_leaf_face.is_err() {
            return Err(first_leaf_face.err().unwrap());
        }

        let leaf_faces_count = reader.read_u16::<LittleEndian>();
        if leaf_faces_count.is_err() {
            return Err(leaf_faces_count.err().unwrap());
        }

        let first_leaf_brush = reader.read_u16::<LittleEndian>();
        if first_leaf_brush.is_err() {
            return Err(first_leaf_brush.err().unwrap());
        }

        let leaf_brushes_count = reader.read_u16::<LittleEndian>();
        if leaf_brushes_count.is_err() {
            return Err(leaf_brushes_count.err().unwrap());
        }

        let leaf_water_data_id = reader.read_i16::<LittleEndian>();
        if leaf_water_data_id.is_err() {
            return Err(leaf_water_data_id.err().unwrap());
        }

        let mut padding: i16 = 0;
        let mut ambient_lighting: CompressedLightCube = Default::default();
        if version <= 19 {
            let ambient_lighting_res = CompressedLightCube::read(reader);
            if ambient_lighting_res.is_err() {
                return Err(ambient_lighting_res.err().unwrap());
            }
            ambient_lighting = ambient_lighting_res.unwrap();

            let padding_res = reader.read_i16::<LittleEndian>();
            if padding_res.is_err() {
                return Err(padding_res.err().unwrap());
            }
            padding = padding_res.unwrap();
        }

        return Ok(Leaf {            
            contents: BrushContents::new(contents.unwrap()),
            cluster: cluster.unwrap(),
            area: area,
            flags: flags,
            mins: mins,
            maxs: maxs,
            first_leaf_face: first_leaf_face.unwrap(),
            leaf_faces_count: leaf_faces_count.unwrap(),
            first_leaf_brush: first_leaf_brush.unwrap(),
            leaf_brushes_count: leaf_brushes_count.unwrap(),
            leaf_water_data_id: leaf_water_data_id.unwrap(),
            ambient_lighting: ambient_lighting,
            padding: padding
        });
    }
}
