use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use build_util::{
    compile_shaders,
    ShadingLanguage,
};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut meta_shader_dir = manifest_dir.clone();
    meta_shader_dir.push("meta_shaders");

    compile_shaders(
        &meta_shader_dir,
        &meta_shader_dir,
        true,
        false,
        &HashMap::new(),
        ShadingLanguage::SpirV,
        |_| true,
    );
}
