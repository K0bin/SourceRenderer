fn main() {
    // Prevent building SPIRV-Cross on wasm32 target
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH");
    if let Ok(arch) = target_arch {
        if "wasm32" == arch {
            return;
        }
    }

    let target_vendor = std::env::var("CARGO_CFG_TARGET_VENDOR");
    let is_apple = target_vendor.is_ok() && target_vendor.unwrap() == "apple";

    let target_os = std::env::var("CARGO_CFG_TARGET_OS");
    let is_ios = target_os.is_ok() && target_os.unwrap() == "ios";

    let mut build = cc::Build::new();
    build.cpp(true);

    let compiler = build.try_get_compiler();
    let is_clang: bool;
    let is_msvc: bool;
    if let Ok(compiler) = compiler {
        is_clang = compiler.is_like_clang();
        is_msvc = compiler.is_like_msvc();
    } else {
        is_clang = false;
        is_msvc = false;
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
         .flag_if_supported("-Wno-sign-compare");

    build
        .include("FidelityFX-FSR2/src/ffx-fsr2-api/")
        .flag("-DFFX_GCC")
    	.file("FidelityFX-FSR2/src/ffx-fsr2-api/ffx_assert.cpp")
        .file("FidelityFX-FSR2/src/ffx-fsr2-api/ffx_fsr2.cpp");

    if !is_msvc {
        build.flag("-Wno-unknown-pragmas");
    }

    build.compile("fsr2");

    let bindings = bindgen::Builder::default()
    	.header("FidelityFX-FSR2/src/ffx-fsr2-api/ffx_fsr2.h")
        .allowlist_file("FidelityFX-FSR2/src/ffx-fsr2-api/ffx_fsr2.h")
        .allowlist_file("FidelityFX-FSR2/src/ffx-fsr2-api/ffx_util.h")
        .allowlist_file("FidelityFX-FSR2/src/ffx-fsr2-api/ffx_types.h")
        .allowlist_file("FidelityFX-FSR2/src/ffx-fsr2-api/ffx_error.h")
        .allowlist_file("FidelityFX-FSR2/src/ffx-fsr2-api/ffx_fsr2_interface.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .clang_arg("-x")
        .clang_arg("c++")
        .clang_arg("-fdeclspec")
        .generate()
        .expect("Unable to generate bindings");

    let out_path = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
