#[cfg(feature = "generate_bindings")]
extern crate bindgen;
extern crate cc;

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let mut build = cc::Build::new();

    build.include("Vulkan-Headers/include");
    build.include("VulkanMemoryAllocator/include");
    build.file("vma.cpp");

    let target = env::var("TARGET").unwrap();
    if target.contains("darwin") {
        build
            .flag("-std=c++17")
            .flag("-Wno-missing-field-initializers")
            .flag("-Wno-unused-variable")
            .flag("-Wno-unused-parameter")
            .flag("-Wno-unused-private-field")
            .flag("-Wno-reorder")
            .flag("-Wno-nullability-completeness")
            .cpp_link_stdlib("c++")
            .cpp_set_stdlib("c++")
            .cpp(true);
    } else if target.contains("ios") {
        build
            .flag("-std=c++17")
            .flag("-Wno-missing-field-initializers")
            .flag("-Wno-unused-variable")
            .flag("-Wno-unused-parameter")
            .flag("-Wno-unused-private-field")
            .flag("-Wno-reorder")
            .cpp_link_stdlib("c++")
            .cpp_set_stdlib("c++")
            .cpp(true);
    } else if target.contains("android") {
        build
            .flag("-std=c++17")
            .flag("-Wno-missing-field-initializers")
            .flag("-Wno-unused-variable")
            .flag("-Wno-unused-parameter")
            .flag("-Wno-unused-private-field")
            .flag("-Wno-reorder")
            .cpp_link_stdlib("c++")
            .cpp(true);
    } else if target.contains("linux") {
        build
            .flag("-std=c++17")
            .flag("-Wno-missing-field-initializers")
            .flag("-Wno-unused-variable")
            .flag("-Wno-unused-parameter")
            .flag("-Wno-unused-private-field")
            .flag("-Wno-reorder")
            .flag("-Wno-implicit-fallthrough")
            .flag("-Wno-parentheses")
            .cpp_link_stdlib("stdc++")
            .cpp(true);
    } else if target.contains("windows") && target.contains("gnu") {
        build
            .flag("-std=c++17")
            .flag("-Wno-missing-field-initializers")
            .flag("-Wno-unused-variable")
            .flag("-Wno-unused-parameter")
            .flag("-Wno-unused-private-field")
            .flag("-Wno-reorder")
            .flag("-Wno-type-limits")
            .cpp_link_stdlib("stdc++")
            .cpp(true);
    }

    build.compile("vma_cpp");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    generate_bindings(&out_path.join("bindings.rs"));
}

fn generate_bindings<P>(output_file: &P)
    where P: AsRef<Path> {
    let bindings = bindgen::Builder::default()
        .clang_arg("-IVulkan-Headers/include")
        .header("VulkanMemoryAllocator/include/vk_mem_alloc.h")
        .rustfmt_bindings(true)
        .size_t_is_usize(true)
        .blocklist_type("__darwin_.*")
        .allowlist_function("vma.*")
        .allowlist_function("PFN_vma.*")
        .allowlist_type("Vma.*")
        .parse_callbacks(Box::new(FixAshTypes))
        .blocklist_type("Vk.*")
        .blocklist_type("PFN_vk.*")
        .raw_line("use ash::vk::*;")
        .trust_clang_mangling(false)
        .layout_tests(false)
        .generate()
        .expect("Unable to generate bindings!");

    bindings
        .write_to_file(output_file)
        .expect("Unable to write bindings!");
}

#[derive(Debug)]
struct FixAshTypes;

impl bindgen::callbacks::ParseCallbacks for FixAshTypes {
    fn item_name(&self, original_item_name: &str) -> Option<String> {
        if original_item_name.starts_with("Vk") {
            // Strip `Vk` prefix, will use `ash::vk::*` instead
            Some(original_item_name.trim_start_matches("Vk").to_string())
        } else if original_item_name.starts_with("PFN_vk") && original_item_name.ends_with("KHR") {
            // VMA uses a few extensions like `PFN_vkGetBufferMemoryRequirements2KHR`,
            // ash keeps these as `PFN_vkGetBufferMemoryRequirements2`
            Some(original_item_name.trim_end_matches("KHR").to_string())
        } else {
            None
        }
    }

    // When ignoring `Vk` types, bindgen loses derives for some type. Quick workaround.
    fn add_derives(&self, name: &str) -> Vec<String> {
        if name.starts_with("VmaAllocationInfo") || name.starts_with("VmaDefragmentationStats") {
            vec!["Debug".into(), "Copy".into(), "Clone".into()]
        } else {
            vec![]
        }
    }
}
