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

pub struct FramePassResourceCreationContext<'a> {
    pass_idx: PassIdx,
    textures: &'a mut BumpVec<'a, ResourceDescription<TextureInfo>>,
    buffers: &'a mut BumpVec<'a, ResourceDescription<(BufferInfo, MemoryUsage)>>,
    texture_metadata: &'a mut BumpVec<'a, ResourceMetadata<TextureHandle>>,
    buffer_metadata: &'a mut BumpVec<'a, ResourceMetadata<BufferHandle>>,
    texture_handle_map: &'a mut HashMap<&'static str, TextureHandle>,
    buffer_handle_map: &'a mut HashMap<&'static str, BufferHandle>,
}

#[derive(Debug, Clone)]
struct ResourceMetadata<THandle: Copy> {
    inherits: Option<THandle>,
    first_used_in: HistoryPassIdx,
    last_used_in: HistoryPassIdx,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
struct TextureHandle(usize);

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
struct BufferHandle(usize);

impl<'a> FramePassResourceCreationContext<'a> {
    pub fn create_texture(&mut self, name: &'static str, info: &TextureInfo, has_history: bool) {
        self.textures.push(ResourceDescription {
            name,
            info: info.clone(),
            has_history,
        });
        let history_pass_idx = HistoryPassIdx(HistoryResourceEntry::Current, self.pass_idx);
        let handle = TextureHandle(self.texture_metadata.len());
        self.texture_metadata
            .push(ResourceMetadata::<TextureHandle> {
                inherits: None,
                first_used_in: history_pass_idx,
                last_used_in: history_pass_idx,
            });
        self.texture_handle_map.insert(name, handle);
    }

    pub fn create_buffer(
        &mut self,
        name: &'static str,
        info: &BufferInfo,
        memory_usage: MemoryUsage,
        has_history: bool,
    ) {
        self.buffers.push(ResourceDescription {
            name,
            info: (info.clone(), memory_usage),
            has_history,
        });
        let history_pass_idx = HistoryPassIdx(HistoryResourceEntry::Current, self.pass_idx);
        let handle = BufferHandle(self.buffer_metadata.len());
        self.buffer_metadata.push(ResourceMetadata::<BufferHandle> {
            inherits: None,
            first_used_in: history_pass_idx,
            last_used_in: history_pass_idx,
        });
        self.buffer_handle_map.insert(name, handle);
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
    pass_idx: HistoryPassIdx,
    name: &'static str,
    kind: TextureAccessKind,
    range: BarrierTextureRange,
    stages: BarrierSync,
}

pub struct BufferAccess {
    pass_idx: HistoryPassIdx,
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

pub struct FramePassResourceAccessContext<'a> {
    pass_idx: PassIdx,
    texture_metadata: &'a mut BumpVec<'a, ResourceMetadata<TextureHandle>>,
    buffer_metadata: &'a mut BumpVec<'a, ResourceMetadata<BufferHandle>>,
    texture_accesses: &'a mut BumpVec<'a, TextureAccess>,
    buffer_accesses: &'a mut BumpVec<'a, BufferAccess>,
    texture_handle_map: &'a mut HashMap<&'static str, TextureHandle>,
    buffer_handle_map: &'a mut HashMap<&'static str, BufferHandle>,
}

pub struct BufferRange {
    pub offset: u64,
    pub len: u64,
}

#[derive(Clone)]
struct ResourceWrite {
    write_pass_idx: Option<PassIdx>,
    discard: bool,
    layout: TextureLayout,
    sync: BarrierSync,
    access: BarrierAccess,
    following_reads_layout: TextureLayout,
    following_reads_syncs: BarrierSync,
    following_reads_accesses: BarrierAccess,
    following_reads_last_pass_idx: Option<PassIdx>,
}

impl<'a> FramePassResourceAccessContext<'a> {
    pub fn register_texture_access(
        &mut self,
        name: &'static str,
        stages: BarrierSync,
        range: BarrierTextureRange,
        access_kind: TextureAccessKind,
        history: HistoryResourceEntry,
    ) {
        let history_pass_idx = HistoryPassIdx(history, self.pass_idx);
        self.texture_accesses.push(TextureAccess {
            pass_idx: history_pass_idx,
            name,
            kind: access_kind,
            range,
            stages,
        });

        let handle = self.texture_handle_map.get_mut(name).unwrap();
        let texture = if access_kind.is_write() {
            let new_handle = TextureHandle(self.texture_metadata.len());
            let new_texture = ResourceMetadata::<TextureHandle> {
                inherits: Some(*handle),
                first_used_in: history_pass_idx,
                last_used_in: history_pass_idx,
            };
            self.texture_metadata.push(new_texture);
            *handle = new_handle;
            self.texture_metadata.last_mut().unwrap()
        } else {
            self.texture_metadata.get_mut(handle.0).unwrap()
        };
        texture.first_used_in = texture.first_used_in.min(history_pass_idx);
        texture.last_used_in = texture.last_used_in.max(history_pass_idx);

        /*
        let texture = self.textures.get_mut(name).unwrap();

        if history == HistoryResourceEntry::Past && texture.resource.b.is_none() {
            panic!("Cannot access previous frame resource when resource isn't configured to be an AB resource.");
        }

        let mip_levels = texture.resource.a.info().mip_levels;
        let first_subresource = range.base_array_layer * mip_levels;
        let last_subresource = (range.base_array_layer + range.array_layer_length) * mip_levels + range.base_mip_level + range.mip_level_length;

        for i in first_subresource..last_subresource {
            let writes = &mut texture.writes[i as usize];
            if access_kind.is_write() {
                if history == HistoryResourceEntry::Past {
                    panic!("Writing to the previous frame resource is not allowed.");
                }

                if let Some(previous_write_dep_idx) = writes.last().and_then(|previous_write| previous_write.following_reads_last_pass_idx.or_else(|| previous_write.write_pass_idx)) {
                    if let Some(old_dep_pass_idx) = self.last_dependency_pass_idx.as_mut() {
                        *old_dep_pass_idx = PassIdx(old_dep_pass_idx.0.max(previous_write_dep_idx.0));
                    } else {
                        self.last_dependency_pass_idx = Some(previous_write_dep_idx);
                    }
                }

                writes.push(ResourceWrite {
                    write_pass_idx: Some(self.pass_idx),
                    discard: access_kind.can_discard(),
                    layout: access_kind.to_layout(),
                    sync: stages,
                    access: access_kind.to_access(),
                    following_reads_layout: TextureLayout::Undefined,
                    following_reads_syncs: BarrierSync::empty(),
                    following_reads_accesses: BarrierAccess::empty(),
                    following_reads_last_pass_idx: None,
                });
            } else {
                if writes.is_empty() {
                    writes.push(ResourceWrite {
                        write_pass_idx: None,
                        discard: access_kind.can_discard(),
                        layout: access_kind.to_layout(),
                        sync: stages,
                        access: access_kind.to_access(),
                        following_reads_layout: TextureLayout::Undefined,
                        following_reads_syncs: BarrierSync::empty(),
                        following_reads_accesses: BarrierAccess::empty(),
                        following_reads_last_pass_idx: None,
                    });
                }

                let write = writes.last_mut().unwrap();
                if let Some(previous_write_dep_idx) = write.write_pass_idx.as_ref() {
                    if let Some(old_dep_pass_idx) = self.last_dependency_pass_idx.as_mut() {
                        *old_dep_pass_idx = PassIdx(old_dep_pass_idx.0.max(previous_write_dep_idx.0));
                    } else {
                        self.last_dependency_pass_idx = Some(*previous_write_dep_idx);
                    }
                }
                write.following_reads_last_pass_idx = Some(write.following_reads_last_pass_idx.map_or(self.pass_idx, |idx| idx.max(self.pass_idx)));
                write.following_reads_syncs |= stages;
                write.following_reads_accesses |= access_kind.to_access();
                let new_layout = access_kind.to_layout();
                if write.following_reads_layout == TextureLayout::Undefined {
                    write.following_reads_layout = new_layout;
                } else if (write.following_reads_layout == TextureLayout::DepthStencilRead
                    && new_layout == TextureLayout::Sampled)
                    || (write.following_reads_layout == TextureLayout::Sampled
                    && new_layout == TextureLayout::DepthStencilRead) {
                    write.following_reads_layout = TextureLayout::DepthStencilRead;
                } else if write.following_reads_layout != new_layout {
                    write.following_reads_layout = TextureLayout::General;
                }
            }
        }*/
    }

    pub fn register_buffer_access(
        &mut self,
        name: &'static str,
        stages: BarrierSync,
        range: BufferRange,
        access_kind: BufferAccessKind,
        history: HistoryResourceEntry,
    ) {
        let history_pass_idx = HistoryPassIdx(history, self.pass_idx);
        self.buffer_accesses.push(BufferAccess {
            pass_idx: history_pass_idx,
            name,
            kind: access_kind,
            range,
            stages,
        });

        let handle = self.buffer_handle_map.get_mut(name).unwrap();
        let buffer = if access_kind.is_write() {
            let new_handle = BufferHandle(self.buffer_metadata.len());
            let new_buffer = ResourceMetadata::<BufferHandle> {
                inherits: Some(*handle),
                first_used_in: history_pass_idx,
                last_used_in: history_pass_idx,
            };
            self.buffer_metadata.push(new_buffer);
            *handle = new_handle;
            self.buffer_metadata.last_mut().unwrap()
        } else {
            self.buffer_metadata.get_mut(handle.0).unwrap()
        };
        buffer.first_used_in = buffer.first_used_in.min(history_pass_idx);
        buffer.last_used_in = buffer.last_used_in.max(history_pass_idx);

        /*let buffer = self.buffers.get_mut(name).unwrap();

        if history == HistoryResourceEntry::Past && buffer.resource.b.is_none() {
            panic!("Cannot access previous frame resource when resource isn't configured to be an AB resource.");
        }

        let first_subresource = range.offset / (BUFFER_PAGE_SIZE as u64);
        let last_subresource = (range.offset + range.len + (BUFFER_PAGE_SIZE as u64) - 1u64) / (BUFFER_PAGE_SIZE as u64);
        for i in first_subresource..last_subresource {
            let writes = &mut buffer.writes[i as usize];
            if access_kind.is_write() {
                if history == HistoryResourceEntry::Past {
                    panic!("Writing to the previous frame resource is not allowed.");
                }

                if let Some(previous_write_dep_idx) = writes.last().and_then(|previous_write| previous_write.following_reads_last_pass_idx.or_else(|| previous_write.write_pass_idx)) {
                    if let Some(old_dep_pass_idx) = self.last_dependency_pass_idx.as_mut() {
                        *old_dep_pass_idx = PassIdx(old_dep_pass_idx.0.max(previous_write_dep_idx.0));
                    } else {
                        self.last_dependency_pass_idx = Some(previous_write_dep_idx);
                    }
                }

                writes.push(ResourceWrite {
                    write_pass_idx: Some(self.pass_idx),
                    discard: false,
                    layout: TextureLayout::General,
                    sync: stages,
                    access: access_kind.to_access(),
                    following_reads_layout: TextureLayout::General,
                    following_reads_syncs: BarrierSync::empty(),
                    following_reads_accesses: BarrierAccess::empty(),
                    following_reads_last_pass_idx: None,
                });
            } else {
                if writes.is_empty() {
                    // We're reading data that hasn't been written yet.
                    writes.push(ResourceWrite {
                        write_pass_idx: None,
                        discard: false,
                        layout: TextureLayout::General,
                        sync: stages,
                        access: access_kind.to_access(),
                        following_reads_layout: TextureLayout::General,
                        following_reads_syncs: BarrierSync::empty(),
                        following_reads_accesses: BarrierAccess::empty(),
                        following_reads_last_pass_idx: None,
                    });
                }

                let write = writes.last_mut().unwrap();
                if let Some(previous_write_dep_idx) = write.write_pass_idx.as_ref() {
                    if let Some(old_dep_pass_idx) = self.last_dependency_pass_idx.as_mut() {
                        *old_dep_pass_idx = PassIdx(old_dep_pass_idx.0.max(previous_write_dep_idx.0));
                    } else {
                        self.last_dependency_pass_idx = Some(*previous_write_dep_idx);
                    }
                }
                write.following_reads_last_pass_idx = Some(write.following_reads_last_pass_idx.map_or(self.pass_idx, |idx| idx.max(self.pass_idx)));
                write.following_reads_syncs |= stages;
                write.following_reads_accesses |= access_kind.to_access();
            }
        }*/
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

struct ResourceHolder<T> {
    resource: AB<T>,
    writes: SmallVec<[Vec<ResourceWrite>; 4]>,
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
    textures: HashMap<&'static str, ResourceHolder<Arc<Texture>>>,
    buffers: HashMap<&'static str, ResourceHolder<Arc<BufferSlice>>>,
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

        let mut texture_handle_map = HashMap::<&'static str, TextureHandle>::new();
        let mut buffer_handle_map = HashMap::<&'static str, BufferHandle>::new();

        let mut textures = BumpVec::<ResourceDescription<TextureInfo>>::new_in(&bump);
        let mut buffers = BumpVec::<ResourceDescription<(BufferInfo, MemoryUsage)>>::new_in(&bump);
        let mut texture_metadata = BumpVec::<ResourceMetadata<TextureHandle>>::new_in(&bump);
        let mut buffer_metadata = BumpVec::<ResourceMetadata<BufferHandle>>::new_in(&bump);

        // Determine required resources
        for (idx, pass) in self.passes.iter_mut().enumerate() {
            let mut context = FramePassResourceCreationContext {
                pass_idx: PassIdx(idx as u32),
                buffers: &mut buffers,
                textures: &mut textures,
                texture_metadata: &mut texture_metadata,
                buffer_metadata: &mut buffer_metadata,
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
                texture_accesses: &mut texture_accesses,
                buffer_accesses: &mut buffer_accesses,
                texture_metadata: &mut texture_metadata,
                buffer_metadata: &mut buffer_metadata,
                texture_handle_map: &mut texture_handle_map,
                buffer_handle_map: &mut buffer_handle_map,
            };

            pass.register_resource_accesses(&mut context);
        }

        // Build actual resources
        for (name, handle) in texture_handle_map {
            let mut texture_meta = texture_metadata.get(handle.0).unwrap();
            let last_used_in = texture_meta.last_used_in;
            while let Some(inherits) = texture_meta.inherits {
                texture_meta = texture_metadata.get(inherits.0).unwrap();
            }
            let first_used_in = texture_meta.first_used_in;
        }

        for texture in &context.textures {
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
                SmallVec::<[Vec<ResourceWrite>; 4]>::with_capacity(num_subresources);
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
        for buffer in &context.buffers {
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
                SmallVec::<[Vec<ResourceWrite>; 4]>::with_capacity(num_subresources);
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
                last_dependency_pass_idx: None,
            };
            pass.register_resource_accesses(&mut context);
            queued_passes.push(QueuedPass {
                last_dependency_pass_idx: context.last_dependency_pass_idx,
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
