use std::env;
use std::fs::*;
use std::path::*;
use std::process::Command;

fn main() {
  let pkg_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
  let shader_dir = Path::join(Path::new(&pkg_dir), Path::new("shaders"));

  println!("cargo:rerun-if-changed={}", shader_dir.as_path().to_str().unwrap());
  let contents = read_dir(&shader_dir).expect("Shader directory couldn't be opened.");
  contents
    .filter(|file_result| file_result.is_ok())
    .map(|file_result| file_result.unwrap())
    .filter(|file| file.path().extension().and_then(|os_str| os_str.to_str()).unwrap_or("") == "glsl")
    .for_each(|file| {
      println!("cargo:rerun-if-changed={}", file.path().as_path().to_str().unwrap());

      let mut is_rt = false;
      let path = file.path();
      if let Some(path) = path.to_str() {
        is_rt = path.contains(".rchit") || path.contains("rgen") || path.contains(".rmiss");
      }

      let compiled_file_path = Path::join(Path::new(&shader_dir), [path.file_stem().unwrap().to_str().unwrap(), ".spv"].concat());
      let output = Command::new("glslangValidator")
      .arg("--target-env")
      .arg(if is_rt { "spirv1.4" } else { "spirv1.3" })
      .arg("-V")
      .arg("-o")
      .arg(compiled_file_path)
      .arg(path.clone())
      .output()
      .unwrap_or_else(|e| panic!("Failed to compile shader: {}\n{}", path.to_str().unwrap(), e.to_string()));

      if !output.status.success() {
        panic!("Failed to compile shader: {}\n{:?}\n", path.to_str().unwrap(), output);
      }
    }
  );
}