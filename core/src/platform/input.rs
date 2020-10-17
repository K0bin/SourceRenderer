use crate::{Vec2UI, Vec2I};

#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Key {
  W,
  A,
  S,
  D,
  Space,
  LCtrl,
  LShift,
}

pub trait Input: Send + Sync {
  fn is_key_down(&self, key: Key) -> bool;
  fn is_mouse_button_down(&self, button: u8) -> bool;
  fn mouse_position(&self) -> Vec2I;
  fn toggle_mouse_lock(&self, enabled: bool);
}
