use std::env;
use std::ffi::{CStr, CString};
use std::fs::*;
use std::io::Write;
use std::os::raw::{c_void, c_char};
use std::path::*;
use std::io::Read;
use build_util::*;
use spirv_cross_sys;

fn main() {
  let pkg_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
  let shader_dir = Path::new(&pkg_dir).join("..").join("..").join("..").join("engine").join("shaders");

  let shader_dir_temp = Path::new(&pkg_dir).join("shaders_temp");
  if !shader_dir_temp.exists() {
    std::fs::create_dir(&shader_dir_temp).expect("Failed to create shader temp directory.");
  }

  compile_shaders(&shader_dir, &shader_dir_temp, true, |f| f.extension().and_then(|os_str| os_str.to_str()).unwrap_or("") == "glsl" && f.file_stem().and_then(|ext| ext.to_str()).map(|s| s.contains(".web.")).unwrap_or(false));

  let compiled_file_folder = Path::new(&pkg_dir).join("..").join("www").join("dist").join("shaders");
  if !compiled_file_folder.exists() {
    std::fs::create_dir_all(&compiled_file_folder).expect("Failed to create output shader directory");
  }

  println!("cargo:rerun-if-changed={}", (&shader_dir_temp).to_str().unwrap());
  let contents = read_dir(&shader_dir_temp).expect("Shader directory couldn't be opened.");
  contents
    .filter(|file_result| file_result.is_ok())
    .map(|file_result| file_result.unwrap())
    .filter(|f| f.path().extension().and_then(|os_str| os_str.to_str()).unwrap_or("") == "spv" && f.path().file_stem().and_then(|ext| ext.to_str()).map(|s| s.contains(".web.")).unwrap_or(false))
    .for_each(|file| {
      println!("cargo:rerun-if-changed={}", (&file.path()).to_str().unwrap());

      let is_ps = file.path().file_stem().and_then(|ext| ext.to_str()).map(|s| s.ends_with("frag")).unwrap_or(false);

      let mut buffer = Vec::<u8>::new();
      let mut file_reader = File::open(file.path()).unwrap();
      file_reader.read_to_end(&mut buffer).unwrap();
      assert_eq!(buffer.len() % std::mem::size_of::<u32>(), 0);
      let words_len = buffer.len() / std::mem::size_of::<u32>();
      let words = unsafe { std::slice::from_raw_parts(buffer.as_ptr() as *const u32, words_len) };

      let mut context: spirv_cross_sys::spvc_context = std::ptr::null_mut();
      let mut ir: spirv_cross_sys::spvc_parsed_ir = std::ptr::null_mut();
      let mut compiler: spirv_cross_sys::spvc_compiler = std::ptr::null_mut();
      let mut resources: spirv_cross_sys::spvc_resources = std::ptr::null_mut();
      let mut options: spirv_cross_sys::spvc_compiler_options = std::ptr::null_mut();
      unsafe {
        assert_eq!(spirv_cross_sys::spvc_context_create(&mut context), spirv_cross_sys::spvc_result_SPVC_SUCCESS);
        spirv_cross_sys::spvc_context_set_error_callback(context, Some(spvc_callback), std::ptr::null_mut());
        assert_eq!(spirv_cross_sys::spvc_context_parse_spirv(context, words.as_ptr() as *const u32, words_len as u64, &mut ir), spirv_cross_sys::spvc_result_SPVC_SUCCESS);
        assert_eq!(spirv_cross_sys::spvc_context_create_compiler(context, spirv_cross_sys::spvc_backend_SPVC_BACKEND_GLSL, ir, spirv_cross_sys::spvc_capture_mode_SPVC_CAPTURE_MODE_COPY, &mut compiler), spirv_cross_sys::spvc_result_SPVC_SUCCESS);

        assert_eq!(spirv_cross_sys::spvc_compiler_create_shader_resources(compiler, &mut resources), spirv_cross_sys::spvc_result_SPVC_SUCCESS);

        assert_eq!(spirv_cross_sys::spvc_compiler_create_compiler_options(compiler, &mut options), spirv_cross_sys::spvc_result_SPVC_SUCCESS);
        assert_eq!(spirv_cross_sys::spvc_compiler_options_set_uint(options, spirv_cross_sys::spvc_compiler_option_SPVC_COMPILER_OPTION_GLSL_VERSION, 300), spirv_cross_sys::spvc_result_SPVC_SUCCESS);
        assert_eq!(spirv_cross_sys::spvc_compiler_options_set_bool(options, spirv_cross_sys::spvc_compiler_option_SPVC_COMPILER_OPTION_GLSL_ES, 1), spirv_cross_sys::spvc_result_SPVC_SUCCESS);
        assert_eq!(spirv_cross_sys::spvc_compiler_options_set_bool(options, spirv_cross_sys::spvc_compiler_option_SPVC_COMPILER_OPTION_GLSL_EMIT_PUSH_CONSTANT_AS_UNIFORM_BUFFER, 1), spirv_cross_sys::spvc_result_SPVC_SUCCESS);
        assert_eq!(spirv_cross_sys::spvc_compiler_options_set_bool(options, spirv_cross_sys::spvc_compiler_option_SPVC_COMPILER_OPTION_FIXUP_DEPTH_CONVENTION, 1), spirv_cross_sys::spvc_result_SPVC_SUCCESS);
        assert_eq!(spirv_cross_sys::spvc_compiler_install_compiler_options(compiler, options), spirv_cross_sys::spvc_result_SPVC_SUCCESS);
      }


      let input_prefix = if is_ps {
        "io"
      } else {
        "vs_input"
      };
      let stage_inputs = unsafe {
        let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource = std::ptr::null();
        let mut resources_count: u64 = 0;
        assert_eq!(spirv_cross_sys::spvc_resources_get_resource_list_for_type(resources, spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_STAGE_INPUT, &mut resources_list, &mut resources_count), spirv_cross_sys::spvc_result_SPVC_SUCCESS);
        std::slice::from_raw_parts(resources_list, resources_count as usize)
      };
      for resource in stage_inputs {
        let location = unsafe {
          spirv_cross_sys::spvc_compiler_get_decoration(compiler, resource.id, spirv_cross_sys::SpvDecoration__SpvDecorationLocation)
        };
        unsafe {
          let new_name = format!("{}_{}", input_prefix, location);
          let c_name = CString::new(new_name.as_str()).unwrap();
          spirv_cross_sys::spvc_compiler_set_name(compiler, resource.id, c_name.as_ptr());
          spirv_cross_sys::spvc_compiler_unset_decoration(compiler, resource.id, spirv_cross_sys::SpvDecoration__SpvDecorationLocation);
        }
      }


      let output_prefix = if is_ps {
        "ps_output"
      } else {
        "io"
      };
      let stage_outputs = unsafe {
        let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource = std::ptr::null();
        let mut resources_count: u64 = 0;
        assert_eq!(spirv_cross_sys::spvc_resources_get_resource_list_for_type(resources, spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_STAGE_OUTPUT, &mut resources_list, &mut resources_count), spirv_cross_sys::spvc_result_SPVC_SUCCESS);
        std::slice::from_raw_parts(resources_list, resources_count as usize)
      };
      for resource in stage_outputs {
        let location = unsafe {
          spirv_cross_sys::spvc_compiler_get_decoration(compiler, resource.id, spirv_cross_sys::SpvDecoration__SpvDecorationLocation)
        };
        unsafe {
          let new_name = format!("{}_{}", output_prefix, location);
          let c_name = CString::new(new_name.as_str()).unwrap();
          spirv_cross_sys::spvc_compiler_set_name(compiler, resource.id, c_name.as_ptr());
          spirv_cross_sys::spvc_compiler_unset_decoration(compiler, resource.id, spirv_cross_sys::SpvDecoration__SpvDecorationLocation);
        }
      }


      let uniform_buffers = unsafe {
        let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource = std::ptr::null();
        let mut resources_count: u64 = 0;
        assert_eq!(spirv_cross_sys::spvc_resources_get_resource_list_for_type(resources, spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_UNIFORM_BUFFER, &mut resources_list, &mut resources_count), spirv_cross_sys::spvc_result_SPVC_SUCCESS);
        std::slice::from_raw_parts(resources_list, resources_count as usize)
      };
      for resource in uniform_buffers {
        let set = unsafe {
          spirv_cross_sys::spvc_compiler_get_decoration(compiler, resource.id, spirv_cross_sys::SpvDecoration__SpvDecorationDescriptorSet)
        };
        let binding = unsafe {
          spirv_cross_sys::spvc_compiler_get_decoration(compiler, resource.id, spirv_cross_sys::SpvDecoration__SpvDecorationBinding)
        };
        let new_name = format!("res_{}_{}", set, binding);
        let c_name = CString::new(new_name.as_str()).unwrap();
        let new_type_name = format!("res_{}_{}_t", set, binding);
        let c_type_name = CString::new(new_type_name.as_str()).unwrap();
        unsafe {
          spirv_cross_sys::spvc_compiler_set_name(compiler, resource.id, c_name.as_ptr());
          spirv_cross_sys::spvc_compiler_set_name(compiler, resource.base_type_id, c_type_name.as_ptr());
          spirv_cross_sys::spvc_compiler_unset_decoration(compiler, resource.id, spirv_cross_sys::SpvDecoration__SpvDecorationDescriptorSet);
          spirv_cross_sys::spvc_compiler_unset_decoration(compiler, resource.id, spirv_cross_sys::SpvDecoration__SpvDecorationBinding);
        }
      }


      let sampled_images = unsafe {
        let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource = std::ptr::null();
        let mut resources_count: u64 = 0;
        assert_eq!(spirv_cross_sys::spvc_resources_get_resource_list_for_type(resources, spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_SAMPLED_IMAGE, &mut resources_list, &mut resources_count), spirv_cross_sys::spvc_result_SPVC_SUCCESS);
        std::slice::from_raw_parts(resources_list, resources_count as usize)
      };
      for resource in sampled_images {
        let set = unsafe {
          spirv_cross_sys::spvc_compiler_get_decoration(compiler, resource.id, spirv_cross_sys::SpvDecoration__SpvDecorationDescriptorSet)
        };
        let binding = unsafe {
          spirv_cross_sys::spvc_compiler_get_decoration(compiler, resource.id, spirv_cross_sys::SpvDecoration__SpvDecorationBinding)
        };
        let new_name = format!("res_{}_{}", set, binding);
        let c_name = CString::new(new_name.as_str()).unwrap();
        unsafe {
          spirv_cross_sys::spvc_compiler_set_name(compiler, resource.id, c_name.as_ptr());
          spirv_cross_sys::spvc_compiler_unset_decoration(compiler, resource.id, spirv_cross_sys::SpvDecoration__SpvDecorationDescriptorSet);
          spirv_cross_sys::spvc_compiler_unset_decoration(compiler, resource.id, spirv_cross_sys::SpvDecoration__SpvDecorationBinding);
        }
      }


      let push_constants = unsafe {
        let mut resources_list: *const spirv_cross_sys::spvc_reflected_resource = std::ptr::null();
        let mut resources_count: u64 = 0;
        assert_eq!(spirv_cross_sys::spvc_resources_get_resource_list_for_type(resources, spirv_cross_sys::spvc_resource_type_SPVC_RESOURCE_TYPE_PUSH_CONSTANT, &mut resources_list, &mut resources_count), spirv_cross_sys::spvc_result_SPVC_SUCCESS);
        std::slice::from_raw_parts(resources_list, resources_count as usize)
      };
      if let Some(push_constants) = push_constants.first() {
        unsafe {
          let push_constants_name = CString::new("push_constants").unwrap();
          let push_constants_type_name = CString::new("push_constants_t").unwrap();
          spirv_cross_sys::spvc_compiler_set_name(compiler, push_constants.id, push_constants_name.as_ptr());
          spirv_cross_sys::spvc_compiler_set_name(compiler, push_constants.base_type_id, push_constants_type_name.as_ptr());
        }
      }


      let mut code: *const std::os::raw::c_char = std::ptr::null();
      unsafe {
        let result = spirv_cross_sys::spvc_compiler_compile(compiler, &mut code);
        if result != spirv_cross_sys::spvc_result_SPVC_SUCCESS {
          spirv_cross_sys::spvc_context_destroy(context);
          return;
        }
      }
      let compiled_file_path = compiled_file_folder.join([file.path().file_stem().unwrap().to_str().unwrap(), ".glsl"].concat());
      let mut out_file = File::create(compiled_file_path).unwrap();
      unsafe {
        write!(out_file, "{}", CStr::from_ptr(code).to_str().unwrap()).unwrap();
      }

      unsafe {
        spirv_cross_sys::spvc_context_destroy(context);
      }
    }
  );
}

unsafe extern "C" fn spvc_callback(user_data: *mut c_void, error: *const c_char) {
  panic!("SPIR-V-Cross Error: {}", CStr::from_ptr(error).to_str().unwrap());
}
