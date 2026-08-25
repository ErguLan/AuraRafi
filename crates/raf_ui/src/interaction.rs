use crate::controls::UiControl;
use crate::events::{UiAction, UiCursorIcon, UiEventKind, UiPointerButton};
use crate::focus::{UiFocusPolicy, UiFocusState, UiInputState, UiModifiers};
use crate::geometry::UiRect;
use crate::hit_test::{hit_test, UiHitRegion, UiHitTestMode};
use crate::node::{UiNode, UiNodeKind};
use crate::state::UiControlState;

const DRAG_THRESHOLD_PX: f32 = 4.0;
const DOUBLE_CLICK_DISTANCE_PX: f32 = 6.0;
const DOUBLE_CLICK_TIME_SECONDS: f64 = 0.45;
const TEXT_REPEAT_DELAY_SECONDS: f64 = 0.45;
const TEXT_REPEAT_INTERVAL_SECONDS: f64 = 0.035;
const GENERATED_SCROLLBAR_PREFIX: &str = "__rafui.scrollbar.";

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
}

#[derive(Debug, Clone, PartialEq)]
struct UiTextClick {
    target_id: String,
    position: [f32; 2],
    time_seconds: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct UiPointerClick {
    target_id: String,
    position: [f32; 2],
    time_seconds: f64,
}

impl UiInteractionState {
    /// Cancels a pointer gesture when its owning host resets or replaces the
    /// retained surface. Hover state remains intact, but no stale drag can
    /// leak into the next pointer press.
    pub fn cancel_pointer_gesture(&mut self) {
        self.pointer_was_down = false;
        self.active_pointer_target = None;
        self.active_drag_start_actions.clear();
        self.active_drag_move_actions.clear();
        self.active_drag_end_actions.clear();
        self.drag_started = false;
        self.drag_origin = None;
        self.text_selection_target = None;
        self.scrollbar_grab_offset = None;
        self.focus.set_active(None);
    }

    pub fn pointer_position(&self) -> Option<[f32; 2]> {
        self.last_pointer_position
    }

    pub fn modifiers(&self) -> UiModifiers {
        self.last_modifiers
    }

    /// Returns whether this surface owns an in-progress primary-pointer
    /// gesture. Hosts can keep delivering a drag after the pointer leaves the
    /// surface without treating another surface's drag as local input.
    pub fn has_pointer_capture(&self) -> bool {
        // A primary button can be down over a transparent/non-interactive
        // region without belonging to RafUI. The old `pointer_was_down`
        // shortcut turned any full-window retained surface into a pointer
        // shield and starved the viewport camera after a toolbar hover or
        // click. Only an actual hit target owns the gesture.
        self.active_pointer_target.is_some()
    }

    /// Returns whether this retained surface currently owns text-entry input.
    ///
    /// Buttons, hierarchy rows, toggles and ranges may be focusable for
    /// keyboard accessibility, but they must not freeze viewport navigation
    /// merely because they remain focused after a click. Only a focused text
    /// input owns the editor's typing boundary.
    pub fn captures_keyboard_input(&self, root: &UiNode) -> bool {
        self.focus
            .focused
            .as_deref()
            .and_then(|id| find_node(root, id))
            .is_some_and(|node| {
                node.kind == UiNodeKind::TextInput
                    && node.focusable
                    && !node.disabled
                    && node.control.text_input().is_some()
            })
    }

    /// Clears pointer and focus state when the retained document changes
    /// identity, preventing a previous page's hover or drag from leaking into
    /// the next page.
    pub fn reset_for_surface_change(&mut self) {
        self.cancel_pointer_gesture();
        self.focus.clear_focus();
        self.focus.set_hovered(None);
        self.last_pointer_position = None;
        self.hovered_since_seconds = None;
        self.last_text_click = None;
        self.last_pointer_click = None;
        self.last_modifiers = UiModifiers::default();
        self.text_repeat_key = None;
        self.text_repeat_next_seconds = 0.0;
    }

    pub fn hover_elapsed_seconds(&self, now_seconds: f64) -> f32 {
        self.hovered_since_seconds
            .map(|started| (now_seconds - started).max(0.0).min(60.0) as f32)
            .unwrap_or(0.0)
    }

    pub fn hover_intent_progress(&self, now_seconds: f64, delay_seconds: f32) -> f32 {
        if delay_seconds <= f32::EPSILON {
            return f32::from(self.focus.hovered.is_some());
        }
        (self.hover_elapsed_seconds(now_seconds) / delay_seconds).clamp(0.0, 1.0)
    }

    /// Returns the semantic cursor for the currently hovered retained node.
    /// Text fields get an I-beam; actionable controls get a pointing hand.
    pub fn cursor_hint(&self, root: &UiNode) -> UiCursorIcon {
        let Some(hovered_id) = self.focus.hovered.as_deref() else {
            return UiCursorIcon::Default;
        };
        if is_scrollbar_target(hovered_id) {
            return UiCursorIcon::ResizeVertical;
        }
        let Some(node) = find_node(root, hovered_id) else {
            return UiCursorIcon::Default;
        };
        if node.disabled {
            return UiCursorIcon::Default;
        }
        if node.control.text_input().is_some() {
            return UiCursorIcon::Text;
        }
        if node
            .classes
            .iter()
            .any(|class| class == "editor-splitter-vertical" || class == "bottom-dock-splitter")
        {
            return UiCursorIcon::ResizeHorizontal;
        }
        if node
            .classes
            .iter()
            .any(|class| class == "editor-splitter-horizontal")
        {
            return UiCursorIcon::ResizeVertical;
        }
        // Focusability is a keyboard-navigation property, not a promise that
        // the pointer is over a clickable control. Panels, scroll containers,
        // and the native title drag region may be focusable/interactive while
        // still needing the normal arrow cursor.
        if node
            .classes
            .iter()
            .any(|class| class == "application-bar-drag-region")
        {
            return UiCursorIcon::Default;
        }
        let value_control = matches!(node.control, UiControl::Toggle(_) | UiControl::Range(_));
        if node.kind == UiNodeKind::Button || !node.event_handlers.is_empty() || value_control {
            return UiCursorIcon::PointingHand;
        }
        UiCursorIcon::Default
    }

    pub fn update(
        &mut self,
        root: &UiNode,
        hit_regions: &[UiHitRegion],
        input: &UiInputState,
        focus_policy: &UiFocusPolicy,
    ) -> Vec<UiDispatchedAction> {
        self.update_with_text_hit_test(root, hit_regions, input, focus_policy, |_, _, _| None)
    }

    /// Variant of [`Self::update`] that lets a presentation host provide an
    /// exact character hit-test using its font metrics. The fallback path in
    /// this module remains proportional, so headless/native callers still get
    /// fully functional selection without depending on a renderer.
    pub fn update_with_text_hit_test<F>(
        &mut self,
        root: &UiNode,
        hit_regions: &[UiHitRegion],
        input: &UiInputState,
        focus_policy: &UiFocusPolicy,
        mut text_hit_test: F,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str, &str, [f32; 2]) -> Option<usize>,
    {
        self.last_modifiers = input.modifiers;
        let hovered = input
            .pointer_position
            .and_then(|point| hit_test(hit_regions, point, UiHitTestMode::InteractiveOnly));
        let hovered_id = hovered.as_ref().map(|hit| hit.id.clone());
        let previous_hovered = self.focus.hovered.clone();
        let mut dispatched = Vec::new();
        let pointer_moved = input.pointer_position != self.last_pointer_position;

        if input.pointer_pressed_outside && !self.has_pointer_capture() {
            self.focus.clear_focus();
            self.focus.set_active(None);
            self.last_text_click = None;
        }

        if previous_hovered != hovered_id {
            if let Some(id) = previous_hovered.as_deref() {
                dispatch(root, id, UiEventKind::HoverLeave, &mut dispatched);
            }
            if let Some(id) = hovered_id.as_deref() {
                dispatch(root, id, UiEventKind::HoverEnter, &mut dispatched);
            }
            self.focus.set_hovered(hovered_id.clone());
            self.hovered_since_seconds = hovered_id.as_ref().map(|_| input.time_seconds);
        }

        if pointer_moved {
            if let Some(id) = hovered_id.as_deref() {
                dispatch(root, id, UiEventKind::PointerMove, &mut dispatched);
            }
        }

        let primary_down = input.button_down(UiPointerButton::Primary);
        if primary_down && !self.pointer_was_down {
            self.active_pointer_target = hovered_id.clone();
            self.active_drag_start_actions.clear();
            self.active_drag_move_actions.clear();
            self.active_drag_end_actions.clear();
            self.drag_started = false;
            self.drag_origin = input.pointer_position;
            self.text_selection_target = None;
            self.focus.set_active(hovered_id.clone());
            if let Some(target_id) = hovered_id.as_deref() {
                if let Some((scroll_id, is_thumb)) = scrollbar_target(target_id) {
                    let thumb = scrollbar_region(hit_regions, scroll_id, true);
                    self.scrollbar_grab_offset = input.pointer_position.map(|point| {
                        if is_thumb {
                            thumb.map(|region| point[1] - region.rect.y).unwrap_or(0.0)
                        } else {
                            thumb.map(|region| region.rect.height * 0.5).unwrap_or(0.0)
                        }
                    });
                    if !is_thumb {
                        if let Some(point) = input.pointer_position {
                            dispatch_scrollbar_position(
                                &mut self.controls,
                                hit_regions,
                                scroll_id,
                                point,
                                self.scrollbar_grab_offset.unwrap_or(0.0),
                                &mut dispatched,
                            );
                        }
                    }
                }
            }
            if let Some(hit) = hovered.as_ref() {
                if hit.focusable {
                    self.focus.request_focus(hit.id.clone());
                }
                if let Some(text_input) =
                    find_node(root, &hit.id).and_then(|node| node.control.text_input())
                {
                    if let Some(point) = input.pointer_position {
                        let value_key = text_input.value_key.as_str();
                        let text = self.controls.text(value_key).to_string();
                        let index = text_hit_test(&hit.id, &text, point)
                            .unwrap_or_else(|| proportional_text_index(&text, hit.rect, point));
                        let is_double_click = self.last_text_click.as_ref().is_some_and(|last| {
                            last.target_id == hit.id
                                && input.time_seconds - last.time_seconds >= 0.0
                                && input.time_seconds - last.time_seconds
                                    <= DOUBLE_CLICK_TIME_SECONDS
                                && distance_squared(last.position, point)
                                    <= DOUBLE_CLICK_DISTANCE_PX * DOUBLE_CLICK_DISTANCE_PX
                        });
                        if is_double_click {
                            let range = word_range_at(&text, index);
                            self.controls
                                .set_selection(value_key, range.start, range.end);
                            self.text_selection_target = Some(hit.id.clone());
                            // A third click starts a fresh click sequence rather
                            // than being treated as another double click.
                            self.last_text_click = None;
                        } else {
                            self.controls.set_cursor(value_key, index, false);
                            self.text_selection_target = Some(hit.id.clone());
                            self.last_text_click = Some(UiTextClick {
                                target_id: hit.id.clone(),
                                position: point,
                                time_seconds: input.time_seconds,
                            });
                        }
                    }
                } else {
                    // A click on another control breaks the double-click
                    // sequence; returning to the field must start a new one.
                    self.last_text_click = None;
                }
                dispatch(
                    root,
                    &hit.id,
                    UiEventKind::PointerDown(UiPointerButton::Primary),
                    &mut dispatched,
                );
                self.active_drag_start_actions =
                    captured_actions(root, &hit.id, &UiEventKind::DragStart);
                self.active_drag_move_actions =
                    captured_actions(root, &hit.id, &UiEventKind::DragMove);
                self.active_drag_end_actions =
                    captured_actions(root, &hit.id, &UiEventKind::DragEnd);
                dispatch_range_value(
                    root,
                    hit_regions,
                    &hit.id,
                    input.pointer_position,
                    UiEventKind::PointerDown(UiPointerButton::Primary),
                    &mut dispatched,
                );
            }
        } else if primary_down && self.pointer_was_down && pointer_moved {
            if let (Some(text_target), Some(point)) = (
                self.text_selection_target.as_deref(),
                input.pointer_position,
            ) {
                if let Some(text_input) =
                    find_node(root, text_target).and_then(|node| node.control.text_input())
                {
                    let value_key = text_input.value_key.as_str();
                    let text = self.controls.text(value_key).to_string();
                    let index = text_hit_test(text_target, &text, point).or_else(|| {
                        hit_regions
                            .iter()
                            .find(|region| region.id == text_target)
                            .map(|region| proportional_text_index(&text, region.rect, point))
                    });
                    if let Some(index) = index {
                        self.controls.set_cursor(value_key, index, true);
                    }
                }
            }
            if let Some(id) = self.active_pointer_target.as_deref() {
                let moved_far_enough = self
                    .drag_origin
                    .zip(input.pointer_position)
                    .map(|(origin, current)| {
                        let dx = current[0] - origin[0];
                        let dy = current[1] - origin[1];
                        dx * dx + dy * dy >= DRAG_THRESHOLD_PX * DRAG_THRESHOLD_PX
                    })
                    .unwrap_or(false);
                if !self.drag_started && moved_far_enough {
                    dispatch_captured(
                        root,
                        id,
                        UiEventKind::DragStart,
                        &self.active_drag_start_actions,
                        &mut dispatched,
                    );
                    self.drag_started = true;
                }
                if let Some((scroll_id, is_thumb)) = scrollbar_target(id) {
                    if self.drag_started || !is_thumb {
                        if let Some(point) = input.pointer_position {
                            dispatch_scrollbar_position(
                                &mut self.controls,
                                hit_regions,
                                scroll_id,
                                point,
                                self.scrollbar_grab_offset.unwrap_or(0.0),
                                &mut dispatched,
                            );
                        }
                    }
                } else if self.drag_started {
                    dispatch_captured(
                        root,
                        id,
                        UiEventKind::DragMove,
                        &self.active_drag_move_actions,
                        &mut dispatched,
                    );
                    dispatch_range_value(
                        root,
                        hit_regions,
                        id,
                        input.pointer_position,
                        UiEventKind::DragMove,
                        &mut dispatched,
                    );
                }
            }
        } else if !primary_down && self.pointer_was_down {
            if let Some(id) = self.active_pointer_target.as_deref() {
                dispatch(
                    root,
                    id,
                    UiEventKind::PointerUp(UiPointerButton::Primary),
                    &mut dispatched,
                );
                if self.drag_started {
                    dispatch_captured(
                        root,
                        id,
                        UiEventKind::DragEnd,
                        &self.active_drag_end_actions,
                        &mut dispatched,
                    );
                } else if hovered_id.as_deref() == Some(id) {
                    dispatch(root, id, UiEventKind::Click, &mut dispatched);
                    if let Some(point) = input.pointer_position {
                        let is_double_click =
                            self.last_pointer_click.as_ref().is_some_and(|last| {
                                last.target_id == id
                                    && input.time_seconds - last.time_seconds >= 0.0
                                    && input.time_seconds - last.time_seconds
                                        <= DOUBLE_CLICK_TIME_SECONDS
                                    && distance_squared(last.position, point)
                                        <= DOUBLE_CLICK_DISTANCE_PX * DOUBLE_CLICK_DISTANCE_PX
                            });
                        if is_double_click {
                            dispatch(root, id, UiEventKind::DoubleClick, &mut dispatched);
                            self.last_pointer_click = None;
                        } else {
                            self.last_pointer_click = Some(UiPointerClick {
                                target_id: id.to_string(),
                                position: point,
                                time_seconds: input.time_seconds,
                            });
                        }
                    }
                    dispatch_toggle_value(root, id, &mut dispatched);
                }
            }
            self.cancel_pointer_gesture();
        }

        if input.button_pressed(UiPointerButton::Secondary) {
            if let Some(hit) = hovered.as_ref() {
                if hit.focusable {
                    self.focus.request_focus(hit.id.clone());
                }
                dispatch(root, &hit.id, UiEventKind::ContextMenu, &mut dispatched);
            }
        }

        let focused_consumes_tab = self
            .focus
            .focused
            .as_deref()
            .map_or(false, |id| node_handles_key(root, id, "Tab"));
        if input.key_pressed("tab") && !focused_consumes_tab {
            let inferred_policy;
            let effective_policy = if focus_policy.tab_order.is_empty() {
                inferred_policy = UiFocusPolicy::from_focusable_ids(
                    hit_regions
                        .iter()
                        .filter(|region| region.focusable && !region.disabled)
                        .map(|region| region.id.clone()),
                );
                &inferred_policy
            } else {
                focus_policy
            };
            let next = if input.modifiers.shift {
                effective_policy.previous_before(self.focus.focused.as_deref())
            } else {
                effective_policy.next_after(self.focus.focused.as_deref())
            };
            if let Some(next) = next {
                self.focus.request_focus(next);
            }
        }

        if input.scroll_delta != [0.0, 0.0] {
            let scroll_container = input
                .pointer_position
                .and_then(|point| find_scroll_container_at(root, hit_regions, point))
                .or_else(|| {
                    hovered_id
                        .as_deref()
                        .and_then(|id| find_scroll_container(root, id))
                });
            if let Some((scroll_id, axis)) = scroll_container {
                let mut delta = input.scroll_delta;
                if !axis.scrolls_horizontally() {
                    delta[0] = 0.0;
                }
                if !axis.scrolls_vertically() {
                    delta[1] = 0.0;
                }
                if delta != [0.0, 0.0] {
                    let offset = self.controls.scroll_by(scroll_id, delta);
                    dispatched.push(UiDispatchedAction {
                        target_id: scroll_id.to_string(),
                        event: UiEventKind::PointerMove,
                        action: UiAction::ScrollTo {
                            id: scroll_id.to_string(),
                            offset,
                        },
                    });
                }
            }
        }

        if let Some(focused) = self.focus.focused.clone() {
            if let Some(input_control) =
                find_node(root, &focused).and_then(|node| node.control.text_input())
            {
                let value_key = input_control.value_key.as_str();
                let mut text_changed = false;
                let mut text_event = None;
                let word_modifier = input.modifiers.control || input.modifiers.command;
                if word_modifier && input.key_pressed("a") {
                    self.controls.select_all(value_key);
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
                } else if (input.key_pressed("backspace") || input.key_down("backspace"))
                    && self.text_repeat_due(input, "backspace")
                    && if word_modifier {
                        self.controls.delete_backward_word(value_key)
                    } else {
                        self.controls.backspace(value_key)
                    }
                {
                    text_changed = true;
                    text_event = Some("backspace".to_string());
                } else if (input.key_pressed("delete") || input.key_down("delete"))
                    && self.text_repeat_due(input, "delete")
                    && if word_modifier {
                        self.controls.delete_forward_word(value_key)
                    } else {
                        self.controls.delete_forward(value_key)
                    }
                {
                    text_changed = true;
                    text_event = Some("delete".to_string());
                } else if input.key_pressed_any(&["arrowleft", "left"]) {
                    self.controls
                        .move_cursor(value_key, -1, input.modifiers.shift);
                } else if input.key_pressed_any(&["arrowright", "right"]) {
                    self.controls
                        .move_cursor(value_key, 1, input.modifiers.shift);
                } else if input.key_pressed("home") {
                    self.controls
                        .move_cursor_to_edge(value_key, false, input.modifiers.shift);
                } else if input.key_pressed("end") {
                    self.controls
                        .move_cursor_to_edge(value_key, true, input.modifiers.shift);
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
            } else if let Some(toggle) =
                find_node(root, &focused).and_then(|node| node.control.toggle())
            {
                if input.key_pressed("space") || input.key_pressed("enter") {
                    dispatched.push(UiDispatchedAction {
                        target_id: focused.clone(),
                        event: UiEventKind::KeyPress("space".to_string()),
                        action: UiAction::SetToggle {
                            key: toggle.value_key.clone(),
                            value: !toggle.value,
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
            }
            for key in &input.pressed_keys {
                dispatch(
                    root,
                    &focused,
                    UiEventKind::KeyPress(key.clone()),
                    &mut dispatched,
                );
            }
            if !input.text_input.is_empty() {
                dispatch(
                    root,
                    &focused,
                    UiEventKind::TextInput(input.text_input.clone()),
                    &mut dispatched,
                );
            }
        }

        self.pointer_was_down = primary_down;
        self.last_pointer_position = input.pointer_position;
        dispatched
    }

    fn text_repeat_due(&mut self, input: &UiInputState, key: &str) -> bool {
        if input.key_pressed(key) {
            self.text_repeat_key = Some(key.to_string());
            self.text_repeat_next_seconds = input.time_seconds + TEXT_REPEAT_DELAY_SECONDS;
            return true;
        }
        if !input.key_down(key) || self.text_repeat_key.as_deref() != Some(key) {
            return false;
        }
        if input.time_seconds < self.text_repeat_next_seconds {
            return false;
        }
        self.text_repeat_next_seconds = input.time_seconds + TEXT_REPEAT_INTERVAL_SECONDS;
        true
    }
}

fn distance_squared(left: [f32; 2], right: [f32; 2]) -> f32 {
    let dx = left[0] - right[0];
    let dy = left[1] - right[1];
    dx * dx + dy * dy
}

fn is_scrollbar_target(id: &str) -> bool {
    id.starts_with(GENERATED_SCROLLBAR_PREFIX)
}

fn scrollbar_target(id: &str) -> Option<(&str, bool)> {
    let suffix = id.strip_prefix(GENERATED_SCROLLBAR_PREFIX)?;
    if let Some(scroll_id) = suffix.strip_suffix(".thumb") {
        return Some((scroll_id, true));
    }
    suffix
        .strip_suffix(".track")
        .map(|scroll_id| (scroll_id, false))
}

fn scrollbar_region<'a>(
    hit_regions: &'a [UiHitRegion],
    scroll_id: &str,
    thumb: bool,
) -> Option<&'a UiHitRegion> {
    let suffix = if thumb { ".thumb" } else { ".track" };
    let id = format!("{GENERATED_SCROLLBAR_PREFIX}{scroll_id}{suffix}");
    hit_regions.iter().find(|region| region.id == id)
}

fn dispatch_scrollbar_position(
    controls: &mut UiControlState,
    hit_regions: &[UiHitRegion],
    scroll_id: &str,
    point: [f32; 2],
    grab_offset: f32,
    dispatched: &mut Vec<UiDispatchedAction>,
) {
    let (Some(track), Some(thumb)) = (
        scrollbar_region(hit_regions, scroll_id, false),
        scrollbar_region(hit_regions, scroll_id, true),
    ) else {
        return;
    };
    let travel = (track.rect.height - thumb.rect.height).max(0.0);
    let fraction = if travel <= f32::EPSILON {
        0.0
    } else {
        ((point[1] - track.rect.y - grab_offset) / travel).clamp(0.0, 1.0)
    };
    let max_offset = controls.scroll_max_offset(scroll_id);
    let offset = controls.set_scroll_offset(scroll_id, [max_offset[0], max_offset[1] * fraction]);
    dispatched.push(UiDispatchedAction {
        target_id: scroll_id.to_string(),
        event: UiEventKind::DragMove,
        action: UiAction::ScrollTo {
            id: scroll_id.to_string(),
            offset,
        },
    });
}

fn proportional_text_index(text: &str, rect: UiRect, point: [f32; 2]) -> usize {
    let length = text.chars().count();
    if length == 0 || rect.width <= f32::EPSILON {
        return 0;
    }
    let progress = ((point[0] - rect.x) / rect.width).clamp(0.0, 1.0);
    (progress * length as f32).round() as usize
}

fn word_range_at(text: &str, index: usize) -> std::ops::Range<usize> {
    let chars = text.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return 0..0;
    }
    let index = index.min(chars.len() - 1);
    let word_character = |character: char| character.is_alphanumeric() || character == '_';
    let kind = word_character(chars[index]);
    let mut start = index;
    while start > 0 && word_character(chars[start - 1]) == kind {
        start -= 1;
    }
    let mut end = index + 1;
    while end < chars.len() && word_character(chars[end]) == kind {
        end += 1;
    }
    start..end
}

fn dispatch_toggle_value(root: &UiNode, target_id: &str, dispatched: &mut Vec<UiDispatchedAction>) {
    let Some(toggle) = find_node(root, target_id).and_then(|node| node.control.toggle()) else {
        return;
    };
    dispatched.push(UiDispatchedAction {
        target_id: target_id.to_string(),
        event: UiEventKind::Click,
        action: UiAction::SetToggle {
            key: toggle.value_key.clone(),
            value: !toggle.value,
        },
    });
}

fn node_handles_key(root: &UiNode, id: &str, key: &str) -> bool {
    find_node(root, id).map_or(false, |node| {
        node.event_handlers.iter().any(|binding| {
            matches!(&binding.event, UiEventKind::KeyPress(binding_key) if binding_key.eq_ignore_ascii_case(key))
        })
    })
}

fn dispatch_range_value(
    root: &UiNode,
    hit_regions: &[UiHitRegion],
    target_id: &str,
    pointer_position: Option<[f32; 2]>,
    event: UiEventKind,
    dispatched: &mut Vec<UiDispatchedAction>,
) {
    let (Some(range), Some(pointer_position), Some(region)) = (
        find_node(root, target_id).and_then(|node| node.control.range()),
        pointer_position,
        hit_regions.iter().find(|region| region.id == target_id),
    ) else {
        return;
    };
    let fraction = if region.rect.width <= f32::EPSILON {
        0.0
    } else {
        (pointer_position[0] - region.rect.x) / region.rect.width
    };
    dispatched.push(UiDispatchedAction {
        target_id: target_id.to_string(),
        event,
        action: UiAction::SetRange {
            key: range.value_key.clone(),
            value: range.value_from_fraction(fraction),
        },
    });
}

fn find_scroll_container<'a>(
    node: &'a UiNode,
    target_id: &str,
) -> Option<(&'a str, crate::UiScrollAxis)> {
    fn visit<'a>(
        node: &'a UiNode,
        target_id: &str,
        nearest: Option<(&'a str, crate::UiScrollAxis)>,
    ) -> Option<(&'a str, crate::UiScrollAxis)> {
        let nearest = node
            .control
            .scroll_axis()
            .map(|axis| (node.id.as_str(), axis))
            .or(nearest);
        if node.id == target_id {
            return nearest;
        }
        node.children
            .iter()
            .find_map(|child| visit(child, target_id, nearest))
    }
    visit(node, target_id, None)
}

/// Finds the deepest scroll view whose viewport contains the pointer.
///
/// Scrollable transcripts commonly contain only labels and passive panels.
/// Those nodes intentionally are not interactive, but the scroll view must
/// still receive the wheel while the pointer is over them. This lookup is
/// separate from interactive hit testing so buttons inside a scroll view keep
/// their normal click ownership.
fn find_scroll_container_at<'a>(
    node: &'a UiNode,
    hit_regions: &[UiHitRegion],
    point: [f32; 2],
) -> Option<(&'a str, crate::UiScrollAxis)> {
    if let Some(container) = node
        .children
        .iter()
        .find_map(|child| find_scroll_container_at(child, hit_regions, point))
    {
        return Some(container);
    }

    let axis = node.control.scroll_axis()?;
    let region = hit_regions.iter().find(|region| region.id == node.id)?;
    (region.rect.contains(point) && region.clip_rect.contains(point))
        .then_some((node.id.as_str(), axis))
}

fn dispatch(
    root: &UiNode,
    target_id: &str,
    event: UiEventKind,
    dispatched: &mut Vec<UiDispatchedAction>,
) {
    let Some(node) = find_node(root, target_id) else {
        return;
    };
    if node.disabled {
        return;
    }

    for binding in &node.event_handlers {
        if event_matches(&binding.event, &event) {
            dispatched.push(UiDispatchedAction {
                target_id: target_id.to_string(),
                event: event.clone(),
                action: binding.action.clone(),
            });
        }
    }
}

fn captured_actions(root: &UiNode, target_id: &str, event: &UiEventKind) -> Vec<UiAction> {
    find_node(root, target_id)
        .into_iter()
        .flat_map(|node| node.event_handlers.iter())
        .filter(|binding| event_matches(&binding.event, event))
        .map(|binding| binding.action.clone())
        .collect()
}

fn event_matches(binding: &UiEventKind, event: &UiEventKind) -> bool {
    match (binding, event) {
        (UiEventKind::KeyPress(left), UiEventKind::KeyPress(right)) => {
            left.eq_ignore_ascii_case(right)
        }
        _ => binding == event,
    }
}

fn dispatch_captured(
    root: &UiNode,
    target_id: &str,
    event: UiEventKind,
    captured: &[UiAction],
    dispatched: &mut Vec<UiDispatchedAction>,
) {
    let before = dispatched.len();
    dispatch(root, target_id, event.clone(), dispatched);
    if dispatched.len() == before {
        dispatched.extend(captured.iter().cloned().map(|action| UiDispatchedAction {
            target_id: target_id.to_string(),
            event: event.clone(),
            action,
        }));
    }
}

fn find_node<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
    if node.id == id {
        return Some(node);
    }
    node.children.iter().find_map(|child| find_node(child, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::UiEventBinding;
    use crate::geometry::UiRect;
    use crate::layout::UiLayout;
    use crate::node::UiNodeKind;

    fn button(id: &str, command: &str) -> UiNode {
        UiNode::new(id, UiNodeKind::Button)
            .focusable()
            .with_event(UiEventBinding::command(UiEventKind::Click, command))
    }

    fn region(id: &str, x: f32) -> UiHitRegion {
        UiHitRegion {
            id: id.to_string(),
            kind: UiNodeKind::Button,
            rect: UiRect::new(x, 0.0, 80.0, 30.0),
            clip_rect: UiRect::new(x, 0.0, 80.0, 30.0),
            z_index: 1,
            interactive: true,
            focusable: true,
            disabled: false,
        }
    }

    fn text_region(id: &str, width: f32) -> UiHitRegion {
        UiHitRegion {
            id: id.to_string(),
            kind: UiNodeKind::TextInput,
            rect: UiRect::new(0.0, 0.0, width, 30.0),
            clip_rect: UiRect::new(0.0, 0.0, width, 30.0),
            z_index: 1,
            interactive: true,
            focusable: true,
            disabled: false,
        }
    }

    #[test]
    fn click_dispatches_only_after_pointer_release_on_same_target() {
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_layout(UiLayout::default())
            .with_child(button("save", "file.save"));
        let regions = vec![region("save", 0.0)];
        let policy = UiFocusPolicy {
            tab_order: vec!["save".to_string()],
            wrap: true,
        };
        let mut state = UiInteractionState::default();

        let press = UiInputState {
            pointer_position: Some([12.0, 12.0]),
            pointer_down: true,
            ..UiInputState::default()
        };
        assert!(state.update(&root, &regions, &press, &policy).is_empty());
        assert!(state.focus.has_focus("save"));

        let release = UiInputState {
            pointer_position: Some([12.0, 12.0]),
            ..UiInputState::default()
        };
        let actions = state.update(&root, &regions, &release, &policy);

        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].action,
            UiAction::Command {
                name: "file.save".to_string()
            }
        );
    }

    #[test]
    fn repeated_pointer_release_dispatches_generic_double_click() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            button("node", "node.select").with_event(UiEventBinding {
                event: UiEventKind::DoubleClick,
                action: UiAction::Command {
                    name: "node.rename".to_string(),
                },
            }),
        );
        let regions = vec![region("node", 0.0)];
        let policy = UiFocusPolicy::default();
        let mut state = UiInteractionState::default();

        for time in [1.0_f64, 1.2_f64] {
            state.update(
                &root,
                &regions,
                &UiInputState {
                    pointer_position: Some([12.0, 12.0]),
                    pointer_down: true,
                    time_seconds: time,
                    ..UiInputState::default()
                },
                &policy,
            );
            let actions = state.update(
                &root,
                &regions,
                &UiInputState {
                    pointer_position: Some([12.0, 12.0]),
                    time_seconds: time + 0.01,
                    ..UiInputState::default()
                },
                &policy,
            );
            if time > 1.0 {
                assert!(actions.iter().any(|action| {
                    action.action
                        == UiAction::Command {
                            name: "node.rename".to_string(),
                        }
                }));
            }
        }
    }

    #[test]
    fn tab_uses_declared_focus_order() {
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_child(button("first", "first"))
            .with_child(button("second", "second"));
        let policy = UiFocusPolicy {
            tab_order: vec!["first".to_string(), "second".to_string()],
            wrap: true,
        };
        let mut state = UiInteractionState::default();

        state.update(
            &root,
            &[],
            &UiInputState {
                pressed_keys: vec!["Tab".to_string()],
                ..UiInputState::default()
            },
            &policy,
        );
        assert!(state.focus.has_focus("first"));

        state.update(
            &root,
            &[],
            &UiInputState {
                pressed_keys: vec!["Tab".to_string()],
                ..UiInputState::default()
            },
            &policy,
        );
        assert!(state.focus.has_focus("second"));
    }

    #[test]
    fn secondary_click_dispatches_context_menu_and_text_targets_focus() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            button("input", "input.click")
                .with_event(UiEventBinding::command(
                    UiEventKind::ContextMenu,
                    "input.menu",
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::TextInput("hello".to_string()),
                    "input.text",
                )),
        );
        let regions = vec![region("input", 0.0)];
        let mut state = UiInteractionState::default();

        let menu_actions = state.update(
            &root,
            &regions,
            &UiInputState {
                pointer_position: Some([12.0, 12.0]),
                pointer_pressed_buttons: vec![UiPointerButton::Secondary],
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        assert!(menu_actions.iter().any(|action| matches!(
            action.action,
            UiAction::Command { ref name } if name == "input.menu"
        )));

        let text_actions = state.update(
            &root,
            &regions,
            &UiInputState {
                pointer_position: Some([12.0, 12.0]),
                text_input: "hello".to_string(),
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        assert!(text_actions.iter().any(|action| matches!(
            action.action,
            UiAction::Command { ref name } if name == "input.text"
        )));
    }

    #[test]
    fn text_input_updates_transient_value_and_emits_set_text() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
            "query",
            crate::UiTextInput::new("hub.query"),
        ));
        let mut state = UiInteractionState::default();
        state.focus.request_focus("query");

        let actions = state.update(
            &root,
            &[region("query", 0.0)],
            &UiInputState {
                text_input: "raf".to_string(),
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );

        assert_eq!(state.controls.text("hub.query"), "raf");
        assert!(actions.iter().any(|action| matches!(
            action.action,
            UiAction::SetText { ref key, ref value } if key == "hub.query" && value == "raf"
        )));
    }

    #[test]
    fn ctrl_a_selects_all_text_in_the_focused_input() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
            "query",
            crate::UiTextInput::new("query.value"),
        ));
        let mut state = UiInteractionState::default();
        state.controls.set_text("query.value", "select me", 64);
        state.focus.request_focus("query");

        state.update(
            &root,
            &[text_region("query", 160.0)],
            &UiInputState {
                pressed_keys: vec!["A".to_string()],
                modifiers: crate::UiModifiers {
                    control: true,
                    ..crate::UiModifiers::default()
                },
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );

        assert_eq!(state.controls.text_edit("query.value").selection(), 0..9);
    }

    #[test]
    fn dragging_text_input_selects_between_pointer_positions() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
            "query",
            crate::UiTextInput::new("query.value"),
        ));
        let regions = [text_region("query", 120.0)];
        let mut state = UiInteractionState::default();
        state.controls.set_text("query.value", "hello world", 64);

        state.update(
            &root,
            &regions,
            &UiInputState {
                pointer_position: Some([0.0, 12.0]),
                pointer_down: true,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        state.update(
            &root,
            &regions,
            &UiInputState {
                pointer_position: Some([110.0, 12.0]),
                pointer_down: true,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );

        assert!(state.controls.text_edit("query.value").has_selection());
        assert_eq!(state.controls.text_edit("query.value").selection(), 0..10);
    }

    #[test]
    fn double_click_selects_the_word_under_the_pointer() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
            "query",
            crate::UiTextInput::new("query.value"),
        ));
        let regions = [text_region("query", 120.0)];
        let mut state = UiInteractionState::default();
        state.controls.set_text("query.value", "hello world", 64);

        for (time_seconds, down) in [(0.0, true), (0.1, false), (0.25, true)] {
            state.update(
                &root,
                &regions,
                &UiInputState {
                    pointer_position: Some([12.0, 12.0]),
                    pointer_down: down,
                    time_seconds,
                    ..UiInputState::default()
                },
                &UiFocusPolicy::default(),
            );
        }

        assert_eq!(state.controls.text_edit("query.value").selection(), 0..5);
    }

    #[test]
    fn cursor_hint_distinguishes_textboxes_from_actionable_controls() {
        let text_root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
            "query",
            crate::UiTextInput::new("query.value"),
        ));
        let mut text_state = UiInteractionState::default();
        text_state.focus.set_hovered(Some("query".to_string()));
        assert_eq!(
            text_state.cursor_hint(&text_root),
            crate::UiCursorIcon::Text
        );

        let button_root = UiNode::new("root", UiNodeKind::Root).with_child(button("save", "save"));
        let mut button_state = UiInteractionState::default();
        button_state.focus.set_hovered(Some("save".to_string()));
        assert_eq!(
            button_state.cursor_hint(&button_root),
            crate::UiCursorIcon::PointingHand
        );

        let panel_root = UiNode::new("root", UiNodeKind::Root)
            .with_child(UiNode::new("settings", UiNodeKind::Panel).focusable());
        let mut panel_state = UiInteractionState::default();
        panel_state.focus.set_hovered(Some("settings".to_string()));
        assert_eq!(
            panel_state.cursor_hint(&panel_root),
            crate::UiCursorIcon::Default
        );
    }

    #[test]
    fn focused_retained_control_captures_editor_keyboard_input() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
            "query",
            crate::UiTextInput::new("query.value"),
        ));
        let mut state = UiInteractionState::default();
        state.focus.request_focus("query");

        assert!(state.captures_keyboard_input(&root));
        state.focus.clear_focus();
        assert!(!state.captures_keyboard_input(&root));
    }

    #[test]
    fn focused_button_does_not_capture_viewport_keyboard_input() {
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_child(UiNode::new("save", UiNodeKind::Button).focusable());
        let mut state = UiInteractionState::default();
        state.focus.request_focus("save");

        assert!(!state.captures_keyboard_input(&root));
    }

    #[test]
    fn outside_primary_press_releases_retained_keyboard_focus() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
            "query",
            crate::UiTextInput::new("query.value"),
        ));
        let mut state = UiInteractionState::default();
        state.focus.request_focus("query");

        state.update(
            &root,
            &[],
            &UiInputState {
                pointer_pressed_outside: true,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );

        assert!(!state.captures_keyboard_input(&root));
    }

    #[test]
    fn drag_end_survives_when_the_source_node_is_rebuilt_away() {
        let drag_root = UiNode::new("root", UiNodeKind::Root).with_child(
            button("tab", "tab.click")
                .with_event(UiEventBinding::command(UiEventKind::DragStart, "tab.start"))
                .with_event(UiEventBinding::command(UiEventKind::DragMove, "tab.move"))
                .with_event(UiEventBinding::command(UiEventKind::DragEnd, "tab.end")),
        );
        let empty_root = UiNode::new("root", UiNodeKind::Root);
        let mut state = UiInteractionState::default();

        state.update(
            &drag_root,
            &[region("tab", 0.0)],
            &UiInputState {
                pointer_position: Some([12.0, 12.0]),
                pointer_down: true,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        let moving = state.update(
            &empty_root,
            &[],
            &UiInputState {
                pointer_position: Some([24.0, 12.0]),
                pointer_down: true,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        assert!(moving.iter().any(|action| matches!(
            action.action,
            UiAction::Command { ref name } if name == "tab.start"
        )));
        assert!(moving.iter().any(|action| matches!(
            action.action,
            UiAction::Command { ref name } if name == "tab.move"
        )));

        let released = state.update(
            &empty_root,
            &[],
            &UiInputState {
                pointer_position: Some([24.0, 12.0]),
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        assert!(released.iter().any(|action| matches!(
            action.action,
            UiAction::Command { ref name } if name == "tab.end"
        )));
    }

    #[test]
    fn scroll_view_moves_state_from_a_descendant_hit() {
        let root = UiNode::scroll_view("list", crate::UiScrollAxis::Vertical)
            .with_child(UiNode::new("list.item", UiNodeKind::Panel).interactive());
        let mut state = UiInteractionState::default();

        let actions = state.update(
            &root,
            &[region("list.item", 0.0)],
            &UiInputState {
                pointer_position: Some([12.0, 12.0]),
                scroll_delta: [0.0, 24.0],
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );

        assert_eq!(state.controls.scroll_offset("list"), [0.0, 24.0]);
        assert!(actions.iter().any(|action| matches!(
            action.action,
            UiAction::ScrollTo { ref id, offset } if id == "list" && offset == [0.0, 24.0]
        )));
    }

    #[test]
    fn scroll_view_accepts_wheel_over_passive_content() {
        let root = UiNode::scroll_view("list", crate::UiScrollAxis::Vertical)
            .with_child(UiNode::new("list.message", UiNodeKind::Panel));
        let regions = vec![
            UiHitRegion {
                id: "list".to_string(),
                kind: UiNodeKind::ScrollView,
                rect: UiRect::new(0.0, 0.0, 240.0, 120.0),
                clip_rect: UiRect::new(0.0, 0.0, 240.0, 120.0),
                z_index: 0,
                interactive: false,
                focusable: false,
                disabled: false,
            },
            UiHitRegion {
                id: "list.message".to_string(),
                kind: UiNodeKind::Panel,
                rect: UiRect::new(0.0, 0.0, 240.0, 300.0),
                clip_rect: UiRect::new(0.0, 0.0, 240.0, 120.0),
                z_index: -1,
                interactive: false,
                focusable: false,
                disabled: false,
            },
        ];
        let mut state = UiInteractionState::default();
        state.controls.set_scroll_metrics("list", [0.0, 180.0]);

        let actions = state.update(
            &root,
            &regions,
            &UiInputState {
                pointer_position: Some([120.0, 80.0]),
                scroll_delta: [0.0, 48.0],
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );

        assert_eq!(state.controls.scroll_offset("list"), [0.0, 48.0]);
        assert!(actions.iter().any(|action| matches!(
            action.action,
            UiAction::ScrollTo { ref id, offset } if id == "list" && offset == [0.0, 48.0]
        )));
    }

    #[test]
    fn manual_scrollbar_thumb_drag_updates_the_retained_scroll_offset() {
        let root = UiNode::scroll_view("list", crate::UiScrollAxis::Vertical);
        let regions = vec![
            UiHitRegion {
                id: "__rafui.scrollbar.list.track".to_string(),
                kind: UiNodeKind::Panel,
                rect: UiRect::new(230.0, 0.0, 10.0, 120.0),
                clip_rect: UiRect::new(0.0, 0.0, 240.0, 120.0),
                z_index: 4,
                interactive: true,
                focusable: false,
                disabled: false,
            },
            UiHitRegion {
                id: "__rafui.scrollbar.list.thumb".to_string(),
                kind: UiNodeKind::Panel,
                rect: UiRect::new(230.0, 0.0, 10.0, 30.0),
                clip_rect: UiRect::new(0.0, 0.0, 240.0, 120.0),
                z_index: 5,
                interactive: true,
                focusable: false,
                disabled: false,
            },
        ];
        let mut state = UiInteractionState::default();
        state.controls.set_scroll_metrics("list", [0.0, 180.0]);

        state.update(
            &root,
            &regions,
            &UiInputState {
                pointer_position: Some([235.0, 10.0]),
                pointer_down: true,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        let actions = state.update(
            &root,
            &regions,
            &UiInputState {
                pointer_position: Some([235.0, 80.0]),
                pointer_down: true,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );

        assert_eq!(state.controls.scroll_offset("list"), [0.0, 140.0]);
        assert!(actions.iter().any(|action| matches!(
            action.action,
            UiAction::ScrollTo { ref id, offset } if id == "list" && offset == [0.0, 140.0]
        )));
    }

    #[test]
    fn toggle_emits_the_next_boolean_value() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::toggle(
            "enabled",
            crate::UiToggle::new("settings.enabled", false),
        ));
        let mut state = UiInteractionState::default();
        let regions = vec![region("enabled", 0.0)];

        state.update(
            &root,
            &regions,
            &UiInputState {
                pointer_position: Some([12.0, 12.0]),
                pointer_down: true,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        let actions = state.update(
            &root,
            &regions,
            &UiInputState {
                pointer_position: Some([12.0, 12.0]),
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );

        assert!(actions.iter().any(|action| matches!(
            action.action,
            UiAction::SetToggle { ref key, value } if key == "settings.enabled" && value
        )));
    }

    #[test]
    fn range_maps_pointer_position_to_a_stepped_value() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::range(
            "scale",
            crate::UiRange::new("settings.scale", 0.0, 0.0, 100.0, 5.0),
        ));
        let mut state = UiInteractionState::default();
        let actions = state.update(
            &root,
            &[region("scale", 0.0)],
            &UiInputState {
                pointer_position: Some([62.0, 12.0]),
                pointer_down: true,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );

        assert!(actions.iter().any(|action| matches!(
            action.action,
            UiAction::SetRange { ref key, value } if key == "settings.scale" && value == 80.0
        )));
    }
}
