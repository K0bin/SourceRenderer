use std::fs::*;
use std::path::*;
use std::process::Command;

pub fn compile_shaders<F>(source_dir: &Path, out_dir: &Path, file_filter: F)
  where F: Fn(&Path) -> bool {
  println!("cargo:rerun-if-changed={}", source_dir.to_str().unwrap());
  let contents = read_dir(&source_dir).expect("Shader directory couldn't be opened.");
  contents
    .filter(|file_result| file_result.is_ok())
    .map(|file_result| file_result.unwrap())
    .filter(|file|
      file.path().extension().and_then(|os_str| os_str.to_str()).unwrap_or("") == "glsl"
      && file_filter(&file.path())
    )
    .for_each(|file| {
      println!("cargo:rerun-if-changed={}", (&file.path()).to_str().unwrap());

      let mut is_rt = false;
      let path = file.path();
      if let Some(path) = path.to_str() {
        is_rt = path.contains(".rchit") || path.contains("rgen") || path.contains(".rmiss");
      }

      let compiled_file_path = Path::join(out_dir, [path.file_stem().unwrap().to_str().unwrap(), ".spv"].concat());
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
