use crate::{Renderer, AssetManager, Vertex};
use std::sync::{Arc, Mutex};
use sourcerenderer_core::{Platform, Vec3, Vec2};
use crossbeam_channel::bounded;
use async_std::task;
use std::path::Path;
use sourcerenderer_core::graphics::{TextureInfo, Format, SampleCount};
use image::GenericImageView;
use nalgebra::{Point3, Matrix4, Rotation3, Vector3};
use crate::renderer::*;
use std::thread::Thread;
use std::time::Duration;

pub struct Scene {
  entities: Mutex<Vec<Entity>>
}

pub struct Entity {
}

impl Scene {
  pub fn run<P: Platform>(renderer: &Arc<Renderer<P>>,
                          asset_manager: &Arc<AssetManager<P>>) -> Arc<Scene> {
    let scene = Arc::new(Scene::new());
    let scene_ref = scene.clone();

    let asset_manager_ref = asset_manager.clone();
    let renderer_ref = renderer.clone();

    task::spawn(async move {

      let indices = [0u32, 1u32, 2u32, 2u32, 3u32, 0u32, // front
        6u32, 5u32, 4u32, 4u32, 7u32, 6u32, // back
        5u32, 1u32, 0u32, 0u32, 4u32, 5u32, // top
        3u32, 2u32, 6u32, 6u32, 7u32, 3u32, // bottom
        7u32, 4u32, 0u32, 0u32, 3u32, 7u32, // left
        1u32, 5u32, 6u32, 6u32, 2u32, 1u32]; // right

      let triangle = [
        Vertex {
          position: Vec3 {
            x: -1.0f32,
            y: -1.0f32,
            z: -1.0f32,
          },
          color: Vec3 {
            x: 1.0f32,
            y: 0.0f32,
            z: 0.0f32,
          },
          uv: Vec2 {
            x: 0.0f32,
            y: 0.0f32
          }
        },
        Vertex {
          position: Vec3 {
            x: 1.0f32,
            y: -1.0f32,
            z: -1.0f32,
          },
          color: Vec3 {
            x: 0.0f32,
            y: 1.0f32,
            z: 0.0f32,
          },
          uv: Vec2 {
            x: 1.0f32,
            y: 0.0f32
          }
        },
        Vertex {
          position: Vec3 {
            x: 1.0f32,
            y: 1.0f32,
            z: -1.0f32,
          },
          color: Vec3 {
            x: 0.0f32,
            y: 0.0f32,
            z: 1.0f32,
          },
          uv: Vec2 {
            x: 1.0f32,
            y: 1.0f32
          }
        },
        Vertex {
          position: Vec3 {
            x: -1.0f32,
            y: 1.0f32,
            z: -1.0f32,
          },
          color: Vec3 {
            x: 1.0f32,
            y: 1.0f32,
            z: 1.0f32,
          },
          uv: Vec2 {
            x: 0.0f32,
            y: 1.0f32
          }
        },
        // face 2
        Vertex {
          position: Vec3 {
            x: -1.0f32,
            y: -1.0f32,
            z: 1.0f32,
          },
          color: Vec3 {
            x: 1.0f32,
            y: 0.0f32,
            z: 0.0f32,
          },
          uv: Vec2 {
            x: 1.0f32,
            y: 0.0f32
          }
        },
        Vertex {
          position: Vec3 {
            x: 1.0f32,
            y: -1.0f32,
            z: 1.0f32,
          },
          color: Vec3 {
            x: 0.0f32,
            y: 1.0f32,
            z: 1.0f32,
          },
          uv: Vec2 {
            x: 0.0f32,
            y: 0.0f32
          }
        },
        Vertex {
          position: Vec3 {
            x: 1.0f32,
            y: 1.0f32,
            z: 1.0f32,
          },
          color: Vec3 {
            x: 1.0f32,
            y: 0.0f32,
            z: 1.0f32,
          },
          uv: Vec2 {
            x: 0.0f32,
            y: 1.0f32
          }
        },
        Vertex {
          position: Vec3 {
            x: -1.0f32,
            y: 1.0f32,
            z: 1.0f32,
          },
          color: Vec3 {
            x: 0.0f32,
            y: 1.0f32,
            z: 1.0f32,
          },
          uv: Vec2 {
            x: 1.0f32,
            y: 1.0f32
          }
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

      let mesh_key = asset_manager_ref.add_mesh("mesh", &triangle, &indices);
      let texture_key = asset_manager_ref.add_texture("texture", &texture_info, &data);
      let material_key = asset_manager_ref.add_material("material", &texture_key);
      let model_key = asset_manager_ref.add_model("model", &mesh_key, &[&material_key]);
      asset_manager_ref.flush();

      let mut camera = Matrix4::identity();
      let mut cube_rotation = 0f32;
      'scene_loop: loop {
        let old_camera = camera;
        camera =
          Matrix4::new_perspective(16f32 / 9f32, 1.02974f32, 0.001f32, 20.0f32)
            *
            Matrix4::look_at_rh(
              &Point3::new(0.0f32, 2.0f32, -5.0f32),
              &Point3::new(0.0f32, 0.0f32, 0.0f32),
              &Vector3::new(0.0f32, 1.0f32, 0.0f32)
            );

        let old_cube_transform = Matrix4::from(Rotation3::from_axis_angle(&Vector3::y_axis(), cube_rotation / 300.0f32));
        cube_rotation += 1f32;
        let cube_transform = Matrix4::from(Rotation3::from_axis_angle(&Vector3::y_axis(), cube_rotation / 300.0f32));

        let message = Renderables::<P> {
          elements: vec![RenderableAndTransform::<P> {
            renderable: Renderable::Static(StaticModelRenderable {
              model: model_key.clone(),
              receive_shadows: false,
              cast_shadows: false,
              movability: Movability::Static
            }),
            transform: cube_transform,
            old_transform: old_cube_transform
          }],
          camera,
          old_camera
        };
        renderer_ref.render(message);
      }
    });
    scene_ref
  }

  fn new() -> Self {
    Self {
      entities: Mutex::new(Vec::new())
    }
  }

  pub fn tick(&self) {

  }
}
