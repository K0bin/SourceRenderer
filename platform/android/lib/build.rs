use std::env;
use std::path::{PathBuf, Path};
use std::collections::HashMap;

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

fn find_ndk() -> Option<PathBuf> {
  if let Some(path) = env::var_os("ANDROID_NDK_HOME") {
    return Some(PathBuf::from(path));
  };

  if let Some(path) = env::var_os("NDK_HOME") {
    return Some(PathBuf::from(path));
  };

  if let Some(sdk_path) = env::var_os("ANDROID_SDK_HOME") {
    let ndk_path = PathBuf::from(&sdk_path).join("ndk");
    let highest_ndk = std::fs::read_dir(ndk_path).ok().and_then(|read_dir| read_dir.filter_map(|it| it.ok()).max_by_key(|it| it.file_name()));
    if let Some(v) = highest_ndk {
      return Some(v.path());
    }
  };

  None
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

  // Copy libc++_shared over
  let ndk_path = find_ndk().expect("Can't find Android NDK, try setting the environment variable ANDROID_NDK_HOME.");
  let target = env::var("TARGET").expect("Can't determine target triple.");

  let mut target_mapping = HashMap::<&'static str, &'static str>::new();
  target_mapping.insert("aarch64-linux-android", "arm64-v8a");
  target_mapping.insert("x86_64-linux-android", "x86_64");

  let mut lib_path = ndk_path.clone();
  lib_path.push("toolchains");
  lib_path.push("llvm");
  lib_path.push("prebuilt");

  #[cfg(target_os = "windows")]
  let mut host_os = "windows".to_string();
  #[cfg(target_os = "linux")]
    let mut host_os = "linux".to_string();
  #[cfg(not(any(target_os = "windows", target_os = "linux")))]
  panic!("Building on this os is currently unsupported.");
  #[cfg(target_arch = "x86_64")]
    {
      host_os += "-x86_64";
    }
  #[cfg(not(target_arch = "x86_64"))]
    panic!("Building on this architecture is currently unsupported.");

  lib_path.push(host_os);
  lib_path.push("sysroot");
  lib_path.push("usr");
  lib_path.push("lib");
  lib_path.push(&target);
  lib_path.push("libc++_shared.so");

  let mut lib_dest_dir = manifest_dir.clone();
  lib_dest_dir.pop();
  lib_dest_dir.push("app");
  lib_dest_dir.push("app");
  lib_dest_dir.push("src");
  lib_dest_dir.push("main");
  lib_dest_dir.push("jniLibs");
  lib_dest_dir.push(target_mapping.get(target.as_str()).expect("Failed to map LLVM target triple to Android jniLibs directory."));
  lib_dest_dir.push("libc++_shared.so");

  std::fs::copy(lib_path, lib_dest_dir).expect("Failed to copy file over.");
}
