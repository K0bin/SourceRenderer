use sourcerenderer_core::gpu::{self, SamplerInfo};
use web_sys::{GpuAddressMode, GpuDevice, GpuFilterMode, GpuMipmapFilterMode, GpuSampler, GpuSamplerDescriptor};

use crate::pipeline::compare_func_to_webgpu;

pub struct WebGPUSampler {
    sampler: GpuSampler
}

unsafe impl Send for WebGPUSampler {}
unsafe impl Sync for WebGPUSampler {}

fn filter_to_webgpu(filter: gpu::Filter) -> GpuFilterMode {
    match filter {
        gpu::Filter::Linear => GpuFilterMode::Linear,
        gpu::Filter::Nearest => GpuFilterMode::Nearest,
        gpu::Filter::Min => panic!("Min filter mode is not supported by WebGPU"),
        gpu::Filter::Max => panic!("Min filter mode is not supported by WebGPU"),
    }
}

fn filter_to_webgpu_mip(filter: gpu::Filter) -> GpuMipmapFilterMode {
    match filter {
        gpu::Filter::Linear => GpuMipmapFilterMode::Linear,
        gpu::Filter::Nearest => GpuMipmapFilterMode::Nearest,
        gpu::Filter::Min => panic!("Min filter mode is not supported by WebGPU"),
        gpu::Filter::Max => panic!("Min filter mode is not supported by WebGPU"),
    }
}

fn address_mode_to_webgpu(address_mode: gpu::AddressMode) -> GpuAddressMode {
    match address_mode {
        gpu::AddressMode::Repeat => GpuAddressMode::Repeat,
        gpu::AddressMode::MirroredRepeat => GpuAddressMode::MirrorRepeat,
        gpu::AddressMode::ClampToEdge => GpuAddressMode::ClampToEdge,
        gpu::AddressMode::ClampToBorder => GpuAddressMode::ClampToEdge,
    }
}

impl WebGPUSampler {
    pub fn new(device: &GpuDevice, info: &SamplerInfo, name: Option<&str>) -> Result<Self, ()> {
        let descriptor = GpuSamplerDescriptor::new();
        descriptor.set_min_filter(filter_to_webgpu(info.min_filter));
        descriptor.set_mag_filter(filter_to_webgpu(info.mag_filter));
        descriptor.set_mipmap_filter(filter_to_webgpu_mip(info.mip_filter));
        descriptor.set_address_mode_u(address_mode_to_webgpu(info.address_mode_u));
        descriptor.set_address_mode_v(address_mode_to_webgpu(info.address_mode_v));
        descriptor.set_address_mode_w(address_mode_to_webgpu(info.address_mode_w));
        descriptor.set_max_anisotropy(info.max_anisotropy as u16);
        descriptor.set_lod_min_clamp(info.min_lod);
        if let Some(max_lod) = info.max_lod {
            descriptor.set_lod_max_clamp(max_lod);
        }
        if let Some(compare_op) = info.compare_op {
            descriptor.set_compare(compare_func_to_webgpu(compare_op));
        }
        if let Some(name) = name {
            descriptor.set_label(name);
        }

        let sampler = device.create_sampler_with_descriptor(&descriptor);
        Ok(Self {
            sampler
        })
    }

    pub(crate) fn handle(&self) -> &GpuSampler {
        &self.sampler
    }
}
