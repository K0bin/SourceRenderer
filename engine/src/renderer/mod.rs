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

pub mod asset;
pub(crate) mod passes;
mod vertex;

pub use self::command::RendererCommand;
pub use self::drawable::DrawablePart;
use self::drawable::RendererStaticDrawable;
pub use self::ecs::{
    DirectionalLightComponent,
    Lightmap,
    PointLightComponent,
    StaticRenderableComponent,
};
pub use self::light::PointLight;
pub use self::renderer::Renderer;
pub use self::renderer_plugin::RendererPlugin;
pub use self::vertex::Vertex;
