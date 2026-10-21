//! Window-independent conversion from native input to script input.
use raf_core::{InputKey, InputSnapshot};
use raf_script::InputSnapshot as ScriptInputSnapshot;
#[derive(Debug, Clone, Default)]
pub struct RuntimeInputState {
    pub snapshot: ScriptInputSnapshot,
}

impl RuntimeInputState {
    pub fn from_input(input: &InputSnapshot) -> Self {
        let mut snapshot = ScriptInputSnapshot::default();
        for (key, label) in SCRIPT_KEYS {
            if input.key_down(*key) {
                snapshot.keys_held.push((*label).to_string());
            }
            if input.key_pressed(*key) {
                snapshot.keys_pressed.push((*label).to_string());
            }
            if input.key_released(*key) {
                snapshot.keys_released.push((*label).to_string());
            }
        }
        for (enabled, pressed, released, label) in [
            (
                input.modifiers.control,
                input.modifiers_pressed.control,
                input.modifiers_released.control,
                "ctrl",
            ),
            (
                input.modifiers.shift,
                input.modifiers_pressed.shift,
                input.modifiers_released.shift,
                "shift",
            ),
            (
                input.modifiers.alt,
                input.modifiers_pressed.alt,
                input.modifiers_released.alt,
                "alt",
            ),
            (
                input.modifiers.command,
                input.modifiers_pressed.command,
                input.modifiers_released.command,
                "cmd",
            ),
        ] {
            if enabled {
                snapshot.keys_held.push(label.to_string());
            }
            if pressed {
                snapshot.keys_pressed.push(label.into());
            }
            if released {
                snapshot.keys_released.push(label.into());
            }
        }
        for (button, label) in [
            (raf_core::PointerButton::Primary, 0),
            (raf_core::PointerButton::Secondary, 1),
            (raf_core::PointerButton::Middle, 2),
            (raf_core::PointerButton::Back, 3),
            (raf_core::PointerButton::Forward, 4),
        ] {
            if input.button_down(button) {
                snapshot.mouse_held.push(label);
            }
            if input.button_pressed(button) {
                snapshot.mouse_pressed.push(label);
            }
            if input.button_released(button) {
                snapshot.mouse_released.push(label);
            }
        }
        snapshot.mouse_position = input.pointer_position;
        snapshot.mouse_delta = input.pointer_delta;
        snapshot.mouse_relative = input.relative_pointer_delta;
        snapshot.mouse_wheel = input.scroll_delta;
        if !input.window_focused {
            snapshot = ScriptInputSnapshot::default();
        }
        Self { snapshot }
    }
}

const SCRIPT_KEYS: &[(InputKey, &str)] = &[
    (InputKey::A, "a"),
    (InputKey::B, "b"),
    (InputKey::C, "c"),
    (InputKey::D, "d"),
    (InputKey::E, "e"),
    (InputKey::F, "f"),
    (InputKey::G, "g"),
    (InputKey::H, "h"),
    (InputKey::I, "i"),
    (InputKey::J, "j"),
    (InputKey::K, "k"),
    (InputKey::L, "l"),
    (InputKey::M, "m"),
    (InputKey::N, "n"),
    (InputKey::O, "o"),
    (InputKey::P, "p"),
    (InputKey::Q, "q"),
    (InputKey::R, "r"),
    (InputKey::S, "s"),
    (InputKey::T, "t"),
    (InputKey::U, "u"),
    (InputKey::V, "v"),
    (InputKey::W, "w"),
    (InputKey::X, "x"),
    (InputKey::Y, "y"),
    (InputKey::Z, "z"),
    (InputKey::ArrowUp, "arrow_up"),
    (InputKey::ArrowDown, "arrow_down"),
    (InputKey::ArrowLeft, "arrow_left"),
    (InputKey::ArrowRight, "arrow_right"),
    (InputKey::Space, "space"),
    (InputKey::Enter, "enter"),
    (InputKey::Escape, "escape"),
    (InputKey::Tab, "tab"),
    (InputKey::Backspace, "backspace"),
    (InputKey::Delete, "delete"),
    (InputKey::Digit0, "0"),
    (InputKey::Digit1, "1"),
    (InputKey::Digit2, "2"),
    (InputKey::Digit3, "3"),
    (InputKey::Digit4, "4"),
    (InputKey::Digit5, "5"),
    (InputKey::Digit6, "6"),
    (InputKey::Digit7, "7"),
    (InputKey::Digit8, "8"),
    (InputKey::Digit9, "9"),
    (InputKey::Insert, "insert"),
    (InputKey::Home, "home"),
    (InputKey::End, "end"),
    (InputKey::PageUp, "page_up"),
    (InputKey::PageDown, "page_down"),
    (InputKey::F1, "f1"),
    (InputKey::F2, "f2"),
    (InputKey::F3, "f3"),
    (InputKey::F4, "f4"),
    (InputKey::F5, "f5"),
    (InputKey::F6, "f6"),
    (InputKey::F7, "f7"),
    (InputKey::F8, "f8"),
    (InputKey::F9, "f9"),
    (InputKey::F10, "f10"),
    (InputKey::F11, "f11"),
    (InputKey::F12, "f12"),
    (InputKey::Minus, "minus"),
    (InputKey::Equal, "equal"),
    (InputKey::BracketLeft, "bracket_left"),
    (InputKey::BracketRight, "bracket_right"),
    (InputKey::Backslash, "backslash"),
    (InputKey::Semicolon, "semicolon"),
    (InputKey::Quote, "quote"),
    (InputKey::Backquote, "backquote"),
    (InputKey::Comma, "comma"),
    (InputKey::Period, "period"),
    (InputKey::Slash, "slash"),
    (InputKey::Numpad0, "numpad0"),
    (InputKey::Numpad1, "numpad1"),
    (InputKey::Numpad2, "numpad2"),
    (InputKey::Numpad3, "numpad3"),
    (InputKey::Numpad4, "numpad4"),
    (InputKey::Numpad5, "numpad5"),
    (InputKey::Numpad6, "numpad6"),
    (InputKey::Numpad7, "numpad7"),
    (InputKey::Numpad8, "numpad8"),
    (InputKey::Numpad9, "numpad9"),
    (InputKey::NumpadAdd, "numpad_add"),
    (InputKey::NumpadSubtract, "numpad_subtract"),
    (InputKey::NumpadMultiply, "numpad_multiply"),
    (InputKey::NumpadDivide, "numpad_divide"),
    (InputKey::NumpadDecimal, "numpad_decimal"),
];
