use std::any::Any;
use std::sync::Arc;

use bumpalo::collections::{
    String as BumpString,
    Vec as BumpVec,
};
use bumpalo::Bump;
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
    fn create_resources(&mut self, builder: &mut FramePassResourceCreator);
    fn register_resource_accesses(&mut self, builder: &mut dyn FramePassResourceUsageRegister);
}

pub enum TextureAccessKind {
    Sampling,
    StorageRead,
    StorageWrite,
    StorageReadWrite,
    RenderTargetOrDepthStencil,
}

struct ResourceDescription<'a, T> {
    name: BumpString<'a>,
    info: T,
    has_history: bool,
}

pub struct FramePassResourceCreator<'a> {
    alloc: &'a Bump,
    textures: BumpVec<'a, ResourceDescription<'a, TextureInfo>>,
    buffers: BumpVec<'a, ResourceDescription<'a, (BufferInfo, MemoryUsage)>>,
    data: BumpVec<'a, Box<dyn Any + Send + Sync + 'static>>,
}

impl<'a> FramePassResourceCreator<'a> {
    fn create_texture(&mut self, name: BumpString<'a>, info: &TextureInfo, has_history: bool) {
        self.textures.push(ResourceDescription {
            name,
            info: info.clone(),
            has_history,
        });
    }

    fn create_buffer(
        &mut self,
        name: BumpString<'a>,
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

    fn create_data(&mut self, name: &str, data: Box<dyn Any + Send + Sync + 'static>) {}

    fn new_string(&self) -> BumpString {
        BumpString::new_in(self.alloc)
    }
}

pub trait FramePassResourceUsageRegister {
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
