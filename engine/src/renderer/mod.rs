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

use std::sync::Arc;
use command::ImguiFrameSnapshot;
use sourcerenderer_core::gpu::Format;
use crate::graphics::{BackendTexture, CommandBuffer, Device, TextureView};
use crate::renderer::asset::{RendererAssets, RendererAssetsReadOnly};
use crate::renderer::renderer_resources::RendererResources;

pub mod asset;
pub(crate) mod passes;
mod vertex;

pub use self::command::RendererCommand;
pub use self::drawable::{DrawablePart, RendererStaticDrawable, VolumeDrawableTransparencyMode};
pub use self::ecs::{
    DirectionalLightComponent, Lightmap, PointLightComponent, StaticRenderableComponent,
    VolumeMeshInstance,
};
pub use self::light::PointLight;
pub use self::renderer::Renderer;
pub use self::renderer_plugin::*;
pub use self::vertex::Vertex;
