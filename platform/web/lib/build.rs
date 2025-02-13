use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use build_util::{
    compile_shader,
    compile_shaders,
    copy_directory_rec, CompiledShaderFileType, ShadingLanguage,
};

fn main() {
    build_util::build_script_logger::init();
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let _out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let mut web_static_dir = manifest_dir.clone();
    web_static_dir.pop();
    web_static_dir.push("www");
    web_static_dir.push("public");
    web_static_dir.push("enginedata");

    // Copy shaders over
    let mut shader_dest_dir = web_static_dir.clone();
    shader_dest_dir.push("shaders");

    if !shader_dest_dir.exists() {
        std::fs::create_dir_all(&shader_dest_dir).expect("Failed to create shader target directory.");
    }

    let output_shading_languages = ShadingLanguage::Wgsl;

    let mut shader_dir = manifest_dir.clone();
    shader_dir.pop();
    shader_dir.pop();
    shader_dir.pop();
    shader_dir.push("engine");
    shader_dir.push("shaders");

    compile_shaders(
        &shader_dir,
        &shader_dest_dir,
        true,
        false,
        &HashMap::new(),
        output_shading_languages,
        |_| true,
    );

    let mut assets_dest_dir = web_static_dir.clone();
    assets_dest_dir.push("assets");

    if !assets_dest_dir.exists() {
        std::fs::create_dir_all(&assets_dest_dir).expect("Failed to create shader target directory.");
    }

    let mut assets_dir = manifest_dir.clone();
    assets_dir.pop();
    assets_dir.pop();
    assets_dir.pop();
    assets_dir.push("engine");
    assets_dir.push("assets");
    copy_directory_rec(&assets_dir, &assets_dest_dir, &(|_| true));

    log::logger().flush();
}
