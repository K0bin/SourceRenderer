
use sourcerenderer_core::{gpu::{Backbuffer, Format, Swapchain, SwapchainError}, Matrix4};
use web_sys::GpuDevice;

use crate::{surface::WebGPUSurface, texture::WebGPUTexture, WebGPUBackend};

pub struct WebGPUBackbuffer {
    texture: WebGPUTexture,
    key: u64
}

impl Backbuffer for WebGPUBackbuffer {
    fn key(&self) -> u64 {
        self.key
    }
}

pub struct WebGPUSwapchain {
    device: GpuDevice,
    surface: WebGPUSurface,
    backbuffer_counter: u64,
}

unsafe impl Send for WebGPUSwapchain {}
unsafe impl Sync for WebGPUSwapchain {}

impl WebGPUSwapchain {
    pub fn new(device: &GpuDevice, surface: WebGPUSurface) -> Self {
        Self {
            device: device.clone(),
            surface,
            backbuffer_counter: 0u64,
        }
    }
}

impl Swapchain<WebGPUBackend> for WebGPUSwapchain {
    type Backbuffer = WebGPUBackbuffer;

    unsafe fn recreate(&mut self) {}

    fn will_reuse_backbuffers(&self) -> bool {
        false
    }

    unsafe fn next_backbuffer(&mut self) -> Result<WebGPUBackbuffer, SwapchainError> {
        let web_texture = self.surface.canvas_context().get_current_texture()
            .map_err(|_e| SwapchainError::Other)?;

        let key = self.backbuffer_counter;
        self.backbuffer_counter += 1;
        let texture = WebGPUTexture::from_texture(&self.device, web_texture);
        let backbuffer = WebGPUBackbuffer {
            texture,
            key
        };

        Ok(backbuffer)
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
