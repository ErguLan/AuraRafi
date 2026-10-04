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
        }
        for (enabled, label) in [
            (input.modifiers.control, "ctrl"),
            (input.modifiers.shift, "shift"),
            (input.modifiers.alt, "alt"),
            (input.modifiers.command, "cmd"),
        ] {
            if enabled {
                snapshot.keys_held.push(label.to_string());
            }
        }
        for (button, label) in [
            (raf_core::PointerButton::Primary, 0),
            (raf_core::PointerButton::Secondary, 1),
            (raf_core::PointerButton::Middle, 2),
        ] {
            if input.button_down(button) {
                snapshot.mouse_held.push(label);
            }
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
];
