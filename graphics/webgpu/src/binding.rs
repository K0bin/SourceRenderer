use std::{cell::RefCell, collections::HashMap, hash::Hash, ops::Deref, sync::Arc};

use bitflags::bitflags;
use js_sys::{wasm_bindgen::JsValue, Array};
use smallvec::SmallVec;
use sourcerenderer_core::gpu;
use web_sys::{GpuBindGroup, GpuBindGroupDescriptor, GpuBindGroupEntry, GpuBindGroupLayout, GpuBindGroupLayoutDescriptor, GpuBindGroupLayoutEntry, GpuBuffer, GpuBufferBinding, GpuBufferBindingLayout, GpuBufferBindingType, GpuDevice, GpuPipelineLayout, GpuPipelineLayoutDescriptor, GpuSampler, GpuSamplerBindingLayout, GpuSamplerBindingType, GpuStorageTextureAccess, GpuStorageTextureBindingLayout, GpuTextureBindingLayout, GpuTextureSampleType, GpuTextureView, GpuTextureViewDimension};

use crate::{sampler::WebGPUSampler, texture::{format_to_webgpu, texture_dimension_to_webgpu_view, WebGPUTextureView}};


bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct DirtyBindGroups: u32 {
        const VERY_FREQUENT = 0b0001;
        const FREQUENT = 0b0010;
        const FRAME = 0b0100;
        const BINDLESS_TEXTURES = 0b10000;
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

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub(crate) enum WebGPUResourceBindingType {
    None,
    UniformBuffer,
    StorageBuffer,
    StorageTexture,
    SampledTexture,
    SampledTextureAndSampler,
    Sampler,
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub(crate) struct WebGPUBindGroupEntryInfo {
    pub(crate) name: String,
    pub(crate) shader_stage: u32,
    pub(crate) index: u32,
    pub(crate) writable: bool,
    pub(crate) descriptor_type: WebGPUResourceBindingType,
    pub(crate) has_dynamic_offset: bool,
    pub(crate) sampling_type: gpu::SamplingType,
    pub(crate) texture_dimension: gpu::TextureDimension,
    pub(crate) is_multisampled: bool,
    pub(crate) storage_format: gpu::Format
}

pub struct WebGPUBindGroupLayout {
    bind_group_layout: GpuBindGroupLayout,
    binding_infos: [Option<WebGPUBindGroupEntryInfo>; gpu::PER_SET_BINDINGS as usize],
    is_empty: bool,
}

unsafe impl Send for WebGPUBindGroupLayout {}
unsafe impl Sync for WebGPUBindGroupLayout {}

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
        device: &GpuDevice
    ) -> Result<Self, ()> {
        let mut binding_infos: [Option<WebGPUBindGroupEntryInfo>; gpu::PER_SET_BINDINGS as usize] =
        Default::default();
        let entries = Array::new_with_length(bindings.len() as u32);
        for i in 0..bindings.len() {
            let binding = &bindings[i];
            let entry = GpuBindGroupLayoutEntry::new(binding.index, binding.shader_stage);
            match binding.descriptor_type {
                WebGPUResourceBindingType::None => continue,
                WebGPUResourceBindingType::UniformBuffer => {
                    let buffer_binding = GpuBufferBindingLayout::new();
                    buffer_binding.set_type(GpuBufferBindingType::Uniform);
                    buffer_binding.set_has_dynamic_offset(true);
                    entry.set_buffer(&buffer_binding);
                },
                WebGPUResourceBindingType::StorageBuffer => {
                    let buffer_binding = GpuBufferBindingLayout::new();
                    buffer_binding.set_type(if binding.writable {
                        GpuBufferBindingType::Storage
                    } else {
                        GpuBufferBindingType::ReadOnlyStorage
                    });
                    buffer_binding.set_has_dynamic_offset(true);
                    entry.set_buffer(&buffer_binding);
                }
                WebGPUResourceBindingType::StorageTexture => {
                    let texture_binding = GpuStorageTextureBindingLayout::new(format_to_webgpu(binding.storage_format));
                    texture_binding.set_access(if binding.writable { GpuStorageTextureAccess::ReadWrite } else { GpuStorageTextureAccess::ReadOnly });
                    texture_binding.set_view_dimension(texture_dimension_to_webgpu_view(binding.texture_dimension));
                    entry.set_storage_texture(&texture_binding);
                },
                WebGPUResourceBindingType::SampledTexture => {
                    let texture_binding = GpuTextureBindingLayout::new();
                    texture_binding.set_multisampled(binding.is_multisampled);
                    texture_binding.set_sample_type(sampling_type_to_webgpu(binding.sampling_type));
                    texture_binding.set_view_dimension(texture_dimension_to_webgpu_view(binding.texture_dimension));
                    entry.set_texture(&texture_binding);
                },
                WebGPUResourceBindingType::SampledTextureAndSampler => panic!("WebGPU does not support combined image and sampler"),
                WebGPUResourceBindingType::Sampler => {
                    let sampler = GpuSamplerBindingLayout::new();
                    sampler.set_type(GpuSamplerBindingType::Filtering);
                    entry.set_sampler(&sampler);
                },
            }
            entries.set(i as u32, JsValue::from(&entry));
            binding_infos[binding.index as usize] = Some(binding.clone());
        }
        let descriptor = GpuBindGroupLayoutDescriptor::new(&entries);
        let bind_group_layout = device.create_bind_group_layout(&descriptor).map_err(|_| ())?;
        Ok(Self {
            bind_group_layout,
            binding_infos,
            is_empty: bindings.is_empty()
        })
    }

    pub(crate) fn handle(&self) -> &GpuBindGroupLayout {
        &self.bind_group_layout
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.is_empty
    }

    pub(crate) fn binding(&self, slot: u32) -> Option<&WebGPUBindGroupEntryInfo> {
        self.binding_infos[slot as usize].as_ref()
    }

    pub(crate) fn is_dynamic_binding(&self, binding_index: u32) -> bool {
        if let Some(binding_info) = self.binding_infos[binding_index as usize].as_ref() {
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

unsafe impl Send for WebGPUPipelineLayout {}
unsafe impl Sync for WebGPUPipelineLayout {}

impl WebGPUPipelineLayout {
    pub(crate) fn new(device: &GpuDevice, bind_group_layouts: &[Option<Arc<WebGPUBindGroupLayout>>]) -> Self {
        let mut owned_bind_group_layouts: [Option<Arc<WebGPUBindGroupLayout>>; gpu::NON_BINDLESS_SET_COUNT as usize] = Default::default();
        let bind_group_layouts_js: Array = Array::new_with_length(gpu::NON_BINDLESS_SET_COUNT);
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
    bindings: [WebGPUBoundResource; gpu::PER_SET_BINDINGS as usize],
}

impl WebGPUBindGroup {
    fn new<'a, T>(
        device: &GpuDevice,
        layout: &Arc<WebGPUBindGroupLayout>,
        is_transient: bool,
        bindings: &'a [T; gpu::PER_SET_BINDINGS as usize],
    ) -> Result<Self, ()>
    where
        WebGPUBoundResource: From<&'a T>,
    {
        let entries = Array::new_with_length(gpu::PER_SET_BINDINGS);
        let mut stored_bindings = <[WebGPUBoundResource; gpu::PER_SET_BINDINGS as usize]>::default();
        for (index, binding) in bindings.iter().enumerate() {
            stored_bindings[index] = binding.into();
        }
        for (binding, resource) in stored_bindings.iter().enumerate() {
            let resource_js_value: JsValue;
            match resource {
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
                    if !layout.is_dynamic_binding(binding as u32) {
                        buffer_info.set_offset(binding_info.offset as f64);
                    }
                    resource_js_value = JsValue::from(&buffer_info);
                },
                WebGPUBoundResource::StorageBuffer(binding_info) => {
                    let buffer_info = GpuBufferBinding::new(&binding_info.buffer);
                    buffer_info.set_size(binding_info.length as f64);
                    if !layout.is_dynamic_binding(binding as u32) {
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
                WebGPUBoundResource::SampledTextureAndSampler(_texture, _sampler) => panic!("Combined texture and sampler is not supported on WebGPU"),
                WebGPUBoundResource::SampledTextureAndSamplerArray(_textures_and_samplers) => panic!("Descriptor arrays are not supported on WebGPU"),
            }
            let entry = GpuBindGroupEntry::new(binding as u32, &resource_js_value);
            entries.set(binding as u32, JsValue::from(&entry));
        }

        let descriptor = GpuBindGroupDescriptor::new(&entries, layout.handle());
        let bind_group = device.create_bind_group(&descriptor);
        Ok(Self {
            bind_group,
            layout: layout.clone(),
            is_transient: is_transient,
            bindings: stored_bindings
        })
    }

    #[inline]
    pub(crate) fn handle(&self) -> &GpuBindGroup {
        &self.bind_group
    }

    #[inline]
    pub(crate) fn is_transient(&self) -> bool {
        self.is_transient
    }

    pub(crate) fn is_compatible<T>(
        &self,
        layout: &Arc<WebGPUBindGroupLayout>,
        bindings: &[T; gpu::PER_SET_BINDINGS as usize],
    ) -> bool
    where
        WebGPUBoundResource: BindingCompare<T>,
    {
        if &self.layout != layout {
            return false;
        }

        self.bindings.iter().enumerate().all(|(index, binding)| {
            let binding_info = self.layout.binding_infos[index].as_ref();
            binding.binding_eq(&bindings[index], binding_info)
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
    UniformBufferArray(SmallVec<[WebGPUBufferBindingInfo; gpu::PER_SET_BINDINGS as usize]>),
    StorageBuffer(WebGPUBufferBindingInfo),
    StorageBufferArray(SmallVec<[WebGPUBufferBindingInfo; gpu::PER_SET_BINDINGS as usize]>),
    StorageTexture(WebGPUHashableTextureView),
    StorageTextureArray(SmallVec<[WebGPUHashableTextureView; gpu::PER_SET_BINDINGS as usize]>),
    SampledTexture(WebGPUHashableTextureView),
    SampledTextureArray(SmallVec<[WebGPUHashableTextureView; gpu::PER_SET_BINDINGS as usize]>),
    SampledTextureAndSampler(WebGPUHashableTextureView, WebGPUHashableSampler),
    SampledTextureAndSamplerArray(SmallVec<[(WebGPUHashableTextureView, WebGPUHashableSampler); gpu::PER_SET_BINDINGS as usize]>),
    Sampler(WebGPUHashableSampler),
}

impl Default for WebGPUBoundResource {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Hash, Eq, PartialEq, Clone)]
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

impl Default for WebGPUBoundResourceRef<'_> {
    fn default() -> Self {
        Self::None
    }
}

impl From<&WebGPUBoundResourceRef<'_>> for WebGPUBoundResource {
    fn from(binding: &WebGPUBoundResourceRef<'_>) -> Self {
        match binding {
            WebGPUBoundResourceRef::None => WebGPUBoundResource::None,
            WebGPUBoundResourceRef::UniformBuffer(info) => WebGPUBoundResource::UniformBuffer(info.clone()),
            WebGPUBoundResourceRef::StorageBuffer(info) => WebGPUBoundResource::StorageBuffer(info.clone()),
            WebGPUBoundResourceRef::StorageTexture(view) => WebGPUBoundResource::StorageTexture(view.clone()),
            WebGPUBoundResourceRef::SampledTexture(view) => WebGPUBoundResource::SampledTexture(view.clone()),
            WebGPUBoundResourceRef::SampledTextureAndSampler(view, sampler) => {
                WebGPUBoundResource::SampledTextureAndSampler(view.clone(), sampler.clone())
            }
            WebGPUBoundResourceRef::Sampler(sampler) => WebGPUBoundResource::Sampler(sampler.clone()),
            WebGPUBoundResourceRef::UniformBufferArray(arr) => WebGPUBoundResource::UniformBufferArray(
                arr.iter()
                    .map(|a| {
                        let info: WebGPUBufferBindingInfo = a.clone();
                        info
                    })
                    .collect(),
            ),
            WebGPUBoundResourceRef::StorageBufferArray(arr) => WebGPUBoundResource::StorageBufferArray(
                arr.iter()
                    .map(|a| {
                        let info: WebGPUBufferBindingInfo = a.clone();
                        info
                    })
                    .collect(),
            ),
            WebGPUBoundResourceRef::StorageTextureArray(arr) => {
                WebGPUBoundResource::StorageTextureArray(arr.iter().map(|a| a.clone()).collect())
            }
            WebGPUBoundResourceRef::SampledTextureArray(arr) => {
                WebGPUBoundResource::SampledTextureArray(arr.iter().map(|a| a.clone()).collect())
            }
            WebGPUBoundResourceRef::SampledTextureAndSamplerArray(arr) => {
                WebGPUBoundResource::SampledTextureAndSamplerArray(
                    arr.iter()
                        .map(|(t, s)| {
                            let tuple: (WebGPUHashableTextureView, WebGPUHashableSampler) = (t.clone(), s.clone());
                            tuple
                        })
                        .collect(),
                )
            }
        }
    }
}

impl From<&Self> for WebGPUBoundResource {
    fn from(other: &Self) -> Self {
        other.clone()
    }
}

impl PartialEq<WebGPUBoundResourceRef<'_>> for WebGPUBoundResource {
    fn eq(&self, other: &WebGPUBoundResourceRef) -> bool {
        match (self, other) {
            (WebGPUBoundResource::None, WebGPUBoundResourceRef::None) => true,
            (
                WebGPUBoundResource::UniformBuffer(WebGPUBufferBindingInfo {
                    buffer: old,
                    offset: old_offset,
                    length: old_length,
                }),
                WebGPUBoundResourceRef::UniformBuffer(WebGPUBufferBindingInfo {
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
                WebGPUBoundResourceRef::StorageBuffer(WebGPUBufferBindingInfo {
                    buffer: new,
                    offset: new_offset,
                    length: new_length,
                }),
            ) => old == new && old_offset == new_offset && old_length == new_length,
            (WebGPUBoundResource::StorageTexture(old), WebGPUBoundResourceRef::StorageTexture(new)) => {
                old == new
            }
            (WebGPUBoundResource::SampledTexture(old), WebGPUBoundResourceRef::SampledTexture(new)) => {
                old == new
            }
            (
                WebGPUBoundResource::SampledTextureAndSampler(old_tex, old_sampler),
                WebGPUBoundResourceRef::SampledTextureAndSampler(new_tex, new_sampler),
            ) => old_tex == new_tex && old_sampler == new_sampler,
            (WebGPUBoundResource::Sampler(old_sampler), WebGPUBoundResourceRef::Sampler(new_sampler)) => {
                old_sampler == new_sampler
            }
            (
                WebGPUBoundResource::StorageBufferArray(old),
                WebGPUBoundResourceRef::StorageBufferArray(new),
            ) => &old[..] == &new[..],
            (
                WebGPUBoundResource::UniformBufferArray(old),
                WebGPUBoundResourceRef::UniformBufferArray(new),
            ) => &old[..] == &new[..],
            (
                WebGPUBoundResource::SampledTextureArray(old),
                WebGPUBoundResourceRef::SampledTextureArray(new),
            ) => old.iter().zip(new.iter()).all(|(old, new)| old == new),
            (
                WebGPUBoundResource::StorageTextureArray(old),
                WebGPUBoundResourceRef::StorageTextureArray(new),
            ) => old.iter().zip(new.iter()).all(|(old, new)| old == new),
            (
                WebGPUBoundResource::SampledTextureAndSamplerArray(old),
                WebGPUBoundResourceRef::SampledTextureAndSamplerArray(new),
            ) => old.iter().zip(new.iter()).all(
                |((old_texture, old_sampler), (new_texture, new_sampler))| {
                    old_texture == new_texture && old_sampler == new_sampler
                },
            ),
            _ => false,
        }
    }
}

impl PartialEq<WebGPUBoundResource> for WebGPUBoundResourceRef<'_> {
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

impl BindingCompare<WebGPUBoundResourceRef<'_>> for WebGPUBoundResource {
    fn binding_eq(
        &self,
        other: &WebGPUBoundResourceRef<'_>,
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
                WebGPUBoundResourceRef::UniformBuffer(WebGPUBufferBindingInfo {
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
                WebGPUBoundResourceRef::StorageBuffer(WebGPUBufferBindingInfo {
                    buffer,
                    offset: _,
                    length,
                }),
            ) = (self, other)
            {
                buffer == entry_buffer && *length == *entry_length
            } else if let (
                WebGPUBoundResource::StorageBufferArray(arr),
                WebGPUBoundResourceRef::StorageBufferArray(arr1),
            ) = (self, other)
            {
                arr.iter()
                    .zip(*arr1)
                    .all(|(b, b1)| b.buffer == b1.buffer && b.length == b1.length)
            } else if let (
                WebGPUBoundResource::UniformBufferArray(arr),
                WebGPUBoundResourceRef::UniformBufferArray(arr1),
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


pub(crate) struct WebGPUBindGroupBinding {
    pub(crate) set: Arc<WebGPUBindGroup>,
    pub(crate) dynamic_offset_count: u32,
    pub(crate) dynamic_offsets: [u64; gpu::PER_SET_BINDINGS as usize],
}

struct WebGPUBindGroupCacheEntry {
    set: Arc<WebGPUBindGroup>,
    last_used_frame: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq)]
enum CacheMode {
    None,
    PerFrame,
    Everything,
}

pub(crate) struct WebGPUBindingManager {
    cache_mode: CacheMode,
    device: GpuDevice,
    current_sets: [Option<Arc<WebGPUBindGroup>>; 4],
    dirty: DirtyBindGroups,
    bindings: [[WebGPUBoundResource; gpu::PER_SET_BINDINGS as usize]; 4],
    transient_cache: RefCell<HashMap<Arc<WebGPUBindGroupLayout>, Vec<WebGPUBindGroupCacheEntry>>>,
    permanent_cache: RefCell<HashMap<Arc<WebGPUBindGroupLayout>, Vec<WebGPUBindGroupCacheEntry>>>,
    last_cleanup_frame: u64,
}

impl WebGPUBindingManager {
    pub(crate) fn new(device: &GpuDevice) -> Self {
        let cache_mode = CacheMode::Everything;

        Self {
            cache_mode,
            device: device.clone(),
            current_sets: Default::default(),
            dirty: DirtyBindGroups::empty(),
            bindings: Default::default(),
            transient_cache: RefCell::new(HashMap::new()),
            permanent_cache: RefCell::new(HashMap::new()),
            last_cleanup_frame: 0,
        }
    }

    pub(crate) fn reset(&mut self, frame: u64) {
        self.dirty = DirtyBindGroups::empty();
        self.bindings = Default::default();
        self.current_sets = Default::default();
        self.clean_permanent_cache(frame);
        if self.cache_mode != CacheMode::None {
            let mut transient_cache_mut = self.transient_cache.borrow_mut();
            transient_cache_mut.clear();
        }
    }

    pub(crate) fn bind(
        &mut self,
        frequency: gpu::BindingFrequency,
        slot: u32,
        binding: WebGPUBoundResourceRef,
    ) {
        let bindings_table = &mut self.bindings[frequency as usize];
        let existing_binding = &mut bindings_table[slot as usize];

        let identical = existing_binding == &binding;

        if !identical {
            self.dirty.insert(DirtyBindGroups::from(frequency));
            *existing_binding = (&binding).into();
        }
    }

    fn find_compatible_set<T>(
        &self,
        frame: u64,
        layout: &Arc<WebGPUBindGroupLayout>,
        bindings: &[T; gpu::PER_SET_BINDINGS as usize],
        use_permanent_cache: bool,
    ) -> Option<Arc<WebGPUBindGroup>>
    where
        WebGPUBoundResource: BindingCompare<T>,
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
        let bindings = &self.bindings[frequency as usize];
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

    pub fn get_descriptor_set_binding_info(
        &self,
        set: Arc<WebGPUBindGroup>,
        bindings: &[WebGPUBoundResource; gpu::PER_SET_BINDINGS as usize],
    ) -> WebGPUBindGroupBinding {
        let mut set_binding = WebGPUBindGroupBinding {
            set: set.clone(),
            dynamic_offsets: Default::default(),
            dynamic_offset_count: 0,
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
                            set_binding.dynamic_offsets
                                [set_binding.dynamic_offset_count as usize] = *offset as u64;
                            set_binding.dynamic_offset_count += 1;
                        }
                        WebGPUBoundResource::StorageBuffer(WebGPUBufferBindingInfo {
                            buffer: _,
                            offset,
                            length: _,
                        }) => {
                            set_binding.dynamic_offsets
                                [set_binding.dynamic_offset_count as usize] = *offset as u64;
                            set_binding.dynamic_offset_count += 1;
                        }
                        WebGPUBoundResource::StorageBufferArray(buffers) => {
                            for WebGPUBufferBindingInfo {
                                buffer: _,
                                offset,
                                length: _,
                            } in buffers
                            {
                                set_binding.dynamic_offsets
                                    [set_binding.dynamic_offset_count as usize] = *offset as u64;
                                set_binding.dynamic_offset_count += 1;
                            }
                        }
                        WebGPUBoundResource::UniformBufferArray(buffers) => {
                            for WebGPUBufferBindingInfo {
                                buffer: _,
                                offset,
                                length: _,
                            } in buffers
                            {
                                set_binding.dynamic_offsets
                                    [set_binding.dynamic_offset_count as usize] = *offset as u64;
                                set_binding.dynamic_offset_count += 1;
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
        layout: &Arc<WebGPUBindGroupLayout>,
        bindings: &'a [T; gpu::PER_SET_BINDINGS as usize],
    ) -> Option<Arc<WebGPUBindGroup>>
    where
        WebGPUBoundResource: BindingCompare<T>,
        WebGPUBoundResource: From<&'a T>,
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
        self.dirty |= DirtyBindGroups::BINDLESS_TEXTURES;
    }

    pub fn dirty_sets(&self) -> DirtyBindGroups {
        self.dirty
    }

    pub(super) fn finish(
        &mut self,
        frame: u64,
        pipeline_layout: &WebGPUPipelineLayout,
    ) -> [Option<WebGPUBindGroupBinding>; 3] {
        if self.dirty.is_empty() {
            return Default::default();
        }

        let mut set_bindings: [Option<WebGPUBindGroupBinding>; 3] = Default::default();
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
