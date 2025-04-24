use std::mem::ManuallyDrop;
use std::sync::Arc;

use super::gpu::{
    self,
    Heap as _,
    Texture as _,
};
use super::*;

pub struct Texture {
    texture: ManuallyDrop<active_gpu_backend::Texture>,
    allocation: Option<MemoryAllocation<active_gpu_backend::Heap>>,
    destroyer: Arc<DeferredDestroyer>,
}

impl Drop for Texture {
    fn drop(&mut self) {
        let texture = unsafe { ManuallyDrop::take(&mut self.texture) };
        self.destroyer.destroy_texture(texture);
        if let Some(allocation) = self.allocation.take() {
            self.destroyer.destroy_allocation(allocation);
        }
    }
}

impl Texture {
    pub(super) fn new(
        device: &Arc<active_gpu_backend::Device>,
        allocator: &MemoryAllocator,
        destroyer: &Arc<DeferredDestroyer>,
        info: &TextureInfo,
        name: Option<&str>,
    ) -> Result<Arc<Self>, OutOfMemoryError> {
        let heap_info = unsafe { device.get_texture_heap_info(info) };
        let (texture, allocation) = if heap_info.dedicated_allocation_preference
            == gpu::DedicatedAllocationPreference::RequireDedicated
            || heap_info.dedicated_allocation_preference
                == gpu::DedicatedAllocationPreference::PreferDedicated
        {
            let memory_types = unsafe { device.memory_type_infos() };
            let mut texture: Result<active_gpu_backend::Texture, OutOfMemoryError> =
                Err(OutOfMemoryError {});

            let mask = allocator.find_memory_type_mask(
                MemoryUsage::GPUMemory,
                MemoryTypeMatchingStrictness::Strict,
            ) & heap_info.memory_type_mask;
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
                let mask = allocator.find_memory_type_mask(
                    MemoryUsage::GPUMemory,
                    MemoryTypeMatchingStrictness::Normal,
                ) & heap_info.memory_type_mask;
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
                let mask = allocator.find_memory_type_mask(
                    MemoryUsage::GPUMemory,
                    MemoryTypeMatchingStrictness::Fallback,
                ) & heap_info.memory_type_mask;
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
            (texture?, None)
        } else {
            let allocation = allocator.allocate(MemoryUsage::GPUMemory, &heap_info)?;
            let texture = unsafe {
                allocation.as_ref().data().create_texture(
                    info,
                    allocation.as_ref().range.offset,
                    name,
                )
            }?;
            (texture, Some(allocation))
        };
        Ok(Arc::new(Self {
            texture: ManuallyDrop::new(texture),
            allocation,
            destroyer: destroyer.clone(),
        }))
    }

    #[allow(unused)]
    pub(super) fn new_from_handle(
        device: &Arc<active_gpu_backend::Device>,
        destroyer: &Arc<DeferredDestroyer>,
        handle: active_gpu_backend::Texture,
    ) -> Result<Arc<Self>, OutOfMemoryError> {
        Ok(Arc::new(Self {
            texture: ManuallyDrop::new(handle),
            allocation: None,
            destroyer: destroyer.clone(),
        }))
    }

    #[inline(always)]
    pub(crate) fn handle(&self) -> &active_gpu_backend::Texture {
        &self.texture
    }

    #[inline(always)]
    pub fn info(&self) -> &TextureInfo {
        self.texture.info()
    }
}

impl PartialEq<Texture> for Texture {
    fn eq(&self, other: &Texture) -> bool {
        self.texture == other.texture
    }
}

pub struct TextureView {
    texture: Option<Arc<Texture>>,
    texture_view: ManuallyDrop<active_gpu_backend::TextureView>,
    destroyer: Arc<DeferredDestroyer>,
}

impl Drop for TextureView {
    fn drop(&mut self) {
        let texture_view = unsafe { ManuallyDrop::take(&mut self.texture_view) };
        self.destroyer.destroy_texture_view(texture_view);
    }
}

impl TextureView {
    pub(super) fn new(
        device: &Arc<active_gpu_backend::Device>,
        destroyer: &Arc<DeferredDestroyer>,
        texture: &Arc<Texture>,
        info: &TextureViewInfo,
        name: Option<&str>,
    ) -> Arc<Self> {
        let texture_view = unsafe { device.create_texture_view(texture.handle(), info, name) };
        Arc::new(Self {
            texture: Some(texture.clone()),
            texture_view: ManuallyDrop::new(texture_view),
            destroyer: destroyer.clone(),
        })
    }

    pub(super) unsafe fn new_from_texture_handle(
        device: &Arc<active_gpu_backend::Device>,
        destroyer: &Arc<DeferredDestroyer>,
        texture: &active_gpu_backend::Texture,
        info: &TextureViewInfo,
        name: Option<&str>,
    ) -> Self {
        let texture_view = unsafe { device.create_texture_view(texture, info, name) };
        Self {
            texture: None,
            texture_view: ManuallyDrop::new(texture_view),
            destroyer: destroyer.clone(),
        }
    }

    #[inline(always)]
    pub(super) fn handle(&self) -> &active_gpu_backend::TextureView {
        &*self.texture_view
    }

    #[inline(always)]
    pub fn texture(&self) -> Option<&Arc<Texture>> {
        self.texture.as_ref()
    }
}

impl PartialEq<TextureView> for TextureView {
    fn eq(&self, other: &TextureView) -> bool {
        self.handle() == other.handle()
    }
}
