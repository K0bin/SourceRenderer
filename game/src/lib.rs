mod fps_camera;

#[cfg(all(feature = "uni_project", feature = "test_game"))]
compile_error!("Features 'Pick only one game feature.");

#[cfg(not(any(feature = "uni_project", feature = "test_game")))]
compile_error!("Features 'Pick a game feature.");

#[cfg(feature = "test_game")]
mod test_game;
#[cfg(feature = "test_game")]
pub use test_game::TestGamePlugin as GamePlugin;

#[cfg(feature = "uni_project")]
mod uni_project;
#[cfg(feature = "uni_project")]
pub use uni_project::UniProjectPlugin as GamePlugin;

use sourcerenderer_engine::renderer::RendererType;

pub trait RendererPicker {
    fn pick_renderer() -> RendererType;
}
