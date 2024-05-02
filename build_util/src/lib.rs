use std::path::{Path, PathBuf};

mod shader_compiler;
pub mod android;
pub use shader_compiler::*;
pub mod build_script_logger;

pub fn copy_directory_rec<F>(from: &Path, to: &Path, file_filter: &F)
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
        std::fs::create_dir(&to_buf).unwrap_or_else(|_| panic!("Failed to create target directory {:?}.", to_buf));
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
    std::fs::copy(&entry.path(), &dst_path).unwrap_or_else(|_| panic!("Failed to copy file over: {:?} to {:?}", entry.path(), &dst_path));
  }
}
