use std::marker::PhantomData;

use crate::{RendererPicker, fps_camera};
use bevy_app::{App, Plugin};
use sourcerenderer_core::platform::PlatformIO;
use sourcerenderer_engine::renderer::RendererType;
use spinning_cube::SpinningCubePlugin;
use crate::test_game::khronos_assets::{load_khronos_model, KhronosAsset};

mod spinning_cube;
mod khronos_assets;

pub struct TestGamePlugin<IO: PlatformIO>(PhantomData<IO>);

unsafe impl<IO: PlatformIO> Send for TestGamePlugin<IO> {}
unsafe impl<IO: PlatformIO> Sync for TestGamePlugin<IO> {}

impl<IO: PlatformIO> Default for TestGamePlugin<IO> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<IO: PlatformIO> Plugin for TestGamePlugin<IO> {
    fn build(&self, app: &mut App) {
        log::info!("Initializing GamePlugin");

        load_khronos_model::<IO>(app, KhronosAsset::FlightHelmet);
        fps_camera::install(app);
        app.add_plugins(SpinningCubePlugin);
    }
}

impl<IO: PlatformIO> RendererPicker for TestGamePlugin<IO> {
    fn pick_renderer() -> RendererType {
        #[cfg(target_arch = "wasm32")]
        {
            RendererType::Compat
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            //RendererType::Regular // Broken!
            RendererType::Compat
        }
    }
}
