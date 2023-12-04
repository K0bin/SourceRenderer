use std::{mem::ManuallyDrop, sync::Arc};

use sourcerenderer_core::gpu::*;

use super::*;

pub struct Texture<B: GPUBackend> {
    device: Arc<B::Device>,
    texture: ManuallyDrop<B::Texture>,
    allocation: Option<MemoryAllocation<B::Heap>>,
    destroyer: Arc<DeferredDestroyer<B>>
}

impl<B: GPUBackend> Drop for Texture<B> {
    fn drop(&mut self) {
        let texture = unsafe { ManuallyDrop::take(&mut self.texture) };
        self.destroyer.destroy_texture(texture);
        if let Some(allocation) = self.allocation.take() {
            self.destroyer.destroy_allocation(allocation);
        }
    }
}

impl<B: GPUBackend> Texture<B> {
    pub(super) fn new(device: &Arc<B::Device>, allocator: &MemoryAllocator<B>, destroyer: &Arc<DeferredDestroyer<B>>, info: &TextureInfo, name: Option<&str>) -> Result<Arc<Self>, OutOfMemoryError> {
        let heap_info = unsafe { device.get_texture_heap_info(info) };
        let (texture, allocation) = if heap_info.prefer_dedicated_allocation {
            let memory_types = unsafe { device.memory_type_infos() };
            let mut mask = allocator.find_memory_type_mask(MemoryUsage::GPUMemory, MemoryTypeMatchingStrictness::Normal) & heap_info.memory_type_mask;
            let mut texture: Result<B::Texture, OutOfMemoryError> = Err(OutOfMemoryError {});
            for i in 0..memory_types.len() as u32 {
                if (mask & i) == 0 {
                    continue;
                }
                texture = unsafe { device.create_texture(info, i, name) };
                if texture.is_ok() {
                    break;
                }
            }

            if texture.is_err() {
                mask = allocator.find_memory_type_mask(MemoryUsage::GPUMemory, MemoryTypeMatchingStrictness::Fallback) & heap_info.memory_type_mask;
                for i in 0..memory_types.len() as u32 {
                    if (mask & i) == 0 {
                        continue;
                    }
                    texture = unsafe { device.create_texture(info, i, name) };
                    if texture.is_ok() {
                        break;
                    }
                }
            }
            (texture?, None)
        } else {
            let allocation = allocator.allocate(MemoryUsage::GPUMemory, &heap_info)?;
            let texture = unsafe { allocation.data().create_texture(info, allocation.range.offset, name) }?;
            (texture, Some(allocation))
        };
        Ok(Arc::new(Self {
            device: device.clone(),
            texture: ManuallyDrop::new(texture),
            allocation,
            destroyer: destroyer.clone()
        }))
    }

    pub(crate) fn handle(&self) -> &B::Texture {
        &self.texture
    }
}

pub struct TextureView<B: GPUBackend> {
    device: Arc<B::Device>,
    texture: Option<Arc<Texture<B>>>,
    texture_view: ManuallyDrop<B::TextureView>,
    destroyer: Arc<DeferredDestroyer<B>>
}

impl<B: GPUBackend> Drop for TextureView<B> {
    fn drop(&mut self) {
        let texture_view = unsafe { ManuallyDrop::take(&mut self.texture_view) };
        self.destroyer.destroy_texture_view(texture_view);
    }
}

impl<B: GPUBackend> TextureView<B> {
    pub(super) fn new(device: &Arc<B::Device>, destroyer: &Arc<DeferredDestroyer<B>>, texture: &Arc<Texture<B>>, info: &TextureViewInfo, name: Option<&str>) -> Self {
        let texture_view = unsafe { device.create_texture_view(texture.handle(), info, name) };
        Self {
            device: device.clone(),
            texture: Some(texture.clone()),
            texture_view: ManuallyDrop::new(texture_view),
            destroyer: destroyer.clone()
        }
    }

    pub(super) unsafe fn new_from_texture_handle(device: &Arc<B::Device>, destroyer: &Arc<DeferredDestroyer<B>>, texture: &B::Texture, info: &TextureViewInfo, name: Option<&str>) -> Self {
        let texture_view = unsafe { device.create_texture_view(texture, info, name) };
        Self {
            device: device.clone(),
            texture: None,
            texture_view: ManuallyDrop::new(texture_view),
            destroyer: destroyer.clone()
        }
    }

    pub(super) fn handle(&self) -> &B::TextureView {
        &*self.texture_view
    }
}
