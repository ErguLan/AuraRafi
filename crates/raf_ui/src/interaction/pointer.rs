use crate::controls::{UiColorPickerHit, UiControl};
use crate::events::{UiAction, UiCursorIcon, UiEventKind, UiPointerButton};
use crate::focus::{UiInputState, UiModifiers};
use crate::geometry::UiRect;
use crate::hit_test::{hit_test, UiHitRegion, UiHitResult, UiHitTestMode};
use crate::node::{UiNode, UiNodeKind};

use super::drag::{captured_actions, dispatch_captured, DRAG_THRESHOLD_PX};
use super::routing::{dispatch, find_node};
use super::scroll::{
    dispatch_scrollbar_position, is_scrollbar_target, scrollbar_region, scrollbar_target,
};
use super::{UiDispatchedAction, UiInteractionState};

const DOUBLE_CLICK_DISTANCE_PX: f32 = 6.0;
const DOUBLE_CLICK_TIME_SECONDS: f64 = 0.45;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct UiTextClick {
    target_id: String,
    position: [f32; 2],
    time_seconds: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct UiPointerClick {
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

    pub(super) fn update_pointer_hover(
        &mut self,
        root: &UiNode,
        hit_regions: &[UiHitRegion],
        input: &UiInputState,
        dispatched: &mut Vec<UiDispatchedAction>,
    ) -> (Option<UiHitResult>, Option<String>, bool) {
        let hovered = input
            .pointer_position
            .and_then(|point| hit_test(hit_regions, point, UiHitTestMode::InteractiveOnly));
        let hovered_id = hovered.as_ref().map(|hit| hit.id.clone());
        let previous_hovered = self.focus.hovered.clone();
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
                dispatch(root, id, UiEventKind::HoverLeave, dispatched);
            }
            if let Some(id) = hovered_id.as_deref() {
                dispatch(root, id, UiEventKind::HoverEnter, dispatched);
            }
            self.focus.set_hovered(hovered_id.clone());
            self.hovered_since_seconds = hovered_id.as_ref().map(|_| input.time_seconds);
        }

        if pointer_moved {
            if let Some(id) = hovered_id.as_deref() {
                dispatch(root, id, UiEventKind::PointerMove, dispatched);
            }
        }

        (hovered, hovered_id, pointer_moved)
    }

    pub(super) fn handle_primary_press<F>(
        &mut self,
        root: &UiNode,
        hit_regions: &[UiHitRegion],
        input: &UiInputState,
        hovered: Option<&UiHitResult>,
        text_hit_test: &mut F,
        dispatched: &mut Vec<UiDispatchedAction>,
    ) where
        F: FnMut(&str, &str, [f32; 2]) -> Option<usize>,
    {
        dispatch_select_click_away(root, hovered.map(|hit| hit.id.as_str()), dispatched);
        self.active_pointer_target = hovered.map(|hit| hit.id.clone());
        self.active_drag_start_actions.clear();
        self.active_drag_move_actions.clear();
        self.active_drag_end_actions.clear();
        self.drag_started = false;
        self.drag_origin = input.pointer_position;
        self.text_selection_target = None;
        self.focus.set_active(hovered.map(|hit| hit.id.clone()));
        if let Some(target_id) = hovered.map(|hit| hit.id.as_str()) {
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
                            dispatched,
                        );
                    }
                }
            }
        }
        if let Some(hit) = hovered {
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
                dispatched,
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
                dispatched,
            );
            dispatch_color_picker_value(
                root,
                hit_regions,
                &hit.id,
                input.pointer_position,
                UiEventKind::PointerDown(UiPointerButton::Primary),
                dispatched,
            );
        }
    }

    pub(super) fn handle_primary_drag<F>(
        &mut self,
        root: &UiNode,
        hit_regions: &[UiHitRegion],
        input: &UiInputState,
        text_hit_test: &mut F,
        dispatched: &mut Vec<UiDispatchedAction>,
    ) where
        F: FnMut(&str, &str, [f32; 2]) -> Option<usize>,
    {
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
                    dispatched,
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
                            dispatched,
                        );
                    }
                }
            } else if self.drag_started {
                dispatch_captured(
                    root,
                    id,
                    UiEventKind::DragMove,
                    &self.active_drag_move_actions,
                    dispatched,
                );
                dispatch_range_value(
                    root,
                    hit_regions,
                    id,
                    input.pointer_position,
                    UiEventKind::DragMove,
                    dispatched,
                );
                dispatch_color_picker_value(
                    root,
                    hit_regions,
                    id,
                    input.pointer_position,
                    UiEventKind::DragMove,
                    dispatched,
                );
            }
        }
    }

    pub(super) fn handle_primary_release(
        &mut self,
        root: &UiNode,
        input: &UiInputState,
        hovered_id: Option<&str>,
        dispatched: &mut Vec<UiDispatchedAction>,
    ) {
        if let Some(id) = self.active_pointer_target.as_deref() {
            dispatch(
                root,
                id,
                UiEventKind::PointerUp(UiPointerButton::Primary),
                dispatched,
            );
            if self.drag_started {
                dispatch_captured(
                    root,
                    id,
                    UiEventKind::DragEnd,
                    &self.active_drag_end_actions,
                    dispatched,
                );
            } else if hovered_id == Some(id) {
                dispatch(root, id, UiEventKind::Click, dispatched);
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
                        dispatch(root, id, UiEventKind::DoubleClick, dispatched);
                        self.last_pointer_click = None;
                    } else {
                        self.last_pointer_click = Some(UiPointerClick {
                            target_id: id.to_string(),
                            position: point,
                            time_seconds: input.time_seconds,
                        });
                    }
                }
                dispatch_toggle_value(root, id, dispatched);
                dispatch_select_open(root, id, dispatched);
            }
        }
        self.cancel_pointer_gesture();
    }

    pub(super) fn handle_secondary_press(
        &mut self,
        root: &UiNode,
        hovered: Option<&UiHitResult>,
        dispatched: &mut Vec<UiDispatchedAction>,
    ) {
        if let Some(hit) = hovered {
            if hit.focusable {
                self.focus.request_focus(hit.id.clone());
            }
            dispatch(root, &hit.id, UiEventKind::ContextMenu, dispatched);
        }
    }
}

fn distance_squared(left: [f32; 2], right: [f32; 2]) -> f32 {
    let dx = left[0] - right[0];
    let dy = left[1] - right[1];
    dx * dx + dy * dy
}

fn proportional_text_index(text: &str, rect: UiRect, point: [f32; 2]) -> usize {
    let length = text.chars().count();
    if length == 0 || rect.width <= f32::EPSILON {
        return 0;
    }
    let progress = ((point[0] - rect.x) / rect.width).clamp(0.0, 1.0);
    (progress * length as f32).round() as usize
}

pub(super) fn word_range_at(text: &str, index: usize) -> std::ops::Range<usize> {
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

fn selectable_text_for_node<'a>(root: &'a UiNode, id: &str) -> Option<&'a str> {
    find_node(root, id)
        .filter(|node| node.text_selectable)
        .and_then(|node| node.text_value.as_deref())
}
