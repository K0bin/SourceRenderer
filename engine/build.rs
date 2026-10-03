use std::path::PathBuf;
use build_util::{setup_shader_path_env, compile_shaders, get_shading_languages_for_target, create_dir_if_necessary};
use std::collections::HashMap;

fn main() {
    build_util::build_script_logger::init_with_filter(|record| {
        let msg = format!("{}", record.args());
        !msg.contains("Unknown decoration Block") // bullshit warning by Naga
    });

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    // Compile shaders
    let mut shader_dest_dir = manifest_dir.clone();
    assert!(shader_dest_dir.pop());
    // Setup env var for code
    shader_dest_dir = setup_shader_path_env(&shader_dest_dir.as_path());
    create_dir_if_necessary(&shader_dest_dir).unwrap();

    let mut shader_dir = manifest_dir.clone();
    shader_dir.push("shaders");

    compile_shaders(
        &shader_dir,
        &shader_dest_dir,
        true,
        false,
        &HashMap::new(),
        get_shading_languages_for_target(false),
        |_| true,
    );
}
