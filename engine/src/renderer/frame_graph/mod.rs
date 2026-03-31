use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::ptr::{read, NonNull};
use std::sync::Arc;

use crate::graphics::{
    Barrier, BarrierAccess, BarrierSync, BarrierTextureRange, BufferInfo, BufferRef, BufferSlice,
    ClearColor, ClearDepthStencilValue, CommandBuffer, Device, GraphicsContext, MemoryUsage,
    QueueType, Texture, TextureInfo, TextureLayout, TextureView,
};
use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut};
use bitvec::domain::PartialElement;
use bumpalo::collections::{String as BumpString, Vec as BumpVec};
use bumpalo::Bump;
use smallvec::{smallvec, SmallVec};
use sourcerenderer_core::gpu::TextureViewInfo;

pub trait RenderPass {
    fn create_resources<'a>(&mut self, builder: &'a mut RenderPassResourceCreationContext<'a>);
    fn register_resource_accesses<'a, 'b: 'a>(
        &mut self,
        builder: &'a mut RenderPassResourceAccessContext<'b>,
    );

    fn execute(&self, resources: &RenderPassExecuteContext);
}

enum BuiltResource<T> {
    Undecided,
    Resource(T),
    EmbeddedInto(&'static str),
}

struct ResourceDescription<TInfo: Clone, THandle: Handle, TResource> {
    name: &'static str,
    info: TInfo,
    merged_info: TInfo,
    last_handle: THandle,
    resource: BuiltResource<TResource>,
    history_type: HistoryType,
}

impl<TInfo: Clone, THandle: Handle, TResource> ResourceDescription<TInfo, THandle, TResource> {
    fn clone_take_resource(&mut self) -> Self {
        Self {
            name: self.name,
            info: self.info.clone(),
            merged_info: self.merged_info.clone(),
            last_handle: self.last_handle,
            history_type: self.history_type,
            resource: std::mem::replace(&mut self.resource, BuiltResource::Undecided),
        }
    }
}

pub struct RenderPassResourceCreationContext<'a> {
    device: &'a Device,
    pass_idx: PassIdx,
    textures: &'a mut HashMap<
        &'static str,
        ResourceDescription<TextureInfo, TextureHandle, Arc<Texture>>,
    >,
    buffers: &'a mut HashMap<
        &'static str,
        ResourceDescription<(BufferInfo, MemoryUsage), BufferHandle, Arc<BufferSlice>>,
    >,
    texture_writes: &'a mut Vec<ResourceWrite<TextureHandle>>,
    buffer_writes: &'a mut Vec<ResourceWrite<BufferHandle>>,
}

pub struct RenderPassExecuteContext<'a, 'b: 'a> {
    cmd_buffer: &'a mut CommandBuffer<'b>,
    textures: &'a HashMap<&'static str, Arc<TextureView>>,
    buffers: &'a HashMap<&'static str, (Arc<BufferSlice>, BufferRange)>,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
struct TextureHandle(usize);

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
struct BufferHandle(usize);

trait Handle: PartialEq + Eq + Copy {
    fn to_index(self) -> usize;
    fn from_index(index: usize) -> Self;
}

impl Handle for TextureHandle {
    fn to_index(self) -> usize {
        self.0
    }

    fn from_index(index: usize) -> Self {
        Self(index)
    }
}

impl Handle for BufferHandle {
    fn to_index(self) -> usize {
        self.0
    }

    fn from_index(index: usize) -> Self {
        Self(index)
    }
}

impl<'a> RenderPassResourceCreationContext<'a> {
    pub fn create_texture(&mut self, name: &'static str, info: &TextureInfo) {
        // Write 0 represents the last write of the previous frame.
        // It'll get removed later if it's never read.
        let handle = TextureHandle(self.texture_writes.len());
        self.texture_writes.push(ResourceWrite::<TextureHandle> {
            previous: None,
            next: None,
            write_pass_idx: None,
            discard: false,
            layout: TextureLayout::Undefined,
            sync: BarrierSync::empty(),
            access: BarrierAccess::empty(),
            following_reads_layout: TextureLayout::Undefined,
            following_reads_syncs: BarrierSync::empty(),
            following_reads_accesses: BarrierAccess::empty(),
            following_reads_first_pass_idx: None,
            following_reads_last_pass_idx: None,
            following_reads_first_baked_pass_idx: None,
            following_reads_last_baked_pass_idx: None,
        });
        self.textures.insert(
            name,
            ResourceDescription {
                name,
                info: info.clone(),
                merged_info: info.clone(),
                history_type: HistoryType::None,
                resource: BuiltResource::Undecided,
                last_handle: handle,
            },
        );
    }

    pub fn create_buffer(
        &mut self,
        name: &'static str,
        info: &BufferInfo,
        memory_usage: MemoryUsage,
    ) {
        // Write 0 represents the last write of the previous frame.
        // It'll get removed later if it's never read.
        let handle = BufferHandle(self.buffer_writes.len());
        self.buffer_writes.push(ResourceWrite::<BufferHandle> {
            previous: None,
            next: None,
            write_pass_idx: None,
            discard: false,
            layout: TextureLayout::Undefined,
            sync: BarrierSync::empty(),
            access: BarrierAccess::empty(),
            following_reads_layout: TextureLayout::Undefined,
            following_reads_syncs: BarrierSync::empty(),
            following_reads_accesses: BarrierAccess::empty(),
            following_reads_first_pass_idx: None,
            following_reads_last_pass_idx: None,
            following_reads_first_baked_pass_idx: None,
            following_reads_last_baked_pass_idx: None,
        });
        self.buffers.insert(
            name,
            ResourceDescription {
                name,
                info: (info.clone(), memory_usage),
                merged_info: (info.clone(), memory_usage),
                history_type: HistoryType::None,
                resource: BuiltResource::Undecided,
                last_handle: handle,
            },
        );
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

#[derive(Copy, Clone, PartialEq, Debug)]
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

    fn discards(self) -> bool {
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

    fn is_compatible(self, sync: BarrierSync) -> bool {
        match self {
            TextureAccessKind::Sampling
            | TextureAccessKind::StorageRead
            | TextureAccessKind::StorageWrite
            | TextureAccessKind::StorageWriteEntireSubresource
            | TextureAccessKind::StorageReadWrite => (sync
                & !(BarrierSync::FRAGMENT_SHADER
                    | BarrierSync::VERTEX_SHADER
                    | BarrierSync::COMPUTE_SHADER
                    | BarrierSync::MESH_SHADER))
                .is_empty(),
            TextureAccessKind::RenderTargetCleared(_) | TextureAccessKind::RenderTarget => {
                sync == BarrierSync::RENDER_TARGET
            }
            TextureAccessKind::DepthStencilCleared(_)
            | TextureAccessKind::DepthStencil
            | TextureAccessKind::DepthStencilReadOnly => {
                (sync & !(BarrierSync::EARLY_DEPTH | BarrierSync::LATE_DEPTH)).is_empty()
            }
            TextureAccessKind::BlitSrc
            | TextureAccessKind::BlitDst
            | TextureAccessKind::BlitDstEntireSubresource => {
                (sync & !(BarrierSync::BLIT | BarrierSync::COPY)).is_empty()
            }
            TextureAccessKind::CopySrc
            | TextureAccessKind::CopyDst
            | TextureAccessKind::CopyDstEntireSubresource => sync == BarrierSync::COPY,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum BufferAccessKind {
    ShaderRead,
    ShaderWrite,
    ShaderWriteEntireSubresource,
    ShaderReadWrite,
    CopySrc,
    CopyDst,
    CopyDstEntireSubresource,
}

impl BufferAccessKind {
    fn is_write(self) -> bool {
        match self {
            BufferAccessKind::ShaderReadWrite
            | BufferAccessKind::ShaderWrite
            | BufferAccessKind::CopyDstEntireSubresource
            | BufferAccessKind::ShaderWriteEntireSubresource
            | BufferAccessKind::CopyDst => true,
            _ => false,
        }
    }

    fn discards(self) -> bool {
        match self {
            BufferAccessKind::CopyDstEntireSubresource
            | BufferAccessKind::ShaderWriteEntireSubresource => true,
            _ => false,
        }
    }

    fn to_access(self) -> BarrierAccess {
        match self {
            BufferAccessKind::ShaderRead => BarrierAccess::SHADER_READ,
            BufferAccessKind::ShaderWrite | BufferAccessKind::ShaderWriteEntireSubresource => {
                BarrierAccess::SHADER_WRITE
            }
            BufferAccessKind::ShaderReadWrite => {
                BarrierAccess::SHADER_READ | BarrierAccess::SHADER_WRITE
            }
            BufferAccessKind::CopySrc => BarrierAccess::COPY_READ,
            BufferAccessKind::CopyDst | BufferAccessKind::CopyDstEntireSubresource => {
                BarrierAccess::COPY_WRITE
            }
        }
    }

    fn is_compatible(self, sync: BarrierSync) -> bool {
        match self {
            BufferAccessKind::ShaderRead
            | BufferAccessKind::ShaderWrite
            | BufferAccessKind::ShaderReadWrite
            | BufferAccessKind::ShaderWriteEntireSubresource => (sync
                & !(BarrierSync::FRAGMENT_SHADER
                    | BarrierSync::VERTEX_SHADER
                    | BarrierSync::COMPUTE_SHADER
                    | BarrierSync::MESH_SHADER))
                .is_empty(),
            BufferAccessKind::CopySrc
            | BufferAccessKind::CopyDst
            | BufferAccessKind::CopyDstEntireSubresource => sync == BarrierSync::COPY,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct TextureAccess {
    pass_idx: PassIdx,
    name: &'static str,
    handle: TextureHandle,
    kind: TextureAccessKind,
    range: BarrierTextureRange,
    sync: BarrierSync,
    history: HistoryResourceEntry,
    write_pass_idx: PassIdx,
}

#[derive(Debug, Eq, PartialEq)]
pub struct BufferAccess {
    pass_idx: PassIdx,
    name: &'static str,
    handle: BufferHandle,
    kind: BufferAccessKind,
    range: BufferRange,
    sync: BarrierSync,
    history: HistoryResourceEntry,
    write_pass_idx: PassIdx,
}

pub struct ResourceAvailability {
    stages: BarrierSync,
    access: BarrierAccess,
    available_since_pass_idx: u32,
}

pub struct RenderPassResourceAccessContext<'a> {
    device: &'a Device,
    pass_idx: PassIdx,
    texture_accesses: &'a mut Vec<TextureAccess>,
    buffer_accesses: &'a mut Vec<BufferAccess>,
    texture_writes: &'a mut Vec<ResourceWrite<TextureHandle>>,
    buffer_writes: &'a mut Vec<ResourceWrite<BufferHandle>>,
    textures: &'a mut HashMap<
        &'static str,
        ResourceDescription<TextureInfo, TextureHandle, Arc<Texture>>,
    >,
    buffers: &'a mut HashMap<
        &'static str,
        ResourceDescription<(BufferInfo, MemoryUsage), BufferHandle, Arc<BufferSlice>>,
    >,
    pass_texture_accesses_range: Range<usize>,
    pass_buffer_accesses_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferRange {
    pub offset: u64,
    pub len: u64,
}

#[derive(Clone)]
struct ResourceWrite<THandle: Handle> {
    previous: Option<THandle>,
    next: Option<THandle>,
    write_pass_idx: Option<PassIdx>,
    discard: bool,
    layout: TextureLayout,
    sync: BarrierSync,
    access: BarrierAccess,
    following_reads_layout: TextureLayout,
    following_reads_syncs: BarrierSync,
    following_reads_accesses: BarrierAccess,
    following_reads_first_pass_idx: Option<PassIdx>,
    following_reads_last_pass_idx: Option<PassIdx>,
    following_reads_first_baked_pass_idx: Option<BakedPassIdx>,
    following_reads_last_baked_pass_idx: Option<BakedPassIdx>,
}

impl<'a> RenderPassResourceAccessContext<'a> {
    fn register_resource_access<'b, TInfo: Clone, THandle: Handle, TResource>(
        device: &Device,
        pass_idx: PassIdx,
        name: &'static str,
        sync: BarrierSync,
        access: BarrierAccess,
        layout: TextureLayout,
        discard: bool,
        history: HistoryResourceEntry,
        handle_map: &mut HashMap<&'static str, ResourceDescription<TInfo, THandle, TResource>>,
        writes: &'b mut Vec<ResourceWrite<THandle>>,
    ) -> &'b mut ResourceWrite<THandle> {
        let mut handle = handle_map.get_mut(&name).unwrap().last_handle;
        if access.is_write() {
            if history == HistoryResourceEntry::Past {
                panic!("Writing to the previous frame resource is not allowed.");
            }

            let new_write = ResourceWrite::<THandle> {
                next: None,
                previous: Some(handle),
                write_pass_idx: Some(pass_idx),
                discard,
                layout,
                sync,
                access,
                following_reads_layout: TextureLayout::Undefined,
                following_reads_syncs: BarrierSync::empty(),
                following_reads_accesses: BarrierAccess::empty(),
                following_reads_first_pass_idx: None,
                following_reads_last_pass_idx: None,
                following_reads_first_baked_pass_idx: None,
                following_reads_last_baked_pass_idx: None,
            };

            let new_handle = THandle::from_index(writes.len());
            writes.push(new_write);
            handle = new_handle;
            writes.get_mut(handle.to_index()).unwrap()
        } else {
            if history == HistoryResourceEntry::Past {
                // The first write represents the previous frame
                let mut write = &writes[handle.to_index()];
                while let Some(previous_handle) = write.previous {
                    handle = previous_handle;
                    write = &writes[handle.to_index()];
                }
            }

            let previous_write = writes.get_mut(handle.to_index()).unwrap();
            assert!(previous_write.next.is_none());
            assert!(
                history == HistoryResourceEntry::Past || previous_write.write_pass_idx.is_some()
            ); // the placeholder write is only valid for reading the past frame resource
            previous_write.following_reads_last_pass_idx = previous_write
                .following_reads_last_pass_idx
                .max(Some(pass_idx));
            previous_write.following_reads_first_pass_idx = Some(
                previous_write
                    .following_reads_first_pass_idx
                    .map_or(pass_idx, |pass_idx| pass_idx.min(pass_idx)),
            );
            previous_write.following_reads_syncs |= sync;
            previous_write.following_reads_accesses |= access;
            previous_write.following_reads_layout =
                TextureLayout::merge_layouts(previous_write.following_reads_layout, layout);
            previous_write
        }
    }

    pub fn register_texture_access(
        &mut self,
        name: &'static str,
        sync: BarrierSync,
        range: BarrierTextureRange,
        access_kind: TextureAccessKind,
        history: HistoryResourceEntry,
    ) {
        assert!(access_kind.is_compatible(sync));

        let mut handle = self.textures.get(name).unwrap().last_handle;
        if history == HistoryResourceEntry::Past {
            // The first element represents the last write of the previous frame.
            handle = RenderGraph::find_first_and_last_usage(handle, self.texture_writes)
                .first_use_handle;
        }

        let write = Self::register_resource_access(
            self.device,
            self.pass_idx,
            name,
            sync,
            access_kind.to_access(),
            access_kind.to_layout(),
            access_kind.discards(),
            history,
            self.textures,
            self.texture_writes,
        );

        self.pass_texture_accesses_range.start = self
            .pass_texture_accesses_range
            .start
            .min(self.texture_accesses.len());
        self.texture_accesses.push(TextureAccess {
            pass_idx: self.pass_idx,
            name,
            handle,
            kind: access_kind,
            range,
            sync,
            history,
            write_pass_idx: write.write_pass_idx.unwrap(),
        });
        self.pass_texture_accesses_range.end = self
            .pass_texture_accesses_range
            .start
            .max(self.texture_accesses.len());
    }

    pub fn register_buffer_access(
        &mut self,
        name: &'static str,
        sync: BarrierSync,
        range: BufferRange,
        access_kind: BufferAccessKind,
        history: HistoryResourceEntry,
    ) {
        assert!(access_kind.is_compatible(sync));

        let mut handle = self.buffers.get(name).unwrap().last_handle;
        if history == HistoryResourceEntry::Past {
            // The first element represents the last write of the previous frame.
            handle =
                RenderGraph::find_first_and_last_usage(handle, self.buffer_writes).first_use_handle;
        }

        let write = Self::register_resource_access(
            self.device,
            self.pass_idx,
            name,
            sync,
            access_kind.to_access(),
            TextureLayout::General,
            access_kind.discards(),
            history,
            self.textures,
            self.texture_writes,
        );

        self.pass_buffer_accesses_range.start = self
            .pass_buffer_accesses_range
            .start
            .min(self.buffer_accesses.len());
        self.buffer_accesses.push(BufferAccess {
            pass_idx: self.pass_idx,
            name,
            handle,
            kind: access_kind,
            range,
            sync,
            history,
            write_pass_idx: write.write_pass_idx.unwrap(),
        });
        self.pass_buffer_accesses_range.end = self
            .pass_buffer_accesses_range
            .start
            .max(self.buffer_accesses.len());
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
struct BakedPassIdx(u32);

impl BakedPassIdx {
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

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
enum Insert {
    Before,
    After,
}

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
enum HistoryType {
    None,
    SingleResource,
    AB,
}

#[derive(Clone)]
struct BakedRenderPass {
    pass_idx: PassIdx,
    texture_accesses: Range<usize>,
    buffer_accesses: Range<usize>,
    textures: HashMap<&'static str, Arc<TextureView>>,
    buffers: HashMap<&'static str, (Arc<BufferSlice>, BufferRange)>,
}

pub struct RenderGraph {
    device: Arc<Device>,
    passes: Vec<Box<dyn RenderPass>>,
    current_ab: ABEntry,

    // Built
    built_passes: Vec<BakedRenderPass>,
    texture_accesses: Vec<TextureAccess>,
    buffer_accesses: Vec<BufferAccess>,
    textures: HashMap<&'static str, ResourceDescription<TextureInfo, TextureHandle, Arc<Texture>>>,
    buffers: HashMap<
        &'static str,
        ResourceDescription<(BufferInfo, MemoryUsage), BufferHandle, Arc<BufferSlice>>,
    >,
    texture_writes: Vec<ResourceWrite<TextureHandle>>,
    buffer_writes: Vec<ResourceWrite<BufferHandle>>,
}

#[derive(Debug, Clone)]
struct ResourceChainInfo<THandle: Handle> {
    first_use_handle: THandle,
    last_use_handle: THandle,
    first_used_in: PassIdx,
    last_used_in: PassIdx,
    history_last_used_in: Option<PassIdx>,
}

impl RenderGraph {
    pub fn add_pass<T: RenderPass + Default + 'static>(&mut self) {
        let pass: Box<dyn RenderPass> = Box::new(T::default());
        self.passes.push(pass);
    }

    #[inline]
    fn find_first_and_last_usage<THandle: Handle>(
        handle: THandle,
        resources: &[ResourceWrite<THandle>],
    ) -> ResourceChainInfo<THandle> {
        let mut resource = resources.get(handle.to_index()).unwrap();
        let mut first_used_in = resource.write_pass_idx.unwrap_or(PassIdx(0u32));
        let mut last_used_in = resource
            .following_reads_last_pass_idx
            .unwrap_or(PassIdx(u32::MAX));
        while let Some(next) = resource.next {
            resource = &resources[next.to_index()];
            last_used_in = last_used_in.max(resource.following_reads_last_pass_idx.unwrap());
        }
        let last_use_handle = handle;
        while let Some(previous) = resource.previous {
            resource = &resources[previous.to_index()];
            first_used_in = first_used_in.max(resource.write_pass_idx.unwrap());
        }
        let first_use_handle = handle;
        ResourceChainInfo {
            first_use_handle,
            last_use_handle,
            first_used_in,
            last_used_in,
            history_last_used_in: resources[first_use_handle.to_index()]
                .following_reads_last_pass_idx,
        }
    }

    fn try_alias_resource<THandle: Handle>(
        resource_chain_info: &ResourceChainInfo<THandle>,
        mut handle_b: THandle,
        writes: &mut [ResourceWrite<THandle>],
    ) -> bool {
        let ResourceChainInfo {
            first_used_in,
            last_used_in,
            first_use_handle,
            last_use_handle,
            history_last_used_in: _,
        } = resource_chain_info.clone();

        let ResourceChainInfo {
            first_used_in: _,
            last_used_in: _,
            first_use_handle: _,
            last_use_handle: last_use_handle_b,
            history_last_used_in: history_last_used_in_b,
        } = Self::find_first_and_last_usage(handle_b, writes);

        let mut insert_location = Option::<(THandle, Insert)>::None;
        handle_b = last_use_handle_b;
        let mut write_b = &writes[handle_b.to_index()];

        // Try to insert it after the last write
        if insert_location.is_none()
            && write_b.write_pass_idx < Some(first_used_in)
            && history_last_used_in_b.is_none()
        {
            // There's no write after this and this write isn't being read in the next frame.
            insert_location = Some((handle_b, Insert::After));
        }

        while let Some(previous) = write_b.previous {
            handle_b = previous;
            let next_write_b = write_b;
            write_b = &writes[previous.to_index()];
            if next_write_b.discard
                && next_write_b.write_pass_idx > Some(last_used_in)
                && write_b.following_reads_last_pass_idx < Some(first_used_in)
                && (history_last_used_in_b
                    .map_or(true, |last_used_in| last_used_in < first_used_in))
            {
                // Insert it in between those two writes.
                // Only possible if its not a history resource or it's after the last access
                // of the previous frames contents
                insert_location = Some((handle_b, Insert::After));
            }
        }

        // Try to insert it before the first write
        if insert_location.is_none()
            && write_b.discard
            && write_b.write_pass_idx.is_some()
            && write_b.write_pass_idx.unwrap() > last_used_in
        {
            // Make sure it's not reading the previous-frame contents.
            insert_location = Some((handle_b, Insert::Before));
        }

        match insert_location {
            None => false,
            Some((idx, Insert::Before)) => {
                let mut next_clone = writes.get_mut(idx.to_index()).unwrap().clone();

                {
                    let insert_first = writes.get_mut(first_use_handle.to_index()).unwrap();
                    insert_first.previous = next_clone.previous;
                }
                {
                    let insert_last = writes.get_mut(last_use_handle.to_index()).unwrap();
                    insert_last.next = Some(idx);
                }

                if let Some(previous_handle) = next_clone.previous {
                    let previous = writes.get_mut(previous_handle.to_index()).unwrap();
                    previous.next = Some(first_use_handle);
                }

                next_clone.previous = Some(last_use_handle);
                writes[idx.to_index()] = next_clone;
                true
            }
            Some((idx, Insert::After)) => {
                let mut previous_clone = writes.get_mut(idx.to_index()).unwrap().clone();

                {
                    let insert_first = writes.get_mut(first_use_handle.to_index()).unwrap();
                    insert_first.previous = Some(idx);
                }
                {
                    let insert_last = writes.get_mut(last_use_handle.to_index()).unwrap();
                    insert_last.next = previous_clone.next;
                }

                if let Some(next_handle) = previous_clone.next {
                    let next = writes.get_mut(next_handle.to_index()).unwrap();
                    next.previous = Some(last_use_handle);
                }

                previous_clone.next = Some(first_use_handle);
                writes[idx.to_index()] = previous_clone;
                true
            }
        }
    }

    fn identify_history_accesses<THandle: Handle>(
        resource_handle: THandle,
        resources: &mut [ResourceWrite<THandle>],
    ) -> HistoryType {
        let chain_info = Self::find_first_and_last_usage(resource_handle, &resources);

        let first_resource_mut = &mut resources[chain_info.first_use_handle.to_index()];
        let mut first_resource_clone = first_resource_mut.clone();
        let first_resource: &ResourceWrite<THandle> =
            &resources[chain_info.first_use_handle.to_index()];

        let next_handle = first_resource.next.expect("Resource is never written");
        if first_resource.following_reads_last_pass_idx.is_none() {
            // Remove write 0 from the chain
            resources[next_handle.to_index()].previous = None;
            return HistoryType::None;
        }

        assert!(!first_resource.following_reads_accesses.is_empty());
        assert!(!first_resource.following_reads_syncs.is_empty());
        assert!(!first_resource.following_reads_last_pass_idx.is_some());

        let next_resource = &resources[next_handle.to_index()];
        let write_pass_idx = next_resource.write_pass_idx.unwrap();
        let history_access_type: HistoryType = if next_resource.discard
            && write_pass_idx > first_resource.following_reads_last_pass_idx.unwrap()
        {
            HistoryType::SingleResource
        } else {
            HistoryType::AB
        };

        // Copy accesses from history resource to last write of current frame resource
        let last_resource = &mut resources[chain_info.first_use_handle.to_index()];
        if first_resource_clone.following_reads_last_pass_idx.is_some() {
            last_resource.following_reads_layout = first_resource_clone.layout;
            last_resource.following_reads_syncs |= first_resource_clone.following_reads_syncs;
            last_resource.following_reads_accesses |= first_resource_clone.following_reads_accesses;

            if last_resource.following_reads_layout == TextureLayout::Undefined {
                last_resource.following_reads_layout = first_resource_clone.layout;
            } else if (last_resource.following_reads_layout == TextureLayout::DepthStencilRead
                && first_resource_clone.layout == TextureLayout::Sampled)
                || (last_resource.following_reads_layout == TextureLayout::Sampled
                    && first_resource_clone.layout == TextureLayout::DepthStencilRead)
            {
                last_resource.following_reads_layout = TextureLayout::DepthStencilRead;
            } else if last_resource.following_reads_layout != first_resource_clone.layout {
                last_resource.following_reads_layout = TextureLayout::General;
            }

            first_resource_clone.layout = last_resource.layout;
            first_resource_clone.following_reads_syncs = last_resource.sync;
            first_resource_clone.following_reads_accesses = last_resource.access;
        }

        resources[chain_info.first_use_handle.to_index()] = first_resource_clone;

        history_access_type
    }

    fn bake(&mut self, device: &Device) {
        self.built_passes.clear();
        self.textures.clear();
        self.buffers.clear();
        self.texture_writes.clear();
        self.buffer_writes.clear();
        self.current_ab = ABEntry::A;

        // Determine required resources
        for (idx, pass) in self.passes.iter_mut().enumerate() {
            let mut context = RenderPassResourceCreationContext {
                device,
                pass_idx: PassIdx(idx as u32),
                textures: &mut self.textures,
                buffers: &mut self.buffers,
                texture_writes: &mut self.texture_writes,
                buffer_writes: &mut self.buffer_writes,
            };

            pass.create_resources(&mut context);
        }

        // Determine resource accesses
        for (idx, pass) in self.passes.iter_mut().enumerate() {
            let mut context = RenderPassResourceAccessContext {
                device,
                pass_idx: PassIdx(idx as u32),
                texture_accesses: &mut self.texture_accesses,
                buffer_accesses: &mut self.buffer_accesses,
                texture_writes: &mut self.texture_writes,
                buffer_writes: &mut self.buffer_writes,
                textures: &mut self.textures,
                buffers: &mut self.buffers,
                pass_texture_accesses_range: Range::default(),
                pass_buffer_accesses_range: Range::default(),
            };

            pass.register_resource_accesses(&mut context);

            self.built_passes.push(BakedRenderPass {
                pass_idx: context.pass_idx,
                texture_accesses: context.pass_texture_accesses_range.clone(),
                buffer_accesses: context.pass_buffer_accesses_range.clone(),
                textures: HashMap::new(),
                buffers: HashMap::new(),
            });
        }

        // Determine history accesses of resources
        for (_, desc) in &mut self.textures {
            let current_handle = desc.last_handle;
            desc.history_type =
                Self::identify_history_accesses(current_handle, &mut self.texture_writes);
        }
        for (_, desc) in &mut self.buffers {
            let current_handle = desc.last_handle;
            desc.history_type =
                Self::identify_history_accesses(current_handle, &mut self.buffer_writes);
        }

        // TODO: Remove unused passes and the associated resource writes (based on following reads sync & following reads access but be careful about depth buffer writes)

        // Try to find aliasing opportunities
        let mut embeds = SmallVec::<[(&'static str, &'static str); 8]>::new();
        for (_, texture_desc) in &self.textures {
            if texture_desc.history_type != HistoryType::None {
                // We cant alias history resources.
                continue;
            }

            let handle = texture_desc.last_handle;

            let chain_info = Self::find_first_and_last_usage(handle, &self.texture_writes);

            let first_texture_write = &self.texture_writes[chain_info.first_use_handle.0];
            if first_texture_write.write_pass_idx.is_none() || !first_texture_write.discard {}

            for (_, texture_desc_b) in &self.textures {
                if texture_desc_b.name == texture_desc.name
                    || texture_desc_b.info.dimension != texture_desc.info.dimension
                    || texture_desc_b.info.width != texture_desc.info.width
                    || texture_desc_b.info.height != texture_desc.info.height
                    || texture_desc_b.info.depth != texture_desc.info.depth
                    || texture_desc_b.info.format != texture_desc.info.format
                    || texture_desc_b.info.samples != texture_desc.info.samples
                // Mip count, array level count and usage will get adjusted during resource build and then controlled using texture views.as
                // TODO: Allow aliasing if format reinterpretation is possible
                // In theory, we could also check if the smaller of the two could be a mipmap of the larger and adjust the mipcount but that probably doesn't happen in practice.
                {
                    continue;
                }
                let handle_b = texture_desc_b.last_handle;
                if Self::try_alias_resource(&chain_info, handle_b, &mut self.texture_writes) {
                    embeds.push((texture_desc.name, texture_desc_b.name));
                    break;
                }
            }
        }
        for (name, embedded_into_name) in embeds.drain(..) {
            let resource = self.textures.get_mut(name).unwrap();
            resource.resource = BuiltResource::EmbeddedInto(embedded_into_name);
            let src_info = resource.info.clone();
            let target_resource = &mut self.textures.get_mut(embedded_into_name).unwrap();
            target_resource.merged_info.usage |= src_info.usage;
            target_resource.merged_info.mip_levels = target_resource
                .merged_info
                .mip_levels
                .max(src_info.mip_levels);
            target_resource.merged_info.array_length = target_resource
                .merged_info
                .array_length
                .max(src_info.array_length);
        }

        for (_, buffer_desc) in &self.buffers {
            if buffer_desc.history_type != HistoryType::None {
                // We cant alias history resources.
                continue;
            }

            let handle = buffer_desc.last_handle;

            let chain_info = Self::find_first_and_last_usage(handle, &self.buffer_writes);

            for (_, buffer_desc_b) in &self.buffers {
                if buffer_desc_b.name == buffer_desc.name
                    || buffer_desc.info.1 != buffer_desc_b.info.1
                // Size and usage will get adjusted during resource build
                {
                    continue;
                }
                let handle_b = buffer_desc_b.last_handle;
                if Self::try_alias_resource(&chain_info, handle_b, &mut self.buffer_writes) {
                    embeds.push((buffer_desc.name, buffer_desc_b.name));
                    break;
                }
            }
        }
        for (_, texture) in &mut self.textures {
            if let &BuiltResource::Undecided = &texture.resource {
                continue;
            }

            texture.resource = BuiltResource::Resource(
                self.device
                    .create_texture(&texture.merged_info, None)
                    .unwrap(),
            );
        }
        for (name, embedded_into_name) in embeds.drain(..) {
            let resource = self.buffers.get_mut(name).unwrap();
            resource.resource = BuiltResource::EmbeddedInto(embedded_into_name);
            let src_info = resource.info.clone();
            let target_resource = self.buffers.get_mut(embedded_into_name).unwrap();
            target_resource.merged_info.0.usage |= src_info.0.usage;
            target_resource.merged_info.0.size = target_resource.info.0.size.max(src_info.0.size);
        }
        for (_, buffer) in &mut self.buffers {
            if let &BuiltResource::Undecided = &buffer.resource {
                continue;
            }

            buffer.resource = BuiltResource::Resource(
                self.device
                    .create_buffer(&buffer.merged_info.0, buffer.merged_info.1, None)
                    .unwrap(),
            );
        }

        for pass in &mut self.built_passes {
            for access in
                &self.texture_accesses[pass.texture_accesses.start..pass.texture_accesses.end]
            {
                let texture = Self::get_resource(access.name, &self.textures);

                pass.textures.insert(
                    access.name,
                    device.create_texture_view(
                        texture,
                        &TextureViewInfo {
                            base_mip_level: access.range.base_mip_level,
                            mip_level_length: access.range.mip_level_length,
                            base_array_layer: access.range.base_array_layer,
                            array_layer_length: access.range.array_layer_length,
                            format: None,
                        },
                        Some(access.name),
                    ),
                );
            }
            for access in
                &self.buffer_accesses[pass.buffer_accesses.start..pass.buffer_accesses.end]
            {
                let buffer = Self::get_resource(access.name, &self.buffers);
                pass.buffers
                    .insert(access.name, (buffer.clone(), access.range.clone()));
            }
        }

        self.built_passes.sort_unstable_by(|a, b| {
            // This is slow but the number of passes should be low enough that this doesn't become a problem.
            // Also, it's only baked once.
            let mut max_dependency = PassIdx(0u32);
            for access in &self.texture_accesses[a.texture_accesses.start..a.texture_accesses.end] {
                if access.write_pass_idx == b.pass_idx {
                    return Ordering::Greater;
                }
                max_dependency = max_dependency.min(access.write_pass_idx);
            }
            for access in &self.buffer_accesses[a.buffer_accesses.start..a.buffer_accesses.end] {
                if access.write_pass_idx == b.pass_idx {
                    return Ordering::Greater;
                }
                max_dependency = max_dependency.min(access.pass_idx);
            }
            let mut other_max_dependency = PassIdx(0u32);
            for access in &self.texture_accesses[b.texture_accesses.start..b.texture_accesses.end] {
                if access.pass_idx == a.pass_idx {
                    return Ordering::Less;
                }
                other_max_dependency = other_max_dependency.min(access.write_pass_idx);
            }
            for access in &self.buffer_accesses[b.buffer_accesses.start..b.buffer_accesses.end] {
                if access.pass_idx == a.pass_idx {
                    return Ordering::Less;
                }
                other_max_dependency = other_max_dependency.min(access.pass_idx);
            }

            max_dependency.cmp(&other_max_dependency)
        });

        // Update first and last following reads
        for write in &mut self.texture_writes {
            write.following_reads_first_baked_pass_idx = None;
            write.following_reads_last_baked_pass_idx = None;
        }
        for write in &mut self.buffer_writes {
            write.following_reads_first_baked_pass_idx = None;
            write.following_reads_last_baked_pass_idx = None;
        }
        for (baked_pass_i, pass) in self.built_passes.iter().enumerate() {
            let baked_pass_idx = BakedPassIdx(baked_pass_i as u32);
            for access in
                &self.texture_accesses[pass.texture_accesses.start..pass.texture_accesses.end]
            {
                let write = &mut self.texture_writes[access.handle.to_index()];
                if write.following_reads_first_baked_pass_idx.is_none() {
                    write.following_reads_first_baked_pass_idx = Some(baked_pass_idx);
                }
                write.following_reads_last_baked_pass_idx = Some(baked_pass_idx);
            }
            for access in
                &self.buffer_accesses[pass.buffer_accesses.start..pass.buffer_accesses.end]
            {
                let write = &mut self.texture_writes[access.handle.to_index()];
                if write.following_reads_first_baked_pass_idx.is_none() {
                    write.following_reads_first_baked_pass_idx = Some(baked_pass_idx);
                }
                write.following_reads_last_baked_pass_idx = Some(baked_pass_idx);
            }
        }
    }

    pub fn execute(&mut self, ctx: &GraphicsContext) {
        let mut cmd_buffer = ctx.get_command_buffer(QueueType::Graphics);

        for (i, pass) in self.built_passes.iter().enumerate() {
            let baked_pass_idx = BakedPassIdx(i as u32);

            for access in
                &self.texture_accesses[pass.texture_accesses.start..pass.texture_accesses.end]
            {
                let write = &self.texture_writes[access.handle.to_index()];
                if write.write_pass_idx != Some(pass.pass_idx)
                    && write.following_reads_first_baked_pass_idx != Some(baked_pass_idx)
                {
                    continue;
                }
                let wait_for_handle = Self::find_write_to_wait_for(
                    pass.pass_idx,
                    write,
                    access.handle,
                    &self.texture_writes,
                );

                let previous_write = &self.texture_writes[wait_for_handle.to_index()];
                let texture = Self::get_resource(access.name, &self.textures);
                let barrier = self.get_texture_barrier(texture, previous_write);
                cmd_buffer.barrier(&[barrier]);
            }

            for access in
                &self.buffer_accesses[pass.buffer_accesses.start..pass.buffer_accesses.end]
            {
                let write = &self.buffer_writes[access.handle.to_index()];
                if write.write_pass_idx != Some(pass.pass_idx)
                    && write.following_reads_first_baked_pass_idx != Some(baked_pass_idx)
                {
                    continue;
                }
                let wait_for_handle = Self::find_write_to_wait_for(
                    pass.pass_idx,
                    write,
                    access.handle,
                    &self.buffer_writes,
                );

                let previous_write = &self.buffer_writes[wait_for_handle.to_index()];
                let buffer = Self::get_resource(access.name, &self.buffers);
                let barrier = self.get_buffer_barrier(buffer, previous_write);
                cmd_buffer.barrier(&[barrier]);
            }

            cmd_buffer.flush_barriers();

            let execute_ctx = RenderPassExecuteContext {
                cmd_buffer: &mut cmd_buffer,
                textures: &pass.textures,
                buffers: &pass.buffers,
            };
            self.passes[pass.pass_idx.to_index()].execute(&execute_ctx);
        }
    }

    fn get_resource<'a, TInfo: Clone, THandle: Handle, T>(
        name: &str,
        resources: &'a HashMap<&'static str, ResourceDescription<TInfo, THandle, T>>,
    ) -> &'a T {
        match &resources[name].resource {
            BuiltResource::Undecided => panic!("Resource hasn't been built yet"),
            BuiltResource::Resource(resource) => resource,
            BuiltResource::EmbeddedInto(name) => {
                if let BuiltResource::Resource(resource) = &resources[name].resource {
                    resource
                } else {
                    panic!("Resource hasn't been built yet")
                }
            }
        }
    }

    fn get_texture_barrier<'a>(
        &self,
        texture: &'a Texture,
        write: &ResourceWrite<TextureHandle>,
    ) -> Barrier<'a> {
        if write.access.is_write() && write.following_reads_first_pass_idx.is_some() {
            // Barrier for upcoming reads. There will be a barrier at the last of those reads.
            Barrier::TextureBarrier {
                old_sync: write.sync,
                new_sync: write.following_reads_syncs,
                old_access: write.access,
                new_access: write.following_reads_accesses,
                old_layout: write.layout,
                new_layout: write.following_reads_layout,
                texture,
                range: BarrierTextureRange::default(),
                queue_ownership: None,
            }
        } else if write.access.is_write() {
            // Barrier for the next write because there are no read passes (Can be the case for e.g. depth buffers)
            let next_write = &self.texture_writes[write.next.unwrap().to_index()];
            Barrier::TextureBarrier {
                old_sync: write.following_reads_syncs,
                new_sync: next_write.sync,
                old_access: write.following_reads_accesses & BarrierAccess::write_mask(),
                new_access: if next_write.discard {
                    BarrierAccess::empty()
                } else {
                    next_write.following_reads_accesses
                },
                old_layout: if next_write.discard {
                    TextureLayout::Undefined
                } else {
                    write.following_reads_layout
                },
                new_layout: next_write.layout,
                texture,
                range: BarrierTextureRange::default(),
                queue_ownership: None,
            }
        } else if write.following_reads_first_pass_idx.is_some() {
            // Barrier for the next write from reads
            let next_write = &self.texture_writes[write.next.unwrap().to_index()];

            Barrier::TextureBarrier {
                old_sync: write.following_reads_syncs,
                new_sync: next_write.sync,
                old_access: BarrierAccess::empty(),
                new_access: next_write.following_reads_accesses,
                old_layout: if next_write.discard {
                    TextureLayout::Undefined
                } else {
                    write.following_reads_layout
                },
                new_layout: next_write.layout,
                texture,
                range: BarrierTextureRange::default(),
                queue_ownership: None,
            }
        } else {
            panic!("No barrier necessary.")
        }
    }

    fn get_buffer_barrier<'a>(
        &self,
        buffer: &'a Arc<BufferSlice>,
        write: &ResourceWrite<BufferHandle>,
    ) -> Barrier<'a> {
        if write.access.is_write() && write.following_reads_first_pass_idx.is_some() {
            // Barrier for upcoming reads. There will be a barrier at the last of those reads.
            Barrier::BufferBarrier {
                old_sync: write.sync,
                new_sync: write.following_reads_syncs,
                old_access: write.access,
                new_access: write.following_reads_accesses,
                buffer: BufferRef::Regular(buffer),
                queue_ownership: None,
            }
        } else if write.access.is_write() {
            // Barrier for the next write because there are no read passes (Can be the case for e.g. depth buffers)
            let next_write = &self.texture_writes[write.next.unwrap().to_index()];
            Barrier::BufferBarrier {
                old_sync: write.following_reads_syncs,
                new_sync: next_write.sync,
                old_access: write.following_reads_accesses & BarrierAccess::write_mask(),
                new_access: if next_write.discard {
                    BarrierAccess::empty()
                } else {
                    next_write.following_reads_accesses
                },
                buffer: BufferRef::Regular(buffer),
                queue_ownership: None,
            }
        } else if write.following_reads_first_pass_idx.is_some() {
            // Barrier for the next write from reads
            let next_write = &self.texture_writes[write.next.unwrap().to_index()];
            Barrier::BufferBarrier {
                old_sync: write.following_reads_syncs,
                new_sync: next_write.sync,
                old_access: BarrierAccess::empty(),
                new_access: next_write.following_reads_accesses,
                buffer: BufferRef::Regular(buffer),
                queue_ownership: None,
            }
        } else {
            panic!("No barrier necessary.")
        }
    }

    fn find_write_to_wait_for<T: Handle>(
        pass_idx: PassIdx,
        write: &ResourceWrite<T>,
        write_handle: T,
        writes: &[ResourceWrite<T>],
    ) -> T {
        if write.write_pass_idx == Some(pass_idx) {
            if let Some(previous) = write.previous {
                previous
            } else {
                let chain = Self::find_first_and_last_usage(write_handle, writes);
                chain.last_use_handle
            }
        } else {
            write_handle
        }
    }
}
