use std::env;
use std::path::{PathBuf, Path};

fn copy_directory_rec<F>(from: &Path, to: &Path, file_filter: &F)
  where F: Fn(&Path) -> bool {
  for entry in std::fs::read_dir(from).unwrap() {
    println!("cargo:rerun-if-changed={}", from.to_str().unwrap());
    let entry = entry.unwrap();
    if entry.file_type().unwrap().is_dir() {
      let mut from_buf = PathBuf::new();
      from_buf.push(from);
      from_buf.push(entry.file_name());
      let mut to_buf = PathBuf::new();
      to_buf.push(to);
      to_buf.push(entry.file_name());
      if !to_buf.exists() {
        std::fs::create_dir(&to_buf).expect(format!("Failed to create target directory {:?}.", to_buf).as_str());
      }
      copy_directory_rec(&from_buf, &to_buf, file_filter);
      continue;
    }

    if !(file_filter)(&entry.path()) {
      continue;
    }
    let mut dst_path = PathBuf::new();
    dst_path.push(to);
    dst_path.push(entry.file_name());
    println!("cargo:rerun-if-changed={}", entry.path().to_str().unwrap());
    std::fs::copy(&entry.path(), &dst_path).expect(format!("Failed to copy file over: {:?} to {:?}", entry.path(), &dst_path).as_str());
  }
}

fn main() {
  let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

  // Copy shaders over
  let mut shader_dest_dir = manifest_dir.clone();
  shader_dest_dir.pop();
  shader_dest_dir.push("app");
  shader_dest_dir.push("app");
  shader_dest_dir.push("src");
  shader_dest_dir.push("main");
  shader_dest_dir.push("assets");
  if !shader_dest_dir.exists() {
    std::fs::create_dir(&shader_dest_dir).expect("Failed to create shader target directory.");
  }

  shader_dest_dir.push("shaders");
  if !shader_dest_dir.exists() {
    std::fs::create_dir(&shader_dest_dir).expect("Failed to create shader target directory.");
  }

  let mut shader_dir = manifest_dir.clone();
  shader_dir.pop();
  shader_dir.pop();
  shader_dir.pop();
  shader_dir.push("engine");
  shader_dir.push("shaders");

  copy_directory_rec(&shader_dir, &shader_dest_dir, &(|f: &Path| f.extension().map(|ext| ext == "spv").unwrap_or(false)));

  for shader in std::fs::read_dir(shader_dir).unwrap() {
    let shader = shader.unwrap();
    if !shader.path().extension().map(|ext| ext == "spv").unwrap_or(false) {
      continue;
    }
    let mut dst_path = shader_dest_dir.clone();
    dst_path.push(shader.file_name());
    std::fs::copy(shader.path(), dst_path).expect("Failed to copy shader over");
  }
}
