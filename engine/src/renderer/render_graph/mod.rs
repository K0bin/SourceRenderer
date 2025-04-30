use std::any::Any;
use std::sync::Arc;

use smallvec::SmallVec;
use sourcerenderer_core::gpu::{
    BarrierAccess,
    BarrierSync,
    BarrierTextureRange,
    BufferInfo,
    TextureInfo,
    TextureLayout,
};

use crate::graphics::{
    BufferSlice,
    MemoryUsage,
    Texture,
};
use crate::renderer::renderer_resources::HistoryResourceEntry;

pub trait RenderPass {
    fn create_resources(&mut self, builder: &mut RenderPassResourceCreator);
    fn register_resource_accesses(&mut self, builder: &mut dyn RenderPassResourceUsageRegister);
}

pub enum TextureAccessKind {
    Sampling,
    StorageRead,
    StorageWrite,
    StorageReadWrite,
    RenderTargetOrDepthStencil,
}

pub struct RenderPassResourceCreator<'a> {
    textures: SmallVec<[Arc<Texture>; 3]>,
    buffer: SmallVec<[Arc<BufferSlice>; 3]>,
    data: SmallVec<[Box<dyn Any + Send + Sync + 'static>; 3]>,
}

impl<'a> RenderPassResourceCreator<'a> {
    fn create_texture(&mut self, name: &str, info: &TextureInfo, has_history: bool) {}

    fn create_buffer(
        &mut self,
        name: &str,
        info: &BufferInfo,
        memory_usage: MemoryUsage,
        has_history: bool,
    ) {
    }

    fn create_data(&mut self, name: &str, data: Box<dyn Any + Send + Sync + 'static>) {}
}

pub trait RenderPassResourceUsageRegister {
    fn register_texture_usage(
        &mut self,
        name: &str,
        stages: BarrierSync,
        range: &BarrierTextureRange,
        access: BarrierAccess,
        layout: TextureLayout,
        discard: bool,
        history: HistoryResourceEntry,
    );

    fn register_buffer_usage(
        &mut self,
        name: &str,
        stages: BarrierSync,
        access: BarrierAccess,
        history: HistoryResourceEntry,
    );

    fn register_data_usage(&mut self, name: &str, mutable: bool);
}

pub struct RenderGraph {}
