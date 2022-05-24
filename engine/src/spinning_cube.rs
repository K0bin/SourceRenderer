use legion::{World, Resources, component};
use log::trace;
use sourcerenderer_core::input::Key;
use crate::input::InputState;
use crate::math::BoundingBox;
use crate::physics::{ColliderComponent, RigidBodyComponent, RigidBodyType};
use crate::{Transform, Camera};
use nalgebra::{Unit, UnitQuaternion};
use crate::asset::{AssetManager, MeshRange};
use sourcerenderer_core::{Platform, Quaternion};
use sourcerenderer_core::Vec3;
use sourcerenderer_core::Vec2;
use std::sync::Arc;
use crate::renderer::{PointLightComponent, StaticRenderableComponent};
use legion::systems::{Builder as SystemBuilder, CommandBuffer};

use crate::camera::ActiveCamera;
use crate::fps_camera::FPSCameraComponent;
use crate::game::DeltaTime;
use std::f32;

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
  systems.add_system(place_lights_system(PlaceLightsState {
    was_space_down: false
  }));

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

  /*let mut image_file = <P::IO as IO>::open_asset("texture.png").expect("Failed to open texture");
  let length = image_file.seek(SeekFrom::End(0)).unwrap();
  image_file.seek(SeekFrom::Start(0)).unwrap();
  let mut data = Vec::<u8>::with_capacity(length as usize);
  unsafe {
    data.set_len(length as usize);
  }
  image_file.read_exact(&mut data).unwrap();
  let image = image::load_from_memory(&data).unwrap();
  let data = image.to_rgba8();
  let texture_info = TextureInfo {
    format: Format::RGBA8,
    width: image.width(),
    height: image.height(),
    depth: 1,
    mip_levels: 1,
    array_length: 1,
    samples: SampleCount::Samples1
  };*/

  let triangle_data = unsafe { std::slice::from_raw_parts(triangle.as_ptr() as *const u8, std::mem::size_of_val(&triangle[..])) }.to_vec().into_boxed_slice();
  let index_data = unsafe { std::slice::from_raw_parts(indices.as_ptr() as *const u8, std::mem::size_of_val(&indices[..])) }.to_vec().into_boxed_slice();
  let bounding_box = BoundingBox::new(Vec3::new(-1f32, -1f32, -1f32), Vec3::new(1f32, 1f32, 1f32));
  asset_manager.add_mesh("cube_mesh", triangle_data, triangle.len() as u32, index_data, vec![MeshRange {
    start: 0,
    count: indices.len() as u32
  }].into_boxed_slice(), Some(bounding_box));
  //asset_manager.add_texture("cube_texture_albedo", &texture_info, data.to_vec().into_boxed_slice());
  asset_manager.add_material("cube_material", "cube_texture_albedo", 0f32, 0f32);
  asset_manager.add_model("cube_model", "cube_mesh", &["cube_material"]);

  systems.add_system(spin_system());
  world.push((StaticRenderableComponent {
    receive_shadows: true,
    cast_shadows: true,
    can_move: true,
    model_path: "cube_model".to_owned()
  },
  Transform::new(Vec3::new(0f32, 0f32, -5f32)),
  SpinningCube {},
  RigidBodyComponent {
    body_type: RigidBodyType::Dynamic,
  },
  ColliderComponent::Box {
    width: 1f32,
    height: 1f32,
    depth: 1f32
  }));

  world.push((Transform::new(Vec3::new(0f32, -2f32, -5f32)),
  RigidBodyComponent {
    body_type: RigidBodyType::Static
  },
  ColliderComponent::Box {
    width: 10f32,
    height: 0.1f32,
    depth: 10f32
  }));

  let camera = world.push((Camera {
    fov: f32::consts::PI / 2f32,
    interpolate_rotation: false
  }, Transform::new(Vec3::new(0.0f32, 1.0f32, -1.0f32)), FPSCameraComponent::default()));

  resources.insert(ActiveCamera(camera));
  trace!("Added spinning cube");
}

#[system(for_each)]
#[filter(component::<SpinningCube>())]
fn spin(transform: &mut Transform, #[resource] delta_time: &DeltaTime) {
  transform.rotation *= Quaternion::from_axis_angle(&Unit::new_unchecked(Vec3::new(0.0f32, 1.0f32, 0.0f32)), 1.0f32 * delta_time.secs());
}


pub struct PlaceLightsState {
  was_space_down: bool
}

#[system(for_each)]
#[filter(component::<FPSCameraComponent>())]
fn place_lights(#[resource] input: &InputState, transform: &mut Transform, #[state] state: &mut PlaceLightsState, cmd_buffer: &mut CommandBuffer) {
  if input.is_key_down(Key::Space) {
    if !state.was_space_down {
      cmd_buffer.extend([(Transform {
        position: transform.position,
        rotation: UnitQuaternion::default(),
        scale: Vec3::new(1f32, 1f32, 1f32),
      }, PointLightComponent { intensity: 1.0f32 })]);
    }
    state.was_space_down = true;
  } else {
    state.was_space_down = false;
  }
}