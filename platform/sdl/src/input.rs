use std::sync::RwLock;

use sdl2::{
  mouse::MouseState,
  keyboard::KeyboardState,
  EventPump
};


use sourcerenderer_core::platform::{Input, Key, Window, WindowState};
use sourcerenderer_core::{Vec2UI, Vec2I};
use bitset_core::BitSet;
use std::task::Context;
use sdl2::mouse::MouseUtil;
use sdl2::keyboard::Scancode;
use std::collections::HashMap;
use sdl_platform::SDLWindow;

pub struct SDLInput {
  inner: RwLock<SDLInputInner>,
  key_to_scancode: HashMap<Key, Scancode>
}

pub struct SDLInputInner {
  mouse_pos: Vec2I,
  mouse_buttons: u32,
  keyboard_keys: [u32; 8],
  lock_mouse: bool,
  was_mouse_locked: bool
}

impl Input for SDLInput {
  fn is_key_down(&self, key: Key) -> bool {
    let guard = self.inner.read().unwrap();
    guard.keyboard_keys.bit_test(key as usize)
  }
  fn is_mouse_button_down(&self, button: u8) -> bool {
    let guard = self.inner.read().unwrap();
    guard.mouse_buttons.bit_test(button as usize)
  }
  fn mouse_position(&self) -> Vec2I {
    let guard = self.inner.read().unwrap();
    guard.mouse_pos.clone()
  }
  fn toggle_mouse_lock(&self, enabled: bool) {
    let mut guard = self.inner.write().unwrap();
    guard.lock_mouse = enabled;
  }
}

impl SDLInput {
  pub(crate) fn new() -> Self {
    let mut key_to_scancode: HashMap<Key, Scancode> = HashMap::new();
    key_to_scancode.insert(Key::W, Scancode::W);
    key_to_scancode.insert(Key::A, Scancode::A);
    key_to_scancode.insert(Key::S, Scancode::S);
    key_to_scancode.insert(Key::D, Scancode::D);
    key_to_scancode.insert(Key::Space, Scancode::Space);
    key_to_scancode.insert(Key::LShift, Scancode::LShift);
    key_to_scancode.insert(Key::LCtrl, Scancode::LCtrl);

    Self {
      inner: RwLock::new(SDLInputInner {
        mouse_pos: Vec2I::new(0i32, 0i32),
        keyboard_keys: Default::default(),
        mouse_buttons: 0u32,
        lock_mouse: false,
        was_mouse_locked: false
      }),
      key_to_scancode
    }
  }

  pub(crate) fn update(&self, event_pump: &EventPump, mouse_util: &MouseUtil, window: &SDLWindow) {
    let mut guard = self.inner.write().unwrap();
    let window_state = window.state();
    let keyboard_state = event_pump.keyboard_state();
    let mouse_state = event_pump.mouse_state();
    guard.mouse_pos = Vec2I::new(0, 0);

    match window_state {
      WindowState::Visible { width, height, focussed } => {
        guard.mouse_pos = Vec2I::new(mouse_state.x() as i32 - width as i32 / 2, mouse_state.y() as i32 - height as i32 / 2);

        if guard.lock_mouse && focussed {
          mouse_util.warp_mouse_in_window(window.sdl_window_handle(), width as i32 / 2, height as i32 / 2);
          if !guard.was_mouse_locked {
            guard.mouse_pos = Vec2I::new(0, 0);
          }
        }
        guard.was_mouse_locked = guard.lock_mouse;
      },
      _ => {}
    };

    for key_index in 0..7 {
      let key = unsafe { std::mem::transmute(key_index as u32) };
      guard.keyboard_keys.bit_cond(key_index, keyboard_state.is_scancode_pressed(
        *self.key_to_scancode.get(&key).unwrap()
      ));
    }
  }
}