use std::process::Command;
use std::{collections::HashMap, path::Path};
use std::{env, error};
use std::path::PathBuf;

use build_util::{compile_shaders, ShadingLanguage};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut meta_shader_dir = manifest_dir.clone();
    meta_shader_dir.push("meta_shaders");

    compile_shaders(
        &meta_shader_dir,
        &meta_shader_dir,
        true,
        &HashMap::new(),
        ShadingLanguage::Air,
        |_| true,
    );

    let mut mdi_shader_path = meta_shader_dir.clone();
    mdi_shader_path.push("mdi.metal");
    let mut compiled_mdi_shader_path = meta_shader_dir.clone();
    compiled_mdi_shader_path.push("mdi.ir");
    compile_msl_shader(&mdi_shader_path, &compiled_mdi_shader_path).unwrap();
}

fn compile_msl_shader(shader_path: &Path, out_path: &Path) -> Result<(), ()> {
    // xcrun -sdk macosx metal -o Shadow.ir  -c Shadow.metal

    println!("cargo:rerun-if-changed={}", shader_path.to_str().unwrap());

    let mut temp_file_name = shader_name.to_string();
    temp_file_name.push_str(".temp.metal");

    let temp_metal_path = output_dir.join(temp_file_name);

    let temp_source_file_res = std::fs::File::create(&temp_metal_path);
    if let Err(e) = temp_source_file_res {
        error!("Error creating temporary file for MSL source: {:?} {:?}", &temp_metal_path, e);
        return Err(());
    }
    let mut temp_source_file = temp_source_file_res.unwrap();
    let write_res = write!(temp_source_file, "{}", &msl);
    if let Err(e) = write_res {
        error!("Error writing MSL source to file: {:?}", e);
        return Err(());
    }

    let mut command = Command::new("xcrun");
    command
        .arg("-sdk")
        .arg("macosx")
        .arg("metal")
        .arg("-o")
        .arg(&out_path)
        .arg("-c")
        .arg(&shader_path);
    let cmd_result = command.output();

    if let Err(e) = &cmd_result {
        eprintln!("Error compiling Metal shader: {:?} {:?}", e, out_path);
        return Err(());
    }

    if !out_path.exists() {
        eprintln!("Compiled Metal shader file does not exist: {:?}", out_path);
        eprintln!("Output of compile command: {}", String::from_utf8(cmd_result.unwrap().stderr).unwrap());
        return Err(());
    }

    let mut command = Command::new("xcrun");
    command
        .arg("-sdk")
        .arg("macosx")
        .arg("metallib")
        .arg("-o")
        .arg(&output_library_path)
        .arg(&output_path);
    let cmd_result = command.output();

    if let Err(e) = cmd_result {
        error!("Error creating Metal library: {:?} {:?}", e, output_dir);
        return Err(());
    }

    let air_file_res = File::open(&output_library_path);
    if let Err(e) = air_file_res {
        error!("Failed to open file containing compiled Metal library code: {:?} {:?}", &output_library_path, e);
        return Err(());
    }

    Ok(())
}
