use crate::engine::WindowState;
use bevy_ecs::entity::Entity;
use bevy_math::Affine3A;
use dear_imgui_rs::FrameSnapshot;

pub enum RendererCommand {
    RegisterStatic {
        entity: Entity,
        transform: Affine3A,
        model_path: String,
        receive_shadows: bool,
        cast_shadows: bool,
        can_move: bool,
    },
    UnregisterStatic(Entity),
    RegisterPointLight {
        entity: Entity,
        transform: Affine3A,
        intensity: f32,
    },
    UnregisterPointLight(Entity),
    RegisterDirectionalLight {
        entity: Entity,
        transform: Affine3A,
        intensity: f32,
    },
    UnregisterDirectionalLight(Entity),
    RegisterVolume {
        entity: Entity,
        transform: Affine3A,
        texture_path: String,
        transfer_function_texture_path: String,
        texture_lod: u32,
        min_threshold: f32,
        max_threshold: f32,
        transparent: bool,
    },
    UnregisterVolume(Entity),
    UpdateTransform {
        entity: Entity,
        transform: Affine3A,
    },
    UpdateCameraTransform {
        camera_transform: Affine3A,
        fov: f32,
    },
    SetLightmap(String),
    EndFrame,
    WindowChanged(WindowState),
    UpdateUIData(FrameSnapshot),
}
