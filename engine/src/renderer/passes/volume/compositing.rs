use crate::graphics::{
    BackendTexture, Barrier, BufferSlice, CommandBuffer, Device, GraphicsPipeline, MemoryUsage,
    PipelineBinding, RenderPassBeginInfo, RenderTarget, StoreOp, TextureView,
};
use crate::renderer::asset::{
    GraphicsPipelineHandle, GraphicsPipelineInfo, RendererAssets, RendererAssetsReadOnly,
};
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::HistoryResourceEntry;
use sourcerenderer_core::gpu::{
    AttachmentBlendInfo, BarrierAccess, BarrierSync, BarrierTextureRange, BindingFrequency,
    BlendFactor, BlendInfo, BlendOp, BufferInfo, BufferUsage, ColorComponents, CompareFunc,
    CullMode, DepthStencilInfo, FillMode, Format, FrontFace, LoadOpColor, LogicOp, PrimitiveType,
    QueueSharingMode, RasterizerInfo, SampleCount, Scissor, ShaderInputElement, Texture,
    TextureLayout, TextureViewInfo, VertexLayoutInfo, Viewport,
};
use sourcerenderer_core::{Vec2, Vec2I, Vec2UI, Vec4};
use std::sync::Arc;

pub struct CompositingPass {
    pipeline: GraphicsPipelineHandle,
}

impl CompositingPass {
    pub fn new(device: &Arc<Device>, assets: &RendererAssets) -> Self {
        let shader_file_extension = "json";

        let pipeline = assets.request_graphics_pipeline(&GraphicsPipelineInfo {
            vs: &format!("shaders/fullscreen_quad.vert.{}", shader_file_extension),
            fs: Some(&format!(
                "shaders/compositing.frag.{}",
                shader_file_extension
            )),
            vertex_layout: VertexLayoutInfo {
                shader_inputs: &[],
                input_assembler: &[],
            },
            rasterizer: RasterizerInfo {
                fill_mode: FillMode::Fill,
                cull_mode: CullMode::None,
                front_face: FrontFace::CounterClockwise,
                sample_count: SampleCount::Samples1,
            },
            depth_stencil: DepthStencilInfo {
                depth_test_enabled: false,
                depth_write_enabled: false,
                depth_func: CompareFunc::Always,
                stencil_enable: false,
                stencil_read_mask: 0,
                stencil_write_mask: 0,
                stencil_front: Default::default(),
                stencil_back: Default::default(),
            },
            blend: BlendInfo {
                alpha_to_coverage_enabled: false,
                logic_op_enabled: false,
                logic_op: LogicOp::Clear,
                attachments: &[AttachmentBlendInfo {
                    blend_enabled: false,
                    src_color_blend_factor: BlendFactor::Zero,
                    dst_color_blend_factor: BlendFactor::Zero,
                    color_blend_op: BlendOp::Add,
                    src_alpha_blend_factor: BlendFactor::Zero,
                    dst_alpha_blend_factor: BlendFactor::Zero,
                    alpha_blend_op: BlendOp::Add,
                    write_mask: ColorComponents::all(),
                }],
                constants: [0.0f32; 4],
            },
            primitive_type: PrimitiveType::Triangles,
            render_target_formats: &[Format::RGBA8UNorm],
            depth_stencil_format: Format::Unknown,
        });

        Self { pipeline }
    }

    #[inline(always)]
    pub(crate) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_graphics_pipeline(self.pipeline).is_some()
    }

    pub fn execute(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        backbuffer: &Arc<TextureView>,
        backbuffer_handle: &BackendTexture,
        params: &RenderPassParameters,
        color_name: &str,
        ssao_name: &str,
    ) {
        cmd_buffer.barrier(&[Barrier::RawTextureBarrier {
            old_sync: BarrierSync::empty(),
            new_sync: BarrierSync::RENDER_TARGET,
            old_access: BarrierAccess::empty(),
            new_access: BarrierAccess::RENDER_TARGET_WRITE | BarrierAccess::RENDER_TARGET_READ,
            old_layout: TextureLayout::Undefined,
            new_layout: TextureLayout::RenderTarget,
            texture: backbuffer_handle,
            range: BarrierTextureRange::default(),
            queue_ownership: None,
        }]);

        let resources = &params.resources;

        let color_view = resources.get_view(
            color_name,
            &TextureViewInfo {
                base_mip_level: 0u32,
                mip_level_length: 0u32,
                base_array_layer: 0u32,
                array_layer_length: 0u32,
                format: None,
            },
            HistoryResourceEntry::Current,
        );

        let ssao_view = resources.get_view(
            ssao_name,
            &TextureViewInfo {
                base_mip_level: 0u32,
                mip_level_length: 0u32,
                base_array_layer: 0u32,
                array_layer_length: 0u32,
                format: None,
            },
            HistoryResourceEntry::Current,
        );

        cmd_buffer.begin_label("Compositing");

        cmd_buffer.flush_barriers();

        cmd_buffer.begin_render_pass(&RenderPassBeginInfo {
            render_targets: &[RenderTarget {
                view: &backbuffer,
                load_op: LoadOpColor::DontCare,
                store_op: StoreOp::Store,
            }],
            depth_stencil: None,
            query_range: None,
        });

        let pipeline = params.assets.get_graphics_pipeline(self.pipeline).unwrap();
        cmd_buffer.set_pipeline(PipelineBinding::Graphics(pipeline));

        cmd_buffer.set_viewports(&[Viewport {
            position: Vec2::new(0.0f32, 0.0f32),
            extent: Vec2::new(
                backbuffer_handle.info().width as f32,
                backbuffer_handle.info().height as f32,
            ),
            min_depth: 0.0f32,
            max_depth: 1.0f32,
        }]);

        cmd_buffer.set_scissors(&[Scissor {
            position: Vec2I::new(0i32, 0i32),
            extent: Vec2UI::new(
                backbuffer_handle.info().width,
                backbuffer_handle.info().height,
            ),
        }]);

        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            0u32,
            &color_view,
            resources.linear_sampler(),
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            1u32,
            &ssao_view,
            resources.linear_sampler(),
        );

        cmd_buffer.finish_binding();

        cmd_buffer.draw(3u32, 1u32, 0u32, 0u32);

        cmd_buffer.end_render_pass();

        cmd_buffer.end_label();

        cmd_buffer.barrier(&[Barrier::RawTextureBarrier {
            old_sync: BarrierSync::RENDER_TARGET,
            new_sync: BarrierSync::empty(),
            old_access: BarrierAccess::RENDER_TARGET_WRITE,
            new_access: BarrierAccess::empty(),
            old_layout: TextureLayout::RenderTarget,
            new_layout: TextureLayout::Present,
            texture: backbuffer_handle,
            queue_ownership: None,
            range: BarrierTextureRange::default(),
        }]);
    }
}
