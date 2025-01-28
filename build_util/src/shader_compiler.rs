use core::panic;
use std::collections::HashMap;
use std::ffi::{c_char, CStr, c_void};
use std::fs::*;
use std::io::{Read, Write};
use std::path::*;
use std::process::Command;

use bitflags::bitflags;

use log::{error, info};
use naga::back::wgsl::WriterFlags;
use naga::front::spv::Options;
use naga::valid::{Capabilities, ValidationFlags, Validator};
use spirv_cross_sys;

use sourcerenderer_core::gpu;

use super::spirv_transformer::*;

fn make_spirv_cross_msl_version(major: u32, minor: u32, patch: u32) -> u32 {
    major * 10000 + minor * 100 + patch
}

pub fn compile_shaders<F>(
    source_dir: &Path,
    out_dir: &Path,
    include_debug_info: bool,
    dump_separate_files: bool,
    arguments: &HashMap<String, String>,
    shading_languages: ShadingLanguage,
    file_filter: F,
) where
    F: Fn(&Path) -> bool,
{
    println!("cargo:rerun-if-changed={}", source_dir.to_str().unwrap());
    let contents = read_dir(&source_dir).expect("Shader directory couldn't be opened.");
    contents
        .filter(|file_result| file_result.is_ok())
        .map(|file_result| file_result.unwrap())
        .filter(|file| {
            file.path()
                .extension()
                .and_then(|os_str| os_str.to_str())
                .unwrap_or("")
                == "glsl"
                && !file
                    .path()
                    .file_stem()
                    .and_then(|ext| ext.to_str())
                    .map(|s| s.contains(".inc"))
                    .unwrap_or(false)
                && file_filter(&file.path())
        })
        .for_each(|file| {
            let file_path = file.path();
            if shading_languages.intersects(ShadingLanguage::SpirV | ShadingLanguage::Air | ShadingLanguage::Dxil | ShadingLanguage::Wgsl) {
                compile_shader(
                    &file_path,
                    out_dir,
                    shading_languages & (ShadingLanguage::SpirV | ShadingLanguage::Air | ShadingLanguage::Dxil | ShadingLanguage::Wgsl),
                    CompiledShaderFileType::Packed,
                    include_debug_info,
                    arguments,
                );
            }
            if dump_separate_files || shading_languages.intersects(ShadingLanguage::Msl | ShadingLanguage::Hlsl) {
                compile_shader(
                    &file_path,
                    out_dir,
                    if dump_separate_files { shading_languages } else { shading_languages & (ShadingLanguage::Msl | ShadingLanguage::Hlsl) },
                    CompiledShaderFileType::Bytecode,
                    include_debug_info,
                    arguments,
                );
            }
        });
}

bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct ShadingLanguage : u8 {
        const SpirV = 1;
        const Hlsl = 2;
        const Dxil = 4;
        const Msl = 8;
        const Air = 16;
        const Wgsl = 32;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompiledShaderFileType {
    Bytecode,
    Packed,
}

fn compile_shader_glsl(
    file_path: &Path,
    output_dir: &Path,
    shader_type: gpu::ShaderType,
    include_debug_info: bool,
    arguments: &HashMap<String, String>,
) -> Result<Vec<u8>, ()> {
    println!("cargo:rerun-if-changed={}", (file_path).to_str().unwrap());

    let mut command = Command::new("glslangValidator");
    command
        .arg("--target-env")
        .arg("spirv1.6")
        .arg("-V");

    let mut compiled_spv_file_name = file_path.file_stem().unwrap().to_str().unwrap().to_string();
    compiled_spv_file_name.push_str(".spv");
    compiled_spv_file_name.push_str(".temp");
    let compiled_spv_file_path = output_dir.join(compiled_spv_file_name);

    if include_debug_info {
        command.arg("-g");
    }
    if shader_type == gpu::ShaderType::ComputeShader {
        command.arg("-S").arg("comp");
    }

    for (key, value) in arguments {
        if !value.is_empty() {
            command.arg("-D".to_string() + key.as_str() + "=" + value.as_str());
        } else {
            command.arg("-D".to_string() + key.as_str());
        }
    }

    command.arg("-o").arg(&compiled_spv_file_path);
    command.arg(&file_path);

    let output_res = command.output();
    match &output_res {
        Err(e) => {
            error!("Failed to compile shader: {}",
                file_path.to_str().unwrap());
            error!("{}", e.to_string());
            return Err(());
        },
        Ok(output) => {
            if !output.status.success() {
                error!("Failed to compile shader: {}", file_path.to_str().unwrap());
                error!("{}", std::str::from_utf8(&output.stdout).unwrap());
                return Err(());
            }
        }
    }

    let mut spirv_bytecode = Vec::<u8>::new();
    {
        let file_res = std::fs::File::open(&compiled_spv_file_path);
        if let Err(e) = file_res {
            error!("Failed to open SPIR-V file: {:?} {:?}", compiled_spv_file_path, e);
            return Err(());
        }
        let mut file = file_res.unwrap();
        let read_res = file.read_to_end(&mut spirv_bytecode);
        if let Err(e) = read_res {
            error!("Failed to read SPIR-V file: {:?} {:?}", compiled_spv_file_path, e);
            return Err(());
        }
    }
    let _ = std::fs::remove_file(compiled_spv_file_path);
    Ok(spirv_bytecode)
}

struct CallbackInfo {
    shader_name: String,
    shading_lang: ShadingLanguage,
    do_panic: bool
}
unsafe extern "C" fn spirv_cross_error_callback(userdata: *mut c_void, error: *const c_char) {
    let info = userdata as *const CallbackInfo;
    let msg_cstr = CStr::from_ptr(error);
    if (*info).do_panic {
        panic!("SPIR-V-CROSS ERROR in shader: {} {:?}: {:?}", (*info).shader_name, (*info).shading_lang, msg_cstr);
    } else {
        error!("SPIR-V-CROSS ERROR in shader: {} {:?}: {:?}", (*info).shader_name, (*info).shading_lang, msg_cstr);
    }
}

fn read_metadata(
    spirv: &[u8],
    shader_name: &str,
    shader_type: gpu::ShaderType,
) -> gpu::PackedShader {
    let mut resources: [Vec<gpu::Resource>; gpu::NON_BINDLESS_SET_COUNT as usize] = Default::default();
    let mut push_constant_size = 0u32;
    let mut uses_bindless_texture_set = false;
    let mut stage_input_count = 0u32;
    let mut max_stage_input = 0u32;

    // Generate metadata
    let mut context: spirv_cross_sys::spvc_context = std::ptr::null_mut();
    let mut ir: spirv_cross_sys::spvc_parsed_ir = std::ptr::null_mut();
    let mut compiler: spirv_cross_sys::spvc_compiler = std::ptr::null_mut();
    let mut spv_resources: spirv_cross_sys::spvc_resources = std::ptr::null_mut();

    unsafe {
        assert_eq!(
            spirv_cross_sys::spvc_context_create(&mut context),
            spirv_cross_sys::spvc_result_SPVC_SUCCESS
        );

        let do_panic = std::env::var("SHADER_COMPILER_PANIC").map(|s| s == "1").unwrap_or(false);
        let mut info = CallbackInfo {
            shader_name: shader_name.to_string(),
            shading_lang: ShadingLanguage::empty(),
            do_panic
        };
        spirv_cross_sys::spvc_context_set_error_callback(context, Some(spirv_cross_error_callback), &mut info as *mut CallbackInfo as *mut c_void);

        assert_eq!(
            spirv_cross_sys::spvc_context_parse_spirv(
                context,
                spirv.as_ptr() as *const u32,
                spirv.len() / std::mem::size_of::<u32>(),
                &mut ir
            ),
            spirv_cross_sys::spvc_result_SPVC_SUCCESS
        );
        assert_eq!(
            spirv_cross_sys::spvc_context_create_compiler(
                context,
                spirv_cross_sys::spvc_backend_SPVC_BACKEND_NONE,
                ir,
                spirv_cross_sys::spvc_capture_mode_SPVC_CAPTURE_MODE_TAKE_OWNERSHIP,
                &mut compiler
            ),
            spirv_cross_sys::spvc_result_SPVC_SUCCESS
        );

        assert_eq!(
            spirv_cross_sys::spvc_compiler_create_shader_resources(
                compiler,
                &mut spv_resources
            ),
            spirv_cross_sys::spvc_result_SPVC_SUCCESS
        );
    }

    // PUSH CONSTANTS
    let push_constant_buffers = unsafe {
        let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource =
            std::ptr::null();
        let mut resources_count: usize = 0;
        assert_eq!(
            spirv_cross_sys::spvc_resources_get_resource_list_for_type(
                spv_resources,
                spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_PUSH_CONSTANT,
                &mut resources_list,
                &mut resources_count
            ),
            spirv_cross_sys::spvc_result_SPVC_SUCCESS
        );
        std::slice::from_raw_parts(resources_list, resources_count as usize)
    };
    let push_constant_resource = push_constant_buffers.first();
    if let Some(resource) = push_constant_resource {
        unsafe {
            let type_handle =
                spirv_cross_sys::spvc_compiler_get_type_handle(compiler, resource.type_id);
            assert_ne!(type_handle, std::ptr::null());
            let mut size = 0usize;
            assert_eq!(
                spirv_cross_sys::spvc_compiler_get_declared_struct_size(
                    compiler,
                    type_handle,
                    &mut size as *mut usize
                ),
                spirv_cross_sys::spvc_result_SPVC_SUCCESS
            );
            push_constant_size = size as u32;
        }
    };

    fn spvc_format_to_format(format: spirv_cross_sys::SpvImageFormat) -> gpu::Format {
        match format {
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRgba32f => gpu::Format::RGBA32Float,
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRgba16f => gpu::Format::RGBA16Float,
            spirv_cross_sys::SpvImageFormat__SpvImageFormatR32f => gpu::Format::R32Float,
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRgba8 => gpu::Format::RGBA8UNorm,
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRgba8Snorm => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRg32f => gpu::Format::RG32Float,
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRg16f => gpu::Format::RG16Float,
            spirv_cross_sys::SpvImageFormat__SpvImageFormatR11fG11fB10f => gpu::Format::R11G11B10Float,
            spirv_cross_sys::SpvImageFormat__SpvImageFormatR16f => gpu::Format::R16Float,
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRgba16 => gpu::Format::RGBA16Float,
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRgb10A2 => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRg16 => gpu::Format::RG16UNorm,
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRg8 => gpu::Format::RG8UNorm,
            spirv_cross_sys::SpvImageFormat__SpvImageFormatR16 => gpu::Format::R16UNorm,
            spirv_cross_sys::SpvImageFormat__SpvImageFormatR8 => gpu::Format::R8Unorm,
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRgba16Snorm => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRg16Snorm => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRg8Snorm => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatR16Snorm => gpu::Format::R16SNorm,
            spirv_cross_sys::SpvImageFormat__SpvImageFormatR8Snorm => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRgba32i => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRgba16i => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRgba8i => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatR32i => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRg32i => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRg16i => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRg8i => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatR16i => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatR8i => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRgba32ui => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRgba16ui => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRgba8ui => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatR32ui => gpu::Format::R32UInt,
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRgb10a2ui => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRg32ui => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRg16ui => gpu::Format::RG16UInt,
            spirv_cross_sys::SpvImageFormat__SpvImageFormatRg8ui => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatR16ui => gpu::Format::R16UInt,
            spirv_cross_sys::SpvImageFormat__SpvImageFormatR8ui => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatR64ui => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatR64i => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatMax => panic!("Unimplemented format"),
            spirv_cross_sys::SpvImageFormat__SpvImageFormatUnknown => gpu::Format::Unknown,
            _ => panic!("Unrecognized format")
        }

    }

    unsafe fn read_resources(
        compiler: spirv_cross_sys::spvc_compiler,
        spv_resource_type: spirv_cross_sys::spvc_resource_type,
        resource_type: gpu::ResourceType,
        can_be_writable: bool,
        resources: &mut [Vec<gpu::Resource>; gpu::NON_BINDLESS_SET_COUNT as usize],
        uses_bindless_texture_set: &mut bool
    ) {
        let mut spv_resources_ptr: spirv_cross_sys::spvc_resources = std::ptr::null_mut();
        spirv_cross_sys::spvc_compiler_create_shader_resources(
            compiler,
            &mut spv_resources_ptr,
        );

        let spv_resources = {
            let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource =
                std::ptr::null();
            let mut resources_count: usize = 0;
            assert_eq!(
                spirv_cross_sys::spvc_resources_get_resource_list_for_type(
                    spv_resources_ptr,
                    spv_resource_type,
                    &mut resources_list,
                    &mut resources_count
                ),
                spirv_cross_sys::spvc_result_SPVC_SUCCESS
            );
            std::slice::from_raw_parts(resources_list, resources_count as usize)
        };
        for resource in spv_resources {
            let set_index = spirv_cross_sys::spvc_compiler_get_decoration(
                compiler,
                resource.id,
                spirv_cross_sys::SpvDecoration__SpvDecorationDescriptorSet,
            );
            let binding_index = spirv_cross_sys::spvc_compiler_get_decoration(
                compiler,
                resource.id,
                spirv_cross_sys::SpvDecoration__SpvDecorationBinding,
            );
            let name = CStr::from_ptr(spirv_cross_sys::spvc_compiler_get_name(
                compiler,
                resource.id,
            ))
            .to_str()
            .unwrap()
            .to_string();
            if set_index == gpu::BINDLESS_TEXTURE_SET_INDEX {
                *uses_bindless_texture_set = true;
                continue;
            }
            let set = &mut resources[set_index as usize];

            let writable = if can_be_writable {
                spirv_cross_sys::spvc_compiler_get_decoration(
                    compiler,
                    resource.id,
                    spirv_cross_sys::SpvDecoration__SpvDecorationNonWritable,
                ) == 0
            } else {
                false
            };

            let type_handle = spirv_cross_sys::spvc_compiler_get_type_handle(
                compiler,
                resource.type_id,
            );

            let array_size = {
                let array_dimensions =
                    spirv_cross_sys::spvc_type_get_num_array_dimensions(type_handle);
                assert!(array_dimensions == 1 || array_dimensions == 0);
                if array_dimensions != 0 {
                    assert!(
                        spirv_cross_sys::spvc_type_array_dimension_is_literal(
                            type_handle,
                            0
                        ) == 1
                    );
                    spirv_cross_sys::spvc_type_get_array_dimension(type_handle, 0)
                } else {
                    1
                }
            };

            let spv_base_type = spirv_cross_sys::spvc_type_get_basetype(type_handle);
            let is_image = spv_base_type == spirv_cross_sys::spvc_basetype_SPVC_BASETYPE_IMAGE
                || spv_base_type == spirv_cross_sys::spvc_basetype_SPVC_BASETYPE_SAMPLED_IMAGE;

            let mut dim = gpu::TextureDimension::Dim1D;
            let mut multisampled = false;
            let mut sampling_type = gpu::SamplingType::Float;
            let mut storage_format = gpu::Format::Unknown;
            if is_image {
                let spvc_is_array = spirv_cross_sys::spvc_type_get_image_arrayed(type_handle) != 0;
                let _spv_dim = spirv_cross_sys::spvc_type_get_image_dimension(type_handle);
                let is_storage = spirv_cross_sys::spvc_type_get_image_is_storage(type_handle) != 0;
                if is_storage {
                    storage_format = spvc_format_to_format(spirv_cross_sys::spvc_type_get_image_storage_format(type_handle));
                }
                let spv_dim = spirv_cross_sys::spvc_type_get_image_storage_format(type_handle);
                dim = match spv_dim {
                    spirv_cross_sys::SpvDim__SpvDim1D => if !spvc_is_array { gpu::TextureDimension::Dim1D } else { gpu::TextureDimension::Dim1DArray },
                    spirv_cross_sys::SpvDim__SpvDim2D => if !spvc_is_array { gpu::TextureDimension::Dim2D } else { gpu::TextureDimension::Dim2DArray },
                    spirv_cross_sys::SpvDim__SpvDim3D => if !spvc_is_array { gpu::TextureDimension::Dim3D } else { panic!("3D Arrays are not supported") },
                    spirv_cross_sys::SpvDim__SpvDimCube => if !spvc_is_array { gpu::TextureDimension::Cube } else { gpu::TextureDimension::CubeArray },
                    _ => gpu::TextureDimension::Dim1D
                };
                multisampled = spirv_cross_sys::spvc_type_get_image_multisampled(type_handle) != 0;
                let is_depth = spirv_cross_sys::spvc_type_get_image_is_depth(type_handle) != 0;
                if is_depth {
                    sampling_type = gpu::SamplingType::Depth;
                } else {
                    let spv_smapled_type_id = spirv_cross_sys::spvc_type_get_image_sampled_type(type_handle);
                    let sampled_type_handle = spirv_cross_sys::spvc_compiler_get_type_handle(compiler, spv_smapled_type_id);
                    let sampled_base_type = spirv_cross_sys::spvc_type_get_basetype(sampled_type_handle);
                    sampling_type = match sampled_base_type {
                        spirv_cross_sys::spvc_basetype_SPVC_BASETYPE_FP16
                            | spirv_cross_sys::spvc_basetype_SPVC_BASETYPE_FP32
                            | spirv_cross_sys::spvc_basetype_SPVC_BASETYPE_FP64 => gpu::SamplingType::Float,
                        spirv_cross_sys::spvc_basetype_SPVC_BASETYPE_INT16
                            | spirv_cross_sys::spvc_basetype_SPVC_BASETYPE_INT8
                            | spirv_cross_sys::spvc_basetype_SPVC_BASETYPE_INT32
                            | spirv_cross_sys::spvc_basetype_SPVC_BASETYPE_INT64
                            | spirv_cross_sys::spvc_basetype_SPVC_BASETYPE_INT_MAX => gpu::SamplingType::SInt,
                        spirv_cross_sys::spvc_basetype_SPVC_BASETYPE_UINT16
                            | spirv_cross_sys::spvc_basetype_SPVC_BASETYPE_UINT8
                            | spirv_cross_sys::spvc_basetype_SPVC_BASETYPE_UINT32
                            | spirv_cross_sys::spvc_basetype_SPVC_BASETYPE_UINT64 => gpu::SamplingType::UInt,
                        _ => gpu::SamplingType::Float
                    };
                }
            }

            set.push(gpu::Resource {
                name: name,
                set: set_index,
                binding: binding_index,
                array_size,
                writable,
                resource_type,
                texture_dimension: dim,
                is_multisampled: multisampled,
                sampling_type,
                storage_format
            });
        }
    }

    // RESOURCES
    unsafe {
        read_resources(
            compiler,
            spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SEPARATE_IMAGE,
            gpu::ResourceType::SampledTexture,
            false,
            &mut resources,
            &mut uses_bindless_texture_set
        );
        read_resources(
            compiler,
            spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SEPARATE_SAMPLERS,
            gpu::ResourceType::Sampler,
            false,
            &mut resources,
            &mut uses_bindless_texture_set
        );
        read_resources(
            compiler,
            spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SAMPLED_IMAGE,
            gpu::ResourceType::CombinedTextureSampler,
            false,
            &mut resources,
            &mut uses_bindless_texture_set
        );
        read_resources(
            compiler,
            spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SUBPASS_INPUT,
            gpu::ResourceType::SubpassInput,
            false,
            &mut resources,
            &mut uses_bindless_texture_set
        );
        read_resources(
            compiler,
            spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_UNIFORM_BUFFER,
            gpu::ResourceType::UniformBuffer,
            false,
            &mut resources,
            &mut uses_bindless_texture_set
        );
        read_resources(
            compiler,
            spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_STORAGE_BUFFER,
            gpu::ResourceType::StorageBuffer,
            true,
            &mut resources,
            &mut uses_bindless_texture_set
        );
        read_resources(
            compiler,
            spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_STORAGE_IMAGE,
            gpu::ResourceType::StorageTexture,
            true,
            &mut resources,
            &mut uses_bindless_texture_set
        );
        read_resources(
            compiler,
            spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_ACCELERATION_STRUCTURE,
            gpu::ResourceType::AccelerationStructure,
            false,
            &mut resources,
            &mut uses_bindless_texture_set
        );
    }

    // Stage inputs
    unsafe {
        let mut spv_resources_ptr: spirv_cross_sys::spvc_resources = std::ptr::null_mut();
        spirv_cross_sys::spvc_compiler_create_shader_resources(
            compiler,
            &mut spv_resources_ptr,
        );

        let spv_resources = {
            let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource =
                std::ptr::null();
            let mut resources_count: usize = 0;
            assert_eq!(
                spirv_cross_sys::spvc_resources_get_resource_list_for_type(
                    spv_resources_ptr,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_STAGE_INPUT,
                    &mut resources_list,
                    &mut resources_count
                ),
                spirv_cross_sys::spvc_result_SPVC_SUCCESS
            );
            std::slice::from_raw_parts(resources_list, resources_count as usize)
        };
        stage_input_count = spv_resources.len() as u32;
        for input in spv_resources {
            let location = spirv_cross_sys::spvc_compiler_get_decoration(compiler, input.id, spirv_cross_sys::SpvDecoration__SpvDecorationLocation);
            max_stage_input = max_stage_input.max(location);
        }
    }

    unsafe {
        spirv_cross_sys::spvc_context_destroy(context);
    }

    resources
        .iter_mut()
        .for_each(|set| set.sort_by_key(|r| r.binding));

    gpu::PackedShader {
        push_constant_size,
        resources: resources.map(|r| r.into_boxed_slice()),
        shader_type,
        stage_input_count,
        max_stage_input,
        uses_bindless_texture_set,
        shader_spirv: Box::new([]),
        shader_air: Box::new([]),
        shader_dxil: Box::new([]),
        shader_wgsl: String::new()
    }
}

fn compile_shader_spirv_cross(
    spirv: &[u8],
    shader_name: &str,
    shader_type: gpu::ShaderType,
    metadata: &gpu::PackedShader,
    output_shading_language: ShadingLanguage
) -> Result<String, ()> {
    let mut compiled_code_cstr_ptr: *const c_char = std::ptr::null();

    // Generate metadata
    let mut context: spirv_cross_sys::spvc_context = std::ptr::null_mut();
    let mut ir: spirv_cross_sys::spvc_parsed_ir = std::ptr::null_mut();
    let mut compiler: spirv_cross_sys::spvc_compiler = std::ptr::null_mut();

    let mut buffer_count: u32 = 0;
    let mut texture_count: u32 = 0;
    let mut sampler_count: u32 = 0;

    unsafe {
        assert_eq!(
            spirv_cross_sys::spvc_context_create(&mut context),
            spirv_cross_sys::spvc_result_SPVC_SUCCESS
        );

        let do_panic = std::env::var("SHADER_COMPILER_PANIC").map(|s| s == "1").unwrap_or(false);
        let mut info = CallbackInfo {
            shader_name: shader_name.to_string(),
            shading_lang: output_shading_language,
            do_panic
        };
        spirv_cross_sys::spvc_context_set_error_callback(context, Some(spirv_cross_error_callback), &mut info as *mut CallbackInfo as *mut c_void);

        assert_eq!(
            spirv_cross_sys::spvc_context_parse_spirv(
                context,
                spirv.as_ptr() as *const u32,
                spirv.len() / std::mem::size_of::<u32>(),
                &mut ir
            ),
            spirv_cross_sys::spvc_result_SPVC_SUCCESS
        );
        assert_eq!(
            spirv_cross_sys::spvc_context_create_compiler(
                context,
                match output_shading_language {
                    ShadingLanguage::SpirV => panic!("No point invoking compile_shader_spirv_cross if the output is SPIR-V"),
                    ShadingLanguage::Hlsl | ShadingLanguage::Dxil =>
                        spirv_cross_sys::spvc_backend_SPVC_BACKEND_HLSL,
                    ShadingLanguage::Msl | ShadingLanguage::Air => spirv_cross_sys::spvc_backend_SPVC_BACKEND_MSL,
                    ShadingLanguage::Wgsl => panic!("SPIRV-Cross does not support WGSL"),
                    _ => panic!("compile_shader_spirv_cross only supports one output shading language at a time")
                },
                ir,
                spirv_cross_sys::spvc_capture_mode_SPVC_CAPTURE_MODE_COPY,
                &mut compiler
            ),
            spirv_cross_sys::spvc_result_SPVC_SUCCESS
        );

        let mut options: spirv_cross_sys::spvc_compiler_options = std::ptr::null_mut();
        spirv_cross_sys::spvc_compiler_create_compiler_options(compiler, &mut options);
        match output_shading_language {
            ShadingLanguage::SpirV => {},
            ShadingLanguage::Hlsl | ShadingLanguage::Dxil => {
                assert_eq!(
                    spirv_cross_sys::spvc_compiler_options_set_uint(options, spirv_cross_sys::spvc_compiler_option_SPVC_COMPILER_OPTION_HLSL_SHADER_MODEL, 65),
                    spirv_cross_sys::spvc_result_SPVC_SUCCESS
                );
                assert_eq!(
                    spirv_cross_sys::spvc_compiler_options_set_bool(options, spirv_cross_sys::spvc_compiler_option_SPVC_COMPILER_OPTION_HLSL_ENABLE_16BIT_TYPES, 1),
                    spirv_cross_sys::spvc_result_SPVC_SUCCESS
                );
                assert_eq!(
                    spirv_cross_sys::spvc_compiler_options_set_bool(options, spirv_cross_sys::spvc_compiler_option_SPVC_COMPILER_OPTION_HLSL_FORCE_STORAGE_BUFFER_AS_UAV, 1),
                    spirv_cross_sys::spvc_result_SPVC_SUCCESS
                );
            },
            ShadingLanguage::Msl | ShadingLanguage::Air => {
                assert_eq!(
                    spirv_cross_sys::spvc_compiler_options_set_uint(options, spirv_cross_sys::spvc_compiler_option_SPVC_COMPILER_OPTION_MSL_VERSION, make_spirv_cross_msl_version(3, 1, 0)),
                    spirv_cross_sys::spvc_result_SPVC_SUCCESS
                );
                assert_eq!(
                    spirv_cross_sys::spvc_compiler_options_set_bool(options, spirv_cross_sys::spvc_compiler_option_SPVC_COMPILER_OPTION_MSL_ARGUMENT_BUFFERS, 1),
                    spirv_cross_sys::spvc_result_SPVC_SUCCESS
                );
                assert_eq!(
                    spirv_cross_sys::spvc_compiler_options_set_uint(options, spirv_cross_sys::spvc_compiler_option_SPVC_COMPILER_OPTION_MSL_ARGUMENT_BUFFERS_TIER, 2),
                    spirv_cross_sys::spvc_result_SPVC_SUCCESS
                );
                for i in 0..gpu::NON_BINDLESS_SET_COUNT {
                    assert_eq!(
                        spirv_cross_sys::spvc_compiler_msl_add_discrete_descriptor_set(compiler, i),
                        spirv_cross_sys::spvc_result_SPVC_SUCCESS
                    );
                }
                // The bindless argument buffer will get remapped to the first buffer after the stage inputs
                assert_eq!(
                    spirv_cross_sys::spvc_compiler_msl_set_argument_buffer_device_address_space(compiler, metadata.max_stage_input + 1, 1),
                    spirv_cross_sys::spvc_result_SPVC_SUCCESS
                );
            },
            ShadingLanguage::Wgsl => {},
            _ => panic!("compile_shader_spirv_cross only supports one output shading language at a time")
        }
        assert_eq!(
            spirv_cross_sys::spvc_compiler_install_compiler_options(compiler, options),
            spirv_cross_sys::spvc_result_SPVC_SUCCESS
        );

        if output_shading_language == ShadingLanguage::Msl {
            // Metal vertex buffers share buffer binding slots
            if metadata.shader_type == gpu::ShaderType::VertexShader {
                // Vertex buffers need to come first so their indices are consistent across shaders.
                // I don't want to rebind those with each pipeline change while that's the expectation
                // for bound resources anyway.
                // We assume the worst case: every input attribute uses a separate vertex buffer.
                buffer_count += metadata.max_stage_input + 1;
            }

            // Remap argument buffer for bindless texture set to the first buffer after the stage inputs
            if metadata.uses_bindless_texture_set {
                let mut spv_resources_ptr: spirv_cross_sys::spvc_resources = std::ptr::null_mut();
                spirv_cross_sys::spvc_compiler_create_shader_resources(
                    compiler,
                    &mut spv_resources_ptr,
                );
                let spv_resources = {
                    let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource =
                        std::ptr::null();
                    let mut resources_count: usize = 0;
                    assert_eq!(
                        spirv_cross_sys::spvc_resources_get_resource_list_for_type(
                            spv_resources_ptr,
                            spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SEPARATE_IMAGE,
                            &mut resources_list,
                            &mut resources_count
                        ),
                        spirv_cross_sys::spvc_result_SPVC_SUCCESS
                    );
                    std::slice::from_raw_parts(resources_list, resources_count as usize)
                };
                for resource in spv_resources {
                    let set_index = spirv_cross_sys::spvc_compiler_get_decoration(
                        compiler,
                        resource.id,
                        spirv_cross_sys::SpvDecoration__SpvDecorationDescriptorSet,
                    );
                    if set_index == gpu::BINDLESS_TEXTURE_SET_INDEX {
                        spirv_cross_sys::spvc_compiler_set_decoration(compiler, resource.id, spirv_cross_sys::SpvDecoration__SpvDecorationDescriptorSet, buffer_count);
                        buffer_count += 1;
                        break;
                    }
                }
            }

            for set in &metadata.resources {
                for resource in set.iter() {
                    let mut msl_binding = spirv_cross_sys::spvc_msl_resource_binding {
                        stage: match shader_type {
                            gpu::ShaderType::VertexShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelVertex,
                            gpu::ShaderType::FragmentShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelFragment,
                            gpu::ShaderType::GeometryShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelGeometry,
                            gpu::ShaderType::TessellationControlShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelTessellationControl,
                            gpu::ShaderType::TessellationEvaluationShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelTessellationEvaluation,
                            gpu::ShaderType::ComputeShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelGLCompute,
                            gpu::ShaderType::RayGen => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelRayGenerationKHR,
                            gpu::ShaderType::RayMiss => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelMissKHR,
                            gpu::ShaderType::RayClosestHit => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelClosestHitKHR,
                        },
                        desc_set: resource.set,
                        binding: resource.binding,
                        msl_buffer: u32::MAX,
                        msl_texture: u32::MAX,
                        msl_sampler: u32::MAX,
                    };
                    assert_ne!(resource.array_size, 0);
                    assert!(resource.binding < gpu::PER_SET_BINDINGS);
                    assert!(resource.set < gpu::TOTAL_SET_COUNT);
                    match resource.resource_type {
                        gpu::ResourceType::UniformBuffer | gpu::ResourceType::StorageBuffer
                            | gpu::ResourceType::AccelerationStructure => {
                            msl_binding.msl_buffer = buffer_count;
                            buffer_count += resource.array_size;
                        }
                        gpu::ResourceType::SubpassInput | gpu::ResourceType::SampledTexture | gpu::ResourceType::StorageTexture => {
                            msl_binding.msl_texture = texture_count;
                            texture_count += resource.array_size;
                        }
                        gpu::ResourceType::Sampler =>  {
                            msl_binding.msl_sampler = sampler_count;
                            sampler_count += resource.array_size;
                        }
                        gpu::ResourceType::CombinedTextureSampler => {
                            msl_binding.msl_sampler = sampler_count;
                            msl_binding.msl_texture = texture_count;
                            sampler_count += resource.array_size;
                            texture_count += resource.array_size;
                        },
                    }
                    spirv_cross_sys::spvc_compiler_msl_add_resource_binding(compiler, &msl_binding as *const spirv_cross_sys::spvc_msl_resource_binding);
                }
            }

            if metadata.push_constant_size != 0 {
                let msl_binding = spirv_cross_sys::spvc_msl_resource_binding {
                    stage: match shader_type {
                        gpu::ShaderType::VertexShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelVertex,
                        gpu::ShaderType::FragmentShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelFragment,
                        gpu::ShaderType::GeometryShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelGeometry,
                        gpu::ShaderType::TessellationControlShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelTessellationControl,
                        gpu::ShaderType::TessellationEvaluationShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelTessellationEvaluation,
                        gpu::ShaderType::ComputeShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelGLCompute,
                        gpu::ShaderType::RayGen => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelRayGenerationKHR,
                        gpu::ShaderType::RayMiss => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelMissKHR,
                        gpu::ShaderType::RayClosestHit => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelClosestHitKHR,
                    },
                    desc_set: spirv_cross_sys::SPVC_MSL_PUSH_CONSTANT_DESC_SET as u32,
                    binding: spirv_cross_sys::SPVC_MSL_PUSH_CONSTANT_BINDING,
                    msl_buffer: buffer_count,
                    msl_texture: u32::MAX,
                    msl_sampler: u32::MAX,
                };
                spirv_cross_sys::spvc_compiler_msl_add_resource_binding(compiler, &msl_binding as *const spirv_cross_sys::spvc_msl_resource_binding);
            }

            if metadata.uses_bindless_texture_set {
                let msl_binding = spirv_cross_sys::spvc_msl_resource_binding {
                    stage: match shader_type {
                        gpu::ShaderType::VertexShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelVertex,
                        gpu::ShaderType::FragmentShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelFragment,
                        gpu::ShaderType::GeometryShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelGeometry,
                        gpu::ShaderType::TessellationControlShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelTessellationControl,
                        gpu::ShaderType::TessellationEvaluationShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelTessellationEvaluation,
                        gpu::ShaderType::ComputeShader => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelGLCompute,
                        gpu::ShaderType::RayGen => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelRayGenerationKHR,
                        gpu::ShaderType::RayMiss => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelMissKHR,
                        gpu::ShaderType::RayClosestHit => spirv_cross_sys::SpvExecutionModel__SpvExecutionModelClosestHitKHR,
                    },
                    desc_set: gpu::BINDLESS_TEXTURE_SET_INDEX,
                    binding: 0, // the binding sets the [[id(n)]] attribute inside the argument buffer which impacts the offset
                    msl_buffer: u32::MAX,
                    msl_texture: 0,
                    msl_sampler: u32::MAX,
                };
                spirv_cross_sys::spvc_compiler_msl_add_resource_binding(compiler, &msl_binding as *const spirv_cross_sys::spvc_msl_resource_binding);
            }
        }

        let result = spirv_cross_sys::spvc_compiler_compile(
            compiler,
            &mut compiled_code_cstr_ptr as *mut *const c_char
        );
        if result != spirv_cross_sys::spvc_result_SPVC_SUCCESS {
            return Err(());
        }
        let code_cstr = CStr::from_ptr(compiled_code_cstr_ptr);
        let code_string = code_cstr.to_string_lossy();
        Ok(code_string.to_string())
    }
}

enum CompiledShaderType<'a> {
    Packed(&'a gpu::PackedShader),
    Source(&'a String),
    Bytecode(&'a Box<[u8]>)
}

fn write_shader(
    input_shader_path: &Path,
    output_dir: &Path,
    output_shading_language: ShadingLanguage,
    shader: CompiledShaderType
) {
    let mut compiled_file_name = input_shader_path.file_stem().unwrap().to_str().unwrap().to_string();
    match &shader {
        CompiledShaderType::Packed(_) => compiled_file_name.push_str(".json"),
        CompiledShaderType::Bytecode(_) | CompiledShaderType::Source(_) => match output_shading_language {
            ShadingLanguage::SpirV => compiled_file_name.push_str(".spv"),
            ShadingLanguage::Dxil => compiled_file_name.push_str(".dxil"),
            ShadingLanguage::Hlsl => compiled_file_name.push_str(".hlsl"),
            ShadingLanguage::Msl => compiled_file_name.push_str(".metal"),
            ShadingLanguage::Air => compiled_file_name.push_str(".air"),
            ShadingLanguage::Wgsl => compiled_file_name.push_str(".wgsl"),
            _ => panic!("write_shader only supports one output shading language at a time when not writing a packed shader")
        },
    }
    let compiled_file_path = output_dir.join(compiled_file_name);

    match shader {
        CompiledShaderType::Bytecode(bytecode) => {
            let mut file = std::fs::File::create(compiled_file_path).expect("Failed to open file");
            file.write_all(bytecode).expect("Failed to write shader file");
        }
        CompiledShaderType::Source(source) => {
            let mut file = std::fs::File::create(compiled_file_path).expect("Failed to open file");
            write!(file, "{}", source).expect("Failed to write shader file");
        }
        CompiledShaderType::Packed(packed_shader) => {
            let serialized_str = serde_json::to_string(&packed_shader).expect("Failed to serialize");
            let mut file = std::fs::File::create(compiled_file_path).expect("Failed to open file");
            write!(file, "{}", serialized_str).expect("Failed to write shader file");
        }
    }
}

fn compile_msl_to_air(
    msl: String,
    shader_name: &str,
    output_dir: &Path,
    include_debug_info: bool
) -> Result<Box<[u8]>, ()> {
    // xcrun -sdk macosx metal -o Shadow.ir  -c Shadow.metal

    if cfg!(not(any(target_os = "macos", target_os = "ios"))) {
        return Err(());
    }

    let mut temp_file_name = shader_name.to_string();
    temp_file_name.push_str(".temp.metal");

    let temp_metal_path = output_dir.join(temp_file_name);

    let temp_source_file_res = std::fs::File::create(&temp_metal_path);
    if let Err(e) = temp_source_file_res {
        error!("Error creating temporary file for MSL source: {:?} {:?}", &temp_metal_path, e);
        return Err(());
    }
    let mut temp_source_file = temp_source_file_res.unwrap();
    let write_res = write!(temp_source_file, "{}", &msl);
    if let Err(e) = write_res {
        error!("Error writing MSL source to file: {:?}", e);
        return Err(());
    }
    std::mem::drop(temp_source_file);

    let mut output_file_name = shader_name.to_string();
    output_file_name.push_str(".temp.ir");
    let output_path = output_dir.join(output_file_name);

    let mut command = Command::new("xcrun");
    command
        .arg("-sdk")
        .arg("macosx")
        .arg("metal")
        .arg("-o")
        .arg(&output_path)
        .arg("-c")
        .arg(&temp_metal_path);

    if include_debug_info {
        command.arg("-frecord-sources");
    }

    let cmd_result = command.output();

    match &cmd_result {
        Err(e) => {
            error!("Error compiling Metal shader: {}", output_path.to_str().unwrap());
            error!("{}", e.to_string());
            return Err(());
        },
        Ok(output) => {
            if !output.status.success() {
                error!("Error compiling Metal shader: {}", shader_name);
                error!("{}", std::str::from_utf8(&output.stderr).unwrap());
                return Err(());
            }
        }
    }

    if !output_path.exists() {
        error!("Compiled Metal shader file does not exist: {:?}", output_path);
        error!("Output of compile command: {}", String::from_utf8(cmd_result.unwrap().stderr).unwrap());
        return Err(());
    }

    let mut output_library_file_name = shader_name.to_string();
    output_library_file_name.push_str(".temp.metallib");
    let output_library_path = output_dir.join(output_library_file_name);

    let mut command = Command::new("xcrun");
    command
        .arg("-sdk")
        .arg("macosx")
        .arg("metallib")
        .arg("-o")
        .arg(&output_library_path)
        .arg(&output_path);
    let cmd_result = command.output();

    match &cmd_result {
        Err(e) => {
            error!("Error creating Metal library: {:?}", output_path.to_str().unwrap());
            error!("{:?}", e.to_string());
            return Err(());
        },
        Ok(output) => {
            if !output.status.success() {
                error!("Error creating Metal library: {:?}", output_path.to_str().unwrap());
                error!("{:?}", std::str::from_utf8(&output.stderr).unwrap());
                return Err(());
            }
        }
    }

    let air_file_res = File::open(&output_library_path);
    if let Err(e) = air_file_res {
        error!("Failed to open file containing compiled Metal library code: {:?} {:?}", &output_library_path, e);
        return Err(());
    }
    let mut air_file = air_file_res.unwrap();
    let mut air_bytecode = Vec::<u8>::new();
    let read_res = air_file.read_to_end(&mut air_bytecode);
    if let Err(e) = read_res {
        error!("Failed to read file containing compiled Metal library code: {:?}", e);
        return Err(());
    }

    let _ = std::fs::remove_file(temp_metal_path);
    let _ = std::fs::remove_file(output_path);

    Ok(air_bytecode.into_boxed_slice())
}

pub fn compile_shader(
    file_path: &Path,
    output_dir: &Path,
    mut output_shading_languages: ShadingLanguage,
    output_file_type: CompiledShaderFileType,
    include_debug_info: bool,
    arguments: &HashMap<String, String>,
) {
    if cfg!(not(target_os = "macos")) {
        output_shading_languages.remove(ShadingLanguage::Air);
        output_shading_languages.remove(ShadingLanguage::Msl);
    }
    if cfg!(not(target_os = "windows")) {
        output_shading_languages.remove(ShadingLanguage::Dxil);
        output_shading_languages.remove(ShadingLanguage::Hlsl);
    }

    info!(
        "Shader: {:?}, file type: {:?}, shading langs: {:?}",
        file_path, output_file_type, output_shading_languages
    );
    println!("cargo:rerun-if-changed={}", file_path.to_str().unwrap());

    let shader_type = if let Some(path) = file_path.to_str() {
        if path.contains(".rchit") {
            gpu::ShaderType::RayClosestHit
        } else if path.contains(".rgen") {
            gpu::ShaderType::RayGen
        } else if path.contains(".rmiss") {
            gpu::ShaderType::RayMiss
        } else if path.contains(".frag") {
            gpu::ShaderType::FragmentShader
        } else if path.contains(".vert") {
            gpu::ShaderType::VertexShader
        } else {
            gpu::ShaderType::ComputeShader
        }
    } else {
        gpu::ShaderType::ComputeShader
    };

    if shader_type != gpu::ShaderType::VertexShader && shader_type != gpu::ShaderType::FragmentShader && shader_type != gpu::ShaderType::ComputeShader {
        output_shading_languages.remove(ShadingLanguage::Air);
        output_shading_languages.remove(ShadingLanguage::Msl);
        output_shading_languages.remove(ShadingLanguage::Wgsl);
    }

    let shader_name = &file_path.file_stem().unwrap().to_string_lossy();

    // Compile GLSL to SPIR-V
    //
    let spirv_bytecode_res = compile_shader_glsl(file_path, output_dir, shader_type, include_debug_info, arguments);
    if spirv_bytecode_res.is_err() {
        error!("Failed to compile GLSL for {:?}", file_path);
        return;
    }
    let spirv_bytecode = spirv_bytecode_res.unwrap();
    let spirv_bytecode_boxed = spirv_bytecode.into_boxed_slice();

    let mut metadata = read_metadata(&spirv_bytecode_boxed, shader_name, shader_type);

    if output_shading_languages.contains(ShadingLanguage::Msl) {
        if output_file_type == CompiledShaderFileType::Packed {
            panic!("Storing MSL in a packed shader is unsupported.");
        }
        let source = compile_shader_spirv_cross(&spirv_bytecode_boxed, shader_name, shader_type, &metadata, ShadingLanguage::Msl);
        if let Ok(source) = source {
            write_shader(file_path, output_dir, ShadingLanguage::Msl, CompiledShaderType::Source(&source));
        }
    }
    if output_shading_languages.contains(ShadingLanguage::Hlsl) {
        if output_file_type == CompiledShaderFileType::Packed {
            panic!("Storing HLSL in a packed shader is unsupported.");
        }
        let source = compile_shader_spirv_cross(&spirv_bytecode_boxed, shader_name, shader_type, &metadata, ShadingLanguage::Hlsl);
        if let Ok(source) = source {
            write_shader(file_path, output_dir, ShadingLanguage::Hlsl, CompiledShaderType::Source(&source));
        }
    }
    if output_shading_languages.contains(ShadingLanguage::Air) {
        let msl = compile_shader_spirv_cross(&spirv_bytecode_boxed, shader_name, shader_type, &metadata, ShadingLanguage::Msl);
        let bytecode = msl.and_then(|msl| compile_msl_to_air(msl, shader_name, &std::env::temp_dir(), include_debug_info));
        if let Ok(bytecode) = bytecode {
            if output_file_type == CompiledShaderFileType::Bytecode {
                write_shader(file_path, output_dir, ShadingLanguage::Air, CompiledShaderType::Bytecode(&bytecode));
            } else if output_file_type == CompiledShaderFileType::Packed {
                metadata.shader_air = bytecode;
            }
        }
    }
    if output_shading_languages.contains(ShadingLanguage::Dxil) {
        let _hlsl = compile_shader_spirv_cross(&spirv_bytecode_boxed, shader_name, shader_type, &metadata, ShadingLanguage::Hlsl);
        error!("Compiling HLSL to DXIL is unimplemented.");
        let bytecode = Result::<Box<[u8]>, ()>::Err(());
        if let Ok(bytecode) = bytecode {
            if output_file_type == CompiledShaderFileType::Bytecode {
                write_shader(file_path, output_dir, ShadingLanguage::Dxil, CompiledShaderType::Bytecode(&bytecode));
            } else if output_file_type == CompiledShaderFileType::Packed {
                metadata.shader_dxil = bytecode;
            }
        }
    }
    if output_shading_languages.contains(ShadingLanguage::Wgsl) {
        let mut prepared_spirv = spirv_bytecode_boxed.clone().into_vec();
        spirv_remove_debug_info(&mut prepared_spirv);
        spirv_remap_bindings(&mut prepared_spirv, |binding| Binding {
            descriptor_set: binding.descriptor_set,
            binding: if binding.descriptor_set == gpu::BindingFrequency::VeryFrequent as u32 { binding.binding + 1 } else { binding.binding }
        });
        spirv_turn_push_const_into_ubo_pass(&mut prepared_spirv, gpu::BindingFrequency::VeryFrequent as u32, 0);
        spirv_separate_combined_image_samplers(&mut prepared_spirv, Option::<fn(&Binding) -> Binding>::None);

        let wgsl = compile_shader_naga(shader_name, &prepared_spirv);
        if let Ok(bytecode) = wgsl {
            if output_file_type == CompiledShaderFileType::Bytecode {
                write_shader(file_path, output_dir, ShadingLanguage::Wgsl, CompiledShaderType::Source(&bytecode));
            } else if output_file_type == CompiledShaderFileType::Packed {
                metadata.shader_wgsl = bytecode;
            }
        }
    }
    if output_shading_languages.contains(ShadingLanguage::SpirV) {
        if output_file_type == CompiledShaderFileType::Bytecode {
            write_shader(file_path, output_dir, ShadingLanguage::SpirV, CompiledShaderType::Bytecode(&spirv_bytecode_boxed));
        } else if output_file_type == CompiledShaderFileType::Packed {
            metadata.shader_spirv = spirv_bytecode_boxed;
        }
    }

    if output_file_type == CompiledShaderFileType::Packed {
        write_shader(file_path, output_dir, output_shading_languages, CompiledShaderType::Packed(&metadata));
    }
}

fn compile_shader_naga(
    shader_name: &str,
    spirv: &[u8]
) -> Result<String, ()> {
    let module = naga::front::spv::parse_u8_slice(spirv, &Options {
        adjust_coordinate_space: true,
        strict_capabilities: true,
        block_ctx_dump_prefix: None,
    }).map_err(|e| { error!("Error parsing SPIR-V when compiling WGSL: {} - {}", shader_name, e); ()})?;

    let mut validator = Validator::new(ValidationFlags::all(), Capabilities::empty());
    let module_info = validator.validate(&module).map_err(|e| { error!("Error validating module when compiling WGSL: {} - {}", shader_name, e); ()})?;

    let wgsl = naga::back::wgsl::write_string(&module, &module_info, WriterFlags::empty()).map_err(|e| { error!("Error compiling WGSL: {} - {}", shader_name, e); ()})?;
    Ok(wgsl)
}

