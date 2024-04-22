use metal;
use metal::foreign_types::{ForeignType, ForeignTypeRef};

use objc::Encode;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{self, Texture as _};

use super::*;

use bitflags::bitflags;

pub const PER_SET_BINDINGS: usize = 32;

#[derive(Clone)]
pub(crate) struct MTLBufferBindingInfo {
    pub(crate) buffer: metal::Buffer,
    pub(crate) offset: u64,
    pub(crate) length: u64
}

#[derive(Clone)]
pub(crate) struct MTLBufferBindingInfoRef<'a> {
    pub(crate) buffer: &'a metal::BufferRef,
    pub(crate) offset: u64,
    pub(crate) length: u64
}

impl From<&MTLBufferBindingInfoRef<'_>> for MTLBufferBindingInfo {
    fn from(binding: &MTLBufferBindingInfoRef<'_>) -> Self {
        Self {
            buffer: binding.buffer.to_owned(),
            offset: binding.offset,
            length: binding.length
        }
    }
}

impl PartialEq<MTLBufferBindingInfoRef<'_>> for MTLBufferBindingInfo {
    fn eq(&self, other: &MTLBufferBindingInfoRef) -> bool {
        self.buffer.as_ptr() == other.buffer.as_ptr() && self.offset == other.offset && self.length == other.length
    }
}

#[derive(Clone)]
pub(crate) enum MTLBoundResource {
    None,
    UniformBuffer(MTLBufferBindingInfo),
    UniformBufferArray(SmallVec<[MTLBufferBindingInfo; PER_SET_BINDINGS]>),
    StorageBuffer(MTLBufferBindingInfo),
    StorageBufferArray(SmallVec<[MTLBufferBindingInfo; PER_SET_BINDINGS]>),
    StorageTexture(metal::Texture),
    StorageTextureArray(SmallVec<[metal::Texture; PER_SET_BINDINGS]>),
    SampledTexture(metal::Texture),
    SampledTextureArray(SmallVec<[metal::Texture; PER_SET_BINDINGS]>),
    SampledTextureAndSampler(metal::Texture, metal::SamplerState),
    SampledTextureAndSamplerArray(SmallVec<[(metal::Texture, metal::SamplerState); PER_SET_BINDINGS]>),
    Sampler(metal::SamplerState),
    AccelerationStructure(metal::AccelerationStructure),
}

impl Default for MTLBoundResource {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone)]
pub(crate) enum MTLBoundResourceRef<'a> {
    None,
    UniformBuffer(MTLBufferBindingInfoRef<'a>),
    UniformBufferArray(&'a [MTLBufferBindingInfoRef<'a>]),
    StorageBuffer(MTLBufferBindingInfoRef<'a>),
    StorageBufferArray(&'a [MTLBufferBindingInfoRef<'a>]),
    StorageTexture(&'a metal::TextureRef),
    StorageTextureArray(&'a [&'a metal::TextureRef]),
    SampledTexture(&'a metal::TextureRef),
    SampledTextureArray(&'a [&'a metal::TextureRef]),
    SampledTextureAndSampler(&'a metal::TextureRef, &'a metal::SamplerStateRef),
    SampledTextureAndSamplerArray(&'a [(&'a metal::TextureRef, &'a metal::SamplerStateRef)]),
    Sampler(&'a metal::SamplerStateRef),
    AccelerationStructure(&'a metal::AccelerationStructureRef),
}

impl Default for MTLBoundResourceRef<'_> {
    fn default() -> Self {
        Self::None
    }
}

impl From<&MTLBoundResourceRef<'_>> for MTLBoundResource {
    fn from(binding: &MTLBoundResourceRef<'_>) -> Self {
        match binding {
            MTLBoundResourceRef::None => MTLBoundResource::None,
            MTLBoundResourceRef::UniformBuffer(info) => MTLBoundResource::UniformBuffer(info.into()),
            MTLBoundResourceRef::StorageBuffer(info) => MTLBoundResource::StorageBuffer(info.into()),
            MTLBoundResourceRef::StorageTexture(view) => MTLBoundResource::StorageTexture((*view).to_owned()),
            MTLBoundResourceRef::SampledTexture(view) => MTLBoundResource::SampledTexture((*view).to_owned()),
            MTLBoundResourceRef::SampledTextureAndSampler(view, sampler) => {
                MTLBoundResource::SampledTextureAndSampler((*view).to_owned(), (*sampler).to_owned())
            }
            MTLBoundResourceRef::Sampler(sampler) => MTLBoundResource::Sampler((*sampler).to_owned()),
            MTLBoundResourceRef::AccelerationStructure(accel) => {
                MTLBoundResource::AccelerationStructure((*accel).to_owned())
            }
            MTLBoundResourceRef::UniformBufferArray(arr) => MTLBoundResource::UniformBufferArray(
                arr.iter()
                    .map(|a| a.into())
                    .collect(),
            ),
            MTLBoundResourceRef::StorageBufferArray(arr) => MTLBoundResource::StorageBufferArray(
                arr.iter()
                    .map(|a| a.into())
                    .collect(),
            ),
            MTLBoundResourceRef::StorageTextureArray(arr) => {
                MTLBoundResource::StorageTextureArray(arr.iter().map(|a| (*a).to_owned()).collect())
            }
            MTLBoundResourceRef::SampledTextureArray(arr) => {
                MTLBoundResource::SampledTextureArray(arr.iter().map(|a| (*a).to_owned()).collect())
            }
            MTLBoundResourceRef::SampledTextureAndSamplerArray(arr) => {
                MTLBoundResource::SampledTextureAndSamplerArray(
                    arr.iter()
                        .map(|(t, s)| {
                            let tuple: (metal::Texture, metal::SamplerState) = ((*t).to_owned(), (*s).to_owned());
                            tuple
                        })
                        .collect(),
                )
            }
        }
    }
}

impl From<&Self> for MTLBoundResource {
    fn from(other: &Self) -> Self {
        other.clone()
    }
}


impl PartialEq<MTLBoundResourceRef<'_>> for MTLBoundResource {
    fn eq(&self, other: &MTLBoundResourceRef) -> bool {
        match (self, other) {
            (MTLBoundResource::None, MTLBoundResourceRef::None) => true,
            (
                MTLBoundResource::UniformBuffer(MTLBufferBindingInfo {
                    buffer: old,
                    offset: old_offset,
                    length: old_length,
                }),
                MTLBoundResourceRef::UniformBuffer(MTLBufferBindingInfoRef {
                    buffer: new,
                    offset: new_offset,
                    length: new_length,
                }),
            ) => old.as_ptr() == new.as_ptr() && old_offset == new_offset && old_length == new_length,
            (
                MTLBoundResource::StorageBuffer(MTLBufferBindingInfo {
                    buffer: old,
                    offset: old_offset,
                    length: old_length,
                }),
                MTLBoundResourceRef::StorageBuffer(MTLBufferBindingInfoRef {
                    buffer: new,
                    offset: new_offset,
                    length: new_length,
                }),
            ) => old.as_ptr() == new.as_ptr() && old_offset == new_offset && old_length == new_length,
            (MTLBoundResource::StorageTexture(old), MTLBoundResourceRef::StorageTexture(new)) => {
                old.as_ptr() == new.as_ptr()
            }
            (MTLBoundResource::SampledTexture(old), MTLBoundResourceRef::SampledTexture(new)) => {
                old.as_ptr() == new.as_ptr()
            }
            (
                MTLBoundResource::SampledTextureAndSampler(old_tex, old_sampler),
                MTLBoundResourceRef::SampledTextureAndSampler(new_tex, new_sampler),
            ) => old_tex.as_ptr() == new_tex.as_ptr() && old_sampler.as_ptr() == new_sampler.as_ptr(),
            (MTLBoundResource::Sampler(old_sampler), MTLBoundResourceRef::Sampler(new_sampler)) => {
                old_sampler.as_ptr() == new_sampler.as_ptr()
            }
            (
                MTLBoundResource::AccelerationStructure(old),
                MTLBoundResourceRef::AccelerationStructure(new),
            ) => old.as_ptr() == new.as_ptr(),
            (
                MTLBoundResource::StorageBufferArray(old),
                MTLBoundResourceRef::StorageBufferArray(new),
            ) => &old[..] == &new[..],
            (
                MTLBoundResource::UniformBufferArray(old),
                MTLBoundResourceRef::UniformBufferArray(new),
            ) => &old[..] == &new[..],
            (
                MTLBoundResource::SampledTextureArray(old),
                MTLBoundResourceRef::SampledTextureArray(new),
            ) => old.iter().zip(new.iter()).all(|(old, new)| old.as_ptr() == new.as_ptr()),
            (
                MTLBoundResource::StorageTextureArray(old),
                MTLBoundResourceRef::StorageTextureArray(new),
            ) => old.iter().zip(new.iter()).all(|(old, new)| old.as_ptr() == new.as_ptr()),
            (
                MTLBoundResource::SampledTextureAndSamplerArray(old),
                MTLBoundResourceRef::SampledTextureAndSamplerArray(new),
            ) => old.iter().zip(new.iter()).all(
                |((old_texture, old_sampler), (new_texture, new_sampler))| {
                    old_texture.as_ptr() == new_texture.as_ptr() && old_sampler.as_ptr() == new_sampler.as_ptr()
                },
            ),
            _ => false,
        }
    }
}

pub(crate) enum MTLEncoderRef<'a> {
    Graphics(&'a metal::RenderCommandEncoderRef),
    Compute(&'a metal::ComputeCommandEncoderRef)
}

pub(crate) struct MTLBindingManager {
    bindings: [[MTLBoundResource; PER_SET_BINDINGS]; 4],
    dirty: [u64; 4]
}

impl MTLBindingManager {
    pub(crate) fn new() -> Self {
        Self {
            bindings: Default::default(),
            dirty: Default::default()
        }
    }

    pub(crate) fn bind(
        &mut self,
        frequency: gpu::BindingFrequency,
        slot: u32,
        binding: MTLBoundResourceRef,
    ) {
        let bindings_table = &mut self.bindings[frequency as usize];
        let existing_binding = &mut bindings_table[slot as usize];

        let identical = existing_binding == &binding;

        if !identical {
            self.dirty[frequency as usize] |= 1 << slot;
            *existing_binding = (&binding).into();
        }
    }

    pub(crate) fn finish(
        &mut self,
        encoder: MTLEncoderRef,
        pipeline: &PipelineResourceMap
    ) {
        match encoder {
            MTLEncoderRef::Graphics(encoder) => {
                for (set_index, dirty) in &mut self.dirty.iter_mut().enumerate() {
                    while dirty.count_ones() != 0 {
                        let slot = dirty.trailing_zeros();
                        *dirty &= !(1 << slot as u64);

                        let metal_index_opt = pipeline.resources.get(&(gpu::ShaderType::VertexShader, set_index as u32, slot));
                        if let Some(metal_binding) = metal_index_opt {
                            match &self.bindings[set_index][slot as usize] {
                                MTLBoundResource::None => {
                                    if let Some(binding) = metal_binding.texture_binding {
                                        encoder.set_vertex_texture(binding as u64, None);
                                    }
                                    if let Some(binding) = metal_binding.sampler_binding {
                                        encoder.set_vertex_sampler_state(binding as u64, None);
                                    }
                                    if let Some(binding) = metal_binding.buffer_binding {
                                        encoder.set_vertex_buffer(binding as u64, None, 0u64);
                                    }
                                },
                                MTLBoundResource::SampledTexture(texture) => {
                                    if metal_binding.texture_binding.is_none() {
                                        continue;
                                    }
                                    encoder.set_vertex_texture(metal_binding.texture_binding.unwrap() as u64, Some(texture));
                                },
                                MTLBoundResource::Sampler(sampler) => {
                                    if metal_binding.sampler_binding.is_none() {
                                        continue;
                                    }
                                    encoder.set_vertex_sampler_state(metal_binding.sampler_binding.unwrap() as u64, Some(sampler));
                                },
                                MTLBoundResource::StorageTexture(texture) => {
                                    if metal_binding.texture_binding.is_none() {
                                        continue;
                                    }
                                    encoder.set_vertex_texture(metal_binding.texture_binding.unwrap() as u64, Some(texture));
                                },
                                MTLBoundResource::SampledTextureAndSampler(texture, sampler) => {
                                    if metal_binding.texture_binding.is_none() || metal_binding.sampler_binding.is_none() {
                                        continue;
                                    }
                                    encoder.set_vertex_texture(metal_binding.texture_binding.unwrap() as u64, Some(texture));
                                    encoder.set_vertex_sampler_state(metal_binding.sampler_binding.unwrap() as u64, Some(sampler));
                                }
                                MTLBoundResource::SampledTextureArray(textures) => {
                                    if metal_binding.texture_binding.is_none() {
                                        continue;
                                    }
                                    let mut handles_opt = SmallVec::<[Option<&metal::TextureRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                    for array_entry in textures {
                                        handles_opt.push(Some(&array_entry));
                                    }
                                    handles_opt.resize(metal_binding.array_count as usize, None);
                                    encoder.set_vertex_textures(metal_binding.texture_binding.unwrap() as u64, &handles_opt);
                                }
                                MTLBoundResource::StorageTextureArray(textures) => {
                                    if metal_binding.texture_binding.is_none() {
                                        continue;
                                    }
                                    let mut handles_opt = SmallVec::<[Option<&metal::TextureRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                    for array_entry in textures {
                                        handles_opt.push(Some(&array_entry));
                                    }
                                    handles_opt.resize(metal_binding.array_count as usize, None);
                                    encoder.set_vertex_textures(metal_binding.texture_binding.unwrap() as u64, &handles_opt);
                                }
                                MTLBoundResource::SampledTextureAndSamplerArray(textures_and_samplers) => {
                                    if metal_binding.texture_binding.is_none() || metal_binding.sampler_binding.is_none() {
                                        continue;
                                    }
                                    let mut texture_handles_opt = SmallVec::<[Option<&metal::TextureRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                    let mut sampler_handles_opt = SmallVec::<[Option<&metal::SamplerStateRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                    for (texture, sampler) in textures_and_samplers {
                                        texture_handles_opt.push(Some(&texture));
                                        sampler_handles_opt.push(Some(&sampler));
                                    }
                                    texture_handles_opt.resize(metal_binding.array_count as usize, None);
                                    sampler_handles_opt.resize(metal_binding.array_count as usize, None);
                                    encoder.set_vertex_textures(metal_binding.texture_binding.unwrap() as u64, &texture_handles_opt);
                                    encoder.set_vertex_sampler_states(metal_binding.sampler_binding.unwrap() as u64, &sampler_handles_opt);
                                }
                                MTLBoundResource::UniformBuffer(buffer_info) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    encoder.set_vertex_buffer(metal_binding.buffer_binding.unwrap() as u64, Some(&buffer_info.buffer), buffer_info.offset);
                                }
                                MTLBoundResource::StorageBuffer(buffer_info) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    encoder.set_vertex_buffer(metal_binding.buffer_binding.unwrap() as u64, Some(&buffer_info.buffer), buffer_info.offset);
                                }
                                MTLBoundResource::AccelerationStructure(acceleration_structure) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    encoder.set_vertex_acceleration_structure(metal_binding.buffer_binding.unwrap() as u64, Some(&acceleration_structure));
                                }
                                MTLBoundResource::UniformBufferArray(buffers) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    let mut handles_opt = SmallVec::<[Option<&metal::BufferRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                    let mut offsets = SmallVec::<[u64; 32]>::with_capacity(metal_binding.array_count as usize);
                                    for array_entry in buffers {
                                        handles_opt.push(Some(&array_entry.buffer));
                                        offsets.push(array_entry.offset);
                                    }
                                    handles_opt.resize(metal_binding.array_count as usize, None);
                                    offsets.resize(metal_binding.array_count as usize, 0u64);
                                    encoder.set_vertex_buffers(metal_binding.texture_binding.unwrap() as u64, &handles_opt, &offsets);
                                },
                                MTLBoundResource::StorageBufferArray(buffers) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    let mut handles_opt = SmallVec::<[Option<&metal::BufferRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                    let mut offsets = SmallVec::<[u64; 32]>::with_capacity(metal_binding.array_count as usize);
                                    for array_entry in buffers {
                                        handles_opt.push(Some(&array_entry.buffer));
                                        offsets.push(array_entry.offset);
                                    }
                                    handles_opt.resize(metal_binding.array_count as usize, None);
                                    offsets.resize(metal_binding.array_count as usize, 0u64);
                                    encoder.set_vertex_buffers(metal_binding.texture_binding.unwrap() as u64, &handles_opt, &offsets);
                                },
                            }
                        }
                        let metal_index_opt = pipeline.resources.get(&(gpu::ShaderType::FragmentShader, set_index as u32, slot));
                        if let Some(metal_binding) = metal_index_opt {
                            match &self.bindings[set_index][slot as usize] {
                                MTLBoundResource::None => {
                                    if let Some(binding) = metal_binding.texture_binding {
                                        encoder.set_fragment_texture(binding as u64, None);
                                    }
                                    if let Some(binding) = metal_binding.sampler_binding {
                                        encoder.set_fragment_sampler_state(binding as u64, None);
                                    }
                                    if let Some(binding) = metal_binding.buffer_binding {
                                        encoder.set_fragment_buffer(binding as u64, None, 0u64);
                                    }
                                },
                                MTLBoundResource::SampledTexture(texture) => {
                                    if metal_binding.texture_binding.is_none() {
                                        continue;
                                    }
                                    encoder.set_fragment_texture(metal_binding.texture_binding.unwrap() as u64, Some(texture));
                                },
                                MTLBoundResource::Sampler(sampler) => {
                                    if metal_binding.sampler_binding.is_none() {
                                        continue;
                                    }
                                    encoder.set_fragment_sampler_state(metal_binding.sampler_binding.unwrap() as u64, Some(sampler));
                                },
                                MTLBoundResource::StorageTexture(texture) => {
                                    if metal_binding.texture_binding.is_none() {
                                        continue;
                                    }
                                    encoder.set_fragment_texture(metal_binding.texture_binding.unwrap() as u64, Some(texture));
                                },
                                MTLBoundResource::SampledTextureAndSampler(texture, sampler) => {
                                    if metal_binding.texture_binding.is_none() || metal_binding.sampler_binding.is_none() {
                                        continue;
                                    }
                                    encoder.set_fragment_texture(metal_binding.texture_binding.unwrap() as u64, Some(texture));
                                    encoder.set_fragment_sampler_state(metal_binding.sampler_binding.unwrap() as u64, Some(sampler));
                                }
                                MTLBoundResource::SampledTextureArray(textures) => {
                                    if metal_binding.texture_binding.is_none() {
                                        continue;
                                    }
                                    let mut handles_opt = SmallVec::<[Option<&metal::TextureRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                    for array_entry in textures {
                                        handles_opt.push(Some(&array_entry));
                                    }
                                    handles_opt.resize(metal_binding.array_count as usize, None);
                                    encoder.set_fragment_textures(metal_binding.texture_binding.unwrap() as u64, &handles_opt);
                                }
                                MTLBoundResource::StorageTextureArray(textures) => {
                                    if metal_binding.texture_binding.is_none() {
                                        continue;
                                    }
                                    let mut handles_opt = SmallVec::<[Option<&metal::TextureRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                    for array_entry in textures {
                                        handles_opt.push(Some(&array_entry));
                                    }
                                    handles_opt.resize(metal_binding.array_count as usize, None);
                                    encoder.set_fragment_textures(metal_binding.texture_binding.unwrap() as u64, &handles_opt);
                                }
                                MTLBoundResource::SampledTextureAndSamplerArray(textures_and_samplers) => {
                                    if metal_binding.texture_binding.is_none() || metal_binding.sampler_binding.is_none() {
                                        continue;
                                    }
                                    let mut texture_handles_opt = SmallVec::<[Option<&metal::TextureRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                    let mut sampler_handles_opt = SmallVec::<[Option<&metal::SamplerStateRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                    for (texture, sampler) in textures_and_samplers {
                                        texture_handles_opt.push(Some(&texture));
                                        sampler_handles_opt.push(Some(&sampler));
                                    }
                                    texture_handles_opt.resize(metal_binding.array_count as usize, None);
                                    sampler_handles_opt.resize(metal_binding.array_count as usize, None);
                                    encoder.set_fragment_textures(metal_binding.texture_binding.unwrap() as u64, &texture_handles_opt);
                                    encoder.set_fragment_sampler_states(metal_binding.sampler_binding.unwrap() as u64, &sampler_handles_opt);
                                }
                                MTLBoundResource::UniformBuffer(buffer_info) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    encoder.set_fragment_buffer(metal_binding.buffer_binding.unwrap() as u64, Some(&buffer_info.buffer), buffer_info.offset);
                                }
                                MTLBoundResource::StorageBuffer(buffer_info) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    encoder.set_fragment_buffer(metal_binding.buffer_binding.unwrap() as u64, Some(&buffer_info.buffer), buffer_info.offset);
                                }
                                MTLBoundResource::AccelerationStructure(acceleration_structure) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    encoder.set_fragment_acceleration_structure(metal_binding.buffer_binding.unwrap() as u64, Some(&acceleration_structure));
                                }
                                MTLBoundResource::UniformBufferArray(buffers) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    let mut handles_opt = SmallVec::<[Option<&metal::BufferRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                    let mut offsets = SmallVec::<[u64; 32]>::with_capacity(metal_binding.array_count as usize);
                                    for array_entry in buffers {
                                        handles_opt.push(Some(&array_entry.buffer));
                                        offsets.push(array_entry.offset);
                                    }
                                    handles_opt.resize(metal_binding.array_count as usize, None);
                                    offsets.resize(metal_binding.array_count as usize, 0u64);
                                    encoder.set_fragment_buffers(metal_binding.texture_binding.unwrap() as u64, &handles_opt, &offsets);
                                },
                                MTLBoundResource::StorageBufferArray(buffers) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    let mut handles_opt = SmallVec::<[Option<&metal::BufferRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                    let mut offsets = SmallVec::<[u64; 32]>::with_capacity(metal_binding.array_count as usize);
                                    for array_entry in buffers {
                                        handles_opt.push(Some(&array_entry.buffer));
                                        offsets.push(array_entry.offset);
                                    }
                                    handles_opt.resize(metal_binding.array_count as usize, None);
                                    offsets.resize(metal_binding.array_count as usize, 0u64);
                                    encoder.set_fragment_buffers(metal_binding.texture_binding.unwrap() as u64, &handles_opt, &offsets);
                                },
                            }
                        }
                    }
                }
            },
            MTLEncoderRef::Compute(encoder) => {
                for (set_index, dirty) in &mut self.dirty.iter_mut().enumerate() {
                    while dirty.count_ones() != 0 {
                        let slot = dirty.trailing_zeros();
                        *dirty &= !(1 << slot as u64);

                        let metal_index_opt = pipeline.resources.get(&(gpu::ShaderType::ComputeShader, set_index as u32, slot));
                        if metal_index_opt.is_none() {
                            continue;
                        }
                        let metal_binding = metal_index_opt.unwrap();
                        match &self.bindings[set_index][slot as usize] {
                            MTLBoundResource::None => {
                                if let Some(binding) = metal_binding.texture_binding {
                                    encoder.set_texture(binding as u64, None);
                                }
                                if let Some(binding) = metal_binding.sampler_binding {
                                    encoder.set_sampler_state(binding as u64, None);
                                }
                                if let Some(binding) = metal_binding.buffer_binding {
                                    encoder.set_buffer(binding as u64, None, 0u64);
                                }
                            },
                            MTLBoundResource::SampledTexture(texture) => {
                                if metal_binding.texture_binding.is_none() {
                                    continue;
                                }
                                encoder.set_texture(metal_binding.texture_binding.unwrap() as u64, Some(texture));
                            },
                            MTLBoundResource::Sampler(sampler) => {
                                if metal_binding.sampler_binding.is_none() {
                                    continue;
                                }
                                encoder.set_sampler_state(metal_binding.sampler_binding.unwrap() as u64, Some(sampler));
                            },
                            MTLBoundResource::StorageTexture(texture) => {
                                if metal_binding.texture_binding.is_none() {
                                    continue;
                                }
                                encoder.set_texture(metal_binding.texture_binding.unwrap() as u64, Some(texture));
                            },
                            MTLBoundResource::SampledTextureAndSampler(texture, sampler) => {
                                if metal_binding.texture_binding.is_none() || metal_binding.sampler_binding.is_none() {
                                    continue;
                                }
                                encoder.set_texture(metal_binding.texture_binding.unwrap() as u64, Some(texture));
                                encoder.set_sampler_state(metal_binding.sampler_binding.unwrap() as u64, Some(sampler));
                            }
                            MTLBoundResource::SampledTextureArray(textures) => {
                                if metal_binding.texture_binding.is_none() {
                                    continue;
                                }
                                let mut handles_opt = SmallVec::<[Option<&metal::TextureRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                for array_entry in textures {
                                    handles_opt.push(Some(&array_entry));
                                }
                                handles_opt.resize(metal_binding.array_count as usize, None);
                                encoder.set_textures(metal_binding.texture_binding.unwrap() as u64, &handles_opt);
                            }
                            MTLBoundResource::StorageTextureArray(textures) => {
                                if metal_binding.texture_binding.is_none() {
                                    continue;
                                }
                                let mut handles_opt = SmallVec::<[Option<&metal::TextureRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                for array_entry in textures {
                                    handles_opt.push(Some(&array_entry));
                                }
                                handles_opt.resize(metal_binding.array_count as usize, None);
                                encoder.set_textures(metal_binding.texture_binding.unwrap() as u64, &handles_opt);
                            }
                            MTLBoundResource::SampledTextureAndSamplerArray(textures_and_samplers) => {
                                if metal_binding.texture_binding.is_none() || metal_binding.sampler_binding.is_none() {
                                    continue;
                                }
                                let mut texture_handles_opt = SmallVec::<[Option<&metal::TextureRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                let mut sampler_handles_opt = SmallVec::<[Option<&metal::SamplerStateRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                for (texture, sampler) in textures_and_samplers {
                                    texture_handles_opt.push(Some(&texture));
                                    sampler_handles_opt.push(Some(&sampler));
                                }
                                texture_handles_opt.resize(metal_binding.array_count as usize, None);
                                sampler_handles_opt.resize(metal_binding.array_count as usize, None);
                                encoder.set_textures(metal_binding.texture_binding.unwrap() as u64, &texture_handles_opt);
                                encoder.set_sampler_states(metal_binding.sampler_binding.unwrap() as u64, &sampler_handles_opt);
                            }
                            MTLBoundResource::UniformBuffer(buffer_info) => {
                                if metal_binding.buffer_binding.is_none() {
                                    continue;
                                }
                                encoder.set_buffer(metal_binding.buffer_binding.unwrap() as u64, Some(&buffer_info.buffer), buffer_info.offset);
                            }
                            MTLBoundResource::StorageBuffer(buffer_info) => {
                                if metal_binding.buffer_binding.is_none() {
                                    continue;
                                }
                                encoder.set_buffer(metal_binding.buffer_binding.unwrap() as u64, Some(&buffer_info.buffer), buffer_info.offset);
                            }
                            MTLBoundResource::AccelerationStructure(acceleration_structure) => {
                                if metal_binding.buffer_binding.is_none() {
                                    continue;
                                }
                                encoder.set_acceleration_structure(metal_binding.buffer_binding.unwrap() as u64, Some(&acceleration_structure));
                            }
                            MTLBoundResource::UniformBufferArray(buffers) => {
                                if metal_binding.buffer_binding.is_none() {
                                    continue;
                                }
                                let mut handles_opt = SmallVec::<[Option<&metal::BufferRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                let mut offsets = SmallVec::<[u64; 32]>::with_capacity(metal_binding.array_count as usize);
                                for array_entry in buffers {
                                    handles_opt.push(Some(&array_entry.buffer));
                                    offsets.push(array_entry.offset);
                                }
                                handles_opt.resize(metal_binding.array_count as usize, None);
                                offsets.resize(metal_binding.array_count as usize, 0u64);
                                encoder.set_buffers(metal_binding.texture_binding.unwrap() as u64, &handles_opt, &offsets);
                            },
                            MTLBoundResource::StorageBufferArray(buffers) => {
                                if metal_binding.buffer_binding.is_none() {
                                    continue;
                                }
                                let mut handles_opt = SmallVec::<[Option<&metal::BufferRef>; 32]>::with_capacity(metal_binding.array_count as usize);
                                let mut offsets = SmallVec::<[u64; 32]>::with_capacity(metal_binding.array_count as usize);
                                for array_entry in buffers {
                                    handles_opt.push(Some(&array_entry.buffer));
                                    offsets.push(array_entry.offset);
                                }
                                handles_opt.resize(metal_binding.array_count as usize, None);
                                offsets.resize(metal_binding.array_count as usize, 0u64);
                                encoder.set_buffers(metal_binding.texture_binding.unwrap() as u64, &handles_opt, &offsets);
                            },
                        }
                    }
                }
            },
        }
    }
}
