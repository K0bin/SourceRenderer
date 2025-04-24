use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use build_util::{
    compile_shaders,
    copy_directory_rec,
    ShadingLanguage,
};

fn main() {
    build_util::build_script_logger::init();
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Copy shaders over
    let mut shader_dest_dir = out_dir.clone();
    for _ in 0..5 {
        shader_dest_dir.pop();
    }
    shader_dest_dir.push("shaders");

    if !shader_dest_dir.exists() {
        std::fs::create_dir_all(&shader_dest_dir)
            .expect("Failed to create shader target directory.");
    }

    let mut output_shading_languages =
        ShadingLanguage::SpirV | ShadingLanguage::Dxil | ShadingLanguage::Air;
    if env::var("DUMP_SHADERS")
        .map(|envvar| envvar == "true" || envvar == "True" || envvar == "1")
        .unwrap_or_default()
    {
        output_shading_languages |= ShadingLanguage::Msl | ShadingLanguage::Hlsl;
    }

    // Unimplemented and the warning is annoying.
    output_shading_languages &= !ShadingLanguage::Dxil;

    let mut shader_dir = manifest_dir.clone();
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

    let mut assets_dest_dir = manifest_dir.clone();
    assets_dest_dir.push("assets");

    if !assets_dest_dir.exists() {
        std::fs::create_dir_all(&assets_dest_dir)
            .expect("Failed to create shader target directory.");
    }

    let mut assets_dir = manifest_dir.clone();
    assets_dir.pop();
    assets_dir.pop();
    assets_dir.push("engine");
    assets_dir.push("assets");
    copy_directory_rec(&assets_dir, &assets_dest_dir, &(|_| true));

    // Copy SDL2.dll/SDL3.dll
    let target = env::var("TARGET").unwrap();
    if target.contains("pc-windows") {
        let mut lib_dir = manifest_dir.clone();
        let mut dll_dir = manifest_dir.clone();
        if target.contains("msvc") {
            lib_dir.push("msvc");
            dll_dir.push("msvc");
        } else {
            lib_dir.push("gnu-mingw");
            dll_dir.push("gnu-mingw");
        }
        lib_dir.push("lib");
        dll_dir.push("dll");
        println!("cargo:rustc-link-search=all={}", lib_dir.display());
        for entry in std::fs::read_dir(dll_dir).expect("Can't read DLL dir") {
            let entry_path = entry.expect("Invalid fs entry").path();
            let file_name_result = entry_path.file_name();
            let mut new_file_path = manifest_dir.clone();
            if let Some(file_name) = file_name_result {
                let file_name = file_name.to_str().unwrap();
                if file_name.ends_with(".dll") {
                    new_file_path.push(file_name);
                    std::fs::copy(&entry_path, &new_file_path).expect("Can't copy from DLL dir");
                }
            }
        }
    } else {
        pkg_config::Config::new().probe("sdl3").unwrap();
    }

    log::logger().flush();
}
