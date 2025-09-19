use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::ptr::{read, NonNull};
use std::sync::Arc;

use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut};
use bitvec::domain::PartialElement;
use bumpalo::collections::{String as BumpString, Vec as BumpVec};
use bumpalo::Bump;
use smallvec::{smallvec, SmallVec};
use sourcerenderer_core::gpu::{
    BarrierAccess, BarrierSync, BarrierTextureRange, BufferInfo, ClearColor,
    ClearDepthStencilValue, TextureInfo, TextureLayout,
};

use crate::graphics::{BufferSlice, Device, GraphicsContext, MemoryUsage, Texture};

pub trait RenderPass {
    fn create_resources<'a>(&mut self, builder: &'a mut FramePassResourceCreationContext<'a>);
    fn register_resource_accesses<'a>(
        &mut self,
        builder: &'a mut FramePassResourceAccessContext<'a>,
    );

    fn execute(&self);
}

#[derive(Debug, Clone)]
struct ResourceDescription<T: Clone, THandle: Handle> {
    name: &'static str,
    info: T,
    last_handle: THandle,
    history_type: HistoryType,
    embedded_into: Option<&'static str>,
}

pub struct FramePassResourceCreationContext<'a> {
    pass_idx: PassIdx,
    textures: &'a mut HashMap<&'static str, ResourceDescription<TextureInfo, TextureHandle>>,
    buffers:
        &'a mut HashMap<&'static str, ResourceDescription<(BufferInfo, MemoryUsage), BufferHandle>>,
    texture_writes: &'a mut Vec<ResourceWrite<TextureHandle>>,
    buffer_writes: &'a mut Vec<ResourceWrite<BufferHandle>>,
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

impl<'a, 'b: 'a> FramePassResourceCreationContext<'a> {
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
            following_reads_last_pass_idx: None,
        });
        self.textures.insert(
            name,
            ResourceDescription {
                name,
                info: info.clone(),
                history_type: HistoryType::None,
                embedded_into: None,
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
            following_reads_last_pass_idx: None,
        });
        self.buffers.insert(
            name,
            ResourceDescription {
                name,
                info: (info.clone(), memory_usage),
                history_type: HistoryType::None,
                embedded_into: None,
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

pub struct TextureAccess {
    pass_idx: PassIdx,
    name: &'static str,
    handle: TextureHandle,
    kind: TextureAccessKind,
    range: BarrierTextureRange,
    sync: BarrierSync,
    history: HistoryResourceEntry,
}

pub struct BufferAccess {
    pass_idx: PassIdx,
    name: &'static str,
    handle: BufferHandle,
    kind: BufferAccessKind,
    range: BufferRange,
    sync: BarrierSync,
    history: HistoryResourceEntry,
}

pub struct ResourceAvailability {
    stages: BarrierSync,
    access: BarrierAccess,
    available_since_pass_idx: u32,
}

pub struct FramePassResourceAccessContext<'a> {
    pass_idx: PassIdx,
    pass_texture_accesses: &'a mut Vec<TextureAccess>,
    pass_buffer_accesses: &'a mut Vec<BufferAccess>,
    texture_writes: &'a mut Vec<ResourceWrite<TextureHandle>>,
    buffer_writes: &'a mut Vec<ResourceWrite<BufferHandle>>,
    textures: &'a mut HashMap<&'static str, ResourceDescription<TextureInfo, TextureHandle>>,
    buffers:
        &'a mut HashMap<&'static str, ResourceDescription<(BufferInfo, MemoryUsage), BufferHandle>>,
}

pub struct BufferRange {
    pub offset: u64,
    pub len: u64,
}

#[derive(Debug, Clone)]
struct ResourceWrite<THandle: Copy> {
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
    following_reads_last_pass_idx: Option<PassIdx>,
}

impl<'a> FramePassResourceAccessContext<'a> {
    fn register_resource_access<TInfo: Clone, THandle: Handle>(
        pass_idx: PassIdx,
        name: &'static str,
        sync: BarrierSync,
        access: BarrierAccess,
        layout: TextureLayout,
        discard: bool,
        history: HistoryResourceEntry,
        handle_map: &mut HashMap<&'static str, ResourceDescription<TInfo, THandle>>,
        resources: &mut Vec<ResourceWrite<THandle>>,
    ) {
        let mut handle = handle_map.get_mut(&name).unwrap().last_handle;
        if access.is_write() {
            if history == HistoryResourceEntry::Past {
                panic!("Writing to the previous frame resource is not allowed.");
            }

            let new_resource = ResourceWrite::<THandle> {
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
                following_reads_last_pass_idx: None,
            };

            let new_handle = THandle::from_index(resources.len());
            resources.push(new_resource);
            handle = new_handle;
            resources.get_mut(handle.to_index()).unwrap();
        } else {
            if history == HistoryResourceEntry::Past {
                // The first write represents the previous frame
                let mut resource = &resources[handle.to_index()];
                while let Some(previous_handle) = resource.previous {
                    handle = previous_handle;
                    resource = &resources[handle.to_index()];
                }
            }

            let previous_resource = resources.get_mut(handle.to_index()).unwrap();
            assert!(previous_resource.next.is_none());
            assert!(
                history == HistoryResourceEntry::Past || previous_resource.write_pass_idx.is_some()
            ); // the placeholder write is only valid for reading the past frame resource
            previous_resource.following_reads_last_pass_idx = previous_resource
                .following_reads_last_pass_idx
                .max(Some(pass_idx));
            previous_resource.following_reads_syncs |= sync;
            previous_resource.following_reads_accesses |= access;
            let new_layout = layout;
            if previous_resource.following_reads_layout == TextureLayout::Undefined {
                previous_resource.following_reads_layout = new_layout;
            } else if (previous_resource.following_reads_layout == TextureLayout::DepthStencilRead
                && new_layout == TextureLayout::Sampled)
                || (previous_resource.following_reads_layout == TextureLayout::Sampled
                    && new_layout == TextureLayout::DepthStencilRead)
            {
                previous_resource.following_reads_layout = TextureLayout::DepthStencilRead;
            } else if previous_resource.following_reads_layout != new_layout {
                previous_resource.following_reads_layout = TextureLayout::General;
            }
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

        self.pass_texture_accesses.push(TextureAccess {
            pass_idx: self.pass_idx,
            name,
            handle,
            kind: access_kind,
            range,
            sync,
            history,
        });
        Self::register_resource_access(
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

        self.pass_buffer_accesses.push(BufferAccess {
            pass_idx: self.pass_idx,
            name,
            handle,
            kind: access_kind,
            range,
            sync,
            history,
        });

        Self::register_resource_access(
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
    //writes: SmallVec<[Vec<ResourceWrite>; 4]>,
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

struct BakedRenderPass {
    texture_accesses: Range<usize>,
    buffer_accesses: Range<usize>,
}

pub struct RenderGraph {
    device: Arc<Device>,
    passes: Vec<Box<dyn RenderPass>>,
    current_ab: ABEntry,

    // Built
    built_passes: Vec<BakedRenderPass>,
    texture_accesses: Vec<TextureAccess>,
    buffer_accesses: Vec<BufferAccess>,
    textures: HashMap<&'static str, ResourceDescription<TextureInfo, TextureHandle>>,
    buffers: HashMap<&'static str, ResourceDescription<(BufferInfo, MemoryUsage), BufferHandle>>,
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
                let mut next_clone = writes.get(idx.to_index()).unwrap().clone();

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
                let mut previous_clone = writes.get(idx.to_index()).unwrap().clone();

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

        let first_resource = &resources[chain_info.first_use_handle.to_index()];
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
        let mut first_resource_clone = first_resource.clone();
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

    fn bake(&mut self) {
        self.built_passes.clear();
        self.textures.clear();
        self.buffers.clear();
        self.texture_writes.clear();
        self.buffer_writes.clear();
        self.current_ab = ABEntry::A;

        // Determine required resources
        for (idx, pass) in self.passes.iter_mut().enumerate() {
            let mut context = FramePassResourceCreationContext {
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
            let mut context = FramePassResourceAccessContext {
                pass_idx: PassIdx(idx as u32),
                pass_texture_accesses: &mut self.texture_accesses,
                pass_buffer_accesses: &mut self.buffer_accesses,
                texture_writes: &mut self.texture_writes,
                buffer_writes: &mut self.buffer_writes,
                textures: &mut self.textures,
                buffers: &mut self.buffers,
            };

            pass.register_resource_accesses(&mut context);
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
                    || texture_desc_b
                    .info.dimension != texture_desc_b.info.dimension
                    || texture_desc_b
                    .info.width != texture_desc_b.info.width
                    || texture_desc_b
                    .info.height != texture_desc_b.info.height
                    || texture_desc_b
                    .info.depth != texture_desc_b.info.depth
                    || texture_desc_b
                    .info.format != texture_desc_b.info.format // TODO: Allow aliasing if format reinterpretation is possible
                    || texture_desc_b
                    .info.samples != texture_desc_b.info.samples
                // Mip count, array level count and usage will get adjusted during resource build and then controlled using texture views
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
            resource.embedded_into = Some(embedded_into_name);
            let src_info = resource.info.clone();
            let target_resource = &mut self.textures.get_mut(embedded_into_name).unwrap();
            target_resource.info.usage |= src_info.usage;
            target_resource.info.mip_levels =
                target_resource.info.mip_levels.max(src_info.mip_levels);
            target_resource.info.array_length =
                target_resource.info.array_length.max(src_info.array_length);
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
        for (name, embedded_into_name) in embeds.drain(..) {
            let resource = self.buffers.get_mut(name).unwrap();
            resource.embedded_into = Some(embedded_into_name);
            let src_info = resource.info.clone();
            let target_resource = self.buffers.get_mut(embedded_into_name).unwrap();
            target_resource.info.0.usage |= src_info.0.usage;
            target_resource.info.0.size = target_resource.info.0.size.max(src_info.0.size);
        }
    }

    pub fn execute(&mut self, ctx: &GraphicsContext) {
        /*let mut queued_passes = Vec::<QueuedPass>::with_capacity(self.passes.len());
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

        }*/*/
    }

    fn needs_barrier(&self, pass_a: &RenderPassHolder) -> bool {
        unimplemented!()
    }
}
