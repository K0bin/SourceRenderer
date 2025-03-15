use crate::Mutex;

use bevy_ecs::system::Resource;
use sourcerenderer_core::input::Key;
use sourcerenderer_core::platform::Event;
use sourcerenderer_core::{
    Vec2,
    Vec2I,
};

use bitset_core::BitSet;
use crate::graphics::ActiveBackend;

#[allow(dead_code)]
pub struct Input {
    state: Mutex<InputState>,
}

#[allow(dead_code)]
impl Input {
    pub fn new() -> Self {
        let input_state = InputState {
            lock_mouse: true,
            ..Default::default()
        };
        Self {
            state: Mutex::new(input_state),
        }
    }

    pub fn process_input_event(&self, event: Event<ActiveBackend>) {
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
            Event::FingerDown(finger_index) => {
                input_guard.fingers_down |= 1 << finger_index;
            }
            Event::FingerUp(finger_index) => {
                input_guard.fingers_down &= !(1 << finger_index);
            }
            Event::FingerMoved { index, position } => {
                if (index as usize) < input_guard.finger_pos.len() {
                    input_guard.finger_pos[index as usize] = position;
                }
            }
            _ => unreachable!(),
        }
    }

    pub fn poll(&self) -> InputState {
        self.state.lock().unwrap().clone()
    }
}

#[allow(dead_code)]
#[derive(Clone, Default, Resource)]
pub struct InputState {
    keyboard_keys: [u32; 4],
    mouse_pos: Vec2I,
    mouse_buttons: u32,
    fingers_down: u32,
    finger_pos: [Vec2; 6],
    lock_mouse: bool,
}

impl InputState {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /*pub fn set_mouse_lock(&mut self, is_locked: bool) {
      self.lock_mouse = is_locked;
    }*/

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub fn is_key_down(&self, key: Key) -> bool {
        self.keyboard_keys.bit_test(key as usize)
    }
    #[allow(dead_code)]
    pub fn is_finger_down(&self, finger_index: u32) -> bool {
        self.fingers_down.bit_test(finger_index as usize)
    }
    #[allow(dead_code)]
    pub fn is_mouse_down(&self, mouse_button: u32) -> bool {
        self.mouse_buttons.bit_test(mouse_button as usize)
    }
    #[allow(dead_code)]
    pub fn mouse_position(&self) -> Vec2I {
        self.mouse_pos
    }
    #[allow(dead_code)]
    pub fn finger_position(&self, finger_index: u32) -> Vec2 {
        self.finger_pos[finger_index as usize]
    }
}
