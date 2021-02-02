use crate::{Vec2UI, Vec2I, Vec2};

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

pub trait Input: Send + Sync {
  fn is_key_down(&self, key: Key) -> bool;
  fn is_mouse_button_down(&self, button: u8) -> bool;
  fn is_finger_down(&self, finger_index: u32) -> bool;
  fn finger_position(&self, finger_index: u32) -> Vec2;
  fn mouse_position(&self) -> Vec2I;
  fn toggle_mouse_lock(&self, enabled: bool);
}
