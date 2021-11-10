use legion::Entity;
use sourcerenderer_core::Matrix4;

pub enum RendererCommand {
  RegisterStatic {
    entity: Entity,
    transform: Matrix4,
    model_path: String,
    receive_shadows: bool,
    cast_shadows: bool,
    can_move: bool
  },
  UnregisterStatic(Entity),
  RegisterPointLight {
    entity: Entity,
    transform: Matrix4,
    intensity: f32
  },
  UnregisterPointLight(Entity),
  RegisterDirectionalLight {
    entity: Entity,
    transform: Matrix4,
    intensity: f32
  },
  UnregisterDirectionalLight(Entity),
  UpdateTransform{ entity: Entity, transform_mat: Matrix4 },
  UpdateCameraTransform { camera_transform_mat: Matrix4, fov: f32 },
  EndFrame
}
