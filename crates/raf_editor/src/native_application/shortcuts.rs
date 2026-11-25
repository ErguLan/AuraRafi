//! Global editor shortcuts and their input-edge policy.

use raf_core::project::ProjectType;

/// Dispatch CAD button edges before later motion changes their location and
/// command chords before Control is released. Presentation still waits for
/// its frame permit after dispatching input.
pub(super) fn electronics_input_edge_pending(
    project_type: Option<ProjectType>,
    input: &raf_core::InputSnapshot,
) -> bool {
    project_type == Some(ProjectType::Electronics)
        && input.window_focused
        && (!input.pointer_pressed_buttons.is_empty()
            || !input.pointer_released_buttons.is_empty()
            || (input.modifiers.command_modifier() && !input.keys_pressed.is_empty()))
}

/// Global accelerator for an Electronics project.
///
/// The same `editor_shortcuts` catalog the application menus advertise is used
/// here, so a visible accelerator cannot drift from the behavior again. Only
/// global chords are considered: viewport single-key gestures belong to the CAD
/// canvas, and a focused text field already filters the input out.
pub(super) fn electronics_shortcut_command(
    input: &raf_core::InputSnapshot,
    text_input_focused: bool,
    has_selection: bool,
) -> Option<&'static str> {
    let context = crate::editor_shortcuts::ShortcutContext {
        keyboard_captured_by_text: text_input_focused,
        modal_open: false,
        viewport_active: false,
        has_selection,
    };
    crate::editor_shortcuts::global_command(input, context)
}
