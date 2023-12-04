use std::io::{
    Cursor,
    Read,
};
use std::path::Path;
use std::sync::Arc;

use image::io::Reader as ImageReader;
use sourcerenderer_core::gpu::GPUBackend;
use sourcerenderer_core::graphics::{
    AddressMode,
    Backend,
    BufferUsage,
    Device,
    Filter,
    Format,
    MemoryUsage,
    SampleCount,
    SamplerInfo,
    TextureDimension,
    TextureInfo,
    TextureUsage,
    TextureViewInfo,
};
use sourcerenderer_core::platform::IO;
use sourcerenderer_core::Platform;

pub struct BlueNoise<B: GPUBackend> {
    frames: [Arc<B::TextureView>; 8],
    sampler: Arc<B::Sampler>,
}

impl<B: GPUBackend> BlueNoise<B> {
    pub fn new<P: Platform>(device: &Arc<crate::graphics::Device<B>>) -> Self {
        Self {
            frames: [
                Self::load_frame::<P>(device, 0),
                Self::load_frame::<P>(device, 1),
                Self::load_frame::<P>(device, 2),
                Self::load_frame::<P>(device, 3),
                Self::load_frame::<P>(device, 4),
                Self::load_frame::<P>(device, 5),
                Self::load_frame::<P>(device, 6),
                Self::load_frame::<P>(device, 7),
            ],
            sampler: device.create_sampler(&SamplerInfo {
                mag_filter: Filter::Nearest,
                min_filter: Filter::Nearest,
                mip_filter: Filter::Nearest,
                address_mode_u: AddressMode::Repeat,
                address_mode_v: AddressMode::Repeat,
                address_mode_w: AddressMode::Repeat,
                mip_bias: 0.0f32,
                max_anisotropy: 0.0f32,
                compare_op: None,
                min_lod: 0f32,
                max_lod: None,
            }),
        }
    }

    fn load_frame<P: Platform>(device: &Arc<crate::graphics::Device<B>>, index: u32) -> Arc<crate::graphics::TextureView<B>> {
        let path = Path::new("assets")
            .join(Path::new("bn"))
            .join(Path::new(&format!("LDR_RGB1_{}.png", index)));
        let mut file = P::IO::open_asset(&path)
            .unwrap_or_else(|e| panic!("Failed to open {:?}: {:?}", &path, e));
        let mut buffer = Vec::<u8>::new();
        file.read_to_end(&mut buffer).unwrap();

        let img = ImageReader::with_format(Cursor::new(buffer), image::ImageFormat::Png)
            .decode()
            .unwrap();
        let rgba_data = img.into_rgba8().to_vec();

        let texture = device.create_texture(
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RGBA8UNorm,
                width: 128,
                height: 128,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::COPY_DST | TextureUsage::SAMPLED | TextureUsage::STORAGE,
                supports_srgb: false,
            },
            Some(&format!("STBlueNoise{}", index)),
        );
        let buffer = device.upload_data(
            &rgba_data[..],
            MemoryUsage::UncachedRAM,
            BufferUsage::COPY_SRC,
        );
        device.init_texture(&texture, &buffer, 0, 0);

        device.create_texture_view(
            &texture,
            &TextureViewInfo::default(),
            Some(&format!("STBlueNoiseUAV{}", index)),
        )
    }

    pub fn frame(&self, index: u64) -> &Arc<crate::graphics::TextureView<B>> {
        &self.frames[(index % (self.frames.len() as u64)) as usize]
    }

    pub fn sampler(&self) -> &Arc<crate::graphics::Sampler<B>> {
        &self.sampler
    }
}
