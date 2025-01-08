use std::{
    f32,
    usize,
};

use bevy_ecs::entity::Entity;
use bevy_math::Affine3A;
use sourcerenderer_core::{
    Matrix4, Quaternion, Vec3
};

use crate::asset::ModelHandle;

pub struct RendererStaticDrawable {
    pub entity: Entity,
    pub transform: Affine3A,
    pub old_transform: Affine3A,
    pub model: ModelHandle,
    pub receive_shadows: bool,
    pub cast_shadows: bool,
    pub can_move: bool,
}

#[derive(Clone)]
pub struct View {
    pub camera_position: Vec3,
    pub camera_rotation: Quaternion,
    pub view_matrix: Matrix4,
    pub proj_matrix: Matrix4,
    pub old_camera_matrix: Matrix4,
    pub camera_transform: Affine3A,
    pub camera_fov: f32,
    pub near_plane: f32,
    pub far_plane: f32,
    pub aspect_ratio: f32,
    pub old_visible_drawables_bitset: Vec<u32>,
    pub visible_drawables_bitset: Vec<u32>,
    pub drawable_parts: Vec<DrawablePart>,
}

impl Default for View {
    fn default() -> Self {
        Self {
            camera_position: Vec3::default(),
            camera_rotation: Quaternion::default(),
            camera_transform: Affine3A::default(),
            old_camera_matrix: Matrix4::default(),
            view_matrix: Matrix4::default(),
            proj_matrix: Matrix4::default(),
            camera_fov: f32::consts::PI / 2f32,
            near_plane: 0.1f32,
            far_plane: 100f32,
            aspect_ratio: 16.0f32 / 9.0f32,
            drawable_parts: Vec::new(),
            old_visible_drawables_bitset: Vec::new(),
            visible_drawables_bitset: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct DrawablePart {
    pub drawable_index: usize,
    pub part_index: usize,
}

pub(crate) fn make_camera_view(position: Vec3, rotation: Quaternion) -> Matrix4 {
    let position = Vec3::new(position.x, position.y, position.z);
    let forward = rotation.mul_vec3(Vec3::new(0.0f32, 0.0f32, 1.0f32));
    Matrix4::look_at_lh(
        position,
        position + forward,
        Vec3::new(0.0f32, 1.0f32, 0.0f32),
    )
}

pub(crate) fn make_camera_proj(fov: f32, aspect_ratio: f32, z_near: f32, z_far: f32) -> Matrix4 {
    let vertical_fov = 2f32 * ((fov / 2f32).tan() * (1f32 / aspect_ratio)).atan();
    Matrix4::perspective_lh(vertical_fov, aspect_ratio, z_near, z_far)
}
