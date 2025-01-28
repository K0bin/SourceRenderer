use std::{
    cmp::max,
    ffi::{
        c_void,
        CString,
    },
    hash::{
        Hash,
        Hasher,
    },
    sync::Arc,
};

use ash::{
    vk,
    vk::Handle as _,
};
use smallvec::SmallVec;
use sourcerenderer_core::gpu;

use super::*;

pub struct VkTexture {
    image: vk::Image,
    device: Arc<RawVkDevice>,
    info: gpu::TextureInfo,
    memory: Option<vk::DeviceMemory>,
    is_image_owned: bool,
    is_memory_owned: bool
}

unsafe impl Send for VkTexture {}
unsafe impl Sync for VkTexture {}

impl VkTexture {
    pub(crate) unsafe fn new(device: &Arc<RawVkDevice>, info: &gpu::TextureInfo, memory: ResourceMemory, name: Option<&str>) -> Result<Self, gpu::OutOfMemoryError> {
        let mut create_info = vk::ImageCreateInfo {
            flags: vk::ImageCreateFlags::empty(),
            tiling: vk::ImageTiling::OPTIMAL,
            initial_layout: vk::ImageLayout::UNDEFINED,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            usage: texture_usage_to_vk(info.usage),
            image_type: match info.dimension {
                gpu::TextureDimension::Dim1DArray | gpu::TextureDimension::Dim1D => vk::ImageType::TYPE_1D,
                gpu::TextureDimension::Dim2DArray
                    | gpu::TextureDimension::Dim2D
                    | gpu::TextureDimension::Cube
                    | gpu::TextureDimension::CubeArray => vk::ImageType::TYPE_2D,
                gpu::TextureDimension::Dim3D => vk::ImageType::TYPE_3D,
            },
            extent: vk::Extent3D {
                width: max(1, info.width),
                height: max(1, info.height),
                depth: max(1, info.depth),
            },
            format: format_to_vk(info.format, device.supports_d24),
            mip_levels: info.mip_levels,
            array_layers: info.array_length,
            samples: samples_to_vk(info.samples),
            ..Default::default()
        };

        debug_assert!(
            info.array_length == 1
                || (info.dimension == gpu::TextureDimension::Dim1DArray
                    || info.dimension == gpu::TextureDimension::Dim2DArray)
        );
        debug_assert!(info.depth == 1 || info.dimension == gpu::TextureDimension::Dim3D);
        debug_assert!(
            info.height == 1
                || (info.dimension == gpu::TextureDimension::Dim2D
                    || info.dimension == gpu::TextureDimension::Dim2DArray
                    || info.dimension == gpu::TextureDimension::Dim3D)
        );

        let mut compatible_formats = SmallVec::<[vk::Format; 2]>::with_capacity(2);
        compatible_formats.push(create_info.format);
        let srgb_format = info.format.srgb_format();
        if let Some(srgb_format) = srgb_format {
            compatible_formats.push(format_to_vk(srgb_format, false));
        }
        let mut format_list = vk::ImageFormatListCreateInfo {
            view_format_count: compatible_formats.len() as u32,
            p_view_formats: compatible_formats.as_ptr(),
            ..Default::default()
        };
        if info.supports_srgb {
            create_info.flags |= vk::ImageCreateFlags::MUTABLE_FORMAT;
            if device.features.contains(VkFeatures::IMAGE_FORMAT_LIST) {
                format_list.p_next = std::mem::replace(
                    &mut create_info.p_next,
                    &format_list as *const vk::ImageFormatListCreateInfo as *const c_void,
                );
            }
        }

        let mut props: vk::ImageFormatProperties2 = Default::default();
        unsafe {
            device
                .instance
                .get_physical_device_image_format_properties2(
                    device.physical_device,
                    &vk::PhysicalDeviceImageFormatInfo2 {
                        format: create_info.format,
                        ty: create_info.image_type,
                        tiling: create_info.tiling,
                        usage: create_info.usage,
                        flags: create_info.flags,
                        ..Default::default()
                    },
                    &mut props,
                )
                .unwrap()
        };


        let image_res = device.create_image(&create_info, None);
        if let Err(e) = image_res {
            if e == vk::Result::ERROR_OUT_OF_DEVICE_MEMORY || e == vk::Result::ERROR_OUT_OF_HOST_MEMORY {
                return Err(gpu::OutOfMemoryError {});
            }
        }
        let image = image_res.unwrap();

        let mut is_memory_owned = false;
        let vk_memory: vk::DeviceMemory;
        match memory {
            ResourceMemory::Dedicated {
                memory_type_index
            } => {
                let requirements_info = vk::ImageMemoryRequirementsInfo2 {
                    image,
                    ..Default::default()
                };
                let mut requirements = vk::MemoryRequirements2::default();
                device.get_image_memory_requirements2(&requirements_info, &mut requirements);
                assert!((requirements.memory_requirements.memory_type_bits & (1 << memory_type_index)) != 0);

                let dedicated_alloc = vk::MemoryDedicatedAllocateInfo {
                    image: image,
                    ..Default::default()
                };
                let memory_info = vk::MemoryAllocateInfo {
                    allocation_size: requirements.memory_requirements.size,
                    memory_type_index,
                    p_next: &dedicated_alloc as *const vk::MemoryDedicatedAllocateInfo as *const c_void,
                    ..Default::default()
                };
                let memory_result: Result<vk::DeviceMemory, vk::Result> = device.allocate_memory(&memory_info, None);
                if let Err(e) = memory_result {
                    if e == vk::Result::ERROR_OUT_OF_DEVICE_MEMORY || e == vk::Result::ERROR_OUT_OF_HOST_MEMORY {
                        return Err(gpu::OutOfMemoryError {});
                    }
                }
                vk_memory = memory_result.unwrap();

                let bind_result = device.bind_image_memory2(&[
                    vk::BindImageMemoryInfo {
                        image,
                        memory: vk_memory,
                        memory_offset: 0u64,
                        ..Default::default()
                    }
                ]);
                if let Err(e) = bind_result {
                    if e == vk::Result::ERROR_OUT_OF_DEVICE_MEMORY || e == vk::Result::ERROR_OUT_OF_HOST_MEMORY {
                        return Err(gpu::OutOfMemoryError {});
                    }
                }

                is_memory_owned = true;
            }

            ResourceMemory::Suballocated {
                memory,
                offset
            } => {
                let bind_result = device.bind_image_memory2(&[
                    vk::BindImageMemoryInfo {
                        image,
                        memory: memory.handle(),
                        memory_offset: offset,
                        ..Default::default()
                    }
                ]);
                if let Err(e) = bind_result {
                    if e == vk::Result::ERROR_OUT_OF_DEVICE_MEMORY || e == vk::Result::ERROR_OUT_OF_HOST_MEMORY {
                        return Err(gpu::OutOfMemoryError {});
                    }
                }

                vk_memory = memory.handle();
            }
        }

        if let Some(name) = name {
            if let Some(debug_utils) = device.debug_utils.as_ref() {
                let name_cstring = CString::new(name).unwrap();
                unsafe {
                    debug_utils
                        .set_debug_utils_object_name(
                            &vk::DebugUtilsObjectNameInfoEXT {
                                object_type: vk::ObjectType::IMAGE,
                                object_handle: image.as_raw(),
                                p_object_name: name_cstring.as_ptr(),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                }
            }
        }
        Ok(Self {
            image,
            device: device.clone(),
            info: info.clone(),
            memory: Some(vk_memory),
            is_image_owned: true,
            is_memory_owned
        })
    }

    pub fn from_image(device: &Arc<RawVkDevice>, image: vk::Image, info: gpu::TextureInfo) -> Self {
        VkTexture {
            image,
            device: device.clone(),
            info,
            is_image_owned: false,
            is_memory_owned: false,
            memory: None
        }
    }

    pub fn handle(&self) -> vk::Image {
        self.image
    }

    pub(crate) fn info(&self) -> &gpu::TextureInfo {
        &self.info
    }
}

pub(crate) fn texture_usage_to_vk(usage: gpu::TextureUsage) -> vk::ImageUsageFlags {
    let mut flags = vk::ImageUsageFlags::empty();

    if usage.contains(gpu::TextureUsage::STORAGE) {
        flags |= vk::ImageUsageFlags::STORAGE;
    }

    if usage.contains(gpu::TextureUsage::SAMPLED) {
        flags |= vk::ImageUsageFlags::SAMPLED;
    }

    let transfer_src_usages =
        gpu::TextureUsage::BLIT_SRC | gpu::TextureUsage::COPY_SRC | gpu::TextureUsage::RESOLVE_SRC; // TODO: sync2
    if usage.intersects(transfer_src_usages) {
        flags |= vk::ImageUsageFlags::TRANSFER_SRC;
    }

    let transfer_dst_usages =
        gpu::TextureUsage::BLIT_DST | gpu::TextureUsage::COPY_DST | gpu::TextureUsage::RESOLVE_DST | gpu::TextureUsage::INITIAL_COPY;
    if usage.intersects(transfer_dst_usages) {
        flags |= vk::ImageUsageFlags::TRANSFER_DST;
    }

    if usage.contains(gpu::TextureUsage::DEPTH_STENCIL) {
        flags |= vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT;
    }

    if usage.contains(gpu::TextureUsage::RENDER_TARGET) {
        flags |= vk::ImageUsageFlags::COLOR_ATTACHMENT;
    }

    flags
}

impl Drop for VkTexture {
    fn drop(&mut self) {
        unsafe {
            if self.is_image_owned {
                self.device.destroy_image(self.image, None);
            }
            if let Some(memory) = self.memory {
                if self.is_memory_owned {
                    self.device.free_memory(memory, None);
                }
            }
        }
    }
}

impl Hash for VkTexture {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.image.hash(state);
    }
}

impl PartialEq for VkTexture {
    fn eq(&self, other: &Self) -> bool {
        self.image == other.image
    }
}

impl Eq for VkTexture {}

impl gpu::Texture for VkTexture {
    fn info(&self) -> &gpu::TextureInfo {
        &self.info
    }

    unsafe fn can_be_written_directly(&self) -> bool {
        false
    }
}

fn filter_to_vk(filter: gpu::Filter) -> vk::Filter {
    match filter {
        gpu::Filter::Linear => vk::Filter::LINEAR,
        gpu::Filter::Nearest => vk::Filter::NEAREST,
        gpu::Filter::Max => vk::Filter::LINEAR,
        gpu::Filter::Min => vk::Filter::LINEAR,
    }
}
fn filter_to_vk_mip(filter: gpu::Filter) -> vk::SamplerMipmapMode {
    match filter {
        gpu::Filter::Linear => vk::SamplerMipmapMode::LINEAR,
        gpu::Filter::Nearest => vk::SamplerMipmapMode::NEAREST,
        gpu::Filter::Max => panic!("Can't use max as mipmap filter."),
        gpu::Filter::Min => panic!("Can't use min as mipmap filter."),
    }
}
fn filter_to_reduction_mode(filter: gpu::Filter) -> vk::SamplerReductionMode {
    match filter {
        gpu::Filter::Max => vk::SamplerReductionMode::MAX,
        gpu::Filter::Min => vk::SamplerReductionMode::MIN,
        _ => unreachable!(),
    }
}

fn address_mode_to_vk(address_mode: gpu::AddressMode) -> vk::SamplerAddressMode {
    match address_mode {
        gpu::AddressMode::Repeat => vk::SamplerAddressMode::REPEAT,
        gpu::AddressMode::ClampToBorder => vk::SamplerAddressMode::CLAMP_TO_BORDER,
        gpu::AddressMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        gpu::AddressMode::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
    }
}

pub struct VkTextureView {
    view: vk::ImageView,
    device: Arc<RawVkDevice>,
    info: gpu::TextureViewInfo,
    texture_info: gpu::TextureInfo, // required to create a frame buffer later
}

impl VkTextureView {
    pub(crate) fn new(
        device: &Arc<RawVkDevice>,
        texture: &VkTexture,
        info: &gpu::TextureViewInfo,
        name: Option<&str>,
    ) -> Self {
        let format = info.format.unwrap_or(texture.info.format);
        let view_create_info = vk::ImageViewCreateInfo {
            image: texture.handle(),
            view_type: match texture.info.dimension {
                gpu::TextureDimension::Dim1D => vk::ImageViewType::TYPE_1D,
                gpu::TextureDimension::Dim2D => vk::ImageViewType::TYPE_2D,
                gpu::TextureDimension::Dim3D => vk::ImageViewType::TYPE_3D,
                gpu::TextureDimension::Dim1DArray => vk::ImageViewType::TYPE_1D_ARRAY,
                gpu::TextureDimension::Dim2DArray
                    | gpu::TextureDimension::Cube
                    | gpu::TextureDimension::CubeArray => vk::ImageViewType::TYPE_2D_ARRAY,
            },
            format: format_to_vk(format, device.supports_d24),
            components: vk::ComponentMapping {
                r: vk::ComponentSwizzle::IDENTITY,
                g: vk::ComponentSwizzle::IDENTITY,
                b: vk::ComponentSwizzle::IDENTITY,
                a: vk::ComponentSwizzle::IDENTITY,
            },
            subresource_range: vk::ImageSubresourceRange {
                aspect_mask: if format.is_depth() {
                    vk::ImageAspectFlags::DEPTH
                } else {
                    vk::ImageAspectFlags::COLOR
                },
                base_mip_level: info.base_mip_level,
                level_count: info.mip_level_length,
                base_array_layer: info.base_array_layer,
                layer_count: info.array_layer_length,
            },
            ..Default::default()
        };
        let view = unsafe { device.create_image_view(&view_create_info, None) }.unwrap();

        if let Some(name) = name {
            if let Some(debug_utils) = device.debug_utils.as_ref() {
                let name_cstring = CString::new(name).unwrap();
                unsafe {
                    debug_utils
                        .set_debug_utils_object_name(
                            &vk::DebugUtilsObjectNameInfoEXT {
                                object_type: vk::ObjectType::IMAGE_VIEW,
                                object_handle: view.as_raw(),
                                p_object_name: name_cstring.as_ptr(),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                }
            }
        }

        Self {
            view,
            device: device.clone(),
            info: info.clone(),
            texture_info: texture.info().clone(),
        }
    }

    #[inline]
    pub(crate) fn view_handle(&self) -> vk::ImageView {
        self.view
    }

    pub(crate) fn info(&self) -> &gpu::TextureViewInfo {
        &self.info
    }

    pub(crate) fn texture_info(&self) -> &gpu::TextureInfo {
        &self.texture_info
    }
}

impl Drop for VkTextureView {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_image_view(self.view, None);
        }
    }
}

impl Hash for VkTextureView {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.view.hash(state);
    }
}

impl PartialEq for VkTextureView {
    fn eq(&self, other: &Self) -> bool {
        self.view == other.view
    }
}

impl Eq for VkTextureView {}

impl gpu::TextureView for VkTextureView {
    fn info(&self) -> &gpu::TextureViewInfo {
        &self.info
    }

    fn texture_info(&self) -> &gpu::TextureInfo {
        &self.texture_info
    }
}

pub struct VkSampler {
    sampler: vk::Sampler,
    device: Arc<RawVkDevice>,
    info: gpu::SamplerInfo
}

impl VkSampler {
    pub fn new(device: &Arc<RawVkDevice>, info: &gpu::SamplerInfo) -> Self {
        let mut sampler_create_info = vk::SamplerCreateInfo {
            mag_filter: filter_to_vk(info.mag_filter),
            min_filter: filter_to_vk(info.mag_filter),
            mipmap_mode: filter_to_vk_mip(info.mip_filter),
            address_mode_u: address_mode_to_vk(info.address_mode_u),
            address_mode_v: address_mode_to_vk(info.address_mode_v),
            address_mode_w: address_mode_to_vk(info.address_mode_u),
            mip_lod_bias: info.mip_bias,
            anisotropy_enable: (info.max_anisotropy.abs() >= 1.0f32) as u32,
            max_anisotropy: info.max_anisotropy,
            compare_enable: info.compare_op.is_some() as u32,
            compare_op: info
                .compare_op
                .map_or(vk::CompareOp::ALWAYS, compare_func_to_vk),
            min_lod: info.min_lod,
            max_lod: info.max_lod.unwrap_or(vk::LOD_CLAMP_NONE),
            border_color: vk::BorderColor::INT_OPAQUE_BLACK,
            unnormalized_coordinates: 0,
            ..Default::default()
        };

        let mut sampler_minmax_info = vk::SamplerReductionModeCreateInfo::default();
        if info.min_filter == gpu::Filter::Min || info.min_filter == gpu::Filter::Max {
            assert!(device.features.contains(VkFeatures::MIN_MAX_FILTER));

            sampler_minmax_info.reduction_mode = filter_to_reduction_mode(info.min_filter);
            sampler_create_info.p_next =
                &sampler_minmax_info as *const vk::SamplerReductionModeCreateInfo as *const c_void;
        }
        debug_assert_ne!(info.mag_filter, gpu::Filter::Min);
        debug_assert_ne!(info.mag_filter, gpu::Filter::Max);
        debug_assert_ne!(info.mip_filter, gpu::Filter::Min);
        debug_assert_ne!(info.mip_filter, gpu::Filter::Max);

        let sampler = unsafe { device.create_sampler(&sampler_create_info, None) }.unwrap();

        Self {
            sampler,
            device: device.clone(),
            info: info.clone()
        }
    }

    #[inline]
    pub(crate) fn handle(&self) -> vk::Sampler {
        self.sampler
    }
}

impl Drop for VkSampler {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_sampler(self.sampler, None);
        }
    }
}

impl Hash for VkSampler {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.sampler.hash(state);
    }
}

impl PartialEq for VkSampler {
    fn eq(&self, other: &Self) -> bool {
        self.sampler == other.sampler
    }
}

impl Eq for VkSampler {}

impl gpu::Sampler for VkSampler {
    fn info(&self) -> &gpu::SamplerInfo {
        &self.info
    }
}

pub(crate) fn texture_subresource_to_vk(subresource: &gpu::TextureSubresource, texture_format: gpu::Format) -> vk::ImageSubresource {
    vk::ImageSubresource {
        mip_level: subresource.mip_level,
        array_layer: subresource.array_layer,
        aspect_mask: if texture_format.is_depth() {
            vk::ImageAspectFlags::DEPTH
        } else {
            vk::ImageAspectFlags::COLOR
        }
    }
}

pub(crate) fn texture_subresource_to_vk_layers(subresource: &gpu::TextureSubresource, texture_format: gpu::Format, layers: u32) -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers {
        mip_level: subresource.mip_level,
        base_array_layer: subresource.array_layer,
        aspect_mask: if texture_format.is_depth() {
            vk::ImageAspectFlags::DEPTH
        } else {
            vk::ImageAspectFlags::COLOR
        },
        layer_count: layers
    }
}
