use crate::Vec2;
use crate::Vec2I;
use crate::Vec2UI;
use crate::Vec3UI;

use super::*;

use bitflags::bitflags;

#[derive(Clone)]
pub struct Viewport {
    pub position: Vec2,
    pub extent: Vec2,
    pub min_depth: f32,
    pub max_depth: f32,
}

#[derive(Clone)]
pub struct Scissor {
    pub position: Vec2I,
    pub extent: Vec2UI,
}

#[derive(Clone, Debug, Copy, PartialEq, Hash)]
pub enum CommandBufferType {
    Primary,
    Secondary,
}

#[derive(Clone)]
pub enum PipelineBinding<'a, B: GPUBackend> {
    Graphics(&'a B::GraphicsPipeline),
    MeshGraphics(&'a B::MeshGraphicsPipeline),
    Compute(&'a B::ComputePipeline),
    RayTracing(&'a B::RayTracingPipeline),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum IndexFormat {
    U16,
    U32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CommandPoolType {
    CommandBuffers,
    InnerCommandBuffers,
}

#[derive(Debug, Clone, Copy)]
pub enum LoadOpColor {
    Load,
    Clear(ClearColor),
    DontCare,
}

#[derive(Debug, Clone, Copy)]
pub enum LoadOpDepthStencil {
    Load,
    Clear(ClearDepthStencilValue),
    DontCare,
}

pub enum StoreOp<'a, B: GPUBackend> {
    Store,
    DontCare,
    Resolve(ResolveAttachment<'a, B>),
}

#[derive(Clone, Copy, PartialEq)]
pub enum ImageLayout {
    Undefined,
    Common,
    RenderTarget,
    DepthWrite,
    DepthRead,
    ShaderResource,
    CopySrcOptimal,
    CopyDstOptimal,
    Present,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RenderpassRecordingMode {
    Commands,
    CommandBuffers(u32),
}

#[derive(Debug)]
pub struct BufferTextureCopyRegion {
    pub buffer_offset: u64,
    pub buffer_row_pitch: u64,
    pub buffer_slice_pitch: u64,
    pub texture_subresource: TextureSubresource,
    pub texture_offset: Vec3UI,
    pub texture_extent: Vec3UI,
}

pub struct BufferCopyRegion {
    pub src_offset: u64,
    pub dst_offset: u64,
    pub size: u64,
}

pub trait CommandPool<B: GPUBackend> {
    unsafe fn create_command_buffer(&mut self) -> B::CommandBuffer;
    unsafe fn reset(&mut self);
}

pub trait CommandBuffer<B: GPUBackend> {
    unsafe fn set_pipeline(&mut self, pipeline: PipelineBinding<B>);
    unsafe fn set_vertex_buffer(&mut self, index: u32, vertex_buffer: &B::Buffer, offset: u64);
    unsafe fn set_index_buffer(
        &mut self,
        index_buffer: &B::Buffer,
        offset: u64,
        format: IndexFormat,
    );
    unsafe fn set_viewports(&mut self, viewports: &[Viewport]);
    unsafe fn set_scissors(&mut self, scissors: &[Scissor]);
    unsafe fn set_push_constant_data<T>(
        &mut self,
        data: &[T],
        visible_for_shader_stage: ShaderType,
    ) where
        T: 'static + Send + Sync + Sized + Clone;
    unsafe fn draw(
        &mut self,
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    );
    unsafe fn draw_indirect(
        &mut self,
        draw_buffer: &B::Buffer,
        draw_buffer_offset: u64,
        draw_count: u32,
        stride: u32,
    );
    unsafe fn draw_indirect_count(
        &mut self,
        draw_buffer: &B::Buffer,
        draw_buffer_offset: u64,
        count_buffer: &B::Buffer,
        count_buffer_offset: u64,
        max_draw_count: u32,
        stride: u32,
    );
    unsafe fn draw_indexed(
        &mut self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    );
    unsafe fn draw_indexed_indirect(
        &mut self,
        draw_buffer: &B::Buffer,
        draw_buffer_offset: u64,
        draw_count: u32,
        stride: u32,
    );
    unsafe fn draw_indexed_indirect_count(
        &mut self,
        draw_buffer: &B::Buffer,
        draw_buffer_offset: u64,
        count_buffer: &B::Buffer,
        count_buffer_offset: u64,
        max_draw_count: u32,
        stride: u32,
    );
    unsafe fn draw_mesh_tasks(
        &mut self,
        group_count_x: u32,
        group_count_y: u32,
        group_count_z: u32,
    );
    unsafe fn draw_mesh_tasks_indirect(
        &mut self,
        draw_buffer: &B::Buffer,
        draw_buffer_offset: u64,
        draw_count: u32,
        stride: u32,
    );
    unsafe fn draw_mesh_tasks_indirect_count(
        &mut self,
        draw_buffer: &B::Buffer,
        draw_buffer_offset: u64,
        count_buffer: &B::Buffer,
        count_buffer_offset: u64,
        max_draw_count: u32,
        stride: u32,
    );
    unsafe fn bind_sampling_view(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        texture: &B::TextureView,
    );
    unsafe fn bind_sampling_view_and_sampler(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        texture: &B::TextureView,
        sampler: &B::Sampler,
    );
    unsafe fn bind_sampling_view_and_sampler_array(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        textures_and_samplers: &[(&B::TextureView, &B::Sampler)],
    );
    unsafe fn bind_storage_view_array(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        textures: &[&B::TextureView],
    );
    unsafe fn bind_uniform_buffer(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        buffer: &B::Buffer,
        offset: u64,
        length: u64,
    );
    unsafe fn bind_storage_buffer(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        buffer: &B::Buffer,
        offset: u64,
        length: u64,
    );
    unsafe fn bind_storage_texture(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        texture: &B::TextureView,
    );
    unsafe fn bind_sampler(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        sampler: &B::Sampler,
    );
    unsafe fn bind_acceleration_structure(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        acceleration_structure: &B::AccelerationStructure,
    );
    unsafe fn finish_binding(&mut self);
    unsafe fn begin_label(&mut self, label: &str);
    unsafe fn end_label(&mut self);
    unsafe fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32);
    unsafe fn dispatch_indirect(&mut self, buffer: &B::Buffer, offset: u64);
    unsafe fn blit(
        &mut self,
        src_texture: &B::Texture,
        src_array_layer: u32,
        src_mip_level: u32,
        dst_texture: &B::Texture,
        dst_array_layer: u32,
        dst_mip_level: u32,
    );

    unsafe fn begin(&mut self, frame: u64, inheritance: Option<&Self::CommandBufferInheritance>);
    unsafe fn finish(&mut self);

    unsafe fn copy_buffer_to_texture(
        &mut self,
        src: &B::Buffer,
        dst: &B::Texture,
        region: &BufferTextureCopyRegion,
    );
    unsafe fn copy_buffer(&mut self, src: &B::Buffer, dst: &B::Buffer, region: &BufferCopyRegion);

    unsafe fn clear_storage_texture(
        &mut self,
        view: &B::Texture,
        array_layer: u32,
        mip_level: u32,
        values: [u32; 4],
    );
    unsafe fn clear_storage_buffer(
        &mut self,
        buffer: &B::Buffer,
        offset: u64,
        length_in_u32s: u64,
        value: u32,
    );

    unsafe fn begin_render_pass(
        &mut self,
        renderpass_info: &RenderPassBeginInfo<B>,
        recording_mode: RenderpassRecordingMode,
    ) -> Option<Self::CommandBufferInheritance>;
    unsafe fn end_render_pass(&mut self);
    unsafe fn barrier(&mut self, barriers: &[Barrier<B>]);

    unsafe fn begin_query(&mut self, query_index: u32);
    unsafe fn end_query(&mut self, query_index: u32);
    unsafe fn copy_query_results_to_buffer(
        &mut self,
        query_pool: &B::QueryPool,
        start_index: u32,
        count: u32,
        buffer: &B::Buffer,
        buffer_offset: u64,
    );

    #[cfg(not(feature = "non_send_gpu"))]
    type CommandBufferInheritance: Send + Sync;

    #[cfg(feature = "non_send_gpu")]
    type CommandBufferInheritance;

    unsafe fn execute_inner(
        &mut self,
        submission: &[&B::CommandBuffer],
        inheritance: Self::CommandBufferInheritance,
    );

    unsafe fn reset(&mut self, frame: u64);

    // RT
    unsafe fn create_bottom_level_acceleration_structure(
        &mut self,
        info: &BottomLevelAccelerationStructureInfo<B>,
        size: u64,
        target_buffer: &B::Buffer,
        target_buffer_offset: u64,
        scratch_buffer: &B::Buffer,
        scratch_buffer_offset: u64,
    ) -> B::AccelerationStructure;

    unsafe fn upload_top_level_instances(
        &mut self,
        instances: &[AccelerationStructureInstance<B>],
        target_buffer: &B::Buffer,
        target_buffer_offset: u64,
    );

    unsafe fn create_top_level_acceleration_structure(
        &mut self,
        info: &TopLevelAccelerationStructureInfo<B>,
        size: u64,
        target_buffer: &B::Buffer,
        target_buffer_offset: u64,
        scratch_buffer: &B::Buffer,
        scratch_buffer_offset: u64,
    ) -> B::AccelerationStructure;

    unsafe fn trace_ray(&mut self, width: u32, height: u32, depth: u32);

    unsafe fn split_barrier_reset(&mut self, split_barrier: &B::SplitBarrier, after: BarrierSync);
    unsafe fn split_barrier_signal(&mut self, split_barrier: &B::SplitBarrier, barrier: Barrier<B>);
    unsafe fn split_barrier_wait(&mut self, waits: &[SplitBarrierWait<B>]);
}

pub struct SplitBarrierWait<'a, B: GPUBackend> {
    pub split_barrier: &'a B::SplitBarrier,
    pub barrier: &'a [Barrier<'a, B>],
}

pub enum RenderPassAttachmentView<'a, B: GPUBackend> {
    RenderTarget(&'a B::TextureView),
    DepthStencil(&'a B::TextureView),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ClearColor {
    color: [u32; 4],
}

impl ClearColor {
    pub const BLACK: ClearColor = ClearColor {
        color: [0u32, 0u32, 0u32, 0u32],
    };

    pub fn from_u32(color: [u32; 4]) -> Self {
        Self { color }
    }

    pub fn from_i32(color: [i32; 4]) -> Self {
        Self {
            color: unsafe { std::mem::transmute_copy(&color) },
        }
    }

    pub fn from_f32(color: [f32; 4]) -> Self {
        Self {
            color: unsafe { std::mem::transmute_copy(&color) },
        }
    }

    pub fn as_i32(&self) -> &[i32] {
        unsafe { std::mem::transmute(&self.color[..]) }
    }

    pub fn as_u32(&self) -> &[u32] {
        unsafe { std::mem::transmute(&self.color[..]) }
    }

    pub fn as_f32(&self) -> &[f32] {
        unsafe { std::mem::transmute(&self.color[..]) }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ClearDepthStencilValue {
    pub depth: f32,
    pub stencil: u32,
}

impl ClearDepthStencilValue {
    pub const DEPTH_ONE: ClearDepthStencilValue = ClearDepthStencilValue {
        depth: 1.0f32,
        stencil: 0u32,
    };
    pub const DEPTH_ZERO: ClearDepthStencilValue = ClearDepthStencilValue {
        depth: 0.0f32,
        stencil: 0u32,
    };
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ResolveMode {
    Average,
    Min,
    Max,
    SampleZero,
}

pub struct ResolveAttachment<'a, B: GPUBackend> {
    pub view: &'a B::TextureView,
    pub mode: ResolveMode,
}

pub struct RenderTarget<'a, B: GPUBackend> {
    pub view: &'a B::TextureView,
    pub load_op: LoadOpColor,
    pub store_op: StoreOp<'a, B>,
}

pub struct DepthStencilAttachment<'a, B: GPUBackend> {
    pub view: &'a B::TextureView,
    pub load_op: LoadOpDepthStencil,
    pub store_op: StoreOp<'a, B>,
}

pub struct RenderPassBeginInfo<'a, B: GPUBackend> {
    pub render_targets: &'a [RenderTarget<'a, B>],
    pub depth_stencil: Option<&'a DepthStencilAttachment<'a, B>>,
    pub query_pool: Option<&'a B::QueryPool>,
}

bitflags! {
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
  pub struct BarrierAccess: u32 {
    const INDEX_READ                   = 0b1;
    const INDIRECT_READ                = 0b10;
    const VERTEX_INPUT_READ            = 0b100;
    const CONSTANT_READ                = 0b1000;
    const STORAGE_READ                 = 0b10000;
    const STORAGE_WRITE                = 0b100000;
    const SAMPLING_READ                = 0b1000000;
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
        BarrierAccess::STORAGE_WRITE
            | BarrierAccess::COPY_WRITE
            | BarrierAccess::DEPTH_STENCIL_WRITE
            | BarrierAccess::RESOLVE_WRITE
            | BarrierAccess::RENDER_TARGET_WRITE
            | BarrierAccess::RENDER_TARGET_WRITE
            | BarrierAccess::SHADER_WRITE
            | BarrierAccess::MEMORY_WRITE
            | BarrierAccess::HOST_WRITE
            | BarrierAccess::ACCELERATION_STRUCTURE_WRITE
    }

    pub fn is_write(&self) -> bool {
        let writes = Self::write_mask();

        self.intersects(writes)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct BarrierTextureRange {
    pub base_mip_level: u32,
    pub mip_level_length: u32,
    pub base_array_layer: u32,
    pub array_layer_length: u32,
}

impl Default for BarrierTextureRange {
    fn default() -> Self {
        Self {
            base_mip_level: 0,
            mip_level_length: 1,
            base_array_layer: 0,
            array_layer_length: 1,
        }
    }
}

impl From<&TextureViewInfo> for BarrierTextureRange {
    fn from(view_info: &TextureViewInfo) -> Self {
        Self {
            base_array_layer: view_info.base_array_layer,
            base_mip_level: view_info.base_mip_level,
            array_layer_length: view_info.array_layer_length,
            mip_level_length: view_info.mip_level_length,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QueueType {
    Graphics,
    Compute,
    Transfer,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueueOwnershipTransfer {
    pub from: QueueType,
    pub to: QueueType,
}

pub enum Barrier<'a, B: GPUBackend> {
    TextureBarrier {
        old_sync: BarrierSync,
        new_sync: BarrierSync,
        old_layout: TextureLayout,
        new_layout: TextureLayout,
        old_access: BarrierAccess,
        new_access: BarrierAccess,
        texture: &'a B::Texture,
        range: BarrierTextureRange,
        queue_ownership: Option<QueueOwnershipTransfer>,
    },
    BufferBarrier {
        old_sync: BarrierSync,
        new_sync: BarrierSync,
        old_access: BarrierAccess,
        new_access: BarrierAccess,
        buffer: &'a B::Buffer,
        offset: u64,
        length: u64,
        queue_ownership: Option<QueueOwnershipTransfer>,
    },
    GlobalBarrier {
        old_sync: BarrierSync,
        new_sync: BarrierSync,
        old_access: BarrierAccess,
        new_access: BarrierAccess,
    },
}

impl<'a, B: GPUBackend> Clone for Barrier<'a, B> {
    fn clone(&self) -> Self {
        match self {
            Self::TextureBarrier {
                old_sync,
                new_sync,
                old_layout,
                new_layout,
                old_access,
                new_access,
                texture,
                range,
                queue_ownership,
            } => Self::TextureBarrier {
                old_sync: *old_sync,
                new_sync: *new_sync,
                old_layout: *old_layout,
                new_layout: *new_layout,
                old_access: *old_access,
                new_access: *new_access,
                texture,
                range: range.clone(),
                queue_ownership: queue_ownership.clone(),
            },
            Self::BufferBarrier {
                old_sync,
                new_sync,
                old_access,
                new_access,
                buffer,
                offset,
                length,
                queue_ownership,
            } => Self::BufferBarrier {
                old_sync: *old_sync,
                new_sync: *new_sync,
                old_access: *old_access,
                new_access: *new_access,
                buffer,
                offset: *offset,
                length: *length,
                queue_ownership: queue_ownership.clone(),
            },
            Self::GlobalBarrier {
                old_sync,
                new_sync,
                old_access,
                new_access,
            } => Self::GlobalBarrier {
                old_sync: *old_sync,
                new_sync: *new_sync,
                old_access: *old_access,
                new_access: *new_access,
            },
        }
    }
}

bitflags! {
    #[derive(Clone, Copy, Eq, Hash, PartialEq, Debug)]
    pub struct CommandPoolFlags : u32 {
        const TRANSIENT = 0x1;
        const INDIVIDUAL_RESET = 0x2;
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq, Debug)]
#[repr(u32)]
pub enum BindingFrequency {
    VeryFrequent = 0,
    Frequent = 1,
    Frame = 2,
}
