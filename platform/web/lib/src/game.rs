use legion::{Schedule, World, Resources};
use sourcerenderer_engine::transform;
use web_sys::{Window, window};
use serde::{Serialize, Deserialize};
use wasm_bindgen::JsValue;

#[derive(Serialize, Deserialize, Debug)]
pub enum GameMessage {
  FrameSimulated(u64)
}

pub struct Game {
  world: World,
  fixed_schedule: Schedule,
  schedule: Schedule,
  resources: Resources,
  frame: u64
}

impl Game {
  pub fn new() -> Self {
    let mut world = World::default();
    let mut fixed_schedule = Schedule::builder();
    let mut schedule = Schedule::builder();
    let mut resources = Resources::default();

    transform::interpolation::install(&mut fixed_schedule, &mut schedule);
    transform::install(&mut fixed_schedule);

    let fixed_schedule = fixed_schedule.build();
    let schedule = schedule.build();

    Self {
      world,
      fixed_schedule,
      schedule,
      resources,
      frame: 0
    }
  }

  pub fn simulate(&mut self) -> u64 {
    let frame = self.frame;
    window().unwrap().post_message(&JsValue::from_serde(&GameMessage::FrameSimulated(frame)).unwrap(), "*");
    self.frame = frame + 1;
    frame
  }
}
