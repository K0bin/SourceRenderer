use std::cell::{
    Ref,
    RefCell,
};
use std::collections::HashMap;
use std::sync::Arc;

use crate::graphics::*;

struct AB<T> {
    a: T,
    b: Option<T>,
}

#[derive(Debug, Clone)]
struct TrackedTextureSubresource {
    stages: BarrierSync,
    access: BarrierAccess,
    layout: TextureLayout,
}

impl Default for TrackedTextureSubresource {
    fn default() -> Self {
        Self {
            stages: BarrierSync::empty(),
            access: BarrierAccess::empty(),
            layout: TextureLayout::default(),
        }
    }
}

struct TrackedTexture {
    subresources: Vec<TrackedTextureSubresource>,
    texture: Arc<Texture>,
    views: HashMap<TextureViewInfo, Arc<TextureView>>,
}

struct TrackedBuffer {
    stages: BarrierSync,
    access: BarrierAccess,
    buffer: Arc<BufferSlice>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum ABEntry {
    A,
    B,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum HistoryResourceEntry {
    Current,
    Past,
}

#[derive(Debug)]
struct GlobalMemoryBarrier {
    stages: BarrierSync,
    access: BarrierAccess,
}

const USE_GLOBAL_MEMORY_BARRIERS_FOR_BUFFERS: bool = true;
const USE_COARSE_BARRIERS_FOR_TEXTURES: bool = false;
const USE_COARSE_BARRIERS_FOR_BUFFERS: bool = false;
const WARN_ABOUT_READ_TO_READ_BARRIERS: bool = true;

fn calculate_subresources(mip_length: u32, array_length: u32) -> u32 {
    array_length * mip_length
}

fn calculate_subresource(mip_level: u32, mip_length: u32, array_layer: u32) -> u32 {
    array_layer * mip_length + mip_level
}

pub struct RendererResources {
    device: Arc<Device>,
    textures: HashMap<String, AB<RefCell<TrackedTexture>>>,
    buffers: HashMap<String, AB<RefCell<TrackedBuffer>>>,
    nearest_sampler: Arc<Sampler>,
    linear_sampler: Arc<Sampler>,
    current_pass: ABEntry,
    global: RefCell<GlobalMemoryBarrier>,
}

impl RendererResources {
    pub fn new(device: &Arc<Device>) -> Self {
        let nearest_sampler = Arc::new(device.create_sampler(&SamplerInfo {
            mag_filter: Filter::Nearest,
            min_filter: Filter::Nearest,
            mip_filter: Filter::Nearest,
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mip_bias: 0f32,
            max_anisotropy: 1f32,
            compare_op: None,
            min_lod: 0f32,
            max_lod: None,
        }));
        let linear_sampler = Arc::new(device.create_sampler(&SamplerInfo {
            mag_filter: Filter::Linear,
            min_filter: Filter::Linear,
            mip_filter: Filter::Linear,
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mip_bias: 0f32,
            max_anisotropy: 1f32,
            compare_op: None,
            min_lod: 0f32,
            max_lod: None,
        }));

        Self {
            device: device.clone(),
            textures: HashMap::new(),
            buffers: HashMap::new(),
            linear_sampler,
            nearest_sampler,
            current_pass: ABEntry::A,
            global: RefCell::new(GlobalMemoryBarrier {
                stages: BarrierSync::empty(),
                access: BarrierAccess::empty(),
            }),
        }
    }

    pub fn swap_history_resources(&mut self) {
        self.current_pass = match self.current_pass {
            ABEntry::A => ABEntry::B,
            ABEntry::B => ABEntry::A,
        };
    }

    #[inline(always)]
    pub fn nearest_sampler(&self) -> &Arc<Sampler> {
        &self.nearest_sampler
    }

    #[inline(always)]
    pub fn linear_sampler(&self) -> &Arc<Sampler> {
        &self.linear_sampler
    }

    pub fn create_texture(&mut self, name: &str, info: &TextureInfo, has_history: bool) {
        let mut subresources: Vec<TrackedTextureSubresource> = Vec::new();
        subresources.resize(
            calculate_subresources(info.mip_levels, info.array_length) as usize,
            TrackedTextureSubresource::default(),
        );

        self.textures.insert(
            name.to_string(),
            AB {
                a: RefCell::new(TrackedTexture {
                    subresources: subresources.clone(),
                    texture: self.device.create_texture(info, Some(name)).unwrap(),
                    views: HashMap::new(),
                }),
                b: has_history.then(|| {
                    RefCell::new(TrackedTexture {
                        subresources,
                        texture: self
                            .device
                            .create_texture(info, Some(&(name.to_string() + "_b"))).unwrap(),
                        views: HashMap::new(),
                    })
                }),
            },
        );
    }

    #[allow(unused)]
    pub fn create_buffer(
        &mut self,
        name: &str,
        info: &BufferInfo,
        memory_usage: MemoryUsage,
        has_history: bool,
    ) {
        self.buffers.insert(
            name.to_string(),
            AB {
                a: RefCell::new(TrackedBuffer {
                    stages: BarrierSync::empty(),
                    access: BarrierAccess::empty(),
                    buffer: self.device.create_buffer(info, memory_usage, Some(name)).unwrap(),
                }),
                b: has_history.then(|| {
                    RefCell::new(TrackedBuffer {
                        stages: BarrierSync::empty(),
                        access: BarrierAccess::empty(),
                        buffer: self.device.create_buffer(
                            info,
                            memory_usage,
                            Some(&(name.to_string() + "_b")),
                        ).unwrap(),
                    })
                }),
            },
        );
    }

    #[inline]
    pub fn texture_info(&self, name: &str) -> Ref<TextureInfo> {
        let entry = self.textures.get(name);
        let texture_ref = entry.unwrap().a.borrow();
        Ref::map(texture_ref, |texture| texture.texture.info())
    }

    #[allow(unused)]
    #[inline]
    pub fn buffer_info(&self, name: &str) -> Ref<BufferInfo> {
        let entry = self.buffers.get(name);
        let buffer_ref = entry.unwrap().a.borrow();
        Ref::map(buffer_ref, |buffer| buffer.buffer.info())
    }

    fn access_texture_internal(
        &self,
        cmd_buffer: &mut CommandBuffer,
        name: &str,
        mut stages: BarrierSync,
        range: &BarrierTextureRange,
        mut access: BarrierAccess,
        layout: TextureLayout,
        discard: bool,
        history: HistoryResourceEntry,
    ) {
        let texture_ab = self
            .textures
            .get(name)
            .unwrap_or_else(|| panic!("No tracked texture by the name {}", name));
        debug_assert!(history != HistoryResourceEntry::Past || texture_ab.b.is_some());

        if USE_COARSE_BARRIERS_FOR_TEXTURES && !access.is_write() {
            // we're doing a read access
            // use broad scope of stages & access flags to avoid further unnecessary reading barriers
            let all_graphics_shaders: BarrierSync =
                BarrierSync::VERTEX_SHADER | BarrierSync::FRAGMENT_SHADER;
            if stages.intersects(all_graphics_shaders) {
                stages |= all_graphics_shaders;
            }
            access = BarrierAccess::MEMORY_READ;
        }

        let use_b_resource = (history == HistoryResourceEntry::Past)
            == (self.current_pass == ABEntry::A)
            && texture_ab.b.is_some();

        let mut texture_mut = if !use_b_resource {
            texture_ab.a.borrow_mut()
        } else {
            texture_ab.b.as_ref().unwrap().borrow_mut()
        };

        let total_mip_level_count = texture_mut.texture.info().mip_levels;
        for array_index in range.base_array_layer..range.base_array_layer + range.array_layer_length
        {
            for mip_index in range.base_mip_level..range.base_mip_level + range.mip_level_length {
                let subresource_index =
                    calculate_subresource(mip_index, total_mip_level_count, array_index);

                let subresource_mut = texture_mut
                    .subresources
                    .get_mut(subresource_index as usize)
                    .unwrap();

                let needs_barrier = access.is_write()
                    || subresource_mut.access.is_write()
                    || subresource_mut.layout != layout
                    || !subresource_mut.access.contains(access)
                    || !subresource_mut.stages.contains(stages);
                if needs_barrier {
                    let mut subresource_clone = subresource_mut.clone();

                    if WARN_ABOUT_READ_TO_READ_BARRIERS
                        && !access.is_write()
                        && !subresource_clone.access.is_write()
                        && subresource_clone.layout == layout
                    {
                        log::warn!(
                            "READ TO READ BARRIER: Texture: \"{}\", stage: {:?}, access: {:?}",
                            name, stages, access
                        );
                    }

                    cmd_buffer.barrier(&[Barrier::TextureBarrier {
                        old_sync: subresource_clone.stages,
                        new_sync: stages,
                        old_layout: if !discard {
                            subresource_clone.layout
                        } else {
                            TextureLayout::Undefined
                        },
                        new_layout: layout,
                        old_access: if !discard {
                            subresource_clone.access & BarrierAccess::write_mask()
                        } else {
                            BarrierAccess::empty()
                        },
                        new_access: access,
                        texture: &texture_mut.texture,
                        range: BarrierTextureRange {
                            base_array_layer: array_index,
                            array_layer_length: 1,
                            base_mip_level: mip_index,
                            mip_level_length: 1,
                        },
                        queue_ownership: None
                    }]);
                    if access.is_write()
                        || subresource_clone.access.is_write()
                        || subresource_clone.layout != layout
                    {
                        subresource_clone.access = access;
                    } else {
                        subresource_clone.access |= access;
                    }
                    subresource_clone.stages = stages;
                    subresource_clone.layout = layout;
                    texture_mut.subresources[subresource_index as usize] = subresource_clone;
                }
            }
        }
    }

    pub fn access_texture(
        &self,
        cmd_buffer: &mut CommandBuffer,
        name: &str,
        range: &BarrierTextureRange,
        stages: BarrierSync,
        access: BarrierAccess,
        layout: TextureLayout,
        discard: bool,
        history: HistoryResourceEntry,
    ) -> Ref<Arc<Texture>> {
        self.access_texture_internal(
            cmd_buffer, name, stages, range, access, layout, discard, history,
        );
        let texture_ab = self
            .textures
            .get(name)
            .unwrap_or_else(|| panic!("No tracked texture by the name {}", name));
        debug_assert!(history != HistoryResourceEntry::Past || texture_ab.b.is_some());
        let use_b_resource = (history == HistoryResourceEntry::Past)
            == (self.current_pass == ABEntry::A)
            && texture_ab.b.is_some();
        let texture_ref = if !use_b_resource {
            texture_ab.a.borrow()
        } else {
            texture_ab.b.as_ref().unwrap().borrow()
        };
        Ref::map(texture_ref, |r| &r.texture)
    }

    pub fn access_view(
        &self,
        cmd_buffer: &mut CommandBuffer,
        name: &str,
        stages: BarrierSync,
        access: BarrierAccess,
        layout: TextureLayout,
        discard: bool,
        info: &TextureViewInfo,
        history: HistoryResourceEntry,
    ) -> Ref<Arc<TextureView>> {
        self.access_texture_internal(
            cmd_buffer,
            name,
            stages,
            &info.into(),
            access,
            layout,
            discard,
            history,
        );
        self.get_view(name, info, history)
    }

    pub fn get_view(
        &self,
        name: &str,
        info: &TextureViewInfo,
        history: HistoryResourceEntry,
    ) -> Ref<Arc<TextureView>> {
        let texture_ab = self
            .textures
            .get(name)
            .unwrap_or_else(|| panic!("No tracked texture by the name {}", name));
        debug_assert!(history != HistoryResourceEntry::Past || texture_ab.b.is_some());
        let use_b_resource = (history == HistoryResourceEntry::Past)
            == (self.current_pass == ABEntry::A)
            && texture_ab.b.is_some();
        {
            let texture_ref = if !use_b_resource {
                texture_ab.a.borrow()
            } else {
                texture_ab.b.as_ref().unwrap().borrow()
            };
            if texture_ref.views.contains_key(info) {
                return Ref::map(texture_ref, |r| r.views.get(info).unwrap());
            }
        }

        {
            let mut texture_mut = if !use_b_resource {
                texture_ab.a.borrow_mut()
            } else {
                texture_ab.b.as_ref().unwrap().borrow_mut()
            };
            let view = self.device.create_texture_view(
                &texture_mut.texture,
                info,
                Some(&(name.to_string() + "_srv")),
            );
            texture_mut.views.insert(info.clone(), view);
        }

        {
            let texture_ref = if !use_b_resource {
                texture_ab.a.borrow()
            } else {
                texture_ab.b.as_ref().unwrap().borrow()
            };
            return Ref::map(texture_ref, |r| r.views.get(info).unwrap());
        }
    }

    pub fn access_buffer(
        &self,
        cmd_buffer: &mut CommandBuffer,
        name: &str,
        mut stages: BarrierSync,
        mut access: BarrierAccess,
        history: HistoryResourceEntry,
    ) -> Ref<Arc<BufferSlice>> {
        debug_assert_eq!(
            access
                & !(BarrierAccess::VERTEX_INPUT_READ
                    | BarrierAccess::INDEX_READ
                    | BarrierAccess::INDIRECT_READ
                    | BarrierAccess::CONSTANT_READ
                    | BarrierAccess::COPY_READ
                    | BarrierAccess::COPY_WRITE
                    | BarrierAccess::STORAGE_READ
                    | BarrierAccess::STORAGE_WRITE
                    | BarrierAccess::ACCELERATION_STRUCTURE_READ
                    | BarrierAccess::ACCELERATION_STRUCTURE_WRITE
                    | BarrierAccess::SHADER_READ
                    | BarrierAccess::SHADER_WRITE
                    | BarrierAccess::MEMORY_READ
                    | BarrierAccess::MEMORY_WRITE
                    | BarrierAccess::HOST_READ
                    | BarrierAccess::HOST_WRITE),
            BarrierAccess::empty()
        );
        debug_assert_eq!(
            stages
                & !(BarrierSync::COPY
                    | BarrierSync::VERTEX_INPUT
                    | BarrierSync::VERTEX_SHADER
                    | BarrierSync::FRAGMENT_SHADER
                    | BarrierSync::COMPUTE_SHADER
                    | BarrierSync::INDEX_INPUT
                    | BarrierSync::INDIRECT
                    | BarrierSync::ACCELERATION_STRUCTURE_BUILD
                    | BarrierSync::RAY_TRACING),
            BarrierSync::empty()
        );

        if USE_COARSE_BARRIERS_FOR_BUFFERS && !access.is_write() {
            // we're doing a read access
            // use broad scope of stages & access flags to avoid further unnecessary reading barriers
            let all_graphics: BarrierSync = BarrierSync::EARLY_DEPTH
                | BarrierSync::LATE_DEPTH
                | BarrierSync::VERTEX_INPUT
                | BarrierSync::VERTEX_SHADER
                | BarrierSync::FRAGMENT_SHADER
                | BarrierSync::RENDER_TARGET
                | BarrierSync::INDIRECT;
            if stages.intersects(all_graphics) {
                stages |= all_graphics;
            }
            access = BarrierAccess::MEMORY_READ;
        }

        let buffer_ab = self
            .buffers
            .get(name)
            .unwrap_or_else(|| panic!("No tracked buffer by the name {}", name));
        debug_assert!(history != HistoryResourceEntry::Past || buffer_ab.b.is_some());
        let use_b_resource = (history == HistoryResourceEntry::Past)
            == (self.current_pass == ABEntry::A)
            && buffer_ab.b.is_some();

        if !USE_GLOBAL_MEMORY_BARRIERS_FOR_BUFFERS {
            let mut buffer_mut = if !use_b_resource {
                buffer_ab.a.borrow_mut()
            } else {
                buffer_ab.b.as_ref().unwrap().borrow_mut()
            };

            let needs_barrier = access.is_write()
                || buffer_mut.access.is_write()
                || !buffer_mut.access.contains(access)
                || !buffer_mut.stages.contains(stages);
            if needs_barrier {
                if WARN_ABOUT_READ_TO_READ_BARRIERS
                    && !access.is_write()
                    && !buffer_mut.access.is_write()
                {
                    log::warn!(
                        "READ TO READ BARRIER: Buffer: \"{}\", stage: {:?}, access: {:?}",
                        name, stages, access
                    );
                }

                cmd_buffer.barrier(&[Barrier::BufferBarrier {
                    old_sync: buffer_mut.stages,
                    new_sync: stages,
                    old_access: buffer_mut.access & BarrierAccess::write_mask(),
                    new_access: access,
                    buffer: BufferRef::Regular(&buffer_mut.buffer),
                    queue_ownership: None
                }]);
                if access.is_write() || buffer_mut.access.is_write() {
                    buffer_mut.access = access;
                } else {
                    buffer_mut.access |= access;
                }
                buffer_mut.stages = stages;
            }
        } else {
            let mut global_mut = self.global.borrow_mut();
            let needs_barrier = access.is_write()
                || global_mut.access.is_write()
                || !global_mut.access.contains(access)
                || !global_mut.stages.contains(stages);
            if needs_barrier {
                if WARN_ABOUT_READ_TO_READ_BARRIERS
                    && !access.is_write()
                    && !global_mut.access.is_write()
                {
                    log::warn!(
                        "READ TO READ BARRIER: Buffer: \"{}\", stage: {:?}, access: {:?}",
                        name, stages, access
                    );
                }

                cmd_buffer.barrier(&[Barrier::GlobalBarrier {
                    old_sync: global_mut.stages,
                    new_sync: stages,
                    old_access: global_mut.access & BarrierAccess::write_mask(),
                    new_access: access,
                }]);
                if access.is_write() || global_mut.access.is_write() {
                    global_mut.access = access;
                } else {
                    global_mut.access |= access;
                }
                global_mut.stages = stages;
            }
        }

        let buffer_ref = if !use_b_resource {
            buffer_ab.a.borrow()
        } else {
            buffer_ab.b.as_ref().unwrap().borrow()
        };
        Ref::map(buffer_ref, |r| &r.buffer)
    }
}
