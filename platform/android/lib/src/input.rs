use sourcerenderer_core::platform::{Input, Key};
use sourcerenderer_core::{Vec2I, Vec2};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
struct FingerState {
  is_down: bool,
  position: Vec2
}

pub struct InputState {
  fingers: HashMap<u32, FingerState>
}

pub struct AndroidInput {
  state: Mutex<InputState>
}

impl AndroidInput {
  pub fn new() -> Self {
    Self {
      state: Mutex::new(InputState {
        fingers: HashMap::new()
      })
    }
  }

  pub(crate) fn update_finger_down(&self, finger_index: u32, is_down: bool) {
    let mut guard = self.state.lock().unwrap();
    let mut finger = guard.fingers.entry(finger_index).or_default();
    finger.is_down = is_down;
    if !is_down {
      finger.position.x = 0f32;
      finger.position.y = 0f32;
    }
  }

  pub(crate) fn update_finger_position(&self, finger_index: u32, x: f32, y: f32) {
    let mut guard = self.state.lock().unwrap();
    let mut finger = guard.fingers.entry(finger_index).or_insert_with(|| FingerState {
      is_down: true,
      position: Vec2::new(0f32, 0f32)
    });
    finger.position.x = x;
    finger.position.y = y;
  }
}

impl Input for AndroidInput {
  fn is_key_down(&self, key: Key) -> bool {
    false
  }

  fn is_mouse_button_down(&self, button: u8) -> bool {
    false
  }

  fn is_finger_down(&self, finger_index: u32) -> bool {
    let guard = self.state.lock().unwrap();
    guard.fingers.get(&finger_index).map_or(false, |f| f.is_down)
  }

  fn finger_position(&self, finger_index: u32) -> Vec2 {
    let guard = self.state.lock().unwrap();
    guard.fingers.get(&finger_index).map_or(Vec2::new(0.0f32, 0.0f32), |f| f.position)
  }

  fn mouse_position(&self) -> Vec2I {
    Vec2I::new(0, 0)
  }

  fn toggle_mouse_lock(&self, enabled: bool) {}
}