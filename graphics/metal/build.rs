use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::{collections::HashMap, path::Path};

use build_util::{compile_shaders, ShadingLanguage};

use log::logger;

fn main() {
    build_util::build_script_logger::init();

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut meta_shader_dir = manifest_dir.clone();
    meta_shader_dir.push("meta_shaders");

    let mut output_shading_languages = ShadingLanguage::Air;
    if env::var("DUMP_SHADERS")
        .map(|envvar| envvar == "true" || envvar == "True" || envvar == "1")
        .unwrap_or_default()
    {
        output_shading_languages |= ShadingLanguage::Msl;
    }

    compile_shaders(
        &meta_shader_dir,
        &meta_shader_dir,
        true,
        false,
        &HashMap::new(),
        output_shading_languages,
        |_| true,
    );

    let mut mdi_shader_path = meta_shader_dir.clone();
    mdi_shader_path.push("mdi.metal");
    let mut compiled_mdi_shader_path = meta_shader_dir.clone();
    compiled_mdi_shader_path.push("mdi.metallib");
    let _ = compile_msl_shader(&mdi_shader_path, &compiled_mdi_shader_path);
}

fn compile_msl_shader(shader_path: &Path, out_path: &Path) -> Result<(), ()> {
    // xcrun -sdk macosx metal -o Shadow.ir  -c Shadow.metal

    if cfg!(not(any(target_os = "macos", target_os = "ios"))) {
        return Err(());
    }

    println!("cargo:rerun-if-changed={}", shader_path.to_str().unwrap());

    let mut temp_file_name = out_path.file_stem().unwrap().to_string_lossy().to_string();
    temp_file_name.push_str(".temp.ir");

    let temp_ir_path = out_path
        .parent()
        .unwrap_or_else(|| &Path::new(""))
        .join(temp_file_name);

    let mut command = Command::new("xcrun");
    command
        .arg("-sdk")
        .arg("macosx")
        .arg("metal")
        .arg("-o")
        .arg(&temp_ir_path)
        .arg("-c")
        .arg(&shader_path);
    let cmd_result = command.output();

    match &cmd_result {
        Err(e) => {
            panic!(
                "Error compiling Metal shader: {}: {}",
                temp_ir_path.to_str().unwrap(),
                e.to_string()
            );
        }
        Ok(output) => {
            if !output.status.success() {
                panic!(
                    "Error compiling Metal shader: {}: {}",
                    temp_ir_path.to_str().unwrap(),
                    std::str::from_utf8(&output.stderr).unwrap()
                );
            }
        }
    }

    if !temp_ir_path.exists() {
        panic!(
            "Compiled Metal shader file does not exist: {:?}",
            temp_ir_path
        );
    }

    let mut command = Command::new("xcrun");
    command
        .arg("-sdk")
        .arg("macosx")
        .arg("metallib")
        .arg("-o")
        .arg(&out_path)
        .arg(&temp_ir_path);
    let cmd_result = command.output();

    match &cmd_result {
        Err(e) => {
            panic!(
                "Error creating Metal library: {}: {}",
                temp_ir_path.to_str().unwrap(),
                e.to_string()
            );
        }
        Ok(output) => {
            if !output.status.success() {
                panic!(
                    "Error creating Metal library: {}: {}",
                    temp_ir_path.to_str().unwrap(),
                    std::str::from_utf8(&output.stderr).unwrap()
                );
            }
        }
    }

    if !out_path.exists() {
        panic!("Compiled Metal shader file does not exist: {:?}", out_path);
    }

    let _ = std::fs::remove_file(temp_ir_path);

    logger().flush();

    Ok(())
}
