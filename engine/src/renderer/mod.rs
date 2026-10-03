macro_rules! shader_path {
    ($shader_name:literal) => {{
        let _ = include_bytes!(concat!(env!("SHADERS_BUILT_DIR"), "/", $shader_name, ".json"));
        if cfg!(target_arch = "wasm32") {
            concat!("shaders_built/", $shader_name, ".json")
        } else {
            concat!(env!("SHADERS_BUILT_DIR"), "/", $shader_name, ".json")
        }
    }};
}

#[proc_macro]
pub fn shader_path(input: proc_macro) -> TokenStream {
    // Extract the string literal from the macro input
    let input_str = input.to_string();
    let filename = input_str.trim_matches('"');

    // Check existence relative to the workspace/compilation directory
    let exists = Path::new(filename).exists();

    // Return a boolean literal as a token stream
    if exists {
        "true".parse().unwrap()
    } else {
        "false".parse().unwrap()
    }
}

use shader_path;

mod renderer;

mod command;
mod drawable;
mod ecs;
mod light;
mod render_path;
mod renderer_culling;
mod renderer_plugin;
mod renderer_resources;
mod renderer_scene;

use crate::graphics::{BackendTexture, CommandBuffer, Device, TextureView};
use crate::renderer::asset::{RendererAssets, RendererAssetsReadOnly};
use crate::renderer::renderer_resources::RendererResources;
use command::ImguiFrameSnapshot;
use sourcerenderer_core::gpu::Format;
use std::sync::Arc;

pub mod asset;
pub(crate) mod passes;
mod vertex;

pub use self::command::RendererCommand;
pub use self::drawable::{DrawablePart, RendererStaticDrawable, VolumeDrawableTransparencyMode};
pub use self::ecs::{
    DirectionalLightComponent, Lightmap, PointLightComponent, StaticRenderableComponent,
    VolumeMeshInstance, VolumeRendererOptions,
};
pub use self::light::PointLight;
pub use self::renderer::Renderer;
pub use self::renderer_plugin::*;
pub use self::vertex::Vertex;
