use widestring::U16CString;
use windows::core::PCWSTR;
use windows::Win32::Graphics::Direct3D as D3D;
use windows::Win32::Graphics::Dxgi;
use windows::Win32::Graphics::Direct3D12 as D3D12;
use windows::core::Interface;

use sourcerenderer_core::gpu;

use super::*;

fn texture_dimension_to_d3d12(dimension: gpu::TextureDimension) -> D3D12::D3D12_RESOURCE_DIMENSION {
    match dimension {
        gpu::TextureDimension::Dim1D | gpu::TextureDimension::Dim1DArray => D3D12::D3D12_RESOURCE_DIMENSION_TEXTURE1D,
        gpu::TextureDimension::Dim2D | gpu::TextureDimension::Dim2DArray => D3D12::D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        gpu::TextureDimension::Dim3D => D3D12::D3D12_RESOURCE_DIMENSION_TEXTURE3D,
    }
}

fn format_to_d3d12(format: gpu::Format) -> Dxgi::Common::DXGI_FORMAT {
    unimplemented!()
}

pub struct D3D12Texture {
    texture: D3D12::ID3D12Resource2,
    info: gpu::TextureInfo
}

impl D3D12Texture {
    pub(crate) fn new(device: &D3D12::ID3D12Device12, memory: ResourceMemory, info: &gpu::TextureInfo, name: Option<&str>) -> Result<Self, gpu::OutOfMemoryError> {
        let mut flags = D3D12::D3D12_RESOURCE_FLAG_NONE;
        if !info.usage.contains(gpu::TextureUsage::SAMPLED) {
            flags |= D3D12::D3D12_RESOURCE_FLAG_DENY_SHADER_RESOURCE;
        }
        if info.usage.contains(gpu::TextureUsage::STORAGE) {
            flags |= D3D12::D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
        }
        if info.usage.contains(gpu::TextureUsage::RENDER_TARGET) {
            flags |= D3D12::D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET;
        }
        if info.usage.contains(gpu::TextureUsage::DEPTH_STENCIL) {
            flags |= D3D12::D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL;
        }

        let mut desc = D3D12::D3D12_RESOURCE_DESC1 {
            Dimension: texture_dimension_to_d3d12(info.dimension),
            Alignment: 0, // the runtime automatically picks it with 0
            Width: info.width as u64,
            Height: info.height,
            DepthOrArraySize: if info.dimension == gpu::TextureDimension::Dim3D { info.depth } else { info.array_length } as u16,
            MipLevels: info.mip_levels as u16,
            Format: Dxgi::Common::DXGI_FORMAT_UNKNOWN,
            SampleDesc: Dxgi::Common::DXGI_SAMPLE_DESC {
                Count: match info.samples {
                    gpu::SampleCount::Samples1 => 1,
                    gpu::SampleCount::Samples2 => 2,
                    gpu::SampleCount::Samples4 => 4,
                    gpu::SampleCount::Samples8 => 8,
                },
                Quality: 0,
            },
            Layout: D3D12::D3D12_TEXTURE_LAYOUT_64KB_UNDEFINED_SWIZZLE,
            Flags: flags,
            SamplerFeedbackMipRegion: D3D12::D3D12_MIP_REGION { Width: 0u32, Height: 0u32, Depth: 0u32 },
        };

        let optimized_clear_value = if info.usage.intersects(gpu::TextureUsage::RENDER_TARGET) {
            Some(D3D12::D3D12_CLEAR_VALUE {
                Format: desc.Format,
                Anonymous: D3D12::D3D12_CLEAR_VALUE_0 { Color: [0f32, 0f32, 0f32, 0f32] }
            })
        } else if info.usage.intersects(gpu::TextureUsage::DEPTH_STENCIL) {
            Some(D3D12::D3D12_CLEAR_VALUE {
                Format: desc.Format,
                Anonymous: D3D12::D3D12_CLEAR_VALUE_0 { DepthStencil: D3D12::D3D12_DEPTH_STENCIL_VALUE {
                    Depth: 0f32,
                    Stencil: 0u8,
                } },
            })
        } else {
            None
        };
        let optimized_clear_value_ptr = optimized_clear_value.as_ref().map(|val| val as *const D3D12::D3D12_CLEAR_VALUE);

        let mut compatible_formats = smallvec::SmallVec::<[Dxgi::Common::DXGI_FORMAT; 2]>::new();
        compatible_formats.push(desc.Format);
        let srgb_format = info.format.srgb_format();
        if let Some(srgb_format) = srgb_format {
            compatible_formats.push(format_to_d3d12(srgb_format));
        }
        let compatible_formats_opt = if info.supports_srgb {
            Some(&compatible_formats)
        } else {
            None
        };

        let mut resource_opt: Option<D3D12::ID3D12Resource2> = None;
        match memory {
            ResourceMemory::Dedicated { heap_type } => {
                let heap_properties = D3D12::D3D12_HEAP_PROPERTIES {
                    Type: heap_type,
                    CPUPageProperty: D3D12::D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
                    MemoryPoolPreference: D3D12::D3D12_MEMORY_POOL_UNKNOWN,
                    CreationNodeMask: 0,
                    VisibleNodeMask: 0,
                };

                let mut flags: D3D12::D3D12_HEAP_FLAGS = D3D12::D3D12_HEAP_FLAG_DENY_BUFFERS;
                flags |= D3D12::D3D12_HEAP_FLAG_CREATE_NOT_ZEROED;
                if heap_type == D3D12::D3D12_HEAP_TYPE_DEFAULT {
                    flags |= D3D12::D3D12_HEAP_FLAG_ALLOW_SHADER_ATOMICS;
                }
                if (info.usage & !(gpu::TextureUsage::RENDER_TARGET | gpu::TextureUsage::DEPTH_STENCIL)).is_empty() {
                    flags |= D3D12::D3D12_HEAP_FLAG_ALLOW_ONLY_RT_DS_TEXTURES;
                } else if !info.usage.intersects(gpu::TextureUsage::RENDER_TARGET | gpu::TextureUsage::DEPTH_STENCIL) {
                    flags |= D3D12::D3D12_HEAP_FLAG_ALLOW_ONLY_NON_RT_DS_TEXTURES;
                }

                unsafe {
                    let protected = Option::<&D3D12::ID3D12ProtectedResourceSession>::None;
                    device.CreateCommittedResource3(
                        &heap_properties as *const D3D12::D3D12_HEAP_PROPERTIES,
                        flags,
                        &desc as *const D3D12::D3D12_RESOURCE_DESC1,
                        D3D12::D3D12_BARRIER_LAYOUT_UNDEFINED,
                        optimized_clear_value_ptr,
                        protected,
                        compatible_formats_opt.map(|vec| &vec[..]),
                        &mut resource_opt as *mut Option<D3D12::ID3D12Resource2>
                    )
                }
            },
            ResourceMemory::Suballocated { memory: heap, offset } => {
                unsafe {
                    device.CreatePlacedResource2(
                        heap.handle(), offset,
                        &desc as *const D3D12::D3D12_RESOURCE_DESC1,
                        D3D12::D3D12_BARRIER_LAYOUT_UNDEFINED,
                        optimized_clear_value_ptr,
                        compatible_formats_opt.map(|vec| &vec[..]),
                        &mut resource_opt as *mut Option<D3D12::ID3D12Resource2>
                    )
                }
            },
        }.map_err(|_e| gpu::OutOfMemoryError {})?;

        let resource = resource_opt.unwrap();
        if let Some(name) = name {
            let wstr = U16CString::from_str(name);
            if let Ok(wstr) = wstr {
                unsafe {
                    resource.SetName(PCWSTR(wstr.as_ptr()));
                }
            }
        }

        Ok(Self {
            texture: resource,
            info: info.clone()
        })
    }
}

impl gpu::Texture for D3D12Texture {
    fn info(&self) -> &gpu::TextureInfo {
        &self.info
    }
}

impl PartialEq<D3D12Texture> for D3D12Texture {
    fn eq(&self, other: &D3D12Texture) -> bool {
        self.texture == other.texture
    }
}

impl Eq for D3D12Texture {}

pub struct D3D12TextureView {
    index: u32,
    handle: D3D12::D3D12_CPU_DESCRIPTOR_HANDLE,
    texture_info: gpu::TextureInfo,
    info: gpu::TextureViewInfo
}

impl gpu::TextureView for D3D12TextureView {
    fn texture_info(&self) -> &gpu::TextureInfo {
        &self.texture_info
    }

    fn info(&self) -> &gpu::TextureViewInfo {
        &self.info
    }
}

impl PartialEq<D3D12TextureView> for D3D12TextureView {
    fn eq(&self, other: &D3D12TextureView) -> bool {
        self.handle == other.handle
    }
}

impl Eq for D3D12TextureView {}

pub struct D3D12Sampler {
    index: u32,
    handle: D3D12::D3D12_CPU_DESCRIPTOR_HANDLE,
    info: gpu::SamplerInfo
}

impl D3D12Sampler {
    pub(crate) fn new(device: &D3D12::ID3D12Device12, info: &gpu::SamplerInfo) -> Self {
        unimplemented!()
    }
}
