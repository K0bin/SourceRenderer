use crate::{Vertex, camera};
use std::sync::{Arc, Mutex};
use sourcerenderer_core::{Platform, Vec3, Vec2};
use crossbeam_channel::bounded;
use async_std::task;
use std::path::Path;
use sourcerenderer_core::graphics::{TextureInfo, Format, SampleCount};
use image::GenericImageView;
use nalgebra::{Point3, Matrix4, Rotation3, Vector3};
use crate::renderer::*;
use std::thread;
use std::time::Duration;
use crate::asset::AssetManager;
use legion::{World, Resources, Schedule};
use legion::systems::Builder as SystemBuilder;
use crate::transform;

pub struct Scene {

}

impl Scene {
  pub fn run<P: Platform>(renderer: &Arc<Renderer<P>>,
                          asset_manager: &Arc<AssetManager<P>>,
                          input: &Arc<P::Input>) {
    let c_renderer = renderer.clone();
    let c_asset_manager = asset_manager.clone();
    let c_input = input.clone();
    thread::spawn(move || {
      let mut world = World::default();
      let mut systems = Schedule::builder();
      let mut resources = Resources::default();

      resources.insert(c_input);

      transform::install(&mut systems);
      camera::install(&mut systems);
      c_renderer.install(&mut world, &mut resources, &mut systems);
      crate::spinning_cube::install(&mut world, &mut systems, &c_asset_manager);

      systems.flush();
      let mut schedule = systems.build();
      loop {
        schedule.execute(&mut world, &mut resources);

        thread::sleep(Duration::new(0, 16_000));
      }
    });
  }
}
