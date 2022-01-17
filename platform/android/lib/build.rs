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
        std::fs::create_dir(&to_buf).unwrap_or_else(|_| panic!("{}", format!("Failed to create target directory {:?}.", to_buf)));
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
    std::fs::copy(&entry.path(), &dst_path).unwrap_or_else(|_| panic!("{}", format!("Failed to copy file over: {:?} to {:?}", entry.path(), &dst_path)));
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

  let mut libcpp_src = ndk_path.clone();
  libcpp_src.push("toolchains");
  libcpp_src.push("llvm");
  libcpp_src.push("prebuilt");

  libcpp_src.push(host_os);
  libcpp_src.push("sysroot");
  libcpp_src.push("usr");
  libcpp_src.push("lib");
  libcpp_src.push(&target);
  libcpp_src.push("libc++_shared.so");

  let mut lib_path = manifest_dir;
  lib_path.pop();
  lib_path.push("app");
  lib_path.push("app");
  lib_path.push("src");
  lib_path.push("main");
  lib_path.push("jniLibs");
  lib_path.push(target_mapping.get(target.as_str()).expect("Failed to map LLVM target triple to Android jniLibs directory."));
  let mut libcpp_dst = lib_path.clone();
  if !lib_path.exists() {
    std::fs::create_dir(&lib_path).expect("Failed to create shader target directory.");
  }
  libcpp_dst.push("libc++_shared.so");

  let copy_res = std::fs::copy(&libcpp_src, &libcpp_dst);
  if let Result::Err(err) = copy_res {
    panic!("Failed to copy file over. {:?} to {:?} {:?}", libcpp_src, libcpp_dst, err);
  }

  let profile = env::var("PROFILE").unwrap_or_else(|_| "release".to_string());
  if profile == "debug" {
    // Copy validation layers
    let mut validation_layer_src = ndk_path;
    validation_layer_src.push("sources");
    validation_layer_src.push("third_party");
    validation_layer_src.push("vulkan");
    validation_layer_src.push("src");
    validation_layer_src.push("build-android");
    validation_layer_src.push("jniLibs");
    validation_layer_src.push(target_mapping.get(target.as_str()).expect("Failed to map LLVM target triple to Android jniLibs directory."));
    validation_layer_src.push("libVkLayer_khronos_validation.so");

    let mut validation_layer_dst = lib_path.clone();
    validation_layer_dst.push("libVkLayer_khronos_validation.so");
    std::fs::copy(validation_layer_src, validation_layer_dst).expect("Failed to copy file over.");
  }
}
