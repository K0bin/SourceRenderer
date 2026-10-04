use std::marker::PhantomData;

use crate::{RendererPicker, fps_camera};
use bevy_app::{App, Plugin};
use bevy_math::Vec3;
use bevy_transform::prelude::Transform;
use sourcerenderer_core::platform::PlatformIO;
use sourcerenderer_engine::asset::PlacedLevel;
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

        let handle = load_khronos_model::<IO>(app, KhronosAsset::FlightHelmet);
        for i in 0..10_000 {
            let x = (i % 100) as f32;
            let z = (i / 100) as f32;
            let transform = Transform::from_translation(Vec3::new(x, 0.0f32, z));
            app.world_mut().spawn((transform, PlacedLevel(handle)));
        }

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
