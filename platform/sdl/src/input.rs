use std::sync::RwLock;
use std::collections::HashMap;

use bitset_core::BitSet;

use sdl2::EventPump;
use sdl2::mouse::MouseUtil;
use sdl2::keyboard::{Scancode, KeyboardState};

use sourcerenderer_core::platform::{Input, Key, Window, WindowState, InputState, InputCommands};
use sourcerenderer_core::{Vec2I, Vec2};

use crate::sdl_platform::SDLWindow;

lazy_static! {
static ref KEY_TO_SCANCODE: HashMap<Key, Scancode> = {
      let mut key_to_scancode: HashMap<Key, Scancode> = HashMap::new();
      key_to_scancode.insert(Key::W, Scancode::W);
      key_to_scancode.insert(Key::A, Scancode::A);
      key_to_scancode.insert(Key::S, Scancode::S);
      key_to_scancode.insert(Key::D, Scancode::D);
      key_to_scancode.insert(Key::Q, Scancode::Q);
      key_to_scancode.insert(Key::E, Scancode::E);
      key_to_scancode.insert(Key::Space, Scancode::Space);
      key_to_scancode.insert(Key::LShift, Scancode::LShift);
      key_to_scancode.insert(Key::LCtrl, Scancode::LCtrl);
      key_to_scancode
    };
}

pub fn process(previous_commands: &mut InputCommands, commands: InputCommands, event_pump: &EventPump, mouse_util: &MouseUtil, window: &SDLWindow) -> InputState {
  let window_state = window.state();
  let (has_focus, width, height) = match &window_state {
    WindowState::Visible { focussed, width, height } => (*focussed, *width, *height),
    WindowState::FullScreen { width, height } => (true, *width, *height),
    _ => (false, 0, 0)
  };

  let mut input_state = InputState::new();
  let mouse_state = event_pump.mouse_state();
  if commands.should_lock_mouse() {
    if has_focus {
      mouse_util.warp_mouse_in_window(window.sdl_window_handle(), width as i32 / 2, height as i32 / 2);
    }

    if !previous_commands.should_lock_mouse() || !has_focus {
      input_state.set_mouse_pos(Vec2I::new(0, 0));
    } else {
      input_state.set_mouse_pos(Vec2I::new(mouse_state.x() - (width as i32) / 2, mouse_state.y() - (height as i32) / 2));
    }
  } else {
    input_state.set_mouse_pos(Vec2I::new(mouse_state.x(), mouse_state.y()));
  }

  let keyboard_state = event_pump.keyboard_state();
  for key_index in 0..7 {
    let key = unsafe { std::mem::transmute(key_index as u32) };
    input_state.set_key_down(key, keyboard_state.is_scancode_pressed(
      *KEY_TO_SCANCODE.get(&key).unwrap()
    ));
  }

  *previous_commands = commands;

  input_state
}
