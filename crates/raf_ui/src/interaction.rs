use crate::events::{UiAction, UiEventKind, UiPointerButton};
use crate::focus::{UiFocusPolicy, UiFocusState, UiInputState};
use crate::hit_test::{hit_test, UiHitRegion, UiHitTestMode};
use crate::node::UiNode;
use crate::state::UiControlState;

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
    drag_started: bool,
    last_pointer_position: Option<[f32; 2]>,
    hovered_since_seconds: Option<f64>,
}

impl UiInteractionState {
    pub fn pointer_position(&self) -> Option<[f32; 2]> {
        self.last_pointer_position
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

    pub fn update(
        &mut self,
        root: &UiNode,
        hit_regions: &[UiHitRegion],
        input: &UiInputState,
        focus_policy: &UiFocusPolicy,
    ) -> Vec<UiDispatchedAction> {
        let hovered = input
            .pointer_position
            .and_then(|point| hit_test(hit_regions, point, UiHitTestMode::InteractiveOnly));
        let hovered_id = hovered.as_ref().map(|hit| hit.id.clone());
        let previous_hovered = self.focus.hovered.clone();
        let mut dispatched = Vec::new();
        let pointer_moved = input.pointer_position != self.last_pointer_position;

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
            self.drag_started = false;
            self.focus.set_active(hovered_id.clone());
            if let Some(hit) = hovered.as_ref() {
                if hit.focusable {
                    self.focus.request_focus(hit.id.clone());
                }
                dispatch(
                    root,
                    &hit.id,
                    UiEventKind::PointerDown(UiPointerButton::Primary),
                    &mut dispatched,
                );
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
            if let Some(id) = self.active_pointer_target.as_deref() {
                if !self.drag_started {
                    dispatch(root, id, UiEventKind::DragStart, &mut dispatched);
                    self.drag_started = true;
                }
                dispatch(root, id, UiEventKind::DragMove, &mut dispatched);
                dispatch_range_value(
                    root,
                    hit_regions,
                    id,
                    input.pointer_position,
                    UiEventKind::DragMove,
                    &mut dispatched,
                );
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
                    dispatch(root, id, UiEventKind::DragEnd, &mut dispatched);
                } else if hovered_id.as_deref() == Some(id) {
                    dispatch(root, id, UiEventKind::Click, &mut dispatched);
                    dispatch_toggle_value(root, id, &mut dispatched);
                }
            }
            self.active_pointer_target = None;
            self.focus.set_active(None);
            self.drag_started = false;
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
            if let Some(next) = effective_policy.next_after(self.focus.focused.as_deref()) {
                self.focus.request_focus(next);
            }
        }

        if let Some(hovered_id) = hovered_id.as_deref() {
            if input.scroll_delta != [0.0, 0.0] {
                if let Some((scroll_id, axis)) = find_scroll_container(root, hovered_id) {
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
        }

        if let Some(focused) = self.focus.focused.as_deref() {
            if let Some(input_control) =
                find_node(root, focused).and_then(|node| node.control.text_input())
            {
                let value_key = input_control.value_key.as_str();
                if input.key_pressed("backspace") && self.controls.backspace(value_key) {
                    dispatched.push(UiDispatchedAction {
                        target_id: focused.to_string(),
                        event: UiEventKind::KeyPress("backspace".to_string()),
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
                        target_id: focused.to_string(),
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
                            target_id: focused.to_string(),
                            event: UiEventKind::KeyPress("enter".to_string()),
                            action: UiAction::Command {
                                name: command.clone(),
                            },
                        });
                    }
                }
            } else if let Some(toggle) =
                find_node(root, focused).and_then(|node| node.control.toggle())
            {
                if input.key_pressed("space") || input.key_pressed("enter") {
                    dispatched.push(UiDispatchedAction {
                        target_id: focused.to_string(),
                        event: UiEventKind::KeyPress("space".to_string()),
                        action: UiAction::SetToggle {
                            key: toggle.value_key.clone(),
                            value: !toggle.value,
                        },
                    });
                }
            } else if let Some(range) =
                find_node(root, focused).and_then(|node| node.control.range())
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
                        target_id: focused.to_string(),
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
                    focused,
                    UiEventKind::KeyPress(key.clone()),
                    &mut dispatched,
                );
            }
            if !input.text_input.is_empty() {
                dispatch(
                    root,
                    focused,
                    UiEventKind::TextInput(input.text_input.clone()),
                    &mut dispatched,
                );
            }
        }

        self.pointer_was_down = primary_down;
        self.last_pointer_position = input.pointer_position;
        dispatched
    }
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
        if binding.event == event {
            dispatched.push(UiDispatchedAction {
                target_id: target_id.to_string(),
                event: event.clone(),
                action: binding.action.clone(),
            });
        }
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
