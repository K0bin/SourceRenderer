#[cfg(feature = "threading")]
mod renderer;

mod drawable;
mod ecs;
mod command;
#[cfg(feature = "threading")]
mod renderer_internal;

mod camera;
pub(crate) mod passes;
mod renderer_assets;

#[cfg(feature = "threading")]
pub use self::renderer::Renderer;

pub use self::ecs::StaticRenderableComponent;
pub use self::drawable::Drawable;
pub use self::drawable::DrawableType;
pub use self::camera::LateLatchCamera;
use self::drawable::View;

#[cfg(feature = "threading")]
use self::renderer_internal::RendererInternal;
