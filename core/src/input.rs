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
