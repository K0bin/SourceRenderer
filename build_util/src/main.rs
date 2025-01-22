use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::{collections::HashMap, fs::File};
use std::env;
use std::path::PathBuf;

use build_util::{
    compile_shader,
    compile_shaders,
    copy_directory_rec, CompiledShaderFileType, ShadingLanguage,
};

mod spirv_transformer;

fn main() {
    // Only used to test it. See the respective build.rs for the actual usage.

    println!("Hello, world!");

    simple_logger::SimpleLogger::new().with_level(log::LevelFilter::Warn).init().unwrap();

    let manifest_dir = PathBuf::from(std::env::current_dir().unwrap());

    // Copy shaders over
    let mut shader_dest_dir = manifest_dir.clone();
    shader_dest_dir.pop();
    shader_dest_dir.push("shaders");

    if !shader_dest_dir.exists() {
        std::fs::create_dir(&shader_dest_dir).expect("Failed to create shader target directory.");
    }

    let mut shader_dir = manifest_dir.clone();
    shader_dir.pop();
    shader_dir.push("engine");
    shader_dir.push("shaders");

    /*compile_shaders(
        &shader_dir,
        &shader_dest_dir,
        true,
        true,
        &HashMap::new(),
        ShadingLanguage::SpirV /*| ShadingLanguage::Dxil*/ | ShadingLanguage::Air | ShadingLanguage::Wgsl,
        |_| true,
    );*/

    let mut buffer = Vec::<u8>::new();
    let mut vert_shader = shader_dir.clone();
    vert_shader.pop();
    vert_shader.pop();
    vert_shader.push("shaders");
    vert_shader.push("web_geometry.web.vert.spv");
    {
        let mut file = File::open(&vert_shader).unwrap();
        buffer.clear();
        file.read_to_end(&mut buffer).unwrap();
    }
    spirv_transformer::spirv_remove_debug_info(&mut buffer);
    spirv_transformer::spirv_push_const_pass(&mut buffer, 0, 0);
    spirv_transformer::spirv_separate_combined_image_samplers(&mut buffer);
    {
        let mut modified = vert_shader.parent().unwrap().to_path_buf();
        modified.push("web_geometry_FIXUP.vert.spv");

        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(&modified)
            .unwrap();
        file.write_all(&buffer[..]).unwrap();
    }

    println!("DOING FS NOW");

    let mut frag_shader = shader_dir.clone();
    frag_shader.pop();
    frag_shader.pop();
    frag_shader.push("shaders");
    frag_shader.push("web_geometry.web.frag.spv");
    {
        let mut file = File::open(&frag_shader).unwrap();
        buffer.clear();
        file.read_to_end(&mut buffer).unwrap();
    }
    spirv_transformer::spirv_remove_debug_info(&mut buffer);
    spirv_transformer::spirv_push_const_pass(&mut buffer, 0, 0);
    spirv_transformer::spirv_separate_combined_image_samplers(&mut buffer);
    {
        let mut modified = frag_shader.parent().unwrap().to_path_buf();
        modified.push("web_geometry_FIXUP.frag.spv");

        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(&modified)
            .unwrap();
        file.write_all(&buffer[..]).unwrap();
    }
}
