//! Winit input adapter for retained UI surfaces.

use raf_ui::UiInputState;
use winit::event::{ElementState, Ime, MouseButton, WindowEvent};
use winit::keyboard::Key;

#[derive(Debug, Clone, Default)]
pub struct NativeUiInputBridge {
    state: UiInputState,
}

impl NativeUiInputBridge {
    pub fn begin_frame(&mut self) {
        self.state.pressed_keys.clear();
        self.state.pointer_pressed_buttons.clear();
        self.state.pointer_released_buttons.clear();
        self.state.pointer_delta = [0.0, 0.0];
        self.state.text_input.clear();
    }

    pub fn ingest(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let next = [position.x as f32, position.y as f32];
                if let Some(previous) = self.state.pointer_position {
                    self.state.pointer_delta = [next[0] - previous[0], next[1] - previous[1]];
                }
                self.state.pointer_position = Some(next);
                true
            }
            WindowEvent::CursorLeft { .. } => {
                self.state.pointer_position = None;
                true
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pointer_button = match button {
                    MouseButton::Left => Some(raf_ui::UiPointerButton::Primary),
                    MouseButton::Right => Some(raf_ui::UiPointerButton::Secondary),
                    MouseButton::Middle => Some(raf_ui::UiPointerButton::Middle),
                    _ => None,
                };
                let Some(pointer_button) = pointer_button else {
                    return false;
                };
                match state {
                    ElementState::Pressed => {
                        if !self.state.pointer_buttons_down.contains(&pointer_button) {
                            self.state.pointer_buttons_down.push(pointer_button);
                        }
                        self.state.pointer_pressed_buttons.push(pointer_button);
                    }
                    ElementState::Released => {
                        self.state
                            .pointer_buttons_down
                            .retain(|button| *button != pointer_button);
                        self.state.pointer_released_buttons.push(pointer_button);
                    }
                }
                self.state.pointer_down = self
                    .state
                    .pointer_buttons_down
                    .contains(&raf_ui::UiPointerButton::Primary);
                true
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let key = match &event.logical_key {
                    Key::Character(value) => value.to_string(),
                    Key::Named(value) => format!("{value:?}"),
                    _ => format!("{:?}", event.logical_key),
                };
                if !key.is_empty() {
                    self.state.pressed_keys.push(key);
                }
                if let Some(text) = event.text.as_deref() {
                    self.state.text_input.push_str(text);
                }
                true
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.state.text_input.push_str(text);
                true
            }
            _ => false,
        }
    }

    pub fn state(&self) -> &UiInputState {
        &self.state
    }
}
