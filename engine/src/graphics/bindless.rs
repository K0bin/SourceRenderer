use std::{fmt::{Debug, Error, Formatter}, sync::Arc};

use sourcerenderer_core::gpu::GPUBackend;

use super::*;

pub(super) struct BindlessSlotAllocator {
    chunk: Chunk<()>
}

pub struct BindlessSlot<B: GPUBackend> {
    alloc: Allocation<()>,
    texture: Arc<super::TextureView<B>>
}

impl BindlessSlotAllocator {
    pub fn new(slots: u32) -> Self {
        Self {
            chunk: Chunk::new((), slots as u64)
        }
    }

    pub fn get_slot<B: GPUBackend>(&self, texture: &Arc<super::TextureView<B>>) -> Option<BindlessSlot<B>> {
        self.chunk.allocate(1, 1).map(|alloc: Allocation<()>| {
            BindlessSlot {
                alloc,
                texture: texture.clone()
            }
        })
    }
}

impl<B: GPUBackend> BindlessSlot<B> {
    #[inline(always)]
    pub fn slot(&self) -> u32 {
        self.alloc.range.offset as u32
    }

    #[inline(always)]
    pub fn texture_view(&self) -> &Arc<TextureView<B>> {
        &self.texture
    }
}

impl<B: GPUBackend> Debug for BindlessSlot<B> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        f.write_fmt(format_args!("BindlessSlot {}", self.alloc.range.offset))
    }
}
