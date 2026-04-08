mod builder;
mod graph;

use std::sync::Arc;
use smallvec::SmallVec;
use crate::graphics::{Barrier, BarrierAccess, BarrierSync, BarrierTextureRange, BufferInfo, BufferRef, BufferSlice, ClearColor, ClearDepthStencilValue, CommandBuffer, Device, GraphicsContext, MemoryUsage, QueueType, Range, Texture, TextureInfo, TextureLayout, TextureView};

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
enum HistoryType {
    None,
    SingleResource,
    AB,
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

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum ABEntry {
    A,
    B,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
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

trait AccessKind {
    fn is_write(self) -> bool;

    fn discards(self) -> bool;

    fn to_access(self) -> BarrierAccess;

    fn is_compatible(self, sync: BarrierSync) -> bool;

    fn to_layout(self) -> TextureLayout;
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

impl AccessKind for TextureAccessKind {
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

impl AccessKind for BufferAccessKind {
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

    fn to_layout(self) -> TextureLayout {
        TextureLayout::General
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResourceAccess<TKind: AccessKind, TRange: Clone + PartialEq + Eq> {
    name: &'static str,
    kind: TKind,
    range: TRange,
    sync: BarrierSync,
    history: HistoryResourceEntry,
}

enum BuiltResource<T> {
    Undecided,
    Resource(AB<T>),
    EmbeddedInto(&'static str),
}

struct ResourceDescription<TInfo: Clone, TResource, TKind: AccessKind, TRange: Clone + PartialEq + Eq> {
    name: &'static str,
    info: TInfo,
    merged_info: TInfo,
    resource: BuiltResource<TResource>,
    history_type: HistoryType,
    accesses: SmallVec<[ResourceAccess<TKind, TRange>; 4]>
}

enum ResourceDescriptionType {
    Texture(ResourceDescription<TextureInfo, Arc<TextureView>, TextureAccessKind, BarrierTextureRange>),
    Buffer(ResourceDescription<BufferInfo, Arc<BufferSlice>, BufferAccessKind, Range>)
}