use build_util;

fn main() {
    // Prevent building SPIRV-Cross on wasm32 target
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH");
    if let Ok(arch) = target_arch.as_ref() {
        if "wasm32" == arch {
            return;
        }
    }

    let target_vendor = std::env::var("CARGO_CFG_TARGET_VENDOR");
    let is_apple = target_vendor.is_ok() && target_vendor.as_ref().unwrap() == "apple";

    let target_os = std::env::var("CARGO_CFG_TARGET_OS");
    let is_ios = target_os.is_ok() && target_os.as_ref().unwrap() == "ios";
    let is_android = target_os.is_ok() && target_os.as_ref().unwrap() == "android";

    let mut build = cc::Build::new();
    build.cpp(true);

    let compiler = build.try_get_compiler();
    let is_clang: bool;
    if let Ok(compiler) = compiler {
        is_clang = compiler.is_like_clang();
    } else {
        is_clang = false;
    }

    if is_apple && (is_clang || is_ios) {
        build.flag("-std=c++17").cpp_set_stdlib("c++");
    } else {
        build.flag_if_supported("-std=c++17");
    }

    build.flag_if_supported("-Wno-missing-field-initializers")
         .flag_if_supported("-Wno-unused-function")
         .flag_if_supported("-Wno-unused-variable")
         .flag_if_supported("-Wno-unused-parameter")
         .flag_if_supported("-Wno-unknown-pragmas")
         .flag_if_supported("-Wno-sign-compare")
         .flag_if_supported("-Wno-missing-field-initializers");

    build
        .include("FidelityFX-FSR2/src/ffx-fsr2-api/")
        .flag("-DFFX_GCC")
    	.file("FidelityFX-FSR2/src/ffx-fsr2-api/ffx_assert.cpp")
        .file("FidelityFX-FSR2/src/ffx-fsr2-api/ffx_fsr2.cpp");

    build.compile("fsr2");

    let mut bindings_builder = bindgen::Builder::default()
    	.header("FidelityFX-FSR2/src/ffx-fsr2-api/ffx_fsr2.h")
        .allowlist_file("FidelityFX-FSR2/src/ffx-fsr2-api/ffx_fsr2.h")
        .allowlist_file("FidelityFX-FSR2/src/ffx-fsr2-api/ffx_util.h")
        .allowlist_file("FidelityFX-FSR2/src/ffx-fsr2-api/ffx_types.h")
        .allowlist_file("FidelityFX-FSR2/src/ffx-fsr2-api/ffx_error.h")
        .allowlist_file("FidelityFX-FSR2/src/ffx-fsr2-api/ffx_fsr2_interface.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .clang_arg("-x")
        .clang_arg("c++")
        .clang_arg("-Wno-missing-field-initializers")
        .clang_arg("-Wno-unused-function")
        .clang_arg("-Wno-unused-variable")
        .clang_arg("-Wno-unused-parameter")
        .clang_arg("-Wno-sign-compare")
        .clang_arg("-Wno-unknown-pragmas")
        .clang_arg("-Wno-missing-field-initializers")
        .clang_arg("-std=c++17")
        .clang_arg("-stdlib=libc++")
        .clang_arg("-fdeclspec")
        .detect_include_paths(true);

    if is_android {
        let target_triple = target_arch.unwrap() + "-linux-android";
        let sysroot = build_util::android::sysroot().unwrap();
        bindings_builder = bindings_builder.clang_arg("-I".to_string() +
        sysroot.join("usr").join("include")
            .to_str().unwrap());
        bindings_builder = bindings_builder.clang_arg("-I".to_string() +
        sysroot.join("usr").join("include").join(target_triple)
            .to_str().unwrap());
    }

    let bindings = bindings_builder
        .generate()
        .expect("Unable to generate bindings");

    let out_path = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
