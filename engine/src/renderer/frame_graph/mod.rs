use std::any::Any;
use std::collections::HashMap;
use std::hash::{
    DefaultHasher,
    Hash,
    Hasher,
};
use std::ptr::read;
use std::sync::Arc;

use atomic_refcell::{
    AtomicRef,
    AtomicRefCell,
    AtomicRefMut,
};
use bumpalo::collections::{
    String as BumpString,
    Vec as BumpVec,
};
use bumpalo::Bump;
use smallvec::SmallVec;
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

use crate::graphics::{BufferSlice, Device, GraphicsContext, MemoryUsage, Texture};

pub type Data = Box<dyn Any + Send + Sync + 'static>;

pub trait RenderPass {
    fn create_resources(&mut self, builder: &mut FramePassResourceCreator);
    fn register_resource_accesses(&mut self, builder: &mut FramePassResourceUsageRegister);

    fn execute(&self);
}

struct ResourceDescription<T> {
    name: &'static str,
    info: T,
    has_history: bool,
}

pub struct FramePassResourceCreator<'a> {
    alloc: &'a Bump,
    textures: BumpVec<'a, ResourceDescription<TextureInfo>>,
    buffers: BumpVec<'a, ResourceDescription<(BufferInfo, MemoryUsage)>>,
    data: BumpVec<'a, (&'static str, AB<Data>)>,
}

impl<'a> FramePassResourceCreator<'a> {
    pub fn create_texture(&mut self, name: &'static str, info: &TextureInfo, has_history: bool) {
        self.textures.push(ResourceDescription {
            name,
            info: info.clone(),
            has_history,
        });
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
    }

    pub fn create_data<T: Default + Any + Send + Sync + 'static>(
        &mut self,
        name: &'static str,
        has_history: bool,
    ) {
        let data_ab: AB<Data> = AB {
            a: Box::new(T::default()),
            b: if has_history {
                Some(Box::new(T::default()))
            } else {
                None
            },
        };
        self.data.push((name, data_ab));
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum HistoryResourceEntry {
    Current,
    Past,
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
            TextureAccessKind::StorageReadWrite => BarrierAccess::STORAGE_WRITE | BarrierAccess::STORAGE_READ,
            TextureAccessKind::RenderTargetCleared(_) => BarrierAccess::RENDER_TARGET_WRITE,
            TextureAccessKind::DepthStencilCleared(_) => BarrierAccess::DEPTH_STENCIL_WRITE,
            TextureAccessKind::DepthStencilReadOnly => BarrierAccess::DEPTH_STENCIL_READ,
            TextureAccessKind::RenderTarget => BarrierAccess::RENDER_TARGET_WRITE | BarrierAccess::RENDER_TARGET_READ,
            TextureAccessKind::DepthStencil => BarrierAccess::DEPTH_STENCIL_READ | BarrierAccess::DEPTH_STENCIL_WRITE,
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
            BufferAccessKind::ShaderReadWrite => BarrierAccess::SHADER_READ | BarrierAccess::SHADER_WRITE,
            BufferAccessKind::CopySrc => BarrierAccess::COPY_READ,
            BufferAccessKind::CopyDst => BarrierAccess::COPY_WRITE,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum DataAccessKind {
    Read,
    Write,
    ReadWrite,
}

pub struct TextureAccess {
    name: &'static str,
    kind: TextureAccessKind,
    range: BarrierTextureRange,
    stages: BarrierSync,
    history: HistoryResourceEntry,
    last_write_idx: Option<u32>,
}

pub struct BufferAccess {
    name: &'static str,
    kind: BufferAccessKind,
    stages: BarrierSync,
    history: HistoryResourceEntry,
    last_write_idx: Option<u32>,
}

pub struct DataAccess {
    name: &'static str,
    kind: DataAccessKind,
    history: HistoryResourceEntry,
    last_write_idx: Option<u32>,
}

pub struct ResourceAvailability {
    stages: BarrierSync,
    access: BarrierAccess,
    available_since_pass_idx: u32
}

pub struct FramePassResourceUsageRegister<'a> {
    texture_accesses: &'a mut Vec<TextureAccess>,
    buffer_accesses: &'a mut Vec<BufferAccess>,
    data_accesses: &'a mut Vec<DataAccess>,
    read_bloom: &'a mut u64,
    write_bloom: &'a mut u64,
    pass_idx: u32,
    textures: &'a mut HashMap<&'static str, ResourceHolder<Arc<Texture>>>,
    buffers: &'a mut HashMap<&'static str, ResourceHolder<Arc<BufferSlice>>>,
    datas: &'a mut HashMap<&'static str, ResourceHolder<AtomicRefCell<Data>>>,
}

struct ResourceWrite {
    write_pass_idx: u32,
    discard: bool,
    layout: TextureLayout,
    sync: BarrierSync,
    access: BarrierAccess,
    following_reads_layout: TextureLayout,
    following_reads_syncs: BarrierSync,
    following_reads_accesses: BarrierAccess,
    following_reads_last_pass_idx: Option<u32>,
}

impl<'a> FramePassResourceUsageRegister<'a> {
    pub fn register_texture_access(
        &mut self,
        name: &'static str,
        stages: BarrierSync,
        range: &BarrierTextureRange,
        access_kind: TextureAccessKind,
        history: HistoryResourceEntry,
    ) {
        let texture = self.textures.get_mut(name).unwrap();
        let writes = &mut texture.writes;

        self.texture_accesses.push(TextureAccess {
            name,
            stages,
            kind: access_kind,
            range: range.clone(),
            history,
            last_write_idx: (!writes.is_empty()).then(|| writes.len() as u32),
        });

        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        let hash = hasher.finish();
        (*self.read_bloom) |= 1 << (hash % 63);
        if access_kind.is_write() {
            (*self.write_bloom) |= 1 << (hash % 63);

            writes.push(ResourceWrite {
                write_pass_idx: self.pass_idx,
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
                // Assume the texture was written in the previous frame
                writes.push(ResourceWrite {
                    write_pass_idx: u32::MAX,
                    discard: false,
                    layout: TextureLayout::Undefined,
                    sync: BarrierSync::empty(),
                    access: BarrierAccess::empty(),
                    following_reads_layout: TextureLayout::Undefined,
                    following_reads_syncs: BarrierSync::empty(),
                    following_reads_accesses: BarrierAccess::empty(),
                    following_reads_last_pass_idx: None,
                });
            }
            let write = writes.last_mut().unwrap();
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
    }

    pub fn register_buffer_access(
        &mut self,
        name: &'static str,
        stages: BarrierSync,
        access_kind: BufferAccessKind,
        history: HistoryResourceEntry,
    ) {
        let buffer = self.buffers.get_mut(name).unwrap();
        let writes = &mut buffer.writes;
        self.buffer_accesses.push(BufferAccess {
            name,
            stages,
            kind: access_kind,
            history,
            last_write_idx: (!writes.is_empty()).then(|| writes.len() as u32),
        });

        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        let hash = hasher.finish();
        (*self.read_bloom) |= 1 << (hash % 63);
        if access_kind.is_write() {
            (*self.write_bloom) |= 1 << (hash % 63);

            writes.push(ResourceWrite {
                write_pass_idx: self.pass_idx,
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
                // Assume the texture was written in the previous frame
                writes.push(ResourceWrite {
                    write_pass_idx: u32::MAX,
                    discard: false,
                    layout: TextureLayout::General,
                    sync: BarrierSync::empty(),
                    access: BarrierAccess::empty(),
                    following_reads_layout: TextureLayout::General,
                    following_reads_syncs: BarrierSync::empty(),
                    following_reads_accesses: BarrierAccess::empty(),
                    following_reads_last_pass_idx: None,
                });
            }
            let write = writes.last_mut().unwrap();
            write.following_reads_last_pass_idx = Some(write.following_reads_last_pass_idx.map_or(self.pass_idx, |idx| idx.max(self.pass_idx)));
            write.following_reads_syncs |= stages;
            write.following_reads_accesses |= access_kind.to_access();
        }
    }

    pub fn register_data_access(
        &mut self,
        name: &'static str,
        data_access_kind: DataAccessKind,
        history: HistoryResourceEntry,
    ) {
        let data = self.datas.get_mut(name).unwrap();
        let writes = &mut data.writes;
        self.data_accesses.push(DataAccess {
            name,
            kind: data_access_kind,
            history,
            last_write_idx: (!writes.is_empty()).then(|| writes.len() as u32),
        });

        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        let hash = hasher.finish();
        (*self.read_bloom) |= 1 << (hash % 63);
        if data_access_kind != DataAccessKind::Read {
            (*self.write_bloom) |= 1 << (hash % 63);

            writes.push(ResourceWrite {
                write_pass_idx: self.pass_idx,
                discard: false,
                layout: TextureLayout::General,
                sync: BarrierSync::empty(),
                access: BarrierAccess::empty(),
                following_reads_layout: TextureLayout::General,
                following_reads_syncs: BarrierSync::empty(),
                following_reads_accesses: BarrierAccess::empty(),
                following_reads_last_pass_idx: None,
            });
        } else {
            if writes.is_empty() {
                // Assume the texture was written in the previous frame
                writes.push(ResourceWrite {
                    write_pass_idx: u32::MAX,
                    discard: false,
                    layout: TextureLayout::General,
                    sync: BarrierSync::empty(),
                    access: BarrierAccess::empty(),
                    following_reads_layout: TextureLayout::General,
                    following_reads_syncs: BarrierSync::empty(),
                    following_reads_accesses: BarrierAccess::empty(),
                    following_reads_last_pass_idx: None,
                });
            }
            let write = writes.last_mut().unwrap();
            write.following_reads_last_pass_idx = Some(write.following_reads_last_pass_idx.map_or(self.pass_idx, |idx| idx.max(self.pass_idx)));
        }
    }
}

struct AB<T> {
    a: T,
    b: Option<T>,
}

struct BufferResource {
    buffers: SmallVec<[Arc<BufferSlice>; 2]>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
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

struct ResourceHolder<T> {
    resource: AB<T>,
    writes: Vec<ResourceWrite>,
    flushed_write_idx: Option<u32>,
}

struct RenderPassHolder {
    pass: Box<dyn RenderPass>,
    accessed_textures: Vec<TextureAccess>,
    accessed_buffers: Vec<BufferAccess>,
    accessed_data: Vec<DataAccess>,
    split_barrier: Option<crate::graphics::SplitBarrier>,
    queued: bool,
    bloom: u64,
    write_bloom: u64,
}

impl RenderPassHolder {
    fn reads_resources_written_by_other_pass(&self, other: &Self) -> bool {
        for texture in &self.accessed_textures {
            if texture.kind.is_write() {
                continue;
            }
            for other_texture in &other.accessed_textures {
                if texture.name == other_texture.name && texture.history == other_texture.history && other_texture.kind.is_write() {
                    return true;
                }
            }
        }
        for buffer in &self.accessed_buffers {
            if buffer.kind.is_write() {
                continue;
            }
            for other_buffer in &other.accessed_buffers {
                if buffer.name == other_buffer.name && buffer.history == other_buffer.history && other_buffer.kind.is_write() {
                    return true;
                }
            }
        }
        false
    }
}

pub struct RenderGraph {
    device: Arc<Device>,
    passes: Vec<RenderPassHolder>,
    textures: HashMap<&'static str, ResourceHolder<Arc<Texture>>>,
    buffers: HashMap<&'static str, ResourceHolder<Arc<BufferSlice>>>,
    data: HashMap<&'static str, ResourceHolder<AtomicRefCell<Data>>>,
    current_ab: ABEntry,
}

impl RenderGraph {
    pub fn add_pass<T: RenderPass + Default + 'static>(&mut self) {
        let mut pass: Box<dyn RenderPass> = Box::new(T::default());
        self.create_resources(pass.as_mut());
        let pass_holder = RenderPassHolder {
            pass,
            queued: false,
            split_barrier: None,
            bloom: 0u64,
            write_bloom: 0u64,
            accessed_buffers: Vec::new(), // Will be populated every frame
            accessed_textures: Vec::new(),
            accessed_data: Vec::new(),
        };
        self.passes.push(pass_holder);
    }

    fn create_resources(&mut self, pass: &mut dyn RenderPass) {
        let bump = Bump::with_capacity(16384);
        let mut context = FramePassResourceCreator {
            alloc: &bump,
            buffers: BumpVec::new_in(&bump),
            textures: BumpVec::new_in(&bump),
            data: BumpVec::new_in(&bump),
        };
        pass.create_resources(&mut context);

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
            let existing = self.textures.insert(texture.name, ResourceHolder {
                resource: texture_ab,
                writes: Vec::new(),
                flushed_write_idx: None,
            });
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
            let existing = self.buffers.insert(buffer.name, ResourceHolder {
                resource: buffer_ab,
                writes: Vec::new(),
                flushed_write_idx: None,
            });
            if existing.is_some() {
                panic!("A buffer with the name {} already exists.", buffer.name);
            }
        }
        for name_and_data in context.data {
            let (name, data) = name_and_data;
            let AB { a, b } = data;
            let refcell_ab = AB {
                a: AtomicRefCell::new(a),
                b: b.map(|b| AtomicRefCell::new(b)),
            };
            let existing = self.data.insert(name, ResourceHolder {
                resource: refcell_ab,
                writes: Vec::new(),
                flushed_write_idx: None,
            });
            if existing.is_some() {
                panic!("Data with the name {} already exists.", name);
            }
        }
    }

    pub fn access_data(&self, name: &'static str) -> AtomicRef<Data> {
        self.data.get(name).unwrap().resource.get(self.current_ab).borrow()
    }

    pub fn access_data_mut(&self, name: &'static str) -> AtomicRefMut<Data> {
        self.data
            .get(name)
            .unwrap()
            .resource
            .get(self.current_ab)
            .borrow_mut()
    }

    pub fn insert_data<T: Any + Send + Sync + 'static>(
        &mut self,
        name: &'static str,
        data: T,
    ) -> bool {
        if self.data.contains_key(name) {
            return false;
        }
        let _ = self.data.insert(
            name,
            ResourceHolder {
                resource: AB {
                    a: AtomicRefCell::new(Box::new(data)),
                    b: None,
                },
                writes: Vec::new(),
                flushed_write_idx: None,
            },
        );
        true
    }

    pub fn insert_data_ab<T: Any + Send + Sync + 'static>(
        &mut self,
        name: &'static str,
        data: T,
        data_b: T,
    ) -> bool {
        if self.data.contains_key(name) {
            return false;
        }
        let _ = self.data.insert(
            name,
            ResourceHolder {
                resource: AB {
                    a: AtomicRefCell::new(Box::new(data)),
                    b: Some(AtomicRefCell::new(Box::new(data_b))),
                },
                writes: Vec::new(),
                flushed_write_idx: None,
            },
        );
        true
    }

    pub fn execute(&mut self, ctx: &GraphicsContext) {
        for (idx, pass) in &mut self.passes.iter_mut().enumerate() {
            // Clear old resources access declarations and state
            pass.accessed_data.clear();
            pass.accessed_textures.clear();
            pass.accessed_buffers.clear();
            pass.queued = false;
            pass.split_barrier = Some(ctx.get_split_barrier());
            pass.bloom = 0u64;
            pass.write_bloom = 0u64;

            // Collect new access declarations
            let mut context = FramePassResourceUsageRegister {
                texture_accesses: &mut pass.accessed_textures,
                buffer_accesses: &mut pass.accessed_buffers,
                data_accesses: &mut pass.accessed_data,
                read_bloom: &mut pass.bloom,
                write_bloom: &mut pass.write_bloom,
                textures: &mut self.textures,
                buffers: &mut self.buffers,
                datas: &mut self.data,
                pass_idx: idx as u32,
            };
            pass.pass.register_resource_accesses(&mut context);
        }


        let mut first_ready_pass = 0usize;
        let mut ready_pass_count = 0usize;
        let mut barrier_passes = 0usize;
        let mut barrier_passes_count = 0usize;

        for (pass_idx, pass) in self.passes.iter().enumerate() {
            let mut is_ready = true;
            for texture_access in &pass.accessed_textures {
                let texture = self.textures.get(&texture_access.name).unwrap();
                let relevant_write = Option::<&ResourceWrite>::None;
                for write in &texture.writes {
                    if write.write_pass_idx >= 
                    relevant_write = Some(write);
                }
            }
        }




        let mut resource_availability = HashMap::<(&'static str, HistoryResourceEntry), ResourceAvailability>::new();
        fn add_or_extend_availability(resource_availability: &mut HashMap<(&'static str, HistoryResourceEntry), ResourceAvailability>, resource_name: &'static str, history: HistoryResourceEntry, stages: BarrierSync, access: BarrierAccess) {
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
                resource_availability.insert((resource_name, history), ResourceAvailability {
                    stages,
                    access,
                    available_since_pass_idx: 0u32,
                });
            }
        }

        // Prepopulate availability for previous frame
        for pass in &self.passes {
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
        }

        let mut first_ready_pass = 0usize;
        let mut ready_pass_count = 0usize;
        let mut barrier_passes = 0usize;
        let mut barrier_passes_count = 0usize;

        fn flush_ready_passes(passes: &mut [RenderPassHolder]) {
            passes.first().unwrap().pass.execute();
        }

        for (idx, pass) in self.passes.iter().enumerate() {
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
            }

            
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
            }*/
            
            
            // Find all passes after the current pass that can 
            
            for pass in &self.passes[idx..] {

            }
        }
    }

    fn needs_barrier(&self, pass_a: &RenderPassHolder) -> bool {
        unimplemented!()
    }
}
