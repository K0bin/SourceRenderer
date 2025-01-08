use std::sync::atomic::{AtomicU32, Ordering};

use smallvec::SmallVec;
use sourcerenderer_core::{gpu::{Backbuffer, Format, SampleCount, Swapchain, SwapchainError, Texture, TextureDimension, TextureInfo, TextureUsage}, Matrix4};
use web_sys::{GpuDevice, GpuTexture, GpuTextureFormat};

use crate::{buffer, surface::WebGPUSurface, texture::WebGPUTexture, WebGPUBackend};

pub struct WebGPUBackbuffer(u32);

impl Backbuffer for WebGPUBackbuffer {
    fn key(&self) -> u64 {
        self.0 as u64
    }
}

pub struct WebGPUSwapchain {
    device: GpuDevice,
    surface: WebGPUSurface,
    backbuffers: SmallVec::<[WebGPUTexture; 5]>,
    index: u32
}

unsafe impl Send for WebGPUSwapchain {}
unsafe impl Sync for WebGPUSwapchain {}

impl WebGPUSwapchain {
    pub fn new(device: &GpuDevice, surface: WebGPUSurface, buffer_count: u32) -> Self {
        let backbuffers = Self::create_backbuffers(device, surface.texture_info(), buffer_count);
        Self {
            device: device.clone(),
            surface,
            backbuffers,
            index: 0u32
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
    type Backbuffer = WebGPUBackbuffer;

    unsafe fn recreate(&mut self) {}

    unsafe fn next_backbuffer(&mut self) -> Result<WebGPUBackbuffer, SwapchainError> {
        self.surface.canvas_context().get_current_texture();
        self.index = (self.index + 1) % (self.backbuffers.len() as u32);
        Ok(WebGPUBackbuffer(self.index))
    }

    unsafe fn texture_for_backbuffer<'a>(&'a self, backbuffer: &'a WebGPUBackbuffer) -> &'a WebGPUTexture {
        &self.backbuffers[backbuffer.0 as usize]
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
