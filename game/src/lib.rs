mod fps_camera;
mod plugin;
mod spinning_cube;
mod uni_project_plugin;

pub use plugin::GamePlugin;
use sourcerenderer_engine::renderer::RendererType;
pub use uni_project_plugin::UniProjectPlugin;

pub trait RendererPicker {
    fn pick_renderer() -> RendererType;
}
