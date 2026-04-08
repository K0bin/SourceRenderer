use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{BufferInfo, TextureInfo};
use crate::graphics::{BufferSlice, CommandBuffer, Device, MemoryUsage, Texture, TextureView};
use crate::renderer::frame_graph::{BufferAccess, BufferHandle, BufferRange, PassIdx, RenderGraph, ResourceDescription, ResourceWrite, TextureAccess, TextureHandle};
use crate::renderer::frame_graph::new::ResourceDescriptionType;

pub trait RenderPass {
    fn create_resources<'a>(&mut self, context: &'a mut RenderPassResourceCreationContext<'a>);
    fn register_resource_accesses<'a, 'b: 'a>(
        &mut self,
        context: &'a mut RenderPassResourceAccessContext<'b>,
    );

    fn execute(&self, context: &RenderPassExecuteContext);
}


pub struct RenderPassResourceCreationContext<'a> {
    device: &'a Device,
}


pub struct RenderPassResourceAccessContext<'a> {
    device: &'a Device,
}


pub struct RenderPassExecuteContext<'a, 'b: 'a> {
    cmd_buffer: &'a mut CommandBuffer<'b>,
    textures: &'a HashMap<&'static str, Arc<TextureView>>,
    buffers: &'a HashMap<&'static str, (Arc<BufferSlice>, BufferRange)>,
}

pub struct FrameGraphBuilder {
    passes: Vec<Box<dyn RenderPass>>,
    resources: SmallVec<[ResourceDescriptionType; 4]>,
}

impl FrameGraphBuilder {
    pub fn build(&self) -> RenderGraph {
        for pass in &self.passes {
            //pass.register_resource_accesses()
        }

        unimplemented!()
    }
}