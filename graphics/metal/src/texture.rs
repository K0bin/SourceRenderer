use metal;
use metal::foreign_types::ForeignType;

use sourcerenderer_core::gpu;

use super::*;

fn texture_dimensions_to_mtl(dimensions: gpu::TextureDimension, samples: gpu::SampleCount) -> metal::MTLTextureType {
    match dimensions {
        gpu::TextureDimension::Dim1D => metal::MTLTextureType::D1,
        gpu::TextureDimension::Dim1DArray => metal::MTLTextureType::D1Array,
        gpu::TextureDimension::Dim2D => if samples == gpu::SampleCount::Samples1 { metal::MTLTextureType::D2 } else { metal::MTLTextureType::D2Multisample },
        gpu::TextureDimension::Dim2DArray => if samples == gpu::SampleCount::Samples1 { metal::MTLTextureType::D2Array } else { metal::MTLTextureType::D2MultisampleArray },
        gpu::TextureDimension::Cube => metal::MTLTextureType::Cube,
        gpu::TextureDimension::CubeArray => metal::MTLTextureType::CubeArray,
        gpu::TextureDimension::Dim3D => metal::MTLTextureType::D3,
    }
}

fn address_mode_to_mtl(address_mode: gpu::AddressMode) -> metal::MTLSamplerAddressMode {
    match address_mode {
        gpu::AddressMode::Repeat => metal::MTLSamplerAddressMode::Repeat,
        gpu::AddressMode::MirroredRepeat => metal::MTLSamplerAddressMode::MirrorRepeat,
        gpu::AddressMode::ClampToEdge => metal::MTLSamplerAddressMode::ClampToEdge,
        gpu::AddressMode::ClampToBorder => metal::MTLSamplerAddressMode::ClampToBorderColor,
    }
}

fn filter_to_mtl_minmag(filter: gpu::Filter) -> metal::MTLSamplerMinMagFilter {
    match filter {
        gpu::Filter::Linear => metal::MTLSamplerMinMagFilter::Linear,
        gpu::Filter::Nearest => metal::MTLSamplerMinMagFilter::Nearest,
        gpu::Filter::Min => panic!("Metal does not support Min/Max filter."),
        gpu::Filter::Max => panic!("Metal does not support Min/Max filter."),
    }
}

fn filter_to_mtl_mip(filter: gpu::Filter) -> metal::MTLSamplerMipFilter {
    match filter {
        gpu::Filter::Linear => metal::MTLSamplerMipFilter::Linear,
        gpu::Filter::Nearest => metal::MTLSamplerMipFilter::Nearest,
        gpu::Filter::Min => panic!("Metal does not support Min/Max filter."),
        gpu::Filter::Max => panic!("Metal does not support Min/Max filter."),
    }
}

fn compare_op_to_mtl(compare_op: gpu::CompareFunc) -> metal::MTLCompareFunction {
    match compare_op {
        gpu::CompareFunc::Never => metal::MTLCompareFunction::Never,
        gpu::CompareFunc::Less => metal::MTLCompareFunction::Less,
        gpu::CompareFunc::LessEqual => metal::MTLCompareFunction::LessEqual,
        gpu::CompareFunc::Equal => metal::MTLCompareFunction::Equal,
        gpu::CompareFunc::NotEqual => metal::MTLCompareFunction::NotEqual,
        gpu::CompareFunc::GreaterEqual => metal::MTLCompareFunction::GreaterEqual,
        gpu::CompareFunc::Greater => metal::MTLCompareFunction::Greater,
        gpu::CompareFunc::Always => metal::MTLCompareFunction::Always,
    }
}

pub(crate) fn format_from_metal(format: metal::MTLPixelFormat) -> gpu::Format {
    match format {
        metal::MTLPixelFormat::RGBA8Unorm => gpu::Format::RGBA8UNorm,
        metal::MTLPixelFormat::RGBA16Float => gpu::Format::RGBA16Float,
        metal::MTLPixelFormat::BGRA8Unorm => gpu::Format::BGRA8UNorm,
        metal::MTLPixelFormat::RGBA8Unorm_sRGB => gpu::Format::RGBA8Srgb,
        _ => panic!("Unsupported texture format")
    }
}

pub struct MTLTexture {
    info: gpu::TextureInfo,
    texture: metal::Texture,
    is_texture_owned: bool
}

impl MTLTexture {
    pub(crate) fn descriptor(info: &gpu::TextureInfo) -> metal::TextureDescriptor {
        let descriptor = metal::TextureDescriptor::new();
        descriptor.set_texture_type(texture_dimensions_to_mtl(info.dimension, info.samples));
        descriptor.set_sample_count(match info.samples {
            gpu::SampleCount::Samples1 => 1,
            gpu::SampleCount::Samples2 => 2,
            gpu::SampleCount::Samples4 => 4,
            gpu::SampleCount::Samples8 => 8,
        });
        descriptor.set_mipmap_level_count(info.mip_levels as u64);
        descriptor.set_array_length(info.array_length as u64);
        descriptor.set_width(info.width as u64);
        descriptor.set_height(info.height as u64);
        descriptor.set_depth(info.depth as u64);
        descriptor.set_pixel_format(format_to_mtl(info.format));

        let mut usage = metal::MTLTextureUsage::empty();
        if info.usage.contains(gpu::TextureUsage::SAMPLED) {
            usage |= metal::MTLTextureUsage::ShaderRead;
        }
        if info.usage.contains(gpu::TextureUsage::STORAGE) {
            usage |= metal::MTLTextureUsage::ShaderRead | metal::MTLTextureUsage::ShaderWrite;
        }
        if info.usage.intersects(gpu::TextureUsage::RENDER_TARGET | gpu::TextureUsage::DEPTH_STENCIL) {
            usage |= metal::MTLTextureUsage::RenderTarget;
            descriptor.set_compression_type(metal::MTLTextureCompressionType::Lossless);
        }
        if info.supports_srgb {
            usage |= metal::MTLTextureUsage::PixelFormatView;
        }
        descriptor.set_usage(usage);
        // We dont need to call the setters for storage mode, caching or hazardtracking, those are taken from the resource options
        descriptor.set_resource_options(Self::resource_options(info));
        descriptor
    }

    pub(crate) fn new(memory: ResourceMemory, info: &gpu::TextureInfo, name: Option<&str>) -> Result<Self, gpu::OutOfMemoryError> {
        let descriptor = Self::descriptor(info);
        let mut options = descriptor.resource_options();

        let texture = match memory {
            ResourceMemory::Dedicated { device, options: memory_options } => {
                if info.usage.gpu_writable() {
                    options |= metal::MTLResourceOptions::HazardTrackingModeTracked;
                } else {
                    options |= metal::MTLResourceOptions::HazardTrackingModeUntracked;
                }
                descriptor.set_resource_options(options | memory_options);
                let texture = device.new_texture(&descriptor);
                if texture.as_ptr() == std::ptr::null_mut() {
                    return Err(gpu::OutOfMemoryError {});
                }
                texture
            }
            ResourceMemory::Suballocated { memory, offset } => {
                options |= metal::MTLResourceOptions::HazardTrackingModeUntracked;
                options |= memory.resource_options();
                descriptor.set_resource_options(options);
                let texture_opt = memory.handle().new_texture_with_offset(&descriptor, offset);
                if texture_opt.is_none() {
                    return Err(gpu::OutOfMemoryError {});
                }
                texture_opt.unwrap()
            }
        };

        if let Some(name) = name {
            texture.set_label(name);
        }

        Ok(Self {
            info: info.clone(),
            texture,
            is_texture_owned: true
        })
    }

    pub(crate) fn from_mtl_texture(texture: &metal::TextureRef, take_reference: bool) -> Self {
        let texture_owned = texture.to_owned();

        // I want to use the nice wrapper
        // but not always add to the reference count.
        // So just create a new wrapper off of the same pointer
        // and drop that to reduce the refcount by 1, counteracting
        // taking the new owned wrapper.

        if !take_reference {
            unsafe {
                let new_ref = metal::Texture::from_ptr(texture_owned.as_ptr());
                std::mem::drop(new_ref);
            }
        }

        let format = format_from_metal(texture.pixel_format());

        let mut usage = gpu::TextureUsage::empty();
        let mtl_usage = texture.usage();
        if mtl_usage.contains(metal::MTLTextureUsage::ShaderRead) {
            usage |= gpu::TextureUsage::SAMPLED;
        }
        if mtl_usage.contains(metal::MTLTextureUsage::ShaderWrite) {
            usage |= gpu::TextureUsage::STORAGE;
        }
        if mtl_usage.contains(metal::MTLTextureUsage::RenderTarget) {
            if format.is_depth() || format.is_stencil() {
                usage |= gpu::TextureUsage::DEPTH_STENCIL;
            } else {
                usage |= gpu::TextureUsage::RENDER_TARGET;
            }
        }

        if !texture.framebuffer_only() {
            usage |= gpu::TextureUsage::COPY_DST | gpu::TextureUsage::COPY_SRC;
        }

        let info = gpu::TextureInfo {
            width: texture.width() as u32,
            height: texture.height() as u32,
            depth: texture.depth() as u32,
            dimension: match texture.texture_type() {
                metal::MTLTextureType::D1 => gpu::TextureDimension::Dim1D,
                metal::MTLTextureType::D1Array => gpu::TextureDimension::Dim1DArray,
                metal::MTLTextureType::D2 => gpu::TextureDimension::Dim2D,
                metal::MTLTextureType::D2Array => gpu::TextureDimension::Dim2DArray,
                metal::MTLTextureType::D2Multisample => gpu::TextureDimension::Dim2D,
                metal::MTLTextureType::Cube => gpu::TextureDimension::Cube,
                metal::MTLTextureType::CubeArray => gpu::TextureDimension::CubeArray,
                metal::MTLTextureType::D3 => gpu::TextureDimension::Dim3D,
                metal::MTLTextureType::D2MultisampleArray => gpu::TextureDimension::Dim2DArray,
            },
            format,
            mip_levels: texture.mipmap_level_count() as u32,
            array_length: texture.array_length() as u32,
            samples: match texture.sample_count() {
                1 => gpu::SampleCount::Samples1,
                2 => gpu::SampleCount::Samples2,
                4 => gpu::SampleCount::Samples4,
                8 => gpu::SampleCount::Samples8,
                _ => panic!("Unsupported sample count")
            },
            usage,
            supports_srgb: mtl_usage.contains(metal::MTLTextureUsage::PixelFormatView),
        };

        Self {
            texture: texture_owned,
            info: info.clone(),
            is_texture_owned: take_reference
        }
    }

    pub(crate) fn resource_options(_info: &gpu::TextureInfo) -> metal::MTLResourceOptions {
        let options = metal::MTLResourceOptions::empty();
        options
    }

    pub(crate) fn handle(&self) -> &metal::TextureRef {
        &self.texture
    }
}

impl Drop for MTLTexture {
    fn drop(&mut self) {
        if !self.is_texture_owned {
            // Add 1 reference to counteract the upcoming drop reducing
            // the refcount by 1
            let new_ref = self.texture.clone();
            std::mem::forget(new_ref);
        }
    }
}

impl gpu::Texture for MTLTexture {
    fn info(&self) -> &gpu::TextureInfo {
        &self.info
    }

    unsafe fn can_be_written_directly(&self) -> bool {
        self.texture.storage_mode() != metal::MTLStorageMode::Private
    }
}

impl PartialEq<MTLTexture> for MTLTexture {
    fn eq(&self, other: &MTLTexture) -> bool {
        self.texture.as_ptr() == other.texture.as_ptr()
    }
}

impl Eq for MTLTexture {}

pub struct MTLTextureView {
    info: gpu::TextureViewInfo,
    texture_info: gpu::TextureInfo,
    view: metal::Texture
}

impl MTLTextureView {
    pub(crate) fn new(texture: &MTLTexture, info: &gpu::TextureViewInfo, name: Option<&str>) -> Self {
        let entire_texture = info.array_layer_length == texture.info.array_length
            && info.base_array_layer == 0
            && info.mip_level_length == texture.info.mip_levels
            && info.base_mip_level == 0;

        let view = if entire_texture && info.format.is_none() {
            texture.handle().to_owned()
        } else if entire_texture {
            texture.handle().new_texture_view(format_to_mtl(info.format.unwrap()))
        } else {
            texture.handle().new_texture_view_from_slice(
                format_to_mtl(info.format.unwrap_or(texture.info.format)),
                texture_dimensions_to_mtl(texture.info.dimension, texture.info.samples),
                metal::NSRange { 
                    location: info.base_mip_level as u64,
                    length: info.mip_level_length as u64
                }, metal::NSRange {
                    location: info.base_array_layer as u64,
                    length: info.array_layer_length as u64
                })
        };

        if let Some(name) = name {
            view.set_label(name);
        }

        Self {
            view,
            info: info.clone(),
            texture_info: texture.info.clone()
        }
    }

    pub(crate) fn handle(&self) -> &metal::TextureRef {
        &self.view
    }
}

impl gpu::TextureView for MTLTextureView {
    fn info(&self) -> &gpu::TextureViewInfo {
        &self.info
    }
    fn texture_info(&self) -> &gpu::TextureInfo {
        &self.texture_info
    }
}

impl PartialEq<MTLTextureView> for MTLTextureView {
    fn eq(&self, other: &MTLTextureView) -> bool {
        self.view.as_ptr() == other.view.as_ptr()
    }
}

impl Eq for MTLTextureView {}

pub struct MTLSampler {
    sampler: metal::SamplerState,
    info: gpu::SamplerInfo
}

impl MTLSampler {
    pub(crate) fn new(device: &metal::DeviceRef, info: &gpu::SamplerInfo) -> Self {
        let descriptor = metal::SamplerDescriptor::new();
        descriptor.set_address_mode_s(address_mode_to_mtl(info.address_mode_u));
        descriptor.set_address_mode_t(address_mode_to_mtl(info.address_mode_v));
        descriptor.set_address_mode_r(address_mode_to_mtl(info.address_mode_w));
        descriptor.set_min_filter(filter_to_mtl_minmag(info.min_filter));
        descriptor.set_mag_filter(filter_to_mtl_minmag(info.mag_filter));
        descriptor.set_mip_filter(filter_to_mtl_mip(info.mip_filter));
        descriptor.set_lod_average(true);
        descriptor.set_support_argument_buffers(false);
        descriptor.set_max_anisotropy(info.max_anisotropy as u64);
        if let Some(compare_op) = info.compare_op {
            descriptor.set_compare_function(compare_op_to_mtl(compare_op));
        }
        descriptor.set_lod_max_clamp(info.min_lod);
        if let Some(max) = info.max_lod {
            descriptor.set_lod_max_clamp(max);
        }
        let sampler = device.new_sampler(&descriptor);
        Self {
            sampler,
            info: info.clone()
        }
    }

    pub(crate) fn handle(&self) -> &metal::SamplerStateRef {
        &self.sampler
    }
}

impl gpu::Sampler for MTLSampler {
    fn info(&self) -> &gpu::SamplerInfo {
        &self.info
    }
}

