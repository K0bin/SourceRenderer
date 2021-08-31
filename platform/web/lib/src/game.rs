use legion::{Schedule, World, Resources};
use wasm_bindgen::{prelude::*, closure::Closure, JsCast};
use sourcerenderer_engine::{transform, DeltaTime, TickDelta, TickDuration, TickRate, Tick};
use web_sys::{MessageEvent, DedicatedWorkerGlobalScope};
use js_sys::Date;
use std::{cell::RefCell, rc::Rc, time::{SystemTime, Duration}};
use crate::console_log;

#[derive(Serialize, Deserialize, Debug)]
pub struct SimulateFrameMessage {
}

#[wasm_bindgen]
pub struct Game {
  _inner: Rc<RefCell<GameInner>>,
  message_callback: Closure<dyn FnMut(MessageEvent)>
}

pub struct GameInner {
  world: World,
  fixed_schedule: Schedule,
  schedule: Schedule,
  resources: Resources,
  last_iter_time: Date,
  last_tick_time: Date,
  tick: u64,
  frame: u64
}

impl Game {
  pub fn run(tick_rate: u32) -> Self {
    let world = World::default();
    let mut fixed_schedule = Schedule::builder();
    let mut schedule = Schedule::builder();
    let mut resources = Resources::default();

    transform::interpolation::install(&mut fixed_schedule, &mut schedule);
    transform::install(&mut fixed_schedule);

    let tick_duration = Duration::new(0, 1_000_000_000 / tick_rate);
    resources.insert(TickRate(tick_rate));
    resources.insert(TickDuration(tick_duration));

    let fixed_schedule = fixed_schedule.build();
    let schedule = schedule.build();

    let inner = Rc::new(RefCell::new(GameInner {
      world,
      fixed_schedule,
      schedule,
      resources,
      frame: 0,
      tick: 0,
      last_iter_time: Date::new_0(),
      last_tick_time: Date::new_0()
    }));

    let c_inner = inner.clone();
    let message_callback = Closure::<dyn FnMut(MessageEvent)>::wrap(Box::new(move |event: MessageEvent| {
      let message = event.data().into_serde::<SimulateFrameMessage>().unwrap();
      let mut game_mut = c_inner.borrow_mut();
      game_mut.simulate(message);
    }));

    let game = Self {
      _inner: inner,
      message_callback
    };

    let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
    global.set_onmessage(Some(game.message_callback.as_ref().unchecked_ref()));

    game
  }
}

impl GameInner {
  fn simulate(&mut self, _message: SimulateFrameMessage) -> u64 {
    let frame = self.frame;
    let now = Date::new_0();

    // run fixed step systems first
    let tick_delta_ms = now.get_time() - self.last_tick_time.get_time();
    let tick_delta = Duration::new((tick_delta_ms / 1_000f64) as u64, (((tick_delta_ms * 1_000_000f64) as u64) % 1000_000_000u64) as u32);
    let tick_duration = self.resources.get::<TickDuration>().unwrap().0.clone();

    while tick_delta >= tick_duration {
      self.last_tick_time.set_time(self.last_tick_time.get_time() + tick_duration.as_millis() as f64);
      self.resources.insert(Tick(self.tick));
      self.fixed_schedule.execute(&mut self.world, &mut self.resources);
      self.tick += 1;
      let tick_delta_ms = now.get_time() - self.last_tick_time.get_time();
      let tick_delta = Duration::new((tick_delta_ms / 1_000f64) as u64, (((tick_delta_ms * 1_000_000f64) as u64) % 1000_000_000u64) as u32);
    }

    let delta_ms = now.get_time() - self.last_iter_time.get_time();
    let delta_nanos = delta_ms * 1_000_000f64;
    let delta = Duration::new((delta_ms / 1_000f64) as u64, (((delta_ms * 1_000_000f64) as u64) % 1000_000_000u64) as u32);
    self.last_iter_time = now;
    self.resources.insert(TickDelta(tick_delta));
    self.resources.insert(DeltaTime(delta));
    self.schedule.execute(&mut self.world, &mut self.resources);

    self.frame = frame + 1;
    frame
  }
}