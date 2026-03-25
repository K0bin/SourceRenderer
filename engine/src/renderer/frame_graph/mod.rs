use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{
    DefaultHasher,
    Hash,
    Hasher,
};
use std::ptr::{
    read,
    NonNull,
};
use std::sync::Arc;

use atomic_refcell::{
    AtomicRef,
    AtomicRefCell,
    AtomicRefMut,
};
use bitvec::domain::PartialElement;
use bumpalo::collections::{
    String as BumpString,
    Vec as BumpVec,
};
use bumpalo::Bump;
use smallvec::{
    smallvec,
    SmallVec,
};
use sourcerenderer_core::gpu::{
    BarrierAccess,
    BarrierSync,
    BarrierTextureRange,
    BufferInfo,
    ClearColor,
    ClearDepthStencilValue,
    TextureInfo,
    TextureLayout,
};

use crate::graphics::{
    BufferSlice,
    Device,
    GraphicsContext,
    MemoryUsage,
    Texture,
};

pub trait RenderPass {
    fn create_resources(&mut self, builder: &mut FramePassResourceCreationContext);
    fn register_resource_accesses(&mut self, builder: &mut FramePassResourceAccessContext);

    fn execute(&self);
}

#[derive(Debug, Clone)]
struct ResourceDescription<T: Clone> {
    name: &'static str,
    info: T,
    has_history: bool,
}

pub struct FramePassResourceCreationContext<'a, 'b> {
    pass_idx: PassIdx,
    texture_descriptions: &'a mut BumpVec<'b, ResourceDescription<TextureInfo>>,
    buffer_descriptions: &'a mut BumpVec<'b, ResourceDescription<(BufferInfo, MemoryUsage)>>,
    texture_metadata: &'a mut BumpVec<'b, ResourceWrite<TextureHandle>>,
    buffer_metadata: &'a mut BumpVec<'b, ResourceWrite<BufferHandle>>,
    texture_handle_map: &'a mut HashMap<&'static str, TextureHandle>,
    buffer_handle_map: &'a mut HashMap<&'static str, BufferHandle>,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
struct TextureHandle(usize);

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
struct BufferHandle(usize);

impl<'a, 'b> FramePassResourceCreationContext<'a, 'b> {
    pub fn create_texture(&mut self, name: &'static str, info: &TextureInfo) {
        self.texture_descriptions.push(ResourceDescription {
            name,
            info: info.clone(),
            has_history: false,
        });
    }

    pub fn create_buffer(
        &mut self,
        name: &'static str,
        info: &BufferInfo,
        memory_usage: MemoryUsage,
    ) {
        self.buffer_descriptions.push(ResourceDescription {
            name,
            info: (info.clone(), memory_usage),
            has_history: false,
        })
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum HistoryResourceEntry {
    Past,
    Current,
}

impl HistoryResourceEntry {
    fn invert(self) -> Self {
        match self {
            HistoryResourceEntry::Past => HistoryResourceEntry::Current,
            HistoryResourceEntry::Current => HistoryResourceEntry::Past,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum TextureAccessKind {
    Sampling,
    StorageRead,
    StorageWrite,
    StorageWriteEntireSubresource,
    StorageReadWrite,
    RenderTargetCleared(ClearColor),
    DepthStencilCleared(ClearDepthStencilValue),
    DepthStencilReadOnly,
    RenderTarget,
    DepthStencil,
    BlitSrc,
    BlitDst,
    BlitDstEntireSubresource,
    CopySrc,
    CopyDst,
    CopyDstEntireSubresource,
}

impl TextureAccessKind {
    fn is_write(self) -> bool {
        match self {
            TextureAccessKind::StorageReadWrite
            | TextureAccessKind::StorageWrite
            | TextureAccessKind::StorageWriteEntireSubresource
            | TextureAccessKind::BlitDst
            | TextureAccessKind::BlitDstEntireSubresource
            | TextureAccessKind::CopyDst
            | TextureAccessKind::CopyDstEntireSubresource
            | TextureAccessKind::RenderTarget
            | TextureAccessKind::RenderTargetCleared(_)
            | TextureAccessKind::DepthStencil
            | TextureAccessKind::DepthStencilCleared(_) => true,
            _ => false,
        }
    }

    fn can_discard(self) -> bool {
        match self {
            TextureAccessKind::StorageWriteEntireSubresource
            | TextureAccessKind::BlitDstEntireSubresource
            | TextureAccessKind::CopyDstEntireSubresource
            | TextureAccessKind::RenderTargetCleared(_)
            | TextureAccessKind::DepthStencilCleared(_) => true,
            _ => false,
        }
    }

    fn to_access(self) -> BarrierAccess {
        match self {
            TextureAccessKind::Sampling => BarrierAccess::SAMPLING_READ,
            TextureAccessKind::StorageRead => BarrierAccess::STORAGE_READ,
            TextureAccessKind::StorageWrite => BarrierAccess::STORAGE_WRITE,
            TextureAccessKind::StorageWriteEntireSubresource => BarrierAccess::STORAGE_WRITE,
            TextureAccessKind::StorageReadWrite => {
                BarrierAccess::STORAGE_WRITE | BarrierAccess::STORAGE_READ
            }
            TextureAccessKind::RenderTargetCleared(_) => BarrierAccess::RENDER_TARGET_WRITE,
            TextureAccessKind::DepthStencilCleared(_) => BarrierAccess::DEPTH_STENCIL_WRITE,
            TextureAccessKind::DepthStencilReadOnly => BarrierAccess::DEPTH_STENCIL_READ,
            TextureAccessKind::RenderTarget => {
                BarrierAccess::RENDER_TARGET_WRITE | BarrierAccess::RENDER_TARGET_READ
            }
            TextureAccessKind::DepthStencil => {
                BarrierAccess::DEPTH_STENCIL_READ | BarrierAccess::DEPTH_STENCIL_WRITE
            }
            TextureAccessKind::BlitSrc => BarrierAccess::COPY_READ,
            TextureAccessKind::BlitDst => BarrierAccess::COPY_WRITE,
            TextureAccessKind::BlitDstEntireSubresource => BarrierAccess::COPY_WRITE,
            TextureAccessKind::CopySrc => BarrierAccess::COPY_READ,
            TextureAccessKind::CopyDst => BarrierAccess::COPY_WRITE,
            TextureAccessKind::CopyDstEntireSubresource => BarrierAccess::COPY_WRITE,
        }
    }

    fn to_layout(self) -> TextureLayout {
        match self {
            TextureAccessKind::Sampling => TextureLayout::Sampled,
            TextureAccessKind::StorageRead => TextureLayout::Storage,
            TextureAccessKind::StorageWrite => TextureLayout::Storage,
            TextureAccessKind::StorageWriteEntireSubresource => TextureLayout::Storage,
            TextureAccessKind::StorageReadWrite => TextureLayout::Storage,
            TextureAccessKind::RenderTargetCleared(_) => TextureLayout::RenderTarget,
            TextureAccessKind::DepthStencilCleared(_) => TextureLayout::DepthStencilReadWrite,
            TextureAccessKind::DepthStencilReadOnly => TextureLayout::DepthStencilRead,
            TextureAccessKind::RenderTarget => TextureLayout::RenderTarget,
            TextureAccessKind::DepthStencil => TextureLayout::DepthStencilReadWrite,
            TextureAccessKind::BlitSrc => TextureLayout::CopySrc,
            TextureAccessKind::BlitDst => TextureLayout::CopyDst,
            TextureAccessKind::BlitDstEntireSubresource => TextureLayout::CopyDst,
            TextureAccessKind::CopySrc => TextureLayout::CopySrc,
            TextureAccessKind::CopyDst => TextureLayout::CopyDst,
            TextureAccessKind::CopyDstEntireSubresource => TextureLayout::CopyDst,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum BufferAccessKind {
    ShaderRead,
    ShaderWrite,
    ShaderReadWrite,
    CopySrc,
    CopyDst,
}

impl BufferAccessKind {
    fn is_write(self) -> bool {
        match self {
            BufferAccessKind::ShaderReadWrite
            | BufferAccessKind::ShaderWrite
            | BufferAccessKind::CopyDst => true,
            _ => false,
        }
    }

    fn to_access(self) -> BarrierAccess {
        match self {
            BufferAccessKind::ShaderRead => BarrierAccess::SHADER_READ,
            BufferAccessKind::ShaderWrite => BarrierAccess::SHADER_WRITE,
            BufferAccessKind::ShaderReadWrite => {
                BarrierAccess::SHADER_READ | BarrierAccess::SHADER_WRITE
            }
            BufferAccessKind::CopySrc => BarrierAccess::COPY_READ,
            BufferAccessKind::CopyDst => BarrierAccess::COPY_WRITE,
        }
    }
}

pub struct TextureAccess {
    history: HistoryResourceEntry,
    name: &'static str,
    kind: TextureAccessKind,
    range: BarrierTextureRange,
    stages: BarrierSync,
}

pub struct BufferAccess {
    history: HistoryResourceEntry,
    name: &'static str,
    kind: BufferAccessKind,
    range: BufferRange,
    stages: BarrierSync,
}

pub struct ResourceAvailability {
    stages: BarrierSync,
    access: BarrierAccess,
    available_since_pass_idx: u32,
}

pub struct FramePassResourceAccessContext<'a, 'b> {
    pass_idx: PassIdx,
    pass_last_dependency_pass_idx: Option<HistoryPassIdx>,
    pass_texture_accesses: &'a mut BumpVec<'b, TextureAccess>,
    pass_buffer_accesses: &'a mut BumpVec<'b, BufferAccess>,
    textures: &'a mut BumpVec<'b, ResourceWrite<TextureHandle>>,
    buffers: &'a mut BumpVec<'b, ResourceWrite<BufferHandle>>,
    texture_handle_map: &'a mut HashMap<&'static str, TextureHandle>,
    buffer_handle_map: &'a mut HashMap<&'static str, BufferHandle>,
}

#[derive(Clone)]
pub struct BufferRange {
    pub offset: u64,
    pub len: u64,
}

#[derive(Debug, Clone)]
struct ResourceWrite<THandle: Copy> {
    handle: THandle,
    previous: Option<THandle>,
    next: Option<THandle>,
    write_pass_idx: PassIdx,
    last_used_in: PassIdx,
    used_in_next_pass: bool,
    discard: bool,
    layout: TextureLayout,
    sync: BarrierSync,
    access: BarrierAccess,
    following_layout: TextureLayout,
    following_syncs: BarrierSync,
    following_accesses: BarrierAccess,
}

impl<'a, 'b> FramePassResourceAccessContext<'a, 'b> {
    pub fn register_texture_access(
        &mut self,
        name: &'static str,
        stages: BarrierSync,
        range: BarrierTextureRange,
        access_kind: TextureAccessKind,
        history: HistoryResourceEntry,
    ) {
        self.pass_texture_accesses.push(TextureAccess {
            history,
            name,
            kind: access_kind,
            range,
            stages,
        });

        let old_handle = self.texture_handle_map.get_mut(name);
        if access_kind.is_write() {
            if history == HistoryResourceEntry::Past {
                panic!("Writing to the previous frame resource is not allowed.");
            }

            let mut new_texture = ResourceWrite::<TextureHandle> {
                handle: TextureHandle(self.textures.len()),
                next: None,
                previous: old_handle.as_ref().map(|h| **h),
                last_used_in: self.pass_idx,
                write_pass_idx: self.pass_idx,
                used_in_next_pass: history == HistoryResourceEntry::Past,
                discard: access_kind.can_discard(),
                layout: access_kind.to_layout(),
                sync: stages,
                access: access_kind.to_access(),
                following_layout: TextureLayout::Undefined,
                following_syncs: BarrierSync::empty(),
                following_accesses: BarrierAccess::empty(),
            };

            let is_initial_write: bool;
            if let Some(old_handle) = &old_handle {
                let previous_texture = &mut self.textures[old_handle.0];
                assert!(previous_texture.next.is_none());
                is_initial_write = previous_texture
                    .write_pass_idx == self.pass_idx;

                if is_initial_write {
                    assert!(previous_texture.previous.is_none());
                    new_texture.previous = None;
                    *previous_texture = new_texture.clone();
                } else {
                    self.pass_last_dependency_pass_idx = Some(
                        self.pass_last_dependency_pass_idx.map_or(
                            HistoryPassIdx(history, previous_texture.write_pass_idx),
                            |last_dep| last_dep.max(HistoryPassIdx(history, previous_texture.write_pass_idx)))
                    );
                    previous_texture.last_used_in = previous_texture.last_used_in.max(self.pass_idx);

                    previous_texture.following_syncs |= stages;
                    previous_texture.following_accesses |= access_kind.to_access();
                    previous_texture.following_layout = TextureLayout::merge_layouts(previous_texture.following_layout, access_kind.to_layout());
                }
            } else {
                is_initial_write = true;
            }

            let new_handle = new_texture.handle;
            self.textures.push(new_texture);

            // Rename texture
            if !is_initial_write {
                if let Some(old_handle) = old_handle {
                    *old_handle = new_handle;
                }
            }
        } else {
            let old_handle = old_handle.expect("The first access to a resource must be a write.");

            let previous_texture = self.textures.get_mut(old_handle.0).unwrap();
            assert!(previous_texture.next.is_none());
            self.pass_last_dependency_pass_idx = Some(
                self.pass_last_dependency_pass_idx.map_or(
                    HistoryPassIdx(history, previous_texture.write_pass_idx),
                    |last_dep| last_dep.max(HistoryPassIdx(history, previous_texture.write_pass_idx)))
            );

            previous_texture.last_used_in = previous_texture.last_used_in.max(self.pass_idx);
            previous_texture.used_in_next_pass |= history == HistoryResourceEntry::Past;

            previous_texture.following_syncs |= stages;
            previous_texture.following_accesses |= access_kind.to_access();
            previous_texture.following_layout = TextureLayout::merge_layouts(previous_texture.following_layout, access_kind.to_layout());
        }
    }

    pub fn register_buffer_access(
        &mut self,
        name: &'static str,
        stages: BarrierSync,
        range: BufferRange,
        access_kind: BufferAccessKind,
        history: HistoryResourceEntry,
    ) {
        self.pass_buffer_accesses.push(BufferAccess {
            history,
            name,
            kind: access_kind,
            range: range.clone(),
            stages,
        });

        let old_handle = self.buffer_handle_map.get_mut(name);
        if access_kind.is_write() {
            if history == HistoryResourceEntry::Past {
                panic!("Writing to the previous frame resource is not allowed.");
            }

            let mut new_buffer = ResourceWrite::<BufferHandle> {
                handle: BufferHandle(self.buffers.len()),
                next: None,
                previous: old_handle.as_ref().map(|h| **h),
                last_used_in: self.pass_idx,
                write_pass_idx: self.pass_idx,
                used_in_next_pass: history == HistoryResourceEntry::Past,
                discard: range.offset == 0 && range.len == 0,
                layout: TextureLayout::General,
                sync: stages,
                access: access_kind.to_access(),
                following_layout: TextureLayout::General,
                following_syncs: BarrierSync::empty(),
                following_accesses: BarrierAccess::empty(),
            };

            let is_initial_write: bool;
            if let Some(old_handle) = &old_handle {
                let previous_buffer = &mut self.buffers[old_handle.0];
                assert!(previous_buffer.next.is_none());
                is_initial_write = previous_buffer.write_pass_idx == self.pass_idx;

                if is_initial_write {
                    assert!(previous_buffer.previous.is_none());
                    new_buffer.previous = None;
                    *previous_buffer = new_buffer.clone();
                } else {
                    self.pass_last_dependency_pass_idx = Some(
                        self.pass_last_dependency_pass_idx.map_or(
                            HistoryPassIdx(history, previous_buffer.write_pass_idx),
                            |last_dep| last_dep.max(HistoryPassIdx(history, previous_buffer.write_pass_idx)))
                    );
                    previous_buffer.last_used_in = previous_buffer.last_used_in.max(self.pass_idx);

                    previous_buffer.following_syncs |= stages;
                    previous_buffer.following_accesses |= access_kind.to_access();
                }
            } else {
                is_initial_write = true;
            }

            let new_handle = new_buffer.handle;
            self.buffers.push(new_buffer);

            // Rename buffer
            if !is_initial_write {
                if let Some(old_handle) = old_handle {
                    *old_handle = new_handle;
                }
            }
        } else {
            let old_handle = old_handle.expect("The first access to a resource must be a write.");

            let previous_buffer = self.buffers.get_mut(old_handle.0).unwrap();
            assert!(previous_buffer.next.is_none());
            self.pass_last_dependency_pass_idx = Some(
                self.pass_last_dependency_pass_idx.map_or(
                    HistoryPassIdx(history, previous_buffer.write_pass_idx),
                    |last_dep| last_dep.max(HistoryPassIdx(history, previous_buffer.write_pass_idx)))
            );

            previous_buffer.last_used_in = previous_buffer.last_used_in.max(self.pass_idx);
            previous_buffer.used_in_next_pass |= history == HistoryResourceEntry::Past;

            previous_buffer.following_syncs |= stages;
            previous_buffer.following_accesses |= access_kind.to_access();
        }
    }
}

struct AB<T> {
    a: T,
    b: Option<T>,
}

impl<T: Clone> Clone for AB<T> {
    fn clone(&self) -> Self {
        AB {
            a: self.a.clone(),
            b: self.b.clone(),
        }
    }
}
impl<T: Clone + Copy> Copy for AB<T> {}

struct BufferResource {
    buffers: SmallVec<[Arc<BufferSlice>; 2]>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum ABEntry {
    A,
    B,
}

impl<T> AB<T> {
    fn get(&self, ab: ABEntry) -> &T {
        if ab == ABEntry::B {
            if let Some(b) = &self.b {
                return b;
            }
        }
        &self.a
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct PassIdx(u32);

impl PassIdx {
    fn to_index(self) -> usize {
        self.0 as usize
    }

    fn min(self, other: Self) -> Self {
        if self < other {
            self
        } else {
            other
        }
    }

    fn max(self, other: Self) -> Self {
        if self < other {
            self
        } else {
            other
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct HistoryPassIdx(HistoryResourceEntry, PassIdx);

impl HistoryPassIdx {
    fn min(self, other: Self) -> Self {
        if self < other {
            self
        } else {
            other
        }
    }

    fn max(self, other: Self) -> Self {
        if self < other {
            self
        } else {
            other
        }
    }
}

struct ResourceHolder<T, THandle : Copy> {
    resource: AB<T>,
    writes: SmallVec<[Vec<ResourceWrite<THandle>>; 4]>,
    layouts: SmallVec<[TextureLayout; 4]>,
    flushed_write_idx: Option<PassIdx>,
}

struct RenderPassHolder {
    pass: Box<dyn RenderPass>,
}

#[derive(PartialEq, Eq)]
struct QueuedPass {
    pass_idx: PassIdx,
    submitted: bool,
    waited_for: bool,
    last_dependency_pass_idx: Option<PassIdx>,
}

impl PartialOrd for QueuedPass {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueuedPass {
    fn cmp(&self, other: &Self) -> Ordering {
        if let (Some(last_dep_idx), Some(other_last_dep_idx)) = (
            self.last_dependency_pass_idx,
            other.last_dependency_pass_idx,
        ) {
            last_dep_idx.cmp(&other_last_dep_idx)
        } else if self.last_dependency_pass_idx.is_some() {
            Ordering::Greater
        } else if other.last_dependency_pass_idx.is_some() {
            Ordering::Less
        } else if self == other {
            Ordering::Equal
        } else if self.submitted != other.submitted {
            self.submitted.cmp(&other.submitted)
        } else if self.waited_for != other.waited_for {
            self.waited_for.cmp(&other.waited_for)
        } else {
            self.pass_idx.cmp(&other.pass_idx)
        }
    }
}

const BUFFER_PAGE_SIZE: u32 = 16_384;

pub struct RenderGraph {
    device: Arc<Device>,
    passes: Vec<Box<dyn RenderPass>>,
    textures: HashMap<&'static str, ResourceHolder<Arc<Texture>, TextureHandle>>,
    buffers: HashMap<&'static str, ResourceHolder<Arc<BufferSlice>, BufferHandle>>,
    current_ab: ABEntry,

    // Builder
    texture_descriptions: HashMap<&'static str, ResourceDescription<TextureInfo>>,
    buffer_descriptions: HashMap<&'static str, ResourceDescription<(BufferInfo, MemoryUsage)>>,
    texture_accesses: Vec<TextureAccess>,
    buffer_accesses: Vec<BufferAccess>,
}

impl RenderGraph {
    pub fn add_pass<T: RenderPass + Default + 'static>(&mut self) {
        let pass: Box<dyn RenderPass> = Box::new(T::default());
        self.passes.push(pass);
    }

    fn create_resources(&mut self) {
        let bump = Bump::with_capacity(16_384_000);

        let mut texture_handle_map =
            HashMap::<&'static str, TextureHandle>::new();
        let mut buffer_handle_map =
            HashMap::<&'static str, BufferHandle>::new();

        let mut texture_descriptions = BumpVec::<ResourceDescription<TextureInfo>>::new_in(&bump);
        let mut buffer_descriptions =
            BumpVec::<ResourceDescription<(BufferInfo, MemoryUsage)>>::new_in(&bump);
        let mut textures = BumpVec::<ResourceWrite<TextureHandle>>::new_in(&bump);
        let mut buffers = BumpVec::<ResourceWrite<BufferHandle>>::new_in(&bump);

        // Determine required resources
        for (idx, pass) in self.passes.iter_mut().enumerate() {
            let mut context = FramePassResourceCreationContext {
                pass_idx: PassIdx(idx as u32),
                buffer_descriptions: &mut buffer_descriptions,
                texture_descriptions: &mut texture_descriptions,
                texture_metadata: &mut textures,
                buffer_metadata: &mut buffers,
                texture_handle_map: &mut texture_handle_map,
                buffer_handle_map: &mut buffer_handle_map,
            };

            pass.create_resources(&mut context);
        }

        let mut texture_accesses = BumpVec::<TextureAccess>::new_in(&bump);
        let mut buffer_accesses = BumpVec::<BufferAccess>::new_in(&bump);

        // Determine resource accesses
        for (idx, pass) in self.passes.iter_mut().enumerate() {
            let mut context = FramePassResourceAccessContext {
                pass_idx: PassIdx(idx as u32),
                pass_last_dependency_pass_idx: None,
                pass_texture_accesses: &mut texture_accesses,
                pass_buffer_accesses: &mut buffer_accesses,
                textures: &mut textures,
                buffers: &mut buffers,
                texture_handle_map: &mut texture_handle_map,
                buffer_handle_map: &mut buffer_handle_map,
            };

            pass.register_resource_accesses(&mut context);
        }

        // Build actual resources
        for texture_desc in &texture_descriptions {
            if texture_desc.has_history {
                // We cant alias history resources.
                continue;
            }

            let handle = texture_handle_map.get(texture_desc.name).unwrap();
            let mut last_used_in: PassIdx;
            let mut write_pass_idx: PassIdx;
            {
                let mut texture = textures.get(handle.0).unwrap();
                let mut last = texture;
                while let Some(next) = last.next {
                    last = textures.get(next.0).unwrap();
                }
                let mut first = texture;
                while let Some(previous) = first.previous {
                    first = textures.get(previous.0).unwrap();
                }
                write_pass_idx = first.write_pass_idx;
                last_used_in = last.last_used_in;
            }


            for texture_desc_b in &texture_descriptions {
                if texture_desc_b.name == texture_desc.name
                    || texture_desc_b.has_history
                    || texture_desc_b.info != texture_desc.info {
                    continue;
                }
                let handle_b = texture_handle_map.get(texture_desc_b.name).unwrap();
                let mut texture_b = textures.get(handle_b.0).unwrap();
                while let Some(next) = texture_b.next {
                    // Start at the last write.
                    texture_b = textures.get(next.0).unwrap();
                }
                while let Some(previous) = texture_b.previous {
                    let next_write_b = texture_b;
                    texture_b = textures.get(previous.0).unwrap();
                    if next_write_b.discard
                        && next_write_b.write_pass_idx > last_used_in
                        && texture_b.last_used_in < write_pass_idx {

                        texture_b.next = first_us;
                        tex

                        // Insert here
                    }
                }
            }

            let mut texture = textures.get(handle.0).unwrap();
            let last_used_in = texture.last_used_in;
            while let Some(inherits) = texture.previous {
                texture = textures.get(inherits.0).unwrap();
            }
            let last_used_in = texture.last_used_in;
        }

        for (&name, &texture) in &self.texture_descriptions {
            let mut texture_ab = AB {
                a: self
                    .device
                    .create_texture(&texture.info, Some(texture.name))
                    .unwrap(),
                b: None,
            };
            if texture.has_history {
                texture_ab.b = Some(
                    self.device
                        .create_texture(&texture.info, Some(texture.name))
                        .unwrap(),
                );
            }
            let num_subresources = (texture.info.array_length * texture.info.mip_levels) as usize;
            let mut write_vecs =
                SmallVec::<[Vec<ResourceWrite<TextureHandle>>; 4]>::with_capacity(num_subresources);
            write_vecs.resize(num_subresources, Vec::new());
            let mut texture_layouts =
                SmallVec::<[TextureLayout; 4]>::with_capacity(num_subresources);
            texture_layouts.resize(num_subresources, TextureLayout::Undefined);
            let existing = self.textures.insert(
                texture.name,
                ResourceHolder {
                    resource: texture_ab,
                    writes: write_vecs,
                    layouts: texture_layouts,
                    flushed_write_idx: None,
                },
            );
            if existing.is_some() {
                panic!("A texture with the name {} already exists.", texture.name);
            }
        }
        for (&name, &buffer) in &self.buffer_descriptions {
            let (buffer_info, memory_usage) = &buffer.info;
            let mut buffer_ab = AB {
                a: self
                    .device
                    .create_buffer(&buffer_info, *memory_usage, Some(buffer.name))
                    .unwrap(),
                b: None,
            };
            if buffer.has_history {
                buffer_ab.b = Some(
                    self.device
                        .create_buffer(&buffer_info, *memory_usage, Some(buffer.name))
                        .unwrap(),
                );
            }
            let num_subresources = ((buffer.info.0.size as u32) / BUFFER_PAGE_SIZE) as usize;
            let mut write_vecs =
                SmallVec::<[Vec<ResourceWrite<BufferHandle>>; 4]>::with_capacity(num_subresources);
            write_vecs.resize(num_subresources, Vec::new());
            let existing = self.buffers.insert(
                buffer.name,
                ResourceHolder {
                    resource: buffer_ab,
                    writes: write_vecs,
                    layouts: smallvec![TextureLayout::General],
                    flushed_write_idx: None,
                },
            );
            if existing.is_some() {
                panic!("A buffer with the name {} already exists.", buffer.name);
            }
        }
    }

    pub fn execute(&mut self, ctx: &GraphicsContext) {
        let mut queued_passes = Vec::<QueuedPass>::with_capacity(self.passes.len());
        for (idx, pass) in &mut self.passes.iter_mut().enumerate() {
            // Clear old resources access declarations and state

            // Collect new access declarations
            let mut context = FramePassResourceAccessContext {
                texture_accesses: &mut self.textures,
                buffer_accesses: &mut self.buffers,
                pass_idx: PassIdx(idx as u32),
                pass_last_dependency_pass_idx: None,
            };
            pass.register_resource_accesses(&mut context);
            queued_passes.push(QueuedPass {
                last_dependency_pass_idx: context.pass_last_dependency_pass_idx,
                pass_idx: context.pass_idx,
                submitted: false,
                waited_for: false,
            });
        }

        let mut executed_pass_idx = Option::<PassIdx>::None;
        let mut first_ready_pass = 0usize;
        let mut ready_pass_count = 0usize;
        let mut barrier_passes = 0usize;
        let mut barrier_passes_count = 0usize;

        while queued_passes.len() - first_ready_pass - ready_pass_count > 0 {}

        /*
        for (pass_idx, pass) in self.passes.iter().enumerate() {
            let mut is_ready = true;
            'textures: for texture_access in &pass.accessed_textures {
                let texture = self.textures.get(&texture_access.name).unwrap();
                if texture_access.kind.is_write() {
                    is_ready = is_ready && texture_access.last_write_idx.is_none();
                } else if let (Some(last_write_idx), Some(last_flushed_write_idx)) = (texture_access.last_write_idx, texture.flushed_write_idx) {
                    is_ready = is_ready && last_write_idx <= last_flushed_write_idx;
                } else if texture_access.last_write_idx.is_some() {
                    is_ready = false;
                }
                if !is_ready {
                    break 'textures;
                }
            }
            'buffers: for buffer_access in &pass.accessed_buffers {
                let buffer = self.buffers.get(&buffer_access.name).unwrap();
                if buffer_access.kind.is_write() {
                    is_ready = false;
                } else if let (Some(last_write_idx), Some(last_flushed_write_idx)) = (buffer_access.last_write_idx, buffer.flushed_write_idx) {
                    is_ready = is_ready && last_write_idx <= last_flushed_write_idx;
                } else if buffer_access.last_write_idx.is_some() {
                    is_ready = false;
                }
                if !is_ready {
                    break 'buffers;
                }
            }
        }*/

        let mut resource_availability =
            HashMap::<(&'static str, HistoryResourceEntry), ResourceAvailability>::new();
        fn add_or_extend_availability(
            resource_availability: &mut HashMap<
                (&'static str, HistoryResourceEntry),
                ResourceAvailability,
            >,
            resource_name: &'static str,
            history: HistoryResourceEntry,
            stages: BarrierSync,
            access: BarrierAccess,
        ) {
            let previous_access = resource_availability.get_mut(&(resource_name, history));
            if let Some(previous_access) = previous_access {
                if access.is_write() || previous_access.access.is_write() {
                    previous_access.access = access;
                    previous_access.stages = stages;
                } else {
                    previous_access.access |= access;
                    previous_access.stages |= stages;
                }
            } else {
                resource_availability.insert(
                    (resource_name, history),
                    ResourceAvailability {
                        stages,
                        access,
                        available_since_pass_idx: 0u32,
                    },
                );
            }
        }

        // Prepopulate availability for previous frame
        /* for pass in &self.passes {
            for texture in &pass.accessed_textures {
                let texture_resource = self.textures.get(&texture.name).unwrap();
                let has_ab = texture_resource.b.is_some();
                let history = if has_ab { texture.history.invert() } else { texture.history };
                add_or_extend_availability(&mut resource_availability, texture.name, history, texture.stages, texture.kind.to_access());
            }
            for buffer in &pass.accessed_buffers {
                let texture_resource = self.buffers.get(&buffer.name).unwrap();
                let has_ab = texture_resource.b.is_some();
                let history = if has_ab { buffer.history.invert() } else { buffer.history };
                add_or_extend_availability(&mut resource_availability, buffer.name, history, buffer.stages, buffer.kind.to_access());
            }
        }*/

        let mut first_ready_pass = 0usize;
        let mut ready_pass_count = 0usize;
        let mut barrier_passes = 0usize;
        let mut barrier_passes_count = 0usize;

        fn flush_ready_passes(passes: &mut [RenderPassHolder]) {
            passes.first().unwrap().pass.execute();
        }

        /*for (idx, pass) in self.passes.iter().enumerate() {
        let mut pass_ready = true;
        let mut has_write = false;
        for texture in &pass.accessed_textures {
            has_write |= texture.kind.is_write();
            let existing_availability = resource_availability.get_mut(&(texture.name, texture.history));
            if existing_availability.is_none() && texture.kind.can_discard() && texture.history == HistoryResourceEntry::Current {
                resource_availability.insert((texture.name, texture.history), ResourceAvailability {
                    stages: texture.stages,
                    access: texture.kind.to_access(),
                    available_since_pass_idx: 0u32,
                });
            } else {
                let existing_availability = existing_availability.unwrap();
                pass_ready = !texture.kind.is_write()
                    && !existing_availability.access.is_write()
                    && existing_availability.access.contains(texture.kind.to_access())
                    && existing_availability.stages.contains(texture.stages);
            }
        }
        if pass_ready {
            for buffer in &pass.accessed_textures {
                has_write |= buffer.kind.is_write();
                let existing_availability = resource_availability.get_mut(&(buffer.name, buffer.history));
                if existing_availability.is_none() && buffer.kind.can_discard() && buffer.history == HistoryResourceEntry::Current {
                    resource_availability.insert((buffer.name, buffer.history), ResourceAvailability {
                        stages: buffer.stages,
                        access: buffer.kind.to_access(),
                        available_since_pass_idx: 0u32,
                    });
                } else {
                    let existing_availability = existing_availability.unwrap();
                    pass_ready = !buffer.kind.is_write()
                        && !existing_availability.access.is_write()
                        && existing_availability.access.contains(buffer.kind.to_access())
                        && existing_availability.stages.contains(buffer.stages);
                }
            }
        }
        assert!(has_write);

        if pass_ready {
            pass.pass.execute();
        } else {
            for pass in &self.passes[barrier_passes..barrier_passes + barrier_passes_count] {

            }
        }*/

        /*resource_availability.clear();
            let mut pass_read_bloom = 0u64;
            let mut pass_write_bloom = 0u64;
            let mut hasher = DefaultHasher::new();
            for texture in &pass.accessed_textures {
                texture.name.hash(&mut hasher);
                let hash = hasher.finish();
                if texture.kind.is_write() {
                    pass_write_bloom |= hash % (std::mem::size_of_val(&write_bloom) as u64 / 8u64);
                    resource_availability.
                } else {
                    pass_read_bloom |= hash % (std::mem::size_of_val(&write_bloom) as u64 / 8u64);
                }
            }*


            // Find all passes after the current pass that can

        }*/
    }

    fn needs_barrier(&self, pass_a: &RenderPassHolder) -> bool {
        unimplemented!()
    }
}
