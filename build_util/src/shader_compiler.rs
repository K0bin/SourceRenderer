use std::collections::HashMap;
use std::ffi::{c_char, CStr, c_void};
use std::fs::*;
use std::io::{Read, Write};
use std::path::*;
use std::process::Command;

use spirv_cross_sys;

use sourcerenderer_core::gpu::{self, ShaderSource};

fn make_spirv_cross_msl_version(major: u32, minor: u32, patch: u32) -> u32 {
    major * 10000 + minor * 100 + patch
}

fn msl_remap_binding(set: u32, binding: u32, shader_stage: gpu::ShaderType) -> u32 {
    match shader_stage {
        gpu::ShaderType::VertexShader | gpu::ShaderType::ComputeShader => set * 16 + binding,
        gpu::ShaderType::FragmentShader => (set + 4) * 16 + binding,
        _ => panic!("Unsupported shader stage")
    }
}

pub fn compile_shaders<F>(
    source_dir: &Path,
    out_dir: &Path,
    include_debug_info: bool,
    arguments: &HashMap<String, String>,
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
            compile_shader(
                &file_path,
                out_dir,
                ShadingLanguage::SpirV,
                CompiledShaderFileType::Packed,
                include_debug_info,
                arguments,
            );
            compile_shader(
                &file_path,
                out_dir,
                ShadingLanguage::SpirV,
                CompiledShaderFileType::Bytecode,
                include_debug_info,
                arguments,
            );
            compile_shader(
                &file_path,
                out_dir,
                ShadingLanguage::Msl,
                CompiledShaderFileType::Bytecode,
                include_debug_info,
                arguments,
            );
            compile_shader(
                &file_path,
                out_dir,
                ShadingLanguage::Air,
                CompiledShaderFileType::Bytecode,
                include_debug_info,
                arguments,
            );
            compile_shader(
                &file_path,
                out_dir,
                ShadingLanguage::Hlsl,
                CompiledShaderFileType::Bytecode,
                include_debug_info,
                arguments,
            );
        });
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShadingLanguage {
    SpirV,
    Hlsl,
    Dxil,
    Msl,
    Air,
    Wgsl,
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
) -> Vec<u8> {
    println!("cargo:rerun-if-changed={}", (file_path).to_str().unwrap());

    let is_rt = shader_type == gpu::ShaderType::RayClosestHit
        || shader_type == gpu::ShaderType::RayGen
        || shader_type == gpu::ShaderType::RayMiss;

    let mut command = Command::new("glslangValidator");
    command
        .arg("--target-env")
        .arg(if is_rt { "spirv1.4" } else { "spirv1.3" })
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
            command.arg("-D".to_string() + key + "=" + value);
        } else {
            command.arg("-D".to_string() + key);
        }
    }

    command.arg("-o").arg(&compiled_spv_file_path);
    command.arg(&file_path);

    let output = command.output().unwrap_or_else(|e| {
        panic!(
            "Failed to compile shader: {}\n{}",
            file_path.to_str().unwrap(),
            e.to_string()
        )
    });

    if !output.status.success() {
        panic!(
            "Failed to compile shader: {}\n{:?}\n",
            file_path.to_str().unwrap(),
            output
        );
    }

    let mut spirv_bytecode = Vec::<u8>::new();
    {
        let file_res = std::fs::File::open(&compiled_spv_file_path);
        let mut file = file_res.unwrap();
        file.read_to_end(&mut spirv_bytecode).unwrap();
    }
    let _ = std::fs::remove_file(compiled_spv_file_path);
    spirv_bytecode
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
        println!("SPIR-V-CROSS ERROR in shader: {} {:?}: {:?}", (*info).shader_name, (*info).shading_lang, msg_cstr);
    }
}

fn compile_shader_spirv_cross(
    spirv: Vec<u8>,
    shader_name: &str,
    shader_type: gpu::ShaderType,
    output_shading_language: ShadingLanguage,
    output_file_type: CompiledShaderFileType
) -> Result<gpu::PackedShader, ()> {
        let mut resources: [Vec<gpu::Resource>; 4] = Default::default();
        let mut push_constant_size = 0u32;
        let mut compiled_code_cstr_ptr: *const c_char = std::ptr::null();
        let compiled_shader: gpu::ShaderSource;

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
                        ShadingLanguage::SpirV => spirv_cross_sys::spvc_backend_SPVC_BACKEND_NONE,
                        ShadingLanguage::Hlsl | ShadingLanguage::Dxil =>
                            spirv_cross_sys::spvc_backend_SPVC_BACKEND_HLSL,
                        ShadingLanguage::Msl | ShadingLanguage::Air => spirv_cross_sys::spvc_backend_SPVC_BACKEND_MSL,
                        ShadingLanguage::Wgsl => unimplemented!(),
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
                        spirv_cross_sys::spvc_compiler_options_set_uint(options, spirv_cross_sys::spvc_compiler_option_SPVC_COMPILER_OPTION_MSL_ARGUMENT_BUFFERS_TIER, 2),
                        spirv_cross_sys::spvc_result_SPVC_SUCCESS
                    );
                },
                ShadingLanguage::Wgsl => {},
            }
            assert_eq!(
                spirv_cross_sys::spvc_compiler_install_compiler_options(compiler, options),
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

        if output_file_type == CompiledShaderFileType::Packed {
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

            unsafe fn read_resources(
                compiler: spirv_cross_sys::spvc_compiler,
                shader_type: gpu::ShaderType,
                shader_language: ShadingLanguage,
                spv_resource_type: spirv_cross_sys::spvc_resource_type,
                resource_type: gpu::ResourceType,
                can_be_writable: bool,
                resources: &mut [Vec<gpu::Resource>; 4],
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

                    let array_size = unsafe {
                        let type_handle = spirv_cross_sys::spvc_compiler_get_type_handle(
                            compiler,
                            resource.type_id,
                        );
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

                    if shader_language == ShadingLanguage::Msl {
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
                            desc_set: set_index,
                            binding: binding_index,
                            msl_buffer: u32::MAX,
                            msl_texture: u32::MAX,
                            msl_sampler: u32::MAX,
                        };
                        let binding = msl_remap_binding(set_index, binding_index, shader_type);
                        match resource_type {
                            gpu::ResourceType::UniformBuffer => { msl_binding.msl_buffer = binding; }
                            gpu::ResourceType::StorageBuffer => { msl_binding.msl_buffer = binding; }
                            gpu::ResourceType::SubpassInput => { msl_binding.msl_texture = binding; }
                            gpu::ResourceType::SampledTexture => { msl_binding.msl_texture = binding; }
                            gpu::ResourceType::StorageTexture => { msl_binding.msl_texture = binding; }
                            gpu::ResourceType::Sampler =>  { msl_binding.msl_sampler = binding; }
                            gpu::ResourceType::CombinedTextureSampler =>  { msl_binding.msl_sampler = binding; msl_binding.msl_texture = binding; },
                            gpu::ResourceType::AccelerationStructure => { msl_binding.msl_buffer = binding; }
                        }
                        spirv_cross_sys::spvc_compiler_msl_add_resource_binding(compiler, &msl_binding as *const spirv_cross_sys::spvc_msl_resource_binding);
                    }

                    set.push(gpu::Resource {
                        name: name,
                        set: set_index,
                        binding: binding_index,
                        array_size,
                        writable,
                        resource_type,
                    });
                }
            }

            // RESOURCES
            unsafe {
                read_resources(
                    compiler,
                    shader_type,
                    output_shading_language,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SEPARATE_IMAGE,
                    gpu::ResourceType::SampledTexture,
                    false,
                    &mut resources,
                );
                read_resources(
                    compiler,
                    shader_type,
                    output_shading_language,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SEPARATE_SAMPLERS,
                    gpu::ResourceType::Sampler,
                    false,
                    &mut resources,
                );
                read_resources(
                    compiler,
                    shader_type,
                    output_shading_language,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SAMPLED_IMAGE,
                    gpu::ResourceType::CombinedTextureSampler,
                    false,
                    &mut resources,
                );
                read_resources(
                    compiler,
                    shader_type,
                    output_shading_language,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SUBPASS_INPUT,
                    gpu::ResourceType::SubpassInput,
                    false,
                    &mut resources,
                );
                read_resources(
                    compiler,
                    shader_type,
                    output_shading_language,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_UNIFORM_BUFFER,
                    gpu::ResourceType::UniformBuffer,
                    false,
                    &mut resources,
                );
                read_resources(
                    compiler,
                    shader_type,
                    output_shading_language,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_STORAGE_BUFFER,
                    gpu::ResourceType::StorageBuffer,
                    true,
                    &mut resources,
                );
                read_resources(
                    compiler,
                    shader_type,
                    output_shading_language,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_STORAGE_IMAGE,
                    gpu::ResourceType::StorageTexture,
                    true,
                    &mut resources,
                );
                read_resources(
                    compiler,
                    shader_type,
                    output_shading_language,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_ACCELERATION_STRUCTURE,
                    gpu::ResourceType::AccelerationStructure,
                    false,
                    &mut resources,
                );
            }

            unsafe {
                if output_shading_language != ShadingLanguage::SpirV {
                    assert_eq!(
                        spirv_cross_sys::spvc_compiler_compile(
                            compiler,
                            &mut compiled_code_cstr_ptr as *mut *const c_char
                        ),
                        spirv_cross_sys::spvc_result_SPVC_SUCCESS
                    );
                    let code_cstr = CStr::from_ptr(compiled_code_cstr_ptr);
                    let code_string = code_cstr.to_string_lossy();
                    compiled_shader = gpu::ShaderSource::Source(code_string.to_string());

                    // TODO: Compile HLSL to DXIL
                } else {
                    compiled_shader =
                        gpu::ShaderSource::Bytecode(spirv.into_boxed_slice());
                }
                spirv_cross_sys::spvc_context_destroy(context);
            }
        } else {
            unsafe {
                let result = spirv_cross_sys::spvc_compiler_compile(
                    compiler,
                    &mut compiled_code_cstr_ptr as *mut *const c_char
                );
                if result != spirv_cross_sys::spvc_result_SPVC_SUCCESS {
                    return Err(());
                }
                let code_cstr = CStr::from_ptr(compiled_code_cstr_ptr);
                let code_string = code_cstr.to_string_lossy();
                compiled_shader = gpu::ShaderSource::Source(code_string.to_string());

                // TODO: Compile HLSL to DXIL
            }
        }

        Ok(gpu::PackedShader {
            resources: resources.map(|r| r.into_boxed_slice()),
            push_constant_size,
            shader_type,
            shader: compiled_shader
        })
}

fn write_shader(
    input_shader_path: &Path,
    output_dir: &Path,
    output_shading_language: ShadingLanguage,
    output_file_type: CompiledShaderFileType,
    packed_shader: gpu::PackedShader
) {
    let mut compiled_file_name = input_shader_path.file_stem().unwrap().to_str().unwrap().to_string();
    match output_file_type {
        CompiledShaderFileType::Packed => compiled_file_name.push_str(".json"),
        CompiledShaderFileType::Bytecode => match output_shading_language {
            ShadingLanguage::SpirV => compiled_file_name.push_str(".spv"),
            ShadingLanguage::Dxil => compiled_file_name.push_str(".dxil"),
            ShadingLanguage::Hlsl => compiled_file_name.push_str(".hlsl"),
            ShadingLanguage::Msl => compiled_file_name.push_str(".metal"),
            ShadingLanguage::Air => compiled_file_name.push_str(".air"),
            ShadingLanguage::Wgsl => compiled_file_name.push_str(".wgsl"),
        },
    }
    let compiled_file_path = output_dir.join(compiled_file_name);

    match output_file_type {
        CompiledShaderFileType::Bytecode => {
            let mut file = std::fs::File::create(compiled_file_path).expect("Failed to open file");
            let gpu::PackedShader { push_constant_size : _, resources : _, shader_type : _, shader : compiled_shader  } = packed_shader;
            match compiled_shader {
                gpu::ShaderSource::Bytecode(bytecode) => {
                    file.write_all(&bytecode).expect("Failed to write shader file");
                }
                gpu::ShaderSource::Source(code) => {
                    write!(file, "{}", code).expect("Failed to write shader file");
                }
            }
        }
        CompiledShaderFileType::Packed => {
            let serialized_str = serde_json::to_string(&packed_shader).expect("Failed to serialize");
            let mut file = std::fs::File::create(compiled_file_path).expect("Failed to open file");
            write!(file, "{}", serialized_str).expect("Failed to write shader file");
        }
    }
}

fn compile_msl_to_air(
    mut packed_shader: gpu::PackedShader,
    shader_name: &str,
    output_dir: &Path
) -> Result<gpu::PackedShader, ()> {
    // xcrun -sdk macosx metal -o Shadow.ir  -c Shadow.metal

    let mut temp_file_name = shader_name.to_string();
    temp_file_name.push_str(".temp.metal");

    let temp_metal_path = output_dir.join(temp_file_name);

    let temp_source_file_res = std::fs::File::create(&temp_metal_path);
    if let Err(e) = temp_source_file_res {
        println!("Error creating temporary file for MSL source: {:?} {:?}", &temp_metal_path, e);
        return Err(());
    }
    let mut temp_source_file = temp_source_file_res.unwrap();
    let source = match &packed_shader.shader {
        ShaderSource::Source(code) => code,
        _ => unreachable!()
    };
    let write_res = write!(temp_source_file, "{}", source);
    if let Err(e) = write_res {
        println!("Error writing MSL source to file: {:?}", e);
        return Err(());
    }
    std::mem::drop(temp_source_file);

    let mut output_file_name = shader_name.to_string();
    output_file_name.push_str(".temp.air");
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
    let cmd_result = command.output();

    if let Err(e) = cmd_result {
        println!("Error compiling Metal shader: {:?}", e);
        return Err(());
    }

    let air_file_res = File::open(&output_path);
    if let Err(e) = air_file_res {
        println!("Failed to open file containing compiled AIR code: {:?} {:?}", &output_path, e);
        return Err(());
    }
    let mut air_file = air_file_res.unwrap();
    let mut air_bytecode = Vec::<u8>::new();
    let read_res = air_file.read_to_end(&mut air_bytecode);
    if let Err(e) = read_res {
        println!("Failed to read file containing compiled AIR code: {:?}", e);
        return Err(());
    }

    let _ = std::fs::remove_file(temp_metal_path);
    let _ = std::fs::remove_file(output_path);

    packed_shader.shader = ShaderSource::Bytecode(air_bytecode.into_boxed_slice());
    Ok(packed_shader)

}

pub fn compile_shader(
    file_path: &Path,
    output_dir: &Path,
    output_shading_language: ShadingLanguage,
    output_file_type: CompiledShaderFileType,
    include_debug_info: bool,
    arguments: &HashMap<String, String>,
) {
    println!(
        "Shader: {:?}, file type: {:?}, shading lang: {:?}",
        file_path, output_file_type, output_shading_language
    );

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
    let shader_name = &file_path.file_stem().unwrap().to_string_lossy();

    // Compile GLSL to SPIR-V
    //
    let spirv_bytecode = compile_shader_glsl(file_path, output_dir, shader_type, include_debug_info, arguments);

    // Compile SPIR-V to shading language source if necessary and/or generate metadata
    //
    let mut packed_shader = if output_file_type != CompiledShaderFileType::Bytecode
    || output_shading_language != ShadingLanguage::SpirV {
        let res = compile_shader_spirv_cross(spirv_bytecode, shader_name, shader_type, output_shading_language, output_file_type);
        if res.is_err() {
            return;
        }
        res.unwrap()
    } else {
        gpu::PackedShader {
            push_constant_size: 0,
            resources: Default::default(),
            shader_type,
            shader: gpu::ShaderSource::Bytecode(spirv_bytecode.into_boxed_slice()),
        }
    };

    // Compile to API bytecode
    //
    if output_shading_language == ShadingLanguage::Air {
        let air_compile_res = compile_msl_to_air(packed_shader, shader_name, output_dir);
        if air_compile_res.is_err() {
            return;
        }
        packed_shader = air_compile_res.unwrap();
    } else if output_shading_language == ShadingLanguage::Dxil {
        unimplemented!()
    }

    // Write finished shader to disk
    //
    write_shader(file_path, output_dir, output_shading_language, output_file_type, packed_shader);
}

pub fn compile_meta_shader() {
    unimplemented!()
}
