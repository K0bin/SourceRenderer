use std::{sync::{Arc, Mutex}, ffi::CString};

use ash::vk::{self, Handle};
use crate::{raw::RawVkDevice, shared::VkDescriptorSetLayoutKey, descriptor::{VkDescriptorSetEntryInfo, VkDescriptorSetLayout}, texture::VkTextureView};

pub(crate) const BINDLESS_TEXTURE_COUNT: u32 = 500_000;
pub(crate) const BINDLESS_TEXTURE_SET_INDEX: u32 = 3;

pub struct VkBindlessDescriptorSet {
  device: Arc<RawVkDevice>,
  inner: Mutex<VkBindlessInner>,
  descriptor_count: u32,
  descriptor_type: vk::DescriptorType,
  layout: Arc<VkDescriptorSetLayout>,
  key: VkDescriptorSetLayoutKey
}

pub struct VkBindlessInner {
  descriptor_pool: vk::DescriptorPool,
  descriptor_set: vk::DescriptorSet,
  next_free_index: u32,
  free_indices: Vec<u32>
}

impl VkBindlessDescriptorSet {
  pub fn new(device: &Arc<RawVkDevice>, descriptor_type: vk::DescriptorType) -> Self {
    let key = VkDescriptorSetLayoutKey {
      bindings: vec![VkDescriptorSetEntryInfo {
        shader_stage: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT | vk::ShaderStageFlags::COMPUTE,
        index: 0,
        descriptor_type,
        count: BINDLESS_TEXTURE_COUNT,
        writable: descriptor_type != vk::DescriptorType::SAMPLED_IMAGE && descriptor_type != vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        flags: vk::DescriptorBindingFlags::UPDATE_AFTER_BIND_EXT | vk::DescriptorBindingFlags::UPDATE_UNUSED_WHILE_PENDING_EXT | vk::DescriptorBindingFlags::PARTIALLY_BOUND_EXT
      }],
      flags: vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL_EXT
    };
    let layout = Arc::new(VkDescriptorSetLayout::new(&key.bindings, key.flags, device));

    let pool_sizes = [vk::DescriptorPoolSize {
      ty: descriptor_type,
      descriptor_count: BINDLESS_TEXTURE_COUNT,
    }];
    let descriptor_pool = unsafe {
      device.create_descriptor_pool(&vk::DescriptorPoolCreateInfo {
        flags: vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND_EXT,
        max_sets: 1,
        pool_size_count: pool_sizes.len() as u32,
        p_pool_sizes: pool_sizes.as_ptr(),
        ..Default::default()
      }, None).unwrap()
    };

    if let Some(debug_utils) = device.instance.debug_utils.as_ref() {
      let name_cstring = CString::new("BindlessTexturesPool").unwrap();
      unsafe {
        debug_utils.debug_utils_loader.debug_utils_set_object_name(device.handle(), &vk::DebugUtilsObjectNameInfoEXT {
          object_type: vk::ObjectType::DESCRIPTOR_POOL,
          object_handle: descriptor_pool.as_raw(),
          p_object_name: name_cstring.as_ptr(),
          ..Default::default()
        }).unwrap();
      }
    }

    let descriptor_set = unsafe {
      device.allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo {
        descriptor_pool,
        descriptor_set_count: 1,
        p_set_layouts: layout.handle(),
        ..Default::default()
      }).unwrap().pop().unwrap()
    };

    if let Some(debug_utils) = device.instance.debug_utils.as_ref() {
      let name_cstring = CString::new("BindlessTextures").unwrap();
      unsafe {
        debug_utils.debug_utils_loader.debug_utils_set_object_name(device.handle(), &vk::DebugUtilsObjectNameInfoEXT {
          object_type: vk::ObjectType::DESCRIPTOR_SET,
          object_handle: descriptor_set.as_raw(),
          p_object_name: name_cstring.as_ptr(),
          ..Default::default()
        }).unwrap();
      }
    }

    Self {
      device: device.clone(),
      descriptor_count: BINDLESS_TEXTURE_COUNT,
      descriptor_type,
      inner: Mutex::new(VkBindlessInner {
        descriptor_pool,
        descriptor_set,
        next_free_index: 0,
        free_indices: Vec::new()
      }),
      layout,
      key
    }
  }

  pub(crate) fn layout(&self) -> (&VkDescriptorSetLayoutKey, &Arc<VkDescriptorSetLayout>) {
    (&self.key, &self.layout)
  }

  pub fn descriptor_set_handle(&self) -> vk::DescriptorSet {
    let lock = self.inner.lock().unwrap();
    lock.descriptor_set
  }

  pub fn write_texture_descriptor(&self, texture: &Arc<VkTextureView>) -> u32 {
    assert_eq!(self.descriptor_type, vk::DescriptorType::SAMPLED_IMAGE);

    let mut lock = self.inner.lock().unwrap();
    let index = if let Some(idx) = lock.free_indices.pop() {
      idx
    } else {
      lock.next_free_index += 1;
      lock.next_free_index - 1
    };
    let image_info = vk::DescriptorImageInfo {
      sampler: vk::Sampler::null(),
      image_view: *texture.view_handle(),
      image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    };
    unsafe {
      self.device.update_descriptor_sets(&[
        vk::WriteDescriptorSet {
          dst_set: lock.descriptor_set,
          dst_binding: 0,
          dst_array_element: index,
          descriptor_count: 1,
          descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
          p_image_info: &image_info as *const vk::DescriptorImageInfo,
          p_buffer_info: std::ptr::null(),
          p_texel_buffer_view: std::ptr::null(),
          ..Default::default()
        }
      ], &[]);
    }
    index
  }

  pub fn free_slot(&self, slot: u32) {
    // Mark slot as free. Only necessary for the index allocator.
    // The descriptor set itself is fine with dead entries.
    let mut lock = self.inner.lock().unwrap();
    lock.free_indices.push(slot);
  }
}

impl Drop for VkBindlessDescriptorSet {
  fn drop(&mut self) {
    let lock = self.inner.lock().unwrap();
    unsafe {
      self.device.destroy_descriptor_pool(lock.descriptor_pool, None);
    }
  }
}

