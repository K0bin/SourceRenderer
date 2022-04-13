use std::env;
use std::fs::*;
use std::io::Write;
use std::path::*;
use std::io::Read;
use spirv_cross::spirv::*;
use build_util::*;

fn main() {
  let pkg_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
  let shader_dir = Path::new(&pkg_dir).join("..").join("..").join("..").join("engine").join("shaders");

  let shader_dir_temp = Path::new(&pkg_dir).join("shaders_temp");
  if !shader_dir_temp.exists() {
    std::fs::create_dir(&shader_dir_temp).expect("Failed to create shader temp directory.");
  }

  compile_shaders(&shader_dir, &shader_dir_temp, |f| f.extension().and_then(|os_str| os_str.to_str()).unwrap_or("") == "glsl" && f.file_stem().and_then(|ext| ext.to_str()).map(|s| s.contains(".web.")).unwrap_or(false));

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
      let module = Module::from_words(words);
      let mut ast = Ast::<spirv_cross::glsl::Target>::parse(&module).unwrap_or_else(|_e| panic!("Failed to parse shader: {:?}", file.path()));
      let mut options = spirv_cross::glsl::CompilerOptions::default();
      options.version = spirv_cross::glsl::Version::V3_00Es;
      options.emit_push_constant_as_uniform_buffer = true;
      ast.set_compiler_options(&options).unwrap_or_else(|_e| panic!("Failed to set compiler options for shader: {:?}", file.path()));
      let resources = ast.get_shader_resources().expect("Failed to get shader resources");
      let input_prefix = if is_ps {
        "io"
      } else {
        "vs_input"
      };
      for input in &resources.stage_inputs {
        let location = ast.get_decoration(input.id, Decoration::Location).unwrap();
        ast.rename_interface_variable(&resources.stage_inputs, location, &format!("{}_{}", input_prefix, location)).unwrap();
        ast.unset_decoration(input.id, Decoration::Location).unwrap();
      }
      let output_prefix = if is_ps {
        "ps_output"
      } else {
        "io"
      };
      for output in &resources.stage_outputs {
        let location = ast.get_decoration(output.id, Decoration::Location).unwrap();
        ast.rename_interface_variable(&resources.stage_outputs, location, &format!("{}_{}", output_prefix, location)).unwrap();
        ast.unset_decoration(output.id, Decoration::Location).unwrap();
      }
      for uniform in &resources.uniform_buffers {
        let set = ast.get_decoration(uniform.id, Decoration::DescriptorSet).unwrap();
        let location = ast.get_decoration(uniform.id, Decoration::Location).unwrap();
        ast.set_name(uniform.id, &format!("res_{}_{}", set, location)).unwrap();
        ast.set_name(uniform.base_type_id, &format!("res_{}_{}_t", set, location)).unwrap();
        ast.unset_decoration(uniform.id, Decoration::Location).unwrap();
        ast.unset_decoration(uniform.id, Decoration::DescriptorSet).unwrap();
      }
      for texture in &resources.sampled_images {
        let set = ast.get_decoration(texture.id, Decoration::DescriptorSet).unwrap();
        let location = ast.get_decoration(texture.id, Decoration::Location).unwrap();
        ast.set_name(texture.id, &format!("res_{}_{}", set, location)).unwrap();
        ast.unset_decoration(texture.id, Decoration::Location).unwrap();
        ast.unset_decoration(texture.id, Decoration::DescriptorSet).unwrap();
      }

      if let Some(push_constants) = resources.push_constant_buffers.first() {
        ast.set_name(push_constants.id, "push_constants").unwrap();
        ast.set_name(push_constants.base_type_id, "push_constants_t").unwrap();
      }

      let code_res = ast.compile();
      if code_res.is_err() {
        return;
      }
      let code = code_res.unwrap();
      let compiled_file_path = compiled_file_folder.join([file.path().file_stem().unwrap().to_str().unwrap(), ".glsl"].concat());
      let mut out_file = File::create(compiled_file_path).unwrap();
      write!(out_file, "{}", code).unwrap();
    }
  );
}
