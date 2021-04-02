use legion::{Schedule, World, Resources};
use wasm_bindgen::{prelude::*, closure::Closure, JsCast};
use sourcerenderer_engine::{transform, DeltaTime, TickDelta, TickDuration, TickRate, Tick};
use web_sys::{MessageEvent, DedicatedWorkerGlobalScope};
use std::{cell::RefCell, rc::Rc, time::{SystemTime, Duration}};

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
  last_iter_time: SystemTime,
  last_tick_time: SystemTime,
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
      last_iter_time: SystemTime::now(),
      last_tick_time: SystemTime::now()
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
    let now = SystemTime::now();

    // run fixed step systems first
    let mut tick_delta = now.duration_since(self.last_tick_time).unwrap();
    let tick_duration = self.resources.get::<TickDuration>().unwrap().0.clone();

    while tick_delta >= tick_duration {
      self.last_tick_time += tick_duration;
      self.resources.insert(Tick(self.tick));
      self.fixed_schedule.execute(&mut self.world, &mut self.resources);
      self.tick += 1;
      tick_delta = now.duration_since(self.last_tick_time).unwrap();
    }

    let delta = now.duration_since(self.last_iter_time).unwrap();
    self.last_iter_time = now;
    self.resources.insert(TickDelta(tick_delta));
    self.resources.insert(DeltaTime(delta));
    self.schedule.execute(&mut self.world, &mut self.resources);

    self.frame = frame + 1;
    frame
  }
}