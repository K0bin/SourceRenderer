use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::{NSRange, NSUInteger};
use objc2_metal::{self, MTLComputeCommandEncoder as _, MTLRenderCommandEncoder};

use smallvec::SmallVec;
use sourcerenderer_core::gpu;

use super::*;

#[derive(Clone, Debug)]
pub(crate) struct MTLBufferBindingInfo {
    pub(crate) buffer: Retained<ProtocolObject<dyn objc2_metal::MTLBuffer>>,
    pub(crate) offset: u64,
    pub(crate) length: u64,
}

#[derive(Clone)]
pub(crate) struct MTLBufferBindingInfoRef<'a> {
    pub(crate) buffer: &'a ProtocolObject<dyn objc2_metal::MTLBuffer>,
    pub(crate) offset: u64,
    pub(crate) length: u64,
}

impl From<&MTLBufferBindingInfoRef<'_>> for MTLBufferBindingInfo {
    fn from(binding: &MTLBufferBindingInfoRef<'_>) -> Self {
        Self {
            buffer: Retained::from(binding.buffer),
            offset: binding.offset,
            length: binding.length,
        }
    }
}

impl PartialEq<MTLBufferBindingInfoRef<'_>> for MTLBufferBindingInfo {
    fn eq(&self, other: &MTLBufferBindingInfoRef) -> bool {
        self.buffer.as_ref() == other.buffer
            && self.offset == other.offset
            && self.length == other.length
    }
}

#[derive(Clone, Debug)]
pub(crate) enum MTLBoundResource {
    None,
    UniformBuffer(MTLBufferBindingInfo),
    UniformBufferArray(SmallVec<[MTLBufferBindingInfo; gpu::PER_SET_BINDINGS as usize]>),
    StorageBuffer(MTLBufferBindingInfo),
    StorageBufferArray(SmallVec<[MTLBufferBindingInfo; gpu::PER_SET_BINDINGS as usize]>),
    StorageTexture(Retained<ProtocolObject<dyn objc2_metal::MTLTexture>>),
    StorageTextureArray(
        SmallVec<
            [Retained<ProtocolObject<dyn objc2_metal::MTLTexture>>; gpu::PER_SET_BINDINGS as usize],
        >,
    ),
    SampledTexture(Retained<ProtocolObject<dyn objc2_metal::MTLTexture>>),
    SampledTextureArray(
        SmallVec<
            [Retained<ProtocolObject<dyn objc2_metal::MTLTexture>>; gpu::PER_SET_BINDINGS as usize],
        >,
    ),
    SampledTextureAndSampler(
        Retained<ProtocolObject<dyn objc2_metal::MTLTexture>>,
        Retained<ProtocolObject<dyn objc2_metal::MTLSamplerState>>,
    ),
    SampledTextureAndSamplerArray(
        SmallVec<
            [(
                Retained<ProtocolObject<dyn objc2_metal::MTLTexture>>,
                Retained<ProtocolObject<dyn objc2_metal::MTLSamplerState>>,
            ); gpu::PER_SET_BINDINGS as usize],
        >,
    ),
    Sampler(Retained<ProtocolObject<dyn objc2_metal::MTLSamplerState>>),
    AccelerationStructure(Retained<ProtocolObject<dyn objc2_metal::MTLAccelerationStructure>>),
}

impl Default for MTLBoundResource {
    fn default() -> Self {
        Self::None
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) enum MTLBoundResourceRef<'a> {
    None,
    UniformBuffer(MTLBufferBindingInfoRef<'a>),
    UniformBufferArray(&'a [MTLBufferBindingInfoRef<'a>]),
    StorageBuffer(MTLBufferBindingInfoRef<'a>),
    StorageBufferArray(&'a [MTLBufferBindingInfoRef<'a>]),
    StorageTexture(&'a ProtocolObject<dyn objc2_metal::MTLTexture>),
    StorageTextureArray(&'a [&'a ProtocolObject<dyn objc2_metal::MTLTexture>]),
    SampledTexture(&'a ProtocolObject<dyn objc2_metal::MTLTexture>),
    SampledTextureArray(&'a [&'a ProtocolObject<dyn objc2_metal::MTLTexture>]),
    SampledTextureAndSampler(
        &'a ProtocolObject<dyn objc2_metal::MTLTexture>,
        &'a ProtocolObject<dyn objc2_metal::MTLSamplerState>,
    ),
    SampledTextureAndSamplerArray(
        &'a [(
            &'a ProtocolObject<dyn objc2_metal::MTLTexture>,
            &'a ProtocolObject<dyn objc2_metal::MTLSamplerState>,
        )],
    ),
    Sampler(&'a ProtocolObject<dyn objc2_metal::MTLSamplerState>),
    AccelerationStructure(&'a ProtocolObject<dyn objc2_metal::MTLAccelerationStructure>),
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
            MTLBoundResourceRef::UniformBuffer(info) => {
                MTLBoundResource::UniformBuffer(info.into())
            }
            MTLBoundResourceRef::StorageBuffer(info) => {
                MTLBoundResource::StorageBuffer(info.into())
            }
            MTLBoundResourceRef::StorageTexture(view) => {
                MTLBoundResource::StorageTexture(Retained::from(*view))
            }
            MTLBoundResourceRef::SampledTexture(view) => {
                MTLBoundResource::SampledTexture(Retained::from(*view))
            }
            MTLBoundResourceRef::SampledTextureAndSampler(view, sampler) => {
                MTLBoundResource::SampledTextureAndSampler(
                    Retained::from(*view),
                    Retained::from(*sampler),
                )
            }
            MTLBoundResourceRef::Sampler(sampler) => {
                MTLBoundResource::Sampler(Retained::from(*sampler))
            }
            MTLBoundResourceRef::AccelerationStructure(accel) => {
                MTLBoundResource::AccelerationStructure(Retained::from(*accel))
            }
            MTLBoundResourceRef::UniformBufferArray(arr) => {
                MTLBoundResource::UniformBufferArray(arr.iter().map(|a| a.into()).collect())
            }
            MTLBoundResourceRef::StorageBufferArray(arr) => {
                MTLBoundResource::StorageBufferArray(arr.iter().map(|a| a.into()).collect())
            }
            MTLBoundResourceRef::StorageTextureArray(arr) => MTLBoundResource::StorageTextureArray(
                arr.iter().map(|a| Retained::from(*a)).collect(),
            ),
            MTLBoundResourceRef::SampledTextureArray(arr) => MTLBoundResource::SampledTextureArray(
                arr.iter().map(|a| Retained::from(*a)).collect(),
            ),
            MTLBoundResourceRef::SampledTextureAndSamplerArray(arr) => {
                MTLBoundResource::SampledTextureAndSamplerArray(
                    arr.iter()
                        .map(|(t, s)| {
                            let tuple: (
                                Retained<ProtocolObject<dyn objc2_metal::MTLTexture>>,
                                Retained<ProtocolObject<dyn objc2_metal::MTLSamplerState>>,
                            ) = (Retained::from(*t), Retained::from(*s));
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
            ) => old.as_ref() == *new && old_offset == new_offset && old_length == new_length,
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
            ) => old.as_ref() == *new && old_offset == new_offset && old_length == new_length,
            (MTLBoundResource::StorageTexture(old), MTLBoundResourceRef::StorageTexture(new)) => {
                old.as_ref() == *new
            }
            (MTLBoundResource::SampledTexture(old), MTLBoundResourceRef::SampledTexture(new)) => {
                old.as_ref() == *new
            }
            (
                MTLBoundResource::SampledTextureAndSampler(old_tex, old_sampler),
                MTLBoundResourceRef::SampledTextureAndSampler(new_tex, new_sampler),
            ) => old_tex.as_ref() == *new_tex && old_sampler.as_ref() == *new_sampler,
            (MTLBoundResource::Sampler(old_sampler), MTLBoundResourceRef::Sampler(new_sampler)) => {
                old_sampler.as_ref() == *new_sampler
            }
            (
                MTLBoundResource::AccelerationStructure(old),
                MTLBoundResourceRef::AccelerationStructure(new),
            ) => old.as_ref() == *new,
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
            ) => old
                .iter()
                .zip(new.iter())
                .all(|(old, new)| old.as_ref() == *new),
            (
                MTLBoundResource::StorageTextureArray(old),
                MTLBoundResourceRef::StorageTextureArray(new),
            ) => old
                .iter()
                .zip(new.iter())
                .all(|(old, new)| old.as_ref() == *new),
            (
                MTLBoundResource::SampledTextureAndSamplerArray(old),
                MTLBoundResourceRef::SampledTextureAndSamplerArray(new),
            ) => old.iter().zip(new.iter()).all(
                |((old_texture, old_sampler), (new_texture, new_sampler))| {
                    old_texture.as_ref() == *new_texture && old_sampler.as_ref() == *new_sampler
                },
            ),
            _ => false,
        }
    }
}

pub(crate) enum MTLEncoderRef<'a> {
    Graphics(&'a ProtocolObject<dyn objc2_metal::MTLRenderCommandEncoder>),
    Compute(&'a ProtocolObject<dyn objc2_metal::MTLComputeCommandEncoder>),
}

pub(crate) struct MTLBindingManager {
    bindings: [[MTLBoundResource; gpu::PER_SET_BINDINGS as usize]; 4],
    dirty: [u64; 4],
}

impl MTLBindingManager {
    pub(crate) fn new() -> Self {
        Self {
            bindings: Default::default(),
            dirty: Default::default(),
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

    pub(crate) fn mark_all_dirty(&mut self) {
        for i in 0..=gpu::BindingFrequency::Frame as u32 {
            self.dirty[i as usize] = !0u64;
        }
    }

    pub(crate) unsafe fn finish(&mut self, encoder: MTLEncoderRef, pipeline: &PipelineResourceMap) {
        match encoder {
            MTLEncoderRef::Graphics(encoder) => {
                for (set_index, dirty) in &mut self.dirty.iter_mut().enumerate() {
                    while dirty.count_ones() != 0 {
                        let slot = dirty.trailing_zeros();
                        *dirty &= !(1 << slot as NSUInteger);

                        let metal_index_opt = pipeline.resources.get(&(
                            gpu::ShaderType::VertexShader,
                            set_index as u32,
                            slot,
                        ));
                        if let Some(metal_binding) = metal_index_opt {
                            match &self.bindings[set_index][slot as usize] {
                                MTLBoundResource::None => {
                                    if let Some(binding) = metal_binding.texture_binding {
                                        encoder
                                            .setVertexTexture_atIndex(None, binding as NSUInteger);
                                    }
                                    if let Some(binding) = metal_binding.sampler_binding {
                                        encoder.setVertexSamplerState_atIndex(
                                            None,
                                            binding as NSUInteger,
                                        );
                                    }
                                    if let Some(binding) = metal_binding.buffer_binding {
                                        encoder.setVertexBuffer_offset_atIndex(
                                            None,
                                            0,
                                            binding as NSUInteger,
                                        );
                                    }
                                }
                                MTLBoundResource::SampledTexture(texture) => {
                                    if metal_binding.texture_binding.is_none() {
                                        continue;
                                    }
                                    encoder.setVertexTexture_atIndex(
                                        Some(texture),
                                        metal_binding.texture_binding.unwrap() as NSUInteger,
                                    );
                                }
                                MTLBoundResource::Sampler(sampler) => {
                                    if metal_binding.sampler_binding.is_none() {
                                        continue;
                                    }
                                    encoder.setVertexSamplerState_atIndex(
                                        Some(sampler),
                                        metal_binding.sampler_binding.unwrap() as NSUInteger,
                                    );
                                }
                                MTLBoundResource::StorageTexture(texture) => {
                                    if metal_binding.texture_binding.is_none() {
                                        continue;
                                    }
                                    encoder.setVertexTexture_atIndex(
                                        Some(texture),
                                        metal_binding.texture_binding.unwrap() as NSUInteger,
                                    );
                                }
                                MTLBoundResource::SampledTextureAndSampler(texture, sampler) => {
                                    if metal_binding.texture_binding.is_none()
                                        || metal_binding.sampler_binding.is_none()
                                    {
                                        continue;
                                    }
                                    encoder.setVertexTexture_atIndex(
                                        Some(texture),
                                        metal_binding.texture_binding.unwrap() as NSUInteger,
                                    );
                                    encoder.setVertexSamplerState_atIndex(
                                        Some(sampler),
                                        metal_binding.sampler_binding.unwrap() as NSUInteger,
                                    );
                                }
                                MTLBoundResource::SampledTextureArray(textures) => {
                                    if metal_binding.texture_binding.is_none() {
                                        continue;
                                    }
                                    let start_binding =
                                        metal_binding.texture_binding.unwrap() as NSUInteger;
                                    let mut handles_opt = SmallVec::<
                                        [*const ProtocolObject<dyn objc2_metal::MTLTexture>; 32],
                                    >::with_capacity(
                                        metal_binding.array_count as usize
                                    );
                                    for array_entry in textures {
                                        handles_opt.push(array_entry.as_ref()
                                            as *const ProtocolObject<dyn objc2_metal::MTLTexture>);
                                    }
                                    encoder.setVertexTextures_withRange(
                                        NonNull::new(handles_opt.as_mut_ptr()).unwrap(),
                                        NSRange {
                                            location: start_binding,
                                            length: handles_opt.len(),
                                        },
                                    );
                                    for i in (start_binding + handles_opt.len())
                                        ..(start_binding as NSUInteger
                                            + metal_binding.array_count as NSUInteger)
                                    {
                                        encoder.setVertexTexture_atIndex(None, i);
                                    }
                                }
                                MTLBoundResource::StorageTextureArray(textures) => {
                                    if metal_binding.texture_binding.is_none() {
                                        continue;
                                    }
                                    let start_binding =
                                        metal_binding.texture_binding.unwrap() as NSUInteger;
                                    let mut handles_opt = SmallVec::<
                                        [*const ProtocolObject<dyn objc2_metal::MTLTexture>; 32],
                                    >::with_capacity(
                                        metal_binding.array_count as usize
                                    );
                                    for array_entry in textures {
                                        handles_opt.push(array_entry.as_ref()
                                            as *const ProtocolObject<dyn objc2_metal::MTLTexture>);
                                    }
                                    encoder.setVertexTextures_withRange(
                                        NonNull::new(handles_opt.as_mut_ptr()).unwrap(),
                                        NSRange {
                                            location: start_binding,
                                            length: handles_opt.len(),
                                        },
                                    );
                                    for i in (start_binding + handles_opt.len())
                                        ..(start_binding as NSUInteger
                                            + metal_binding.array_count as NSUInteger)
                                    {
                                        encoder.setVertexTexture_atIndex(None, i);
                                    }
                                }
                                MTLBoundResource::SampledTextureAndSamplerArray(
                                    textures_and_samplers,
                                ) => {
                                    if metal_binding.texture_binding.is_none()
                                        || metal_binding.sampler_binding.is_none()
                                    {
                                        continue;
                                    }
                                    let mut texture_handles_opt = SmallVec::<
                                        [*const ProtocolObject<dyn objc2_metal::MTLTexture>; 32],
                                    >::with_capacity(
                                        metal_binding.array_count as usize,
                                    );
                                    let mut sampler_handles_opt = SmallVec::<
                                        [*const ProtocolObject<dyn objc2_metal::MTLSamplerState>;
                                            32],
                                    >::with_capacity(
                                        metal_binding.array_count as usize,
                                    );
                                    for (texture, sampler) in textures_and_samplers {
                                        texture_handles_opt.push(texture.as_ref()
                                            as *const ProtocolObject<dyn objc2_metal::MTLTexture>);
                                        sampler_handles_opt.push(sampler.as_ref()
                                            as *const ProtocolObject<
                                                dyn objc2_metal::MTLSamplerState,
                                            >);
                                    }
                                    encoder.setVertexTextures_withRange(
                                        NonNull::new(texture_handles_opt.as_mut_ptr()).unwrap(),
                                        NSRange {
                                            location: metal_binding.texture_binding.unwrap()
                                                as NSUInteger,
                                            length: texture_handles_opt.len(),
                                        },
                                    );
                                    encoder.setVertexSamplerStates_withRange(
                                        NonNull::new(sampler_handles_opt.as_mut_ptr()).unwrap(),
                                        NSRange {
                                            location: metal_binding.sampler_binding.unwrap()
                                                as NSUInteger,
                                            length: sampler_handles_opt.len(),
                                        },
                                    );
                                    for i in 0..(metal_binding.array_count as NSUInteger) {
                                        encoder.setVertexTexture_atIndex(
                                            None,
                                            metal_binding.texture_binding.unwrap() as NSUInteger
                                                + i,
                                        );
                                        encoder.setVertexSamplerState_atIndex(
                                            None,
                                            metal_binding.sampler_binding.unwrap() as NSUInteger
                                                + i,
                                        );
                                    }
                                }
                                MTLBoundResource::UniformBuffer(buffer_info) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    encoder.setVertexBuffer_offset_atIndex(
                                        Some(&buffer_info.buffer),
                                        buffer_info.offset as NSUInteger,
                                        metal_binding.buffer_binding.unwrap() as NSUInteger,
                                    );
                                }
                                MTLBoundResource::StorageBuffer(buffer_info) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    encoder.setVertexBuffer_offset_atIndex(
                                        Some(&buffer_info.buffer),
                                        buffer_info.offset as NSUInteger,
                                        metal_binding.buffer_binding.unwrap() as NSUInteger,
                                    );
                                }
                                MTLBoundResource::AccelerationStructure(acceleration_structure) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    encoder.setVertexAccelerationStructure_atBufferIndex(
                                        Some(&acceleration_structure),
                                        metal_binding.buffer_binding.unwrap() as NSUInteger,
                                    );
                                }
                                MTLBoundResource::UniformBufferArray(buffers) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    let start_binding =
                                        metal_binding.buffer_binding.unwrap() as NSUInteger;
                                    let mut handles_opt = SmallVec::<
                                        [*const ProtocolObject<dyn objc2_metal::MTLBuffer>; 32],
                                    >::with_capacity(
                                        metal_binding.array_count as usize
                                    );
                                    let mut offsets = SmallVec::<[NSUInteger; 32]>::with_capacity(
                                        metal_binding.array_count as usize,
                                    );
                                    for array_entry in buffers {
                                        handles_opt.push(array_entry.buffer.as_ref()
                                            as *const ProtocolObject<dyn objc2_metal::MTLBuffer>);
                                        offsets.push(array_entry.offset as NSUInteger);
                                    }
                                    encoder.setVertexBuffers_offsets_withRange(
                                        NonNull::new(handles_opt.as_mut_ptr()).unwrap(),
                                        NonNull::new(offsets.as_mut_ptr()).unwrap(),
                                        NSRange {
                                            location: start_binding,
                                            length: handles_opt.len(),
                                        },
                                    );
                                }
                                MTLBoundResource::StorageBufferArray(buffers) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    let start_binding =
                                        metal_binding.buffer_binding.unwrap() as NSUInteger;
                                    let mut handles_opt = SmallVec::<
                                        [*const ProtocolObject<dyn objc2_metal::MTLBuffer>; 32],
                                    >::with_capacity(
                                        metal_binding.array_count as usize
                                    );
                                    let mut offsets = SmallVec::<[NSUInteger; 32]>::with_capacity(
                                        metal_binding.array_count as usize,
                                    );
                                    for array_entry in buffers {
                                        handles_opt.push(array_entry.buffer.as_ref()
                                            as *const ProtocolObject<dyn objc2_metal::MTLBuffer>);
                                        offsets.push(array_entry.offset as NSUInteger);
                                    }
                                    encoder.setVertexBuffers_offsets_withRange(
                                        NonNull::new(handles_opt.as_mut_ptr()).unwrap(),
                                        NonNull::new(offsets.as_mut_ptr()).unwrap(),
                                        NSRange {
                                            location: start_binding,
                                            length: handles_opt.len(),
                                        },
                                    );
                                }
                            }
                        }
                        let metal_index_opt = pipeline.resources.get(&(
                            gpu::ShaderType::FragmentShader,
                            set_index as u32,
                            slot,
                        ));
                        if let Some(metal_binding) = metal_index_opt {
                            match &self.bindings[set_index][slot as usize] {
                                MTLBoundResource::None => {
                                    if let Some(binding) = metal_binding.texture_binding {
                                        encoder.setFragmentTexture_atIndex(
                                            None,
                                            binding as NSUInteger,
                                        );
                                    }
                                    if let Some(binding) = metal_binding.sampler_binding {
                                        encoder.setFragmentSamplerState_atIndex(
                                            None,
                                            binding as NSUInteger,
                                        );
                                    }
                                    if let Some(binding) = metal_binding.buffer_binding {
                                        encoder.setFragmentBuffer_offset_atIndex(
                                            None,
                                            0,
                                            binding as NSUInteger,
                                        );
                                    }
                                }
                                MTLBoundResource::SampledTexture(texture) => {
                                    if metal_binding.texture_binding.is_none() {
                                        continue;
                                    }
                                    encoder.setFragmentTexture_atIndex(
                                        Some(texture),
                                        metal_binding.texture_binding.unwrap() as NSUInteger,
                                    );
                                }
                                MTLBoundResource::Sampler(sampler) => {
                                    if metal_binding.sampler_binding.is_none() {
                                        continue;
                                    }
                                    encoder.setFragmentSamplerState_atIndex(
                                        Some(sampler),
                                        metal_binding.sampler_binding.unwrap() as NSUInteger,
                                    );
                                }
                                MTLBoundResource::StorageTexture(texture) => {
                                    if metal_binding.texture_binding.is_none() {
                                        continue;
                                    }
                                    encoder.setFragmentTexture_atIndex(
                                        Some(texture),
                                        metal_binding.texture_binding.unwrap() as NSUInteger,
                                    );
                                }
                                MTLBoundResource::SampledTextureAndSampler(texture, sampler) => {
                                    if metal_binding.texture_binding.is_none()
                                        || metal_binding.sampler_binding.is_none()
                                    {
                                        continue;
                                    }
                                    encoder.setFragmentTexture_atIndex(
                                        Some(texture),
                                        metal_binding.texture_binding.unwrap() as NSUInteger,
                                    );
                                    encoder.setFragmentSamplerState_atIndex(
                                        Some(sampler),
                                        metal_binding.sampler_binding.unwrap() as NSUInteger,
                                    );
                                }
                                MTLBoundResource::SampledTextureArray(textures) => {
                                    if metal_binding.texture_binding.is_none() {
                                        continue;
                                    }
                                    let start_binding =
                                        metal_binding.texture_binding.unwrap() as NSUInteger;
                                    let mut handles_opt = SmallVec::<
                                        [*const ProtocolObject<dyn objc2_metal::MTLTexture>; 32],
                                    >::with_capacity(
                                        metal_binding.array_count as usize
                                    );
                                    for array_entry in textures {
                                        handles_opt.push(array_entry.as_ref()
                                            as *const ProtocolObject<dyn objc2_metal::MTLTexture>);
                                    }
                                    encoder.setFragmentTextures_withRange(
                                        NonNull::new(handles_opt.as_mut_ptr()).unwrap(),
                                        NSRange {
                                            location: start_binding,
                                            length: handles_opt.len(),
                                        },
                                    );
                                    for i in (start_binding + handles_opt.len())
                                        ..(start_binding as NSUInteger
                                            + metal_binding.array_count as NSUInteger)
                                    {
                                        encoder.setFragmentTexture_atIndex(None, i);
                                    }
                                }
                                MTLBoundResource::StorageTextureArray(textures) => {
                                    if metal_binding.texture_binding.is_none() {
                                        continue;
                                    }
                                    let start_binding =
                                        metal_binding.texture_binding.unwrap() as NSUInteger;
                                    let mut handles_opt = SmallVec::<
                                        [*const ProtocolObject<dyn objc2_metal::MTLTexture>; 32],
                                    >::with_capacity(
                                        metal_binding.array_count as usize
                                    );
                                    for array_entry in textures {
                                        handles_opt.push(array_entry.as_ref()
                                            as *const ProtocolObject<dyn objc2_metal::MTLTexture>);
                                    }
                                    encoder.setFragmentTextures_withRange(
                                        NonNull::new(handles_opt.as_mut_ptr()).unwrap(),
                                        NSRange {
                                            location: start_binding,
                                            length: handles_opt.len(),
                                        },
                                    );
                                    for i in (start_binding + handles_opt.len())
                                        ..(start_binding as NSUInteger
                                            + metal_binding.array_count as NSUInteger)
                                    {
                                        encoder.setFragmentTexture_atIndex(None, i);
                                    }
                                }
                                MTLBoundResource::SampledTextureAndSamplerArray(
                                    textures_and_samplers,
                                ) => {
                                    if metal_binding.texture_binding.is_none()
                                        || metal_binding.sampler_binding.is_none()
                                    {
                                        continue;
                                    }
                                    let mut texture_handles_opt = SmallVec::<
                                        [*const ProtocolObject<dyn objc2_metal::MTLTexture>; 32],
                                    >::with_capacity(
                                        metal_binding.array_count as usize,
                                    );
                                    let mut sampler_handles_opt = SmallVec::<
                                        [*const ProtocolObject<dyn objc2_metal::MTLSamplerState>;
                                            32],
                                    >::with_capacity(
                                        metal_binding.array_count as usize,
                                    );
                                    for (texture, sampler) in textures_and_samplers {
                                        texture_handles_opt.push(texture.as_ref()
                                            as *const ProtocolObject<dyn objc2_metal::MTLTexture>);
                                        sampler_handles_opt.push(sampler.as_ref()
                                            as *const ProtocolObject<
                                                dyn objc2_metal::MTLSamplerState,
                                            >);
                                    }
                                    encoder.setFragmentTextures_withRange(
                                        NonNull::new(texture_handles_opt.as_mut_ptr()).unwrap(),
                                        NSRange {
                                            location: metal_binding.texture_binding.unwrap()
                                                as NSUInteger,
                                            length: texture_handles_opt.len(),
                                        },
                                    );
                                    encoder.setFragmentSamplerStates_withRange(
                                        NonNull::new(sampler_handles_opt.as_mut_ptr()).unwrap(),
                                        NSRange {
                                            location: metal_binding.sampler_binding.unwrap()
                                                as NSUInteger,
                                            length: sampler_handles_opt.len(),
                                        },
                                    );
                                    for i in 0..(metal_binding.array_count as NSUInteger) {
                                        encoder.setFragmentTexture_atIndex(
                                            None,
                                            metal_binding.texture_binding.unwrap() as NSUInteger
                                                + i,
                                        );
                                        encoder.setFragmentSamplerState_atIndex(
                                            None,
                                            metal_binding.sampler_binding.unwrap() as NSUInteger
                                                + i,
                                        );
                                    }
                                }
                                MTLBoundResource::UniformBuffer(buffer_info) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    encoder.setFragmentBuffer_offset_atIndex(
                                        Some(&buffer_info.buffer),
                                        buffer_info.offset as NSUInteger,
                                        metal_binding.buffer_binding.unwrap() as NSUInteger,
                                    );
                                }
                                MTLBoundResource::StorageBuffer(buffer_info) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    encoder.setFragmentBuffer_offset_atIndex(
                                        Some(&buffer_info.buffer),
                                        buffer_info.offset as NSUInteger,
                                        metal_binding.buffer_binding.unwrap() as NSUInteger,
                                    );
                                }
                                MTLBoundResource::AccelerationStructure(acceleration_structure) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    encoder.setFragmentAccelerationStructure_atBufferIndex(
                                        Some(&acceleration_structure),
                                        metal_binding.buffer_binding.unwrap() as NSUInteger,
                                    );
                                }
                                MTLBoundResource::UniformBufferArray(buffers) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    let start_binding =
                                        metal_binding.buffer_binding.unwrap() as NSUInteger;
                                    let mut handles_opt = SmallVec::<
                                        [*const ProtocolObject<dyn objc2_metal::MTLBuffer>; 32],
                                    >::with_capacity(
                                        metal_binding.array_count as usize
                                    );
                                    let mut offsets = SmallVec::<[NSUInteger; 32]>::with_capacity(
                                        metal_binding.array_count as usize,
                                    );
                                    for array_entry in buffers {
                                        handles_opt.push(array_entry.buffer.as_ref()
                                            as *const ProtocolObject<dyn objc2_metal::MTLBuffer>);
                                        offsets.push(array_entry.offset as NSUInteger);
                                    }
                                    encoder.setFragmentBuffers_offsets_withRange(
                                        NonNull::new(handles_opt.as_mut_ptr()).unwrap(),
                                        NonNull::new(offsets.as_mut_ptr()).unwrap(),
                                        NSRange {
                                            location: start_binding,
                                            length: handles_opt.len(),
                                        },
                                    );
                                }
                                MTLBoundResource::StorageBufferArray(buffers) => {
                                    if metal_binding.buffer_binding.is_none() {
                                        continue;
                                    }
                                    let start_binding =
                                        metal_binding.buffer_binding.unwrap() as NSUInteger;
                                    let mut handles_opt = SmallVec::<
                                        [*const ProtocolObject<dyn objc2_metal::MTLBuffer>; 32],
                                    >::with_capacity(
                                        metal_binding.array_count as usize
                                    );
                                    let mut offsets = SmallVec::<[NSUInteger; 32]>::with_capacity(
                                        metal_binding.array_count as usize,
                                    );
                                    for array_entry in buffers {
                                        handles_opt.push(array_entry.buffer.as_ref()
                                            as *const ProtocolObject<dyn objc2_metal::MTLBuffer>);
                                        offsets.push(array_entry.offset as NSUInteger);
                                    }
                                    encoder.setFragmentBuffers_offsets_withRange(
                                        NonNull::new(handles_opt.as_mut_ptr()).unwrap(),
                                        NonNull::new(offsets.as_mut_ptr()).unwrap(),
                                        NSRange {
                                            location: start_binding,
                                            length: handles_opt.len(),
                                        },
                                    );
                                }
                            }
                        }
                    }

                    *dirty = 0;
                }
            }
            MTLEncoderRef::Compute(encoder) => {
                for (set_index, dirty) in &mut self.dirty.iter_mut().enumerate() {
                    while dirty.count_ones() != 0 {
                        let slot = dirty.trailing_zeros();
                        *dirty &= !(1 << slot as NSUInteger);

                        let metal_index_opt = pipeline.resources.get(&(
                            gpu::ShaderType::ComputeShader,
                            set_index as u32,
                            slot,
                        ));
                        if metal_index_opt.is_none() {
                            continue;
                        }
                        let metal_binding = metal_index_opt.unwrap();
                        match &self.bindings[set_index][slot as usize] {
                            MTLBoundResource::None => {
                                if let Some(binding) = metal_binding.texture_binding {
                                    encoder.setTexture_atIndex(None, binding as NSUInteger);
                                }
                                if let Some(binding) = metal_binding.sampler_binding {
                                    encoder.setSamplerState_atIndex(None, binding as NSUInteger);
                                }
                                if let Some(binding) = metal_binding.buffer_binding {
                                    encoder.setBuffer_offset_atIndex(
                                        None,
                                        0,
                                        binding as NSUInteger,
                                    );
                                }
                            }
                            MTLBoundResource::SampledTexture(texture) => {
                                if metal_binding.texture_binding.is_none() {
                                    continue;
                                }
                                encoder.setTexture_atIndex(
                                    Some(texture),
                                    metal_binding.texture_binding.unwrap() as NSUInteger,
                                );
                            }
                            MTLBoundResource::Sampler(sampler) => {
                                if metal_binding.sampler_binding.is_none() {
                                    continue;
                                }
                                encoder.setSamplerState_atIndex(
                                    Some(sampler),
                                    metal_binding.sampler_binding.unwrap() as NSUInteger,
                                );
                            }
                            MTLBoundResource::StorageTexture(texture) => {
                                if metal_binding.texture_binding.is_none() {
                                    continue;
                                }
                                encoder.setTexture_atIndex(
                                    Some(texture),
                                    metal_binding.texture_binding.unwrap() as NSUInteger,
                                );
                            }
                            MTLBoundResource::SampledTextureAndSampler(texture, sampler) => {
                                if metal_binding.texture_binding.is_none()
                                    || metal_binding.sampler_binding.is_none()
                                {
                                    continue;
                                }
                                encoder.setTexture_atIndex(
                                    Some(texture),
                                    metal_binding.texture_binding.unwrap() as NSUInteger,
                                );
                                encoder.setSamplerState_atIndex(
                                    Some(sampler),
                                    metal_binding.sampler_binding.unwrap() as NSUInteger,
                                );
                            }
                            MTLBoundResource::SampledTextureArray(textures) => {
                                if metal_binding.texture_binding.is_none() {
                                    continue;
                                }
                                let start_binding =
                                    metal_binding.texture_binding.unwrap() as NSUInteger;
                                let mut handles_opt = SmallVec::<
                                    [*const ProtocolObject<dyn objc2_metal::MTLTexture>; 32],
                                >::with_capacity(
                                    metal_binding.array_count as usize
                                );
                                for array_entry in textures {
                                    handles_opt.push(array_entry.as_ref()
                                        as *const ProtocolObject<dyn objc2_metal::MTLTexture>);
                                }
                                encoder.setTextures_withRange(
                                    NonNull::new(handles_opt.as_mut_ptr()).unwrap(),
                                    NSRange {
                                        location: start_binding,
                                        length: handles_opt.len(),
                                    },
                                );
                                for i in (start_binding + handles_opt.len())
                                    ..(start_binding as NSUInteger
                                        + metal_binding.array_count as NSUInteger)
                                {
                                    encoder.setTexture_atIndex(None, i);
                                }
                            }
                            MTLBoundResource::StorageTextureArray(textures) => {
                                if metal_binding.texture_binding.is_none() {
                                    continue;
                                }
                                let start_binding =
                                    metal_binding.texture_binding.unwrap() as NSUInteger;
                                let mut handles_opt = SmallVec::<
                                    [*const ProtocolObject<dyn objc2_metal::MTLTexture>; 32],
                                >::with_capacity(
                                    metal_binding.array_count as usize
                                );
                                for array_entry in textures {
                                    handles_opt.push(array_entry.as_ref()
                                        as *const ProtocolObject<dyn objc2_metal::MTLTexture>);
                                }
                                encoder.setTextures_withRange(
                                    NonNull::new(handles_opt.as_mut_ptr()).unwrap(),
                                    NSRange {
                                        location: start_binding,
                                        length: handles_opt.len(),
                                    },
                                );
                                for i in (start_binding + handles_opt.len())
                                    ..(start_binding as NSUInteger
                                        + metal_binding.array_count as NSUInteger)
                                {
                                    encoder.setTexture_atIndex(None, i);
                                }
                            }
                            MTLBoundResource::SampledTextureAndSamplerArray(
                                textures_and_samplers,
                            ) => {
                                if metal_binding.texture_binding.is_none()
                                    || metal_binding.sampler_binding.is_none()
                                {
                                    continue;
                                }
                                let mut texture_handles_opt = SmallVec::<
                                    [*const ProtocolObject<dyn objc2_metal::MTLTexture>; 32],
                                >::with_capacity(
                                    metal_binding.array_count as usize
                                );
                                let mut sampler_handles_opt = SmallVec::<
                                    [*const ProtocolObject<dyn objc2_metal::MTLSamplerState>; 32],
                                >::with_capacity(
                                    metal_binding.array_count as usize
                                );
                                for (texture, sampler) in textures_and_samplers {
                                    texture_handles_opt.push(texture.as_ref()
                                        as *const ProtocolObject<dyn objc2_metal::MTLTexture>);
                                    sampler_handles_opt.push(sampler.as_ref()
                                        as *const ProtocolObject<dyn objc2_metal::MTLSamplerState>);
                                }
                                encoder.setTextures_withRange(
                                    NonNull::new(texture_handles_opt.as_mut_ptr()).unwrap(),
                                    NSRange {
                                        location: metal_binding.texture_binding.unwrap()
                                            as NSUInteger,
                                        length: texture_handles_opt.len(),
                                    },
                                );
                                encoder.setSamplerStates_withRange(
                                    NonNull::new(sampler_handles_opt.as_mut_ptr()).unwrap(),
                                    NSRange {
                                        location: metal_binding.sampler_binding.unwrap()
                                            as NSUInteger,
                                        length: sampler_handles_opt.len(),
                                    },
                                );
                                for i in 0..(metal_binding.array_count as NSUInteger) {
                                    encoder.setTexture_atIndex(
                                        None,
                                        metal_binding.texture_binding.unwrap() as NSUInteger + i,
                                    );
                                    encoder.setSamplerState_atIndex(
                                        None,
                                        metal_binding.sampler_binding.unwrap() as NSUInteger + i,
                                    );
                                }
                            }
                            MTLBoundResource::UniformBuffer(buffer_info) => {
                                if metal_binding.buffer_binding.is_none() {
                                    continue;
                                }
                                encoder.setBuffer_offset_atIndex(
                                    Some(&buffer_info.buffer),
                                    buffer_info.offset as NSUInteger,
                                    metal_binding.buffer_binding.unwrap() as NSUInteger,
                                );
                            }
                            MTLBoundResource::StorageBuffer(buffer_info) => {
                                if metal_binding.buffer_binding.is_none() {
                                    continue;
                                }
                                encoder.setBuffer_offset_atIndex(
                                    Some(&buffer_info.buffer),
                                    buffer_info.offset as NSUInteger,
                                    metal_binding.buffer_binding.unwrap() as NSUInteger,
                                );
                            }
                            MTLBoundResource::AccelerationStructure(acceleration_structure) => {
                                if metal_binding.buffer_binding.is_none() {
                                    continue;
                                }
                                encoder.setAccelerationStructure_atBufferIndex(
                                    Some(&acceleration_structure),
                                    metal_binding.buffer_binding.unwrap() as NSUInteger,
                                );
                            }
                            MTLBoundResource::UniformBufferArray(buffers) => {
                                if metal_binding.buffer_binding.is_none() {
                                    continue;
                                }
                                let start_binding =
                                    metal_binding.buffer_binding.unwrap() as NSUInteger;
                                let mut handles_opt = SmallVec::<
                                    [*const ProtocolObject<dyn objc2_metal::MTLBuffer>; 32],
                                >::with_capacity(
                                    metal_binding.array_count as usize
                                );
                                let mut offsets = SmallVec::<[NSUInteger; 32]>::with_capacity(
                                    metal_binding.array_count as usize,
                                );
                                for array_entry in buffers {
                                    handles_opt.push(array_entry.buffer.as_ref()
                                        as *const ProtocolObject<dyn objc2_metal::MTLBuffer>);
                                    offsets.push(array_entry.offset as NSUInteger);
                                }
                                encoder.setBuffers_offsets_withRange(
                                    NonNull::new(handles_opt.as_mut_ptr()).unwrap(),
                                    NonNull::new(offsets.as_mut_ptr()).unwrap(),
                                    NSRange {
                                        location: start_binding,
                                        length: handles_opt.len(),
                                    },
                                );
                            }
                            MTLBoundResource::StorageBufferArray(buffers) => {
                                if metal_binding.buffer_binding.is_none() {
                                    continue;
                                }
                                let start_binding =
                                    metal_binding.buffer_binding.unwrap() as NSUInteger;
                                let mut handles_opt = SmallVec::<
                                    [*const ProtocolObject<dyn objc2_metal::MTLBuffer>; 32],
                                >::with_capacity(
                                    metal_binding.array_count as usize
                                );
                                let mut offsets = SmallVec::<[NSUInteger; 32]>::with_capacity(
                                    metal_binding.array_count as usize,
                                );
                                for array_entry in buffers {
                                    handles_opt.push(array_entry.buffer.as_ref()
                                        as *const ProtocolObject<dyn objc2_metal::MTLBuffer>);
                                    offsets.push(array_entry.offset as NSUInteger);
                                }
                                encoder.setBuffers_offsets_withRange(
                                    NonNull::new(handles_opt.as_mut_ptr()).unwrap(),
                                    NonNull::new(offsets.as_mut_ptr()).unwrap(),
                                    NSRange {
                                        location: start_binding,
                                        length: handles_opt.len(),
                                    },
                                );
                            }
                        }
                    }

                    *dirty = 0;
                }
            }
        }
    }
}
