#[cfg(feature = "threading")]
mod renderer;

mod command;
mod drawable;
mod ecs;
mod light;
mod render_path;
#[cfg(feature = "threading")]
mod renderer_internal;
mod renderer_resources;
mod renderer_scene;
mod renderer_plugin;

mod asset_buffer;
mod late_latch_camera;
mod late_latching;
pub(crate) mod passes;
mod renderer_assets;
mod shader_manager;
mod vertex;

pub use self::command::RendererCommand;
pub use self::drawable::DrawablePart;
use self::drawable::{
    RendererStaticDrawable,
    View,
};
pub use self::ecs::{
    DirectionalLightComponent,
    Lightmap,
    PointLightComponent,
    RendererInterface,
    StaticRenderableComponent,
};
pub use self::late_latch_camera::LateLatchCamera;
pub use self::late_latching::LateLatching;
pub use self::light::PointLight;
#[cfg(feature = "threading")]
pub use self::renderer::Renderer;
#[cfg(feature = "threading")]
use self::renderer_internal::RendererInternal;
pub use self::vertex::Vertex;
pub use self::renderer_plugin::RendererPlugin;
