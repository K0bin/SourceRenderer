use std::collections::HashMap;
use std::ffi::{c_char, CStr};
use std::fs::*;
use std::io::{Read, Write};
use std::path::*;
use std::process::Command;

use spirv_cross_sys;

use sourcerenderer_core::gpu;

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
        });
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShadingLanguage {
    SpirV,
    Hlsl,
    Dxil,
    Msl,
    Wgsl,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompiledShaderFileType {
    Bytecode,
    Packed,
}

pub fn compile_shader(
    file_path: &Path,
    output_dir: &Path,
    output_shading_language: ShadingLanguage,
    output_file_type: CompiledShaderFileType,
    include_debug_info: bool,
    arguments: &HashMap<String, String>,
) {
    println!("cargo:rerun-if-changed={}", (file_path).to_str().unwrap());

    println!(
        "Shader: {:?}, file type: {:?}, shading lang: {:?}",
        file_path, output_file_type, output_shading_language
    );

    let compiled_shader: gpu::ShaderSource;

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

    let mut resources: [Vec<gpu::Resource>; 4] = Default::default();
    let mut push_constant_size = 0u32;
    let mut compiled_code_cstr_ptr: *const c_char = std::ptr::null();

    if output_file_type != CompiledShaderFileType::Bytecode
        || output_shading_language != ShadingLanguage::SpirV
    {
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
            assert_eq!(
                spirv_cross_sys::spvc_context_parse_spirv(
                    context,
                    spirv_bytecode.as_ptr() as *const u32,
                    spirv_bytecode.len() / std::mem::size_of::<u32>(),
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
                        ShadingLanguage::Msl => spirv_cross_sys::spvc_backend_SPVC_BACKEND_MSL,
                        ShadingLanguage::Wgsl => unimplemented!(),
                    },
                    ir,
                    spirv_cross_sys::spvc_capture_mode_SPVC_CAPTURE_MODE_COPY,
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
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SEPARATE_IMAGE,
                    gpu::ResourceType::SampledTexture,
                    false,
                    &mut resources,
                );
                read_resources(
                    compiler,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SEPARATE_SAMPLERS,
                    gpu::ResourceType::Sampler,
                    false,
                    &mut resources,
                );
                read_resources(
                    compiler,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SAMPLED_IMAGE,
                    gpu::ResourceType::CombinedTextureSampler,
                    false,
                    &mut resources,
                );
                read_resources(
                    compiler,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SUBPASS_INPUT,
                    gpu::ResourceType::SubpassInput,
                    false,
                    &mut resources,
                );
                read_resources(
                    compiler,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_UNIFORM_BUFFER,
                    gpu::ResourceType::UniformBuffer,
                    false,
                    &mut resources,
                );
                read_resources(
                    compiler,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_STORAGE_BUFFER,
                    gpu::ResourceType::StorageBuffer,
                    true,
                    &mut resources,
                );
                read_resources(
                    compiler,
                    spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_STORAGE_IMAGE,
                    gpu::ResourceType::StorageTexture,
                    true,
                    &mut resources,
                );
                read_resources(
                    compiler,
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
                        gpu::ShaderSource::Bytecode(spirv_bytecode.into_boxed_slice());
                }
                spirv_cross_sys::spvc_context_destroy(context);
            }
        } else {
            unsafe {
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
            }
        }
    } else {
        compiled_shader = gpu::ShaderSource::Bytecode(spirv_bytecode.into_boxed_slice());
    }

    let mut compiled_file_name = file_path.file_stem().unwrap().to_str().unwrap().to_string();
    match output_file_type {
        CompiledShaderFileType::Packed => compiled_file_name.push_str(".json"),
        CompiledShaderFileType::Bytecode => match output_shading_language {
            ShadingLanguage::SpirV => compiled_file_name.push_str(".spv"),
            ShadingLanguage::Dxil => compiled_file_name.push_str(".dxil"),
            ShadingLanguage::Hlsl => compiled_file_name.push_str(".hlsl"),
            ShadingLanguage::Msl => compiled_file_name.push_str(".msl"),
            ShadingLanguage::Wgsl => compiled_file_name.push_str(".wgsl"),
        },
    }
    let compiled_file_path = output_dir.join(compiled_file_name);

    match output_file_type {
        CompiledShaderFileType::Bytecode => {
            let mut file = std::fs::File::create(compiled_file_path).expect("Failed to open file");
            match compiled_shader {
                gpu::ShaderSource::Bytecode(bytecode) => {
                    file.write_all(&bytecode);
                }
                gpu::ShaderSource::Source(code) => {
                    write!(file, "{}", code).expect("Failed to write shader file");
                }
            }
        }
        CompiledShaderFileType::Packed => {
            let resources: [Box<[gpu::Resource]>; 4] = resources.map(|r| r.into_boxed_slice());
            let packed = gpu::PackedShader {
                push_constant_size,
                resources,
                shader_type,
                shader: compiled_shader,
            };
            let serialized_str = serde_json::to_string(&packed).expect("Failed to serialize");
            let mut file = std::fs::File::create(compiled_file_path).expect("Failed to open file");
            write!(file, "{}", serialized_str).expect("Failed to write shader file");
        }
    }
}

pub fn compile_meta_shader() {
    unimplemented!()
}
