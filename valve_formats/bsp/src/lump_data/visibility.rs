use crate::PrimitiveRead;
use std::io::{Read, Result as IOResult};

pub struct Visibility {
    pub num_clusters: i32,
    pub byte_offsets: [Box<[i32]>; 2],
}

impl Visibility {
    pub fn read(reader: &mut dyn Read) -> IOResult<Self> {
        let num_clusters = reader.read_i32()?;
        let mut byte_offsets: [Box<[i32]>; 2] = [Box::new([0i32; 0]), Box::new([0i32; 0])];

        for offsets in &mut byte_offsets {
            let mut offsets_vec = Vec::with_capacity(num_clusters as usize);
            for _ in 0..num_clusters {
                offsets_vec.push(reader.read_i32()?);
            }
            *offsets = offsets_vec.into_boxed_slice();
        }

        Ok(Self {
            num_clusters,
            byte_offsets,
        })
    }
}
