use std::{cell::RefCell, collections::HashMap, hash::Hash, ops::Deref, sync::Arc};

use bitflags::bitflags;
use js_sys::{wasm_bindgen::JsValue, Array, Uint8Array};
use smallvec::SmallVec;
use sourcerenderer_core::{align_up_64, gpu};
use web_sys::{GpuBindGroup, GpuBindGroupDescriptor, GpuBindGroupEntry, GpuBindGroupLayout, GpuBindGroupLayoutDescriptor, GpuBindGroupLayoutEntry, GpuBuffer, GpuBufferBinding, GpuBufferBindingLayout, GpuBufferBindingType, GpuDevice, GpuPipelineLayout, GpuPipelineLayoutDescriptor, GpuSampler, GpuSamplerBindingLayout, GpuSamplerBindingType, GpuStorageTextureAccess, GpuStorageTextureBindingLayout, GpuTextureBindingLayout, GpuTextureSampleType, GpuTextureView, GpuBufferDescriptor};

use crate::{sampler::WebGPUSampler, texture::{format_to_webgpu, texture_dimension_to_webgpu_view, WebGPUTextureView}, WebGPULimits};

pub(crate) const WEBGPU_BIND_COUNT_PER_SET: u32 = gpu::PER_SET_BINDINGS * 2 + 2;
const DEFAULT_DESCRIPTOR_ARRAY_SIZE: usize = 4usize;
const DEFAULT_PER_SET_PREALLOCATED_SIZE: usize = 8usize;


bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct DirtyBindGroups: u32 {
        const VERY_FREQUENT = 0b0001;
        const FREQUENT = 0b0010;
        const FRAME = 0b0100;
    }
}

impl From<gpu::BindingFrequency> for DirtyBindGroups {
    fn from(value: gpu::BindingFrequency) -> Self {
        match value {
            gpu::BindingFrequency::VeryFrequent => DirtyBindGroups::VERY_FREQUENT,
            gpu::BindingFrequency::Frequent => DirtyBindGroups::FREQUENT,
            gpu::BindingFrequency::Frame => DirtyBindGroups::FRAME,
        }
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub(crate) struct WebGPUBindGroupEntryInfo {
    pub(crate) name: String,
    pub(crate) shader_stage: u32,
    pub(crate) index: u32,
    pub(crate) writable: bool,
    pub(crate) resource_type: gpu::ResourceType,
    pub(crate) has_dynamic_offset: bool,
    pub(crate) sampling_type: gpu::SamplingType,
    pub(crate) texture_dimension: gpu::TextureDimension,
    pub(crate) is_multisampled: bool,
    pub(crate) storage_format: gpu::Format,
    pub(crate) struct_size: u32
}

pub struct WebGPUBindGroupLayout {
    bind_group_layout: GpuBindGroupLayout,
    binding_infos: SmallVec<[Option<WebGPUBindGroupEntryInfo>; DEFAULT_PER_SET_PREALLOCATED_SIZE]>,
    is_empty: bool,
    max_used_binding: u32
}

fn sampling_type_to_webgpu(sampling_type: gpu::SamplingType) -> GpuTextureSampleType {
    match sampling_type {
        gpu::SamplingType::Float => GpuTextureSampleType::Float,
        gpu::SamplingType::Depth => GpuTextureSampleType::Depth,
        gpu::SamplingType::SInt => GpuTextureSampleType::Sint,
        gpu::SamplingType::UInt => GpuTextureSampleType::Uint,
    }
}


impl WebGPUBindGroupLayout {
    pub fn new(
        bindings: &[WebGPUBindGroupEntryInfo],
        device: &GpuDevice,
    ) -> Result<Self, ()> {
        let mut binding_infos: SmallVec<[Option<WebGPUBindGroupEntryInfo>; DEFAULT_PER_SET_PREALLOCATED_SIZE]> = SmallVec::new();
        binding_infos.resize(WEBGPU_BIND_COUNT_PER_SET as usize, None);

        let entries = Array::new();

        let mut max_used_binding = 0u32;

        for binding in bindings {
            let entry = GpuBindGroupLayoutEntry::new(binding.index, binding.shader_stage);
            match binding.resource_type {
                gpu::ResourceType::UniformBuffer => {
                    let buffer_binding = GpuBufferBindingLayout::new();
                    buffer_binding.set_type(GpuBufferBindingType::Uniform);
                    buffer_binding.set_has_dynamic_offset(binding.has_dynamic_offset);
                    buffer_binding.set_min_binding_size(binding.struct_size as f64);
                    entry.set_buffer(&buffer_binding);
                },
                gpu::ResourceType::StorageBuffer => {
                    let buffer_binding = GpuBufferBindingLayout::new();
                    buffer_binding.set_type(if binding.writable {
                        GpuBufferBindingType::Storage
                    } else {
                        GpuBufferBindingType::ReadOnlyStorage
                    });
                    buffer_binding.set_has_dynamic_offset(binding.has_dynamic_offset);
                    buffer_binding.set_min_binding_size(binding.struct_size as f64);
                    entry.set_buffer(&buffer_binding);
                }
                gpu::ResourceType::StorageTexture => {
                    let texture_binding = GpuStorageTextureBindingLayout::new(format_to_webgpu(binding.storage_format));
                    texture_binding.set_access(if binding.writable { GpuStorageTextureAccess::ReadWrite } else { GpuStorageTextureAccess::ReadOnly });
                    texture_binding.set_view_dimension(texture_dimension_to_webgpu_view(binding.texture_dimension));
                    entry.set_storage_texture(&texture_binding);
                },
                gpu::ResourceType::SampledTexture => {
                    let texture_binding = GpuTextureBindingLayout::new();
                    texture_binding.set_multisampled(binding.is_multisampled);
                    texture_binding.set_sample_type(sampling_type_to_webgpu(binding.sampling_type));
                    texture_binding.set_view_dimension(texture_dimension_to_webgpu_view(binding.texture_dimension));
                    entry.set_texture(&texture_binding);
                },
                gpu::ResourceType::CombinedTextureSampler => unreachable!(),
                gpu::ResourceType::Sampler => {
                    let sampler = GpuSamplerBindingLayout::new();
                    sampler.set_type(GpuSamplerBindingType::Filtering);
                    entry.set_sampler(&sampler);
                },
                _ => panic!("Unsupported resource type")
            }
            entries.push(&entry);

            if binding_infos.len() <= binding.index as usize {
                binding_infos.resize((binding.index + 1) as usize, None);
            }
            binding_infos[binding.index as usize] = Some(binding.clone());
            max_used_binding = max_used_binding.max(binding.index);
        }
        let descriptor = GpuBindGroupLayoutDescriptor::new(&entries);
        let bind_group_layout = device.create_bind_group_layout(&descriptor)
            .map_err(|e| log::error!("CreateBindGroup failed: {:?}", e))?;
        Ok(Self {
            bind_group_layout,
            binding_infos,
            is_empty: bindings.is_empty(),
            max_used_binding
        })
    }

    pub(crate) fn handle(&self) -> &GpuBindGroupLayout {
        &self.bind_group_layout
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.is_empty
    }

    pub(crate) fn max_used_binding(&self) -> u32 {
        self.max_used_binding
    }

    pub(crate) fn binding(&self, slot: u32) -> Option<&WebGPUBindGroupEntryInfo> {
        if slot >= self.binding_infos.len() as u32 {
            None
        } else {
            self.binding_infos[slot as usize].as_ref()
        }
    }

    pub(crate) fn is_dynamic_binding(&self, binding_index: u32) -> bool {
        if binding_index >= self.binding_infos.len() as u32 {
            false
        } else if let Some(binding_info) = self.binding_infos[binding_index as usize].as_ref() {
            binding_info.has_dynamic_offset
        } else {
            false
        }
    }
}

impl Hash for WebGPUBindGroupLayout {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let val: usize = unsafe { std::mem::transmute(self.bind_group_layout.as_ref() as *const GpuBindGroupLayout) };
        val.hash(state);
    }
}

impl PartialEq for WebGPUBindGroupLayout {
    fn eq(&self, other: &Self) -> bool {
        self.bind_group_layout == other.bind_group_layout
    }
}

impl Eq for WebGPUBindGroupLayout {}

pub struct WebGPUPipelineLayout {
    layout: GpuPipelineLayout,
    bind_group_layouts: [Option<Arc<WebGPUBindGroupLayout>>; gpu::NON_BINDLESS_SET_COUNT as usize]
}

impl WebGPUPipelineLayout {
    pub(crate) fn new(device: &GpuDevice, bind_group_layouts: &[Option<Arc<WebGPUBindGroupLayout>>]) -> Self {
        let mut owned_bind_group_layouts: [Option<Arc<WebGPUBindGroupLayout>>; gpu::NON_BINDLESS_SET_COUNT as usize] = Default::default();
        let bind_group_layouts_js: Array = Array::new();
        for (index, bind_group_layout_opt) in bind_group_layouts.iter().enumerate() {
            if let Some(bind_group_layout) = bind_group_layout_opt {
                bind_group_layouts_js.push(bind_group_layout.handle());
                owned_bind_group_layouts[index] = Some(bind_group_layout.clone());
            } else {
                bind_group_layouts_js.push(&JsValue::null());
            }
        }
        let descriptor = GpuPipelineLayoutDescriptor::new(&bind_group_layouts_js);
        let handle = device.create_pipeline_layout(&descriptor);
        Self {
            layout: handle,
            bind_group_layouts: owned_bind_group_layouts
        }
    }

    pub(crate) fn handle(&self) -> &GpuPipelineLayout {
        &self.layout
    }

    pub(crate) fn bind_group_layout(&self, index: u32) -> Option<&Arc<WebGPUBindGroupLayout>> {
        self.bind_group_layouts[index as usize].as_ref()
    }
}

pub struct WebGPUBindGroup {
    bind_group: GpuBindGroup,
    layout: Arc<WebGPUBindGroupLayout>,
    is_transient: bool,
    bindings: SmallVec<[WebGPUBoundResource; DEFAULT_PER_SET_PREALLOCATED_SIZE]>,
}

impl WebGPUBindGroup {
    fn new<'a, T>(
        device: &GpuDevice,
        layout: &Arc<WebGPUBindGroupLayout>,
        is_transient: bool,
        bindings: &'a [T],
    ) -> Result<Self, ()>
    where
        WebGPUBoundResource: From<&'a T>,
    {
        let entries = Array::new();
        let mut stored_bindings = SmallVec::<[WebGPUBoundResource; DEFAULT_PER_SET_PREALLOCATED_SIZE]>::new();

        for (index, binding_ref) in bindings.iter().enumerate() {
            let binding: WebGPUBoundResource = binding_ref.into();
            let resource_js_value: JsValue;
            match &binding {
                WebGPUBoundResource::None => continue,
                WebGPUBoundResource::SampledTexture(texture) => {
                    resource_js_value = JsValue::from(&*texture as &GpuTextureView);
                }
                WebGPUBoundResource::Sampler(sampler) => {
                    resource_js_value = JsValue::from(&*sampler as &GpuSampler);
                }
                WebGPUBoundResource::UniformBuffer(binding_info) => {
                    let buffer_info = GpuBufferBinding::new(&binding_info.buffer);
                    buffer_info.set_size(binding_info.length as f64);
                    if !layout.is_dynamic_binding(index as u32) {
                        buffer_info.set_offset(binding_info.offset as f64);
                    }
                    resource_js_value = JsValue::from(&buffer_info);
                },
                WebGPUBoundResource::StorageBuffer(binding_info) => {
                    let buffer_info = GpuBufferBinding::new(&binding_info.buffer);
                    buffer_info.set_size(binding_info.length as f64);
                    if !layout.is_dynamic_binding(index as u32) {
                        buffer_info.set_offset(binding_info.offset as f64);
                    }
                    resource_js_value = JsValue::from(&buffer_info);
                },
                WebGPUBoundResource::StorageTexture(texture) => {
                    resource_js_value = JsValue::from(&*texture as &GpuTextureView);
                },
                WebGPUBoundResource::UniformBufferArray(_buffers) => panic!("Descriptor arrays are not supported on WebGPU"),
                WebGPUBoundResource::StorageBufferArray(_buffers) => panic!("Descriptor arrays are not supported on WebGPU"),
                WebGPUBoundResource::StorageTextureArray(_textures) => panic!("Descriptor arrays are not supported on WebGPU"),
                WebGPUBoundResource::SampledTextureArray(_textures) => panic!("Descriptor arrays are not supported on WebGPU"),
            }
            let entry = GpuBindGroupEntry::new(index as u32, &resource_js_value);
            entries.push(&entry);
            stored_bindings.push(binding);
        }

        let descriptor = GpuBindGroupDescriptor::new(&entries, layout.handle());
        let bind_group = device.create_bind_group(&descriptor);
        Ok(Self {
            bind_group,
            layout: layout.clone(),
            is_transient,
            bindings: stored_bindings
        })
    }

    #[inline(always)]
    pub(crate) fn handle(&self) -> &GpuBindGroup {
        &self.bind_group
    }

    #[allow(unused)]
    #[inline(always)]
    pub(crate) fn is_transient(&self) -> bool {
        self.is_transient
    }

    pub(crate) fn is_compatible<'a, T>(
        &self,
        layout: &'a Arc<WebGPUBindGroupLayout>,
        bindings: &'a [T],
    ) -> bool
    where
        WebGPUBoundResource: BindingCompare<Option<&'a T>>,
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


#[derive(Eq, PartialEq, Clone)]
pub(crate) struct WebGPUBufferBindingInfo {
    pub(crate) buffer: GpuBuffer,
    pub(crate) offset: u64,
    pub(crate) length: u64,
}

impl Hash for WebGPUBufferBindingInfo {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let val: usize = unsafe { std::mem::transmute(self.buffer.as_ref() as *const GpuBuffer) };
        val.hash(state);
        self.offset.hash(state);
        self.length.hash(state);
    }
}

#[derive(Eq, PartialEq, Clone)]
pub(crate) struct WebGPUHashableTextureView(GpuTextureView);

impl Hash for WebGPUHashableTextureView {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let val: usize = unsafe { std::mem::transmute(self.0.as_ref() as *const GpuTextureView) };
        val.hash(state);
    }
}

impl From<GpuTextureView> for WebGPUHashableTextureView {
    fn from(value: GpuTextureView) -> Self {
        Self(value)
    }
}

impl From<&GpuTextureView> for WebGPUHashableTextureView {
    fn from(value: &GpuTextureView) -> Self {
        Self(value.clone())
    }
}

impl From<&WebGPUTextureView> for WebGPUHashableTextureView {
    fn from(value: &WebGPUTextureView) -> Self {
        Self(value.handle().clone())
    }
}

impl Deref for WebGPUHashableTextureView {
    type Target = GpuTextureView;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Eq, PartialEq, Clone)]
pub(crate) struct WebGPUHashableSampler(GpuSampler);

impl Hash for WebGPUHashableSampler {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let val: usize = unsafe { std::mem::transmute(self.0.as_ref() as *const GpuSampler) };
        val.hash(state);
    }
}

impl From<GpuSampler> for WebGPUHashableSampler {
    fn from(value: GpuSampler) -> Self {
        Self(value)
    }
}

impl From<&GpuSampler> for WebGPUHashableSampler {
    fn from(value: &GpuSampler) -> Self {
        Self(value.clone())
    }
}

impl From<&WebGPUSampler> for WebGPUHashableSampler {
    fn from(value: &WebGPUSampler) -> Self {
        Self(value.handle().clone())
    }
}

impl Deref for WebGPUHashableSampler {
    type Target = GpuSampler;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Hash, Eq, PartialEq, Clone)]
pub(crate) enum WebGPUBoundResource {
    None,
    UniformBuffer(WebGPUBufferBindingInfo),
    UniformBufferArray(SmallVec<[WebGPUBufferBindingInfo; DEFAULT_DESCRIPTOR_ARRAY_SIZE]>),
    StorageBuffer(WebGPUBufferBindingInfo),
    StorageBufferArray(SmallVec<[WebGPUBufferBindingInfo; DEFAULT_DESCRIPTOR_ARRAY_SIZE]>),
    StorageTexture(WebGPUHashableTextureView),
    StorageTextureArray(SmallVec<[WebGPUHashableTextureView; DEFAULT_DESCRIPTOR_ARRAY_SIZE]>),
    SampledTexture(WebGPUHashableTextureView),
    SampledTextureArray(SmallVec<[WebGPUHashableTextureView; DEFAULT_DESCRIPTOR_ARRAY_SIZE]>),
    Sampler(WebGPUHashableSampler),
}

impl Default for WebGPUBoundResource {
    fn default() -> Self {
        Self::None
    }
}


#[derive(Hash, Eq, PartialEq, Clone)]
#[allow(unused)]
pub(crate) enum WebGPUBoundResourceRef<'a> {
    None,
    UniformBuffer(WebGPUBufferBindingInfo),
    UniformBufferArray(&'a [WebGPUBufferBindingInfo]),
    StorageBuffer(WebGPUBufferBindingInfo),
    StorageBufferArray(&'a [WebGPUBufferBindingInfo]),
    StorageTexture(WebGPUHashableTextureView),
    StorageTextureArray(&'a [WebGPUHashableTextureView]),
    SampledTexture(WebGPUHashableTextureView),
    SampledTextureArray(&'a [WebGPUHashableTextureView]),
    SampledTextureAndSampler(WebGPUHashableTextureView, WebGPUHashableSampler),
    SampledTextureAndSamplerArray(&'a [(WebGPUHashableTextureView, WebGPUHashableSampler)]),
    Sampler(WebGPUHashableSampler),
}

#[derive(Hash, Eq, PartialEq, Clone)]
#[allow(unused)]
enum WebGPUBoundResourceRefInternal<'a> {
    None,
    UniformBuffer(WebGPUBufferBindingInfo),
    UniformBufferArray(&'a [WebGPUBufferBindingInfo]),
    StorageBuffer(WebGPUBufferBindingInfo),
    StorageBufferArray(&'a [WebGPUBufferBindingInfo]),
    StorageTexture(WebGPUHashableTextureView),
    StorageTextureArray(&'a [WebGPUHashableTextureView]),
    SampledTexture(WebGPUHashableTextureView),
    SampledTextureArray(&'a [WebGPUHashableTextureView]),
    Sampler(WebGPUHashableSampler),
}

impl Default for WebGPUBoundResourceRefInternal<'_> {
    fn default() -> Self {
        Self::None
    }
}

impl From<&WebGPUBoundResourceRefInternal<'_>> for WebGPUBoundResource {
    fn from(binding: &WebGPUBoundResourceRefInternal<'_>) -> Self {
        match binding {
            WebGPUBoundResourceRefInternal::None => WebGPUBoundResource::None,
            WebGPUBoundResourceRefInternal::UniformBuffer(info) => WebGPUBoundResource::UniformBuffer(info.clone()),
            WebGPUBoundResourceRefInternal::StorageBuffer(info) => WebGPUBoundResource::StorageBuffer(info.clone()),
            WebGPUBoundResourceRefInternal::StorageTexture(view) => WebGPUBoundResource::StorageTexture(view.clone()),
            WebGPUBoundResourceRefInternal::SampledTexture(view) => WebGPUBoundResource::SampledTexture(view.clone()),
            WebGPUBoundResourceRefInternal::Sampler(sampler) => WebGPUBoundResource::Sampler(sampler.clone()),
            WebGPUBoundResourceRefInternal::UniformBufferArray(arr) => WebGPUBoundResource::UniformBufferArray(
                arr.iter()
                    .map(|a| {
                        let info: WebGPUBufferBindingInfo = a.clone();
                        info
                    })
                    .collect(),
            ),
            WebGPUBoundResourceRefInternal::StorageBufferArray(arr) => WebGPUBoundResource::StorageBufferArray(
                arr.iter()
                    .map(|a| {
                        let info: WebGPUBufferBindingInfo = a.clone();
                        info
                    })
                    .collect(),
            ),
            WebGPUBoundResourceRefInternal::StorageTextureArray(arr) => {
                WebGPUBoundResource::StorageTextureArray(arr.iter().map(|a| a.clone()).collect())
            }
            WebGPUBoundResourceRefInternal::SampledTextureArray(arr) => {
                WebGPUBoundResource::SampledTextureArray(arr.iter().map(|a| a.clone()).collect())
            }
        }
    }
}

impl From<&Self> for WebGPUBoundResource {
    fn from(other: &Self) -> Self {
        other.clone()
    }
}

impl PartialEq<WebGPUBoundResourceRefInternal<'_>> for WebGPUBoundResource {
    fn eq(&self, other: &WebGPUBoundResourceRefInternal) -> bool {
        match (self, other) {
            (WebGPUBoundResource::None, WebGPUBoundResourceRefInternal::None) => true,
            (
                WebGPUBoundResource::UniformBuffer(WebGPUBufferBindingInfo {
                    buffer: old,
                    offset: old_offset,
                    length: old_length,
                }),
                WebGPUBoundResourceRefInternal::UniformBuffer(WebGPUBufferBindingInfo {
                    buffer: new,
                    offset: new_offset,
                    length: new_length,
                }),
            ) => old == new && old_offset == new_offset && old_length == new_length,
            (
                WebGPUBoundResource::StorageBuffer(WebGPUBufferBindingInfo {
                    buffer: old,
                    offset: old_offset,
                    length: old_length,
                }),
                WebGPUBoundResourceRefInternal::StorageBuffer(WebGPUBufferBindingInfo {
                    buffer: new,
                    offset: new_offset,
                    length: new_length,
                }),
            ) => old == new && old_offset == new_offset && old_length == new_length,
            (WebGPUBoundResource::StorageTexture(old), WebGPUBoundResourceRefInternal::StorageTexture(new)) => {
                old == new
            }
            (WebGPUBoundResource::SampledTexture(old), WebGPUBoundResourceRefInternal::SampledTexture(new)) => {
                old == new
            }
            (WebGPUBoundResource::Sampler(old_sampler), WebGPUBoundResourceRefInternal::Sampler(new_sampler)) => {
                old_sampler == new_sampler
            }
            (
                WebGPUBoundResource::StorageBufferArray(old),
                WebGPUBoundResourceRefInternal::StorageBufferArray(new),
            ) => &old[..] == &new[..],
            (
                WebGPUBoundResource::UniformBufferArray(old),
                WebGPUBoundResourceRefInternal::UniformBufferArray(new),
            ) => &old[..] == &new[..],
            (
                WebGPUBoundResource::SampledTextureArray(old),
                WebGPUBoundResourceRefInternal::SampledTextureArray(new),
            ) => old.iter().zip(new.iter()).all(|(old, new)| old == new),
            (
                WebGPUBoundResource::StorageTextureArray(old),
                WebGPUBoundResourceRefInternal::StorageTextureArray(new),
            ) => old.iter().zip(new.iter()).all(|(old, new)| old == new),
            _ => false,
        }
    }
}

impl PartialEq<WebGPUBoundResource> for WebGPUBoundResourceRefInternal<'_> {
    fn eq(&self, other: &WebGPUBoundResource) -> bool {
        other == self
    }
}

pub(crate) trait BindingCompare<T> {
    fn binding_eq(&self, other: &T, binding_info: Option<&WebGPUBindGroupEntryInfo>) -> bool;
}

impl BindingCompare<Self> for WebGPUBoundResource {
    fn binding_eq(&self, other: &Self, binding_info: Option<&WebGPUBindGroupEntryInfo>) -> bool {
        if self == &WebGPUBoundResource::None && binding_info.is_none() {
            true
        } else if binding_info.is_none() {
            false
        } else if !binding_info.unwrap().has_dynamic_offset {
            self == other
        } else {
            // https://github.com/rust-lang/rust/issues/53667
            if let (
                WebGPUBoundResource::UniformBuffer(WebGPUBufferBindingInfo {
                    buffer: entry_buffer,
                    offset: _,
                    length: entry_length,
                }),
                WebGPUBoundResource::UniformBuffer(WebGPUBufferBindingInfo {
                    buffer,
                    offset: _,
                    length,
                }),
            ) = (self, other)
            {
                buffer == entry_buffer && *length == *entry_length
            } else if let (
                WebGPUBoundResource::StorageBuffer(WebGPUBufferBindingInfo {
                    buffer: entry_buffer,
                    offset: _,
                    length: entry_length,
                }),
                WebGPUBoundResource::StorageBuffer(WebGPUBufferBindingInfo {
                    buffer,
                    offset: _,
                    length,
                }),
            ) = (self, other)
            {
                buffer == entry_buffer && *length == *entry_length
            } else if let (
                WebGPUBoundResource::StorageBufferArray(arr),
                WebGPUBoundResource::StorageBufferArray(arr1),
            ) = (self, other)
            {
                arr.iter()
                    .zip(arr1)
                    .all(|(b, b1)| b.buffer == b1.buffer && b.length == b1.length)
            } else if let (
                WebGPUBoundResource::UniformBufferArray(arr),
                WebGPUBoundResource::UniformBufferArray(arr1),
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

impl BindingCompare<WebGPUBoundResourceRefInternal<'_>> for WebGPUBoundResource {
    fn binding_eq(
        &self,
        other: &WebGPUBoundResourceRefInternal<'_>,
        binding_info: Option<&WebGPUBindGroupEntryInfo>,
    ) -> bool {
        if self == &WebGPUBoundResource::None && binding_info.is_none() {
            true
        } else if binding_info.is_none() {
            false
        } else if !binding_info.unwrap().has_dynamic_offset {
            self == other
        } else {
            // https://github.com/rust-lang/rust/issues/53667
            if let (
                WebGPUBoundResource::UniformBuffer(WebGPUBufferBindingInfo {
                    buffer: entry_buffer,
                    offset: _,
                    length: entry_length,
                }),
                WebGPUBoundResourceRefInternal::UniformBuffer(WebGPUBufferBindingInfo {
                    buffer,
                    offset: _,
                    length,
                }),
            ) = (self, other)
            {
                buffer == entry_buffer && *length == *entry_length
            } else if let (
                WebGPUBoundResource::StorageBuffer(WebGPUBufferBindingInfo {
                    buffer: entry_buffer,
                    offset: _,
                    length: entry_length,
                }),
                WebGPUBoundResourceRefInternal::StorageBuffer(WebGPUBufferBindingInfo {
                    buffer,
                    offset: _,
                    length,
                }),
            ) = (self, other)
            {
                buffer == entry_buffer && *length == *entry_length
            } else if let (
                WebGPUBoundResource::StorageBufferArray(arr),
                WebGPUBoundResourceRefInternal::StorageBufferArray(arr1),
            ) = (self, other)
            {
                arr.iter()
                    .zip(*arr1)
                    .all(|(b, b1)| b.buffer == b1.buffer && b.length == b1.length)
            } else if let (
                WebGPUBoundResource::UniformBufferArray(arr),
                WebGPUBoundResourceRefInternal::UniformBufferArray(arr1),
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

impl BindingCompare<Option<&WebGPUBoundResource>> for WebGPUBoundResource {
    fn binding_eq(
        &self,
        other: &Option<&WebGPUBoundResource>,
        binding_info: Option<&WebGPUBindGroupEntryInfo>,
    ) -> bool {
        if let Some(other) = other {
            self.binding_eq(*other, binding_info)
        } else if self == &WebGPUBoundResource::None {
            true
        } else {
            false
        }
    }
}

impl BindingCompare<Option<&WebGPUBoundResourceRefInternal<'_>>> for WebGPUBoundResource {
    fn binding_eq(
        &self,
        other: &Option<&WebGPUBoundResourceRefInternal<'_>>,
        binding_info: Option<&WebGPUBindGroupEntryInfo>,
    ) -> bool {
        if let Some(other) = other {
            self.binding_eq(*other, binding_info)
        } else if self == &WebGPUBoundResource::None {
            true
        } else {
            false
        }
    }
}

pub(crate) struct WebGPUBindGroupBinding {
    pub(crate) set: Arc<WebGPUBindGroup>,
    pub(crate) dynamic_offsets: SmallVec<[u64; 4]>,
}

struct WebGPUBindGroupCacheEntry {
    set: Arc<WebGPUBindGroup>,
    last_used_frame: u64,
}

#[allow(unused)]
#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq)]
enum CacheMode {
    None,
    PerFrame,
    Everything,
}

const PUSH_CONST_BUMP_ALLOCATOR_BUFFER_SIZE: u64 = 4u64 << 20u64;

struct PushConstBumpAllocator {
    buffer: GpuBuffer,
    offset: u64
}

pub(crate) struct WebGPUBindingManager {
    cache_mode: CacheMode,
    device: GpuDevice,
    current_sets: [Option<Arc<WebGPUBindGroup>>; gpu::NON_BINDLESS_SET_COUNT as usize],
    dirty: DirtyBindGroups,
    bindings: [Vec<WebGPUBoundResource>; gpu::NON_BINDLESS_SET_COUNT as usize],
    transient_cache: RefCell<HashMap<Arc<WebGPUBindGroupLayout>, Vec<WebGPUBindGroupCacheEntry>>>,
    permanent_cache: RefCell<HashMap<Arc<WebGPUBindGroupLayout>, Vec<WebGPUBindGroupCacheEntry>>>,
    last_cleanup_frame: u64,
    bump_allocator: PushConstBumpAllocator,
    limits: WebGPULimits
}

impl WebGPUBindingManager {
    pub(crate) fn new(device: &GpuDevice, limits: &WebGPULimits) -> Self {
        let cache_mode = CacheMode::Everything;

        let mut bindings: [Vec<WebGPUBoundResource>; gpu::NON_BINDLESS_SET_COUNT as usize] = Default::default();
        for set in &mut bindings {
            set.resize(WEBGPU_BIND_COUNT_PER_SET as usize, WebGPUBoundResource::None);
        }

        let bump_alloc_buffer = Self::create_push_const_buffer(
            device,
            if crate::buffer::PREFER_DISCARD_OVER_QUEUE_WRITE { 8u64 } else { PUSH_CONST_BUMP_ALLOCATOR_BUFFER_SIZE },
            false,
            web_sys::gpu_buffer_usage::UNIFORM | web_sys::gpu_buffer_usage::COPY_DST
        );

        let mut result = Self {
            cache_mode,
            device: device.clone(),
            current_sets: Default::default(),
            dirty: DirtyBindGroups::all(),
            bindings,
            transient_cache: RefCell::new(HashMap::new()),
            permanent_cache: RefCell::new(HashMap::new()),
            last_cleanup_frame: 0,
            bump_allocator: PushConstBumpAllocator {
                buffer: bump_alloc_buffer,
                offset: 0
            },
            limits: limits.clone()
        };

        result.set_push_constant_data(&[0u64], gpu::ShaderType::VertexShader);
        result.set_push_constant_data(&[0u64], gpu::ShaderType::FragmentShader);

        result
    }

    pub(crate) fn reset(&mut self, frame: u64) {
        self.dirty = DirtyBindGroups::all();
        for set in &mut self.bindings {
            set.clear();
            set.resize(WEBGPU_BIND_COUNT_PER_SET as usize, WebGPUBoundResource::None);
        }

        self.current_sets = Default::default();
        self.clean_permanent_cache(frame);
        if self.cache_mode != CacheMode::None {
            let mut transient_cache_mut = self.transient_cache.borrow_mut();
            transient_cache_mut.clear();
        }
        self.bump_allocator.offset = 0;
        self.set_push_constant_data(&[0u64], gpu::ShaderType::VertexShader);
        self.set_push_constant_data(&[0u64], gpu::ShaderType::FragmentShader);
    }

    pub(crate) fn bind(
        &mut self,
        frequency: gpu::BindingFrequency,
        slot: u32,
        binding: WebGPUBoundResourceRef,
    ) -> bool {
        let adjusted_slot = slot * 2 + if frequency == gpu::BindingFrequency::VeryFrequent { 2 } else { 0 };

        let mut internal_binding_2 = WebGPUBoundResourceRefInternal::None;
        let internal_binding = match binding {
            WebGPUBoundResourceRef::None => WebGPUBoundResourceRefInternal::None,
            WebGPUBoundResourceRef::UniformBuffer(buffer) => WebGPUBoundResourceRefInternal::UniformBuffer(buffer),
            WebGPUBoundResourceRef::UniformBufferArray(buffer_arr) => WebGPUBoundResourceRefInternal::UniformBufferArray(buffer_arr),
            WebGPUBoundResourceRef::StorageBuffer(buffer) => WebGPUBoundResourceRefInternal::StorageBuffer(buffer),
            WebGPUBoundResourceRef::StorageBufferArray(buffer_arr) => WebGPUBoundResourceRefInternal::StorageBufferArray(buffer_arr),
            WebGPUBoundResourceRef::StorageTexture(texture) => WebGPUBoundResourceRefInternal::StorageTexture(texture),
            WebGPUBoundResourceRef::StorageTextureArray(texture_arr) => WebGPUBoundResourceRefInternal::StorageTextureArray(texture_arr),
            WebGPUBoundResourceRef::SampledTexture(texture) => WebGPUBoundResourceRefInternal::SampledTexture(texture),
            WebGPUBoundResourceRef::SampledTextureArray(texture_arr) => WebGPUBoundResourceRefInternal::SampledTextureArray(texture_arr),
            WebGPUBoundResourceRef::SampledTextureAndSampler(texture, sampler) => {
                internal_binding_2 = WebGPUBoundResourceRefInternal::Sampler(sampler);
                WebGPUBoundResourceRefInternal::SampledTexture(texture)
            },
            WebGPUBoundResourceRef::SampledTextureAndSamplerArray(_texture_and_sampler_arr) => {
                unimplemented!()
            },
            WebGPUBoundResourceRef::Sampler(sampler) => WebGPUBoundResourceRefInternal::Sampler(sampler),
        };

        let bindings_table = &mut self.bindings[frequency as usize];
        let (existing_binding_slice, existing_binding_slice_2) = bindings_table.split_at_mut(adjusted_slot as usize + 1);
        let existing_binding = existing_binding_slice.last_mut().unwrap();
        let existing_binding_2 = existing_binding_slice_2.first_mut().unwrap();

        let identical = existing_binding == &internal_binding && existing_binding == &internal_binding_2;

        if !identical {
            self.dirty.insert(DirtyBindGroups::from(frequency));
            *existing_binding = (&internal_binding).into();
            *existing_binding_2 = (&internal_binding_2).into();
        }
        identical
    }

    fn create_push_const_buffer(device: &GpuDevice, size: u64, mapped: bool, usage: u32) -> GpuBuffer {
        let descriptor = GpuBufferDescriptor::new(size as f64, usage);
        descriptor.set_mapped_at_creation(mapped);
        device.create_buffer(&descriptor).unwrap()
    }

    pub(crate) fn set_push_constant_data<T>(&mut self, data: &[T], visible_for_shader_stage: gpu::ShaderType) {
        let data_as_bytes = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * std::mem::size_of::<T>()) };

        if crate::buffer::PREFER_DISCARD_OVER_QUEUE_WRITE {
            let aligned_len = align_up_64(data_as_bytes.len() as u64, 4) as usize;
            let buffer = Self::create_push_const_buffer(&self.device, aligned_len as u64, true, web_sys::gpu_buffer_usage::UNIFORM);
            let mapped_range = buffer.get_mapped_range().unwrap();
            let uint8_array = Uint8Array::new_with_byte_offset_and_length(&mapped_range, 0u32, aligned_len as u32);
            if aligned_len != data_as_bytes.len() {
                let mut padded_data = SmallVec::<[u8; 64]>::new();
                padded_data.copy_from_slice(data_as_bytes);
                padded_data.resize(aligned_len, 0u8);
                uint8_array.copy_from(&padded_data);
            } else {
                uint8_array.copy_from(&data_as_bytes);
            }
            buffer.unmap();

            let binding_index = if visible_for_shader_stage == gpu::ShaderType::FragmentShader { 1 } else { 0 };
            self.bindings[gpu::BindingFrequency::VeryFrequent as usize][binding_index] = WebGPUBoundResource::UniformBuffer(WebGPUBufferBindingInfo {
                buffer: buffer, offset: 0, length: aligned_len as u64
            });
        } else {
            let allocator = &mut self.bump_allocator;
            if allocator.offset + (data_as_bytes.len() as u64) > PUSH_CONST_BUMP_ALLOCATOR_BUFFER_SIZE {
                allocator.buffer = Self::create_push_const_buffer(&self.device, PUSH_CONST_BUMP_ALLOCATOR_BUFFER_SIZE, false, web_sys::gpu_buffer_usage::UNIFORM | web_sys::gpu_buffer_usage::COPY_DST);
                allocator.offset = 0;
            }

            assert_eq!(data_as_bytes.len() % 4, 0);
            self.device.queue().write_buffer_with_u32_and_u8_slice(
                &allocator.buffer,
                allocator.offset as u32,
                data_as_bytes
            ).unwrap();

            let binding_index = if visible_for_shader_stage == gpu::ShaderType::FragmentShader { 1 } else { 0 };
            self.bindings[gpu::BindingFrequency::VeryFrequent as usize][binding_index] = WebGPUBoundResource::UniformBuffer(WebGPUBufferBindingInfo {
                buffer: allocator.buffer.clone(), offset: allocator.offset, length: data_as_bytes.len() as u64
            });

            allocator.offset = align_up_64(allocator.offset + (data_as_bytes.len() as u64), self.limits.min_uniform_buffer_offset_alignment as u64);
        }

        self.dirty.insert(DirtyBindGroups::from(gpu::BindingFrequency::VeryFrequent));
    }

    fn find_compatible_set<'a, T>(
        &self,
        frame: u64,
        layout: &'a Arc<WebGPUBindGroupLayout>,
        bindings: &'a [T],
        use_permanent_cache: bool,
    ) -> Option<Arc<WebGPUBindGroup>>
    where
        WebGPUBoundResource: BindingCompare<Option<&'a T>>,
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
        pipeline_layout: &WebGPUPipelineLayout,
        frequency: gpu::BindingFrequency,
    ) -> Option<WebGPUBindGroupBinding> {
        let layout_option = pipeline_layout.bind_group_layout(frequency as u32);
        if !self.dirty.contains(DirtyBindGroups::from(frequency)) || layout_option.is_none() {
            return None;
        }
        let layout = layout_option.unwrap();

        let mut set: Option<Arc<WebGPUBindGroup>> = None;
        let bindings = &self.bindings[frequency as usize][..(layout.max_used_binding() + 1) as usize];
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
        set: Arc<WebGPUBindGroup>,
        bindings: &[WebGPUBoundResource],
    ) -> WebGPUBindGroupBinding {
        let mut set_binding = WebGPUBindGroupBinding {
            set: set.clone(),
            dynamic_offsets: Default::default(),
        };
        bindings.iter().enumerate().for_each(|(index, binding)| {
            if let Some(binding_info) = set.layout.binding_infos[index].as_ref() {
                if binding_info.has_dynamic_offset {
                    match binding {
                        WebGPUBoundResource::UniformBuffer(WebGPUBufferBindingInfo {
                            buffer: _,
                            offset,
                            length: _,
                        }) => {
                            set_binding.dynamic_offsets.push(*offset as u64);
                        }
                        WebGPUBoundResource::StorageBuffer(WebGPUBufferBindingInfo {
                            buffer: _,
                            offset,
                            length: _,
                        }) => {
                            set_binding.dynamic_offsets.push(*offset as u64);
                        }
                        WebGPUBoundResource::StorageBufferArray(buffers) => {
                            for WebGPUBufferBindingInfo {
                                buffer: _,
                                offset,
                                length: _,
                            } in buffers
                            {
                                set_binding.dynamic_offsets.push(*offset as u64);
                            }
                        }
                        WebGPUBoundResource::UniformBufferArray(buffers) => {
                            for WebGPUBufferBindingInfo {
                                buffer: _,
                                offset,
                                length: _,
                            } in buffers
                            {
                                set_binding.dynamic_offsets.push(*offset as u64);
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
        layout: &'a Arc<WebGPUBindGroupLayout>,
        bindings: &'a [T],
    ) -> Option<Arc<WebGPUBindGroup>>
    where
        WebGPUBoundResource: BindingCompare<Option<&'a T>>,
        WebGPUBoundResource: From<&'a T>,
    {
        if layout.is_empty() {
            log::warn!("Skipping bc empty layout");
            return None;
        }

        let transient = self.cache_mode != CacheMode::Everything;

        let cached_set = if self.cache_mode == CacheMode::None {
            None
        } else {
            self.find_compatible_set(frame, layout, &bindings, !transient)
        };
        let set: Arc<WebGPUBindGroup> = if let Some(cached_set) = cached_set {
            cached_set
        } else {
            let new_set = Arc::new(WebGPUBindGroup::new(&self.device, layout, transient, bindings).unwrap());

            if self.cache_mode != CacheMode::None {
                let mut cache = if transient {
                    self.transient_cache.borrow_mut()
                } else {
                    self.permanent_cache.borrow_mut()
                };
                cache
                    .entry(layout.clone())
                    .or_default()
                    .push(WebGPUBindGroupCacheEntry {
                        set: new_set.clone(),
                        last_used_frame: frame,
                    });
            }
            new_set
        };
        Some(set)
    }

    pub fn mark_all_dirty(&mut self) {
        self.dirty |= DirtyBindGroups::VERY_FREQUENT;
        self.dirty |= DirtyBindGroups::FREQUENT;
        self.dirty |= DirtyBindGroups::FRAME;
    }

    #[allow(unused)]
    #[inline(always)]
    pub fn dirty_sets(&self) -> DirtyBindGroups {
        self.dirty
    }

    pub(super) fn finish(
        &mut self,
        frame: u64,
        pipeline_layout: &WebGPUPipelineLayout,
    ) -> [Option<WebGPUBindGroupBinding>; gpu::NON_BINDLESS_SET_COUNT as usize] {
        if self.dirty.is_empty() {
            return Default::default();
        }

        let mut set_bindings: [Option<WebGPUBindGroupBinding>; gpu::NON_BINDLESS_SET_COUNT as usize] = Default::default();
        set_bindings[gpu::BindingFrequency::VeryFrequent as usize] =
            self.finish_set(frame, pipeline_layout, gpu::BindingFrequency::VeryFrequent);
        set_bindings[gpu::BindingFrequency::Frame as usize] =
            self.finish_set(frame, pipeline_layout, gpu::BindingFrequency::Frame);
        set_bindings[gpu::BindingFrequency::Frequent as usize] =
            self.finish_set(frame, pipeline_layout, gpu::BindingFrequency::Frequent);

        self.dirty = DirtyBindGroups::empty();
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
