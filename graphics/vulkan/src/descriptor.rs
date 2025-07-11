use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::hash::{
    Hash,
    Hasher,
};
use std::marker::PhantomData;
use std::sync::{
    Arc,
    Mutex,
    MutexGuard,
};

use ash::vk;
use bitflags::bitflags;
use smallvec::SmallVec;
use sourcerenderer_core::gpu;

use super::*;

// TODO: clean up descriptor management
// TODO: determine descriptor and set counts

// TODO: this shit is really slow. rewrite all of it.

const DEFAULT_DESCRIPTOR_ARRAY_SIZE: usize = 4usize;
const DEFAULT_PER_SET_PREALLOCATED_SIZE: usize = 8usize;

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct DirtyDescriptorSets: u32 {
        const VERY_FREQUENT = 0b0001;
        const FREQUENT = 0b0010;
        const FRAME = 0b0100;
    }
}

impl From<gpu::BindingFrequency> for DirtyDescriptorSets {
    fn from(value: gpu::BindingFrequency) -> Self {
        match value {
            gpu::BindingFrequency::VeryFrequent => DirtyDescriptorSets::VERY_FREQUENT,
            gpu::BindingFrequency::Frequent => DirtyDescriptorSets::FREQUENT,
            gpu::BindingFrequency::Frame => DirtyDescriptorSets::FRAME,
        }
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub(crate) struct VkDescriptorSetEntryInfo {
    pub(crate) name: String,
    pub(crate) shader_stage: vk::ShaderStageFlags,
    pub(crate) index: u32,
    pub(crate) count: u32,
    pub(crate) descriptor_type: vk::DescriptorType,
    pub(crate) writable: bool,
    pub(crate) flags: vk::DescriptorBindingFlags,
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub(crate) struct VkConstantRange {
    pub(crate) offset: u32,
    pub(crate) size: u32,
    pub(crate) shader_stage: vk::ShaderStageFlags,
}

pub(crate) struct VkDescriptorSetLayout {
    pub device: Arc<RawVkDevice>,
    layout: vk::DescriptorSetLayout,
    binding_infos: SmallVec<[Option<VkDescriptorSetEntryInfo>; DEFAULT_PER_SET_PREALLOCATED_SIZE]>,
    is_empty: bool,
    template: Option<vk::DescriptorUpdateTemplate>,
    max_used_binding: u32,
}

impl VkDescriptorSetLayout {
    pub fn new(
        bindings: &[VkDescriptorSetEntryInfo],
        flags: vk::DescriptorSetLayoutCreateFlags,
        device: &Arc<RawVkDevice>,
    ) -> Self {
        let mut vk_bindings: Vec<vk::DescriptorSetLayoutBinding> = Vec::new();
        let mut vk_binding_flags: Vec<vk::DescriptorBindingFlags> = Vec::new();
        let mut vk_template_entries: Vec<vk::DescriptorUpdateTemplateEntry> = Vec::new();
        let mut binding_infos: SmallVec<
            [Option<VkDescriptorSetEntryInfo>; DEFAULT_PER_SET_PREALLOCATED_SIZE],
        > = SmallVec::with_capacity(bindings.len());

        let mut max_used_binding = 0u32;

        for binding in bindings.iter() {
            if binding.index as usize >= binding_infos.len() {
                binding_infos.resize(binding.index as usize + 1, None);
            }
            binding_infos[binding.index as usize] = Some(binding.clone());

            vk_bindings.push(vk::DescriptorSetLayoutBinding {
                binding: binding.index,
                descriptor_count: binding.count,
                descriptor_type: binding.descriptor_type,
                stage_flags: binding.shader_stage,
                p_immutable_samplers: std::ptr::null(),
                _marker: PhantomData,
            });
            vk_binding_flags.push(binding.flags);

            vk_template_entries.push(vk::DescriptorUpdateTemplateEntry {
                dst_binding: binding.index,
                dst_array_element: 0,
                descriptor_count: binding.count,
                descriptor_type: binding.descriptor_type,
                offset: binding.index as usize * std::mem::size_of::<VkDescriptorEntry>(),
                stride: std::mem::size_of::<VkDescriptorEntry>(),
            });
            max_used_binding = max_used_binding.max(binding.index);
        }

        let binding_flags_struct = vk::DescriptorSetLayoutBindingFlagsCreateInfo {
            binding_count: vk_binding_flags.len() as u32,
            p_binding_flags: vk_binding_flags.as_ptr(),
            ..Default::default()
        };

        let info = vk::DescriptorSetLayoutCreateInfo {
            p_next: if device.features_12.buffer_device_address == vk::TRUE {
                &binding_flags_struct as *const vk::DescriptorSetLayoutBindingFlagsCreateInfo
                    as *const c_void
            } else {
                std::ptr::null()
            },
            p_bindings: vk_bindings.as_ptr(),
            binding_count: vk_bindings.len() as u32,
            flags,
            ..Default::default()
        };
        let layout = unsafe { device.create_descriptor_set_layout(&info, None) }.unwrap();

        let template_info = vk::DescriptorUpdateTemplateCreateInfo {
            s_type: vk::StructureType::DESCRIPTOR_UPDATE_TEMPLATE_CREATE_INFO,
            p_next: std::ptr::null(),
            flags: vk::DescriptorUpdateTemplateCreateFlags::empty(),
            descriptor_update_entry_count: vk_template_entries.len() as u32,
            p_descriptor_update_entries: vk_template_entries.as_ptr(),
            template_type: vk::DescriptorUpdateTemplateType::DESCRIPTOR_SET,
            descriptor_set_layout: layout,
            // the following are irrelevant because we're not updating push descriptors
            pipeline_bind_point: vk::PipelineBindPoint::GRAPHICS,
            pipeline_layout: vk::PipelineLayout::null(),
            set: 0,
            _marker: PhantomData,
        };
        let template = if !flags
            .contains(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL_EXT)
            && !vk_template_entries.is_empty()
        {
            Some(unsafe { device.create_descriptor_update_template(&template_info, None) }.unwrap())
        } else {
            None
        };

        Self {
            device: device.clone(),
            layout,
            binding_infos,
            template,
            is_empty: bindings.is_empty(),
            max_used_binding,
        }
    }

    #[inline(always)]
    pub(crate) fn handle(&self) -> vk::DescriptorSetLayout {
        self.layout
    }

    #[inline(always)]
    pub(crate) fn is_empty(&self) -> bool {
        self.is_empty
    }

    #[inline(always)]
    pub(crate) fn max_used_binding(&self) -> u32 {
        self.max_used_binding
    }

    #[inline(always)]
    pub(crate) fn binding(&self, slot: u32) -> Option<&VkDescriptorSetEntryInfo> {
        if slot >= self.binding_infos.len() as u32 {
            None
        } else {
            self.binding_infos[slot as usize].as_ref()
        }
    }

    #[inline(always)]
    pub(crate) fn is_dynamic_binding(&self, binding_index: u32) -> bool {
        if binding_index >= self.binding_infos.len() as u32 {
            false
        } else if let Some(binding_info) = self.binding_infos[binding_index as usize].as_ref() {
            binding_info.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                || binding_info.descriptor_type == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
        } else {
            false
        }
    }
}

impl Drop for VkDescriptorSetLayout {
    fn drop(&mut self) {
        unsafe {
            if let Some(template) = self.template {
                self.device
                    .destroy_descriptor_update_template(template, None);
            }
            self.device.destroy_descriptor_set_layout(self.layout, None);
        }
    }
}

impl Hash for VkDescriptorSetLayout {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.layout.hash(state);
    }
}

impl PartialEq for VkDescriptorSetLayout {
    fn eq(&self, other: &Self) -> bool {
        self.layout == other.layout
    }
}

impl Eq for VkDescriptorSetLayout {}

pub(crate) struct VkDescriptorPool {
    descriptor_pool: Mutex<vk::DescriptorPool>,
    device: Arc<RawVkDevice>,
}

impl VkDescriptorPool {
    fn new(device: &Arc<RawVkDevice>, is_transient: bool) -> Self {
        // TODO figure out proper numbers
        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: 256,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
                descriptor_count: 512,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: 256,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 256,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER_DYNAMIC,
                descriptor_count: 256,
            },
        ];
        let info = vk::DescriptorPoolCreateInfo {
            max_sets: 128,
            p_pool_sizes: pool_sizes.as_ptr(),
            pool_size_count: pool_sizes.len() as u32,
            flags: if !is_transient {
                vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET
            } else {
                vk::DescriptorPoolCreateFlags::empty()
            },
            ..Default::default()
        };
        let descriptor_pool =
            Mutex::new(unsafe { device.create_descriptor_pool(&info, None) }.unwrap());
        Self {
            descriptor_pool,
            device: device.clone(),
        }
    }

    #[inline]
    fn handle(&self) -> MutexGuard<vk::DescriptorPool> {
        self.descriptor_pool.lock().unwrap()
    }

    fn reset(&self) {
        let guard = self.handle();
        unsafe {
            self.device
                .reset_descriptor_pool(*guard, vk::DescriptorPoolResetFlags::empty())
                .unwrap();
        }
    }
}

impl Drop for VkDescriptorPool {
    fn drop(&mut self) {
        let pool = self.handle();
        unsafe {
            self.device.destroy_descriptor_pool(*pool, None);
        }
    }
}

#[repr(C)]
union VkDescriptorEntry {
    image: vk::DescriptorImageInfo,
    buffer: vk::DescriptorBufferInfo,
    buffer_view: vk::BufferView,
    acceleration_structure: vk::AccelerationStructureKHR,
}

impl Default for VkDescriptorEntry {
    fn default() -> Self {
        Self {
            buffer: vk::DescriptorBufferInfo::default(),
        }
    }
}

pub(crate) struct VkDescriptorSet {
    descriptor_set: vk::DescriptorSet,
    pool: Arc<VkDescriptorPool>,
    layout: Arc<VkDescriptorSetLayout>,
    is_transient: bool,
    bindings: SmallVec<[VkBoundResource; DEFAULT_PER_SET_PREALLOCATED_SIZE]>,
    device: Arc<RawVkDevice>,
}

impl VkDescriptorSet {
    fn new<'a, T>(
        pool: &Arc<VkDescriptorPool>,
        device: &Arc<RawVkDevice>,
        layout: &Arc<VkDescriptorSetLayout>,
        is_transient: bool,
        bindings: &'a [T],
    ) -> ash::prelude::VkResult<Self>
    where
        VkBoundResource: From<&'a T>,
    {
        let pool_guard = pool.handle();
        let set_create_info = vk::DescriptorSetAllocateInfo {
            descriptor_pool: *pool_guard,
            descriptor_set_count: 1,
            p_set_layouts: &layout.handle() as *const vk::DescriptorSetLayout,
            ..Default::default()
        };
        let set = unsafe { device.allocate_descriptor_sets(&set_create_info) }?
            .pop()
            .unwrap();

        let mut stored_bindings =
            SmallVec::<[VkBoundResource; DEFAULT_PER_SET_PREALLOCATED_SIZE]>::default();
        for binding in bindings {
            stored_bindings.push(binding.into());
        }

        match Option::<vk::DescriptorUpdateTemplate>::None {
            None => {
                let mut writes: SmallVec<
                    [vk::WriteDescriptorSet; DEFAULT_PER_SET_PREALLOCATED_SIZE],
                > = SmallVec::with_capacity(bindings.len());
                let mut image_writes: SmallVec<
                    [vk::DescriptorImageInfo; DEFAULT_PER_SET_PREALLOCATED_SIZE],
                > = SmallVec::with_capacity(bindings.len());
                let mut buffer_writes: SmallVec<
                    [vk::DescriptorBufferInfo; DEFAULT_PER_SET_PREALLOCATED_SIZE],
                > = SmallVec::with_capacity(bindings.len());
                let mut acceleration_structures: SmallVec<[vk::AccelerationStructureKHR; 2]> =
                    Default::default();
                let mut acceleration_structure_writes: SmallVec<
                    [vk::WriteDescriptorSetAccelerationStructureKHR; 2],
                > = Default::default();
                for (binding, resource) in stored_bindings.iter().enumerate() {
                    // We're using pointers to elements in those vecs, so we cant relocate
                    assert_ne!(writes.len(), writes.capacity());
                    assert_ne!(image_writes.len(), image_writes.capacity());
                    assert_ne!(buffer_writes.len(), buffer_writes.capacity());
                    assert_ne!(
                        acceleration_structures.len(),
                        acceleration_structures.capacity()
                    );
                    assert_ne!(
                        acceleration_structure_writes.len(),
                        acceleration_structure_writes.capacity()
                    );

                    let binding_info = layout.binding(binding as u32);
                    if binding_info.is_none() {
                        continue;
                    }
                    let binding_info = binding_info.unwrap();

                    let mut write = vk::WriteDescriptorSet {
                        dst_set: set,
                        dst_binding: binding as u32,
                        dst_array_element: 0,
                        descriptor_count: 1,
                        ..Default::default()
                    };

                    match resource {
                        VkBoundResource::StorageBuffer(VkBufferBindingInfo {
                            buffer,
                            offset,
                            length,
                        }) => {
                            assert!(
                                binding_info.descriptor_type
                                    == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                                    || binding_info.descriptor_type
                                        == vk::DescriptorType::STORAGE_BUFFER
                            );

                            let buffer_info = vk::DescriptorBufferInfo {
                                buffer: *buffer,
                                offset: if binding_info.descriptor_type
                                    == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                                {
                                    0
                                } else {
                                    *offset as vk::DeviceSize
                                },
                                range: *length as vk::DeviceSize,
                            };
                            buffer_writes.push(buffer_info);
                            write.p_buffer_info = unsafe {
                                buffer_writes
                                    .as_ptr()
                                    .offset(buffer_writes.len() as isize - 1)
                            };
                            write.descriptor_type = binding_info.descriptor_type;
                        }
                        VkBoundResource::StorageBufferArray(buffers) => {
                            assert!(
                                binding_info.descriptor_type
                                    == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                                    || binding_info.descriptor_type
                                        == vk::DescriptorType::STORAGE_BUFFER
                            );
                            assert_eq!(binding_info.count, buffers.len() as u32);

                            for VkBufferBindingInfo {
                                buffer,
                                offset,
                                length,
                            } in buffers
                            {
                                let buffer_info = vk::DescriptorBufferInfo {
                                    buffer: *buffer,
                                    offset: if binding_info.descriptor_type
                                        == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                                    {
                                        0
                                    } else {
                                        *offset as vk::DeviceSize
                                    },
                                    range: *length as vk::DeviceSize,
                                };
                                buffer_writes.push(buffer_info);
                            }
                            write.p_buffer_info = unsafe {
                                buffer_writes
                                    .as_ptr()
                                    .offset(buffer_writes.len() as isize - buffers.len() as isize)
                            };
                            write.descriptor_type = binding_info.descriptor_type;
                            write.descriptor_count = buffers.len() as u32;
                        }
                        VkBoundResource::StorageTexture(texture) => {
                            let texture_info = vk::DescriptorImageInfo {
                                image_view: *texture,
                                sampler: vk::Sampler::null(),
                                image_layout: vk::ImageLayout::GENERAL,
                            };
                            image_writes.push(texture_info);
                            write.p_image_info = unsafe {
                                image_writes
                                    .as_ptr()
                                    .offset(image_writes.len() as isize - 1)
                            };
                            write.descriptor_type = vk::DescriptorType::STORAGE_IMAGE;
                        }
                        VkBoundResource::StorageTextureArray(textures) => {
                            assert_eq!(binding_info.count, textures.len() as u32);

                            for texture in textures {
                                let texture_info = vk::DescriptorImageInfo {
                                    image_view: *texture,
                                    sampler: vk::Sampler::null(),
                                    image_layout: vk::ImageLayout::GENERAL,
                                };
                                image_writes.push(texture_info);
                            }
                            write.p_image_info = unsafe {
                                image_writes
                                    .as_ptr()
                                    .offset(image_writes.len() as isize - textures.len() as isize)
                            };
                            write.descriptor_type = vk::DescriptorType::STORAGE_IMAGE;
                            write.descriptor_count = textures.len() as u32;
                        }
                        VkBoundResource::UniformBuffer(VkBufferBindingInfo {
                            buffer,
                            offset,
                            length,
                        }) => {
                            assert!(
                                binding_info.descriptor_type
                                    == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                                    || binding_info.descriptor_type
                                        == vk::DescriptorType::UNIFORM_BUFFER
                            );

                            let buffer_info = vk::DescriptorBufferInfo {
                                buffer: *buffer,
                                offset: if binding_info.descriptor_type
                                    == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                                {
                                    0
                                } else {
                                    *offset as vk::DeviceSize
                                },
                                range: *length as vk::DeviceSize,
                            };
                            buffer_writes.push(buffer_info);
                            write.p_buffer_info = unsafe {
                                buffer_writes
                                    .as_ptr()
                                    .offset(buffer_writes.len() as isize - 1)
                            };
                            write.descriptor_type = binding_info.descriptor_type;
                        }
                        VkBoundResource::UniformBufferArray(buffers) => {
                            assert!(
                                binding_info.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER
                                    || binding_info.descriptor_type
                                        == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                            );
                            assert_eq!(binding_info.count, buffers.len() as u32);

                            for VkBufferBindingInfo {
                                buffer,
                                offset,
                                length,
                            } in buffers
                            {
                                let buffer_info = vk::DescriptorBufferInfo {
                                    buffer: *buffer,
                                    offset: if binding_info.descriptor_type
                                        == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                                    {
                                        0
                                    } else {
                                        *offset as vk::DeviceSize
                                    },
                                    range: *length as vk::DeviceSize,
                                };
                                buffer_writes.push(buffer_info);
                            }
                            write.p_buffer_info = unsafe {
                                buffer_writes
                                    .as_ptr()
                                    .offset(buffer_writes.len() as isize - buffers.len() as isize)
                            };
                            write.descriptor_type = binding_info.descriptor_type;
                            write.descriptor_count = buffers.len() as u32;
                        }
                        VkBoundResource::SampledTexture(texture) => {
                            let texture_info = vk::DescriptorImageInfo {
                                image_view: *texture,
                                sampler: vk::Sampler::null(),
                                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                            };
                            image_writes.push(texture_info);
                            write.p_image_info = unsafe {
                                image_writes
                                    .as_ptr()
                                    .offset(image_writes.len() as isize - 1)
                            };
                            write.descriptor_type = vk::DescriptorType::SAMPLED_IMAGE;
                        }
                        VkBoundResource::SampledTextureArray(textures) => {
                            assert_eq!(binding_info.count, textures.len() as u32);

                            for texture in textures {
                                let texture_info = vk::DescriptorImageInfo {
                                    image_view: *texture,
                                    sampler: vk::Sampler::null(),
                                    image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                                };
                                image_writes.push(texture_info);
                            }
                            write.p_image_info = unsafe {
                                image_writes
                                    .as_ptr()
                                    .offset(image_writes.len() as isize - textures.len() as isize)
                            };
                            write.descriptor_type = vk::DescriptorType::SAMPLED_IMAGE;
                            write.descriptor_count = textures.len() as u32;
                        }
                        VkBoundResource::SampledTextureAndSampler(texture, sampler) => {
                            let texture_info = vk::DescriptorImageInfo {
                                image_view: *texture,
                                sampler: *sampler,
                                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                            };
                            image_writes.push(texture_info);
                            write.p_image_info = unsafe {
                                image_writes
                                    .as_ptr()
                                    .offset(image_writes.len() as isize - 1)
                            };
                            write.descriptor_type = vk::DescriptorType::COMBINED_IMAGE_SAMPLER;
                        }
                        VkBoundResource::SampledTextureAndSamplerArray(textures_and_samplers) => {
                            assert_eq!(binding_info.count, textures_and_samplers.len() as u32);

                            for (texture, sampler) in textures_and_samplers {
                                let texture_info = vk::DescriptorImageInfo {
                                    image_view: *texture,
                                    sampler: *sampler,
                                    image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                                };
                                image_writes.push(texture_info);
                            }
                            write.p_image_info = unsafe {
                                image_writes.as_ptr().offset(
                                    image_writes.len() as isize
                                        - textures_and_samplers.len() as isize,
                                )
                            };
                            write.descriptor_type = vk::DescriptorType::SAMPLED_IMAGE;
                            write.descriptor_count = textures_and_samplers.len() as u32;
                        }
                        VkBoundResource::Sampler(sampler) => {
                            let texture_info = vk::DescriptorImageInfo {
                                image_view: vk::ImageView::null(),
                                sampler: *sampler,
                                image_layout: vk::ImageLayout::UNDEFINED,
                            };
                            image_writes.push(texture_info);
                            write.p_image_info = unsafe {
                                image_writes
                                    .as_ptr()
                                    .offset(image_writes.len() as isize - 1)
                            };
                            write.descriptor_type = vk::DescriptorType::SAMPLER;
                        }
                        VkBoundResource::AccelerationStructure(accel_struct) => {
                            acceleration_structures.push(*accel_struct);
                            let acceleration_structure_write =
                                vk::WriteDescriptorSetAccelerationStructureKHR {
                                    acceleration_structure_count: 1,
                                    p_acceleration_structures: unsafe {
                                        acceleration_structures
                                            .as_ptr()
                                            .offset(acceleration_structures.len() as isize - 1)
                                    },
                                    ..Default::default()
                                };
                            acceleration_structure_writes.push(acceleration_structure_write);
                            write.p_next = unsafe {
                                acceleration_structure_writes
                                    .as_ptr()
                                    .offset(acceleration_structure_writes.len() as isize - 1)
                                    as _
                            };
                            write.descriptor_type = vk::DescriptorType::ACCELERATION_STRUCTURE_KHR;
                        }
                        VkBoundResource::None => {
                            panic!("Shader expects resource in binding: {}", binding)
                        }
                    }
                    assert_eq!(
                        layout.binding_infos[binding]
                            .as_ref()
                            .unwrap()
                            .descriptor_type,
                        write.descriptor_type
                    );
                    writes.push(write);
                }
                unsafe {
                    device.update_descriptor_sets(&writes, &[]);
                }
            }
            Some(template) => {
                let mut entries: SmallVec<[VkDescriptorEntry; DEFAULT_PER_SET_PREALLOCATED_SIZE]> =
                    SmallVec::with_capacity(bindings.len());

                for (binding, resource) in stored_bindings.iter().enumerate() {
                    assert_ne!(entries.len(), entries.capacity());
                    let binding_info = layout.binding(binding as u32);
                    if binding_info.is_none() {
                        continue;
                    }
                    let binding_info = binding_info.unwrap();

                    match resource {
                        VkBoundResource::StorageBuffer(VkBufferBindingInfo {
                            buffer,
                            offset,
                            length,
                        }) => {
                            assert!(
                                binding_info.descriptor_type
                                    == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                                    || binding_info.descriptor_type
                                        == vk::DescriptorType::STORAGE_BUFFER
                            );

                            let mut entry = VkDescriptorEntry::default();
                            entry.buffer = vk::DescriptorBufferInfo {
                                buffer: *buffer,
                                offset: if binding_info.descriptor_type
                                    == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                                {
                                    0
                                } else {
                                    *offset as vk::DeviceSize
                                },
                                range: *length as vk::DeviceSize,
                            };
                            entries.push(entry);
                        }
                        VkBoundResource::StorageBufferArray(buffers) => {
                            assert!(
                                binding_info.descriptor_type
                                    == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                                    || binding_info.descriptor_type
                                        == vk::DescriptorType::STORAGE_BUFFER
                            );
                            assert_eq!(binding_info.count, buffers.len() as u32);

                            for VkBufferBindingInfo {
                                buffer,
                                offset,
                                length,
                            } in buffers
                            {
                                let mut entry = VkDescriptorEntry::default();
                                entry.buffer = vk::DescriptorBufferInfo {
                                    buffer: *buffer,
                                    offset: if binding_info.descriptor_type
                                        == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                                    {
                                        0
                                    } else {
                                        *offset as vk::DeviceSize
                                    },
                                    range: *length as vk::DeviceSize,
                                };
                                entries.push(entry);
                            }
                        }
                        VkBoundResource::UniformBuffer(VkBufferBindingInfo {
                            buffer,
                            offset,
                            length,
                        }) => {
                            assert!(
                                binding_info.descriptor_type
                                    == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                                    || binding_info.descriptor_type
                                        == vk::DescriptorType::UNIFORM_BUFFER
                            );

                            let mut entry = VkDescriptorEntry::default();
                            entry.buffer = vk::DescriptorBufferInfo {
                                buffer: *buffer,
                                offset: if binding_info.descriptor_type
                                    == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                                {
                                    0
                                } else {
                                    *offset as vk::DeviceSize
                                },
                                range: *length as vk::DeviceSize,
                            };
                            entries.push(entry);
                        }
                        VkBoundResource::UniformBufferArray(buffers) => {
                            assert!(
                                binding_info.descriptor_type
                                    == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                                    || binding_info.descriptor_type
                                        == vk::DescriptorType::UNIFORM_BUFFER
                            );
                            assert_eq!(binding_info.count, buffers.len() as u32);

                            for VkBufferBindingInfo {
                                buffer,
                                offset,
                                length,
                            } in buffers
                            {
                                let mut entry = VkDescriptorEntry::default();
                                entry.buffer = vk::DescriptorBufferInfo {
                                    buffer: *buffer,
                                    offset: if binding_info.descriptor_type
                                        == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                                    {
                                        0
                                    } else {
                                        *offset as vk::DeviceSize
                                    },
                                    range: *length as vk::DeviceSize,
                                };
                                entries.push(entry);
                            }
                        }
                        VkBoundResource::SampledTexture(texture) => {
                            let mut entry = VkDescriptorEntry::default();
                            entry.image = vk::DescriptorImageInfo {
                                image_view: *texture,
                                sampler: vk::Sampler::null(),
                                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                            };
                            entries.push(entry);
                        }
                        VkBoundResource::SampledTextureArray(textures) => {
                            assert_eq!(binding_info.count, textures.len() as u32);

                            for texture in textures {
                                let mut entry = VkDescriptorEntry::default();
                                entry.image = vk::DescriptorImageInfo {
                                    image_view: *texture,
                                    sampler: vk::Sampler::null(),
                                    image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                                };
                                entries.push(entry);
                            }
                        }
                        VkBoundResource::SampledTextureAndSampler(texture, sampler) => {
                            let mut entry = VkDescriptorEntry::default();
                            entry.image = vk::DescriptorImageInfo {
                                image_view: *texture,
                                sampler: *sampler,
                                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                            };
                            entries.push(entry);
                        }
                        VkBoundResource::SampledTextureAndSamplerArray(textures_and_samplers) => {
                            assert_eq!(binding_info.count, textures_and_samplers.len() as u32);

                            for (texture, sampler) in textures_and_samplers {
                                let mut entry = VkDescriptorEntry::default();
                                entry.image = vk::DescriptorImageInfo {
                                    image_view: *texture,
                                    sampler: *sampler,
                                    image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                                };
                                entries.push(entry);
                            }
                        }
                        VkBoundResource::StorageTexture(texture) => {
                            let mut entry = VkDescriptorEntry::default();
                            entry.image = vk::DescriptorImageInfo {
                                image_view: *texture,
                                sampler: vk::Sampler::null(),
                                image_layout: vk::ImageLayout::GENERAL,
                            };
                            entries.push(entry);
                        }
                        VkBoundResource::StorageTextureArray(textures) => {
                            assert_eq!(binding_info.count, textures.len() as u32);

                            for texture in textures {
                                let mut entry = VkDescriptorEntry::default();
                                entry.image = vk::DescriptorImageInfo {
                                    image_view: *texture,
                                    sampler: vk::Sampler::null(),
                                    image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                                };
                                entries.push(entry);
                            }
                        }
                        VkBoundResource::Sampler(sampler) => {
                            let mut entry = VkDescriptorEntry::default();
                            entry.image = vk::DescriptorImageInfo {
                                image_view: vk::ImageView::null(),
                                sampler: *sampler,
                                image_layout: vk::ImageLayout::UNDEFINED,
                            };
                            entries.push(entry);
                        }
                        VkBoundResource::AccelerationStructure(acceleration_structure) => {
                            let mut entry = VkDescriptorEntry::default();
                            entry.acceleration_structure = *acceleration_structure;
                            entries.push(entry);
                        }
                        _ => {}
                    }
                }
                unsafe {
                    device.update_descriptor_set_with_template(
                        set,
                        template,
                        entries.as_ptr() as *const c_void,
                    );
                }
            }
        }

        Ok(Self {
            descriptor_set: set,
            pool: pool.clone(),
            layout: layout.clone(),
            is_transient,
            bindings: stored_bindings,
            device: device.clone(),
        })
    }

    #[inline]
    pub(crate) fn handle(&self) -> vk::DescriptorSet {
        self.descriptor_set
    }

    #[allow(unused)]
    #[inline]
    pub(crate) fn is_transient(&self) -> bool {
        self.is_transient
    }

    pub(crate) fn is_compatible<'a, T>(
        &self,
        layout: &'a Arc<VkDescriptorSetLayout>,
        bindings: &'a [T],
    ) -> bool
    where
        VkBoundResource: BindingCompare<Option<&'a T>>,
    {
        if &self.layout != layout {
            return false;
        }

        self.bindings.iter().enumerate().all(|(index, binding)| {
            let binding_info = self.layout.binding(index as u32);
            binding.binding_eq(&bindings.get(index), binding_info)
        })
    }
}

impl Drop for VkDescriptorSet {
    fn drop(&mut self) {
        if self.is_transient {
            return;
        }
        unsafe {
            self.device
                .free_descriptor_sets(*self.pool.handle(), &[self.descriptor_set])
                .unwrap();
        }
    }
}

#[derive(Hash, Eq, PartialEq, Clone)]
pub(crate) struct VkBufferBindingInfo {
    pub(crate) buffer: vk::Buffer,
    pub(crate) offset: u64,
    pub(crate) length: u64,
}

#[derive(Hash, Eq, PartialEq, Clone)]
pub(crate) enum VkBoundResource {
    None,
    UniformBuffer(VkBufferBindingInfo),
    UniformBufferArray(SmallVec<[VkBufferBindingInfo; DEFAULT_DESCRIPTOR_ARRAY_SIZE]>),
    StorageBuffer(VkBufferBindingInfo),
    StorageBufferArray(SmallVec<[VkBufferBindingInfo; DEFAULT_DESCRIPTOR_ARRAY_SIZE]>),
    StorageTexture(vk::ImageView),
    StorageTextureArray(SmallVec<[vk::ImageView; DEFAULT_DESCRIPTOR_ARRAY_SIZE]>),
    SampledTexture(vk::ImageView),
    SampledTextureArray(SmallVec<[vk::ImageView; DEFAULT_DESCRIPTOR_ARRAY_SIZE]>),
    SampledTextureAndSampler(vk::ImageView, vk::Sampler),
    SampledTextureAndSamplerArray(
        SmallVec<[(vk::ImageView, vk::Sampler); DEFAULT_DESCRIPTOR_ARRAY_SIZE]>,
    ),
    Sampler(vk::Sampler),
    AccelerationStructure(vk::AccelerationStructureKHR),
}

impl Default for VkBoundResource {
    fn default() -> Self {
        VkBoundResource::None
    }
}

#[allow(unused)]
#[derive(Hash, Eq, PartialEq, Clone)]
pub(crate) enum VkBoundResourceRef<'a> {
    None,
    UniformBuffer(VkBufferBindingInfo),
    UniformBufferArray(&'a [VkBufferBindingInfo]),
    StorageBuffer(VkBufferBindingInfo),
    StorageBufferArray(&'a [VkBufferBindingInfo]),
    StorageTexture(vk::ImageView),
    StorageTextureArray(&'a [vk::ImageView]),
    SampledTexture(vk::ImageView),
    SampledTextureArray(&'a [vk::ImageView]),
    SampledTextureAndSampler(vk::ImageView, vk::Sampler),
    SampledTextureAndSamplerArray(&'a [(vk::ImageView, vk::Sampler)]),
    Sampler(vk::Sampler),
    AccelerationStructure(vk::AccelerationStructureKHR),
}

impl Default for VkBoundResourceRef<'_> {
    fn default() -> Self {
        Self::None
    }
}

impl From<&VkBoundResourceRef<'_>> for VkBoundResource {
    fn from(binding: &VkBoundResourceRef<'_>) -> Self {
        match binding {
            VkBoundResourceRef::None => VkBoundResource::None,
            VkBoundResourceRef::UniformBuffer(info) => VkBoundResource::UniformBuffer(info.clone()),
            VkBoundResourceRef::StorageBuffer(info) => VkBoundResource::StorageBuffer(info.clone()),
            VkBoundResourceRef::StorageTexture(view) => VkBoundResource::StorageTexture(*view),
            VkBoundResourceRef::SampledTexture(view) => VkBoundResource::SampledTexture(*view),
            VkBoundResourceRef::SampledTextureAndSampler(view, sampler) => {
                VkBoundResource::SampledTextureAndSampler(*view, *sampler)
            }
            VkBoundResourceRef::Sampler(sampler) => VkBoundResource::Sampler(*sampler),
            VkBoundResourceRef::AccelerationStructure(accel) => {
                VkBoundResource::AccelerationStructure(*accel)
            }
            VkBoundResourceRef::UniformBufferArray(arr) => VkBoundResource::UniformBufferArray(
                arr.iter()
                    .map(|a| {
                        let info: VkBufferBindingInfo = a.clone();
                        info
                    })
                    .collect(),
            ),
            VkBoundResourceRef::StorageBufferArray(arr) => VkBoundResource::StorageBufferArray(
                arr.iter()
                    .map(|a| {
                        let info: VkBufferBindingInfo = a.clone();
                        info
                    })
                    .collect(),
            ),
            VkBoundResourceRef::StorageTextureArray(arr) => {
                VkBoundResource::StorageTextureArray(arr.iter().map(|a| *a).collect())
            }
            VkBoundResourceRef::SampledTextureArray(arr) => {
                VkBoundResource::SampledTextureArray(arr.iter().map(|a| *a).collect())
            }
            VkBoundResourceRef::SampledTextureAndSamplerArray(arr) => {
                VkBoundResource::SampledTextureAndSamplerArray(
                    arr.iter()
                        .map(|(t, s)| {
                            let tuple: (vk::ImageView, vk::Sampler) = (*t, *s);
                            tuple
                        })
                        .collect(),
                )
            }
        }
    }
}

impl From<&Self> for VkBoundResource {
    fn from(other: &Self) -> Self {
        other.clone()
    }
}

pub(crate) trait BindingCompare<T> {
    fn binding_eq(&self, other: &T, binding_info: Option<&VkDescriptorSetEntryInfo>) -> bool;
}

impl BindingCompare<Self> for VkBoundResource {
    fn binding_eq(&self, other: &Self, binding_info: Option<&VkDescriptorSetEntryInfo>) -> bool {
        if self == &VkBoundResource::None && binding_info.is_none() {
            true
        } else if binding_info.is_none() {
            false
        } else if binding_info.unwrap().descriptor_type
            != vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
            && binding_info.unwrap().descriptor_type != vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
        {
            self == other
        } else {
            // https://github.com/rust-lang/rust/issues/53667
            if let (
                VkBoundResource::UniformBuffer(VkBufferBindingInfo {
                    buffer: entry_buffer,
                    offset: _,
                    length: entry_length,
                }),
                VkBoundResource::UniformBuffer(VkBufferBindingInfo {
                    buffer,
                    offset: _,
                    length,
                }),
            ) = (self, other)
            {
                buffer == entry_buffer && *length == *entry_length
            } else if let (
                VkBoundResource::StorageBuffer(VkBufferBindingInfo {
                    buffer: entry_buffer,
                    offset: _,
                    length: entry_length,
                }),
                VkBoundResource::StorageBuffer(VkBufferBindingInfo {
                    buffer,
                    offset: _,
                    length,
                }),
            ) = (self, other)
            {
                buffer == entry_buffer && *length == *entry_length
            } else if let (
                VkBoundResource::StorageBufferArray(arr),
                VkBoundResource::StorageBufferArray(arr1),
            ) = (self, other)
            {
                arr.iter()
                    .zip(arr1)
                    .all(|(b, b1)| b.buffer == b1.buffer && b.length == b1.length)
            } else if let (
                VkBoundResource::UniformBufferArray(arr),
                VkBoundResource::UniformBufferArray(arr1),
            ) = (self, other)
            {
                arr.iter()
                    .zip(arr1)
                    .all(|(b, b1)| b.buffer == b1.buffer && b.length == b1.length)
            } else {
                false
            }
        }
    }
}

impl BindingCompare<VkBoundResourceRef<'_>> for VkBoundResource {
    fn binding_eq(
        &self,
        other: &VkBoundResourceRef<'_>,
        binding_info: Option<&VkDescriptorSetEntryInfo>,
    ) -> bool {
        if self == &VkBoundResource::None && binding_info.is_none() {
            true
        } else if binding_info.is_none() {
            false
        } else if binding_info.unwrap().descriptor_type
            != vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
            && binding_info.unwrap().descriptor_type != vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
        {
            self == other
        } else {
            // https://github.com/rust-lang/rust/issues/53667
            if let (
                VkBoundResource::UniformBuffer(VkBufferBindingInfo {
                    buffer: entry_buffer,
                    offset: _,
                    length: entry_length,
                }),
                VkBoundResourceRef::UniformBuffer(VkBufferBindingInfo {
                    buffer,
                    offset: _,
                    length,
                }),
            ) = (self, other)
            {
                buffer == entry_buffer && *length == *entry_length
            } else if let (
                VkBoundResource::StorageBuffer(VkBufferBindingInfo {
                    buffer: entry_buffer,
                    offset: _,
                    length: entry_length,
                }),
                VkBoundResourceRef::StorageBuffer(VkBufferBindingInfo {
                    buffer,
                    offset: _,
                    length,
                }),
            ) = (self, other)
            {
                buffer == entry_buffer && *length == *entry_length
            } else if let (
                VkBoundResource::StorageBufferArray(arr),
                VkBoundResourceRef::StorageBufferArray(arr1),
            ) = (self, other)
            {
                arr.iter()
                    .zip(*arr1)
                    .all(|(b, b1)| b.buffer == b1.buffer && b.length == b1.length)
            } else if let (
                VkBoundResource::UniformBufferArray(arr),
                VkBoundResourceRef::UniformBufferArray(arr1),
            ) = (self, other)
            {
                arr.iter()
                    .zip(*arr1)
                    .all(|(b, b1)| b.buffer == b1.buffer && b.length == b1.length)
            } else {
                false
            }
        }
    }
}

impl BindingCompare<Option<&VkBoundResource>> for VkBoundResource {
    fn binding_eq(
        &self,
        other: &Option<&VkBoundResource>,
        binding_info: Option<&VkDescriptorSetEntryInfo>,
    ) -> bool {
        if let Some(other) = other {
            self.binding_eq(*other, binding_info)
        } else if self == &VkBoundResource::None {
            true
        } else {
            false
        }
    }
}

impl BindingCompare<Option<&VkBoundResourceRef<'_>>> for VkBoundResource {
    fn binding_eq(
        &self,
        other: &Option<&VkBoundResourceRef<'_>>,
        binding_info: Option<&VkDescriptorSetEntryInfo>,
    ) -> bool {
        if let Some(other) = other {
            self.binding_eq(*other, binding_info)
        } else if self == &VkBoundResource::None {
            true
        } else {
            false
        }
    }
}

impl PartialEq<VkBoundResourceRef<'_>> for VkBoundResource {
    fn eq(&self, other: &VkBoundResourceRef) -> bool {
        match (self, other) {
            (VkBoundResource::None, VkBoundResourceRef::None) => true,
            (
                VkBoundResource::UniformBuffer(VkBufferBindingInfo {
                    buffer: old,
                    offset: old_offset,
                    length: old_length,
                }),
                VkBoundResourceRef::UniformBuffer(VkBufferBindingInfo {
                    buffer: new,
                    offset: new_offset,
                    length: new_length,
                }),
            ) => old == new && old_offset == new_offset && old_length == new_length,
            (
                VkBoundResource::StorageBuffer(VkBufferBindingInfo {
                    buffer: old,
                    offset: old_offset,
                    length: old_length,
                }),
                VkBoundResourceRef::StorageBuffer(VkBufferBindingInfo {
                    buffer: new,
                    offset: new_offset,
                    length: new_length,
                }),
            ) => old == new && old_offset == new_offset && old_length == new_length,
            (VkBoundResource::StorageTexture(old), VkBoundResourceRef::StorageTexture(new)) => {
                old == new
            }
            (VkBoundResource::SampledTexture(old), VkBoundResourceRef::SampledTexture(new)) => {
                old == new
            }
            (
                VkBoundResource::SampledTextureAndSampler(old_tex, old_sampler),
                VkBoundResourceRef::SampledTextureAndSampler(new_tex, new_sampler),
            ) => old_tex == new_tex && old_sampler == new_sampler,
            (VkBoundResource::Sampler(old_sampler), VkBoundResourceRef::Sampler(new_sampler)) => {
                old_sampler == new_sampler
            }
            (
                VkBoundResource::AccelerationStructure(old),
                VkBoundResourceRef::AccelerationStructure(new),
            ) => old == new,
            (
                VkBoundResource::StorageBufferArray(old),
                VkBoundResourceRef::StorageBufferArray(new),
            ) => &old[..] == &new[..],
            (
                VkBoundResource::UniformBufferArray(old),
                VkBoundResourceRef::UniformBufferArray(new),
            ) => &old[..] == &new[..],
            (
                VkBoundResource::SampledTextureArray(old),
                VkBoundResourceRef::SampledTextureArray(new),
            ) => old.iter().zip(new.iter()).all(|(old, new)| old == new),
            (
                VkBoundResource::StorageTextureArray(old),
                VkBoundResourceRef::StorageTextureArray(new),
            ) => old.iter().zip(new.iter()).all(|(old, new)| old == new),
            (
                VkBoundResource::SampledTextureAndSamplerArray(old),
                VkBoundResourceRef::SampledTextureAndSamplerArray(new),
            ) => old.iter().zip(new.iter()).all(
                |((old_texture, old_sampler), (new_texture, new_sampler))| {
                    old_texture == new_texture && old_sampler == new_sampler
                },
            ),
            _ => false,
        }
    }
}

impl PartialEq<VkBoundResource> for VkBoundResourceRef<'_> {
    fn eq(&self, other: &VkBoundResource) -> bool {
        other == self
    }
}

pub(crate) struct VkDescriptorSetBinding {
    pub(crate) set: Arc<VkDescriptorSet>,
    pub(crate) dynamic_offsets: SmallVec<[u32; 4]>,
}

struct VkDescriptorSetCacheEntry {
    set: Arc<VkDescriptorSet>,
    last_used_frame: u64,
}

#[allow(unused)]
#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq)]
enum CacheMode {
    None,
    PerFrame,
    Everything,
}

struct DescriptorPools {
    pools: Vec<Arc<VkDescriptorPool>>,
    next_non_full_pool_index: u32,
}

pub(crate) struct VkBindingManager {
    cache_mode: CacheMode,
    transient_pools: RefCell<DescriptorPools>,
    permanent_pools: RefCell<DescriptorPools>,
    device: Arc<RawVkDevice>,
    current_sets: [Option<Arc<VkDescriptorSet>>; gpu::NON_BINDLESS_SET_COUNT as usize],
    dirty: DirtyDescriptorSets,
    bindings:
        [[VkBoundResource; gpu::PER_SET_BINDINGS as usize]; gpu::NON_BINDLESS_SET_COUNT as usize],
    transient_cache: RefCell<HashMap<Arc<VkDescriptorSetLayout>, Vec<VkDescriptorSetCacheEntry>>>,
    permanent_cache: RefCell<HashMap<Arc<VkDescriptorSetLayout>, Vec<VkDescriptorSetCacheEntry>>>,
    last_cleanup_frame: u64,
}

impl VkBindingManager {
    pub(crate) fn new(device: &Arc<RawVkDevice>) -> Self {
        let transient_pool = Arc::new(VkDescriptorPool::new(device, true));
        let permanent_pool = Arc::new(VkDescriptorPool::new(device, false));

        let cache_mode = CacheMode::Everything;

        Self {
            cache_mode,
            transient_pools: RefCell::new(DescriptorPools {
                pools: vec![transient_pool],
                next_non_full_pool_index: 0,
            }),
            permanent_pools: RefCell::new(DescriptorPools {
                pools: vec![permanent_pool],
                next_non_full_pool_index: 0,
            }),
            device: device.clone(),
            current_sets: Default::default(),
            dirty: DirtyDescriptorSets::all(),
            bindings: Default::default(),
            transient_cache: RefCell::new(HashMap::new()),
            permanent_cache: RefCell::new(HashMap::new()),
            last_cleanup_frame: 0,
        }
    }

    pub(crate) fn reset(&mut self, frame: u64) {
        self.dirty = DirtyDescriptorSets::all();
        self.bindings = Default::default();
        self.current_sets = Default::default();
        self.clean_permanent_cache(frame);
        if self.cache_mode != CacheMode::None {
            let mut transient_cache_mut = self.transient_cache.borrow_mut();
            transient_cache_mut.clear();
        }
        let mut transient_pools_mut = self.transient_pools.borrow_mut();
        transient_pools_mut.next_non_full_pool_index = 0u32;
        for pool in transient_pools_mut.pools.iter_mut() {
            pool.reset();
        }
        let mut permanent_pools_mut = self.permanent_pools.borrow_mut();
        permanent_pools_mut.next_non_full_pool_index = 0u32;
    }

    pub(crate) fn bind(
        &mut self,
        frequency: gpu::BindingFrequency,
        slot: u32,
        binding: VkBoundResourceRef,
    ) {
        let bindings_table = &mut self.bindings[frequency as usize];
        let existing_binding = &mut bindings_table[slot as usize];

        let identical = existing_binding == &binding;

        if !identical {
            self.dirty.insert(DirtyDescriptorSets::from(frequency));
            *existing_binding = (&binding).into();
        }
    }

    fn find_compatible_set<'a, T>(
        &self,
        frame: u64,
        layout: &'a Arc<VkDescriptorSetLayout>,
        bindings: &'a [T],
        use_permanent_cache: bool,
    ) -> Option<Arc<VkDescriptorSet>>
    where
        VkBoundResource: BindingCompare<Option<&'a T>>,
    {
        let mut cache = if use_permanent_cache {
            self.permanent_cache.borrow_mut()
        } else {
            self.transient_cache.borrow_mut()
        };

        let mut entry_opt = cache.get_mut(layout).and_then(|sets| {
            sets.iter_mut()
                .find(|entry| entry.set.is_compatible(layout, bindings))
        });
        if let Some(entry) = &mut entry_opt {
            entry.last_used_frame = frame;
        }
        entry_opt.map(|entry| entry.set.clone())
    }

    fn finish_set(
        &mut self,
        frame: u64,
        pipeline_layout: &VkPipelineLayout,
        frequency: gpu::BindingFrequency,
    ) -> Option<VkDescriptorSetBinding> {
        let layout_option = pipeline_layout.descriptor_set_layout(frequency as u32);
        if !self.dirty.contains(DirtyDescriptorSets::from(frequency)) || layout_option.is_none() {
            return None;
        }
        let layout = layout_option.unwrap();

        let mut set: Option<Arc<VkDescriptorSet>> = None;
        let bindings =
            &self.bindings[frequency as usize][..(layout.max_used_binding() + 1) as usize];
        if let Some(current_set) = &self.current_sets[frequency as usize] {
            // This should cover the hottest case.
            if current_set.is_compatible(layout, bindings) {
                set = Some(current_set.clone());
            }
        }

        set = set.or_else(|| self.get_or_create_set(frame, layout, bindings));
        self.current_sets[frequency as usize] = set.clone();
        set.map(|set| self.get_descriptor_set_binding_info(set, bindings))
    }

    fn get_descriptor_set_binding_info(
        &self,
        set: Arc<VkDescriptorSet>,
        bindings: &[VkBoundResource],
    ) -> VkDescriptorSetBinding {
        let mut set_binding = VkDescriptorSetBinding {
            set: set.clone(),
            dynamic_offsets: Default::default(),
        };
        bindings.iter().enumerate().for_each(|(index, binding)| {
            if let Some(binding_info) = set.layout.binding(index as u32) {
                if binding_info.descriptor_type == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                    || binding_info.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                {
                    match binding {
                        VkBoundResource::UniformBuffer(VkBufferBindingInfo {
                            buffer: _,
                            offset,
                            length: _,
                        }) => {
                            set_binding.dynamic_offsets.push(*offset as u32);
                        }
                        VkBoundResource::StorageBuffer(VkBufferBindingInfo {
                            buffer: _,
                            offset,
                            length: _,
                        }) => {
                            set_binding.dynamic_offsets.push(*offset as u32);
                        }
                        VkBoundResource::StorageBufferArray(buffers) => {
                            for VkBufferBindingInfo {
                                buffer: _,
                                offset,
                                length: _,
                            } in buffers
                            {
                                set_binding.dynamic_offsets.push(*offset as u32);
                            }
                        }
                        VkBoundResource::UniformBufferArray(buffers) => {
                            for VkBufferBindingInfo {
                                buffer: _,
                                offset,
                                length: _,
                            } in buffers
                            {
                                set_binding.dynamic_offsets.push(*offset as u32);
                            }
                        }
                        _ => {}
                    }
                }
            }
        });

        set_binding
    }

    pub fn get_or_create_set<'a, T>(
        &self,
        frame: u64,
        layout: &'a Arc<VkDescriptorSetLayout>,
        bindings: &'a [T],
    ) -> Option<Arc<VkDescriptorSet>>
    where
        VkBoundResource: BindingCompare<Option<&'a T>>,
        VkBoundResource: From<&'a T>,
    {
        if layout.is_empty() {
            return None;
        }

        let transient = self.cache_mode != CacheMode::Everything;

        let cached_set = if self.cache_mode == CacheMode::None {
            None
        } else {
            self.find_compatible_set(frame, layout, &bindings, !transient)
        };
        let set: Arc<VkDescriptorSet> = if let Some(cached_set) = cached_set {
            cached_set
        } else {
            let mut pools = if !transient {
                self.permanent_pools.borrow_mut()
            } else {
                self.transient_pools.borrow_mut()
            };
            let mut new_set = Option::<VkDescriptorSet>::None;

            'pools_iter: for i in (pools.next_non_full_pool_index as usize)..pools.pools.len() {
                let pool = &pools.pools[i];
                let set_res = VkDescriptorSet::new(pool, &self.device, layout, transient, bindings);
                match set_res {
                    Ok(set) => {
                        new_set = Some(set);
                        break 'pools_iter;
                    }
                    Err(vk::Result::ERROR_OUT_OF_HOST_MEMORY) => panic!("Out of host memory."),
                    _ => {
                        if i == pools.next_non_full_pool_index as usize {
                            pools.next_non_full_pool_index += 1;
                        }
                    }
                }
            }

            if new_set.is_none() {
                let pool = Arc::new(VkDescriptorPool::new(&self.device, transient));
                new_set =
                    VkDescriptorSet::new(&pool, &self.device, layout, transient, bindings).ok();
                pools.pools.push(pool);
            }
            let new_set = Arc::new(new_set.unwrap());

            if self.cache_mode != CacheMode::None {
                let mut cache = if transient {
                    self.transient_cache.borrow_mut()
                } else {
                    self.permanent_cache.borrow_mut()
                };
                cache
                    .entry(layout.clone())
                    .or_default()
                    .push(VkDescriptorSetCacheEntry {
                        set: new_set.clone(),
                        last_used_frame: frame,
                    });
            }
            new_set
        };
        Some(set)
    }

    pub fn mark_all_dirty(&mut self) {
        self.dirty |= DirtyDescriptorSets::VERY_FREQUENT;
        self.dirty |= DirtyDescriptorSets::FREQUENT;
        self.dirty |= DirtyDescriptorSets::FRAME;
    }

    #[allow(unused)]
    #[inline(always)]
    pub fn dirty_sets(&self) -> DirtyDescriptorSets {
        self.dirty
    }

    pub(super) fn finish(
        &mut self,
        frame: u64,
        pipeline_layout: &VkPipelineLayout,
    ) -> [Option<VkDescriptorSetBinding>; gpu::NON_BINDLESS_SET_COUNT as usize] {
        if self.dirty.is_empty() {
            return Default::default();
        }

        let mut set_bindings: [Option<VkDescriptorSetBinding>;
            gpu::NON_BINDLESS_SET_COUNT as usize] = Default::default();
        set_bindings[gpu::BindingFrequency::VeryFrequent as usize] =
            self.finish_set(frame, pipeline_layout, gpu::BindingFrequency::VeryFrequent);
        set_bindings[gpu::BindingFrequency::Frame as usize] =
            self.finish_set(frame, pipeline_layout, gpu::BindingFrequency::Frame);
        set_bindings[gpu::BindingFrequency::Frequent as usize] =
            self.finish_set(frame, pipeline_layout, gpu::BindingFrequency::Frequent);

        self.dirty = DirtyDescriptorSets::empty();
        set_bindings
    }

    const FRAMES_BETWEEN_CLEANUP: u64 = 0;
    const MAX_FRAMES_SET_UNUSED: u64 = 16;
    fn clean_permanent_cache(&mut self, frame: u64) {
        // TODO: I might need to make this more aggressive because of memory usage.

        if self.cache_mode != CacheMode::Everything
            || frame - self.last_cleanup_frame < Self::FRAMES_BETWEEN_CLEANUP
        {
            return;
        }

        let mut cache_mut = self.permanent_cache.borrow_mut();
        for entries in cache_mut.values_mut() {
            entries.retain(|entry| (frame - entry.last_used_frame) < Self::MAX_FRAMES_SET_UNUSED);
        }
        self.last_cleanup_frame = frame;
    }
}
