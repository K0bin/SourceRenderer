use bevy_ecs::entity::Entity;
use sourcerenderer_core::{Matrix4, gpu::GPUBackend};

use crate::ui::UIDrawData;

pub enum RendererCommand<B: GPUBackend> {
    RegisterStatic {
        entity: Entity,
        transform: Matrix4,
        model_path: String,
        receive_shadows: bool,
        cast_shadows: bool,
        can_move: bool,
    },
    UnregisterStatic(Entity),
    RegisterPointLight {
        entity: Entity,
        transform: Matrix4,
        intensity: f32,
    },
    UnregisterPointLight(Entity),
    RegisterDirectionalLight {
        entity: Entity,
        transform: Matrix4,
        intensity: f32,
    },
    UnregisterDirectionalLight(Entity),
    UpdateTransform {
        entity: Entity,
        transform_mat: Matrix4,
    },
    UpdateCameraTransform {
        camera_transform_mat: Matrix4,
        fov: f32,
    },
    SetLightmap(String),
    RenderUI(UIDrawData<B>),
    EndFrame,
    Quit
}
