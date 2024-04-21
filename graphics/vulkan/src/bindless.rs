use std::ffi::CString;
use std::sync::{
    Arc,
    Mutex,
};

use ash::vk::{
    self,
    Handle as _
};
use smallvec::SmallVec;

use super::*;

pub(super) const BINDLESS_TEXTURE_COUNT: u32 = 500_000;
pub(super) const BINDLESS_TEXTURE_SET_INDEX: u32 = 3;

pub struct VkBindlessDescriptorSet {
    device: Arc<RawVkDevice>,
    inner: Mutex<VkBindlessInner>,
    descriptor_count: u32,
    layout: Arc<VkDescriptorSetLayout>,
    key: VkDescriptorSetLayoutKey,
}

pub struct VkBindlessInner {
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
}

impl VkBindlessDescriptorSet {
    pub fn new(device: &Arc<RawVkDevice>) -> Self {
        let mut bindings = SmallVec::<[VkDescriptorSetEntryInfo; 16]>::new();
        bindings.push(VkDescriptorSetEntryInfo {
            name: "bindless_textures".to_string(),
            shader_stage: vk::ShaderStageFlags::VERTEX
                | vk::ShaderStageFlags::FRAGMENT
                | vk::ShaderStageFlags::COMPUTE,
            index: 0,
            descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
            count: BINDLESS_TEXTURE_COUNT,
            writable: false,
            flags: vk::DescriptorBindingFlags::UPDATE_AFTER_BIND_EXT
                | vk::DescriptorBindingFlags::UPDATE_UNUSED_WHILE_PENDING_EXT
                | vk::DescriptorBindingFlags::PARTIALLY_BOUND_EXT,
        });

        let key = VkDescriptorSetLayoutKey {
            bindings,
            flags: vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL_EXT,
        };
        let layout = Arc::new(VkDescriptorSetLayout::new(&key.bindings, key.flags, device));

        let pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::SAMPLED_IMAGE,
            descriptor_count: BINDLESS_TEXTURE_COUNT,
        }];
        let descriptor_pool = unsafe {
            device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo {
                        flags: vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND_EXT,
                        max_sets: 1,
                        pool_size_count: pool_sizes.len() as u32,
                        p_pool_sizes: pool_sizes.as_ptr(),
                        ..Default::default()
                    },
                    None,
                )
                .unwrap()
        };

        if let Some(debug_utils) = device.instance.debug_utils.as_ref() {
            let name_cstring = CString::new("BindlessTexturesPool").unwrap();
            unsafe {
                debug_utils
                    .debug_utils_loader
                    .set_debug_utils_object_name(
                        device.handle(),
                        &vk::DebugUtilsObjectNameInfoEXT {
                            object_type: vk::ObjectType::DESCRIPTOR_POOL,
                            object_handle: descriptor_pool.as_raw(),
                            p_object_name: name_cstring.as_ptr(),
                            ..Default::default()
                        },
                    )
                    .unwrap();
            }
        }

        let descriptor_set = unsafe {
            device
                .allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo {
                    descriptor_pool,
                    descriptor_set_count: 1,
                    p_set_layouts: &layout.handle() as *const vk::DescriptorSetLayout,
                    ..Default::default()
                })
                .unwrap()
                .pop()
                .unwrap()
        };

        if let Some(debug_utils) = device.instance.debug_utils.as_ref() {
            let name_cstring = CString::new("BindlessTextures").unwrap();
            unsafe {
                debug_utils
                    .debug_utils_loader
                    .set_debug_utils_object_name(
                        device.handle(),
                        &vk::DebugUtilsObjectNameInfoEXT {
                            object_type: vk::ObjectType::DESCRIPTOR_SET,
                            object_handle: descriptor_set.as_raw(),
                            p_object_name: name_cstring.as_ptr(),
                            ..Default::default()
                        },
                    )
                    .unwrap();
            }
        }

        Self {
            device: device.clone(),
            descriptor_count: BINDLESS_TEXTURE_COUNT,
            inner: Mutex::new(VkBindlessInner {
                descriptor_pool,
                descriptor_set,
            }),
            layout,
            key,
        }
    }

    pub(super) fn layout(&self) -> (&VkDescriptorSetLayoutKey, &Arc<VkDescriptorSetLayout>) {
        (&self.key, &self.layout)
    }

    pub fn descriptor_set_handle(&self) -> vk::DescriptorSet {
        let lock = self.inner.lock().unwrap();
        lock.descriptor_set
    }

    pub fn write_texture_descriptor(&self, slot: u32, texture: &VkTextureView) {
        let lock = self.inner.lock().unwrap();

        let image_info = vk::DescriptorImageInfo {
            sampler: vk::Sampler::null(),
            image_view: texture.view_handle(),
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        };
        unsafe {
            self.device.update_descriptor_sets(
                &[vk::WriteDescriptorSet {
                    dst_set: lock.descriptor_set,
                    dst_binding: 0,
                    dst_array_element: slot,
                    descriptor_count: 1,
                    descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
                    p_image_info: &image_info as *const vk::DescriptorImageInfo,
                    p_buffer_info: std::ptr::null(),
                    p_texel_buffer_view: std::ptr::null(),
                    ..Default::default()
                }],
                &[],
            );
        }
    }
}

impl Drop for VkBindlessDescriptorSet {
    fn drop(&mut self) {
        unsafe {
            let lock = self.inner.lock().unwrap();
            self.device
                .destroy_descriptor_pool(lock.descriptor_pool, None);
        }
    }
}
