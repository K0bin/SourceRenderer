use std::{sync::{Arc, Mutex, MutexGuard}, cell::RefCell};
use ash::{vk, prelude::VkResult};
use crate::{raw::{RawVkDevice, VkFeatures}, texture::VkSampler, rt::VkAccelerationStructure};
use sourcerenderer_core::graphics::{BindingFrequency};
use std::collections::HashMap;

use crate::texture::VkTextureView;
use crate::buffer::VkBufferSlice;
use std::hash::{Hash, Hasher};
use crate::pipeline::VkPipelineLayout;
use std::ffi::c_void;
use smallvec::SmallVec;

// TODO: clean up descriptor management
// TODO: determine descriptor and set counts

// TODO: this shit is really slow. rewrite all of it.

bitflags! {
    pub struct DirtyDescriptorSets: u32 {
        const PER_DRAW = 0b0001;
        const PER_MATERIAL = 0b0010;
        const PER_FRAME = 0b0100;
        const BINDLESS_TEXTURES = 0b10000;
    }
}

impl From<BindingFrequency> for DirtyDescriptorSets {
  fn from(value: BindingFrequency) -> Self {
    match value {
      BindingFrequency::PerDraw => DirtyDescriptorSets::PER_DRAW,
      BindingFrequency::PerMaterial => DirtyDescriptorSets::PER_MATERIAL,
      BindingFrequency::PerFrame => DirtyDescriptorSets::PER_FRAME,
    }
  }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub(crate) struct VkDescriptorSetEntryInfo {
  pub(crate) shader_stage: vk::ShaderStageFlags,
  pub(crate) index: u32,
  pub(crate) count: u32,
  pub(crate) descriptor_type: vk::DescriptorType,
  pub(crate) writable: bool,
  pub(crate) flags: vk::DescriptorBindingFlags
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
  binding_infos: [Option<VkDescriptorSetEntryInfo>; 16],
  template: Option<vk::DescriptorUpdateTemplate>
}

impl VkDescriptorSetLayout {
  pub fn new(bindings: &[VkDescriptorSetEntryInfo], flags: vk::DescriptorSetLayoutCreateFlags, device: &Arc<RawVkDevice>) -> Self {
    let mut vk_bindings: Vec<vk::DescriptorSetLayoutBinding> = Vec::new();
    let mut vk_binding_flags: Vec<vk::DescriptorBindingFlags> = Vec::new();
    let mut vk_template_entries: Vec<vk::DescriptorUpdateTemplateEntry> = Vec::new();
    let mut binding_infos: [Option<VkDescriptorSetEntryInfo>; 16] = Default::default();

    for binding in bindings.iter() {
      binding_infos[binding.index as usize] = Some(binding.clone());

      vk_bindings.push(vk::DescriptorSetLayoutBinding {
        binding: binding.index,
        descriptor_count: binding.count,
        descriptor_type: binding.descriptor_type,
        stage_flags: binding.shader_stage,
        p_immutable_samplers: std::ptr::null()
      });
      vk_binding_flags.push(binding.flags);

      vk_template_entries.push(vk::DescriptorUpdateTemplateEntry {
        dst_binding: binding.index,
        dst_array_element: 0,
        descriptor_count: 1,
        descriptor_type: binding.descriptor_type,
        offset: binding.index as usize * std::mem::size_of::<VkDescriptorEntry>(),
        stride: std::mem::size_of::<VkDescriptorEntry>()
      });
    }

    let binding_flags_struct = vk::DescriptorSetLayoutBindingFlagsCreateInfo {
      binding_count: vk_binding_flags.len() as u32,
      p_binding_flags: vk_binding_flags.as_ptr(),
      ..Default::default()
    };

    let info = vk::DescriptorSetLayoutCreateInfo {
      p_next: if device.features.contains(VkFeatures::DESCRIPTOR_INDEXING) { &binding_flags_struct as *const vk::DescriptorSetLayoutBindingFlagsCreateInfo as *const c_void } else { std::ptr::null() },
      p_bindings: vk_bindings.as_ptr(),
      binding_count: vk_bindings.len() as u32,
      flags,
      ..Default::default()
    };
    let layout = unsafe {
      device.create_descriptor_set_layout(&info, None)
    }.unwrap();

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
      set: 0
    };
    let template = if !flags.contains(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL_EXT) && !vk_template_entries.is_empty() &&
      device.features.contains(VkFeatures::DESCRIPTOR_TEMPLATE) {
      Some(unsafe {
        device.create_descriptor_update_template(&template_info, None)
      }.unwrap())
    } else {
      None
    };

    Self {
      device: device.clone(),
      layout,
      binding_infos,
      template
    }
  }

  pub(crate) fn handle(&self) -> &vk::DescriptorSetLayout {
    &self.layout
  }

  pub(crate) fn binding_count(&self) -> usize {
    self.binding_infos.len()
  }

  pub(crate) fn is_dynamic_binding(&self, binding_index: u32) -> bool {
    if let Some(binding_info) = self.binding_infos[binding_index as usize].as_ref() {
      binding_info.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC || binding_info.descriptor_type == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
    } else {
      false
    }
  }
}

impl Drop for VkDescriptorSetLayout {
  fn drop(&mut self) {
    unsafe {
      if let Some(template) = self.template {
        self.device.destroy_descriptor_update_template(template, None);
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
  is_transient: bool
}

impl VkDescriptorPool {
  fn new(device: &Arc<RawVkDevice>, is_transient: bool) -> Self {
    // TODO figure out proper numbers
    let pool_sizes = [vk::DescriptorPoolSize {
      ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
      descriptor_count: 256
    }, vk::DescriptorPoolSize {
      ty: vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
      descriptor_count: 512
    }, vk::DescriptorPoolSize {
      ty: vk::DescriptorType::UNIFORM_BUFFER,
      descriptor_count: 256
    }, vk::DescriptorPoolSize {
      ty: vk::DescriptorType::STORAGE_BUFFER,
      descriptor_count: 256
    }, vk::DescriptorPoolSize {
      ty: vk::DescriptorType::STORAGE_BUFFER_DYNAMIC,
      descriptor_count: 256
    }];
    let info = vk::DescriptorPoolCreateInfo {
      max_sets: 128,
      p_pool_sizes: pool_sizes.as_ptr(),
      pool_size_count: pool_sizes.len() as u32,
      flags: if !is_transient { vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET } else { vk::DescriptorPoolCreateFlags::empty() },
      ..Default::default()
    };
    let descriptor_pool = Mutex::new(unsafe {
      device.create_descriptor_pool(&info, None)
    }.unwrap());
    Self {
      descriptor_pool,
      device: device.clone(),
      is_transient
    }
  }

  #[inline]
  fn handle(&self) -> MutexGuard<vk::DescriptorPool> {
    self.descriptor_pool.lock().unwrap()
  }

  fn reset(&self) {
    if !self.is_transient {
      return;
    }
    let guard = self.handle();
    unsafe {
      self.device.reset_descriptor_pool(*guard, vk::DescriptorPoolResetFlags::empty()).unwrap();
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
  acceleration_structure: vk::AccelerationStructureKHR
}

impl Default for VkDescriptorEntry {
  fn default() -> Self {
    Self {
      buffer: vk::DescriptorBufferInfo::default()
    }
  }
}

pub(crate) struct VkDescriptorSet {
  descriptor_set: vk::DescriptorSet,
  pool: Arc<VkDescriptorPool>,
  layout: Arc<VkDescriptorSetLayout>,
  is_transient: bool,
  bindings: [VkBoundResource; 16],
  device: Arc<RawVkDevice>
}

impl VkDescriptorSet {
  fn new(pool: &Arc<VkDescriptorPool>, device: &Arc<RawVkDevice>, layout: &Arc<VkDescriptorSetLayout>, is_transient: bool, bindings: &[VkBoundResourceRef; 16]) -> VkResult<Self> {
    let pool_guard = pool.handle();
    let set_create_info = vk::DescriptorSetAllocateInfo {
      descriptor_pool: *pool_guard,
      descriptor_set_count: 1,
      p_set_layouts: layout.handle() as *const vk::DescriptorSetLayout,
      ..Default::default()
    };
    let set = unsafe {
      device.allocate_descriptor_sets(&set_create_info)
    }?.pop().unwrap();

    match Option::<vk::DescriptorUpdateTemplate>::None {
      None => {
        let mut writes: SmallVec<[vk::WriteDescriptorSet; 16]> = Default::default();
        let mut image_writes: SmallVec<[vk::DescriptorImageInfo; 16]> = Default::default();
        let mut buffer_writes: SmallVec<[vk::DescriptorBufferInfo; 16]> = Default::default();
        let mut acceleration_structures: SmallVec<[vk::AccelerationStructureKHR; 2]> = Default::default();
        let mut acceleration_structure_writes: SmallVec<[vk::WriteDescriptorSetAccelerationStructureKHR; 2]> = Default::default();
        for (binding, resource) in bindings.iter().enumerate() {
          // We're using pointers to elements in those vecs, so we cant relocate
          assert_ne!(writes.len(), writes.capacity());
          assert_ne!(image_writes.len(), image_writes.capacity());
          assert_ne!(buffer_writes.len(), buffer_writes.capacity());
          assert_ne!(acceleration_structures.len(), acceleration_structures.capacity());
          assert_ne!(acceleration_structure_writes.len(), acceleration_structure_writes.capacity());

          let binding_info = &layout.binding_infos[binding].as_ref();
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
            VkBoundResourceRef::StorageBuffer { buffer, offset, length } => {
              assert!(binding_info.descriptor_type == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC || binding_info.descriptor_type == vk::DescriptorType::STORAGE_BUFFER);

              let buffer_info = vk::DescriptorBufferInfo {
                buffer: *buffer.buffer().handle(),
                offset: if binding_info.descriptor_type == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC { 0 } else { (buffer.offset() + *offset) as vk::DeviceSize },
                range: *length as vk::DeviceSize
              };
              buffer_writes.push(buffer_info);
              write.p_buffer_info = unsafe { buffer_writes.as_ptr().offset(buffer_writes.len() as isize - 1) };
              write.descriptor_type = binding_info.descriptor_type;
            },
            VkBoundResourceRef::StorageTexture(texture) => {
              let texture_info = vk::DescriptorImageInfo {
                image_view: *texture.view_handle(),
                sampler: vk::Sampler::null(),
                image_layout: vk::ImageLayout::GENERAL
              };
              image_writes.push(texture_info);
              write.p_image_info = unsafe { image_writes.as_ptr().offset(image_writes.len() as isize - 1) };
              write.descriptor_type = vk::DescriptorType::STORAGE_IMAGE;
            },
            VkBoundResourceRef::UniformBuffer { buffer, offset, length } => {
              assert!(binding_info.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC || binding_info.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER);

              let buffer_info = vk::DescriptorBufferInfo {
                buffer: *buffer.buffer().handle(),
                offset: if binding_info.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC { 0 } else { (buffer.offset() + *offset) as vk::DeviceSize },
                range: *length as vk::DeviceSize
              };
              buffer_writes.push(buffer_info);
              write.p_buffer_info = unsafe { buffer_writes.as_ptr().offset(buffer_writes.len() as isize - 1) };
              write.descriptor_type = binding_info.descriptor_type;
            },
            VkBoundResourceRef::SampledTexture(texture) => {
              let texture_info = vk::DescriptorImageInfo {
                image_view: *texture.view_handle(),
                sampler: vk::Sampler::null(),
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
              };
              image_writes.push(texture_info);
              write.p_image_info = unsafe { image_writes.as_ptr().offset(image_writes.len() as isize - 1) };
              write.descriptor_type = vk::DescriptorType::SAMPLED_IMAGE;
            },
            VkBoundResourceRef::SampledTextureAndSampler(texture, sampler) => {
              let texture_info = vk::DescriptorImageInfo {
                image_view: *texture.view_handle(),
                sampler: *sampler.handle(),
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
              };
              image_writes.push(texture_info);
              write.p_image_info = unsafe { image_writes.as_ptr().offset(image_writes.len() as isize - 1) };
              write.descriptor_type = vk::DescriptorType::COMBINED_IMAGE_SAMPLER;
            },
            VkBoundResourceRef::Sampler(sampler) => {
              let texture_info = vk::DescriptorImageInfo {
                image_view: vk::ImageView::null(),
                sampler: *sampler.handle(),
                image_layout: vk::ImageLayout::UNDEFINED
              };
              image_writes.push(texture_info);
              write.p_image_info = unsafe { image_writes.as_ptr().offset(image_writes.len() as isize - 1) };
              write.descriptor_type = vk::DescriptorType::SAMPLER;
            },
            VkBoundResourceRef::AccelerationStructure(accel_struct) => {
              acceleration_structures.push(*accel_struct.handle());
              let acceleration_structure_write = vk::WriteDescriptorSetAccelerationStructureKHR {
                acceleration_structure_count: 1,
                p_acceleration_structures: unsafe { acceleration_structures.as_ptr().offset(acceleration_structures.len() as isize - 1) },
                ..Default::default()
              };
              acceleration_structure_writes.push(acceleration_structure_write);
              write.p_next = unsafe { acceleration_structure_writes.as_ptr().offset(acceleration_structure_writes.len() as isize - 1) as _ };
              write.descriptor_type = vk::DescriptorType::ACCELERATION_STRUCTURE_KHR;
            },
            VkBoundResourceRef::None => panic!("Shader expectes resource in binding: {}", binding)
          }
          assert_eq!(layout.binding_infos[binding].as_ref().unwrap().descriptor_type, write.descriptor_type);
          writes.push(write);
        }
        unsafe {
          device.update_descriptor_sets(&writes, &[]);
        }
      },
      Some(template) => {
        let mut entries: SmallVec<[VkDescriptorEntry; 16]> = Default::default();

        for (binding, resource) in bindings.iter().enumerate() {
          let binding_info = &layout.binding_infos[binding].as_ref();
          if binding_info.is_none() {
            continue;
          }
          let binding_info = binding_info.unwrap();

          let mut entry = VkDescriptorEntry::default();
          match resource {
            VkBoundResourceRef::StorageBuffer { buffer, offset, length } => {
              assert!(binding_info.descriptor_type == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC || binding_info.descriptor_type == vk::DescriptorType::STORAGE_BUFFER);

              entry.buffer = vk::DescriptorBufferInfo {
                buffer: *buffer.buffer().handle(),
                offset: if binding_info.descriptor_type == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC { 0 } else { (buffer.offset() + *offset) as vk::DeviceSize },
                range: *length as vk::DeviceSize
              };
            },
            VkBoundResourceRef::UniformBuffer { buffer, offset, length } => {
              assert!(binding_info.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC || binding_info.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER);

              entry.buffer = vk::DescriptorBufferInfo {
                buffer: *buffer.buffer().handle(),
                offset: if binding_info.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC { 0 } else { (buffer.offset() + *offset) as vk::DeviceSize },
                range: *length as vk::DeviceSize
              };
            },
            VkBoundResourceRef::SampledTexture(texture) => {
              entry.image = vk::DescriptorImageInfo {
                image_view: *texture.view_handle(),
                sampler: vk::Sampler::null(),
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
              };
            },
            VkBoundResourceRef::SampledTextureAndSampler(texture, sampler) => {
              entry.image = vk::DescriptorImageInfo {
                image_view: *texture.view_handle(),
                sampler: *sampler.handle(),
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
              };
            },
            VkBoundResourceRef::StorageTexture(texture) => {
              entry.image = vk::DescriptorImageInfo {
                image_view: *texture.view_handle(),
                sampler: vk::Sampler::null(),
                image_layout: vk::ImageLayout::GENERAL
              };
            },
            VkBoundResourceRef::Sampler(sampler) => {
              entry.image = vk::DescriptorImageInfo {
                image_view: vk::ImageView::null(),
                sampler: *sampler.handle(),
                image_layout: vk::ImageLayout::UNDEFINED
              };
            },
            VkBoundResourceRef::AccelerationStructure(acceleration_structure) => {
              entry.acceleration_structure = *acceleration_structure.handle();
            },
            _ => {}
          }
          entries.push(entry);
        }
        unsafe {
          device.update_descriptor_set_with_template(set, template, entries.as_ptr() as *const c_void);
        }
      }
    }

    let mut stored_bindings: [VkBoundResource; 16] = Default::default();
    for (index, binding) in bindings.iter().enumerate() {
      stored_bindings[index] = binding.into();
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
  pub(crate) fn handle(&self) -> &vk::DescriptorSet {
    &self.descriptor_set
  }

  #[inline]
  pub(crate) fn is_transient(&self) -> bool {
    self.is_transient
  }
}

impl Drop for VkDescriptorSet {
  fn drop(&mut self) {
    if self.is_transient {
      return;
    }
    unsafe {
      self.device.free_descriptor_sets(*self.pool.handle(), &[self.descriptor_set]).unwrap();
    }
  }
}

#[derive(Hash, Eq, PartialEq, Clone)]
pub(crate) enum VkBoundResource {
  None,
  UniformBuffer{
    buffer: Arc<VkBufferSlice>,
    offset: usize,
    length: usize,
  },
  StorageBuffer{
    buffer: Arc<VkBufferSlice>,
    offset: usize,
    length: usize,
  },
  StorageTexture(Arc<VkTextureView>),
  SampledTexture(Arc<VkTextureView>),
  SampledTextureAndSampler(Arc<VkTextureView>, Arc<VkSampler>),
  Sampler(Arc<VkSampler>),
  AccelerationStructure(Arc<VkAccelerationStructure>),
}

impl Default for VkBoundResource {
  fn default() -> Self {
    VkBoundResource::None
  }
}

#[derive(Hash, Eq, PartialEq, Clone)]
pub(crate) enum VkBoundResourceRef<'a> {
  None,
  UniformBuffer {
    buffer: &'a Arc<VkBufferSlice>,
    offset: usize,
    length: usize,
  },
  StorageBuffer {
    buffer: &'a Arc<VkBufferSlice>,
    offset: usize,
    length: usize,
  },
  StorageTexture(&'a Arc<VkTextureView>),
  SampledTextureAndSampler(&'a Arc<VkTextureView>, &'a Arc<VkSampler>),
  SampledTexture(&'a Arc<VkTextureView>),
  Sampler(&'a Arc<VkSampler>),
  AccelerationStructure(&'a Arc<VkAccelerationStructure>),
}

impl Default for VkBoundResourceRef<'_> {
  fn default() -> Self {
    Self::None
  }
}

impl<'a> From<&'a VkBoundResource> for VkBoundResourceRef<'a> {
  fn from(binding: &'a VkBoundResource) -> Self {
    match binding {
      VkBoundResource::None => VkBoundResourceRef::None,
      VkBoundResource::UniformBuffer { buffer, offset, length } => VkBoundResourceRef::UniformBuffer { buffer, offset: *offset, length: *length },
      VkBoundResource::StorageBuffer { buffer, offset, length } => VkBoundResourceRef::StorageBuffer { buffer, offset: *offset, length: *length },
      VkBoundResource::StorageTexture(view) => VkBoundResourceRef::StorageTexture(view),
      VkBoundResource::SampledTexture(view) => VkBoundResourceRef::SampledTexture(view),
      VkBoundResource::SampledTextureAndSampler(view, sampler) => VkBoundResourceRef::SampledTextureAndSampler(view, sampler),
      VkBoundResource::Sampler(sampler) => VkBoundResourceRef::Sampler(sampler),
      VkBoundResource::AccelerationStructure(accel) => VkBoundResourceRef::AccelerationStructure(accel),
    }
  }
}

impl From<&VkBoundResourceRef<'_>> for VkBoundResource {
  fn from(binding: &VkBoundResourceRef<'_>) -> Self {
    match binding {
      VkBoundResourceRef::None => VkBoundResource::None,
      VkBoundResourceRef::UniformBuffer { buffer, offset, length } => VkBoundResource::UniformBuffer { buffer: (*buffer).clone(), offset: *offset, length: *length },
      VkBoundResourceRef::StorageBuffer { buffer, offset, length } => VkBoundResource::StorageBuffer { buffer: (*buffer).clone(), offset: *offset, length: *length },
      VkBoundResourceRef::StorageTexture(view) => VkBoundResource::StorageTexture((*view).clone()),
      VkBoundResourceRef::SampledTexture(view) => VkBoundResource::SampledTexture((*view).clone()),
      VkBoundResourceRef::SampledTextureAndSampler(view, sampler) => VkBoundResource::SampledTextureAndSampler((*view).clone(), (*sampler).clone()),
      VkBoundResourceRef::Sampler(sampler) => VkBoundResource::Sampler((*sampler).clone()),
      VkBoundResourceRef::AccelerationStructure(accel) => VkBoundResource::AccelerationStructure((*accel).clone()),
    }
  }
}

impl PartialEq<VkBoundResourceRef<'_>> for VkBoundResource {
  fn eq(&self, other: &VkBoundResourceRef) -> bool {
    match (self, other) {
      (VkBoundResource::None, VkBoundResourceRef::None) => true,
      (VkBoundResource::UniformBuffer {
        buffer: old, offset: old_offset, length: old_length
      }, VkBoundResourceRef::UniformBuffer {
        buffer: new, offset: new_offset, length: new_length
      }) => old == *new && *old_offset == *new_offset && *old_length == *new_length,
      (VkBoundResource::StorageBuffer {
        buffer: old, offset: old_offset, length: old_length
      }, VkBoundResourceRef::StorageBuffer {
        buffer: new, offset: new_offset, length: new_length
      }) => old == *new && *old_offset == *new_offset && *old_length == *new_length,
      (VkBoundResource::StorageTexture(old), VkBoundResourceRef::StorageTexture(new)) => old == *new,
      (VkBoundResource::SampledTexture(old), VkBoundResourceRef::SampledTexture(new)) => old == *new,
      (VkBoundResource::SampledTextureAndSampler(old_tex, old_sampler), VkBoundResourceRef::SampledTextureAndSampler(new_tex, new_sampler)) => old_tex == *new_tex && old_sampler == *new_sampler,
      (VkBoundResource::Sampler(old_sampler), VkBoundResourceRef::Sampler(new_sampler)) => old_sampler == *new_sampler,
      (VkBoundResource::AccelerationStructure(old), VkBoundResourceRef::AccelerationStructure(new)) => old == *new,
      _ => false
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
  pub(crate) dynamic_offset_count: u32,
  pub(crate) dynamic_offsets: [u64; 8]
}

struct VkDescriptorSetCacheEntry {
  set: Arc<VkDescriptorSet>,
  last_used_frame: u64
}

pub(crate) struct VkBindingManager {
  transient_pools: RefCell<Vec<Arc<VkDescriptorPool>>>,
  permanent_pools: RefCell<Vec<Arc<VkDescriptorPool>>>,
  device: Arc<RawVkDevice>,
  current_sets: [Option<VkDescriptorSet>; 4],
  dirty: DirtyDescriptorSets,
  bindings: [[VkBoundResource; 16]; 4],
  transient_cache: RefCell<HashMap<Arc<VkDescriptorSetLayout>, Vec<VkDescriptorSetCacheEntry>>>,
  permanent_cache: RefCell<HashMap<Arc<VkDescriptorSetLayout>, Vec<VkDescriptorSetCacheEntry>>>,
  last_cleanup_frame: u64
}

impl VkBindingManager {
  pub(crate) fn new(device: &Arc<RawVkDevice>) -> Self {
    let transient_pool = Arc::new(VkDescriptorPool::new(device, true));
    let permanent_pool = Arc::new(VkDescriptorPool::new(device, false));

    Self {
      transient_pools: RefCell::new(vec![transient_pool]),
      permanent_pools: RefCell::new(vec![permanent_pool]),
      device: device.clone(),
      current_sets: Default::default(),
      dirty: DirtyDescriptorSets::empty(),
      bindings: Default::default(),
      transient_cache: RefCell::new(HashMap::new()),
      permanent_cache: RefCell::new(HashMap::new()),
      last_cleanup_frame: 0
    }
  }

  pub(crate) fn reset(&mut self) {
    self.dirty = DirtyDescriptorSets::empty();
    self.bindings = Default::default();
    let mut transient_cache_mut = self.transient_cache.borrow_mut();
    transient_cache_mut.clear();
    let mut transient_pools_mut = self.transient_pools.borrow_mut();
    for pool in transient_pools_mut.iter_mut() {
      pool.reset();
    }
    let mut permanent_pools_mut = self.permanent_pools.borrow_mut();
    for pool in permanent_pools_mut.iter_mut() {
      pool.reset();
    }
  }

  pub(crate) fn bind(&mut self, frequency: BindingFrequency, slot: u32, binding: VkBoundResourceRef) {
    let bindings_table = &mut self.bindings[frequency as usize];
    let existing_binding = &mut bindings_table[slot as usize];

    let identical = existing_binding == &binding;

    if !identical {
      self.dirty.insert(DirtyDescriptorSets::from(frequency));
      *existing_binding = (&binding).into();
    }
  }

  fn find_compatible_set(&self, frame: u64, layout: &Arc<VkDescriptorSetLayout>, bindings: &[VkBoundResourceRef; 16], use_permanent_cache: bool) -> Option<Arc<VkDescriptorSet>> {
    // TODO: use a hashmap with the layout as the key
    let mut cache = if use_permanent_cache { self.permanent_cache.borrow_mut() } else { self.transient_cache.borrow_mut() };

    let mut entry_opt = cache
        .get_mut(layout)
        .and_then(|sets| {
          sets
            .iter_mut()
            .find(|entry|
              &entry.set.layout == layout // We cache descriptor set entries, so this should be fine.
              && entry.set.bindings.iter().enumerate().all(|(index, binding)| {
                let binding_info = entry.set.layout.binding_infos[index].as_ref();
                if binding_info.is_none() {
                  false
                } else if binding_info.unwrap().descriptor_type != vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC && binding_info.unwrap().descriptor_type != vk::DescriptorType::STORAGE_BUFFER_DYNAMIC {
                  binding == &bindings[index]
                } else {
                  // https://github.com/rust-lang/rust/issues/53667
                  if let (VkBoundResource::UniformBuffer{ buffer: entry_buffer, offset: _, length: entry_length }, VkBoundResourceRef::UniformBuffer { buffer, offset: _, length }) = (binding, &bindings[index]) {
                    buffer.buffer() == entry_buffer.buffer()
                      && *length == *entry_length
                  } else if let (VkBoundResource::StorageBuffer{ buffer: entry_buffer, offset: _, length: entry_length }, VkBoundResourceRef::StorageBuffer { buffer, offset: _, length }) = (binding, &bindings[index]) {
                    buffer.buffer() == entry_buffer.buffer()
                    && *length == *entry_length
                  } else {
                    false
                  }
                }
              })
            )
        });
    if let Some(entry) = &mut entry_opt {
      entry.last_used_frame = frame;
    }
    entry_opt.map(|entry| entry.set.clone())
  }

  fn finish_set<'a>(&mut self, frame: u64, pipeline_layout: &VkPipelineLayout, frequency: BindingFrequency) -> Option<VkDescriptorSetBinding> {
    let layout_option = pipeline_layout.descriptor_set_layout(frequency as u32);
    if !self.dirty.contains(DirtyDescriptorSets::from(frequency)) || layout_option.is_none() {
      return None;
    }

    let mut binding_refs = <[VkBoundResourceRef<'a>; 16]>::default();
    for (index, binding) in self.bindings[frequency as usize].iter().enumerate() {
      binding_refs[index] = binding.into();
    }
    self.get_or_create_set(frame, layout_option.unwrap(), &binding_refs)
  }

  pub fn get_or_create_set(&self, frame: u64, layout: &Arc<VkDescriptorSetLayout>, bindings: &[VkBoundResourceRef; 16]) -> Option<VkDescriptorSetBinding> {
    if layout.binding_count() == 0 {
      return None;
    }

    let cached_set = self.find_compatible_set(frame, layout, &bindings, false);

    let set = if let Some(cached_set) = cached_set {
      cached_set
    } else {
      let transient = true;
      let mut pools = if !transient { self.permanent_pools.borrow_mut() } else { self.transient_pools.borrow_mut() };
      let mut new_set = Option::<VkDescriptorSet>::None;
      'pools_iter: for pool in pools.iter() {
        let set_res = VkDescriptorSet::new(pool, &self.device, layout, transient, &bindings);
        match set_res {
          Ok(set) => {
            new_set = Some(set);
            break 'pools_iter;
          },
          Err(vk::Result::ERROR_OUT_OF_HOST_MEMORY) => panic!("Out of host memory."),
          _ => {}
        }
      }
      if new_set.is_none() {
        let pool = Arc::new(VkDescriptorPool::new(&self.device, transient));
        new_set = VkDescriptorSet::new(&pool, &self.device, layout, transient, &bindings).ok();
        pools.push(pool);
      }
      let new_set = Arc::new(new_set.unwrap());

      let mut cache = if transient { self.transient_cache.borrow_mut() } else { self.permanent_cache.borrow_mut() };
      cache.entry(layout.clone()).or_default().push(VkDescriptorSetCacheEntry {
        set: new_set.clone(),
        last_used_frame: frame
      });
      new_set
    };
    let mut set_binding = VkDescriptorSetBinding {
      set,
      dynamic_offsets: Default::default(),
      dynamic_offset_count: 0
    };
    bindings.iter().enumerate().for_each(|(index, binding)| {
      if let Some(binding_info) = layout.binding_infos[index].as_ref() {
        if binding_info.descriptor_type == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC || binding_info.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC {
          match binding {
            VkBoundResourceRef::UniformBuffer { buffer, offset, length: _ } => {
              set_binding.dynamic_offsets[set_binding.dynamic_offset_count as usize] = (buffer.offset() + offset) as u64;
              set_binding.dynamic_offset_count += 1;
            }
            VkBoundResourceRef::StorageBuffer { buffer, offset, length: _ } => {
              set_binding.dynamic_offsets[set_binding.dynamic_offset_count as usize] = (buffer.offset() + offset) as u64;
              set_binding.dynamic_offset_count += 1;
            },
            _ => {}
          }
        }
      }
    });
    Some(set_binding)
  }

  pub fn mark_all_dirty(&mut self) {
    self.dirty |= DirtyDescriptorSets::PER_DRAW;
    self.dirty |= DirtyDescriptorSets::PER_MATERIAL;
    self.dirty |= DirtyDescriptorSets::PER_FRAME;
    self.dirty |= DirtyDescriptorSets::BINDLESS_TEXTURES;
  }

  pub fn dirty_sets(&self) -> DirtyDescriptorSets {
    self.dirty
  }

  pub fn finish(&mut self, frame: u64, pipeline_layout: &VkPipelineLayout) -> [Option<VkDescriptorSetBinding>; 3] {
    if self.dirty.is_empty() {
      return Default::default();
    }
    self.clean(frame);

    let mut set_bindings: [Option<VkDescriptorSetBinding>; 3] = Default::default();
    set_bindings[BindingFrequency::PerDraw as usize] = self.finish_set(frame, pipeline_layout, BindingFrequency::PerDraw);
    set_bindings[BindingFrequency::PerFrame as usize] = self.finish_set(frame, pipeline_layout, BindingFrequency::PerFrame);
    set_bindings[BindingFrequency::PerMaterial as usize] = self.finish_set(frame, pipeline_layout, BindingFrequency::PerMaterial);

    self.dirty = DirtyDescriptorSets::empty();
    set_bindings
  }

  const FRAMES_BETWEEN_CLEANUP: u64 = 5;
  const MAX_FRAMES_SET_UNUSED: u64 = 5;
  fn clean(&mut self, frame: u64) {
    if frame - self.last_cleanup_frame <= Self::FRAMES_BETWEEN_CLEANUP {
      return;
    }

    let mut cache_mut = self.permanent_cache.borrow_mut();
    for entries in cache_mut.values_mut() {
      entries.retain(|entry| {
        frame - entry.last_used_frame >= Self::MAX_FRAMES_SET_UNUSED
      });
    }
    self.last_cleanup_frame = frame;
  }
}
