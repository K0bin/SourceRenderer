use std::sync::atomic::{AtomicUsize, Ordering};

use metal::{self, MetalDrawable};
use metal::foreign_types::ForeignTypeRef;

use smallvec::SmallVec;
use sourcerenderer_core::gpu::{self, Texture};
use sourcerenderer_core::Matrix4;

use super::*;

pub struct MTLSurface {
    layer: metal::MetalLayer
}

impl MTLSurface {
    pub fn new(_instance: &MTLInstance, layer: &metal::MetalLayerRef) -> Self {
        Self {
            layer: layer.to_owned()
        }
    }

    pub(crate) fn handle(&self) -> &metal::MetalLayerRef {
        &self.layer
    }
}

impl PartialEq<MTLSurface> for MTLSurface {
    fn eq(&self, other: &MTLSurface) -> bool {
        self.layer.as_ptr() == other.layer.as_ptr()
    }
}

impl Eq for MTLSurface {}

pub struct MTLSwapchain {
    surface: MTLSurface,
    device: metal::Device,
    backbuffers: SmallVec<[MTLTexture; 5]>,
    current_backbuffer_index: AtomicUsize,
}

const IMAGE_COUNT: u32 = 3;

impl MTLSwapchain {
    pub fn new(surface: MTLSurface, device: &metal::DeviceRef, extends: Option<(u32, u32)>) -> Self {
        surface.layer.set_device(device);
        assert!(IMAGE_COUNT == 2 || IMAGE_COUNT == 3);
        surface.layer.set_maximum_drawable_count(IMAGE_COUNT as u64);
        let mut backbuffers = SmallVec::<[MTLTexture; 5]>::with_capacity(IMAGE_COUNT as usize);

        let width: u32;
        let height: u32;
        if let Some((param_width, param_height)) = extends {
            width = param_width;
            height = param_height;
        } else {
            width = surface.handle().drawable_size().width as u32;
            height = surface.handle().drawable_size().height as u32;
        }

        for i in 0..IMAGE_COUNT {
            let texture = MTLTexture::new(
                ResourceMemory::Dedicated { device: device, options: metal::MTLResourceOptions::StorageModePrivate },
                &gpu::TextureInfo {
                    dimension: gpu::TextureDimension::Dim2D,
                    format: gpu::Format::BGRA8UNorm,
                    width,
                    height,
                    depth: 1,
                    mip_levels: 1,
                    array_length: 1,
                    samples: gpu::SampleCount::Samples1,
                    usage: gpu::TextureUsage::RENDER_TARGET | gpu::TextureUsage::SAMPLED,
                    supports_srgb: false,
                }, Some(&format!("Backbuffer {}", i))).unwrap();
            backbuffers.push(texture);
        }
        Self {
            surface,
            backbuffers: backbuffers,
            device: device.to_owned(),
            current_backbuffer_index: AtomicUsize::new(0usize),
        }
    }

    pub(crate) fn take_drawable(&self) -> MetalDrawable {
        self.surface.layer.next_drawable().unwrap().to_owned()
    }
}

impl gpu::Swapchain<MTLBackend> for MTLSwapchain {
    unsafe fn recreate(old: Self, width: u32, height: u32) -> Result<Self, gpu::SwapchainError> {
        Ok(Self::new(old.surface, &old.device, Some((width, height))))
    }

    unsafe fn recreate_on_surface(old: Self, surface: MTLSurface, width: u32, height: u32) -> Result<Self, gpu::SwapchainError> {
        Ok(Self::new(surface, &old.device, Some((width, height))))
    }

    unsafe fn next_backbuffer(&self) -> Result<(), gpu::SwapchainError> {
        self.current_backbuffer_index.fetch_add(1, Ordering::AcqRel);
        return Ok(());
    }

    fn backbuffer(&self, index: u32) -> &MTLTexture {
        &self.backbuffers[index as usize]
    }

    fn backbuffer_index(&self) -> u32 {
        (self.current_backbuffer_index.load(Ordering::Acquire) % self.backbuffers.len()) as u32
    }

    fn backbuffer_count(&self) -> u32 {
        self.backbuffers.len() as u32
    }

    fn sample_count(&self) -> gpu::SampleCount {
        self.backbuffers.first().unwrap().info().samples
    }

    fn format(&self) -> gpu::Format {
        self.backbuffers.first().unwrap().info().format
    }

    fn surface(&self) -> &MTLSurface {
        &self.surface
    }

    fn transform(&self) -> Matrix4 {
        Matrix4::IDENTITY
    }

    fn width(&self) -> u32 {
        self.backbuffers.first().unwrap().info().width
    }

    fn height(&self) -> u32 {
        self.backbuffers.first().unwrap().info().height
    }
}
