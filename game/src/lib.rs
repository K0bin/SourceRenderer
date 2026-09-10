mod fps_camera;
mod plugin;
mod spinning_cube;

pub use plugin::GamePlugin;
mod uni_project;
pub use uni_project::UniProjectPlugin;

use sourcerenderer_engine::renderer::RendererType;

pub trait RendererPicker {
    fn pick_renderer() -> RendererType;
}
