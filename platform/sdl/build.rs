use build_util::{ShadingLanguage, compile_shaders, get_shader_path, copy_directory_rec};
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

fn main() {
    build_util::build_script_logger::init();
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

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
