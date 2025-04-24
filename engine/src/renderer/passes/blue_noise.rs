use std::io::Cursor;
use std::sync::Arc;

#[allow(deprecated)]
use image::io::Reader as ImageReader;

use crate::graphics::{
    Device,
    SamplerInfo,
    TextureInfo,
    *,
};

pub struct BlueNoise {
    frames: [Arc<TextureView>; 8],
    sampler: Arc<Sampler>,
}

impl BlueNoise {
    pub fn new(device: &Arc<Device>) -> Self {
        Self {
            frames: [
                Self::load_frame(device, 0),
                Self::load_frame(device, 1),
                Self::load_frame(device, 2),
                Self::load_frame(device, 3),
                Self::load_frame(device, 4),
                Self::load_frame(device, 5),
                Self::load_frame(device, 6),
                Self::load_frame(device, 7),
            ],
            sampler: Arc::new(device.create_sampler(&SamplerInfo {
                mag_filter: Filter::Nearest,
                min_filter: Filter::Nearest,
                mip_filter: Filter::Nearest,
                address_mode_u: AddressMode::Repeat,
                address_mode_v: AddressMode::Repeat,
                address_mode_w: AddressMode::Repeat,
                mip_bias: 0.0f32,
                max_anisotropy: 1f32,
                compare_op: None,
                min_lod: 0f32,
                max_lod: None,
            })),
        }
    }

    #[allow(unused)]
    #[allow(deprecated)]
    fn load_frame(device: &Arc<Device>, index: u32) -> Arc<TextureView> {
        /*let path = Path::new("assets")
            .join(Path::new("bn"))
            .join(Path::new(&format!("LDR_RGB1_{}.png", index)));
        let mut file = P::IO::open_asset(&path)
            .unwrap_or_else(|e| panic!("Failed to open {:?}: {:?}", &path, e));*/
        let buffer = Vec::<u8>::new();
        //file.read_to_end(&mut buffer).unwrap();

        let img = ImageReader::with_format(Cursor::new(buffer), image::ImageFormat::Png)
            .decode()
            .unwrap();
        let rgba_data = img.into_rgba8().to_vec();

        let dev = device.as_ref() as &crate::graphics::Device;

        let texture = dev
            .create_texture(
                &TextureInfo {
                    dimension: TextureDimension::Dim2D,
                    format: Format::RGBA8UNorm,
                    width: 128,
                    height: 128,
                    depth: 1,
                    mip_levels: 1,
                    array_length: 1,
                    samples: SampleCount::Samples1,
                    usage: TextureUsage::INITIAL_COPY
                        | TextureUsage::SAMPLED
                        | TextureUsage::STORAGE,
                    supports_srgb: false,
                },
                Some(&format!("STBlueNoise{}", index)),
            )
            .unwrap();

        dev.init_texture(&rgba_data[..], &texture, 0, 0).unwrap();

        dev.create_texture_view(
            &texture,
            &TextureViewInfo::default(),
            Some(&format!("STBlueNoiseUAV{}", index)),
        )
    }

    pub fn frame(&self, index: u64) -> &Arc<crate::graphics::TextureView> {
        &self.frames[(index % (self.frames.len() as u64)) as usize]
    }

    pub fn sampler(&self) -> &Arc<crate::graphics::Sampler> {
        &self.sampler
    }
}
