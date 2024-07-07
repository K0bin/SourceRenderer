use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use build_util::{
    compile_shader,
    compile_shaders,
    copy_directory_rec, CompiledShaderFileType, ShadingLanguage,
};

fn main() {
    println!("Hello, world!");

    simple_logger::SimpleLogger::new().init().unwrap();

    let manifest_dir = PathBuf::from(std::env::current_dir().unwrap());

    // Copy shaders over
    let mut shader_dest_dir = manifest_dir.clone();
    shader_dest_dir.pop();
    shader_dest_dir.push("platform");
    shader_dest_dir.push("sdl");
    shader_dest_dir.push("shaders");

    if !shader_dest_dir.exists() {
        std::fs::create_dir(&shader_dest_dir).expect("Failed to create shader target directory.");
    }

    let mut shader_dir = manifest_dir.clone();
    shader_dir.pop();
    shader_dir.push("engine");
    shader_dir.push("shaders");

    compile_shaders(
        &shader_dir,
        &shader_dest_dir,
        true,
        &HashMap::new(),
        ShadingLanguage::SpirV /*| ShadingLanguage::Dxil*/ | ShadingLanguage::Air,
        |_| true,
    );
}
