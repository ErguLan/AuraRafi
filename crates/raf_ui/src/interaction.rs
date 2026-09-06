use crate::controls::{UiColorPickerHit, UiControl};
use crate::events::{UiAction, UiCursorIcon, UiEventKind, UiPointerButton};
use crate::focus::{UiFocusPolicy, UiFocusState, UiInputState, UiModifiers};
use crate::geometry::UiRect;
use crate::hit_test::{hit_test, UiHitRegion, UiHitTestMode};
use crate::node::{UiAccessibilityRole, UiNode, UiNodeKind};
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
    select_typeahead_target: Option<String>,
    select_typeahead: String,
    select_typeahead_at_seconds: f64,
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

    /// Returns whether a held text-editing key needs another frame for
    /// keyboard repeat. Native hosts use this to keep event-driven windows
    /// alive while a key remains down.
    pub fn has_active_text_repeat(&self) -> bool {
        self.text_repeat_key.is_some()
    }

    /// Returns whether this retained surface currently owns text-entry input.
    ///
    /// Text inputs and focused actionable controls own the small keyboard
    /// boundary needed for native activation. Passive panels deliberately do
    /// not capture the editor's global navigation shortcuts.
    pub fn captures_keyboard_input(&self, root: &UiNode) -> bool {
        self.focus
            .focused
            .as_deref()
            .and_then(|id| find_node(root, id))
            .is_some_and(|node| {
                node.focusable
                    && !node.disabled
                    && (node.control.text_input().is_some()
                        || matches!(node.kind, UiNodeKind::Button | UiNodeKind::TextInput)
                        || node.text_selectable
                        || matches!(
                            &node.control,
                            UiControl::Toggle(_)
                                | UiControl::Range(_)
                                | UiControl::Select(_)
                                | UiControl::ColorPicker(_)
                        ))
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
        self.select_typeahead_target = None;
        self.select_typeahead.clear();
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
        if node
            .classes
            .iter()
            .any(|class| class == "settings-modal-resize-handle")
        {
            return UiCursorIcon::ResizeNorthWestSouthEast;
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
        let value_control = matches!(
            &node.control,
            UiControl::Toggle(_)
                | UiControl::Range(_)
                | UiControl::ColorPicker(_)
                | UiControl::Select(_)
        );
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
        text_hit_test: F,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str, &str, [f32; 2]) -> Option<usize>,
    {
        self.update_with_text_hit_test_and_vertical(
            root,
            hit_regions,
            input,
            focus_policy,
            text_hit_test,
            |_, _| Vec::new(),
        )
    }

    /// Variant that also supplies visual line ranges for multiline keyboard
    /// navigation. Keeping the old method above preserves the backend-neutral
    /// API for callers that only need proportional hit testing.
    pub fn update_with_text_hit_test_and_vertical<F, G>(
        &mut self,
        root: &UiNode,
        hit_regions: &[UiHitRegion],
        input: &UiInputState,
        focus_policy: &UiFocusPolicy,
        mut text_hit_test: F,
        mut visual_lines: G,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str, &str, [f32; 2]) -> Option<usize>,
        G: FnMut(&str, &str) -> Vec<(usize, usize)>,
    {
        self.last_modifiers = input.modifiers;
        if self
            .text_repeat_key
            .as_deref()
            .is_some_and(|key| !input.key_down(key))
        {
            self.text_repeat_key = None;
            self.text_repeat_next_seconds = 0.0;
        }
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
            self.select_typeahead_target = None;
            self.select_typeahead.clear();
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
        // A native event loop can deliver a press and its matching release
        // before the next redraw. Use the explicit transient edges as well as
        // the held-button state so a short click is never lost when that
        // happens. The derived edges keep the legacy `pointer_down` API fully
        // compatible with headless callers and existing embedders.
        let primary_pressed = input.button_pressed(UiPointerButton::Primary)
            || (primary_down && !self.pointer_was_down);
        let primary_released = input.button_released(UiPointerButton::Primary)
            || (!primary_down && self.pointer_was_down);
        if primary_pressed {
            dispatch_select_click_away(root, hovered_id.as_deref(), &mut dispatched);
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
                } else if let Some(text) = selectable_text_for_node(root, &hit.id) {
                    if let Some(point) = input.pointer_position {
                        let index = text_hit_test(&hit.id, text, point)
                            .unwrap_or_else(|| proportional_text_index(text, hit.rect, point));
                        let is_double_click = self.last_text_click.as_ref().is_some_and(|last| {
                            last.target_id == hit.id
                                && input.time_seconds - last.time_seconds >= 0.0
                                && input.time_seconds - last.time_seconds
                                    <= DOUBLE_CLICK_TIME_SECONDS
                                && distance_squared(last.position, point)
                                    <= DOUBLE_CLICK_DISTANCE_PX * DOUBLE_CLICK_DISTANCE_PX
                        });
                        if is_double_click {
                            let range = word_range_at(text, index);
                            self.controls
                                .set_selectable_cursor(&hit.id, text, range.start, false);
                            self.controls
                                .set_selectable_cursor(&hit.id, text, range.end, true);
                            self.text_selection_target = Some(hit.id.clone());
                            self.last_text_click = None;
                        } else {
                            self.controls
                                .set_selectable_cursor(&hit.id, text, index, false);
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
                    if find_node(root, &hit.id)
                        .and_then(|node| node.control.select())
                        .is_none()
                    {
                        self.select_typeahead_target = None;
                        self.select_typeahead.clear();
                    }
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
                dispatch_color_picker_value(
                    root,
                    hit_regions,
                    &hit.id,
                    input.pointer_position,
                    UiEventKind::PointerDown(UiPointerButton::Primary),
                    &mut dispatched,
                );
            }
        }
        if primary_down && self.pointer_was_down && pointer_moved {
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
                } else if let Some(text) = selectable_text_for_node(root, text_target) {
                    let index = text_hit_test(text_target, text, point).or_else(|| {
                        hit_regions
                            .iter()
                            .find(|region| region.id == text_target)
                            .map(|region| proportional_text_index(text, region.rect, point))
                    });
                    if let Some(index) = index {
                        self.controls
                            .set_selectable_cursor(text_target, text, index, true);
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
                    dispatch_color_picker_value(
                        root,
                        hit_regions,
                        id,
                        input.pointer_position,
                        UiEventKind::DragMove,
                        &mut dispatched,
                    );
                }
            }
        }
        if primary_released {
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
                    dispatch_select_open(root, id, &mut dispatched);
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
            if let Some(_tab) = find_node(root, &focused)
                .filter(|node| node.accessibility_role == UiAccessibilityRole::Tab)
            {
                if input.key_pressed("enter") || input.key_pressed("space") {
                    dispatch(root, &focused, UiEventKind::Click, &mut dispatched);
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
                        dispatch(root, &focused, UiEventKind::Click, &mut dispatched);
                    }
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

        // `cancel_pointer_gesture` clears this during a release. Do not
        // overwrite that result when a coalesced press/release pair reports
        // the final physical state as still down.
        self.pointer_was_down = if primary_released {
            false
        } else {
            primary_down
        };
        self.last_pointer_position = input.pointer_position;
        dispatched
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
    // Keep words, whitespace and punctuation as separate tokens. Treating
    // every non-alphanumeric character as one bucket made a double-click on
    // punctuation select an arbitrary run of spaces and symbols together.
    let token_kind = |character: char| {
        if character.is_whitespace() {
            0u8
        } else if character.is_alphanumeric() || character == '_' {
            1u8
        } else {
            2u8
        }
    };
    let kind = token_kind(chars[index]);
    let mut start = index;
    while start > 0 && token_kind(chars[start - 1]) == kind {
        start -= 1;
    }
    let mut end = index + 1;
    while end < chars.len() && token_kind(chars[end]) == kind {
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

fn dispatch_select_open(root: &UiNode, target_id: &str, dispatched: &mut Vec<UiDispatchedAction>) {
    let Some(select) = find_node(root, target_id).and_then(|node| node.control.select()) else {
        return;
    };
    dispatched.push(UiDispatchedAction {
        target_id: target_id.to_string(),
        event: UiEventKind::Click,
        action: UiAction::SetSelectOpen {
            id: target_id.to_string(),
            open: !select.open,
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
    let fraction = match range.orientation {
        crate::UiRangeOrientation::Horizontal => {
            if region.rect.width <= f32::EPSILON {
                0.0
            } else {
                (pointer_position[0] - region.rect.x) / region.rect.width
            }
        }
        crate::UiRangeOrientation::Vertical => {
            if region.rect.height <= f32::EPSILON {
                0.0
            } else {
                1.0 - (pointer_position[1] - region.rect.y) / region.rect.height
            }
        }
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

fn dispatch_color_picker_value(
    root: &UiNode,
    hit_regions: &[UiHitRegion],
    target_id: &str,
    pointer_position: Option<[f32; 2]>,
    event: UiEventKind,
    dispatched: &mut Vec<UiDispatchedAction>,
) {
    let (Some(picker), Some(pointer_position), Some(region)) = (
        find_node(root, target_id).and_then(|node| node.control.color_picker()),
        pointer_position,
        hit_regions.iter().find(|region| region.id == target_id),
    ) else {
        return;
    };
    if region.rect.width <= f32::EPSILON || region.rect.height <= f32::EPSILON {
        return;
    }
    let local_position = [
        pointer_position[0] - region.rect.x,
        pointer_position[1] - region.rect.y,
    ];
    let Some(hit) = picker.hit_test(local_position, [region.rect.width, region.rect.height]) else {
        return;
    };
    let (hue, saturation, value) = match hit {
        UiColorPickerHit::Hue(hue) => (hue, picker.saturation, picker.value),
        UiColorPickerHit::SaturationValue { saturation, value } => (picker.hue, saturation, value),
    };
    dispatched.push(UiDispatchedAction {
        target_id: target_id.to_string(),
        event,
        action: UiAction::SetColorHsv {
            key: picker.value_key.clone(),
            hue,
            saturation,
            value,
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

fn dispatch_select_click_away(
    root: &UiNode,
    hovered_id: Option<&str>,
    dispatched: &mut Vec<UiDispatchedAction>,
) {
    let mut open_selects = Vec::new();
    collect_open_selects(root, &mut open_selects);
    for (select_id, popup_id) in open_selects {
        let inside_trigger = hovered_id.is_some_and(|target_id| {
            root.find(&select_id)
                .is_some_and(|node| node_contains_id(node, target_id))
        });
        let inside_popup = popup_id.as_deref().is_some_and(|popup_id| {
            hovered_id.is_some_and(|target_id| {
                root.find(popup_id)
                    .is_some_and(|node| node_contains_id(node, target_id))
            })
        });
        if !inside_trigger && !inside_popup {
            dispatched.push(UiDispatchedAction {
                target_id: select_id.clone(),
                event: UiEventKind::PointerDown(UiPointerButton::Primary),
                action: UiAction::SetSelectOpen {
                    id: select_id,
                    open: false,
                },
            });
        }
    }
}

fn collect_open_selects(node: &UiNode, output: &mut Vec<(String, Option<String>)>) {
    if let Some(select) = node.control.select().filter(|select| select.open) {
        output.push((node.id.clone(), select.popup_id.clone()));
    }
    for child in &node.children {
        collect_open_selects(child, output);
    }
}

fn node_contains_id(node: &UiNode, target_id: &str) -> bool {
    node.id == target_id
        || node
            .children
            .iter()
            .any(|child| node_contains_id(child, target_id))
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

fn selectable_text_for_node<'a>(root: &'a UiNode, id: &str) -> Option<&'a str> {
    find_node(root, id)
        .filter(|node| node.text_selectable)
        .and_then(|node| node.text_value.as_deref())
}

fn collect_focusable_tabs(node: &UiNode, ids: &mut Vec<String>) {
    if node.accessibility_role == UiAccessibilityRole::Tab && node.focusable && !node.disabled {
        ids.push(node.id.clone());
    }
    for child in &node.children {
        collect_focusable_tabs(child, ids);
    }
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
    fn coalesced_press_and_release_still_dispatches_a_click() {
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_layout(UiLayout::default())
            .with_child(button("save", "file.save"));
        let regions = vec![region("save", 0.0)];
        let mut state = UiInteractionState::default();

        let actions = state.update(
            &root,
            &regions,
            &UiInputState {
                pointer_position: Some([12.0, 12.0]),
                pointer_pressed_buttons: vec![UiPointerButton::Primary],
                pointer_released_buttons: vec![UiPointerButton::Primary],
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );

        assert!(actions.iter().any(|action| {
            action.action
                == UiAction::Command {
                    name: "file.save".to_string(),
                }
        }));
        assert!(!state.has_pointer_capture());
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
    fn dragging_selectable_text_selects_it_and_ctrl_c_emits_clipboard_action() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::new("message", UiNodeKind::Label)
                .with_text_value("hello agent")
                .selectable_text(),
        );
        let regions = [UiHitRegion {
            id: "message".to_string(),
            kind: UiNodeKind::Label,
            rect: UiRect::new(0.0, 0.0, 120.0, 30.0),
            clip_rect: UiRect::new(0.0, 0.0, 120.0, 30.0),
            z_index: 1,
            interactive: true,
            focusable: true,
            disabled: false,
        }];
        let mut state = UiInteractionState::default();

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
                pointer_position: Some([55.0, 12.0]),
                pointer_down: true,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );

        assert_eq!(
            state
                .controls
                .selected_selectable_text("message", "hello agent"),
            "hello"
        );

        let actions = state.update(
            &root,
            &regions,
            &UiInputState {
                pressed_keys: vec!["C".to_string()],
                modifiers: UiModifiers {
                    control: true,
                    ..UiModifiers::default()
                },
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        assert!(actions.iter().any(|action| matches!(
            &action.action,
            UiAction::SetClipboard { text } if text == "hello"
        )));
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
    fn held_delete_repeats_until_release() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
            "query",
            crate::UiTextInput::new("query.value"),
        ));
        let regions = [text_region("query", 120.0)];
        let mut state = UiInteractionState::default();
        state.controls.set_text("query.value", "abcd", 64);
        state.controls.set_cursor("query.value", 0, false);
        state.focus.request_focus("query");

        state.update(
            &root,
            &regions,
            &UiInputState {
                pressed_keys: vec!["delete".to_string()],
                keys_down: vec!["delete".to_string()],
                time_seconds: 0.0,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        assert_eq!(state.controls.text("query.value"), "bcd");

        state.update(
            &root,
            &regions,
            &UiInputState {
                keys_down: vec!["delete".to_string()],
                time_seconds: 0.46,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        assert_eq!(state.controls.text("query.value"), "cd");

        state.update(
            &root,
            &regions,
            &UiInputState {
                time_seconds: 0.5,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        assert!(!state.has_active_text_repeat());
    }

    #[test]
    fn delete_consumes_coalesced_press_edges_and_keeps_repeating() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
            "query",
            crate::UiTextInput::new("query.value"),
        ));
        let regions = [text_region("query", 120.0)];
        let mut state = UiInteractionState::default();
        state.controls.set_text("query.value", "abcdef", 64);
        state.controls.set_cursor("query.value", 0, false);
        state.focus.request_focus("query");

        state.update(
            &root,
            &regions,
            &UiInputState {
                // Two press/release cycles can arrive before one redraw. A
                // boolean key query must not collapse them into one edit.
                pressed_keys: vec!["delete".to_string(), "delete".to_string()],
                keys_down: vec!["delete".to_string()],
                time_seconds: 0.0,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        assert_eq!(state.controls.text("query.value"), "cdef");

        state.update(
            &root,
            &regions,
            &UiInputState {
                keys_down: vec!["delete".to_string()],
                time_seconds: 0.46,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        assert_eq!(state.controls.text("query.value"), "def");

        state.update(
            &root,
            &regions,
            &UiInputState {
                keys_down: vec!["delete".to_string()],
                time_seconds: 0.50,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        assert_eq!(state.controls.text("query.value"), "ef");

        state.update(
            &root,
            &regions,
            &UiInputState {
                time_seconds: 0.51,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        assert!(!state.has_active_text_repeat());
    }

    #[test]
    fn backspace_consumes_coalesced_press_edges() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::text_input(
            "query",
            crate::UiTextInput::new("query.value"),
        ));
        let regions = [text_region("query", 120.0)];
        let mut state = UiInteractionState::default();
        state.controls.set_text("query.value", "abcdef", 64);
        state.controls.set_cursor("query.value", 6, false);
        state.focus.request_focus("query");

        state.update(
            &root,
            &regions,
            &UiInputState {
                pressed_keys: vec!["backspace".to_string(), "backspace".to_string()],
                keys_down: vec!["backspace".to_string()],
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );

        assert_eq!(state.controls.text("query.value"), "abcd");
    }

    #[test]
    fn double_click_keeps_punctuation_separate_from_words_and_spaces() {
        assert_eq!(word_range_at("hello,  world", 1), 0..5);
        assert_eq!(word_range_at("hello,  world", 5), 5..6);
        assert_eq!(word_range_at("hello,  world", 6), 6..8);
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

        let resize_root = UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::new("resize", UiNodeKind::Button).with_class("settings-modal-resize-handle"),
        );
        let mut resize_state = UiInteractionState::default();
        resize_state.focus.set_hovered(Some("resize".to_string()));
        assert_eq!(
            resize_state.cursor_hint(&resize_root),
            crate::UiCursorIcon::ResizeNorthWestSouthEast
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
    fn focused_button_captures_keyboard_input_for_activation() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(button("save", "file.save"));
        let mut state = UiInteractionState::default();
        state.focus.request_focus("save");

        assert!(state.captures_keyboard_input(&root));

        let actions = state.update(
            &root,
            &[],
            &UiInputState {
                pressed_keys: vec!["enter".to_string()],
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        assert!(actions.iter().any(|action| matches!(
            action.action,
            UiAction::Command { ref name } if name == "file.save"
        )));
    }

    #[test]
    fn focused_select_emits_navigation_and_toggle_actions() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::select(
            "quality",
            crate::UiSelect::new(
                "settings.quality",
                vec![
                    crate::UiSelectOption::new("low", "quality.low"),
                    crate::UiSelectOption::new("high", "quality.high"),
                ],
                0,
            ),
        ));
        let mut state = UiInteractionState::default();
        state.focus.request_focus("quality");

        let down = state.update(
            &root,
            &[],
            &UiInputState {
                pressed_keys: vec!["arrowdown".to_string()],
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        assert!(down.iter().any(|action| matches!(
            action.action,
            UiAction::SetSelect { ref key, ref value, index: 1 }
                if key == "settings.quality" && value == "high"
        )));

        let enter = state.update(
            &root,
            &[],
            &UiInputState {
                pressed_keys: vec!["enter".to_string()],
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        assert!(enter.iter().any(|action| matches!(
            action.action,
            UiAction::SetSelectOpen { ref id, open: true } if id == "quality"
        )));
    }

    #[test]
    fn focused_select_supports_typeahead_without_requiring_a_second_widget() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(UiNode::select(
            "quality",
            crate::UiSelect::new(
                "settings.quality",
                vec![
                    crate::UiSelectOption::new("low", "quality.low"),
                    crate::UiSelectOption::new("high", "quality.high"),
                ],
                0,
            ),
        ));
        let mut state = UiInteractionState::default();
        state.focus.request_focus("quality");
        let actions = state.update(
            &root,
            &[],
            &UiInputState {
                text_input: "h".to_string(),
                time_seconds: 1.0,
                ..UiInputState::default()
            },
            &UiFocusPolicy::default(),
        );
        assert!(actions.iter().any(|action| matches!(
            action.action,
            UiAction::SetSelect { ref key, ref value, index: 1 }
                if key == "settings.quality" && value == "high"
        )));
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
