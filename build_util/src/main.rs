use std::collections::HashMap;
use std::path::PathBuf;

use build_util::{compile_shaders, ShadingLanguage};

fn main() {
    // Only used to test it. See the respective build.rs for the actual usage.

    println!("Hello, world!");

    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Warn)
        .init()
        .unwrap();

    let manifest_dir = PathBuf::from(std::env::current_dir().unwrap());

    // Copy shaders over
    let mut shader_dest_dir = manifest_dir.clone();
    shader_dest_dir.pop();
    shader_dest_dir.push("shaders");

    if !shader_dest_dir.exists() {
        std::fs::create_dir_all(&shader_dest_dir)
            .expect("Failed to create shader target directory.");
    }

    let mut shader_dir = manifest_dir.clone();
    shader_dir.pop();
    shader_dir.push("engine");
    shader_dir.push("shaders");

    compile_shaders(
        &shader_dir,
        &shader_dest_dir,
        true,
        true,
        &HashMap::new(),
        ShadingLanguage::SpirV /*| ShadingLanguage::Dxil*/ | ShadingLanguage::Air | ShadingLanguage::Wgsl,
        |_| true,
    );
}
