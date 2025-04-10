use std::hash::Hash;
use sourcerenderer_core::gpu::{self, Texture as _};
use web_sys::{js_sys, wasm_bindgen::JsValue, GpuDevice, GpuExtent3dDict, GpuTexture, GpuTextureDescriptor, GpuTextureFormat, GpuTextureView, GpuTextureViewDescriptor, GpuTextureViewDimension};

pub(crate) fn format_to_webgpu(format: gpu::Format) -> GpuTextureFormat {
    match format {
        gpu::Format::Unknown => GpuTextureFormat::__Invalid,
        gpu::Format::R32UNorm => panic!("Unsupported format"),
        gpu::Format::R16UNorm => panic!("Unsupported format"),
        gpu::Format::R8Unorm => GpuTextureFormat::R8unorm,
        gpu::Format::RGBA8UNorm => GpuTextureFormat::Rgba8unorm,
        gpu::Format::RGBA8Srgb => GpuTextureFormat::Rgba8unormSrgb,
        gpu::Format::BGR8UNorm => panic!("Unsupported format"),
        gpu::Format::BGRA8UNorm => GpuTextureFormat::Bgra8unorm,
        gpu::Format::BC1 => GpuTextureFormat::Bc1RgbaUnorm,
        gpu::Format::BC1Alpha => GpuTextureFormat::Bc1RgbaUnorm,
        gpu::Format::BC2 => GpuTextureFormat::Bc2RgbaUnorm,
        gpu::Format::BC3 => GpuTextureFormat::Bc3RgbaUnorm,
        gpu::Format::R16Float => GpuTextureFormat::R16float,
        gpu::Format::R32Float => GpuTextureFormat::R32float,
        gpu::Format::RG32Float => GpuTextureFormat::Rg32float,
        gpu::Format::RG16Float => GpuTextureFormat::Rg16float,
        gpu::Format::RGB32Float => panic!("Unsupported format"),
        gpu::Format::RGBA32Float => GpuTextureFormat::Rgba32float,
        gpu::Format::RG16UNorm => panic!("Unsupported format"),
        gpu::Format::RG8UNorm => GpuTextureFormat::Rg8unorm,
        gpu::Format::R32UInt => GpuTextureFormat::R32uint,
        gpu::Format::RGBA16Float => GpuTextureFormat::Rgba16float,
        gpu::Format::R11G11B10Float => panic!("Unsupported format"),
        gpu::Format::RG16UInt => GpuTextureFormat::Rg16uint,
        gpu::Format::RG16SInt => GpuTextureFormat::Rg16sint,
        gpu::Format::R16UInt => GpuTextureFormat::R16uint,
        gpu::Format::R16SNorm => panic!("Unsupported format"),
        gpu::Format::R16SInt => GpuTextureFormat::R16sint,
        gpu::Format::D16 => GpuTextureFormat::Depth16unorm,
        gpu::Format::D16S8 => GpuTextureFormat::Depth24plusStencil8,
        gpu::Format::D32 => GpuTextureFormat::Depth32float,
        gpu::Format::D32S8 => GpuTextureFormat::Depth32floatStencil8,
        gpu::Format::D24S8 => GpuTextureFormat::Depth24plusStencil8,
    }
}

pub(crate) fn format_from_webgpu(format: GpuTextureFormat) -> gpu::Format {
    match format {
        GpuTextureFormat::Rgba8unorm => gpu::Format::RGBA8UNorm,
        GpuTextureFormat::Rgba8unormSrgb => gpu::Format::RGBA8Srgb,
        GpuTextureFormat::Bgra8unorm => gpu::Format::BGRA8UNorm,
        GpuTextureFormat::Rgba32float => gpu::Format::RGBA32Float,
        GpuTextureFormat::Rgba16float => gpu::Format::RGBA16Float,
        _ => todo!(),
    }
}

pub(crate) fn texture_dimension_to_webgpu_view(texture_dimension: gpu::TextureDimension) -> GpuTextureViewDimension {
    match texture_dimension {
        gpu::TextureDimension::Dim1D => GpuTextureViewDimension::N1d,
        gpu::TextureDimension::Dim2D => GpuTextureViewDimension::N2d,
        gpu::TextureDimension::Dim3D => GpuTextureViewDimension::N3d,
        gpu::TextureDimension::Dim1DArray => panic!("1D texture arrays are unsupported by WebGPU"),
        gpu::TextureDimension::Dim2DArray => GpuTextureViewDimension::N2dArray,
        gpu::TextureDimension::Cube => GpuTextureViewDimension::Cube,
        gpu::TextureDimension::CubeArray => GpuTextureViewDimension::CubeArray,
    }
}

pub struct WebGPUTexture {
    texture: GpuTexture,
    info: gpu::TextureInfo
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

impl Drop for WebGPUTexture {
    fn drop(&mut self) {
        self.texture.destroy();
    }
}

impl WebGPUTexture {
    pub fn new(device: &GpuDevice, info: &gpu::TextureInfo, name: Option<&str>) -> Result<Self, ()> {

        let size = GpuExtent3dDict::new(info.width);
        if info.dimension != gpu::TextureDimension::Dim1D && info.dimension != gpu::TextureDimension::Dim1DArray {
            size.set_height(info.height);
        }
        size.set_depth_or_array_layers(if info.dimension == gpu::TextureDimension::Dim3D { info.depth } else { info.array_length });
        let mut usage = 0u32;
        if info.usage.contains(gpu::TextureUsage::SAMPLED) {
            usage |= web_sys::gpu_texture_usage::TEXTURE_BINDING;
        }
        if info.usage.intersects(gpu::TextureUsage::RENDER_TARGET | gpu::TextureUsage::DEPTH_STENCIL) {
            usage |= web_sys::gpu_texture_usage::RENDER_ATTACHMENT;
        }
        if info.usage.contains(gpu::TextureUsage::STORAGE) {
            usage |= web_sys::gpu_texture_usage::STORAGE_BINDING;
        }
        if info.usage.intersects(gpu::TextureUsage::COPY_DST | gpu::TextureUsage::INITIAL_COPY | gpu::TextureUsage::BLIT_DST) {
            usage |= web_sys::gpu_texture_usage::COPY_DST;
        }
        if info.usage.intersects(gpu::TextureUsage::COPY_SRC | gpu::TextureUsage::BLIT_SRC) {
            usage |= web_sys::gpu_texture_usage::COPY_SRC;
        }
        if info.usage.contains(gpu::TextureUsage::RESOLVE_SRC) {
            usage |= web_sys::gpu_texture_usage::COPY_SRC;
        }
        if info.usage.contains(gpu::TextureUsage::RESOLVE_DST) {
            usage |= web_sys::gpu_texture_usage::COPY_DST;
        }
        let descriptor = GpuTextureDescriptor::new(format_to_webgpu(info.format), &JsValue::from(&size), usage);
        descriptor.set_mip_level_count(info.mip_levels);
        descriptor.set_sample_count(match info.samples {
            gpu::SampleCount::Samples1 => 1,
            gpu::SampleCount::Samples2 => 2,
            gpu::SampleCount::Samples4 => 4,
            gpu::SampleCount::Samples8 => 8,
        });
        descriptor.set_dimension(match info.dimension {
            gpu::TextureDimension::Dim1D | gpu::TextureDimension::Dim1DArray => web_sys::GpuTextureDimension::N1d,
            gpu::TextureDimension::Dim2D | gpu::TextureDimension::Dim2DArray | gpu::TextureDimension::Cube | gpu::TextureDimension::CubeArray => web_sys::GpuTextureDimension::N2d,
            gpu::TextureDimension::Dim3D => web_sys::GpuTextureDimension::N3d,
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

    pub fn from_texture(_device: &GpuDevice, texture: GpuTexture) -> Self {
        let format = format_from_webgpu(texture.format());
        let mut usage = gpu::TextureUsage::empty();
        let web_usage = texture.usage();
        if web_usage & web_sys::gpu_texture_usage::COPY_SRC != 0 {
            usage |= gpu::TextureUsage::COPY_SRC;
        }
        if web_usage & web_sys::gpu_texture_usage::COPY_DST != 0 {
            usage |= gpu::TextureUsage::COPY_DST;
        }
        if web_usage & web_sys::gpu_texture_usage::RENDER_ATTACHMENT != 0 {
            if format.is_depth() || format.is_stencil() {
                usage |= gpu::TextureUsage::DEPTH_STENCIL;
            } else {
                usage |= gpu::TextureUsage::RENDER_TARGET;
            }

        }
        if web_usage & web_sys::gpu_texture_usage::STORAGE_BINDING != 0 {
            usage |= gpu::TextureUsage::STORAGE;
        }
        if web_usage & web_sys::gpu_texture_usage::TEXTURE_BINDING != 0 {
            usage |= gpu::TextureUsage::SAMPLED;
        }

        let info = gpu::TextureInfo {
            width: texture.width(),
            height: texture.height(),
            dimension: match texture.dimension() {
                web_sys::GpuTextureDimension::N1d => gpu::TextureDimension::Dim1D,
                web_sys::GpuTextureDimension::N2d => gpu::TextureDimension::Dim2D,
                web_sys::GpuTextureDimension::N3d => gpu::TextureDimension::Dim3D,
                _ => todo!(),
            },
            depth: if texture.dimension() == web_sys::GpuTextureDimension::N3d {
                texture.depth_or_array_layers() as u32
            } else {
                1
            },
            array_length: if texture.dimension() == web_sys::GpuTextureDimension::N3d {
                1
            } else {
                texture.depth_or_array_layers() as u32
            },
            format: format_from_webgpu(texture.format()),
            mip_levels: texture.mip_level_count() as u32,
            samples: match texture.sample_count() {
                1 => gpu::SampleCount::Samples1,
                2 => gpu::SampleCount::Samples2,
                4 => gpu::SampleCount::Samples4,
                8 => gpu::SampleCount::Samples8,
                _ => panic!("Unsupported sample count")
            },
            usage,
            supports_srgb: false,
        };

        Self {
            texture,
            info
        }
    }

    #[inline(always)]
    pub fn handle(&self) -> &GpuTexture {
        &self.texture
    }
}

impl gpu::Texture for WebGPUTexture {
    fn info(&self) -> &gpu::TextureInfo {
        &self.info
    }

    unsafe fn can_be_written_directly(&self) -> bool {
        true
    }
}

pub struct WebGPUTextureView {
    view: GpuTextureView,
    texture_info: gpu::TextureInfo,
    info: gpu::TextureViewInfo
}

impl PartialEq for WebGPUTextureView {
    fn eq(&self, other: &Self) -> bool {
        self.view == other.view
    }
}

impl Eq for WebGPUTextureView {}

impl Hash for WebGPUTextureView {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let ptr_val: usize = unsafe { std::mem::transmute(&self.view as *const GpuTextureView) };
        ptr_val.hash(state);
    }
}

impl WebGPUTextureView {
    pub fn new(_device: &GpuDevice, texture: &WebGPUTexture, info: &gpu::TextureViewInfo, name: Option<&str>) -> Result<Self, ()> {
        let descriptor = GpuTextureViewDescriptor::new();
        descriptor.set_array_layer_count(info.array_layer_length);
        descriptor.set_base_array_layer(info.base_array_layer);
        descriptor.set_mip_level_count(info.mip_level_length);
        descriptor.set_base_mip_level(info.base_mip_level);
        descriptor.set_dimension(texture_dimension_to_webgpu_view(texture.info().dimension));
        if let Some(format) = info.format {
            descriptor.set_format(format_to_webgpu(format));
        }
        if let Some(name) = name {
            descriptor.set_label(name);
        }
        let view = texture.handle().create_view_with_descriptor(&descriptor).map_err(|_| ())?;
        Ok(Self {
            view,
            texture_info: texture.info().clone(),
            info: info.clone()
        })
    }

    #[inline(always)]
    pub fn handle(&self) -> &GpuTextureView {
        &self.view
    }
}

impl gpu::TextureView for WebGPUTextureView {
    fn texture_info(&self) -> &gpu::TextureInfo {
        &self.texture_info
    }

    fn info(&self) -> &gpu::TextureViewInfo {
        &self.info
    }
}
