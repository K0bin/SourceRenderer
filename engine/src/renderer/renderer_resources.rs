use crate::graphics::*;
use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::sync::Arc;
use smallvec::SmallVec;

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

pub struct TrackedTexture {
    subresources: Vec<RefCell<TrackedTextureSubresource>>,
    texture: Arc<Texture>,
    views: RefCell<HashMap<TextureViewInfo, Arc<TextureView>>>,
}

enum TrackedBufferType {
    Transient(TransientBufferSlice),
    Regular(Arc<BufferSlice>)
}

pub struct TrackedBuffer {
    stages: BarrierSync,
    access: BarrierAccess,
    buffer: TrackedBufferType,
}

impl TrackedBufferType {
    fn buffer_ref(&self) -> BufferRef<'_> {
        match self {
            TrackedBufferType::Transient(b) => BufferRef::Transient(b),
            TrackedBufferType::Regular(b) => BufferRef::Regular(b)
        }
    }

    fn length(&self) -> u64 {
        match self {
            TrackedBufferType::Transient(b) => b.length(),
            TrackedBufferType::Regular(b) => b.length(),
        }
    }
}

pub enum ResourceViewType<'a, T> {
    Mut(RefMut<'a, T>),
    Ref(Ref<'a, T>),
}

pub struct BufferAccessor<'a> {
    view_type: ResourceViewType<'a, TrackedBuffer>,
}
impl BufferAccessor<'_> {
    pub fn buffer_ref(&self) -> BufferRef<'_> {
        match &self.view_type {
            ResourceViewType::Mut(b) => b.buffer.buffer_ref(),
            ResourceViewType::Ref(b) => b.buffer.buffer_ref(),
        }
    }

    pub fn length(&self) -> u64 {
        match &self.view_type {
            ResourceViewType::Mut(b) => b.buffer.length(),
            ResourceViewType::Ref(b) => b.buffer.length(),
        }
    }
}

pub struct TextureAccessor<'a> {
    _subresources: SmallVec<[ResourceViewType<'a, TrackedTextureSubresource>; 1]>,
    texture: &'a Arc<Texture>,
    view: Option<Arc<TextureView>>,
}

impl TextureAccessor<'_> {
    pub fn view(&self) -> &Arc<TextureView> {
        self.view.as_ref().unwrap()
    }
    pub fn texture(&self) -> &Arc<Texture> {
        self.texture
    }
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

const USE_COARSE_BARRIERS_FOR_TEXTURES: bool = false;
const USE_COARSE_BARRIERS_FOR_BUFFERS: bool = false;
const WARN_ABOUT_READ_TO_READ_BARRIERS: bool = false;

fn calculate_subresources(mip_length: u32, array_length: u32) -> u32 {
    array_length * mip_length
}

fn calculate_subresource(mip_level: u32, mip_length: u32, array_layer: u32) -> u32 {
    array_layer * mip_length + mip_level
}

pub struct RendererResources {
    device: Arc<Device>,
    textures: HashMap<String, AB<TrackedTexture>>,
    buffers: HashMap<String, AB<RefCell<TrackedBuffer>>>,
    nearest_sampler: Arc<Sampler>,
    linear_sampler: Arc<Sampler>,
    current_pass: ABEntry,
}

impl RendererResources {
    pub fn new(device: &Arc<Device>) -> Self {
        let nearest_sampler = Arc::new(device.create_sampler(
            &SamplerInfo {
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
            },
            Some("Nearest"),
        ));
        let linear_sampler = Arc::new(device.create_sampler(
            &SamplerInfo {
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
            },
            Some("Linear"),
        ));

        Self {
            device: device.clone(),
            textures: HashMap::new(),
            buffers: HashMap::new(),
            linear_sampler,
            nearest_sampler,
            current_pass: ABEntry::A,
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
        let mut subresources: Vec<RefCell<TrackedTextureSubresource>> = Vec::new();
        let array_layers = info.array_length
            * if info.dimension == TextureDimension::Cube
                || info.dimension == TextureDimension::CubeArray
            {
                6u32
            } else {
                1u32
            };
        subresources.resize(
            calculate_subresources(info.mip_levels, array_layers) as usize,
            RefCell::new(TrackedTextureSubresource::default()),
        );

        let old = self.textures.insert(
            name.to_string(),
            AB {
                a: TrackedTexture {
                    subresources: subresources.clone(),
                    texture: self.device.create_texture(info, Some(name)).unwrap(),
                    views: RefCell::new(HashMap::new()),
                },
                b: has_history.then(|| {
                    TrackedTexture {
                        subresources,
                        texture: self
                            .device
                            .create_texture(info, Some(&(name.to_string() + "_b")))
                            .unwrap(),
                        views: RefCell::new(HashMap::new()),
                    }
                }),
            },
        );

        if old.is_some() {
            log::warn!("Replacing texture {}", name);
        }
    }

    #[allow(unused)]
    pub fn create_buffer(
        &mut self,
        name: &str,
        info: &BufferInfo,
        memory_usage: MemoryUsage,
        has_history: bool,
    ) {
        let old = self.buffers.insert(
            name.to_string(),
            AB {
                a: RefCell::new(TrackedBuffer {
                    stages: BarrierSync::empty(),
                    access: BarrierAccess::empty(),
                    buffer: TrackedBufferType::Regular(self
                        .device
                        .create_buffer(info, memory_usage, Some(name))
                        .unwrap()),
                }),
                b: has_history.then(|| {
                    RefCell::new(TrackedBuffer {
                        stages: BarrierSync::empty(),
                        access: BarrierAccess::empty(),
                        buffer: TrackedBufferType::Regular(self
                            .device
                            .create_buffer(info, memory_usage, Some(&(name.to_string() + "_b")))
                            .unwrap()),
                    })
                }),
            },
        );

        if old.is_some() {
            log::warn!("Replacing buffer {}", name);
        }
    }

    #[inline]
    pub fn texture_info(&self, name: &str) -> &TextureInfo {
        let entry = self.textures.get(name);
        &entry.as_ref().unwrap().a.texture.info()
    }

    pub fn has_buffer(&self, name: &str) -> bool {
        self.buffers.contains_key(name)
    }

    pub fn has_texture(&self, name: &str) -> bool {
        self.textures.contains_key(name)
    }

    #[allow(unused)]
    #[inline]
    pub fn buffer_range(&self, name: &str) -> Range {
        let entry = self.buffers.get(name);
        let buffer_ref = entry.unwrap().a.borrow();
        match &buffer_ref.buffer {
            TrackedBufferType::Transient(b) => b.range(),
            TrackedBufferType::Regular(b) => b.range()
        }
    }

    pub fn destroy_buffer(&mut self, name: &str) {
        self.buffers.remove(name);
    }

    pub fn destroy_texture(&mut self, name: &str) {
        self.textures.remove(name);
    }

    pub fn buffer_keys(&self) -> Vec<&str> {
        self.buffers.keys().map(|k| k.as_str()).collect()
    }

    pub fn texture_keys(&self) -> Vec<&str> {
        self.textures.keys().map(|k| k.as_str()).collect()
    }

    pub fn access_texture<'a>(
        &'a self,
        cmd_buffer: &mut CommandBuffer,
        name: &str,
        range: &BarrierTextureRange,
        mut stages: BarrierSync,
        mut access: BarrierAccess,
        layout: TextureLayout,
        discard: bool,
        history: HistoryResourceEntry,
    ) -> TextureAccessor<'a> {
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

        debug_assert!(
            access.is_empty() || !stages.is_empty() || (access.is_empty() && stages.is_empty())
        );

        let use_b_resource = (history == HistoryResourceEntry::Past)
            == (self.current_pass == ABEntry::A)
            && texture_ab.b.is_some();

        let texture = if !use_b_resource {
            &texture_ab.a
        } else {
            texture_ab.b.as_ref().unwrap()
        };

        let mut subresources: SmallVec<[ResourceViewType<TrackedTextureSubresource>; 1]> = SmallVec::new();
        let total_mip_level_count = texture.texture.info().mip_levels;
        for array_index in range.base_array_layer..range.base_array_layer + range.array_layer_length
        {
            for mip_index in range.base_mip_level..range.base_mip_level + range.mip_level_length {
                let subresource_index =
                    calculate_subresource(mip_index, total_mip_level_count, array_index);

                let subresource_refcell = texture
                    .subresources
                    .get(subresource_index as usize)
                    .unwrap();
                let mut subresource_mut = subresource_refcell
                    .borrow_mut();

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
                            name,
                            stages,
                            access
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
                        texture: &texture.texture,
                        range: BarrierTextureRange {
                            base_array_layer: array_index,
                            array_layer_length: 1,
                            base_mip_level: mip_index,
                            mip_level_length: 1,
                        },
                        queue_ownership: None,
                    }]);
                    if access.is_write()
                        || subresource_clone.access.is_write()
                        || subresource_clone.layout != layout
                    {
                        subresource_clone.access = access;
                        subresource_clone.stages = stages;
                        subresource_clone.layout = layout;
                    } else {
                        subresource_clone.access |= access;
                        subresource_clone.stages |= stages;
                    }
                    *subresource_mut = subresource_clone;
                }

                let inner = if access.is_write() {
                    ResourceViewType::Mut(subresource_mut)
                } else {
                    std::mem::drop(subresource_mut);
                    ResourceViewType::Ref(subresource_refcell.borrow())
                };
                subresources.push(inner);
            }
        }

        TextureAccessor {
            _subresources: subresources,
            view: None,
            texture: &texture.texture,
        }
    }

    pub fn access_view<'a>(
        &'a self,
        cmd_buffer: &mut CommandBuffer,
        name: &str,
        stages: BarrierSync,
        access: BarrierAccess,
        layout: TextureLayout,
        discard: bool,
        info: &TextureViewInfo,
        history: HistoryResourceEntry,
    ) -> TextureAccessor<'a> {
        let mut accessor = self.access_texture(
            cmd_buffer,
            name,
            &info.into(),
            stages,
            access,
            layout,
            discard,
            history,
        );
        accessor.view = Some(self.get_view(name, info, history).clone());
        accessor
    }

    pub fn get_view<'a>(
        &'a self,
        name: &str,
        info: &TextureViewInfo,
        history: HistoryResourceEntry,
    ) -> Ref<'a, Arc<TextureView>> {
        let texture_ab = self
            .textures
            .get(name)
            .unwrap_or_else(|| panic!("No tracked texture by the name {}", name));
        debug_assert!(history != HistoryResourceEntry::Past || texture_ab.b.is_some());
        let use_b_resource = (history == HistoryResourceEntry::Past)
            == (self.current_pass == ABEntry::A)
            && texture_ab.b.is_some();

        let tracked_texture = if !use_b_resource {
            &texture_ab.a
        } else {
            texture_ab.b.as_ref().unwrap()
        };

        let views = tracked_texture.views.borrow();
        if !views.contains_key(&info) {
            std::mem::drop(views);
            let mut views = tracked_texture.views.borrow_mut();
            let texture = &tracked_texture.texture;
            let view = self.device.create_texture_view(
                &texture,
                info,
                Some(&(name.to_string() + "_srv")),
            );
            views.insert(info.clone(), view);
        }

        let views = tracked_texture.views.borrow();
        Ref::map(views, |views| views.get(info).unwrap())
    }

    pub fn has_resource(&self, name: &str) -> bool {
        self.textures.contains_key(name) || self.buffers.contains_key(name)
    }

    pub fn access_buffer<'a>(
        &'a self,
        cmd_buffer: &mut CommandBuffer,
        name: &str,
        mut stages: BarrierSync,
        mut access: BarrierAccess,
        history: HistoryResourceEntry,
    ) -> BufferAccessor<'a> {
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
                    | BarrierAccess::HOST_WRITE
                    | BarrierAccess::CLEAR_WRITE
                    | BarrierAccess::BLIT_READ
                    | BarrierAccess::BLIT_WRITE),
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
                    | BarrierSync::RAY_TRACING_SHADER
                    | BarrierSync::BLIT
                    | BarrierSync::CLEAR),
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

        debug_assert!(
            access.is_empty() || !stages.is_empty() || (access.is_empty() && stages.is_empty())
        );

        let buffer_ab = self
            .buffers
            .get(name)
            .unwrap_or_else(|| panic!("No tracked buffer by the name {}", name));
        debug_assert!(history != HistoryResourceEntry::Past || buffer_ab.b.is_some());
        let use_b_resource = (history == HistoryResourceEntry::Past)
            == (self.current_pass == ABEntry::A)
            && buffer_ab.b.is_some();

        let buffer_refcell = if !use_b_resource {
            &buffer_ab.a
        } else {
            buffer_ab.b.as_ref().unwrap()
        };

        let mut buffer_mut = buffer_refcell.borrow_mut();

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
                    name,
                    stages,
                    access
                );
            }

            cmd_buffer.barrier(&[Barrier::BufferBarrier {
                old_sync: buffer_mut.stages,
                new_sync: stages,
                old_access: buffer_mut.access & BarrierAccess::write_mask(),
                new_access: access,
                buffer: buffer_mut.buffer.buffer_ref(),
                queue_ownership: None,
            }]);
            if access.is_write() || buffer_mut.access.is_write() {
                buffer_mut.access = access;
                buffer_mut.stages = stages;
            } else {
                buffer_mut.access |= access;
                buffer_mut.stages |= stages;
            }
        }

        let inner = if access.is_write() {
            ResourceViewType::Mut(buffer_mut)
        } else {
            std::mem::drop(buffer_mut);
            ResourceViewType::Ref(buffer_refcell.borrow())
        };
        BufferAccessor {
            view_type: inner,
        }
    }

    pub fn track_transient_buffer(&mut self, name: &str, buffer: TransientBufferSlice) {
        self.buffers.insert(name.to_string(), AB {
            a: RefCell::new(TrackedBuffer {
                stages: BarrierSync::empty(),
                access: BarrierAccess::empty() ,
                buffer: TrackedBufferType::Transient(buffer),
            }),
            b: None
        });
    }

    pub fn remove_transient_buffers(&mut self) {
        self.buffers.retain(|_, buffer| {
            if let &TrackedBufferType::Regular(_) = &buffer.a.borrow().buffer {
                true
            } else {
                false
            }
        });
    }
}
