use bevy_app::{App, Update, FixedUpdate};
use bevy_ecs::component::Component;
use bevy_ecs::event::EventReader;
use bevy_ecs::query::With;
use bevy_ecs::system::{Query, Res};
use bevy_input::keyboard::KeyCode;
use bevy_input::mouse::MouseMotion;
use bevy_input::ButtonInput;
use bevy_math::Vec2;
use bevy_time::{Fixed, Time};
use bevy_transform::components::Transform;
use sourcerenderer_core::{
    Quaternion,
    Vec3,
};

use sourcerenderer_engine::Camera;

pub fn install(app: &mut App) {
    app.add_systems(FixedUpdate, (fps_camera_movement,));
    app.add_systems(Update, (retrieve_fps_camera_rotation,));
}

#[derive(Component, Default)]
pub(crate) struct FPSCameraComponent {
    fps_camera: FPSCamera,
}

#[derive(Debug)]
struct FPSCamera {
    sensitivity: f32,
    pitch: f32,
    yaw: f32,
    last_touch_position: Vec2,
}

impl Default for FPSCamera {
    fn default() -> Self {
        FPSCamera {
            sensitivity: 10.0f32,
            pitch: 0f32,
            yaw: 0f32,
            last_touch_position: Vec2::new(0f32, 0f32),
        }
    }
}

fn fps_camera_rotation(mouse: &MouseMotion, fps_camera: &mut FPSCamera) -> Quaternion {
    let touch_position = Vec2::new(0f32, 0f32);
    let touch_delta = if fps_camera.last_touch_position.x.abs() > 0.1f32
        && fps_camera.last_touch_position.y.abs() > 0.1f32
        && touch_position.x.abs() > 0.1f32
        && touch_position.y.abs() > 0.1f32
    {
        touch_position - fps_camera.last_touch_position
    } else {
        Vec2::new(0f32, 0f32)
    };
    fps_camera.pitch += mouse.delta.y as f32 / 20_000f32 * fps_camera.sensitivity;
    fps_camera.yaw += mouse.delta.x as f32 / 20_000f32 * fps_camera.sensitivity;
    fps_camera.pitch -= touch_delta.y / 20_000f32 * fps_camera.sensitivity;
    fps_camera.yaw -= touch_delta.x / 20_000f32 * fps_camera.sensitivity;

    fps_camera.pitch = fps_camera
        .pitch
        .max(-std::f32::consts::FRAC_PI_2 + 0.01f32)
        .min(std::f32::consts::FRAC_PI_2 - 0.01f32);

    fps_camera.last_touch_position = touch_position;
    Quaternion::from_euler(bevy_math::EulerRot::YXZ, fps_camera.yaw, fps_camera.pitch, 0f32)
}

pub(crate) fn retrieve_fps_camera_rotation(
    mut mouse_motion: EventReader<MouseMotion>,
    mut query: Query<(&mut Transform, &mut FPSCameraComponent), With<Camera>>,
) {
    for (mut transform, mut fps_camera) in query.iter_mut() {
        for event in mouse_motion.read() {
            transform.rotation = fps_camera_rotation(event, &mut fps_camera.fps_camera);
        }
    }
}

pub(crate) fn fps_camera_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<&mut Transform, (With<Camera>, With<FPSCameraComponent>)>,
    tick_rate: Res<Time<Fixed>>,
) {
    let mut movement_vector = Vec3::new(0f32, 0f32, 0f32);
    if keyboard.pressed(KeyCode::KeyW) {
        movement_vector.z += 1f32;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        movement_vector.z -= 1f32;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        movement_vector.x -= 1f32;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        movement_vector.x += 1f32;
    }
    if keyboard.pressed(KeyCode::KeyQ) {
        movement_vector.y += 1f32;
    }
    if keyboard.pressed(KeyCode::KeyE) {
        movement_vector.y -= 1f32;
    }

    for mut transform in query.iter_mut() {
        if movement_vector.x.abs() > 0.00001f32 || movement_vector.z.abs() > 0.00001f32 {
            let y = movement_vector.y;
            movement_vector = movement_vector.normalize();
            movement_vector = Vec3::new(movement_vector.x, 0.0f32, movement_vector.z).normalize();
            movement_vector = transform.rotation.mul_vec3(movement_vector);
            movement_vector.y = y;
        }

        if movement_vector.x.abs() > 0.00001f32
            || movement_vector.y.abs() > 0.00001f32
            || movement_vector.z.abs() > 0.00001f32
        {
            movement_vector = movement_vector.normalize();
            transform.translation += movement_vector * 8f32 * tick_rate.timestep().as_secs_f32();
        }
    }
}
