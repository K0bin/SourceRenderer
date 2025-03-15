use std::{fmt::{Debug, Error, Formatter}, sync::Arc};

use super::*;

pub(super) struct BindlessSlotAllocator {
    chunk: Chunk<()>
}

pub struct BindlessSlot {
    alloc: Allocation<()>,
    texture: Arc<super::TextureView>
}

impl BindlessSlotAllocator {
    pub fn new(slots: u32) -> Self {
        Self {
            chunk: Chunk::new((), slots as u64)
        }
    }

    pub fn get_slot(&self, texture: &Arc<super::TextureView>) -> Option<BindlessSlot> {
        self.chunk.allocate(1, 1).map(|alloc: Allocation<()>| {
            BindlessSlot {
                alloc,
                texture: texture.clone()
            }
        })
    }
}

impl BindlessSlot {
    #[inline(always)]
    pub fn slot(&self) -> u32 {
        self.alloc.range.offset as u32
    }

    #[inline(always)]
    pub fn texture_view(&self) -> &Arc<TextureView> {
        &self.texture
    }
}

impl Debug for BindlessSlot {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        f.write_fmt(format_args!("BindlessSlot {}", self.alloc.range.offset))
    }
}
