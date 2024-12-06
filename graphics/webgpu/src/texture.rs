use std::hash::Hash;
use sourcerenderer_core::gpu::{Format, SampleCount, Texture, TextureDimension, TextureInfo, TextureUsage};
use web_sys::{js_sys, wasm_bindgen::JsValue, GpuDevice, GpuExtent3dDict, GpuTexture, GpuTextureDescriptor, GpuTextureFormat};

pub(crate) fn format_to_webgpu(format: Format) -> GpuTextureFormat {
    match format {
        Format::Unknown => GpuTextureFormat::__Invalid,
        Format::R32UNorm => panic!("Unsupported format"),
        Format::R16UNorm => panic!("Unsupported format"),
        Format::R8Unorm => GpuTextureFormat::R8unorm,
        Format::RGBA8UNorm => GpuTextureFormat::Rgba8unorm,
        Format::RGBA8Srgb => GpuTextureFormat::Rgba8unormSrgb,
        Format::BGR8UNorm => panic!("Unsupported format"),
        Format::BGRA8UNorm => GpuTextureFormat::Bgra8unorm,
        Format::BC1 => GpuTextureFormat::Bc1RgbaUnorm,
        Format::BC1Alpha => GpuTextureFormat::Bc1RgbaUnorm,
        Format::BC2 => GpuTextureFormat::Bc2RgbaUnorm,
        Format::BC3 => GpuTextureFormat::Bc3RgbaUnorm,
        Format::R16Float => GpuTextureFormat::R16float,
        Format::R32Float => GpuTextureFormat::R32float,
        Format::RG32Float => GpuTextureFormat::Rg32float,
        Format::RG16Float => GpuTextureFormat::Rg16float,
        Format::RGB32Float => panic!("Unsupported format"),
        Format::RGBA32Float => GpuTextureFormat::Rgba32float,
        Format::RG16UNorm => panic!("Unsupported format"),
        Format::RG8UNorm => GpuTextureFormat::Rg8unorm,
        Format::R32UInt => GpuTextureFormat::R32uint,
        Format::RGBA16Float => GpuTextureFormat::Rgba16float,
        Format::R11G11B10Float => panic!("Unsupported format"),
        Format::RG16UInt => GpuTextureFormat::Rg16uint,
        Format::RG16SInt => GpuTextureFormat::Rg16sint,
        Format::R16UInt => GpuTextureFormat::R16uint,
        Format::R16SNorm => panic!("Unsupported format"),
        Format::R16SInt => GpuTextureFormat::R16sint,
        Format::D16 => GpuTextureFormat::Depth16unorm,
        Format::D16S8 => GpuTextureFormat::Depth24plusStencil8,
        Format::D32 => GpuTextureFormat::Depth32float,
        Format::D32S8 => GpuTextureFormat::Depth32floatStencil8,
        Format::D24S8 => GpuTextureFormat::Depth24plusStencil8,
    }
}

pub struct WebGPUTexture {
    texture: GpuTexture,
    info: TextureInfo
}

impl PartialEq for WebGPUTexture {
    fn eq(&self, other: &Self) -> bool {
        self.texture == other.texture
    }
}

impl Eq for WebGPUTexture {}

impl Hash for WebGPUTexture {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let ptr_val: usize = unsafe { std::mem::transmute(&self.texture as *const GpuTexture) };
        ptr_val.hash(state);
    }
}

unsafe impl Send for WebGPUTexture {}
unsafe impl Sync for WebGPUTexture {}

impl WebGPUTexture {
    pub fn new(device: &GpuDevice, info: &TextureInfo, name: Option<&str>) -> Result<Self, ()> {

        let size = GpuExtent3dDict::new(info.width);
        if info.dimension != TextureDimension::Dim1D && info.dimension != TextureDimension::Dim1DArray {
            size.set_height(info.height);
        }
        size.set_depth_or_array_layers(if info.dimension == TextureDimension::Dim3D { info.depth } else { info.array_length });
        let mut usage = 0u32;
        if info.usage.contains(TextureUsage::SAMPLED) {
            usage |= web_sys::gpu_texture_usage::TEXTURE_BINDING;
        }
        if info.usage.intersects(TextureUsage::RENDER_TARGET | TextureUsage::DEPTH_STENCIL) {
            usage |= web_sys::gpu_texture_usage::RENDER_ATTACHMENT;
        }
        if info.usage.contains(TextureUsage::STORAGE) {
            usage |= web_sys::gpu_texture_usage::STORAGE_BINDING;
        }
        if info.usage.intersects(TextureUsage::COPY_DST | TextureUsage::INITIAL_COPY | TextureUsage::BLIT_DST) {
            usage |= web_sys::gpu_texture_usage::COPY_DST;
        }
        if info.usage.intersects(TextureUsage::COPY_SRC | TextureUsage::BLIT_SRC) {
            usage |= web_sys::gpu_texture_usage::COPY_SRC;
        }
        if info.usage.contains(TextureUsage::RESOLVE_SRC) {
            usage |= web_sys::gpu_texture_usage::COPY_SRC;
        }
        if info.usage.contains(TextureUsage::RESOLVE_DST) {
            usage |= web_sys::gpu_texture_usage::COPY_DST;
        }
        let descriptor = GpuTextureDescriptor::new(format_to_webgpu(info.format), &JsValue::from(&size), usage);
        descriptor.set_mip_level_count(info.mip_levels);
        descriptor.set_sample_count(match info.samples {
            SampleCount::Samples1 => 1,
            SampleCount::Samples2 => 2,
            SampleCount::Samples4 => 4,
            SampleCount::Samples8 => 8,
        });
        descriptor.set_dimension(match info.dimension {
            TextureDimension::Dim1D | TextureDimension::Dim1DArray => web_sys::GpuTextureDimension::N1d,
            TextureDimension::Dim2D | TextureDimension::Dim2DArray => web_sys::GpuTextureDimension::N2d,
            TextureDimension::Dim3D => web_sys::GpuTextureDimension::N3d,
        });
        if let Some(name) = name {
            descriptor.set_label(name);
        }

        let srgb_format = info.supports_srgb.then_some(true).and_then(|_| info.format.srgb_format());
        if let Some(srgb_format) = srgb_format {
            let formats_array = js_sys::Array::new_with_length(2);
            formats_array.set(0, JsValue::from(format_to_webgpu(info.format)));
            formats_array.set(1, JsValue::from(format_to_webgpu(srgb_format)));
            descriptor.set_view_formats(&JsValue::from(formats_array));
        } else {
            let formats_array = js_sys::Array::new_with_length(1);
            formats_array.set(0, JsValue::from(format_to_webgpu(info.format)));
            descriptor.set_view_formats(&JsValue::from(formats_array));
        }
        let texture = device.create_texture(&descriptor).map_err(|_| ())?;

        Ok(Self {
            texture,
            info: info.clone()
        })
    }

    pub fn from_texture(_device: &GpuDevice, texture: GpuTexture, info: &TextureInfo) -> Self {
        Self {
            texture,
            info: info.clone()
        }
    }
}

impl Texture for WebGPUTexture {
    fn info(&self) -> &TextureInfo {
        &self.info
    }
}
