use std::env;
use std::path::{PathBuf, Path};
use std::collections::HashMap;
use build_util::*;

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
  let mut android_asset_dir = manifest_dir.clone();
  android_asset_dir.pop();
  android_asset_dir.push("app");
  android_asset_dir.push("app");
  android_asset_dir.push("src");
  android_asset_dir.push("main");
  android_asset_dir.push("assets");
  if !android_asset_dir.exists() {
    std::fs::create_dir(&android_asset_dir).expect("Failed to create shader target directory.");
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
    std::fs::create_dir(&shader_dest_dir).expect("Failed to create shader target directory.");
  }
  compile_shaders(&shader_dir, &shader_dest_dir, false, true, &HashMap::new(), |_| true);

  let mut assets_dir = engine_dir.clone();
  assets_dir.push("assets");
  let mut asset_dest_dir = android_asset_dir.clone();
  asset_dest_dir.push("assets");
  if !asset_dest_dir.exists() {
    std::fs::create_dir(&asset_dest_dir).expect("Failed to create shader target directory.");
  }
  copy_directory_rec(&assets_dir, &asset_dest_dir, &(|_| true));
}
