use bitflags::bitflags;

use sourcerenderer_core::gpu;
use web_sys::{GpuAdapter, GpuDevice};

use crate::{WebGPUBackend, WebGPUDevice, WebGPUSurface};

pub struct WebGPUAdapter {
    _adapter: GpuAdapter,
    device: GpuDevice,
    debug: bool,
    features: WebGPUFeatures,
    limits: WebGPULimits,
    adapter_type: gpu::AdapterType
}

impl WebGPUAdapter {
    pub fn new(adapter: GpuAdapter, device: GpuDevice, adapter_type: gpu::AdapterType, debug: bool) -> Self {
        let mut features = WebGPUFeatures::empty();
        let js_features = adapter.features();
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
        let js_limits = adapter.limits();
        limits.max_texture_dimension_1d = js_limits.max_texture_dimension_1d();
        limits.max_texture_dimension_2d = js_limits.max_texture_dimension_2d();
        limits.max_texture_dimension_3d = js_limits.max_texture_dimension_3d();
        limits.max_texture_array_layers = js_limits.max_texture_array_layers();
        limits.max_bind_groups = js_limits.max_bind_groups();
        limits.max_bindings_per_bind_groups = js_limits.max_bindings_per_bind_group();
        limits.max_dynamic_uniform_buffers_per_pipeline_layout = js_limits.max_dynamic_uniform_buffers_per_pipeline_layout();
        limits.max_dynamic_storage_buffers_per_pipeline_layout = js_limits.max_dynamic_storage_buffers_per_pipeline_layout();
        limits.max_sampled_textures_per_shader_stage = js_limits.max_sampled_textures_per_shader_stage();
        limits.max_samplers_per_shader_stage = js_limits.max_samplers_per_shader_stage();
        limits.max_storage_buffers_per_shader_stage = js_limits.max_storage_buffers_per_shader_stage();
        limits.max_uniform_buffers_per_shader_stage = js_limits.max_uniform_buffers_per_shader_stage();
        limits.max_storage_textures_per_shader_stage = js_limits.max_storage_textures_per_shader_stage();
        limits.max_uniform_buffer_binding_size = js_limits.max_uniform_buffer_binding_size() as u32;
        limits.max_storage_buffer_binding_size = js_limits.max_storage_buffer_binding_size() as u32;
        limits.min_uniform_buffer_offset_alignment = js_limits.min_uniform_buffer_offset_alignment();
        limits.max_vertex_buffers = js_limits.max_vertex_buffers();
        limits.max_buffer_size = js_limits.max_buffer_size() as u32;
        limits.max_vertex_attributes = js_limits.max_vertex_attributes();
        limits.max_vertex_buffer_array_stride = js_limits.max_vertex_buffer_array_stride();
        //limits.max_inter_stage_shader_components = js_limits.max_inter_stage_shader_components(); // missing for some reason
        limits.max_inter_stage_shader_variables = js_limits.max_inter_stage_shader_variables();
        limits.max_color_attachments = js_limits.max_color_attachments();
        limits.max_color_attachment_bytes_per_sample = js_limits.max_color_attachment_bytes_per_sample();
        limits.max_color_attachment_bytes_per_sample = js_limits.max_color_attachment_bytes_per_sample();
        limits.max_compute_workgroup_storage_size = js_limits.max_compute_workgroup_storage_size();
        limits.max_compute_invocations_per_workgroup = js_limits.max_compute_invocations_per_workgroup();
        limits.max_compute_workgroup_size_x = js_limits.max_compute_workgroup_size_x();
        limits.max_compute_workgroup_size_y = js_limits.max_compute_workgroup_size_y();
        limits.max_compute_workgroup_size_z = js_limits.max_compute_workgroup_size_z();
        limits.max_compute_workgroups_per_dimension = js_limits.max_compute_workgroups_per_dimension();

        log::info!("Adapter features: {:?}", &features);
        log::info!("Adapter limits: {:?}", &limits);

        Self {
            _adapter: adapter,
            device,
            debug,
            features,
            limits,
            adapter_type
        }
    }
}

unsafe impl Send for WebGPUAdapter {}
unsafe impl Sync for WebGPUAdapter {}

impl gpu::Adapter<WebGPUBackend> for WebGPUAdapter {
    fn adapter_type(&self) -> sourcerenderer_core::gpu::AdapterType {
        self.adapter_type
    }

    unsafe fn create_device(&self, _surface: &WebGPUSurface) -> WebGPUDevice {
        WebGPUDevice::new(self.device.clone(), &self.features, &self.limits, self.debug)
    }
}

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
            max_compute_workgroups_per_dimension: 65535
        }
    }
}
