use std::any::Any;
use std::collections::HashMap;
use std::hash::{
    DefaultHasher,
    Hash,
    Hasher,
};
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

use crate::graphics::{
    BufferSlice,
    Device,
    MemoryUsage,
    Texture,
};

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
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum DataAccessKind {
    Read,
    Write,
}

pub struct TextureAccess {
    name: &'static str,
    kind: TextureAccessKind,
    range: BarrierTextureRange,
    stages: BarrierSync,
    history: HistoryResourceEntry,
}

pub struct BufferAccess {
    name: &'static str,
    kind: BufferAccessKind,
    stages: BarrierSync,
    history: HistoryResourceEntry,
}

pub struct DataAccess {
    name: &'static str,
    kind: DataAccessKind,
    history: HistoryResourceEntry,
}

pub struct FramePassResourceUsageRegister<'a> {
    textures: &'a mut Vec<TextureAccess>,
    buffers: &'a mut Vec<BufferAccess>,
    data: &'a mut Vec<DataAccess>,
    read_bloom: &'a mut u64,
    write_bloom: &'a mut u64,
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
        self.textures.push(TextureAccess {
            name,
            stages,
            kind: access_kind,
            range: range.clone(),
            history,
        });

        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        let hash = hasher.finish();
        (*self.read_bloom) |= 1 << (hash % 63);
        if access_kind.is_write() {
            (*self.write_bloom) |= 1 << (hash % 63);
        }
    }

    pub fn register_buffer_access(
        &mut self,
        name: &'static str,
        stages: BarrierSync,
        access_kind: BufferAccessKind,
        history: HistoryResourceEntry,
    ) {
        self.buffers.push(BufferAccess {
            name,
            stages,
            kind: access_kind,
            history,
        });

        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        let hash = hasher.finish();
        (*self.read_bloom) |= 1 << (hash % 63);
        if access_kind.is_write() {
            (*self.write_bloom) |= 1 << (hash % 63);
        }
    }

    pub fn register_data_access(
        &mut self,
        name: &'static str,
        data_access_kind: DataAccessKind,
        history: HistoryResourceEntry,
    ) {
        self.data.push(DataAccess {
            name,
            kind: data_access_kind,
            history,
        });

        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        let hash = hasher.finish();
        (*self.read_bloom) |= 1 << (hash % 63);
        if data_access_kind == DataAccessKind::Write {
            (*self.write_bloom) |= 1 << (hash % 63);
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
}

struct RenderPassHolder {
    pass: Box<dyn RenderPass>,
    accessed_textures: Vec<TextureAccess>,
    accessed_buffers: Vec<BufferAccess>,
    accessed_data: Vec<DataAccess>,
    queued: bool,
    bloom: u64,
    write_bloom: u64,
}

pub struct RenderGraph {
    device: Arc<Device>,
    passes: Vec<RenderPassHolder>,
    textures: HashMap<&'static str, AB<Arc<Texture>>>,
    buffers: HashMap<&'static str, AB<Arc<BufferSlice>>>,
    data: HashMap<&'static str, AB<AtomicRefCell<Data>>>,
    current_ab: ABEntry,
}

impl RenderGraph {
    pub fn add_pass<T: RenderPass + Default + 'static>(&mut self) {
        let mut pass: Box<dyn RenderPass> = Box::new(T::default());
        self.create_resources(pass.as_mut());
        let pass_holder = RenderPassHolder {
            pass,
            queued: false,
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
            let existing = self.textures.insert(texture.name, texture_ab);
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
            let existing = self.buffers.insert(buffer.name, buffer_ab);
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
            let existing = self.data.insert(name, refcell_ab);
            if existing.is_some() {
                panic!("Data with the name {} already exists.", name);
            }
        }
    }

    pub fn access_data(&self, name: &'static str) -> AtomicRef<Data> {
        self.data.get(name).unwrap().get(self.current_ab).borrow()
    }

    pub fn access_data_mut(&self, name: &'static str) -> AtomicRefMut<Data> {
        self.data
            .get(name)
            .unwrap()
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
            AB {
                a: AtomicRefCell::new(Box::new(data)),
                b: None,
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
            AB {
                a: AtomicRefCell::new(Box::new(data)),
                b: Some(AtomicRefCell::new(Box::new(data_b))),
            },
        );
        true
    }

    pub fn execute(&mut self) {
        for pass in &mut self.passes {
            // Clear old resources access declarations and state
            pass.accessed_data.clear();
            pass.accessed_textures.clear();
            pass.accessed_buffers.clear();
            pass.queued = false;
            pass.bloom = 0u64;
            pass.write_bloom = 0u64;

            // Collect new access declarations
            let mut context = FramePassResourceUsageRegister {
                textures: &mut pass.accessed_textures,
                buffers: &mut pass.accessed_buffers,
                data: &mut pass.accessed_data,
                read_bloom: &mut pass.bloom,
                write_bloom: &mut pass.write_bloom,
            };
            pass.pass.register_resource_accesses(&mut context);
        }

        let mut read_bloom = 0u64;
        let mut write_bloom = 0u64;
        for (idx, pass) in self.passes.iter().enumerate() {
            for pass in &self.passes[idx..] {}
        }
    }

    fn needs_barrier(&self, pass_a: &RenderPassHolder) -> bool {
        unimplemented!()
    }
}
