use std::collections::HashMap;
use std::env;
use std::path::{PathBuf, Path};
use build_util::{copy_directory_rec, compile_shaders, compile_shader};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // Copy shaders over
    let mut shader_dest_dir = manifest_dir.clone();
    shader_dest_dir.push("shaders");

    if !shader_dest_dir.exists() {
        std::fs::create_dir(&shader_dest_dir).expect("Failed to create shader target directory.");
    }

    let mut shader_dir = manifest_dir.clone();
    shader_dir.pop();
    shader_dir.pop();
    shader_dir.push("engine");
    shader_dir.push("shaders");

    compile_shaders(&shader_dir, &shader_dest_dir, true, false, &HashMap::new(), |_| true);

    let mut fsr_shader_dir = manifest_dir.clone();
    fsr_shader_dir.pop();
    fsr_shader_dir.pop();
    fsr_shader_dir.push("vendor");
    fsr_shader_dir.push("fsr2");
    fsr_shader_dir.push("FidelityFX-FSR2");
    fsr_shader_dir.push("src");
    fsr_shader_dir.push("ffx-fsr2-api");
    fsr_shader_dir.push("shaders");
    let mut map = HashMap::new();
    map.insert("FFX_GPU".to_string(), "1".to_string());
    map.insert("FFX_GLSL".to_string(), "1".to_string());
    compile_shaders(&fsr_shader_dir, &shader_dest_dir, true, false, &map, |f|
        f.extension().and_then(|ext| ext.to_str()).map(|ext| ext == "glsl").unwrap_or_default()
    );
    let mut accumulate_sharpen_path = fsr_shader_dir.clone();
    accumulate_sharpen_path.push("ffx_fsr2_accumulate_pass.glsl");
    let mut accumulate_sharpen_compiled_path = shader_dest_dir.clone();
    accumulate_sharpen_compiled_path.push("ffx_fsr2_accumulate_sharpen_pass.spv");
    map.insert("FFX_FSR2_OPTION_APPLY_SHARPENING".to_string(), "1".to_string());
    compile_shader(&accumulate_sharpen_path, &accumulate_sharpen_compiled_path, true, &map);

    let mut assets_dest_dir = manifest_dir.clone();
    assets_dest_dir.push("assets");

    if !assets_dest_dir.exists() {
        std::fs::create_dir(&assets_dest_dir).expect("Failed to create shader target directory.");
    }

    let mut assets_dir = manifest_dir.clone();
    assets_dir.pop();
    assets_dir.pop();
    assets_dir.push("engine");
    assets_dir.push("assets");
    copy_directory_rec(&assets_dir, &assets_dest_dir, &(|_| true));

    // Copy SDL2.dll
    let target = env::var("TARGET").unwrap();
    if target.contains("pc-windows") {
        let mut lib_dir = manifest_dir.clone();
        let mut dll_dir = manifest_dir.clone();
        if target.contains("msvc") {
            lib_dir.push("msvc");
            dll_dir.push("msvc");
        }
        else {
            lib_dir.push("gnu-mingw");
            dll_dir.push("gnu-mingw");
        }
        lib_dir.push("lib");
        dll_dir.push("dll");
        println!("cargo:rustc-link-search=all={}", lib_dir.display());
        for entry in std::fs::read_dir(dll_dir).expect("Can't read DLL dir")  {
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
    }
}
