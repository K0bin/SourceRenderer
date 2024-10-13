use bevy_ecs::entity::Entity;
use sourcerenderer_core::{Matrix4, gpu::GPUBackend, Affine3A};

use crate::ui::UIDrawData;

pub enum RendererCommand<B: GPUBackend> {
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
    UpdateTransform {
        entity: Entity,
        transform: Affine3A,
    },
    UpdateCameraTransform {
        camera_transform: Affine3A,
        fov: f32,
    },
    SetLightmap(String),
    RenderUI(UIDrawData<B>),
    EndFrame,
    Quit
}
