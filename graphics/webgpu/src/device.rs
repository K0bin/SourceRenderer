use crate::{
    WebGPUBackend, WebGPUBuffer, WebGPUComputePipeline, WebGPUFence, WebGPUGraphicsPipeline,
    WebGPUHeap, WebGPUQueryPool, WebGPUQueue, WebGPUSampler, WebGPUShader, WebGPUShared,
    WebGPUTexture, WebGPUTextureView,
};
use bitflags::bitflags;
use js_sys::wasm_bindgen::{JsCast, prelude::Closure};
use sourcerenderer_core::gpu::{MemoryInfo, PipelineShaderStage};
use sourcerenderer_core::{
    align_up_32,
    gpu::{self, Texture as _},
};
use std::marker::PhantomData;
use web_sys::{GpuDevice, GpuExtent3dDict, GpuTexelCopyBufferLayout, GpuTexelCopyTextureInfo};

bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
    pub struct WebGPUFeatures : u32 {
        const BGR8_UNORM_STORAGE = 1;
        const CLIP_DISTANCES = 1 << 1;
        const DEPTH_CLIP_CONTROL = 1 << 2;
        const DEPTH32FLOAT_STENCIL8 = 1 << 3;
        const DUAL_SOURCE_BLENDING = 1 << 4;
        const FLOAT32_BLENDABLE = 1 << 5;
        const FLOAT32_FILTERABLE = 1 << 6;
        const INDIRECT_FIRST_INSTANCE = 1 << 7;
        const RG11B10_UFLOAT_RENDERABLE = 1 << 8;
        const SHADER_F16 = 1 << 9;
        const TEXTURE_COMPRESSION_BC = 1 << 10;
        const TEXTURE_COMPRESSION_BC_SLICED_3D = 1 << 11;
        const TEXTURE_COMPRESSION_ASTC = 1 << 12;
        const TEXTURE_COMPRESSION_ASTC_SLICED_3D = 1 << 13;
        const TEXTURE_COMPRESSION_ETC2 = 1 << 14;
        const TIMESTAMP_QUERY = 1 << 15;
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WebGPULimits {
    pub(crate) max_texture_dimension_1d: u32,
    pub(crate) max_texture_dimension_2d: u32,
    pub(crate) max_texture_dimension_3d: u32,
    pub(crate) max_texture_array_layers: u32,
    pub(crate) max_bind_groups: u32,
    pub(crate) max_bindings_per_bind_groups: u32,
    pub(crate) max_dynamic_uniform_buffers_per_pipeline_layout: u32,
    pub(crate) max_dynamic_storage_buffers_per_pipeline_layout: u32,
    pub(crate) max_sampled_textures_per_shader_stage: u32,
    pub(crate) max_samplers_per_shader_stage: u32,
    pub(crate) max_storage_buffers_per_shader_stage: u32,
    pub(crate) max_uniform_buffers_per_shader_stage: u32,
    pub(crate) max_storage_textures_per_shader_stage: u32,
    pub(crate) max_uniform_buffer_binding_size: u32,
    pub(crate) max_storage_buffer_binding_size: u32,
    pub(crate) min_uniform_buffer_offset_alignment: u32,
    pub(crate) min_storage_buffer_offset_alignment: u32,
    pub(crate) max_vertex_buffers: u32,
    pub(crate) max_buffer_size: u32,
    pub(crate) max_vertex_attributes: u32,
    pub(crate) max_vertex_buffer_array_stride: u32,
    #[allow(unused)]
    pub(crate) max_inter_stage_shader_components: u32,
    pub(crate) max_inter_stage_shader_variables: u32,
    pub(crate) max_color_attachments: u32,
    pub(crate) max_color_attachment_bytes_per_sample: u32,
    pub(crate) max_compute_workgroup_storage_size: u32,
    pub(crate) max_compute_invocations_per_workgroup: u32,
    pub(crate) max_compute_workgroup_size_x: u32,
    pub(crate) max_compute_workgroup_size_y: u32,
    pub(crate) max_compute_workgroup_size_z: u32,
    pub(crate) max_compute_workgroups_per_dimension: u32,
}

impl Default for WebGPULimits {
    fn default() -> Self {
        Self {
            max_texture_dimension_1d: 8192,
            max_texture_dimension_2d: 8192,
            max_texture_dimension_3d: 2048,
            max_texture_array_layers: 256,
            max_bind_groups: 4,
            max_bindings_per_bind_groups: 640,
            max_dynamic_uniform_buffers_per_pipeline_layout: 8,
            max_dynamic_storage_buffers_per_pipeline_layout: 4,
            max_sampled_textures_per_shader_stage: 16,
            max_samplers_per_shader_stage: 16,
            max_storage_buffers_per_shader_stage: 8,
            max_storage_textures_per_shader_stage: 4,
            max_uniform_buffers_per_shader_stage: 12,
            max_uniform_buffer_binding_size: 65536,
            max_storage_buffer_binding_size: 128 << 20,
            min_uniform_buffer_offset_alignment: 256,
            min_storage_buffer_offset_alignment: 256,
            max_vertex_buffers: 8,
            max_buffer_size: 256 << 20,
            max_vertex_attributes: 16,
            max_vertex_buffer_array_stride: 2048,
            max_inter_stage_shader_components: 60,
            max_inter_stage_shader_variables: 16,
            max_color_attachments: 8,
            max_color_attachment_bytes_per_sample: 32,
            max_compute_workgroup_storage_size: 16384,
            max_compute_invocations_per_workgroup: 256,
            max_compute_workgroup_size_x: 256,
            max_compute_workgroup_size_y: 256,
            max_compute_workgroup_size_z: 64,
            max_compute_workgroups_per_dimension: 65535,
        }
    }
}

pub struct WebGPUDevice {
    device: GpuDevice,
    shared: WebGPUShared,
    memory_infos: [gpu::MemoryTypeInfo; 1],
    queue: WebGPUQueue,
    features: WebGPUFeatures,
    limits: WebGPULimits,
    _p: PhantomData<*const std::ffi::c_void>,
}

impl WebGPUDevice {
    pub(crate) fn new(device: GpuDevice, debug: bool) -> Self {
        let memory_infos: [gpu::MemoryTypeInfo; 1] = [gpu::MemoryTypeInfo {
            is_cached: true,
            is_coherent: false,
            is_cpu_accessible: true,
            memory_index: 0,
            memory_kind: gpu::MemoryKind::VRAM,
        }];

        if debug {
            log::info!("Initializing device with error callback.");
            let callback_closure = Closure::wrap(Box::new(move |event: web_sys::Event| {
                Self::on_uncaptured_error(event);
            }) as Box<dyn FnMut(_)>);
            device
                .add_event_listener_with_callback(
                    "uncapturederror",
                    callback_closure.as_ref().unchecked_ref(),
                )
                .unwrap();
            std::mem::forget(callback_closure);
        }

        let mut features = WebGPUFeatures::empty();
        let js_features = device.features();
        if js_features.has("bgra8unorm-storage") {
            features |= WebGPUFeatures::BGR8_UNORM_STORAGE;
        }
        if js_features.has("clip-distances") {
            features |= WebGPUFeatures::CLIP_DISTANCES;
        }
        if js_features.has("depth-clip-control") {
            features |= WebGPUFeatures::DEPTH_CLIP_CONTROL;
        }
        if js_features.has("depth32float-stencil8") {
            features |= WebGPUFeatures::DEPTH32FLOAT_STENCIL8;
        }
        if js_features.has("dual-source-blending") {
            features |= WebGPUFeatures::DUAL_SOURCE_BLENDING;
        }
        if js_features.has("float32-blendable") {
            features |= WebGPUFeatures::FLOAT32_BLENDABLE;
        }
        if js_features.has("float32-filterable") {
            features |= WebGPUFeatures::FLOAT32_FILTERABLE;
        }
        if js_features.has("indirect-first-instance") {
            features |= WebGPUFeatures::INDIRECT_FIRST_INSTANCE;
        }
        if js_features.has("rg11b10ufloat-renderable") {
            features |= WebGPUFeatures::RG11B10_UFLOAT_RENDERABLE;
        }
        if js_features.has("shader-f16") {
            features |= WebGPUFeatures::SHADER_F16;
        }
        if js_features.has("texture-compression-bc") {
            features |= WebGPUFeatures::TEXTURE_COMPRESSION_BC;
        }
        if js_features.has("texture-compression-bc-sliced-3d") {
            features |= WebGPUFeatures::TEXTURE_COMPRESSION_BC_SLICED_3D;
        }
        if js_features.has("texture-compression-astc") {
            features |= WebGPUFeatures::TEXTURE_COMPRESSION_ASTC;
        }
        if js_features.has("texture-compression-astc-sliced-3d") {
            features |= WebGPUFeatures::TEXTURE_COMPRESSION_ASTC_SLICED_3D;
        }
        if js_features.has("texture-compression-etc2") {
            features |= WebGPUFeatures::TEXTURE_COMPRESSION_ETC2;
        }
        if js_features.has("timestamp-query") {
            features |= WebGPUFeatures::TIMESTAMP_QUERY;
        }

        let mut limits = WebGPULimits::default();
        let js_limits = device.limits();
        limits.max_texture_dimension_1d = js_limits.max_texture_dimension_1d();
        limits.max_texture_dimension_2d = js_limits.max_texture_dimension_2d();
        limits.max_texture_dimension_3d = js_limits.max_texture_dimension_3d();
        limits.max_texture_array_layers = js_limits.max_texture_array_layers();
        limits.max_bind_groups = js_limits.max_bind_groups();
        limits.max_bindings_per_bind_groups = js_limits.max_bindings_per_bind_group();
        limits.max_dynamic_uniform_buffers_per_pipeline_layout =
            js_limits.max_dynamic_uniform_buffers_per_pipeline_layout();
        limits.max_dynamic_storage_buffers_per_pipeline_layout =
            js_limits.max_dynamic_storage_buffers_per_pipeline_layout();
        limits.max_sampled_textures_per_shader_stage =
            js_limits.max_sampled_textures_per_shader_stage();
        limits.max_samplers_per_shader_stage = js_limits.max_samplers_per_shader_stage();
        limits.max_storage_buffers_per_shader_stage =
            js_limits.max_storage_buffers_per_shader_stage();
        limits.max_uniform_buffers_per_shader_stage =
            js_limits.max_uniform_buffers_per_shader_stage();
        limits.max_storage_textures_per_shader_stage =
            js_limits.max_storage_textures_per_shader_stage();
        limits.max_uniform_buffer_binding_size = js_limits.max_uniform_buffer_binding_size() as u32;
        limits.max_storage_buffer_binding_size = js_limits.max_storage_buffer_binding_size() as u32;
        limits.min_uniform_buffer_offset_alignment =
            js_limits.min_uniform_buffer_offset_alignment();
        limits.max_vertex_buffers = js_limits.max_vertex_buffers();
        limits.max_buffer_size = js_limits.max_buffer_size() as u32;
        limits.max_vertex_attributes = js_limits.max_vertex_attributes();
        limits.max_vertex_buffer_array_stride = js_limits.max_vertex_buffer_array_stride();
        //limits.max_inter_stage_shader_components = js_limits.max_inter_stage_shader_components(); // missing for some reason
        limits.max_inter_stage_shader_variables = js_limits.max_inter_stage_shader_variables();
        limits.max_color_attachments = js_limits.max_color_attachments();
        limits.max_color_attachment_bytes_per_sample =
            js_limits.max_color_attachment_bytes_per_sample();
        limits.max_color_attachment_bytes_per_sample =
            js_limits.max_color_attachment_bytes_per_sample();
        limits.max_compute_workgroup_storage_size = js_limits.max_compute_workgroup_storage_size();
        limits.max_compute_invocations_per_workgroup =
            js_limits.max_compute_invocations_per_workgroup();
        limits.max_compute_workgroup_size_x = js_limits.max_compute_workgroup_size_x();
        limits.max_compute_workgroup_size_y = js_limits.max_compute_workgroup_size_y();
        limits.max_compute_workgroup_size_z = js_limits.max_compute_workgroup_size_z();
        limits.max_compute_workgroups_per_dimension =
            js_limits.max_compute_workgroups_per_dimension();

        let shared = WebGPUShared::new(&device);
        let queue = WebGPUQueue::new(&device, &limits);

        Self {
            device,
            shared,
            memory_infos,
            queue,
            features: features.clone(),
            limits: limits.clone(),
            _p: PhantomData,
        }
    }

    #[inline(always)]
    pub fn handle(&self) -> &GpuDevice {
        &self.device
    }

    fn on_uncaptured_error(event: web_sys::Event) {
        let webgpu_error = event
            .dyn_into::<web_sys::GpuUncapturedErrorEvent>()
            .unwrap();
        log::error!(
            "Uncaptured WebGPU error: {}",
            webgpu_error.error().message()
        )
    }

    pub(crate) fn used_webgpu_features() -> &'static [&'static str] {
        &[
            "core-features-and-limits",
            "bgra8unorm-storage",
            "depth32float-stencil8",
        ]
    }
}

impl Drop for WebGPUDevice {
    fn drop(&mut self) {
        self.device.destroy();
    }
}

impl gpu::Device<WebGPUBackend> for WebGPUDevice {
    fn create_buffer(
        &self,
        info: &gpu::BufferInfo,
        memory_type_index: u32,
        name: Option<&str>,
    ) -> Result<WebGPUBuffer, gpu::OutOfMemoryError> {
        let mem = &self.memory_infos[memory_type_index as usize];
        WebGPUBuffer::new(&self.device, info, mem.is_cpu_accessible, name)
            .map_err(|_e| gpu::OutOfMemoryError {})
    }

    fn create_texture(
        &self,
        info: &gpu::TextureInfo,
        _memory_type_index: u32,
        name: Option<&str>,
    ) -> Result<WebGPUTexture, gpu::OutOfMemoryError> {
        WebGPUTexture::new(&self.device, info, name).map_err(|_e| gpu::OutOfMemoryError {})
    }

    fn create_shader(&self, shader: &gpu::PackedShader, name: Option<&str>) -> WebGPUShader {
        WebGPUShader::new(&self.device, shader, name)
    }

    fn create_texture_view(
        &self,
        texture: &WebGPUTexture,
        info: &gpu::TextureViewInfo,
        name: Option<&str>,
    ) -> WebGPUTextureView {
        WebGPUTextureView::new(&self.device, texture, info, name).unwrap()
    }

    fn create_compute_pipeline(
        &self,
        shader: PipelineShaderStage<WebGPUBackend>,
        name: Option<&str>,
    ) -> WebGPUComputePipeline {
        WebGPUComputePipeline::new(&self.device, shader, &self.shared, name, &self.limits).unwrap()
    }

    fn create_sampler(&self, info: &gpu::SamplerInfo, name: Option<&str>) -> WebGPUSampler {
        WebGPUSampler::new(&self.device, info, name).unwrap()
    }

    fn create_graphics_pipeline(
        &self,
        info: &gpu::GraphicsPipelineInfo<WebGPUBackend>,
        name: Option<&str>,
    ) -> WebGPUGraphicsPipeline {
        WebGPUGraphicsPipeline::new(&self.device, info, &self.shared, name, &self.limits).unwrap()
    }

    unsafe fn block_until_idle(&self) {}

    fn create_fence(&self, _is_cpu_accessible: bool) -> WebGPUFence {
        WebGPUFence::new(&self.device)
    }

    fn memory_infos(&self) -> Box<[MemoryInfo]> {
        /*
           TODO: Implement rudimentary memory tracking by having a fixed number and change it in WebGPUTexture, WebGPUBuffer and WebGPUHeap.
           Increase it in the constructor and decrease it in the destructor.
        */
        vec![gpu::MemoryInfo {
            available: (u32::MAX as u64) / 3u64,
            total: (u32::MAX as u64) / 3u64,
            memory_kind: gpu::MemoryKind::VRAM,
        }]
        .into_boxed_slice()
    }

    fn memory_type_infos(&self) -> &[gpu::MemoryTypeInfo] {
        &self.memory_infos
    }

    fn create_heap(
        &self,
        memory_type_index: u32,
        size: u64,
    ) -> Result<WebGPUHeap, gpu::OutOfMemoryError> {
        let mem = &self.memory_infos[memory_type_index as usize];
        Ok(WebGPUHeap::new(
            &self.device,
            memory_type_index,
            size,
            mem.is_cpu_accessible,
        ))
    }

    fn get_buffer_heap_info(&self, info: &gpu::BufferInfo) -> gpu::ResourceHeapInfo {
        let mut alignment = 4;
        if info.usage.contains(gpu::BufferUsage::CONSTANT) {
            alignment = alignment.max(self.limits.min_uniform_buffer_offset_alignment);
        }
        if info.usage.contains(gpu::BufferUsage::STORAGE) {
            alignment = alignment.max(self.limits.min_storage_buffer_offset_alignment);
        }
        gpu::ResourceHeapInfo {
            dedicated_allocation_preference: if info.usage.gpu_writable() {
                // WebGPU does tracking for barriers at the resource level.
                gpu::DedicatedAllocationPreference::PreferDedicated
            } else {
                gpu::DedicatedAllocationPreference::PreferSuballocated
            },
            memory_type_mask: 1,
            alignment: alignment as u64,
            size: info.size,
        }
    }

    fn get_texture_heap_info(&self, info: &gpu::TextureInfo) -> gpu::ResourceHeapInfo {
        gpu::ResourceHeapInfo {
            dedicated_allocation_preference: gpu::DedicatedAllocationPreference::PreferDedicated,
            memory_type_mask: 1,
            alignment: 4,
            size: (info.width * info.height * info.array_length * (4 * 4)) as u64, // TODO: We just assume RGBA Float32, make this take the format into account properly
        }
    }

    unsafe fn insert_texture_into_bindless_heap(&self, _slot: u32, _texture: &WebGPUTextureView) {
        panic!("WebGPU does not support bindless textures");
    }

    fn graphics_queue(&self) -> &WebGPUQueue {
        &self.queue
    }

    fn compute_queue(&self) -> Option<&WebGPUQueue> {
        None
    }

    fn transfer_queue(&self) -> Option<&WebGPUQueue> {
        None
    }

    fn supports_bindless(&self) -> bool {
        false
    }

    fn supports_ray_tracing_pipeline(&self) -> bool {
        false
    }

    fn supports_ray_tracing_query(&self) -> bool {
        false
    }

    fn supports_indirect_count(&self) -> bool {
        false
    }

    fn supports_indirect_first_instance(&self) -> bool {
        self.features
            .contains(WebGPUFeatures::INDIRECT_FIRST_INSTANCE)
    }

    fn supports_indirect_count_mesh_shader(&self) -> bool {
        false
    }

    fn supports_min_max_filter(&self) -> bool {
        false
    }

    fn supports_barycentrics(&self) -> bool {
        false
    }

    fn supports_mesh_shader(&self) -> bool {
        false
    }

    fn get_bottom_level_acceleration_structure_size(
        &self,
        _info: &gpu::BottomLevelAccelerationStructureInfo<WebGPUBackend>,
    ) -> gpu::AccelerationStructureSizes {
        panic!("WebGPU does not support ray tracing")
    }

    fn get_top_level_acceleration_structure_size(
        &self,
        _info: &gpu::TopLevelAccelerationStructureInfo<WebGPUBackend>,
    ) -> gpu::AccelerationStructureSizes {
        panic!("WebGPU does not support ray tracing")
    }

    fn get_top_level_instances_buffer_size(
        &self,
        _instances: &[gpu::AccelerationStructureInstance<WebGPUBackend>],
    ) -> u64 {
        panic!("WebGPU does not support ray tracing")
    }

    fn get_raytracing_pipeline_sbt_buffer_size(
        &self,
        _info: &gpu::RayTracingPipelineInfo<WebGPUBackend>,
    ) -> u64 {
        panic!("WebGPU does not support ray tracing")
    }

    unsafe fn create_raytracing_pipeline(
        &self,
        _info: &gpu::RayTracingPipelineInfo<WebGPUBackend>,
        _sbt_buffer: &WebGPUBuffer,
        _sbt_buffer_offset: u64,
        _name: Option<&str>,
    ) -> () {
        panic!("WebGPU does not support ray tracing")
    }

    fn create_mesh_graphics_pipeline(
        &self,
        _info: &gpu::MeshGraphicsPipelineInfo<WebGPUBackend>,
        _name: Option<&str>,
    ) -> <WebGPUBackend as gpu::GPUBackend>::MeshGraphicsPipeline {
        panic!("WebGPU does not support mesh shaders")
    }

    unsafe fn transition_texture(
        &self,
        _dst: &WebGPUTexture,
        _transition: &gpu::CPUTextureTransition<'_, WebGPUBackend>,
    ) {
    }

    unsafe fn copy_to_texture(
        &self,
        src: *const std::ffi::c_void,
        dst: &WebGPUTexture,
        _texture_layout: gpu::TextureLayout,
        region: &gpu::MemoryTextureCopyRegion,
    ) {
        let src_info = GpuTexelCopyBufferLayout::new();

        let format = dst.info().format;
        let row_pitch = if region.row_pitch != 0 {
            region.row_pitch
        } else {
            (align_up_32(region.texture_extent.x, format.block_size().x) / format.block_size().x
                * format.element_size()) as u64
        };
        let slice_pitch = if region.slice_pitch != 0 {
            region.slice_pitch
        } else {
            (align_up_32(region.texture_extent.y, format.block_size().y) / format.block_size().y)
                as u64
                * row_pitch
        };
        assert_eq!(slice_pitch % row_pitch, 0);

        src_info.set_bytes_per_row(row_pitch as u32);
        src_info.set_rows_per_image((slice_pitch / row_pitch) as u32);
        let dst_info = GpuTexelCopyTextureInfo::new(dst.handle());
        dst_info.set_mip_level(region.texture_subresource.mip_level);
        let mut origin = [
            js_sys::Number::from(0),
            js_sys::Number::from(0),
            js_sys::Number::from(0),
        ];
        origin[0] = js_sys::Number::from(region.texture_offset.x as f64);
        origin[1] = js_sys::Number::from(region.texture_offset.y as f64);
        let copy_size = GpuExtent3dDict::new(region.texture_extent.x);
        copy_size.set_height(region.texture_extent.y);
        assert!(
            dst.info().array_length == 0 || dst.info().dimension != gpu::TextureDimension::Dim3D
        );
        if dst.info().dimension == gpu::TextureDimension::Dim3D {
            assert_eq!(region.texture_subresource.array_layer, 0);
            copy_size.set_depth_or_array_layers(region.texture_extent.z);
            origin[2] = js_sys::Number::from(region.texture_offset.z as f64);
        } else {
            assert_eq!(region.texture_extent.z, 1);
            assert_eq!(region.texture_offset.z, 0);
            copy_size.set_depth_or_array_layers(1);
            origin[2] = js_sys::Number::from(region.texture_subresource.array_layer as f64);
        }
        dst_info.set_origin(&origin);

        let queue = self.queue.handle();
        let data_len = slice_pitch as usize * dst.info().depth as usize;
        let slice = unsafe { std::slice::from_raw_parts(src as *const u8, data_len) };
        queue
            .write_texture_with_u8_slice_and_gpu_extent_3d_dict(
                &dst_info, slice, &src_info, &copy_size,
            )
            .unwrap();
    }

    fn create_query_pool(&self, count: u32) -> WebGPUQueryPool {
        WebGPUQueryPool::new(&self.device, count)
    }

    fn create_split_barrier(&self) -> () {
        ()
    }
}
