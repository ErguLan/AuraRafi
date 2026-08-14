//! Winit input adapter for retained UI surfaces.

use raf_ui::UiInputState;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
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
        self.state.scroll_delta = [0.0, 0.0];
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
            WindowEvent::Focused(false) => {
                // A window can lose focus without emitting button-up events.
                // Clearing capture here prevents a drag or text key from
                // remaining logically active after an Alt+Tab or modal dialog.
                self.state.pointer_buttons_down.clear();
                self.state.pointer_pressed_buttons.clear();
                self.state.pointer_released_buttons.clear();
                self.state.pointer_down = false;
                self.state.modifiers = raf_ui::UiModifiers::default();
                self.state.pressed_keys.clear();
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
            WindowEvent::MouseWheel { delta, .. } => {
                let delta = match delta {
                    MouseScrollDelta::LineDelta(x, y) => [-x * 24.0, -y * 24.0],
                    MouseScrollDelta::PixelDelta(delta) => [-delta.x as f32, -delta.y as f32],
                };
                self.state.scroll_delta[0] += delta[0];
                self.state.scroll_delta[1] += delta[1];
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
            WindowEvent::ModifiersChanged(modifiers) => {
                let modifiers = modifiers.state();
                self.state.modifiers = raf_ui::UiModifiers {
                    shift: modifiers.shift_key(),
                    control: modifiers.control_key(),
                    alt: modifiers.alt_key(),
                    command: modifiers.super_key(),
                };
                true
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.state.text_input.push_str(text);
                self.state.ime_preedit.clear();
                true
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                self.state.ime_preedit = text.clone();
                true
            }
            WindowEvent::Ime(Ime::Enabled) | WindowEvent::Ime(Ime::Disabled) => {
                self.state.ime_preedit.clear();
                true
            }
            _ => false,
        }
    }

    pub fn state(&self) -> &UiInputState {
        &self.state
    }

    pub fn set_time_seconds(&mut self, time_seconds: f64) {
        self.state.time_seconds = time_seconds.max(0.0);
    }
}
