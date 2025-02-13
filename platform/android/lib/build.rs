use std::env;
use std::path::{PathBuf, Path};
use std::collections::HashMap;
use build_util::*;

fn main() {
  let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

  // Copy libc++_shared over
  let ndk_path = build_util::android::derive_ndk_path().expect("Can't find Android NDK, try setting the environment variable ANDROID_NDK_HOME.").1;
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

  let mut lib_path = manifest_dir.clone();
  lib_path.pop();
  lib_path.push("app");
  lib_path.push("app");
  lib_path.push("src");
  lib_path.push("main");
  lib_path.push("jniLibs");
  lib_path.push(target_mapping.get(target.as_str()).expect("Failed to map LLVM target triple to Android jniLibs directory."));
  let mut libcpp_dst = lib_path.clone();
  if !lib_path.exists() {
    std::fs::create_dir_all(&lib_path).expect("Failed to create shader target directory.");
  }
  libcpp_dst.push("libc++_shared.so");

  let copy_res = std::fs::copy(&libcpp_src, &libcpp_dst);
  if let Result::Err(err) = copy_res {
    panic!("Failed to copy file over. {:?} to {:?} {:?}", libcpp_src, libcpp_dst, err);
  }

  // Copy shaders over
  let mut android_asset_dir = manifest_dir.clone();
  android_asset_dir.pop();
  android_asset_dir.push("app");
  android_asset_dir.push("app");
  android_asset_dir.push("src");
  android_asset_dir.push("main");
  android_asset_dir.push("assets");
  if !android_asset_dir.exists() {
    std::fs::create_dir_all(&android_asset_dir).expect("Failed to create shader target directory.");
  }

  let mut engine_dir = manifest_dir.clone();
  engine_dir.pop();
  engine_dir.pop();
  engine_dir.pop();
  engine_dir.push("engine");

  let mut shader_dir = engine_dir.clone();
  shader_dir.push("shaders");
  let mut shader_dest_dir = android_asset_dir.clone();
  shader_dest_dir.push("shaders");
  if !shader_dest_dir.exists() {
    std::fs::create_dir_all(&shader_dest_dir).expect("Failed to create shader target directory.");
  }
  compile_shaders(&shader_dir, &shader_dest_dir, false, false, true, &HashMap::new(), |_| true);

  let mut assets_dir = engine_dir.clone();
  assets_dir.push("assets");
  let mut asset_dest_dir = android_asset_dir.clone();
  asset_dest_dir.push("assets");
  if !asset_dest_dir.exists() {
    std::fs::create_dir_all(&asset_dest_dir).expect("Failed to create assets target directory.");
  }
  copy_directory_rec(&assets_dir, &asset_dest_dir, &(|_| true));
}
