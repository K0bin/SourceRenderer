#[cfg(feature = "threading")]
mod renderer;

mod drawable;
mod ecs;
mod command;
#[cfg(feature = "threading")]
mod renderer_internal;
mod renderer_scene;
mod light;
mod render_path;

mod late_latch_camera;
pub(crate) mod passes;
mod renderer_assets;
mod late_latching;

#[cfg(feature = "threading")]
pub use self::renderer::Renderer;

pub use self::ecs::StaticRenderableComponent;
pub use self::ecs::PointLightComponent;
pub use self::ecs::DirectionalLightComponent;
pub use self::drawable::DrawablePart;
pub use self::late_latch_camera::LateLatchCamera;
use self::drawable::View;
pub use self::ecs::{RendererInterface, Lightmap};
pub use self::command::RendererCommand;
pub use self::light::PointLight;
use self::drawable::RendererStaticDrawable;
use self::renderer_scene::RendererScene;
pub use self::late_latching::LateLatching;

#[cfg(feature = "threading")]
use self::renderer_internal::RendererInternal;
