//! Input policy helpers shared by the native viewport and RafUI boundary.

use raf_core::{InputKey, InputRouter, InputSnapshot};

pub fn can_route_camera_input(
    input: &InputSnapshot,
    router: &InputRouter,
    text_input_focused: bool,
) -> bool {
    input.window_focused && !text_input_focused && !router.has_exclusive_pointer_capture()
}

pub fn tool_key(input: &InputSnapshot) -> Option<InputKey> {
    [InputKey::G, InputKey::R, InputKey::T]
        .into_iter()
        .find(|key| input.key_pressed(*key))
}
