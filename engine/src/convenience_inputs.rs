use bevy_app::{App, AppExit, Plugin, Update};
use bevy_ecs::change_detection::Res;
use bevy_input::ButtonInput;
use bevy_input::keyboard::KeyCode;

use crate::engine::{FullscreenPreference, MouseLockPreference};
use bevy_ecs::message::MessageWriter;
use bevy_ecs::system::ResMut;

pub struct ConvenienceInputs;

impl Plugin for ConvenienceInputs {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (inputs_system,));
    }
}

pub(crate) fn inputs_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut fullscreen_preference: ResMut<FullscreenPreference>,
    mut lock_preference: ResMut<MouseLockPreference>,
    mut events: MessageWriter<AppExit>,
) {
    if !cfg!(target_arch = "wasm32") && keyboard.pressed(KeyCode::Escape) {
        events.write(AppExit::Success);
    }
    if keyboard.just_pressed(KeyCode::F11) {
        fullscreen_preference.request_fullscreen = !fullscreen_preference.request_fullscreen;
    }
    if keyboard.just_pressed(KeyCode::F10) {
        lock_preference.request_lock = !lock_preference.request_lock;
    }
}
