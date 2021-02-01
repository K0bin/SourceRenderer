use legion::{World, Resources, component};
use crate::{Transform, Camera};
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
use sourcerenderer_core::platform::io::IO;
use std::io::{Seek, SeekFrom, Read, Cursor};

#[derive(Clone)]
#[repr(C)]
struct Vertex {
  pub position: Vec3,
  pub normal: Vec3,
  pub uv: Vec2,
  pub lightmap_uv: Vec2,
  pub alpha: f32
}

struct SpinningCube {}

pub fn install<P: Platform>(world: &mut World, resources: &mut Resources, systems: &mut SystemBuilder, asset_manager: &Arc<AssetManager<P>>) {
  let indices: [u32; 36] = [
    2, 1, 0, 0, 3, 2, // front
    4, 5, 6, 6, 7, 4, // back
    10, 9, 8, 8, 11, 10, // top
    12, 13, 14, 14, 15, 12, // bottom
    18, 17, 16, 16, 19, 18, // left
    20, 21, 22, 22, 23, 20, // right
  ];

  let triangle = [
    // FRONT
    Vertex {
      position: Vec3::new(-1.0f32, -1.0f32, -1.0f32),
      normal: Vec3::new(0.0f32, 0.0f32, -1.0f32),
      uv: Vec2::new(0.0f32, 0.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(1.0f32, -1.0f32, -1.0f32),
      normal: Vec3::new(1.0f32, 0.0f32, -1.0f32),
      uv: Vec2::new(1.0f32, 0.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(1.0f32, 1.0f32, -1.0f32),
      normal: Vec3::new(0.0f32, 0.0f32, -1.0f32),
      uv: Vec2::new(1.0f32, 1.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(-1.0f32, 1.0f32, -1.0f32),
      normal: Vec3::new(0.0f32, 0.0f32, -1.0f32),
      uv: Vec2::new(0.0f32, 1.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    // BACK
    Vertex {
      position: Vec3::new(-1.0f32, -1.0f32, 1.0f32),
      normal: Vec3::new(0.0f32, 0.0f32, 1.0f32),
      uv: Vec2::new(1.0f32, 0.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(1.0f32, -1.0f32, 1.0f32),
      normal: Vec3::new(0.0f32, 0.0f32, 1.0f32),
      uv: Vec2::new(0.0f32, 0.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(1.0f32, 1.0f32, 1.0f32),
      normal: Vec3::new(0.0f32, 0.0f32, 1.0f32),
      uv: Vec2::new(0.0f32, 1.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(-1.0f32, 1.0f32, 1.0f32),
      normal: Vec3::new(0.0f32, 0.0f32, 1.0f32),
      uv: Vec2::new(1.0f32, 1.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    // TOP
    Vertex {
      position: Vec3::new(-1.0f32, 1.0f32, -1.0f32),
      normal: Vec3::new(0.0f32, 1.0f32, 0.0f32),
      uv: Vec2::new(1.0f32, 0.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(1.0f32, 1.0f32, -1.0f32),
      normal: Vec3::new(0.0f32, 1.0f32, 0.0f32),
      uv: Vec2::new(0.0f32, 0.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(1.0f32, 1.0f32, 1.0f32),
      normal: Vec3::new(0.0f32, 1.0f32, 0.0f32),
      uv: Vec2::new(0.0f32, 1.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(-1.0f32, 1.0f32, 1.0f32),
      normal: Vec3::new(0.0f32, 1.0f32, 0.0f32),
      uv: Vec2::new(1.0f32, 1.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    // BOTTOM
    Vertex {
      position: Vec3::new(-1.0f32, -1.0f32, -1.0f32),
      normal: Vec3::new(0.0f32, -1.0f32, 0.0f32),
      uv: Vec2::new(1.0f32, 0.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(1.0f32, -1.0f32, -1.0f32),
      normal: Vec3::new(0.0f32, -1.0f32, 0.0f32),
      uv: Vec2::new(0.0f32, 0.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(1.0f32, -1.0f32, 1.0f32),
      normal: Vec3::new(0.0f32, -1.0f32, 0.0f32),
      uv: Vec2::new(0.0f32, 1.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(-1.0f32, -1.0f32, 1.0f32),
      normal: Vec3::new(0.0f32, -1.0f32, 0.0f32),
      uv: Vec2::new(1.0f32, 1.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    // LEFT
    Vertex {
      position: Vec3::new(-1.0f32, -1.0f32, -1.0f32),
      normal: Vec3::new(-1.0f32, 0.0f32, 0.0f32),
      uv: Vec2::new(1.0f32, 0.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(-1.0f32, 1.0f32, -1.0f32),
      normal: Vec3::new(-1.0f32, 0.0f32, 0.0f32),
      uv: Vec2::new(0.0f32, 0.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(-1.0f32, 1.0f32, 1.0f32),
      normal: Vec3::new(-1.0f32, 0.0f32, 0.0f32),
      uv: Vec2::new(0.0f32, 1.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(-1.0f32, -1.0f32, 1.0f32),
      normal: Vec3::new(-1.0f32, 0.0f32, 0.0f32),
      uv: Vec2::new(1.0f32, 1.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    // RIGHT
    Vertex {
      position: Vec3::new(1.0f32, -1.0f32, -1.0f32),
      normal: Vec3::new(1.0f32, 0.0f32, 0.0f32),
      uv: Vec2::new(1.0f32, 0.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(1.0f32, 1.0f32, -1.0f32),
      normal: Vec3::new(1.0f32, 0.0f32, 0.0f32),
      uv: Vec2::new(0.0f32, 0.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(1.0f32, 1.0f32, 1.0f32),
      normal: Vec3::new(1.0f32, 0.0f32, 0.0f32),
      uv: Vec2::new(0.0f32, 1.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
    Vertex {
      position: Vec3::new(1.0f32, -1.0f32, 1.0f32),
      normal: Vec3::new(1.0f32, 0.0f32, 0.0f32),
      uv: Vec2::new(1.0f32, 1.0f32),
      lightmap_uv: Vec2::new(0.0f32, 0.0f32),
      alpha: 0f32
    },
  ];

  let mut image_file = <P::IO as IO>::open_asset("texture.png").expect("Failed to open texture");
  let length = image_file.seek(SeekFrom::End(0)).unwrap();
  image_file.seek(SeekFrom::Start(0)).unwrap();
  let mut data = Vec::<u8>::with_capacity(length as usize);
  unsafe {
    data.set_len(length as usize);
  }
  image_file.read_exact(&mut data).unwrap();
  let image = image::load_from_memory(&data).unwrap();
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

  let triangle_data = unsafe { std::slice::from_raw_parts(triangle.as_ptr() as *const u8, triangle.len() * std::mem::size_of::<Vertex>()) };
  let index_data = unsafe { std::slice::from_raw_parts(indices.as_ptr() as *const u8, indices.len() * std::mem::size_of::<u32>()) };
  asset_manager.add_mesh("cube_mesh", triangle_data, index_data);
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
  }, Transform::new(Vec3::new(0f32, 0f32, -5f32)), SpinningCube {}));

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