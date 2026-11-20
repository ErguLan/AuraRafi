use crate::events::{UiAction, UiEventKind};
use crate::focus::UiInputState;
use crate::node::{UiAccessibilityRole, UiNode, UiNodeKind};

use super::focus::collect_focusable_tabs;
use super::routing::{dispatch, find_node};
use super::{UiDispatchedAction, UiInteractionState};

const TEXT_REPEAT_DELAY_SECONDS: f64 = 0.45;
const TEXT_REPEAT_INTERVAL_SECONDS: f64 = 0.035;

pub(super) fn node_handles_key(root: &UiNode, id: &str, key: &str) -> bool {
    find_node(root, id).map_or(false, |node| {
        node.event_handlers.iter().any(|binding| {
            matches!(&binding.event, UiEventKind::KeyPress(binding_key) if binding_key.eq_ignore_ascii_case(key))
        })
    })
}

impl UiInteractionState {
    /// Returns whether a held text-editing key needs another frame for
    /// keyboard repeat. Native hosts use this to keep event-driven windows
    /// alive while a key remains down.
    pub fn has_active_text_repeat(&self) -> bool {
        self.text_repeat_key.is_some()
    }

    pub(super) fn handle_focused_key<G>(
        &mut self,
        root: &UiNode,
        input: &UiInputState,
        visual_lines: &mut G,
        dispatched: &mut Vec<UiDispatchedAction>,
    ) where
        G: FnMut(&str, &str) -> Vec<(usize, usize)>,
    {
        if let Some(focused) = self.focus.focused.clone() {
            if let Some(_tab) = find_node(root, &focused)
                .filter(|node| node.accessibility_role == UiAccessibilityRole::Tab)
            {
                if input.key_pressed("enter") || input.key_pressed("space") {
                    dispatch(root, &focused, UiEventKind::Click, dispatched);
                }
                let direction =
                    if input.key_pressed_any(&["arrowright", "right", "arrowdown", "down"]) {
                        Some(true)
                    } else if input.key_pressed_any(&["arrowleft", "left", "arrowup", "up"]) {
                        Some(false)
                    } else if input.key_pressed("home") {
                        Some(false)
                    } else if input.key_pressed("end") {
                        Some(true)
                    } else {
                        None
                    };
                if let Some(forward) = direction {
                    let mut tabs = Vec::new();
                    collect_focusable_tabs(root, &mut tabs);
                    if let Some(position) = tabs.iter().position(|id| id == &focused) {
                        let next = if input.key_pressed("home") {
                            tabs.first()
                        } else if input.key_pressed("end") {
                            tabs.last()
                        } else if forward {
                            tabs.get(position + 1).or_else(|| tabs.first())
                        } else {
                            position
                                .checked_sub(1)
                                .and_then(|index| tabs.get(index))
                                .or_else(|| tabs.last())
                        };
                        if let Some(next) = next {
                            self.focus.request_focus(next.clone());
                        }
                    }
                }
            } else if let Some(input_control) =
                find_node(root, &focused).and_then(|node| node.control.text_input())
            {
                let value_key = input_control.value_key.as_str();
                self.controls
                    .set_ime_preedit(value_key, input.ime_preedit.clone());
                let mut text_changed = false;
                let mut text_event = None;
                let word_modifier = input.modifiers.control || input.modifiers.command;
                if word_modifier && input.key_pressed("a") {
                    self.controls.select_all(value_key);
                } else if word_modifier && input.modifiers.shift && input.key_pressed("z") {
                    text_changed = self.controls.redo(value_key);
                    text_event = Some("redo".to_string());
                } else if word_modifier && input.key_pressed("z") {
                    text_changed = self.controls.undo(value_key);
                    text_event = Some("undo".to_string());
                } else if word_modifier && input.key_pressed("y") {
                    text_changed = self.controls.redo(value_key);
                    text_event = Some("redo".to_string());
                } else if word_modifier && input.key_pressed("c") {
                    let selected = self.controls.selected_text(value_key);
                    if !selected.is_empty() {
                        dispatched.push(UiDispatchedAction {
                            target_id: focused.clone(),
                            event: UiEventKind::KeyPress("copy".to_string()),
                            action: UiAction::SetClipboard { text: selected },
                        });
                    }
                } else if word_modifier && input.key_pressed("x") {
                    let selected = self.controls.selected_text(value_key);
                    if !selected.is_empty() {
                        dispatched.push(UiDispatchedAction {
                            target_id: focused.clone(),
                            event: UiEventKind::KeyPress("cut".to_string()),
                            action: UiAction::SetClipboard { text: selected },
                        });
                        text_changed = self.controls.backspace(value_key);
                        text_event = Some("cut".to_string());
                    }
                } else if word_modifier && input.key_pressed("v") {
                    if let Some(paste) = input.clipboard_text.as_deref() {
                        text_changed =
                            self.controls
                                .append_text(value_key, paste, input_control.max_length);
                        text_event = Some("paste".to_string());
                    }
                } else if let Some(repeat_count) = self.text_repeat_count_any(input, &["backspace"])
                {
                    let mut changed = false;
                    for _ in 0..repeat_count {
                        changed |= if word_modifier {
                            self.controls.delete_backward_word(value_key)
                        } else {
                            self.controls.backspace(value_key)
                        };
                    }
                    if changed {
                        text_changed = true;
                        text_event = Some("backspace".to_string());
                    }
                } else if let Some(repeat_count) = self.text_repeat_count_any(input, &["delete"]) {
                    let mut changed = false;
                    for _ in 0..repeat_count {
                        changed |= if word_modifier {
                            self.controls.delete_forward_word(value_key)
                        } else {
                            self.controls.delete_forward(value_key)
                        };
                    }
                    if changed {
                        text_changed = true;
                        text_event = Some("delete".to_string());
                    }
                } else if word_modifier && self.text_repeat_due_any(input, &["arrowleft", "left"]) {
                    self.controls
                        .move_cursor_by_word(value_key, -1, input.modifiers.shift);
                } else if word_modifier && self.text_repeat_due_any(input, &["arrowright", "right"])
                {
                    self.controls
                        .move_cursor_by_word(value_key, 1, input.modifiers.shift);
                } else if self.text_repeat_due_any(input, &["arrowleft", "left"]) {
                    self.controls
                        .move_cursor(value_key, -1, input.modifiers.shift);
                } else if self.text_repeat_due_any(input, &["arrowright", "right"]) {
                    self.controls
                        .move_cursor(value_key, 1, input.modifiers.shift);
                } else if input_control.multiline
                    && self.text_repeat_due_any(input, &["arrowup", "up"])
                {
                    let lines = visual_lines(value_key, self.controls.text(value_key));
                    if lines.is_empty() {
                        self.controls
                            .move_cursor_vertical(value_key, -1, input.modifiers.shift);
                    } else {
                        self.controls.move_cursor_vertical_with_lines(
                            value_key,
                            -1,
                            input.modifiers.shift,
                            &lines,
                        );
                    }
                } else if input_control.multiline
                    && self.text_repeat_due_any(input, &["arrowdown", "down"])
                {
                    let lines = visual_lines(value_key, self.controls.text(value_key));
                    if lines.is_empty() {
                        self.controls
                            .move_cursor_vertical(value_key, 1, input.modifiers.shift);
                    } else {
                        self.controls.move_cursor_vertical_with_lines(
                            value_key,
                            1,
                            input.modifiers.shift,
                            &lines,
                        );
                    }
                } else if input.key_pressed("home") {
                    if input_control.multiline {
                        self.controls.move_cursor_to_line_edge(
                            value_key,
                            false,
                            input.modifiers.shift,
                        );
                    } else {
                        self.controls
                            .move_cursor_to_edge(value_key, false, input.modifiers.shift);
                    }
                } else if input.key_pressed("end") {
                    if input_control.multiline {
                        self.controls.move_cursor_to_line_edge(
                            value_key,
                            true,
                            input.modifiers.shift,
                        );
                    } else {
                        self.controls
                            .move_cursor_to_edge(value_key, true, input.modifiers.shift);
                    }
                }
                if input.key_pressed("enter") && input_control.multiline {
                    if self
                        .controls
                        .append_text(value_key, "\n", input_control.max_length)
                    {
                        text_changed = true;
                        text_event = Some("newline".to_string());
                    }
                }
                if text_changed {
                    dispatched.push(UiDispatchedAction {
                        target_id: focused.clone(),
                        event: UiEventKind::KeyPress(
                            text_event.unwrap_or_else(|| "text_edit".to_string()),
                        ),
                        action: UiAction::SetText {
                            key: input_control.value_key.clone(),
                            value: self.controls.text(value_key).to_string(),
                        },
                    });
                }
                if !input.text_input.is_empty()
                    && (input_control.multiline
                        || !input
                            .text_input
                            .chars()
                            .any(|character| character == '\n' || character == '\r'))
                    && self.controls.append_text(
                        value_key,
                        &input.text_input,
                        input_control.max_length,
                    )
                {
                    dispatched.push(UiDispatchedAction {
                        target_id: focused.clone(),
                        event: UiEventKind::TextInput(input.text_input.clone()),
                        action: UiAction::SetText {
                            key: input_control.value_key.clone(),
                            value: self.controls.text(value_key).to_string(),
                        },
                    });
                }
                if input.key_pressed("enter") && !input_control.multiline {
                    if let Some(command) = input_control.submit_command.as_ref() {
                        dispatched.push(UiDispatchedAction {
                            target_id: focused.clone(),
                            event: UiEventKind::KeyPress("enter".to_string()),
                            action: UiAction::Command {
                                name: command.clone(),
                            },
                        });
                    }
                }
            } else if let Some(node) = find_node(root, &focused).filter(|node| node.text_selectable)
            {
                let text = node.text_value.as_deref().unwrap_or_default();
                let word_modifier = input.modifiers.control || input.modifiers.command;
                if word_modifier && input.key_pressed("a") {
                    self.controls.select_all_selectable(&focused, text);
                } else if word_modifier && input.key_pressed("c") {
                    let selected = self.controls.selected_selectable_text(&focused, text);
                    if !selected.is_empty() {
                        dispatched.push(UiDispatchedAction {
                            target_id: focused.clone(),
                            event: UiEventKind::KeyPress("copy".to_string()),
                            action: UiAction::SetClipboard { text: selected },
                        });
                    }
                } else if input.key_pressed_any(&["arrowleft", "left"]) {
                    self.controls
                        .move_selectable_cursor(&focused, text, -1, input.modifiers.shift);
                } else if input.key_pressed_any(&["arrowright", "right"]) {
                    self.controls
                        .move_selectable_cursor(&focused, text, 1, input.modifiers.shift);
                } else if input.key_pressed("home") {
                    self.controls.move_selectable_cursor_to_edge(
                        &focused,
                        text,
                        false,
                        input.modifiers.shift,
                    );
                } else if input.key_pressed("end") {
                    self.controls.move_selectable_cursor_to_edge(
                        &focused,
                        text,
                        true,
                        input.modifiers.shift,
                    );
                }
            } else if let Some(select) =
                find_node(root, &focused).and_then(|node| node.control.select())
            {
                if self.select_typeahead_target.as_deref() != Some(focused.as_str()) {
                    self.select_typeahead_target = Some(focused.clone());
                    self.select_typeahead.clear();
                }
                if !input.text_input.is_empty() {
                    if input.time_seconds - self.select_typeahead_at_seconds > 0.8 {
                        self.select_typeahead.clear();
                    }
                    self.select_typeahead
                        .push_str(&input.text_input.to_lowercase());
                    self.select_typeahead_at_seconds = input.time_seconds;
                    let query = self.select_typeahead.as_str();
                    if let Some((index, option)) =
                        select.options.iter().enumerate().find(|(_, option)| {
                            !option.disabled
                                && (option.value.to_lowercase().starts_with(query)
                                    || option.label_key.to_lowercase().starts_with(query))
                        })
                    {
                        dispatched.push(UiDispatchedAction {
                            target_id: focused.clone(),
                            event: UiEventKind::TextInput(input.text_input.clone()),
                            action: UiAction::SetSelect {
                                key: select.value_key.clone(),
                                value: option.value.clone(),
                                index,
                            },
                        });
                    }
                }
                let key = if input.key_pressed_any(&["arrowdown", "down"]) {
                    Some(true)
                } else if input.key_pressed_any(&["arrowup", "up"]) {
                    Some(false)
                } else {
                    None
                };
                if let Some(forward) = key {
                    if let Some(index) = select.next_enabled_index(select.active_index, forward) {
                        if let Some(option) = select.options.get(index) {
                            dispatched.push(UiDispatchedAction {
                                target_id: focused.clone(),
                                event: UiEventKind::KeyPress(if forward {
                                    "arrowdown".to_string()
                                } else {
                                    "arrowup".to_string()
                                }),
                                action: UiAction::SetSelect {
                                    key: select.value_key.clone(),
                                    value: option.value.clone(),
                                    index,
                                },
                            });
                        }
                    }
                } else if input.key_pressed("home") || input.key_pressed("end") {
                    let index = if input.key_pressed("home") {
                        select.options.iter().position(|option| !option.disabled)
                    } else {
                        select.options.iter().rposition(|option| !option.disabled)
                    };
                    if let Some(index) = index {
                        if let Some(option) = select.options.get(index) {
                            dispatched.push(UiDispatchedAction {
                                target_id: focused.clone(),
                                event: UiEventKind::KeyPress(if input.key_pressed("home") {
                                    "home".to_string()
                                } else {
                                    "end".to_string()
                                }),
                                action: UiAction::SetSelect {
                                    key: select.value_key.clone(),
                                    value: option.value.clone(),
                                    index,
                                },
                            });
                        }
                    }
                } else if input.key_pressed("escape") && select.open {
                    dispatched.push(UiDispatchedAction {
                        target_id: focused.clone(),
                        event: UiEventKind::KeyPress("escape".to_string()),
                        action: UiAction::SetSelectOpen {
                            id: focused.clone(),
                            open: false,
                        },
                    });
                } else if input.key_pressed("enter") || input.key_pressed("space") {
                    dispatched.push(UiDispatchedAction {
                        target_id: focused.clone(),
                        event: UiEventKind::KeyPress("enter".to_string()),
                        action: UiAction::SetSelectOpen {
                            id: focused.clone(),
                            open: !select.open,
                        },
                    });
                }
            } else if let Some(toggle) =
                find_node(root, &focused).and_then(|node| node.control.toggle())
            {
                if input.key_pressed("space") || input.key_pressed("enter") {
                    dispatched.push(UiDispatchedAction {
                        target_id: focused.clone(),
                        event: UiEventKind::KeyPress(if input.key_pressed("enter") {
                            "enter".to_string()
                        } else {
                            "space".to_string()
                        }),
                        action: UiAction::SetToggle {
                            key: toggle.value_key.clone(),
                            value: !toggle.value,
                        },
                    });
                }
            } else if let Some(picker) =
                find_node(root, &focused).and_then(|node| node.control.color_picker())
            {
                let mut saturation = picker.saturation;
                let mut value = picker.value;
                let step = 0.01;
                if input.key_pressed_any(&["arrowleft", "left"]) {
                    saturation = (saturation - step).clamp(0.0, 1.0);
                } else if input.key_pressed_any(&["arrowright", "right"]) {
                    saturation = (saturation + step).clamp(0.0, 1.0);
                } else if input.key_pressed_any(&["arrowup", "up"]) {
                    value = (value + step).clamp(0.0, 1.0);
                } else if input.key_pressed_any(&["arrowdown", "down"]) {
                    value = (value - step).clamp(0.0, 1.0);
                } else if input.key_pressed("home") {
                    saturation = 0.0;
                } else if input.key_pressed("end") {
                    saturation = 1.0;
                } else {
                    saturation = picker.saturation;
                    value = picker.value;
                }
                if (saturation - picker.saturation).abs() > f32::EPSILON
                    || (value - picker.value).abs() > f32::EPSILON
                {
                    dispatched.push(UiDispatchedAction {
                        target_id: focused.clone(),
                        event: UiEventKind::KeyPress("color-picker".to_string()),
                        action: UiAction::SetColorHsv {
                            key: picker.value_key.clone(),
                            hue: picker.hue,
                            saturation,
                            value,
                        },
                    });
                }
            } else if let Some(range) =
                find_node(root, &focused).and_then(|node| node.control.range())
            {
                let value = if input.key_pressed("home") {
                    Some(range.min)
                } else if input.key_pressed("end") {
                    Some(range.max)
                } else if input.key_pressed("arrowleft") || input.key_pressed("arrowdown") {
                    Some((range.value - range.step).clamp(range.min, range.max))
                } else if input.key_pressed("arrowright") || input.key_pressed("arrowup") {
                    Some((range.value + range.step).clamp(range.min, range.max))
                } else {
                    None
                };
                if let Some(value) = value {
                    dispatched.push(UiDispatchedAction {
                        target_id: focused.clone(),
                        event: UiEventKind::KeyPress("range".to_string()),
                        action: UiAction::SetRange {
                            key: range.value_key.clone(),
                            value,
                        },
                    });
                }
            } else if let Some(button) = find_node(root, &focused)
                .filter(|node| node.kind == UiNodeKind::Button && !node.disabled)
            {
                if let Some(key) = input
                    .pressed_keys
                    .iter()
                    .find(|key| matches!(key.as_str(), "enter" | "space"))
                {
                    let has_explicit_binding = button.event_handlers.iter().any(|binding| {
                        matches!(&binding.event, UiEventKind::KeyPress(binding_key)
                            if binding_key.eq_ignore_ascii_case(key))
                    });
                    if !has_explicit_binding {
                        // Keyboard activation reuses the existing Click
                        // bindings so hosts do not need a second command path.
                        dispatch(root, &focused, UiEventKind::Click, dispatched);
                    }
                }
            }
            for key in &input.pressed_keys {
                dispatch(
                    root,
                    &focused,
                    UiEventKind::KeyPress(key.clone()),
                    dispatched,
                );
            }
            if !input.text_input.is_empty() {
                dispatch(
                    root,
                    &focused,
                    UiEventKind::TextInput(input.text_input.clone()),
                    dispatched,
                );
            }
        }
    }

    fn text_repeat_due_any(&mut self, input: &UiInputState, keys: &[&str]) -> bool {
        self.text_repeat_count_any(input, keys).is_some()
    }

    /// Returns the number of edit operations due for this frame.
    ///
    /// `pressed_keys` is usually a set-like list, but the native bridge keeps
    /// repeated text-editing edges as duplicate entries so fast taps are not
    /// lost between redraws. A held key uses the retained timer instead.
    fn text_repeat_count_any(&mut self, input: &UiInputState, keys: &[&str]) -> Option<usize> {
        if let Some(key) = keys
            .iter()
            .copied()
            .find(|key| input.key_press_count(key) > 0)
        {
            let count = input.key_press_count(key);
            if input.key_down(key) {
                self.text_repeat_key = Some(key.to_string());
                self.text_repeat_next_seconds = input.time_seconds + TEXT_REPEAT_DELAY_SECONDS;
            } else {
                self.text_repeat_key = None;
                self.text_repeat_next_seconds = 0.0;
            }
            return Some(count);
        }

        if keys
            .iter()
            .copied()
            .find(|key| input.key_down(key) && self.text_repeat_key.as_deref() == Some(*key))
            .is_some()
        {
            if input.time_seconds < self.text_repeat_next_seconds {
                return None;
            }
            self.text_repeat_next_seconds = input.time_seconds + TEXT_REPEAT_INTERVAL_SECONDS;
            return Some(1);
        }
        None
    }
}
