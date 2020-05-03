use std::sync::Arc;
use ash::vk;
use raw::RawVkDevice;
use ash::version::DeviceV1_0;
use std::ops::Deref;
use ash::prelude::VkResult;
use sourcerenderer_core::graphics::{ShaderType, BindingFrequency};
use std::collections::HashMap;
use std::cell::RefCell;
use ::{VkPipeline, VkTexture};
use bitflags::_core::cell::RefMut;
use texture::VkTextureShaderResourceView;
use buffer::VkBufferSlice;

#[derive(Clone, Eq, PartialEq, Hash)]
pub(crate) enum VkDescriptorType {
  UniformBuffer,
  Sampler,
  Texture,
  CombinedTextureSampler
}

/*bitflags! {
    struct DirtyDescriptorSets: u32 {
        const PER_DRAW = BindingFrequency::PerDraw;
        const PER_MATERIAL = BindingFrequency::PerMaterial;
        const PER_MODEL = BindingFrequency::PerModel;
        const RARELY = BindingFrequency::Rarely;
    }
}*/

#[derive(Clone, Eq, PartialEq, Hash)]
pub(crate) struct VkDescriptorSetBindingInfo {
  pub(crate) shader_stage: vk::ShaderStageFlags,
  pub(crate) index: u32,
  pub(crate) descriptor_type: vk::DescriptorType
}

pub(crate) struct VkDescriptorSetLayout {
  pub device: Arc<RawVkDevice>,
  layout: vk::DescriptorSetLayout
}

impl VkDescriptorSetLayout {
  pub fn new(bindings: &[VkDescriptorSetBindingInfo], device: &Arc<RawVkDevice>) -> Self {
    let vk_bindings: Vec<vk::DescriptorSetLayoutBinding> = bindings.iter()
      .map(|binding| vk::DescriptorSetLayoutBinding {
        binding: binding.index,
        descriptor_count: 1,
        descriptor_type: binding.descriptor_type,
        stage_flags: binding.shader_stage,
        p_immutable_samplers: std::ptr::null()
      }).collect();

    let info = vk::DescriptorSetLayoutCreateInfo {
      p_bindings: vk_bindings.as_ptr(),
      binding_count: vk_bindings.len() as u32,
      ..Default::default()
    };
    let layout = unsafe {
      device.create_descriptor_set_layout(&info, None)
    }.unwrap();
    Self {
      device: device.clone(),
      layout
    }
  }

  pub(crate) fn get_handle(&self) -> &vk::DescriptorSetLayout {
    &self.layout
  }
}

impl Drop for VkDescriptorSetLayout {
  fn drop(&mut self) {
    unsafe {
      self.device.destroy_descriptor_set_layout(self.layout, None);
    }
  }
}

pub(crate) struct VkBindingManager {
  transient_pool: vk::DescriptorPool,
  //permanent_pool: vk::DescriptorPool,
  device: Arc<RawVkDevice>,
  current_sets: RefCell<HashMap<BindingFrequency, vk::DescriptorSet>>
}

impl VkBindingManager {
  pub(crate) fn new(device: &Arc<RawVkDevice>) -> Self {
    // TODO: figure out count values
    let pool_sizes = [vk::DescriptorPoolSize {
      ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
      descriptor_count: 128
    }, vk::DescriptorPoolSize {
      ty: vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
      descriptor_count: 32
    }, vk::DescriptorPoolSize {
      ty: vk::DescriptorType::UNIFORM_BUFFER,
      descriptor_count: 128
    }];
    let info = vk::DescriptorPoolCreateInfo {
      max_sets: 16,
      p_pool_sizes: pool_sizes.as_ptr(),
      pool_size_count: pool_sizes.len() as u32,
      ..Default::default()
    };
    let transient_pool = unsafe {
      device.create_descriptor_pool(&info, None)
    }.unwrap();

    Self {
      transient_pool,
      device: device.clone(),
      current_sets: RefCell::new(HashMap::new())
    }
  }

  pub(crate) fn reset(&self) {
    unsafe {
      self.device.reset_descriptor_pool(self.transient_pool, vk::DescriptorPoolResetFlags::empty());
    }
  }

  #[inline]
  fn get_set_or_create(&self, frequency: BindingFrequency, layout: &VkDescriptorSetLayout) -> vk::DescriptorSet {
    let mut sets_ref = self.current_sets.borrow_mut();
    *sets_ref.entry(frequency).or_insert_with(|| {
      let set_create_info = vk::DescriptorSetAllocateInfo {
        descriptor_pool: self.transient_pool,
        descriptor_set_count: 1,
        p_set_layouts: layout.get_handle() as *const vk::DescriptorSetLayout,
        ..Default::default()
      };
      unsafe {
        self.device.allocate_descriptor_sets(&set_create_info)
      }.unwrap().pop().unwrap()
    })
  }

  pub(crate) fn bind_texture_view(&self, frequency: BindingFrequency, layout: &VkDescriptorSetLayout, binding: u32, texture: &VkTextureShaderResourceView) {
    let set = self.get_set_or_create(frequency, layout);

    let image_info = vk::DescriptorImageInfo {
      sampler: *texture.get_sampler_handle(),
      image_view: *texture.get_view_handle(),
      image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
    };

    let write = [vk::WriteDescriptorSet {
      dst_set: set,
      dst_binding: binding,
      dst_array_element: 0,
      descriptor_count: 1,
      descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
      p_image_info: &image_info as *const vk::DescriptorImageInfo,
      ..Default::default()
    }];
    unsafe { self.device.update_descriptor_sets(&write, &[]); }
  }

  pub(crate) fn bind_buffer(&self, frequency: BindingFrequency, layout: &VkDescriptorSetLayout, binding: u32, buffer: &VkBufferSlice) {
    let set = self.get_set_or_create(frequency, layout);

    let buffer_info = vk::DescriptorBufferInfo {
      buffer: *buffer.get_buffer().get_handle(),
      offset: buffer.get_offset_and_length().0 as vk::DeviceSize,
      range: buffer.get_offset_and_length().1 as vk::DeviceSize
    };

    let write = [vk::WriteDescriptorSet {
      dst_set: set,
      dst_binding: binding,
      dst_array_element: 0,
      descriptor_count: 1,
      descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
      p_buffer_info: &buffer_info,
      ..Default::default()
    }];
    unsafe { self.device.update_descriptor_sets(&write, &[]); }
  }

  pub fn finish(&self, frequency: BindingFrequency) -> Option<vk::DescriptorSet> {
    let mut sets_ref = self.current_sets.borrow_mut();
    sets_ref.remove(&frequency)
  }
}

impl Drop for VkBindingManager {
  fn drop(&mut self) {
    unsafe {
      self.device.destroy_descriptor_pool(self.transient_pool, None);
    }
  }
}
