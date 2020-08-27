use std::sync::{Arc, Mutex, MutexGuard};
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
use texture::VkTextureView;
use buffer::VkBufferSlice;
use std::hash::{Hash, Hasher};
use pipeline::VkPipelineLayout;
use spirv_cross::spirv::Decoration::Index;
use sourcerenderer_core::graphics::LogicOp::Set;

// TODO: clean up descriptor management

#[derive(Clone, Eq, PartialEq, Hash)]
pub(crate) enum VkDescriptorType {
  UniformBuffer,
  Sampler,
  Texture,
  CombinedTextureSampler
}

bitflags! {
    pub struct DirtyDescriptorSets: u32 {
        const PER_DRAW = 0b0001;
        const PER_MATERIAL = 0b0010;
        const PER_MODEL = 0b0100;
        const RARELY = 0b1000;
    }
}

impl From<BindingFrequency> for DirtyDescriptorSets {
  fn from(value: BindingFrequency) -> Self {
    match value {
      BindingFrequency::PerDraw => DirtyDescriptorSets::PER_DRAW,
      BindingFrequency::PerMaterial => DirtyDescriptorSets::PER_MATERIAL,
      BindingFrequency::PerModel => DirtyDescriptorSets::PER_MODEL,
      BindingFrequency::Rarely => DirtyDescriptorSets::RARELY
    }
  }
}

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
      descriptor_count: 64
    }, vk::DescriptorPoolSize {
      ty: vk::DescriptorType::UNIFORM_BUFFER,
      descriptor_count: 256
    }];
    let info = vk::DescriptorPoolCreateInfo {
      max_sets: 32,
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
      self.device.reset_descriptor_pool(*guard, vk::DescriptorPoolResetFlags::empty());
    }
  }

  pub fn new_set(self: &Arc<Self>, layout: &Arc<VkDescriptorSetLayout>, dynamic_buffer_offsets: bool, bindings: &[VkBoundResource; 8]) -> VkDescriptorSet {
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

pub(crate) struct VkDescriptorSet {
  descriptor_set: vk::DescriptorSet,
  pool: Arc<VkDescriptorPool>,
  layout: Arc<VkDescriptorSetLayout>,
  is_transient: bool,
  is_using_dynamic_buffer_offsets: bool,
  bindings: [VkBoundResource; 8],
  device: Arc<RawVkDevice>
}

impl VkDescriptorSet {
  fn new(pool: &Arc<VkDescriptorPool>, device: &Arc<RawVkDevice>, layout: &Arc<VkDescriptorSetLayout>, is_transient: bool, dynamic_buffer_offsets: bool, bindings: &[VkBoundResource; 8]) -> Self {
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

    for (binding, resource) in bindings.iter().enumerate() {
      let mut write = vk::WriteDescriptorSet {
        dst_set: set,
        dst_binding: binding as u32,
        dst_array_element: 0,
        descriptor_count: 1,
        ..Default::default()
      };

      match resource {
        VkBoundResource::Buffer(buffer) => {
          let buffer_info = vk::DescriptorBufferInfo {
            buffer: *buffer.get_buffer().get_handle(),
            offset: if dynamic_buffer_offsets { 0 } else { buffer.get_offset_and_length().0 as vk::DeviceSize },
            range:  buffer.get_offset_and_length().1 as vk::DeviceSize
          };
          write.p_buffer_info = &buffer_info;
          write.descriptor_type = if dynamic_buffer_offsets { vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC } else { vk::DescriptorType::UNIFORM_BUFFER };
          unsafe {
            device.update_descriptor_sets(&[write], &[]);
          }
        },
        VkBoundResource::Texture(texture) => {
          let texture_info = vk::DescriptorImageInfo {
            image_view: *texture.get_view_handle(),
            sampler: *texture.get_sampler_handle().unwrap(),
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
          };
          write.p_image_info = &texture_info;
          write.descriptor_type = vk::DescriptorType::COMBINED_IMAGE_SAMPLER;
          unsafe {
            device.update_descriptor_sets(&[write], &[]);
          }
        },
        _ => {}
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
      self.device.free_descriptor_sets(*self.pool.get_handle(), &[self.descriptor_set]);
    }
  }
}

#[derive(Hash, Eq, PartialEq, Clone)]
pub(crate) enum VkBoundResource {
  None,
  Buffer(Arc<VkBufferSlice>),
  Texture(Arc<VkTextureView>)
}

impl Default for VkBoundResource {
  fn default() -> Self {
    VkBoundResource::None
  }
}

pub(crate) struct VkDescriptorSetBinding {
  pub(crate) set: Arc<VkDescriptorSet>,
  pub(crate) dynamic_offset_count: u32,
  pub(crate) dynamic_offsets: [u64; 8]
}

pub(crate) struct VkBindingManager {
  transient_pool: Arc<VkDescriptorPool>,
  permanent_pool: Arc<VkDescriptorPool>,
  device: Arc<RawVkDevice>,
  current_sets: HashMap<BindingFrequency, VkDescriptorSet>,
  dirty: DirtyDescriptorSets,
  bindings: HashMap<BindingFrequency, [VkBoundResource; 8]>,
  transient_cache: HashMap<Arc<VkDescriptorSetLayout>, Vec<Arc<VkDescriptorSet>>>,
  permanent_cache: HashMap<Arc<VkDescriptorSetLayout>, Vec<Arc<VkDescriptorSet>>>
}

impl VkBindingManager {
  pub(crate) fn new(device: &Arc<RawVkDevice>) -> Self {
    let transient_pool = Arc::new(VkDescriptorPool::new(device, true));
    let permanent_pool = Arc::new(VkDescriptorPool::new(device, false));

    Self {
      transient_pool,
      permanent_pool,
      device: device.clone(),
      current_sets: HashMap::new(),
      dirty: DirtyDescriptorSets::empty(),
      bindings: HashMap::new(),
      transient_cache: HashMap::new(),
      permanent_cache: HashMap::new()
    }
  }

  pub(crate) fn reset(&mut self) {
    self.dirty = DirtyDescriptorSets::empty();
    self.bindings.clear();
    self.transient_cache.clear();
    self.transient_pool.reset();
    self.permanent_pool.reset();
  }

  pub(crate) fn bind(&mut self, frequency: BindingFrequency, slot: u32, binding: VkBoundResource) {
    let bindings_table = self.bindings.entry(frequency).or_insert_with(move || {
      Default::default()
    });
    let existing_binding = bindings_table.get(slot as usize);
    if existing_binding.is_some() && existing_binding.unwrap() != &binding {
      self.dirty.insert(DirtyDescriptorSets::from(frequency));
      bindings_table[slot as usize] = binding;
    }
  }

  fn find_compatible_set(&self, layout: &Arc<VkDescriptorSetLayout>, bindings: &[VkBoundResource; 8], use_permanent_cache: bool, use_dynamic_offsets: bool) -> Option<Arc<VkDescriptorSet>> {
    let cache = if use_permanent_cache { &self.permanent_cache } else { &self.transient_cache };

    cache
        .get(layout)
        .and_then(|sets| {
          sets
              .iter()
              .find(|set|
                  set.is_using_dynamic_buffer_offsets == use_dynamic_offsets
                  && set.bindings.iter().enumerate().all(|(index, binding)|
                      if !set.is_using_dynamic_buffer_offsets {
                        binding == &bindings[index]
                      } else {
                        if let (VkBoundResource::Buffer(entry_buffer), VkBoundResource::Buffer(buffer)) = (binding, &bindings[index]) {
                          // https://github.com/rust-lang/rust/issues/53667
                          buffer.get_buffer() == entry_buffer.get_buffer()
                        } else {
                          binding == &bindings[index]
                        }
                      }
                  )
              )
              .map(|set| set.clone())
        })
  }

  fn finish_set(&mut self, pipeline_layout: &VkPipelineLayout, frequency: BindingFrequency) -> Option<VkDescriptorSetBinding> {
    let layout_option = pipeline_layout.get_descriptor_set_layout(frequency as u32);
    let bindings_option = self.bindings.get(&frequency);
    if self.dirty.contains(DirtyDescriptorSets::from(frequency)) && layout_option.is_some() {
      let layout = layout_option.unwrap();
      let bindings = bindings_option.unwrap();
      let cached_set = self.find_compatible_set(layout, bindings, frequency == BindingFrequency::Rarely, frequency == BindingFrequency::PerDraw);

      let mut is_new = false;
      let set = cached_set.unwrap_or_else(|| {
        let pool = if frequency == BindingFrequency::Rarely { &self.permanent_pool } else { &self.transient_pool };
        let new_set = Arc::new(VkDescriptorSet::new(pool, &self.device, layout, frequency != BindingFrequency::Rarely, frequency == BindingFrequency::PerDraw, bindings));
        is_new = true;
        new_set
      });
      if is_new {
        let mut cache = if frequency == BindingFrequency::Rarely { &mut self.permanent_cache } else { &mut self.transient_cache };
        cache.entry(layout.clone()).or_default().push(set.clone());
      }
      let mut set_binding = VkDescriptorSetBinding {
        set: set.clone(),
        dynamic_offsets: Default::default(),
        dynamic_offset_count: 0
      };
      if frequency == BindingFrequency::PerDraw {
        bindings.iter().enumerate().for_each(|(index, binding)| {
          if let VkBoundResource::Buffer(buffer) = binding {
            if let VkBoundResource::Buffer(set_buffer) = &set.bindings[index] {
              set_binding.dynamic_offsets[set_binding.dynamic_offset_count as usize] = buffer.get_offset() as u64;
              set_binding.dynamic_offset_count += 1;
            } else {
              unreachable!("trying to use incompatible descriptor set with dynamic offsets");
            }
          }
        })
      }
      return Some(set_binding);
    }
    None
  }

  pub fn finish(&mut self, pipeline_layout: &VkPipelineLayout) -> [Option<VkDescriptorSetBinding>; 4] {
    let mut set_bindings: [Option<VkDescriptorSetBinding>; 4] = Default::default();

    set_bindings[BindingFrequency::PerDraw as usize] = self.finish_set(pipeline_layout, BindingFrequency::PerDraw);
    set_bindings[BindingFrequency::PerModel as usize] = self.finish_set(pipeline_layout, BindingFrequency::PerModel);
    set_bindings[BindingFrequency::PerMaterial as usize] = self.finish_set(pipeline_layout, BindingFrequency::PerMaterial);
    set_bindings[BindingFrequency::Rarely as usize] = self.finish_set(pipeline_layout, BindingFrequency::Rarely);
    set_bindings
  }
}
