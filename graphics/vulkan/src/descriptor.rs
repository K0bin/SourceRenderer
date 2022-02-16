use std::sync::{Arc, Mutex, MutexGuard};
use ash::vk;
use crate::{raw::{RawVkDevice, VkFeatures}, texture::VkSampler};
use sourcerenderer_core::graphics::{BindingFrequency};
use std::collections::HashMap;

use crate::texture::VkTextureView;
use crate::buffer::VkBufferSlice;
use std::hash::{Hash, Hasher};
use crate::pipeline::VkPipelineLayout;
use std::ffi::c_void;
use crate::VkAdapterExtensionSupport;
use smallvec::SmallVec;

// TODO: clean up descriptor management
// TODO: determine descriptor and set counts

// TODO: this shit is really slow. rewrite all of it.

bitflags! {
    pub struct DirtyDescriptorSets: u32 {
        const PER_DRAW = 0b0001;
        const PER_MATERIAL = 0b0010;
        const PER_FRAME = 0b0100;
        const RARELY = 0b1000;
    }
}

impl From<BindingFrequency> for DirtyDescriptorSets {
  fn from(value: BindingFrequency) -> Self {
    match value {
      BindingFrequency::PerDraw => DirtyDescriptorSets::PER_DRAW,
      BindingFrequency::PerMaterial => DirtyDescriptorSets::PER_MATERIAL,
      BindingFrequency::PerFrame => DirtyDescriptorSets::PER_FRAME,
      BindingFrequency::Rarely => DirtyDescriptorSets::RARELY
    }
  }
}

#[derive(Clone, Eq, PartialEq, Hash)]
pub(crate) struct VkDescriptorSetBindingInfo {
  pub(crate) shader_stage: vk::ShaderStageFlags,
  pub(crate) index: u32,
  pub(crate) descriptor_type: vk::DescriptorType,
  pub(crate) writable: bool
}

pub(crate) struct VkDescriptorSetLayout {
  pub device: Arc<RawVkDevice>,
  layout: vk::DescriptorSetLayout,
  binding_infos: [Option<VkDescriptorSetBindingInfo>; 16],
  template: Option<vk::DescriptorUpdateTemplate>
}

impl VkDescriptorSetLayout {
  pub fn new(bindings: &[VkDescriptorSetBindingInfo], device: &Arc<RawVkDevice>) -> Self {
    let mut vk_bindings: Vec<vk::DescriptorSetLayoutBinding> = Vec::new();
    let mut vk_template_entries: Vec<vk::DescriptorUpdateTemplateEntry> = Vec::new();
    let mut binding_infos: [Option<VkDescriptorSetBindingInfo>; 16] = Default::default();

    for binding in bindings.iter() {
      binding_infos[binding.index as usize] = Some(binding.clone());

      vk_bindings.push(vk::DescriptorSetLayoutBinding {
        binding: binding.index,
        descriptor_count: 1,
        descriptor_type: binding.descriptor_type,
        stage_flags: binding.shader_stage,
        p_immutable_samplers: std::ptr::null()
      });

      vk_template_entries.push(vk::DescriptorUpdateTemplateEntry {
        dst_binding: binding.index,
        dst_array_element: 0,
        descriptor_count: 1,
        descriptor_type: binding.descriptor_type,
        offset: binding.index as usize * std::mem::size_of::<VkDescriptorEntry>(),
        stride: std::mem::size_of::<VkDescriptorEntry>()
      });
    }

    let info = vk::DescriptorSetLayoutCreateInfo {
      p_bindings: vk_bindings.as_ptr(),
      binding_count: vk_bindings.len() as u32,
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
    let template = if !vk_template_entries.is_empty() &&
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

  pub(crate) fn get_handle(&self) -> &vk::DescriptorSetLayout {
    &self.layout
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
      descriptor_count: 16384
    }, vk::DescriptorPoolSize {
      ty: vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
      descriptor_count: 4096
    }, vk::DescriptorPoolSize {
      ty: vk::DescriptorType::UNIFORM_BUFFER,
      descriptor_count: 4096
    }, vk::DescriptorPoolSize {
      ty: vk::DescriptorType::STORAGE_BUFFER,
      descriptor_count: 4096
    }, vk::DescriptorPoolSize {
      ty: vk::DescriptorType::STORAGE_BUFFER_DYNAMIC,
      descriptor_count: 512
    }];
    let info = vk::DescriptorPoolCreateInfo {
      max_sets: 4096,
      p_pool_sizes: pool_sizes.as_ptr(),
      pool_size_count: pool_sizes.len() as u32,
      flags: if is_transient { vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET } else { vk::DescriptorPoolCreateFlags::empty() },
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
  fn get_handle(&self) -> MutexGuard<vk::DescriptorPool> {
    self.descriptor_pool.lock().unwrap()
  }

  fn reset(&self) {
    if !self.is_transient {
      return;
    }
    let guard = self.get_handle();
    unsafe {
      self.device.reset_descriptor_pool(*guard, vk::DescriptorPoolResetFlags::empty()).unwrap();
    }
  }

  pub fn new_set(self: &Arc<Self>, layout: &Arc<VkDescriptorSetLayout>, dynamic_buffer_offsets: bool, bindings: &[VkBoundResource; 16]) -> VkDescriptorSet {
    VkDescriptorSet::new(self, &self.device, layout, self.is_transient, dynamic_buffer_offsets, bindings)
  }
}

impl Drop for VkDescriptorPool {
  fn drop(&mut self) {
    let pool = self.get_handle();
    unsafe {
      self.device.destroy_descriptor_pool(*pool, None);
    }
  }
}

#[repr(C)]
union VkDescriptorEntry {
  image: vk::DescriptorImageInfo,
  buffer: vk::DescriptorBufferInfo,
  buffer_view: vk::BufferView
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
  is_using_dynamic_buffer_offsets: bool,
  bindings: [VkBoundResource; 16],
  device: Arc<RawVkDevice>
}

impl VkDescriptorSet {
  fn new(pool: &Arc<VkDescriptorPool>, device: &Arc<RawVkDevice>, layout: &Arc<VkDescriptorSetLayout>, is_transient: bool, dynamic_buffer_offsets: bool, bindings: &[VkBoundResource; 16]) -> Self {
    let pool_guard = pool.get_handle();
    let set_create_info = vk::DescriptorSetAllocateInfo {
      descriptor_pool: *pool_guard,
      descriptor_set_count: 1,
      p_set_layouts: layout.get_handle() as *const vk::DescriptorSetLayout,
      ..Default::default()
    };
    let set = unsafe {
      device.allocate_descriptor_sets(&set_create_info)
    }.unwrap().pop().unwrap();

    match layout.template {
      None => {
        let mut writes: SmallVec<[vk::WriteDescriptorSet; 16]> = Default::default();
        let mut image_writes: SmallVec<[vk::DescriptorImageInfo; 16]> = Default::default();
        let mut buffer_writes: SmallVec<[vk::DescriptorBufferInfo; 16]> = Default::default();
        for (binding, resource) in bindings.iter().enumerate() {
          let binding_info = &layout.binding_infos[binding];
          if binding_info.is_none() {
            continue;
          }
          let binding_info = binding_info.as_ref().unwrap();

          let mut write = vk::WriteDescriptorSet {
            dst_set: set,
            dst_binding: binding as u32,
            dst_array_element: 0,
            descriptor_count: 1,
            ..Default::default()
          };

          match resource {
            VkBoundResource::StorageBuffer(buffer) => {
              let buffer_info = vk::DescriptorBufferInfo {
                buffer: *buffer.get_buffer().get_handle(),
                offset: if dynamic_buffer_offsets { 0 } else { buffer.get_offset_and_length().0 as vk::DeviceSize },
                range: buffer.get_offset_and_length().1 as vk::DeviceSize
              };
              buffer_writes.push(buffer_info);
              write.p_buffer_info = unsafe { buffer_writes.as_ptr().offset(buffer_writes.len() as isize - 1) };
              write.descriptor_type = if dynamic_buffer_offsets { vk::DescriptorType::STORAGE_BUFFER_DYNAMIC } else { vk::DescriptorType::STORAGE_BUFFER };
            },
            VkBoundResource::StorageTexture(texture) => {
              let texture_info = vk::DescriptorImageInfo {
                image_view: *texture.get_view_handle(),
                sampler: vk::Sampler::null(),
                image_layout: if !binding_info.writable { vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL } else { vk::ImageLayout::GENERAL }
              };
              image_writes.push(texture_info);
              write.p_image_info = unsafe { image_writes.as_ptr().offset(image_writes.len() as isize - 1) };
              write.descriptor_type = vk::DescriptorType::STORAGE_IMAGE;
            },
            VkBoundResource::UniformBuffer(buffer) => {
              let buffer_info = vk::DescriptorBufferInfo {
                buffer: *buffer.get_buffer().get_handle(),
                offset: if dynamic_buffer_offsets { 0 } else { buffer.get_offset_and_length().0 as vk::DeviceSize },
                range: buffer.get_offset_and_length().1 as vk::DeviceSize
              };
              buffer_writes.push(buffer_info);
              write.p_buffer_info = unsafe { buffer_writes.as_ptr().offset(buffer_writes.len() as isize - 1) };
              write.descriptor_type = if dynamic_buffer_offsets { vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC } else { vk::DescriptorType::UNIFORM_BUFFER };
            },
            VkBoundResource::SampledTexture(texture, sampler) => {
              let texture_info = vk::DescriptorImageInfo {
                image_view: *texture.get_view_handle(),
                sampler: *sampler.get_handle(),
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
              };
              image_writes.push(texture_info);
              write.p_image_info = unsafe { image_writes.as_ptr().offset(image_writes.len() as isize - 1) };
              write.descriptor_type = vk::DescriptorType::COMBINED_IMAGE_SAMPLER;
            },
            _ => unimplemented!()
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
          let binding_info = &layout.binding_infos[binding];
          if binding_info.is_none() {
            continue;
          }
          let mut entry = VkDescriptorEntry::default();
          match resource {
            VkBoundResource::StorageBuffer(buffer) => {
              entry.buffer = vk::DescriptorBufferInfo {
                buffer: *buffer.get_buffer().get_handle(),
                offset: if dynamic_buffer_offsets { 0 } else { buffer.get_offset_and_length().0 as vk::DeviceSize },
                range: buffer.get_offset_and_length().1 as vk::DeviceSize
              };
            },
            VkBoundResource::UniformBuffer(buffer) => {
              entry.buffer = vk::DescriptorBufferInfo {
                buffer: *buffer.get_buffer().get_handle(),
                offset: if dynamic_buffer_offsets { 0 } else { buffer.get_offset_and_length().0 as vk::DeviceSize },
                range: buffer.get_offset_and_length().1 as vk::DeviceSize
              };
            },
            VkBoundResource::SampledTexture(texture, sampler) => {
              entry.image = vk::DescriptorImageInfo {
                image_view: *texture.get_view_handle(),
                sampler: *sampler.get_handle(),
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
              };
            },
            VkBoundResource::StorageTexture(texture) => {
              entry.image = vk::DescriptorImageInfo {
                image_view: *texture.get_view_handle(),
                sampler: vk::Sampler::null(),
                image_layout: vk::ImageLayout::GENERAL
              };
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

    Self {
      descriptor_set: set,
      pool: pool.clone(),
      layout: layout.clone(),
      is_transient,
      is_using_dynamic_buffer_offsets: dynamic_buffer_offsets,
      bindings: bindings.clone(),
      device: device.clone(),
    }
  }

  #[inline]
  pub(crate) fn get_handle(&self) -> &vk::DescriptorSet {
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
      self.device.free_descriptor_sets(*self.pool.get_handle(), &[self.descriptor_set]).unwrap();
    }
  }
}

#[derive(Hash, Eq, PartialEq, Clone)]
pub(crate) enum VkBoundResource {
  None,
  UniformBuffer(Arc<VkBufferSlice>),
  StorageBuffer(Arc<VkBufferSlice>),
  StorageTexture(Arc<VkTextureView>),
  SampledTexture(Arc<VkTextureView>, Arc<VkSampler>)
}

impl Default for VkBoundResource {
  fn default() -> Self {
    VkBoundResource::None
  }
}

#[derive(Hash, Eq, PartialEq, Clone)]
pub(crate) enum VkBoundResourceRef<'a> {
  None,
  UniformBuffer(&'a Arc<VkBufferSlice>),
  StorageBuffer(&'a Arc<VkBufferSlice>),
  StorageTexture(&'a Arc<VkTextureView>),
  SampledTexture(&'a Arc<VkTextureView>, &'a Arc<VkSampler>)
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
  transient_pool: Arc<VkDescriptorPool>,
  permanent_pool: Arc<VkDescriptorPool>,
  device: Arc<RawVkDevice>,
  current_sets: [Option<VkDescriptorSet>; 4],
  dirty: DirtyDescriptorSets,
  bindings: [[VkBoundResource; 16]; 4],
  transient_cache: HashMap<Arc<VkDescriptorSetLayout>, Vec<VkDescriptorSetCacheEntry>>,
  permanent_cache: HashMap<Arc<VkDescriptorSetLayout>, Vec<VkDescriptorSetCacheEntry>>,
  last_cleanup_frame: u64
}

impl VkBindingManager {
  pub(crate) fn new(device: &Arc<RawVkDevice>) -> Self {
    let transient_pool = Arc::new(VkDescriptorPool::new(device, true));
    let permanent_pool = Arc::new(VkDescriptorPool::new(device, false));

    Self {
      transient_pool,
      permanent_pool,
      device: device.clone(),
      current_sets: Default::default(),
      dirty: DirtyDescriptorSets::empty(),
      bindings: Default::default(),
      transient_cache: HashMap::new(),
      permanent_cache: HashMap::new(),
      last_cleanup_frame: 0
    }
  }

  pub(crate) fn reset(&mut self) {
    self.dirty = DirtyDescriptorSets::empty();
    self.bindings = Default::default();
    self.transient_cache.clear();
    self.transient_pool.reset();
    self.permanent_pool.reset();
  }

  pub(crate) fn bind(&mut self, frequency: BindingFrequency, slot: u32, binding: VkBoundResourceRef) {
    let bindings_table = &mut self.bindings[frequency as usize];
    let existing_binding = &mut bindings_table[slot as usize];
    let identical = match (&existing_binding, &binding) {
        (VkBoundResource::None, VkBoundResourceRef::None) => true,
        (VkBoundResource::UniformBuffer(old), VkBoundResourceRef::UniformBuffer(new)) => old == *new,
        (VkBoundResource::StorageBuffer(old), VkBoundResourceRef::StorageBuffer(new)) => old == *new,
        (VkBoundResource::StorageTexture(old), VkBoundResourceRef::StorageTexture(new)) => old == *new,
        (VkBoundResource::SampledTexture(old_tex, old_sampler), VkBoundResourceRef::SampledTexture(new_tex, new_sampler)) => old_tex == *new_tex && old_sampler == *new_sampler,
        _ => false
    };

    if !identical {
      self.dirty.insert(DirtyDescriptorSets::from(frequency));
      *existing_binding = match binding {
        VkBoundResourceRef::None => VkBoundResource::None,
        VkBoundResourceRef::UniformBuffer(ubo) => VkBoundResource::UniformBuffer(ubo.clone()),
        VkBoundResourceRef::StorageBuffer(ssbo) => VkBoundResource::StorageBuffer(ssbo.clone()),
        VkBoundResourceRef::StorageTexture(storage_tex) => VkBoundResource::StorageTexture(storage_tex.clone()),
        VkBoundResourceRef::SampledTexture(tex, sampler) => VkBoundResource::SampledTexture(tex.clone(), sampler.clone()),
      };
    }
  }

  fn find_compatible_set(&mut self, frame: u64, layout: &Arc<VkDescriptorSetLayout>, frequency: BindingFrequency, use_permanent_cache: bool, use_dynamic_offsets: bool) -> Option<Arc<VkDescriptorSet>> {
    let cache = if use_permanent_cache { &mut self.permanent_cache } else { &mut self.transient_cache };
    let bindings = self.bindings.get(frequency as usize).unwrap();

    let mut entry_opt = cache
        .get_mut(layout)
        .and_then(|sets| {
          sets
              .iter_mut()
              .find(|entry|
                entry.set.is_using_dynamic_buffer_offsets == use_dynamic_offsets
                  && entry.set.bindings.iter().enumerate().all(|(index, binding)|
                      if !entry.set.is_using_dynamic_buffer_offsets {
                        binding == &bindings[index]
                      } else {
                        // https://github.com/rust-lang/rust/issues/53667
                        if let (VkBoundResource::UniformBuffer(entry_buffer), VkBoundResource::UniformBuffer(buffer)) = (binding, &bindings[index]) {
                          buffer.get_buffer() == entry_buffer.get_buffer()
                        } else if let (VkBoundResource::StorageBuffer(entry_buffer), VkBoundResource::StorageBuffer(buffer)) = (binding, &bindings[index]) {
                          buffer.get_buffer() == entry_buffer.get_buffer()
                        } else {
                          binding == &bindings[index]
                        }
                      }
                  )
              )
        });
    if let Some(entry) = &mut entry_opt {
      entry.last_used_frame = frame;
    }
    entry_opt.map(|entry| entry.set.clone())
  }

  fn finish_set(&mut self, frame: u64, pipeline_layout: &VkPipelineLayout, frequency: BindingFrequency) -> Option<VkDescriptorSetBinding> {
    let layout_option = pipeline_layout.get_descriptor_set_layout(frequency as u32);
    if !self.dirty.contains(DirtyDescriptorSets::from(frequency)) && layout_option.is_some() {
      return None;
    }

    let layout = layout_option.unwrap();
    let cached_set = self.find_compatible_set(frame, layout, frequency, frequency == BindingFrequency::Rarely, frequency == BindingFrequency::PerDraw);

    let bindings = self.bindings.get(frequency as usize).unwrap();
    let mut is_new = false;
    let set = cached_set.unwrap_or_else(|| {
      let pool = if frequency == BindingFrequency::Rarely { &self.permanent_pool } else { &self.transient_pool };
      let new_set = Arc::new(VkDescriptorSet::new(pool, &self.device, layout, frequency != BindingFrequency::Rarely, frequency == BindingFrequency::PerDraw, bindings));
      is_new = true;
      new_set
    });
    if is_new {
      let cache = if frequency == BindingFrequency::Rarely { &mut self.permanent_cache } else { &mut self.transient_cache };
      cache.entry(layout.clone()).or_default().push(VkDescriptorSetCacheEntry {
        set: set.clone(),
        last_used_frame: frame
      });
    }
    let mut set_binding = VkDescriptorSetBinding {
      set,
      dynamic_offsets: Default::default(),
      dynamic_offset_count: 0
    };
    if frequency == BindingFrequency::PerDraw {
      bindings.iter().enumerate().for_each(|(index, binding)| {
        if layout.binding_infos[index].is_some() {
          match binding {
            VkBoundResource::UniformBuffer(buffer) => {
              set_binding.dynamic_offsets[set_binding.dynamic_offset_count as usize] = buffer.get_offset() as u64;
              set_binding.dynamic_offset_count += 1;
            }
            VkBoundResource::StorageBuffer(buffer) => {
              set_binding.dynamic_offsets[set_binding.dynamic_offset_count as usize] = buffer.get_offset() as u64;
              set_binding.dynamic_offset_count += 1;
            },
            _ => {}
          }
        }
      })
    }
    Some(set_binding)
  }

  pub fn finish(&mut self, frame: u64, pipeline_layout: &VkPipelineLayout) -> [Option<VkDescriptorSetBinding>; 4] {
    if self.dirty.is_empty() {
      return Default::default();
    }
    self.clean(frame);

    let mut set_bindings: [Option<VkDescriptorSetBinding>; 4] = Default::default();
    set_bindings[BindingFrequency::PerDraw as usize] = self.finish_set(frame, pipeline_layout, BindingFrequency::PerDraw);
    set_bindings[BindingFrequency::PerFrame as usize] = self.finish_set(frame, pipeline_layout, BindingFrequency::PerFrame);
    set_bindings[BindingFrequency::PerMaterial as usize] = self.finish_set(frame, pipeline_layout, BindingFrequency::PerMaterial);
    set_bindings[BindingFrequency::Rarely as usize] = self.finish_set(frame, pipeline_layout, BindingFrequency::Rarely);

    self.dirty = DirtyDescriptorSets::empty();
    set_bindings
  }

  const FRAMES_BETWEEN_CLEANUP: u64 = 5;
  const MAX_FRAMES_SET_UNUSED: u64 = 5;
  fn clean(&mut self, frame: u64) {
    if frame - self.last_cleanup_frame <= Self::FRAMES_BETWEEN_CLEANUP {
      return;
    }

    for entries in &mut self.permanent_cache.values_mut() {
      entries.retain(|entry| {
        frame - entry.last_used_frame >= Self::MAX_FRAMES_SET_UNUSED
      });
    }
    self.last_cleanup_frame = frame;
  }
}
