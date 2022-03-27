use std::sync::Arc;

use crate::Vec2;
use crate::Vec2I;
use crate::Vec2UI;

use crate::graphics::{Backend, BufferUsage, TextureUsage};

use super::AccelerationStructureInstance;
use super::BottomLevelAccelerationStructureInfo;
use super::BufferInfo;
use super::LoadOp;
use super::MemoryUsage;
use super::RenderpassRecordingMode;
use super::ShaderType;
use super::StoreOp;
use super::SubpassInfo;
use super::TopLevelAccelerationStructureInfo;
use super::texture::TextureLayout;

#[derive(Clone)]
pub struct Viewport {
  pub position: Vec2,
  pub extent: Vec2,
  pub min_depth: f32,
  pub max_depth: f32
}

#[derive(Clone)]
pub struct Scissor {
  pub position: Vec2I,
  pub extent: Vec2UI
}

#[derive(Clone, Debug, Copy, PartialEq, Hash)]
pub enum CommandBufferType {
  PRIMARY,
  SECONDARY
}

#[derive(Clone)]
pub enum PipelineBinding<'a, B: Backend> {
  Graphics(&'a Arc<B::GraphicsPipeline>),
  Compute(&'a Arc<B::ComputePipeline>),
  RayTracing(&'a Arc<B::RayTracingPipeline>),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum IndexFormat {
  U16,
  U32
}

pub trait CommandBuffer<B: Backend> {
  fn set_pipeline(&mut self, pipeline: PipelineBinding<B>);
  fn set_vertex_buffer(&mut self, vertex_buffer: &Arc<B::Buffer>, offset: usize);
  fn set_index_buffer(&mut self, index_buffer: &Arc<B::Buffer>, offset: usize, format: IndexFormat);
  fn set_viewports(&mut self, viewports: &[ Viewport ]);
  fn set_scissors(&mut self, scissors: &[ Scissor ]);
  fn upload_dynamic_data<T>(&mut self, data: &[T], usage: BufferUsage) -> Arc<B::Buffer>
  where T: 'static + Send + Sync + Sized + Clone;
  fn upload_dynamic_data_inline<T>(&mut self, data: &[T], visible_for_shader_stage: ShaderType)
    where T: 'static + Send + Sync + Sized + Clone;
  fn create_temporary_buffer(&mut self, info: &BufferInfo, memory_usage: MemoryUsage) -> Arc<B::Buffer>;
  fn draw(&mut self, vertices: u32, offset: u32);
  fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32);
  fn bind_texture_view(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<B::TextureSamplingView>, sampler: &Arc<B::Sampler>);
  fn bind_uniform_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: &Arc<B::Buffer>, offset: usize, length: usize);
  fn bind_storage_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: &Arc<B::Buffer>, offset: usize, length: usize);
  fn bind_storage_texture(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<B::TextureStorageView>);
  fn bind_sampler(&mut self, frequency: BindingFrequency, binding: u32, sampler: &Arc<B::Sampler>);
  fn bind_acceleration_structure(&mut self, frequency: BindingFrequency, binding: u32, acceleration_structure: &Arc<B::AccelerationStructure>);
  fn track_texture_view(&mut self, texture_view: &Arc<B::TextureSamplingView>);
  fn finish_binding(&mut self);
  fn begin_label(&mut self, label: &str);
  fn end_label(&mut self);
  fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32);
  fn blit(&mut self, src_texture: &Arc<B::Texture>, src_array_layer: u32, src_mip_level: u32, dst_texture: &Arc<B::Texture>, dst_array_layer: u32, dst_mip_level: u32);
  fn finish(self) -> B::CommandBufferSubmission;

  fn begin_render_pass(&mut self, renderpass_info: &RenderPassBeginInfo<B>, recording_mode: RenderpassRecordingMode);
  fn advance_subpass(&mut self);
  fn end_render_pass(&mut self);
  fn barrier(&mut self, barriers: &[Barrier<B>]);
  fn flush_barriers(&mut self);

  fn create_query_range(&mut self, count: u32) -> Arc<B::QueryRange>;
  fn begin_query(&mut self, query_range: &Arc<B::QueryRange>, query_index: u32);
  fn end_query(&mut self, query_range: &Arc<B::QueryRange>, query_index: u32);
  fn copy_query_results_to_buffer(&mut self, query_range: &Arc<B::QueryRange>, buffer: &Arc<B::Buffer>, start_index: u32, count: u32);

  fn inheritance(&self) -> &Self::CommandBufferInheritance;
  type CommandBufferInheritance: Send + Sync;
  fn execute_inner(&mut self, submission: Vec<B::CommandBufferSubmission>);

  // RT
  fn create_bottom_level_acceleration_structure(&mut self, info: &BottomLevelAccelerationStructureInfo<B>, size: usize, target_buffer: &Arc<B::Buffer>, scratch_buffer: &Arc<B::Buffer>) -> Arc<B::AccelerationStructure>;
  fn upload_top_level_instances(&mut self, instances: &[AccelerationStructureInstance<B>]) -> Arc<B::Buffer>;
  fn create_top_level_acceleration_structure(&mut self, info: &TopLevelAccelerationStructureInfo<B>, size: usize, target_buffer: &Arc<B::Buffer>, scratch_buffer: &Arc<B::Buffer>) -> Arc<B::AccelerationStructure>;
  fn trace_ray(&mut self, width: u32, height: u32, depth: u32);
}

pub trait Queue<B: Backend> {
  fn create_command_buffer(&self) -> B::CommandBuffer;
  fn create_inner_command_buffer(&self, inheritance: &<B::CommandBuffer as CommandBuffer<B>>::CommandBufferInheritance) -> B::CommandBuffer;
  fn submit(&self, submission: B::CommandBufferSubmission, fence: Option<&Arc<B::Fence>>, wait_semaphores: &[&Arc<B::Semaphore>], signal_semaphores: &[&Arc<B::Semaphore>], delayed: bool);
  fn present(&self, swapchain: &Arc<B::Swapchain>, wait_semaphores: &[&Arc<B::Semaphore>], delayed: bool);
  fn process_submissions(&self);
}

pub enum RenderPassAttachmentView<'a, B: Backend> {
  RenderTarget(&'a Arc<B::TextureRenderTargetView>),
  DepthStencil(&'a Arc<B::TextureDepthStencilView>)
}

pub struct RenderPassAttachment<'a, B: Backend> {
  pub view: RenderPassAttachmentView<'a, B>,
  pub load_op: LoadOp,
  pub store_op: StoreOp
}

pub struct RenderPassBeginInfo<'a, B: Backend> {
  pub attachments: &'a [RenderPassAttachment<'a, B>],
  pub subpasses: &'a [SubpassInfo]
}

bitflags! {
  pub struct BarrierSync: u32 {
    const VERTEX_INPUT                 = 0b1;
    const VERTEX_SHADER                = 0b10;
    const FRAGMENT_SHADER              = 0b100;
    const COMPUTE_SHADER               = 0b1000;
    const EARLY_DEPTH                  = 0b10000;
    const LATE_DEPTH                   = 0b100000;
    const RENDER_TARGET                = 0b1000000;
    const COPY                         = 0b10000000;
    const RESOLVE                      = 0b100000000;
    const INDIRECT                     = 0b1000000000;
    const INDEX_INPUT                  = 0b10000000000;
    const HOST                         = 0b100000000000;
    const ACCELERATION_STRUCTURE_BUILD = 0b1000000000000;
    const RAY_TRACING                  = 0b10000000000000;
  }
}

bitflags! {
  pub struct BarrierAccess: u32 {
    const INDEX_READ                   = 0b1;
    const INDIRECT_READ                = 0b10;
    const VERTEX_INPUT_READ            = 0b100;
    const CONSTANT_READ                = 0b1000;
    const STORAGE_READ                 = 0b10000;
    const STORAGE_WRITE                = 0b100000;
    const SHADER_RESOURCE_READ         = 0b1000000;
    const COPY_READ                    = 0b10000000;
    const COPY_WRITE                   = 0b100000000;
    const RESOLVE_READ                 = 0b1000000000;
    const RESOLVE_WRITE                = 0b10000000000;
    const DEPTH_STENCIL_READ           = 0b100000000000;
    const DEPTH_STENCIL_WRITE          = 0b1000000000000;
    const RENDER_TARGET_READ           = 0b10000000000000;
    const RENDER_TARGET_WRITE          = 0b100000000000000;
    const SHADER_READ                  = 0b1000000000000000;
    const SHADER_WRITE                 = 0b10000000000000000;
    const MEMORY_READ                  = 0b100000000000000000;
    const MEMORY_WRITE                 = 0b1000000000000000000;
    const HOST_READ                    = 0b10000000000000000000;
    const HOST_WRITE                   = 0b100000000000000000000;
    const ACCELERATION_STRUCTURE_READ  = 0b1000000000000000000000;
    const ACCELERATION_STRUCTURE_WRITE = 0b10000000000000000000000;
  }
}

impl BarrierAccess {
  pub fn write_mask() -> BarrierAccess {
    BarrierAccess::STORAGE_WRITE | BarrierAccess::COPY_WRITE | BarrierAccess::DEPTH_STENCIL_WRITE
      | BarrierAccess::RESOLVE_WRITE | BarrierAccess::RENDER_TARGET_WRITE | BarrierAccess::RENDER_TARGET_WRITE
      | BarrierAccess::SHADER_WRITE | BarrierAccess::MEMORY_WRITE | BarrierAccess::HOST_WRITE | BarrierAccess::ACCELERATION_STRUCTURE_WRITE
  }

  pub fn is_write(&self) -> bool {
    let writes = Self::write_mask();

    self.intersects(writes)
  }
}

pub enum Barrier<'a, B: Backend> {
  TextureBarrier {
    old_sync: BarrierSync,
    new_sync: BarrierSync,
    old_layout: TextureLayout,
    new_layout: TextureLayout,
    old_access: BarrierAccess,
    new_access: BarrierAccess,
    texture: &'a Arc<B::Texture>
  },
  BufferBarrier {
    old_sync: BarrierSync,
    new_sync: BarrierSync,
    old_access: BarrierAccess,
    new_access: BarrierAccess,
    buffer: &'a Arc<B::Buffer>
  },
  GlobalBarrier {
    old_sync: BarrierSync,
    new_sync: BarrierSync,
    old_access: BarrierAccess,
    new_access: BarrierAccess,
  }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq, Debug)]
pub enum BindingFrequency {
  PerDraw = 0,
  PerMaterial = 1,
  PerFrame = 2,
}

pub trait InnerCommandBufferProvider<B: Backend> : Send + Sync {
  fn get_inner_command_buffer(&self) -> B::CommandBuffer;
}
