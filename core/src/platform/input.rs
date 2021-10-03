use crate::{Vec2I, Vec2};
use bitset_core::BitSet;

#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Key {
  W,
  A,
  S,
  D,
  Q,
  E,
  Space,
  LCtrl,
  LShift,
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

  pub fn set_mouse_lock(&mut self, is_locked: bool) {
    self.lock_mouse = is_locked;
  }

  pub fn mouse_locked(&self) -> bool {
    self.lock_mouse
  }

  pub fn set_key_down(&mut self, key: Key, is_down: bool) {
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
  }

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

#[derive(Default)]
pub struct InputCommands {
  lock_mouse: bool
}

impl InputCommands {
  pub fn new() -> Self {
    Self {
      lock_mouse: false
    }
  }

  pub fn should_lock_mouse(&self) -> bool {
    self.lock_mouse
  }
  pub fn lock_mouse(&mut self, lock: bool) {
    self.lock_mouse = lock;
  }
}

pub trait Input: Send + Sync {
  fn is_key_down(&self, key: Key) -> bool;
  fn is_mouse_button_down(&self, button: u8) -> bool;
  fn is_finger_down(&self, finger_index: u32) -> bool;
  fn finger_position(&self, finger_index: u32) -> Vec2;
  fn mouse_position(&self) -> Vec2I;
  fn toggle_mouse_lock(&self, enabled: bool);
}
