use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use sourcerenderer_core::input::Key;
use sourcerenderer_core::platform::Event;
use sourcerenderer_core::{Platform, Vec2, Vec2I};

use crate::bitset_core::BitSet;
use crate::fps_camera::fps_camera_rotation;
use crate::renderer::LateLatching;

pub struct Input {
  state: Mutex<InputState>
}

impl Input {
  pub fn new() -> Self {
    let mut input_state = InputState::default();
    input_state.lock_mouse = true;
    Self {
      state: Mutex::new(input_state)
    }
  }

  pub fn process_input_event<P: Platform>(&self, event: Event<P>, late_latching: Option<&dyn LateLatching<P::GraphicsBackend>>) {
    let mut input_guard = self.state.lock().unwrap();
    match event {
      Event::KeyDown(key) => {
        input_guard.keyboard_keys.bit_set(key as usize);
      }
      Event::KeyUp(key) => {
        input_guard.keyboard_keys.bit_reset(key as usize);
      }
      Event::MouseMoved(position) => {
        input_guard.mouse_pos = position;
      }
      _ => unreachable!()
    }

    if let Some(late_latching) = late_latching {
      late_latching.process_input(&input_guard);
    }
  }

  pub fn poll(&self) -> InputState {
    self.state.lock().unwrap().clone()
  }
}

#[derive(Clone, Default)]
pub struct InputState {
  keyboard_keys: [u32; 4],
  mouse_pos: Vec2I,
  mouse_buttons: u32,
  fingers_down: u32,
  finger_pos: [Vec2; 6],
  lock_mouse: bool
}

impl InputState {
  pub fn new() -> Self {
    Self::default()
  }

  /*pub fn set_mouse_lock(&mut self, is_locked: bool) {
    self.lock_mouse = is_locked;
  }*/

  pub fn mouse_locked(&self) -> bool {
    self.lock_mouse
  }

  /*pub fn set_key_down(&mut self, key: Key, is_down: bool) {
    if is_down {
      self.keyboard_keys.bit_set(key as usize);
    } else {
      self.keyboard_keys.bit_reset(key as usize);
    }
  }
  pub fn set_finger_down(&mut self, finger_index: u32, is_down: bool) {
    if is_down {
      self.fingers_down.bit_set(finger_index as usize);
    } else {
      self.fingers_down.bit_reset(finger_index as usize);
    }
  }
  pub fn set_mouse_button_down(&mut self, mouse_button: u32, is_down: bool) {
    if is_down {
      self.mouse_buttons.bit_set(mouse_button as usize);
    } else {
      self.mouse_buttons.bit_reset(mouse_button as usize);
    }
  }
  pub fn set_mouse_pos(&mut self, position: Vec2I) {
    self.mouse_pos = position;
  }
  pub fn set_finger_position(&mut self, finger_index: u32, position: Vec2) {
    self.finger_pos[finger_index as usize] = position;
  }*/

  pub fn is_key_down(&self, key: Key) -> bool {
    self.keyboard_keys.bit_test(key as usize)
  }
  pub fn is_finger_down(&self, finger_index: u32) -> bool {
    self.fingers_down.bit_test(finger_index as usize)
  }
  pub fn is_mouse_down(&self, mouse_button: u32) -> bool {
    self.mouse_buttons.bit_test(mouse_button as usize)
  }
  pub fn mouse_position(&self) -> Vec2I {
    self.mouse_pos
  }
  pub fn finger_position(&self, finger_index: u32) -> Vec2 {
    self.finger_pos[finger_index as usize]
  }
}
