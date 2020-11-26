use legion::{World, Resources, component};
use crate::{Vertex, Transform, Camera};
use std::path::Path;
use sourcerenderer_core::graphics::{Format, TextureInfo, SampleCount};
use nalgebra::{Unit};
use image::GenericImageView;
use crate::asset::AssetManager;
use sourcerenderer_core::{Platform, Quaternion};
use sourcerenderer_core::Vec3;
use sourcerenderer_core::Vec2;
use std::sync::Arc;
use crate::renderer::StaticRenderableComponent;
use legion::systems::Builder as SystemBuilder;

use crate::camera::ActiveCamera;
use crate::fps_camera::{FPSCamera, FPSCameraComponent};
use crate::scene::DeltaTime;
use std::f32;

struct SpinningCube {}

pub fn install<P: Platform>(world: &mut World, resources: &mut Resources, systems: &mut SystemBuilder, asset_manager: &Arc<AssetManager<P>>) {
  let indices = [0u32, 1u32, 2u32, 2u32, 3u32, 0u32, // front
    6u32, 5u32, 4u32, 4u32, 7u32, 6u32, // back
    5u32, 1u32, 0u32, 0u32, 4u32, 5u32, // top
    3u32, 2u32, 6u32, 6u32, 7u32, 3u32, // bottom
    7u32, 4u32, 0u32, 0u32, 3u32, 7u32, // left
    1u32, 5u32, 6u32, 6u32, 2u32, 1u32]; // right

  let triangle = [
    Vertex {
      position: Vec3::new(-1.0f32, -1.0f32, -1.0f32),
      color: Vec3::new(1.0f32, 0.0f32, 0.0f32),
      uv: Vec2::new(0.0f32, 0.0f32)
    },
    Vertex {
      position: Vec3::new(1.0f32, -1.0f32, -1.0f32),
      color: Vec3::new(0.0f32, 1.0f32, 0.0f32),
      uv: Vec2::new(1.0f32, 0.0f32)
    },
    Vertex {
      position: Vec3::new(1.0f32, 1.0f32, -1.0f32),
      color: Vec3::new(0.0f32, 0.0f32, 1.0f32),
      uv: Vec2::new(1.0f32, 1.0f32)
    },
    Vertex {
      position: Vec3::new(-1.0f32, 1.0f32, -1.0f32),
      color: Vec3::new(1.0f32, 1.0f32, 1.0f32),
      uv: Vec2::new(0.0f32, 1.0f32)
    },
    // face 2
    Vertex {
      position: Vec3::new(-1.0f32, -1.0f32, 1.0f32),
      color: Vec3::new(1.0f32, 0.0f32, 0.0f32),
      uv: Vec2::new(1.0f32, 0.0f32)
    },
    Vertex {
      position: Vec3::new(1.0f32, -1.0f32, 1.0f32),
      color: Vec3::new(0.0f32, 1.0f32, 1.0f32),
      uv: Vec2::new(0.0f32, 0.0f32)
    },
    Vertex {
      position: Vec3::new(1.0f32, 1.0f32, 1.0f32),
      color: Vec3::new(1.0f32, 0.0f32, 1.0f32),
      uv: Vec2::new(0.0f32, 1.0f32)
    },
    Vertex {
      position: Vec3::new(-1.0f32, 1.0f32, 1.0f32),
      color: Vec3::new(0.0f32, 1.0f32, 1.0f32),
      uv: Vec2::new(1.0f32, 1.0f32)
    }
  ];

  let image = image::open(Path::new("..").join(Path::new("..")).join(Path::new("engine")).join(Path::new("texture.png"))).expect("Failed to open texture");
  let data = image.to_rgba();
  let texture_info = TextureInfo {
    format: Format::RGBA8,
    width: image.width(),
    height: image.height(),
    depth: 0,
    mip_levels: 1,
    array_length: 1,
    samples: SampleCount::Samples1
  };

  asset_manager.add_mesh("cube_mesh", &triangle, &indices);
  asset_manager.add_texture("cube_texture_albedo", &texture_info, &data);
  asset_manager.add_material("cube_material", "cube_texture_albedo");
  asset_manager.add_model("cube_model", "cube_mesh", &["cube_material"]);
  asset_manager.flush();

  systems.add_system(spin_system());
  world.push((StaticRenderableComponent {
    receive_shadows: true,
    cast_shadows: true,
    can_move: true,
    model_path: "cube_model".to_owned()
  }, Transform::new(Vec3::new(0f32, 0f32, 0f32)), SpinningCube {}));

  let camera = world.push((Camera {
    fov: f32::consts::PI / 2f32
  }, Transform::new(Vec3::new(0.0f32, 0.0f32, -5.0f32)), FPSCameraComponent {}));

  resources.insert(ActiveCamera(camera));
}

#[system(for_each)]
#[filter(component::<SpinningCube>())]
fn spin(transform: &mut Transform, #[resource] delta_time: &DeltaTime) {
  transform.rotation *= Quaternion::from_axis_angle(&Unit::new_unchecked(Vec3::new(0.0f32, 1.0f32, 0.0f32)), 1.0f32 * delta_time.secs());
}