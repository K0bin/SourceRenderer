use std::sync::Arc;

use smallvec::SmallVec;
use sourcerenderer_core::{
    Matrix4,
    Platform,
    Vec2,
    Vec2UI,
    Vec3, Vec3UI, Vec4,
};

use super::acceleration_structure_update::AccelerationStructureUpdatePass;
use super::clustering::ClusteringPass;
use super::geometry::GeometryPass;
use super::light_binning::LightBinningPass;
//use super::occlusion::OcclusionPass;
use super::prepass::Prepass;
use super::rt_shadows::RTShadowPass;
use super::sharpen::SharpenPass;
use super::ssao::SsaoPass;
use super::taa::TAAPass;
use crate::asset::AssetManager;
use crate::renderer::asset::RendererAssetsReadOnly;
use crate::renderer::passes::blit::BlitPass;
use crate::renderer::passes::blue_noise::BlueNoise;
use crate::renderer::passes::modern::gpu_scene::{BufferBinding, SceneBuffers};
use crate::renderer::render_path::{
    FrameInfo, RenderPassParameters, RenderPath, RenderPathResult, SceneInfo
};
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};

use crate::graphics::*;

pub struct ConservativeRenderer {
    device: Arc<crate::graphics::Device>,
    barriers: RendererResources,
    clustering_pass: ClusteringPass,
    light_binning_pass: LightBinningPass,
    prepass: Prepass,
    geometry: GeometryPass,
    taa: TAAPass,
    sharpen: SharpenPass,
    ssao: SsaoPass,
    //occlusion: OcclusionPass,
    rt_passes: Option<RTPasses>,
    blue_noise: BlueNoise,
    blit_pass: BlitPass
}

pub struct RTPasses {
    acceleration_structure_update: AccelerationStructureUpdatePass,
    shadows: RTShadowPass,
}

pub struct FrameBindings<'a>{
    gpu_scene_buffer: BufferRef<'a>,
    camera_buffer: BufferRef<'a>,
    camera_history_buffer: BufferRef<'a>,
    vertex_buffer: BufferRef<'a>,
    index_buffer: BufferRef<'a>,
    directional_lights: TransientBufferSlice,
    point_lights: TransientBufferSlice,
    setup_buffer: TransientBufferSlice,
}

#[derive(Clone)]
#[repr(C)]
struct CameraBuffer {
    view_proj: Matrix4,
    inv_proj: Matrix4,
    view: Matrix4,
    proj: Matrix4,
    inv_view: Matrix4,
    position: Vec4,
    inv_proj_view: Matrix4,
    z_near: f32,
    z_far: f32,
    aspect_ratio: f32,
    fov: f32,
}

impl ConservativeRenderer {
    #[allow(unused)]
    pub fn new(
        device: &Arc<crate::graphics::Device>,
        swapchain: &crate::graphics::Swapchain,
        context: &mut GraphicsContext,
        asset_manager: &Arc<AssetManager>,
    ) -> Self {
        let mut init_cmd_buffer = context.get_command_buffer(QueueType::Graphics);
        let resolution = Vec2UI::new(swapchain.width(), swapchain.height());

        let mut barriers = RendererResources::new(device);

        let blue_noise = BlueNoise::new(device);

        let clustering = ClusteringPass::new(&mut barriers, asset_manager);
        let light_binning = LightBinningPass::new(&mut barriers, asset_manager);
        let prepass = Prepass::new(&mut barriers, asset_manager, resolution);
        let geometry = GeometryPass::new(device, resolution, &mut barriers, asset_manager);
        let taa = TAAPass::new(resolution, &mut barriers, asset_manager, false);
        let sharpen = SharpenPass::new(resolution, &mut barriers, asset_manager);
        let ssao = SsaoPass::new(device, resolution, &mut barriers, asset_manager, false);
        //let occlusion = OcclusionPass::new(device, shader_manager);
        let rt_passes = device.supports_ray_tracing().then(|| RTPasses {
            acceleration_structure_update: AccelerationStructureUpdatePass::new(
                device,
                &mut init_cmd_buffer,
            ),
            shadows: RTShadowPass::new(resolution, &mut barriers, asset_manager),
        });
        let blit = BlitPass::new(&mut barriers, asset_manager, swapchain.format());

        init_cmd_buffer.flush_barriers();
        device.flush_transfers();

        device.submit(QueueType::Graphics, QueueSubmission {
            command_buffer: init_cmd_buffer.finish(),
            wait_fences: &[],
            signal_fences: &[],
            acquire_swapchain: None,
            release_swapchain: None
        });
        let c_device = device.clone();
        let task_pool = bevy_tasks::ComputeTaskPool::get();
        task_pool.spawn(async move { c_device.flush(QueueType::Graphics); }).detach();

        Self {
            device: device.clone(),
            barriers,
            clustering_pass: clustering,
            light_binning_pass: light_binning,
            prepass,
            geometry,
            taa,
            sharpen,
            ssao,
            //occlusion,
            rt_passes,
            blue_noise,
            blit_pass: blit
        }
    }

    fn create_frame_bindings<'a, 'b>(
        &'b self,
        cmd_buf: &'b mut CommandBuffer,
        scene: &'a SceneInfo<'a>,
        swapchain: &'a Swapchain,
        gpu_scene_buffers: &'a SceneBuffers,
        camera_buffer: BufferRef<'a>,
        camera_history_buffer: BufferRef<'a>,
        rendering_resolution: &Vec2UI,
        frame: u64,
    ) -> FrameBindings<'a>
        where 'a: 'b {
        let view = &scene.scene.views()[scene.active_view_index];

        let cluster_count = self.clustering_pass.cluster_count();
        let cluster_z_scale = (cluster_count.z as f32) / (view.far_plane / view.near_plane).log2();
        let cluster_z_bias = -(cluster_count.z as f32) * (view.near_plane).log2()
            / (view.far_plane / view.near_plane).log2();
        #[repr(C)]
        #[derive(Debug, Clone)]
        struct SetupBuffer {
            point_light_count: u32,
            directional_light_count: u32,
            cluster_z_bias: f32,
            cluster_z_scale: f32,
            cluster_count: Vec3UI,
            _padding: u32,
            swapchain_transform: Matrix4,
            halton_point: Vec2,
            rt_size: Vec2UI,
            frame: u32
        }
        let setup_buffer = cmd_buf.upload_dynamic_data(
            &[SetupBuffer {
                point_light_count: scene.scene.point_lights().len() as u32,
                directional_light_count: scene.scene.directional_lights().len() as u32,
                cluster_z_bias,
                cluster_z_scale,
                cluster_count,
                _padding: 0,
                swapchain_transform: swapchain.transform(),
                halton_point: super::taa::scaled_halton_point(
                    rendering_resolution.x,
                    rendering_resolution.y,
                    (frame % 8) as u32 + 1,
                ),
                rt_size: *rendering_resolution,
                frame: frame as u32
            }],
            BufferUsage::CONSTANT,
        ).unwrap();
        #[repr(C)]
        #[derive(Debug, Clone)]
        struct PointLight {
            position: Vec3,
            intensity: f32,
        }
        let point_lights: SmallVec<[PointLight; 16]> = scene.scene
            .point_lights()
            .iter()
            .map(|l| PointLight {
                position: l.position,
                intensity: l.intensity,
            })
            .collect();
        let point_lights_buffer = cmd_buf.upload_dynamic_data(&point_lights, BufferUsage::CONSTANT).unwrap();
        #[repr(C)]
        #[derive(Debug, Clone)]
        struct DirectionalLight {
            direction: Vec3,
            intensity: f32,
        }
        let directional_lights: SmallVec<[DirectionalLight; 16]> = scene.scene
            .directional_lights()
            .iter()
            .map(|l| DirectionalLight {
                direction: l.direction,
                intensity: l.intensity,
            })
            .collect();
        let directional_lights_buffer =
            cmd_buf.upload_dynamic_data(&directional_lights, BufferUsage::CONSTANT).unwrap();

        FrameBindings {
            gpu_scene_buffer: BufferRef::Transient(&gpu_scene_buffers.buffer),
            camera_buffer: camera_buffer.clone(),
            camera_history_buffer: camera_history_buffer.clone(),
            vertex_buffer: scene.vertex_buffer.clone(),
            index_buffer: scene.index_buffer.clone(),
            directional_lights: directional_lights_buffer,
            point_lights: point_lights_buffer,
            setup_buffer: setup_buffer,
        }
    }
}

impl<P: Platform> RenderPath<P> for ConservativeRenderer {
    fn is_gpu_driven(&self) -> bool {
        false
    }

    fn write_occlusion_culling_results(&self, _frame: u64, _bitset: &mut Vec<u32>) {
        //self.occlusion.write_occlusion_query_results(frame, bitset);
    }

    fn on_swapchain_changed(
        &mut self,
        _swapchain: &Swapchain,
    ) {
        // TODO: resize render targets
    }

    fn is_ready(&self, asset_manager: &Arc<AssetManager>) -> bool {
        let assets = asset_manager.read_renderer_assets();
        self.clustering_pass.is_ready(&assets)
        && self.light_binning_pass.is_ready(&assets)
        && self.prepass.is_ready(&assets)
        && self.ssao.is_ready(&assets)
        && self.rt_passes.as_ref().map(|passes| passes.shadows.is_ready(&assets)).unwrap_or(true)
        && self.geometry.is_ready(&assets)
        && self.blit_pass.is_ready(&assets)
        && self.taa.is_ready(&assets)
        && self.sharpen.is_ready(&assets)
    }

    #[profiling::function]
    fn render(
        &mut self,
        context: &mut GraphicsContext,
        swapchain: &mut Swapchain,
        scene: &SceneInfo,
        frame_info: &FrameInfo,
        assets: &RendererAssetsReadOnly<'_>
    ) -> Result<RenderPathResult, SwapchainError> {
        let mut cmd_buf = context.get_command_buffer(QueueType::Graphics);

        let main_view = &scene.scene.views()[scene.active_view_index];

        let camera_buffer = cmd_buf.upload_dynamic_data(&[CameraBuffer {
            view_proj: main_view.proj_matrix * main_view.view_matrix,
            inv_proj: main_view.proj_matrix.inverse(),
            view: main_view.view_matrix,
            proj: main_view.proj_matrix,
            inv_view: main_view.view_matrix.inverse(),
            position: Vec4::new(main_view.camera_position.x, main_view.camera_position.y, main_view.camera_position.z, 1.0f32),
            inv_proj_view: (main_view.proj_matrix * main_view.view_matrix).inverse(),
            z_near: main_view.near_plane,
            z_far: main_view.far_plane,
            aspect_ratio: main_view.aspect_ratio,
            fov: main_view.camera_fov
        }], BufferUsage::CONSTANT).unwrap();

        let camera_history_buffer = &camera_buffer;

        let empty_buffer = cmd_buf.create_temporary_buffer(
            &BufferInfo {
                size: 16,
                usage: BufferUsage::STORAGE,
                sharing_mode: QueueSharingMode::Concurrent
            },
            MemoryUsage::GPUMemory,
        ).unwrap();
        let gpu_scene = SceneBuffers {
            buffer: empty_buffer,
            scene_buffer: BufferBinding { offset: 0, length: 0 },
            draws_buffer: BufferBinding { offset: 0, length: 0 },
            meshes_buffer:  BufferBinding { offset: 0, length: 0 },
            drawables_buffer: BufferBinding { offset: 0, length: 0 },
            parts_buffer: BufferBinding { offset: 0, length: 0 },
            materials_buffer: BufferBinding { offset: 0, length: 0 },
            lights_buffer: BufferBinding { offset: 0, length: 0 }
        };

        let frame_bindings = self.create_frame_bindings(
            &mut cmd_buf,
            scene,
            swapchain,
            &gpu_scene,
            BufferRef::Transient(&camera_buffer),
            BufferRef::Transient(camera_history_buffer),
            &Vec2UI::new(swapchain.width(), swapchain.height()),
            frame_info.frame,
        );
        setup_frame(&mut cmd_buf, &frame_bindings);

        let params = RenderPassParameters {
            device: self.device.as_ref(),
            scene,
            resources: &mut self.barriers,
            assets
        };

        if let Some(rt_passes) = self.rt_passes.as_mut() {
            rt_passes
                .acceleration_structure_update
                .execute(&mut cmd_buf, &params);
        }

        /*self.occlusion.execute(
            context,
            &mut cmd_buf,
            &params,
            frame_info.frame,
            &camera_buffer,
            Prepass::DEPTH_TEXTURE_NAME,
        );*/
        self.clustering_pass.execute(
            &mut cmd_buf,
            &params,
            Vec2UI::new(swapchain.width(), swapchain.height()),
            &camera_buffer
        );
        self.light_binning_pass.execute(
            &mut cmd_buf,
            &params,
            &camera_buffer
        );
        self.prepass.execute(
            context,
            &mut cmd_buf,
            &params,
            swapchain.transform(),
            frame_info.frame,
            &camera_buffer,
            &camera_history_buffer
        );
        self.ssao.execute(
            &mut cmd_buf,
            &params,
            Prepass::DEPTH_TEXTURE_NAME,
            Some("TODO"),
            &camera_buffer,
            self.blue_noise.frame(frame_info.frame),
            self.blue_noise.sampler(),
            false
        );
        if let Some(rt_passes) = self.rt_passes.as_mut() {
            rt_passes.shadows.execute(
                &mut cmd_buf,
                &params,
                Prepass::DEPTH_TEXTURE_NAME,
                rt_passes
                    .acceleration_structure_update
                    .acceleration_structure(),
                &self.blue_noise.frame(frame_info.frame),
                &self.blue_noise.sampler(),
            );
        }
        self.geometry.execute::<P>(
            context,
            &mut cmd_buf,
            &params,
            Prepass::DEPTH_TEXTURE_NAME,
            &frame_bindings
        );
        self.taa.execute(
            &mut cmd_buf,
            &params,
            GeometryPass::GEOMETRY_PASS_TEXTURE_NAME,
            Prepass::DEPTH_TEXTURE_NAME,
            Some("TODO"),
            false
        );
        self.sharpen
            .execute(&mut cmd_buf, &params);

        let sharpened_texture = params.resources.access_texture(
            &mut cmd_buf,
            SharpenPass::SHAPENED_TEXTURE_NAME,
            &BarrierTextureRange::default(),
            BarrierSync::COPY,
            BarrierAccess::COPY_READ,
            TextureLayout::CopySrc,
            false,
            HistoryResourceEntry::Current,
        );

        let backbuffer = swapchain.next_backbuffer()?;
        let backbuffer_view = swapchain.backbuffer_view(&backbuffer);
        let backbuffer_handle = swapchain.backbuffer_handle(&backbuffer);

        cmd_buf.barrier(&[Barrier::RawTextureBarrier {
            old_sync: BarrierSync::empty(),
            new_sync: BarrierSync::RENDER_TARGET, // BarrierSync::COPY,
            old_access: BarrierAccess::empty(),
            new_access: BarrierAccess::RENDER_TARGET_WRITE, // BarrierAccess::COPY_WRITE,
            old_layout: TextureLayout::Undefined,
            new_layout: TextureLayout::RenderTarget, // TextureLayout::CopyDst,
            texture: backbuffer_handle,
            range: BarrierTextureRange::default(),
            queue_ownership: None
        }]);
        //cmd_buf.flush_barriers();
        //cmd_buf.blit_to_handle(&*sharpened_texture, 0, 0, swapchain.backbuffer_handle(), 0, 0);
        std::mem::drop(sharpened_texture);
        let sharpened_view = params.resources.access_view(&mut cmd_buf, SharpenPass::SHAPENED_TEXTURE_NAME,
            BarrierSync::FRAGMENT_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo {
                base_mip_level: 0,
                mip_level_length: 1,
                base_array_layer: 0,
                array_layer_length: 1,
                format: None
            }, HistoryResourceEntry::Current);
        let sampler = params.resources.linear_sampler();
        cmd_buf.flush_barriers();

        let resolution = Vec2UI::new(swapchain.width(), swapchain.height());
        self.blit_pass.execute(context, &mut cmd_buf, &params.assets, &sharpened_view, &backbuffer_view, sampler, resolution);
        std::mem::drop(sharpened_view);
        cmd_buf.barrier(&[Barrier::RawTextureBarrier {
            old_sync: BarrierSync::RENDER_TARGET, // BarrierSync::COPY,
            new_sync: BarrierSync::empty(),
            old_access: BarrierAccess::RENDER_TARGET_WRITE, // BarrierAccess::COPY_WRITE,
            new_access: BarrierAccess::empty(),
            old_layout: TextureLayout::RenderTarget, // TextureLayout::CopyDst,
            new_layout: TextureLayout::Present,
            texture: backbuffer_handle,
            range: BarrierTextureRange::default(),
            queue_ownership: None
        }]);

        Ok(RenderPathResult {
            cmd_buffer: cmd_buf.finish(),
            backbuffer: Some(backbuffer)
        })
    }

    fn set_ui_data(&mut self, _data: crate::ui::UIDrawData) {
    }
}

pub fn setup_frame(cmd_buf: &mut CommandBuffer, frame_bindings: &FrameBindings) {
    for i in 0..7 {
        cmd_buf.bind_storage_buffer(
            BindingFrequency::Frame,
            i,
            frame_bindings.gpu_scene_buffer,
            0,
            WHOLE_BUFFER,
        );
    }
    cmd_buf.bind_uniform_buffer(
        BindingFrequency::Frame,
        7,
        frame_bindings.camera_buffer,
        0,
        WHOLE_BUFFER,
    );
    cmd_buf.bind_uniform_buffer(
        BindingFrequency::Frame,
        8,
        frame_bindings.camera_history_buffer,
        0,
        WHOLE_BUFFER,
    );
    cmd_buf.bind_storage_buffer(
        BindingFrequency::Frame,
        9,
        frame_bindings.vertex_buffer,
        0,
        WHOLE_BUFFER,
    );
    cmd_buf.bind_storage_buffer(
        BindingFrequency::Frame,
        10,
        frame_bindings.index_buffer,
        0,
        WHOLE_BUFFER,
    );
    cmd_buf.bind_uniform_buffer(
        BindingFrequency::Frame,
        11,
        BufferRef::Transient(&frame_bindings.setup_buffer),
        0,
        WHOLE_BUFFER,
    );
    cmd_buf.bind_uniform_buffer(
        BindingFrequency::Frame,
        12,
        BufferRef::Transient(&frame_bindings.point_lights),
        0,
        WHOLE_BUFFER,
    );
    cmd_buf.bind_uniform_buffer(
        BindingFrequency::Frame,
        13,
        BufferRef::Transient(&frame_bindings.directional_lights),
        0,
          WHOLE_BUFFER,
    );
}
