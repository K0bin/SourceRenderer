use std::sync::atomic::{AtomicU32, Ordering};

use smallvec::SmallVec;
use sourcerenderer_core::{gpu::{Format, SampleCount, Swapchain, SwapchainError, Texture, TextureDimension, TextureInfo, TextureUsage}, Matrix4};
use web_sys::{GpuDevice, GpuTexture, GpuTextureFormat};

use crate::{buffer, surface::WebGPUSurface, texture::WebGPUTexture, WebGPUBackend};

pub struct WebGPUSwapchain {
    device: GpuDevice,
    surface: WebGPUSurface,
    backbuffers: SmallVec::<[WebGPUTexture; 5]>,
    index: AtomicU32
}

impl WebGPUSwapchain {
    pub fn new(device: &GpuDevice, surface: WebGPUSurface, buffer_count: u32) -> Self {
        let backbuffers = Self::create_backbuffers(device, surface.texture_info(), buffer_count);
        Self {
            device: device.clone(),
            surface,
            backbuffers,
            index: AtomicU32::new(0)
        }
    }

    fn create_backbuffers(device: &GpuDevice, info: &TextureInfo, buffer_count: u32) -> SmallVec<[WebGPUTexture; 5]> {
        let mut buffers: SmallVec<[WebGPUTexture; 5]> = SmallVec::<[WebGPUTexture; 5]>::with_capacity(buffer_count as usize);
        for i in 0..buffer_count {
            buffers.push(WebGPUTexture::new(device, &info, Some(&format!("Backbuffer {}", i))).unwrap());
        }
        buffers
    }

    pub(crate) fn get_current_texture(&self) -> Result<GpuTexture, ()> {
        self.surface.canvas_context().get_current_texture().map_err(|_| ())
    }
}

impl Swapchain<WebGPUBackend> for WebGPUSwapchain {
    unsafe fn recreate(old: Self, width: u32, height: u32) -> Result<Self, SwapchainError> {
        let info = TextureInfo {
            width, height, ..old.surface.texture_info().clone()
        };
        let backbuffers = Self::create_backbuffers(&old.device, &info, old.backbuffers.len() as u32);
        Ok(Self {
            device: old.device,
            surface: old.surface,
            backbuffers,
            index: AtomicU32::new(0)
        })
    }

    unsafe fn recreate_on_surface(old: Self, surface: WebGPUSurface, width: u32, height: u32) -> Result<Self, SwapchainError> {
        let info = TextureInfo {
            width, height, ..surface.texture_info().clone()
        };
        let backbuffers = Self::create_backbuffers(&old.device, &info, old.backbuffers.len() as u32);
        Ok(Self {
            device: old.device,
            surface: surface,
            backbuffers,
            index: AtomicU32::new(0)
        })
    }

    unsafe fn next_backbuffer(&self) -> Result<(), SwapchainError> {
        self.surface.canvas_context().get_current_texture();
        self.index.fetch_update(Ordering::Release, Ordering::Acquire, |val| Some((val + 1) % (self.backbuffers.len() as u32)));
        Ok(())
    }

    fn backbuffer(&self, index: u32) -> &WebGPUTexture {
        &self.backbuffers[index as usize]
    }

    fn backbuffer_index(&self) -> u32 {
        self.index.load(Ordering::SeqCst)
    }

    fn backbuffer_count(&self) -> u32 {
        self.backbuffers.len() as u32
    }

    fn sample_count(&self) -> SampleCount {
        self.surface.texture_info().samples
    }

    fn format(&self) -> Format {
        self.surface.texture_info().format
    }

    fn surface(&self) -> &WebGPUSurface {
        &self.surface
    }

    fn transform(&self) -> Matrix4 {
        Matrix4::IDENTITY
    }

    fn width(&self) -> u32 {
        self.surface.texture_info().width
    }

    fn height(&self) -> u32 {
        self.surface.texture_info().height
    }
}
