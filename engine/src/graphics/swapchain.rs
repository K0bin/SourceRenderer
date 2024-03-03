use std::sync::Arc;

use smallvec::SmallVec;
use sourcerenderer_core::{gpu::{GPUBackend, SwapchainError, Swapchain as _, Format, SampleCount, TextureViewInfo}, Matrix4};

use super::{DeferredDestroyer, Device};

pub struct Swapchain<B: GPUBackend> {
    device: Arc<B::Device>,
    destroyer: Arc<DeferredDestroyer<B>>,
    swapchain: B::Swapchain,
    views: SmallVec<[Arc<super::TextureView<B>>; 5]>
}

impl<B: GPUBackend> Swapchain<B> {
    pub fn new(swapchain: B::Swapchain, device: &Device<B>) -> Self {
        let views = Self::create_image_views(device.handle(), device.destroyer(), &swapchain);
        Self {
            swapchain,
            destroyer: device.destroyer().clone(),
            device: device.handle().clone(),
            views
        }
    }

    fn create_image_views(device: &Arc<B::Device>, destroyer: &Arc<DeferredDestroyer<B>>, swapchain: &B::Swapchain) -> SmallVec<[Arc<super::TextureView<B>>; 5]> {
        let count = swapchain.backbuffer_count();
        let mut views = SmallVec::<[Arc<super::TextureView<B>>; 5]>::with_capacity(count as usize);
        for i in 0..count {
            let name = format!("Backbuffer_{}", i);

            unsafe {
                views.push(
                    Arc::new(
                        super::TextureView::new_from_texture_handle(
                            device, destroyer, swapchain.backbuffer(i),
                            &TextureViewInfo::default(), Some(&name)
                        )
                    )
                );
            }
        }
        views
    }

    pub fn recreate(old: Self, width: u32, height: u32) -> Result<Self, SwapchainError> {
        let new_sc = unsafe {
            B::Swapchain::recreate(old.swapchain, width, height)
        }?;
        let views = Self::create_image_views(&old.device, &old.destroyer, &new_sc);

        Ok(Self {
            swapchain: new_sc,
            views,
            destroyer: old.destroyer.clone(),
            device: old.device.clone()
        })
    }

    pub fn recreate_on_surface(old: Self, surface: B::Surface, width: u32, height: u32) -> Result<Self, SwapchainError> {
        let new_sc = unsafe {
            B::Swapchain::recreate_on_surface(old.swapchain, surface, width, height)
        }?;
        let views = Self::create_image_views(&old.device, &old.destroyer, &new_sc);

        Ok(Self {
            swapchain: new_sc,
            views,
            destroyer: old.destroyer.clone(),
            device: old.device.clone()
        })
    }

    pub fn sample_count(&self) -> SampleCount {
        self.swapchain.sample_count()
    }

    pub fn format(&self) -> Format {
        self.swapchain.format()
    }

    pub fn surface(&self) -> &B::Surface {
        self.swapchain.surface()
    }

    pub fn next_backbuffer(&self) -> Result<(), SwapchainError> {
        unsafe { self.swapchain.next_backbuffer() }
    }

    pub fn backbuffer(&self) -> &Arc<super::TextureView<B>> {
        let idx = self.swapchain.backbuffer_index();
        &self.views[idx as usize]
    }

    pub fn backbuffer_handle(&self) -> &B::Texture {
        let idx = self.swapchain.backbuffer_index();
        self.swapchain.backbuffer(idx)
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
}
