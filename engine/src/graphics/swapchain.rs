use std::collections::HashMap;
use std::sync::Arc;

use sourcerenderer_core::gpu::Backbuffer as _;
use sourcerenderer_core::Matrix4;

use super::*;

pub struct Swapchain {
    device: Arc<active_gpu_backend::Device>,
    destroyer: Arc<DeferredDestroyer>,
    swapchain: active_gpu_backend::Swapchain,
    views: HashMap<u64, Arc<super::TextureView>>,
    recreation_count: u32,
}

impl Swapchain {
    pub fn new(swapchain: active_gpu_backend::Swapchain, device: &Device) -> Self {
        Self {
            swapchain,
            destroyer: device.destroyer().clone(),
            device: device.handle().clone(),
            views: HashMap::new(),
            recreation_count: 0u32,
        }
    }

    #[inline(always)]
    pub fn format(&self) -> Format {
        self.swapchain.format()
    }

    #[inline(always)]
    pub fn surface(&self) -> &active_gpu_backend::Surface {
        self.swapchain.surface()
    }

    pub fn recreate(&mut self) {
        unsafe {
            self.swapchain.recreate();
        }
        self.views.clear();
        self.recreation_count += 1;
    }

    pub fn backbuffer_view(
        &self,
        backbuffer: &active_gpu_backend::Backbuffer,
    ) -> Arc<super::TextureView> {
        if self.swapchain.will_reuse_backbuffers() {
            self.views.get(&backbuffer.key()).unwrap().clone()
        } else {
            unsafe {
                let texture = self.swapchain.texture_for_backbuffer(backbuffer);
                Arc::new(super::TextureView::new_from_texture_handle(
                    &self.device,
                    &self.destroyer,
                    texture,
                    &TextureViewInfo::default(),
                    None,
                ))
            }
        }
    }

    pub fn ensure_backbuffer_view(&mut self, backbuffer: &active_gpu_backend::Backbuffer) {
        let key = backbuffer.key();
        self.views.entry(key).or_insert_with(|| unsafe {
            let texture = self.swapchain.texture_for_backbuffer(backbuffer);
            Arc::new(super::TextureView::new_from_texture_handle(
                &self.device,
                &self.destroyer,
                texture,
                &TextureViewInfo::default(),
                None,
            ))
        });
    }

    pub fn backbuffer_handle<'a>(
        &'a self,
        backbuffer: &'a active_gpu_backend::Backbuffer,
    ) -> &'a active_gpu_backend::Texture {
        unsafe { self.swapchain.texture_for_backbuffer(backbuffer) }
    }

    pub fn next_backbuffer(
        &mut self,
    ) -> Result<Arc<active_gpu_backend::Backbuffer>, SwapchainError> {
        let backbuffer = unsafe { self.swapchain.next_backbuffer()? };
        if self.swapchain.will_reuse_backbuffers() {
            self.ensure_backbuffer_view(&backbuffer);
        }
        Ok(Arc::new(backbuffer))
    }

    #[inline(always)]
    pub fn transform(&self) -> Matrix4 {
        self.swapchain.transform()
    }

    #[inline(always)]
    pub fn width(&self) -> u32 {
        self.swapchain.width()
    }

    #[inline(always)]
    pub fn height(&self) -> u32 {
        self.swapchain.height()
    }

    #[inline(always)]
    pub fn handle(&self) -> &active_gpu_backend::Swapchain {
        &self.swapchain
    }

    #[inline(always)]
    pub fn handle_mut(&mut self) -> &mut active_gpu_backend::Swapchain {
        &mut self.swapchain
    }
}
