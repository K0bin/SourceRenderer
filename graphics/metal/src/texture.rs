use objc2::ffi::NSUInteger;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::{NSRange, NSString};
use objc2_metal::{self, MTLDevice, MTLHeap, MTLResource as _, MTLTexture as _};

use sourcerenderer_core::gpu;

use super::*;

fn texture_dimensions_to_mtl(
    dimensions: gpu::TextureDimension,
    samples: gpu::SampleCount,
) -> objc2_metal::MTLTextureType {
    match dimensions {
        gpu::TextureDimension::Dim1D => objc2_metal::MTLTextureType::Type1D,
        gpu::TextureDimension::Dim1DArray => objc2_metal::MTLTextureType::Type1DArray,
        gpu::TextureDimension::Dim2D => {
            if samples == gpu::SampleCount::Samples1 {
                objc2_metal::MTLTextureType::Type2D
            } else {
                objc2_metal::MTLTextureType::Type2DMultisample
            }
        }
        gpu::TextureDimension::Dim2DArray => {
            if samples == gpu::SampleCount::Samples1 {
                objc2_metal::MTLTextureType::Type2DArray
            } else {
                objc2_metal::MTLTextureType::Type2DMultisampleArray
            }
        }
        gpu::TextureDimension::Cube => objc2_metal::MTLTextureType::TypeCube,
        gpu::TextureDimension::CubeArray => objc2_metal::MTLTextureType::TypeCubeArray,
        gpu::TextureDimension::Dim3D => objc2_metal::MTLTextureType::Type3D,
    }
}

fn address_mode_to_mtl(address_mode: gpu::AddressMode) -> objc2_metal::MTLSamplerAddressMode {
    match address_mode {
        gpu::AddressMode::Repeat => objc2_metal::MTLSamplerAddressMode::Repeat,
        gpu::AddressMode::MirroredRepeat => objc2_metal::MTLSamplerAddressMode::MirrorRepeat,
        gpu::AddressMode::ClampToEdge => objc2_metal::MTLSamplerAddressMode::ClampToEdge,
        gpu::AddressMode::ClampToBorder => objc2_metal::MTLSamplerAddressMode::ClampToBorderColor,
    }
}

fn filter_to_mtl_minmag(filter: gpu::Filter) -> objc2_metal::MTLSamplerMinMagFilter {
    match filter {
        gpu::Filter::Linear => objc2_metal::MTLSamplerMinMagFilter::Linear,
        gpu::Filter::Nearest => objc2_metal::MTLSamplerMinMagFilter::Nearest,
        gpu::Filter::Min => panic!("Metal does not support Min/Max filter."),
        gpu::Filter::Max => panic!("Metal does not support Min/Max filter."),
    }
}

fn filter_to_mtl_mip(filter: gpu::Filter) -> objc2_metal::MTLSamplerMipFilter {
    match filter {
        gpu::Filter::Linear => objc2_metal::MTLSamplerMipFilter::Linear,
        gpu::Filter::Nearest => objc2_metal::MTLSamplerMipFilter::Nearest,
        gpu::Filter::Min => panic!("Metal does not support Min/Max filter."),
        gpu::Filter::Max => panic!("Metal does not support Min/Max filter."),
    }
}

fn compare_op_to_mtl(compare_op: gpu::CompareFunc) -> objc2_metal::MTLCompareFunction {
    match compare_op {
        gpu::CompareFunc::Never => objc2_metal::MTLCompareFunction::Never,
        gpu::CompareFunc::Less => objc2_metal::MTLCompareFunction::Less,
        gpu::CompareFunc::LessEqual => objc2_metal::MTLCompareFunction::LessEqual,
        gpu::CompareFunc::Equal => objc2_metal::MTLCompareFunction::Equal,
        gpu::CompareFunc::NotEqual => objc2_metal::MTLCompareFunction::NotEqual,
        gpu::CompareFunc::GreaterEqual => objc2_metal::MTLCompareFunction::GreaterEqual,
        gpu::CompareFunc::Greater => objc2_metal::MTLCompareFunction::Greater,
        gpu::CompareFunc::Always => objc2_metal::MTLCompareFunction::Always,
    }
}

pub(crate) fn format_from_metal(format: objc2_metal::MTLPixelFormat) -> gpu::Format {
    match format {
        objc2_metal::MTLPixelFormat::RGBA8Unorm => gpu::Format::RGBA8UNorm,
        objc2_metal::MTLPixelFormat::RGBA16Float => gpu::Format::RGBA16Float,
        objc2_metal::MTLPixelFormat::BGRA8Unorm => gpu::Format::BGRA8UNorm,
        objc2_metal::MTLPixelFormat::RGBA8Unorm_sRGB => gpu::Format::RGBA8Srgb,
        _ => panic!("Unsupported texture format"),
    }
}

pub struct MTLTexture {
    info: gpu::TextureInfo,
    texture: Retained<ProtocolObject<dyn objc2_metal::MTLTexture>>,
}

unsafe impl Send for MTLTexture {}
unsafe impl Sync for MTLTexture {}

impl MTLTexture {
    pub(crate) unsafe fn descriptor(
        info: &gpu::TextureInfo,
    ) -> Retained<objc2_metal::MTLTextureDescriptor> {
        let descriptor = objc2_metal::MTLTextureDescriptor::new();
        descriptor.setTextureType(texture_dimensions_to_mtl(info.dimension, info.samples));
        descriptor.setSampleCount(match info.samples {
            gpu::SampleCount::Samples1 => 1,
            gpu::SampleCount::Samples2 => 2,
            gpu::SampleCount::Samples4 => 4,
            gpu::SampleCount::Samples8 => 8,
        });
        descriptor.setMipmapLevelCount(info.mip_levels as NSUInteger);
        descriptor.setArrayLength(info.array_length as NSUInteger);
        descriptor.setWidth(info.width as NSUInteger);
        descriptor.setHeight(info.height as NSUInteger);
        descriptor.setDepth(info.depth as NSUInteger);
        descriptor.setPixelFormat(format_to_mtl(info.format));

        let mut usage = objc2_metal::MTLTextureUsage::empty();
        if info.usage.contains(gpu::TextureUsage::SAMPLED) {
            usage |= objc2_metal::MTLTextureUsage::ShaderRead;
        }
        if info.usage.contains(gpu::TextureUsage::STORAGE) {
            usage |= objc2_metal::MTLTextureUsage::ShaderRead
                | objc2_metal::MTLTextureUsage::ShaderWrite;
        }
        if info
            .usage
            .intersects(gpu::TextureUsage::RENDER_TARGET | gpu::TextureUsage::DEPTH_STENCIL)
        {
            usage |= objc2_metal::MTLTextureUsage::RenderTarget;
            descriptor.setCompressionType(objc2_metal::MTLTextureCompressionType::Lossless);
        }
        if info.supports_srgb {
            usage |= objc2_metal::MTLTextureUsage::PixelFormatView;
        }
        descriptor.setUsage(usage);
        // We dont need to call the setters for storage mode, caching or hazardtracking, those are taken from the resource options
        descriptor.setResourceOptions(Self::resource_options(info));
        descriptor
    }

    pub(crate) unsafe fn new(
        memory: ResourceMemory,
        info: &gpu::TextureInfo,
        name: Option<&str>,
    ) -> Result<Self, gpu::OutOfMemoryError> {
        let descriptor = Self::descriptor(info);
        let mut options = descriptor.resourceOptions();

        let texture_opt = match memory {
            ResourceMemory::Dedicated {
                device,
                options: memory_options,
            } => {
                if info.usage.gpu_writable() {
                    options |= objc2_metal::MTLResourceOptions::HazardTrackingModeTracked;
                } else {
                    options |= objc2_metal::MTLResourceOptions::HazardTrackingModeUntracked;
                }
                descriptor.setResourceOptions(options | memory_options);
                device.newTextureWithDescriptor(&descriptor)
            }
            ResourceMemory::Suballocated { memory, offset } => {
                options |= objc2_metal::MTLResourceOptions::HazardTrackingModeUntracked;
                options |= memory.resource_options();
                descriptor.setResourceOptions(options);
                memory
                    .handle()
                    .newTextureWithDescriptor_offset(&descriptor, offset as NSUInteger)
            }
        };

        if texture_opt.is_none() {
            return Err(gpu::OutOfMemoryError {});
        }
        let texture = texture_opt.unwrap();

        if let Some(name) = name {
            texture.setLabel(Some(&NSString::from_str(name)));
        }

        Ok(Self {
            info: info.clone(),
            texture,
        })
    }

    pub(crate) fn from_mtl_texture(
        texture: Retained<ProtocolObject<dyn objc2_metal::MTLTexture>>,
    ) -> Self {
        let format = format_from_metal(texture.pixelFormat());

        let mut usage = gpu::TextureUsage::empty();
        let mtl_usage = texture.usage();
        if mtl_usage.contains(objc2_metal::MTLTextureUsage::ShaderRead) {
            usage |= gpu::TextureUsage::SAMPLED;
        }
        if mtl_usage.contains(objc2_metal::MTLTextureUsage::ShaderWrite) {
            usage |= gpu::TextureUsage::STORAGE;
        }
        if mtl_usage.contains(objc2_metal::MTLTextureUsage::RenderTarget) {
            if format.is_depth() || format.is_stencil() {
                usage |= gpu::TextureUsage::DEPTH_STENCIL;
            } else {
                usage |= gpu::TextureUsage::RENDER_TARGET;
            }
        }

        if !texture.isFramebufferOnly() {
            usage |= gpu::TextureUsage::COPY_DST | gpu::TextureUsage::COPY_SRC;
        }

        let info = gpu::TextureInfo {
            width: texture.width() as u32,
            height: texture.height() as u32,
            depth: texture.depth() as u32,
            dimension: match texture.textureType() {
                objc2_metal::MTLTextureType::Type1D => gpu::TextureDimension::Dim1D,
                objc2_metal::MTLTextureType::Type1DArray => gpu::TextureDimension::Dim1DArray,
                objc2_metal::MTLTextureType::Type2D => gpu::TextureDimension::Dim2D,
                objc2_metal::MTLTextureType::Type2DArray => gpu::TextureDimension::Dim2DArray,
                objc2_metal::MTLTextureType::Type2DMultisample => gpu::TextureDimension::Dim2D,
                objc2_metal::MTLTextureType::TypeCube => gpu::TextureDimension::Cube,
                objc2_metal::MTLTextureType::TypeCubeArray => gpu::TextureDimension::CubeArray,
                objc2_metal::MTLTextureType::Type3D => gpu::TextureDimension::Dim3D,
                objc2_metal::MTLTextureType::Type2DMultisampleArray => {
                    gpu::TextureDimension::Dim2DArray
                }
                _ => unimplemented!(),
            },
            format,
            mip_levels: texture.mipmapLevelCount() as u32,
            array_length: texture.arrayLength() as u32,
            samples: match texture.sampleCount() {
                1 => gpu::SampleCount::Samples1,
                2 => gpu::SampleCount::Samples2,
                4 => gpu::SampleCount::Samples4,
                8 => gpu::SampleCount::Samples8,
                _ => panic!("Unsupported sample count"),
            },
            usage,
            supports_srgb: mtl_usage.contains(objc2_metal::MTLTextureUsage::PixelFormatView),
        };

        Self {
            texture,
            info: info.clone(),
        }
    }

    pub(crate) fn resource_options(_info: &gpu::TextureInfo) -> objc2_metal::MTLResourceOptions {
        let options = objc2_metal::MTLResourceOptions::empty();
        options
    }

    pub(crate) fn handle(&self) -> &ProtocolObject<dyn objc2_metal::MTLTexture> {
        &self.texture
    }
}

impl gpu::Texture for MTLTexture {
    fn info(&self) -> &gpu::TextureInfo {
        &self.info
    }

    unsafe fn can_be_written_directly(&self) -> bool {
        let texture_ref = self.handle();
        texture_ref.storageMode() == objc2_metal::MTLStorageMode::Private
    }
}

impl PartialEq<MTLTexture> for MTLTexture {
    fn eq(&self, other: &MTLTexture) -> bool {
        let texture_ref = self.handle();
        let other_ref = other.handle();
        texture_ref == other_ref
    }
}

impl Eq for MTLTexture {}

pub struct MTLTextureView {
    info: gpu::TextureViewInfo,
    texture_info: gpu::TextureInfo,
    view: Retained<ProtocolObject<dyn objc2_metal::MTLTexture>>,
}

unsafe impl Send for MTLTextureView {}
unsafe impl Sync for MTLTextureView {}

impl MTLTextureView {
    pub(crate) unsafe fn new(
        texture: &MTLTexture,
        info: &gpu::TextureViewInfo,
        name: Option<&str>,
    ) -> Self {
        let entire_texture = info.array_layer_length == texture.info.array_length
            && info.base_array_layer == 0
            && info.mip_level_length == texture.info.mip_levels
            && info.base_mip_level == 0;

        let view = if entire_texture && info.format.is_none() {
            Retained::from(texture.handle())
        } else if entire_texture {
            texture
                .handle()
                .newTextureViewWithPixelFormat(format_to_mtl(info.format.unwrap()))
                .unwrap()
        } else {
            texture
                .handle()
                .newTextureViewWithPixelFormat_textureType_levels_slices(
                    format_to_mtl(info.format.unwrap_or(texture.info.format)),
                    texture_dimensions_to_mtl(texture.info.dimension, texture.info.samples),
                    NSRange {
                        location: info.base_mip_level as NSUInteger,
                        length: info.mip_level_length as NSUInteger,
                    },
                    NSRange {
                        location: info.base_array_layer as NSUInteger,
                        length: info.array_layer_length as NSUInteger,
                    },
                )
                .unwrap()
        };

        if let Some(name) = name {
            view.setLabel(Some(NSString::from_str(name).as_ref()));
        }

        Self {
            view,
            info: info.clone(),
            texture_info: texture.info.clone(),
        }
    }

    pub(crate) fn handle(&self) -> &ProtocolObject<dyn objc2_metal::MTLTexture> {
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
        &self.view == &other.view
    }
}

impl Eq for MTLTextureView {}

pub struct MTLSampler {
    sampler: Retained<ProtocolObject<dyn objc2_metal::MTLSamplerState>>,
    info: gpu::SamplerInfo,
}

unsafe impl Send for MTLSampler {}
unsafe impl Sync for MTLSampler {}

impl MTLSampler {
    pub(crate) fn new(
        device: &ProtocolObject<dyn objc2_metal::MTLDevice>,
        info: &gpu::SamplerInfo,
    ) -> Self {
        let descriptor: Retained<objc2_metal::MTLSamplerDescriptor> =
            objc2_metal::MTLSamplerDescriptor::new();
        descriptor.setSAddressMode(address_mode_to_mtl(info.address_mode_u));
        descriptor.setTAddressMode(address_mode_to_mtl(info.address_mode_v));
        descriptor.setRAddressMode(address_mode_to_mtl(info.address_mode_w));
        descriptor.setMinFilter(filter_to_mtl_minmag(info.min_filter));
        descriptor.setMagFilter(filter_to_mtl_minmag(info.mag_filter));
        descriptor.setMipFilter(filter_to_mtl_mip(info.mip_filter));
        descriptor.setLodAverage(true);
        descriptor.setSupportArgumentBuffers(false);
        descriptor.setMaxAnisotropy(info.max_anisotropy as NSUInteger);
        if let Some(compare_op) = info.compare_op {
            descriptor.setCompareFunction(compare_op_to_mtl(compare_op));
        }
        descriptor.setLodMinClamp(info.min_lod);
        if let Some(max) = info.max_lod {
            descriptor.setLodMaxClamp(max);
        }
        let sampler = device.newSamplerStateWithDescriptor(&descriptor).unwrap();
        Self {
            sampler,
            info: info.clone(),
        }
    }

    pub(crate) fn handle(&self) -> &ProtocolObject<dyn objc2_metal::MTLSamplerState> {
        &self.sampler
    }
}

impl gpu::Sampler for MTLSampler {
    fn info(&self) -> &gpu::SamplerInfo {
        &self.info
    }
}
