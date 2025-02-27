use std::{
    cmp::max, ffi::{
        c_void,
        CString,
    }, hash::{
        Hash,
        Hasher,
    }, pin::Pin, sync::Arc
};

use ash::{
    vk,
    vk::Handle as _,
};
use sourcerenderer_core::{gpu, FixedSizeSmallVec};

use super::*;

pub(crate) struct VkImageCreateInfoCollection<'a> {
    pub(crate) create_info: vk::ImageCreateInfo<'a>,
    pub(crate) compatible_vk_formats: FixedSizeSmallVec<[vk::Format; 2]>,
    pub(crate) format_list: vk::ImageFormatListCreateInfo<'a>
}

impl Default for VkImageCreateInfoCollection<'_> {
    fn default() -> Self {
        Self {
            create_info: vk::ImageCreateInfo::default(),
            compatible_vk_formats: FixedSizeSmallVec::new(),
            format_list: vk::ImageFormatListCreateInfo::default()
        }
    }
}

pub struct VkTexture {
    image: vk::Image,
    device: Arc<RawVkDevice>,
    info: gpu::TextureInfo,
    memory: Option<vk::DeviceMemory>,
    is_image_owned: bool,
    is_memory_owned: bool,
    supports_direct_copy: bool
}

unsafe impl Send for VkTexture {}
unsafe impl Sync for VkTexture {}

impl VkTexture {
    pub(crate) fn build_create_info(device: &RawVkDevice, mut target: Pin<&mut VkImageCreateInfoCollection>, info: &gpu::TextureInfo) {
        target.create_info = vk::ImageCreateInfo {
            flags: vk::ImageCreateFlags::empty(),
            tiling: vk::ImageTiling::OPTIMAL,
            initial_layout: vk::ImageLayout::UNDEFINED,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            usage: texture_usage_to_vk(info.usage, device.host_image_copy.is_some()),
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

        let main_format = target.create_info.format;
        target.compatible_vk_formats.push(main_format);
        if info.supports_srgb {
            let srgb_format_opt = info.format.srgb_format();
            if srgb_format_opt.is_none() {
                panic!("Format {:?} does not have an equivalent srgb format", info.format);
            }
            let srgb_format = srgb_format_opt.unwrap();
            target.compatible_vk_formats.push(format_to_vk(srgb_format, false));
            target.format_list = vk::ImageFormatListCreateInfo {
                view_format_count: target.compatible_vk_formats.as_ref().len() as u32,
                p_view_formats: target.compatible_vk_formats.as_ref().as_ptr(),
                ..Default::default()
            };

            target.create_info.flags |= vk::ImageCreateFlags::MUTABLE_FORMAT;
            let old_p_next = target.create_info.p_next;
            target.create_info.p_next = &target.format_list as *const vk::ImageFormatListCreateInfo as *const c_void;
            target.format_list.p_next = old_p_next;
        }

        let mut props: vk::ImageFormatProperties2 = Default::default();
        let mut host_image_copy_format_info = vk::HostImageCopyDevicePerformanceQueryEXT::default();
        let format_info = vk::PhysicalDeviceImageFormatInfo2 {
            format: target.create_info.format,
            ty: target.create_info.image_type,
            tiling: target.create_info.tiling,
            usage: target.create_info.usage,
            flags: target.create_info.flags,
            ..Default::default()
        };
        if target.create_info.usage.contains(vk::ImageUsageFlags::HOST_TRANSFER_EXT) {
            props.p_next = &mut host_image_copy_format_info as *mut vk::HostImageCopyDevicePerformanceQueryEXT
                as *mut c_void;
        }
        unsafe {
            device
                .instance
                .get_physical_device_image_format_properties2(
                    device.physical_device,
                    &format_info,
                    &mut props,
                )
                .unwrap()
        };

        if target.create_info.usage.contains(vk::ImageUsageFlags::HOST_TRANSFER_EXT)
            && host_image_copy_format_info.optimal_device_access != vk::TRUE {
            target.create_info.usage = texture_usage_to_vk(info.usage, false);
        }
    }

    pub(crate) unsafe fn new(device: &Arc<RawVkDevice>, info: &gpu::TextureInfo, memory: ResourceMemory, name: Option<&str>) -> Result<Self, gpu::OutOfMemoryError> {
        let memory_type_index = match &memory {
            ResourceMemory::Suballocated { memory, .. } => { memory.memory_type_index() },
            ResourceMemory::Dedicated { memory_type_index } => *memory_type_index
        };

        let mut create_info_collection = VkImageCreateInfoCollection::default();
        let mut pinned = Pin::new(&mut create_info_collection);
        Self::build_create_info(device, pinned.as_mut(), info);

        if pinned.create_info.usage.contains(vk::ImageUsageFlags::HOST_TRANSFER_EXT)
            && device.host_image_copy.as_ref().unwrap().properties_host_image_copy.identical_memory_type_requirements == vk::FALSE {
            // Memory type requirements might change based on HOST_TRANSFER, so we need to check
            // if the allocated memory (or predetermined memory type) is actually compatible.

            let mut requirements = vk::MemoryRequirements2::default();
            let image_requirements_info = vk::DeviceImageMemoryRequirements {
                p_create_info: &pinned.create_info as *const vk::ImageCreateInfo,
                ..Default::default()
            };
            unsafe { device.get_device_image_memory_requirements(&image_requirements_info, &mut requirements); }
            if (requirements.memory_requirements.memory_type_bits & (1 << memory_type_index)) == 0 {
                log::info!("Switching from HOST_IMAGE_COPY to gpu image copy because memory type is not compatible.");
                pinned.create_info.usage |= vk::ImageUsageFlags::TRANSFER_DST;
                pinned.create_info.usage &= !vk::ImageUsageFlags::HOST_TRANSFER_EXT;
            }
        }

        let image_res = device.create_image(&create_info_collection.create_info, None);
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
                        device.destroy_image(image, None);
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
                        device.destroy_image(image, None);
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
                        device.destroy_image(image, None);
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

        if create_info_collection.create_info.usage.contains(vk::ImageUsageFlags::HOST_TRANSFER_EXT) {
            log::info!("Texture supports direct copy!");
        }

        Ok(Self {
            image,
            device: device.clone(),
            info: info.clone(),
            memory: Some(vk_memory),
            is_image_owned: true,
            is_memory_owned,
            supports_direct_copy: create_info_collection.create_info.usage.contains(vk::ImageUsageFlags::HOST_TRANSFER_EXT)
        })
    }

    pub fn from_image(device: &Arc<RawVkDevice>, image: vk::Image, info: gpu::TextureInfo) -> Self {
        VkTexture {
            image,
            device: device.clone(),
            info,
            is_image_owned: false,
            is_memory_owned: false,
            memory: None,
            supports_direct_copy: false
        }
    }

    pub fn handle(&self) -> vk::Image {
        self.image
    }

    pub(crate) fn info(&self) -> &gpu::TextureInfo {
        &self.info
    }
}

pub(crate) fn texture_usage_to_vk(usage: gpu::TextureUsage, host_image_copy: bool) -> vk::ImageUsageFlags {
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
        gpu::TextureUsage::BLIT_DST | gpu::TextureUsage::COPY_DST | gpu::TextureUsage::RESOLVE_DST;
    if usage.intersects(transfer_dst_usages) {
        flags |= vk::ImageUsageFlags::TRANSFER_DST;
    }

    if usage.intersects(gpu::TextureUsage::INITIAL_COPY) {
        if host_image_copy {
            flags |= vk::ImageUsageFlags::HOST_TRANSFER_EXT;
        } else {
            flags |= vk::ImageUsageFlags::TRANSFER_DST;
        }
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
        self.supports_direct_copy
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
                aspect_mask: aspect_mask_from_format(format),
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
            assert!(device.features_12.sampler_filter_minmax == vk::TRUE);

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
        aspect_mask: aspect_mask_from_format(texture_format)
    }
}

pub(crate) fn texture_subresource_to_vk_layers(subresource: &gpu::TextureSubresource, texture_format: gpu::Format, layers: u32) -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers {
        mip_level: subresource.mip_level,
        base_array_layer: subresource.array_layer,
        aspect_mask: aspect_mask_from_format(texture_format),
        layer_count: layers
    }
}
