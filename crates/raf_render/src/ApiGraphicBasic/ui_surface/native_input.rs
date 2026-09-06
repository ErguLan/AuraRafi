//! Winit adapter for the shared backend-neutral input snapshot.

use arboard::Clipboard;
use raf_core::{
    InputKey, InputModifiers, InputOwner, InputRouter, InputSnapshot,
    PointerButton as RafiPointerButton,
};
use raf_ui::{UiInputState, UiModifiers, UiPointerButton, UiRect};
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

#[derive(Debug, Clone)]
pub struct NativeUiInputBridge {
    snapshot: InputSnapshot,
    scale_factor: f64,
    clipboard_paste: Option<String>,
    /// Text-editing key presses are kept as events in addition to the bitset
    /// snapshot. Winit can deliver two quick press/release cycles before the
    /// next redraw; a bitset would collapse them into one `Delete` press.
    pending_text_key_presses: Vec<InputKey>,
}

impl Default for NativeUiInputBridge {
    fn default() -> Self {
        Self {
            snapshot: InputSnapshot::default(),
            scale_factor: 1.0,
            clipboard_paste: None,
            pending_text_key_presses: Vec::new(),
        }
    }
}

impl NativeUiInputBridge {
    pub fn begin_frame(&mut self) {
        self.snapshot.clear_transient();
        self.clipboard_paste = None;
        self.pending_text_key_presses.clear();
    }

    pub fn ingest(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = scale_factor.max(0.25);
                true
            }
            WindowEvent::CursorMoved { position, .. } => {
                let next = [
                    (position.x / self.scale_factor) as f32,
                    (position.y / self.scale_factor) as f32,
                ];
                if let Some(previous) = self.snapshot.pointer_position {
                    self.snapshot.pointer_delta[0] += next[0] - previous[0];
                    self.snapshot.pointer_delta[1] += next[1] - previous[1];
                }
                self.snapshot.pointer_position = Some(next);
                true
            }
            WindowEvent::CursorLeft { .. } => {
                self.snapshot.pointer_position = None;
                true
            }
            WindowEvent::Focused(focused) => {
                self.snapshot.window_focused = *focused;
                if !focused {
                    self.snapshot.cancel_all();
                    self.pending_text_key_presses.clear();
                }
                true
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(button) = pointer_button(*button) else {
                    return false;
                };
                match state {
                    ElementState::Pressed => {
                        if !self.snapshot.pointer_buttons_down.contains(button) {
                            self.snapshot.pointer_pressed_buttons.insert(button);
                        }
                        self.snapshot.pointer_buttons_down.insert(button);
                    }
                    ElementState::Released => {
                        self.snapshot.pointer_buttons_down.remove(button);
                        self.snapshot.pointer_released_buttons.insert(button);
                    }
                }
                true
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let delta = match delta {
                    // Keep the platform snapshot convention here. The RafUI
                    // projection below translates it to retained-offset
                    // direction without changing the viewport input path.
                    MouseScrollDelta::LineDelta(x, y) => [x * 24.0, y * 24.0],
                    MouseScrollDelta::PixelDelta(delta) => [
                        (delta.x / self.scale_factor) as f32,
                        (delta.y / self.scale_factor) as f32,
                    ],
                };
                self.snapshot.scroll_delta[0] += delta[0];
                self.snapshot.scroll_delta[1] += delta[1];
                true
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(key) = event_key(event.physical_key, &event.logical_key) {
                    match event.state {
                        ElementState::Pressed => {
                            if is_text_edit_key(key) {
                                self.pending_text_key_presses.push(key);
                            }
                            if !self.snapshot.keys_down.contains(key) {
                                self.snapshot.keys_pressed.insert(key);
                            }
                            self.snapshot.keys_down.insert(key);
                            if self.snapshot.modifiers.command_modifier()
                                && matches!(key, InputKey::V)
                            {
                                self.clipboard_paste = Clipboard::new()
                                    .and_then(|mut clipboard| clipboard.get_text())
                                    .ok();
                            }
                        }
                        ElementState::Released => {
                            self.snapshot.keys_down.remove(key);
                            self.snapshot.keys_released.insert(key);
                        }
                    }
                }
                if event.state == ElementState::Pressed {
                    if let Some(text) = event.text.as_deref() {
                        if !self.snapshot.modifiers.command_modifier()
                            && !self.snapshot.modifiers.alt
                        {
                            self.snapshot.text_input.push_str(text);
                        }
                    }
                }
                true
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let modifiers = modifiers.state();
                self.snapshot.modifiers = InputModifiers {
                    shift: modifiers.shift_key(),
                    control: modifiers.control_key(),
                    alt: modifiers.alt_key(),
                    command: modifiers.super_key(),
                };
                true
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.snapshot.text_input.push_str(text);
                self.snapshot.ime_preedit.clear();
                true
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                self.snapshot.ime_preedit.clone_from(text);
                true
            }
            WindowEvent::Ime(Ime::Enabled) | WindowEvent::Ime(Ime::Disabled) => {
                self.snapshot.ime_preedit.clear();
                true
            }
            _ => false,
        }
    }

    pub fn snapshot(&self) -> &InputSnapshot {
        &self.snapshot
    }

    pub fn snapshot_mut(&mut self) -> &mut InputSnapshot {
        &mut self.snapshot
    }

    pub fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    pub fn set_scale_factor(&mut self, scale_factor: f64) {
        self.scale_factor = scale_factor.max(0.25);
    }

    pub fn set_time_seconds(&mut self, time_seconds: f64) {
        let next = time_seconds.max(0.0);
        self.snapshot.delta_seconds = (next - self.snapshot.time_seconds).clamp(0.0, 0.25) as f32;
        self.snapshot.time_seconds = next;
    }

    /// Projects the shared snapshot into one retained surface's logical
    /// coordinate system. A captured drag keeps receiving movement outside
    /// the original rectangle; an uncaptured outside press is click-away only.
    pub fn ui_state_for_owner(
        &self,
        rect: UiRect,
        owner: InputOwner,
        router: &InputRouter,
    ) -> UiInputState {
        let pointer_inside = self
            .snapshot
            .pointer_position
            .is_some_and(|point| rect.contains(point));
        let retained_pointer_capture = pointer_pairs()
            .iter()
            .any(|(button, _)| router.is_pointer_owned_by(*button, owner));
        let pointer_local = self.snapshot.pointer_position.and_then(|point| {
            (pointer_inside || retained_pointer_capture)
                .then_some([point[0] - rect.x, point[1] - rect.y])
        });
        let any_press = !self.snapshot.pointer_pressed_buttons.is_empty();
        let receives_pointer = pointer_inside || retained_pointer_capture;

        let mut down = Vec::with_capacity(3);
        let mut pressed = Vec::with_capacity(3);
        let mut released = Vec::with_capacity(3);
        if receives_pointer {
            for (rafi, ui) in pointer_pairs() {
                let routed_here = router
                    .pointer_owner(rafi)
                    .map_or(true, |current| current == owner);
                if routed_here && self.snapshot.pointer_buttons_down.contains(rafi) {
                    down.push(ui);
                }
                if routed_here && self.snapshot.pointer_pressed_buttons.contains(rafi) {
                    pressed.push(ui);
                }
                if routed_here && self.snapshot.pointer_released_buttons.contains(rafi) {
                    released.push(ui);
                }
            }
        }

        let receives_keyboard = router.keyboard_owner() == Some(owner);

        UiInputState {
            pointer_position: pointer_local,
            pointer_delta: if receives_pointer {
                self.snapshot.pointer_delta
            } else {
                [0.0; 2]
            },
            time_seconds: self.snapshot.time_seconds,
            // The native snapshot follows the platform convention where
            // positive wheel Y means wheel-up. RafUI receives a retained
            // offset delta instead, so wheel-down must increase that offset.
            scroll_delta: if pointer_inside || retained_pointer_capture {
                [
                    -self.snapshot.scroll_delta[0],
                    -self.snapshot.scroll_delta[1],
                ]
            } else {
                [0.0; 2]
            },
            pointer_down: down.contains(&UiPointerButton::Primary),
            pointer_buttons_down: down,
            pointer_pressed_buttons: pressed,
            pointer_released_buttons: released,
            pointer_pressed_outside: any_press && !pointer_inside && !retained_pointer_capture,
            pressed_keys: if receives_keyboard {
                ui_pressed_keys(&self.snapshot, &self.pending_text_key_presses)
            } else {
                Vec::new()
            },
            keys_down: if receives_keyboard {
                ui_down_keys(&self.snapshot)
            } else {
                Vec::new()
            },
            text_input: if receives_keyboard {
                self.snapshot.text_input.clone()
            } else {
                String::new()
            },
            clipboard_text: if receives_keyboard {
                self.clipboard_paste.clone()
            } else {
                None
            },
            ime_preedit: if receives_keyboard {
                self.snapshot.ime_preedit.clone()
            } else {
                String::new()
            },
            modifiers: UiModifiers {
                shift: self.snapshot.modifiers.shift,
                control: self.snapshot.modifiers.control,
                alt: self.snapshot.modifiers.alt,
                command: self.snapshot.modifiers.command,
            },
        }
    }

    pub fn write_clipboard(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Ok(mut clipboard) = Clipboard::new() {
            let _ = clipboard.set_text(text.to_string());
        }
    }
}

fn pointer_button(button: MouseButton) -> Option<RafiPointerButton> {
    match button {
        MouseButton::Left => Some(RafiPointerButton::Primary),
        MouseButton::Right => Some(RafiPointerButton::Secondary),
        MouseButton::Middle => Some(RafiPointerButton::Middle),
        MouseButton::Back => Some(RafiPointerButton::Back),
        MouseButton::Forward => Some(RafiPointerButton::Forward),
        MouseButton::Other(_) => None,
    }
}

fn pointer_pairs() -> [(RafiPointerButton, UiPointerButton); 3] {
    [
        (RafiPointerButton::Primary, UiPointerButton::Primary),
        (RafiPointerButton::Secondary, UiPointerButton::Secondary),
        (RafiPointerButton::Middle, UiPointerButton::Middle),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::{CaptureMode, InputRegionId, InputRouter, PointerButton};

    #[test]
    fn native_wheel_down_becomes_a_positive_retained_scroll_offset() {
        let mut bridge = NativeUiInputBridge::default();
        bridge.snapshot_mut().pointer_position = Some([40.0, 40.0]);
        bridge.snapshot_mut().scroll_delta = [0.0, -48.0];
        let owner = InputOwner::RetainedUi(InputRegionId::from_static("test.ui"));
        let state = bridge.ui_state_for_owner(
            UiRect::new(0.0, 0.0, 240.0, 120.0),
            owner,
            &InputRouter::default(),
        );

        assert_eq!(state.scroll_delta, [0.0, 48.0]);
    }

    #[test]
    fn captured_surface_keeps_receiving_wheel_delta_after_pointer_leaves() {
        let mut bridge = NativeUiInputBridge::default();
        bridge.snapshot_mut().pointer_position = Some([360.0, 180.0]);
        bridge.snapshot_mut().scroll_delta = [0.0, -24.0];
        let owner = InputOwner::RetainedUi(InputRegionId::from_static("test.ui"));
        let mut router = InputRouter::default();
        assert!(router.try_capture_pointer(
            PointerButton::Primary,
            owner,
            CaptureMode::PerButton,
            [20.0, 20.0],
            1.0,
        ));

        let state = bridge.ui_state_for_owner(UiRect::new(0.0, 0.0, 240.0, 120.0), owner, &router);

        assert_eq!(state.scroll_delta, [0.0, 24.0]);
    }

    #[test]
    fn native_text_key_edges_preserve_fast_delete_taps_until_the_frame_is_consumed() {
        let mut bridge = NativeUiInputBridge::default();
        bridge.snapshot_mut().keys_pressed.insert(InputKey::Delete);
        bridge.snapshot_mut().keys_down.insert(InputKey::Delete);
        bridge.pending_text_key_presses = vec![InputKey::Delete, InputKey::Delete];

        let owner = InputOwner::RetainedUi(InputRegionId::from_static("test.text"));
        let mut router = InputRouter::default();
        assert!(router.try_capture_keyboard(owner));
        let state = bridge.ui_state_for_owner(UiRect::new(0.0, 0.0, 240.0, 120.0), owner, &router);

        assert_eq!(state.key_press_count("delete"), 2);
        bridge.begin_frame();
        assert!(bridge.pending_text_key_presses.is_empty());
        assert!(!bridge.snapshot().key_pressed(InputKey::Delete));
        assert!(bridge.snapshot().key_down(InputKey::Delete));
    }
}

fn event_key(physical: PhysicalKey, logical: &Key) -> Option<InputKey> {
    match physical {
        PhysicalKey::Code(code) => key_code(code),
        PhysicalKey::Unidentified(_) => logical_key(logical),
    }
}

fn logical_key(key: &Key) -> Option<InputKey> {
    match key {
        Key::Named(NamedKey::Escape) => Some(InputKey::Escape),
        Key::Named(NamedKey::Enter) => Some(InputKey::Enter),
        Key::Named(NamedKey::Tab) => Some(InputKey::Tab),
        Key::Named(NamedKey::Space) => Some(InputKey::Space),
        Key::Named(NamedKey::Backspace) => Some(InputKey::Backspace),
        Key::Named(NamedKey::Delete) => Some(InputKey::Delete),
        Key::Named(NamedKey::ArrowUp) => Some(InputKey::ArrowUp),
        Key::Named(NamedKey::ArrowDown) => Some(InputKey::ArrowDown),
        Key::Named(NamedKey::ArrowLeft) => Some(InputKey::ArrowLeft),
        Key::Named(NamedKey::ArrowRight) => Some(InputKey::ArrowRight),
        Key::Character(value) => match value.to_ascii_lowercase().as_str() {
            "a" => Some(InputKey::A),
            "c" => Some(InputKey::C),
            "d" => Some(InputKey::D),
            "e" => Some(InputKey::E),
            "f" => Some(InputKey::F),
            "g" => Some(InputKey::G),
            "k" => Some(InputKey::K),
            "q" => Some(InputKey::Q),
            "r" => Some(InputKey::R),
            "s" => Some(InputKey::S),
            "t" => Some(InputKey::T),
            "v" => Some(InputKey::V),
            "w" => Some(InputKey::W),
            "y" => Some(InputKey::Y),
            "z" => Some(InputKey::Z),
            "0" => Some(InputKey::Digit0),
            "1" => Some(InputKey::Digit1),
            "2" => Some(InputKey::Digit2),
            "3" => Some(InputKey::Digit3),
            _ => None,
        },
        _ => None,
    }
}

fn key_code(code: KeyCode) -> Option<InputKey> {
    Some(match code {
        KeyCode::KeyA => InputKey::A,
        KeyCode::KeyB => InputKey::B,
        KeyCode::KeyC => InputKey::C,
        KeyCode::KeyD => InputKey::D,
        KeyCode::KeyE => InputKey::E,
        KeyCode::KeyF => InputKey::F,
        KeyCode::KeyG => InputKey::G,
        KeyCode::KeyH => InputKey::H,
        KeyCode::KeyI => InputKey::I,
        KeyCode::KeyJ => InputKey::J,
        KeyCode::KeyK => InputKey::K,
        KeyCode::KeyL => InputKey::L,
        KeyCode::KeyM => InputKey::M,
        KeyCode::KeyN => InputKey::N,
        KeyCode::KeyO => InputKey::O,
        KeyCode::KeyP => InputKey::P,
        KeyCode::KeyQ => InputKey::Q,
        KeyCode::KeyR => InputKey::R,
        KeyCode::KeyS => InputKey::S,
        KeyCode::KeyT => InputKey::T,
        KeyCode::KeyU => InputKey::U,
        KeyCode::KeyV => InputKey::V,
        KeyCode::KeyW => InputKey::W,
        KeyCode::KeyX => InputKey::X,
        KeyCode::KeyY => InputKey::Y,
        KeyCode::KeyZ => InputKey::Z,
        KeyCode::Digit0 => InputKey::Digit0,
        KeyCode::Digit1 => InputKey::Digit1,
        KeyCode::Digit2 => InputKey::Digit2,
        KeyCode::Digit3 => InputKey::Digit3,
        KeyCode::Digit4 => InputKey::Digit4,
        KeyCode::Digit5 => InputKey::Digit5,
        KeyCode::Digit6 => InputKey::Digit6,
        KeyCode::Digit7 => InputKey::Digit7,
        KeyCode::Digit8 => InputKey::Digit8,
        KeyCode::Digit9 => InputKey::Digit9,
        KeyCode::Escape => InputKey::Escape,
        KeyCode::Enter => InputKey::Enter,
        KeyCode::Tab => InputKey::Tab,
        KeyCode::Space => InputKey::Space,
        KeyCode::Backspace => InputKey::Backspace,
        KeyCode::Delete => InputKey::Delete,
        KeyCode::Insert => InputKey::Insert,
        KeyCode::Home => InputKey::Home,
        KeyCode::End => InputKey::End,
        KeyCode::PageUp => InputKey::PageUp,
        KeyCode::PageDown => InputKey::PageDown,
        KeyCode::ArrowUp => InputKey::ArrowUp,
        KeyCode::ArrowDown => InputKey::ArrowDown,
        KeyCode::ArrowLeft => InputKey::ArrowLeft,
        KeyCode::ArrowRight => InputKey::ArrowRight,
        KeyCode::F1 => InputKey::F1,
        KeyCode::F2 => InputKey::F2,
        KeyCode::F3 => InputKey::F3,
        KeyCode::F4 => InputKey::F4,
        KeyCode::F5 => InputKey::F5,
        KeyCode::F6 => InputKey::F6,
        KeyCode::F7 => InputKey::F7,
        KeyCode::F8 => InputKey::F8,
        KeyCode::F9 => InputKey::F9,
        KeyCode::F10 => InputKey::F10,
        KeyCode::F11 => InputKey::F11,
        KeyCode::F12 => InputKey::F12,
        KeyCode::Minus => InputKey::Minus,
        KeyCode::Equal => InputKey::Equal,
        KeyCode::BracketLeft => InputKey::BracketLeft,
        KeyCode::BracketRight => InputKey::BracketRight,
        KeyCode::Backslash => InputKey::Backslash,
        KeyCode::Semicolon => InputKey::Semicolon,
        KeyCode::Quote => InputKey::Quote,
        KeyCode::Backquote => InputKey::Backquote,
        KeyCode::Comma => InputKey::Comma,
        KeyCode::Period => InputKey::Period,
        KeyCode::Slash => InputKey::Slash,
        KeyCode::Numpad0 => InputKey::Numpad0,
        KeyCode::Numpad1 => InputKey::Numpad1,
        KeyCode::Numpad2 => InputKey::Numpad2,
        KeyCode::Numpad3 => InputKey::Numpad3,
        KeyCode::Numpad4 => InputKey::Numpad4,
        KeyCode::Numpad5 => InputKey::Numpad5,
        KeyCode::Numpad6 => InputKey::Numpad6,
        KeyCode::Numpad7 => InputKey::Numpad7,
        KeyCode::Numpad8 => InputKey::Numpad8,
        KeyCode::Numpad9 => InputKey::Numpad9,
        KeyCode::NumpadAdd => InputKey::NumpadAdd,
        KeyCode::NumpadSubtract => InputKey::NumpadSubtract,
        KeyCode::NumpadMultiply => InputKey::NumpadMultiply,
        KeyCode::NumpadDivide => InputKey::NumpadDivide,
        KeyCode::NumpadDecimal => InputKey::NumpadDecimal,
        _ => return None,
    })
}

fn ui_pressed_keys(snapshot: &InputSnapshot, pending_text_key_presses: &[InputKey]) -> Vec<String> {
    const UI_KEYS: &[(InputKey, &str)] = &[
        (InputKey::A, "a"),
        (InputKey::C, "c"),
        (InputKey::V, "v"),
        (InputKey::X, "x"),
        (InputKey::Z, "z"),
        (InputKey::Escape, "escape"),
        (InputKey::Enter, "enter"),
        (InputKey::Space, "space"),
        (InputKey::Tab, "tab"),
        (InputKey::Backspace, "backspace"),
        (InputKey::Delete, "delete"),
        (InputKey::Home, "home"),
        (InputKey::End, "end"),
        (InputKey::ArrowUp, "arrowup"),
        (InputKey::ArrowDown, "arrowdown"),
        (InputKey::ArrowLeft, "arrowleft"),
        (InputKey::ArrowRight, "arrowright"),
    ];
    let mut keys = Vec::with_capacity(4);

    // Preserve every text-editing edge that arrived since the last consumed
    // frame. The semantic set below remains the fallback for callers that
    // populate `InputSnapshot` directly (for example headless embedders).
    for key in pending_text_key_presses {
        if let Some((_, name)) = UI_KEYS.iter().find(|(candidate, _)| candidate == key) {
            keys.push((*name).to_string());
        }
    }
    for (key, name) in UI_KEYS {
        if snapshot.key_pressed(*key)
            && (!is_text_edit_key(*key) || !pending_text_key_presses.contains(key))
        {
            keys.push((*name).to_string());
        }
    }
    keys
}

fn is_text_edit_key(key: InputKey) -> bool {
    matches!(
        key,
        InputKey::Backspace
            | InputKey::Delete
            | InputKey::Home
            | InputKey::End
            | InputKey::ArrowUp
            | InputKey::ArrowDown
            | InputKey::ArrowLeft
            | InputKey::ArrowRight
    )
}

fn ui_down_keys(snapshot: &InputSnapshot) -> Vec<String> {
    const UI_KEYS: &[(InputKey, &str)] = &[
        (InputKey::Backspace, "backspace"),
        (InputKey::Delete, "delete"),
        (InputKey::ArrowUp, "arrowup"),
        (InputKey::ArrowDown, "arrowdown"),
        (InputKey::ArrowLeft, "arrowleft"),
        (InputKey::ArrowRight, "arrowright"),
        (InputKey::Home, "home"),
        (InputKey::End, "end"),
    ];
    UI_KEYS
        .iter()
        .filter(|(key, _)| snapshot.keys_down.contains(*key))
        .map(|(_, name)| (*name).to_string())
        .collect()
}
