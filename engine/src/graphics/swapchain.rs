use std::{collections::HashMap, sync::{Arc, Mutex}};

use smallvec::SmallVec;
use sourcerenderer_core::{gpu::{Backbuffer, Format, GPUBackend, SampleCount, Swapchain as GPUSwapchain, SwapchainError, TextureViewInfo}, Matrix4};

use super::{DeferredDestroyer, Device};

pub struct Swapchain<B: GPUBackend> {
    device: Arc<B::Device>,
    destroyer: Arc<DeferredDestroyer<B>>,
    swapchain: B::Swapchain,
    views: HashMap<u64, Arc<super::TextureView<B>>>,
    recreation_count: u32
}

impl<B: GPUBackend> Swapchain<B> {
    pub fn new(swapchain: B::Swapchain, device: &Device<B>) -> Self {
        Self {
            swapchain,
            destroyer: device.destroyer().clone(),
            device: device.handle().clone(),
            views: HashMap::new(),
            recreation_count: 0u32
        }
    }

    pub fn format(&self) -> Format {
        self.swapchain.format()
    }

    pub fn surface(&self) -> &B::Surface {
        self.swapchain.surface()
    }

    pub fn recreate(&mut self) {
        unsafe { self.swapchain.recreate(); }
        self.views.clear();
        self.recreation_count += 1;
    }

    pub fn backbuffer_view(&self, backbuffer: &<B::Swapchain as GPUSwapchain<B>>::Backbuffer) -> &Arc<super::TextureView<B>>{
        self.views.get(&backbuffer.key()).unwrap()
    }

    pub fn ensure_backbuffer_view(&mut self, backbuffer: &<B::Swapchain as GPUSwapchain<B>>::Backbuffer) {
        let key = backbuffer.key();
        self.views.entry(key).or_insert_with(|| {
            unsafe {
                let texture = self.swapchain.texture_for_backbuffer(backbuffer);
                Arc::new(
                    super::TextureView::new_from_texture_handle(
                        &self.device, &self.destroyer, texture,
                        &TextureViewInfo::default(), None
                    )
                )
            }
        });
    }

    pub fn backbuffer_handle(&self, backbuffer: &<B::Swapchain as GPUSwapchain<B>>::Backbuffer) -> &B::Texture {
        unsafe { self.swapchain.texture_for_backbuffer(backbuffer) }
    }

    pub fn next_backbuffer(&mut self) -> Result<Arc<<B::Swapchain as GPUSwapchain<B>>::Backbuffer>, SwapchainError> {
        let result = unsafe { self.swapchain.next_backbuffer() };
        if let Ok(backbuffer) = result.as_ref() {
            self.ensure_backbuffer_view(backbuffer);
        }
        result.map(|bb| Arc::new(bb))
    }

    pub fn transform(&self) -> Matrix4 {
        self.swapchain.transform()
    }

    pub fn width(&self) -> u32 {
        self.swapchain.width()
    }

    pub fn height(&self) -> u32 {
        self.swapchain.height()
    }

    pub fn handle(&self) -> &B::Swapchain {
        &self.swapchain
    }

    pub fn handle_mut(&mut self) -> &mut B::Swapchain {
        &mut self.swapchain
    }
}
