use std::sync::Arc;

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
    pub fn slot(&self) -> u32 {
        self.alloc.range.offset as u32
    }

    pub fn texture_view(&self) -> &Arc<TextureView<B>> {
        &self.texture
    }
}
