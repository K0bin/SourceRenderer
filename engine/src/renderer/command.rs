use legion::Entity;
use sourcerenderer_core::Matrix4;

#[derive(Clone)]
pub struct StaticDrawable {
  pub entity: Entity,
  pub transform: Matrix4,
  pub model_path: String,
  pub receive_shadows: bool,
  pub cast_shadows: bool,
  pub can_move: bool
}

impl StaticDrawable {
  pub fn new(entity: Entity, transform: Matrix4, model_path: &str, receive_shadows: bool, cast_shadows: bool, can_move: bool) -> Self {
    Self {
      entity,
      transform,
      can_move,
      cast_shadows,
      receive_shadows,
      model_path: model_path.to_string()
    }
  }
}

pub enum RendererCommand {
  RegisterStatic(StaticDrawable),
  UnregisterStatic(Entity),
  UpdateTransform{ entity: Entity, transform_mat: Matrix4 },
  UpdateCameraTransform { camera_transform_mat: Matrix4, fov: f32 },
  EndFrame
}
