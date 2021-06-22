#[cfg(feature = "threading")]
mod renderer;

mod drawable;
mod ecs;
mod command;
#[cfg(feature = "threading")]
mod renderer_internal;
mod renderer_scene;
mod light;

mod camera;
pub(crate) mod passes;
mod renderer_assets;

#[cfg(feature = "threading")]
pub use self::renderer::Renderer;

pub use self::ecs::StaticRenderableComponent;
pub use self::ecs::PointLightComponent;
pub use self::drawable::DrawablePart;
pub use self::camera::LateLatchCamera;
use self::drawable::View;
pub use self::ecs::RendererInterface;
pub use self::command::RendererCommand;
pub use self::light::PointLight;
use self::drawable::RendererStaticDrawable;
use self::renderer_scene::RendererScene;

#[cfg(feature = "threading")]
use self::renderer_internal::RendererInternal;
