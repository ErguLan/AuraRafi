use crate::events::{UiAction, UiEventKind};
use crate::focus::{UiFocusState, UiModifiers};
use crate::state::UiControlState;

use self::pointer::{UiPointerClick, UiTextClick};

mod drag;
mod focus;
mod keyboard;
mod pointer;
mod routing;
mod scroll;

#[cfg(test)]
mod tests;

/// A resolved UI action emitted by the retained interaction controller.
#[derive(Debug, Clone, PartialEq)]
pub struct UiDispatchedAction {
    pub target_id: String,
    pub event: UiEventKind,
    pub action: UiAction,
}

/// Renderer-neutral pointer and keyboard state for a retained `UiSurface`.
///
/// The caller feeds an input snapshot each frame and executes the returned
/// actions in its own application boundary. This keeps UI definitions data
/// driven and makes the same surface usable with the current shell or a
/// direct window surface later.
#[derive(Debug, Clone, Default)]
pub struct UiInteractionState {
    pub focus: UiFocusState,
    pub controls: UiControlState,
    pointer_was_down: bool,
    active_pointer_target: Option<String>,
    active_drag_start_actions: Vec<UiAction>,
    active_drag_move_actions: Vec<UiAction>,
    active_drag_end_actions: Vec<UiAction>,
    drag_started: bool,
    drag_origin: Option<[f32; 2]>,
    last_pointer_position: Option<[f32; 2]>,
    hovered_since_seconds: Option<f64>,
    text_selection_target: Option<String>,
    last_text_click: Option<UiTextClick>,
    last_pointer_click: Option<UiPointerClick>,
    last_modifiers: UiModifiers,
    text_repeat_key: Option<String>,
    text_repeat_next_seconds: f64,
    scrollbar_grab_offset: Option<f32>,
    select_typeahead_target: Option<String>,
    select_typeahead: String,
    select_typeahead_at_seconds: f64,
}
