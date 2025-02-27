use bevy_ecs::entity::Entity;
use bevy_math::Affine3A;
use sourcerenderer_core::gpu::GPUBackend;

use crate::{engine::WindowState, ui::UIDrawData};

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
    Quit,
    WindowChanged(WindowState)
}
