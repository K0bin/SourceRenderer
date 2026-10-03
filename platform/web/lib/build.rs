use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use build_util::{compile_shaders, copy_directory_rec, ShadingLanguage, symlink_dir, create_dir_if_necessary, get_shading_languages_for_target};

fn main() {
    build_util::build_script_logger::init_with_filter(|record| {
        let msg = format!("{}", record.args());
        !msg.contains("Unknown decoration Block") // bullshit warning by Naga
    });

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let _out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let mut web_static_dir = manifest_dir.clone();
    web_static_dir.pop();
    web_static_dir.push("www");
    web_static_dir.push("public");
    web_static_dir.push("enginedata");
    create_dir_if_necessary(&web_static_dir).unwrap();

    // Copy shaders over
    let mut shader_dir = manifest_dir.clone();
    shader_dir.pop();
    shader_dir.pop();
    shader_dir.pop();
    shader_dir.push("shaders_built");
    shader_dir.push("wasm32-unknown-unknown");
    create_dir_if_necessary(&shader_dir).unwrap();

    let mut shader_dest_dir = web_static_dir.clone();
    shader_dest_dir.push("shaders_built");
    if let Err(e) = symlink_dir(&shader_dir, &shader_dest_dir) {
        let mut compile_fallback = true;
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            if let Ok(target) = std::fs::read_link(&shader_dest_dir) {
                compile_fallback = false;
                if target != shader_dir {
                    log::warn!("Found different symlink for shaders. Expected {:?}, Actual: {:?}, compiling again.", &shader_dir, &target);
                }
            }
        }
        if compile_fallback {
            log::warn!("Creating symlink for shaders to {:?} failed: {:?}, compiling again.", &shader_dest_dir, &e);

            let mut shader_source_dir = manifest_dir.clone();
            shader_source_dir.pop();
            shader_source_dir.pop();
            shader_source_dir.pop();
            shader_source_dir.push("engine");
            shader_source_dir.push("shaders");

            create_dir_if_necessary(&shader_dest_dir).unwrap();

            compile_shaders(
                &shader_source_dir,
                &shader_dest_dir,
                true,
                false,
                &HashMap::new(),
                get_shading_languages_for_target(false),
                |_| true,
            );
        }
    }

    // Copy assets over
    let mut assets_dir = manifest_dir.clone();
    assets_dir.pop();
    assets_dir.pop();
    assets_dir.pop();
    assets_dir.push("assets");

    let mut assets_dest_dir = web_static_dir.clone();
    assets_dest_dir.push("assets");
    if let Err(e) = symlink_dir(&assets_dir, &assets_dest_dir) {
        let mut copy_fallback = true;
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            if let Ok(target) = std::fs::read_link(&assets_dest_dir) {
                copy_fallback = false;
                if target != assets_dir {
                    log::warn!("Found different symlink for shaders. Expected {:?}, Actual: {:?}, compiling again.", &assets_dir, &target);
                }
            }
        }

        if copy_fallback {
            log::warn!("Creating symlink for assets to {:?} failed: {:?}, falling back to copying.", &assets_dest_dir, &e);
            create_dir_if_necessary(&assets_dest_dir).unwrap();
            copy_directory_rec(&assets_dir, &assets_dest_dir, &(|_| true));
        }
    }

    log::logger().flush();
}
