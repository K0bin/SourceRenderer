use std::{mem::ManuallyDrop, sync::Arc};

use sourcerenderer_core::gpu::{self, Texture as _, Device as _, Heap as _};

use super::*;

pub struct Texture<B: GPUBackend> {
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
        let (texture, allocation) = if heap_info.dedicated_allocation_preference == gpu::DedicatedAllocationPreference::RequireDedicated || heap_info.dedicated_allocation_preference == gpu::DedicatedAllocationPreference::PreferDedicated {
            let memory_types = unsafe { device.memory_type_infos() };
            let mut texture: Result<B::Texture, OutOfMemoryError> = Err(OutOfMemoryError {});

            let mask = allocator.find_memory_type_mask(MemoryUsage::GPUMemory, MemoryTypeMatchingStrictness::Strict) & heap_info.memory_type_mask;
            for i in 0..memory_types.len() as u32 {
                if (mask & (1 << i)) == 0 {
                    continue;
                }
                texture = unsafe { device.create_texture(info, i, name) };
                if texture.is_ok() {
                    break;
                }
            }

            if texture.is_err() {
                let mask = allocator.find_memory_type_mask(MemoryUsage::GPUMemory, MemoryTypeMatchingStrictness::Normal) & heap_info.memory_type_mask;
                for i in 0..memory_types.len() as u32 {
                    if (mask & (1 << i)) == 0 {
                        continue;
                    }
                    texture = unsafe { device.create_texture(info, i, name) };
                    if texture.is_ok() {
                        break;
                    }
                }
            }

            if texture.is_err() {
                let mask = allocator.find_memory_type_mask(MemoryUsage::GPUMemory, MemoryTypeMatchingStrictness::Fallback) & heap_info.memory_type_mask;
                for i in 0..memory_types.len() as u32 {
                    if (mask & (1 <<i)) == 0 {
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
            let texture = unsafe { allocation.as_ref().data().create_texture(info, allocation.as_ref().range.offset, name) }?;
            (texture, Some(allocation))
        };
        Ok(Arc::new(Self {
            texture: ManuallyDrop::new(texture),
            allocation,
            destroyer: destroyer.clone()
        }))
    }

    #[allow(unused)]
    pub(super) fn new_from_handle(device: &Arc<B::Device>, destroyer: &Arc<DeferredDestroyer<B>>, handle: B::Texture) -> Result<Arc<Self>, OutOfMemoryError> {
        Ok(Arc::new(Self {
            texture: ManuallyDrop::new(handle),
            allocation: None,
            destroyer: destroyer.clone()
        }))
    }

    #[inline(always)]
    pub(crate) fn handle(&self) -> &B::Texture {
        &self.texture
    }

    #[inline(always)]
    pub fn info(&self) -> &TextureInfo {
        self.texture.info()
    }
}

impl<B: GPUBackend> PartialEq<Texture<B>> for Texture<B> {
    fn eq(&self, other: &Texture<B>) -> bool {
        self.texture == other.texture
    }
}

pub struct TextureView<B: GPUBackend> {
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
    pub(super) fn new(device: &Arc<B::Device>, destroyer: &Arc<DeferredDestroyer<B>>, texture: &Arc<Texture<B>>, info: &TextureViewInfo, name: Option<&str>) -> Arc<Self> {
        let texture_view = unsafe { device.create_texture_view(texture.handle(), info, name) };
        Arc::new(Self {
            texture: Some(texture.clone()),
            texture_view: ManuallyDrop::new(texture_view),
            destroyer: destroyer.clone()
        })
    }

    pub(super) unsafe fn new_from_texture_handle(device: &Arc<B::Device>, destroyer: &Arc<DeferredDestroyer<B>>, texture: &B::Texture, info: &TextureViewInfo, name: Option<&str>) -> Self {
        let texture_view = unsafe { device.create_texture_view(texture, info, name) };
        Self {
            texture: None,
            texture_view: ManuallyDrop::new(texture_view),
            destroyer: destroyer.clone()
        }
    }

    #[inline(always)]
    pub(super) fn handle(&self) -> &B::TextureView {
        &*self.texture_view
    }

    #[inline(always)]
    pub fn texture(&self) -> Option<&Arc<Texture<B>>> {
        self.texture.as_ref()
    }
}

impl<B: GPUBackend> PartialEq<TextureView<B>> for TextureView<B> {
    fn eq(&self, other: &TextureView<B>) -> bool {
        self.handle() == other.handle()
    }
}
