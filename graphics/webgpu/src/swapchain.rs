use std::sync::atomic::{AtomicU32, Ordering};

use smallvec::SmallVec;
use sourcerenderer_core::{gpu::{Backbuffer, Format, SampleCount, Swapchain, SwapchainError, Texture, TextureDimension, TextureInfo, TextureUsage}, Matrix4};
use web_sys::{GpuDevice, GpuTexture, GpuTextureFormat};

use crate::{buffer, surface::WebGPUSurface, texture::WebGPUTexture, WebGPUBackend};

pub struct WebGPUBackbuffer{
    texture: WebGPUTexture,
    key: u32
}

impl Backbuffer for WebGPUBackbuffer {
    fn key(&self) -> u64 {
        self.key as u64
    }
}

pub struct WebGPUSwapchain {
    device: GpuDevice,
    surface: WebGPUSurface,
    backbuffer_counter: u32
}

unsafe impl Send for WebGPUSwapchain {}
unsafe impl Sync for WebGPUSwapchain {}

impl WebGPUSwapchain {
    pub fn new(device: &GpuDevice, surface: WebGPUSurface) -> Self {
        Self {
            device: device.clone(),
            surface,
            backbuffer_counter: 0u32
        }
    }

    fn read_backbuffer_key(&mut self, texture: &GpuTexture) -> u32 {
        // This is completely terrible.
        let label = texture.label();
        const PREFIX: &'static str = "Backbuffer ";
        if label.is_empty() {
            let key = self.backbuffer_counter;
            texture.set_label(&format!("{}{:05}", PREFIX, key));
            self.backbuffer_counter += 1;
            return key;
        }

        let key_res = label[PREFIX.len()..].parse::<u32>();
        key_res.expect("Texture is not a backbuffer or was named somewhere else.")
    }
}

impl Swapchain<WebGPUBackend> for WebGPUSwapchain {
    type Backbuffer = WebGPUBackbuffer;

    unsafe fn recreate(&mut self) {}

    unsafe fn next_backbuffer(&mut self) -> Result<WebGPUBackbuffer, SwapchainError> {
        let web_texture = self.surface.canvas_context().get_current_texture()
            .map_err(|_e| SwapchainError::Other)?;
        let key = self.read_backbuffer_key(&web_texture);
        let texture = WebGPUTexture::from_texture(&self.device, web_texture);
        Ok(WebGPUBackbuffer {
            texture,
            key
        })
    }

    unsafe fn texture_for_backbuffer<'a>(&'a self, backbuffer: &'a WebGPUBackbuffer) -> &'a WebGPUTexture {
        &backbuffer.texture
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
