use crate::events::{UiAction, UiEventKind, UiPointerButton};
use crate::focus::{UiFocusPolicy, UiFocusState, UiInputState};
use crate::hit_test::{hit_test, UiHitRegion, UiHitTestMode};
use crate::node::UiNode;

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
    pointer_was_down: bool,
    active_pointer_target: Option<String>,
    drag_started: bool,
    last_pointer_position: Option<[f32; 2]>,
}

impl UiInteractionState {
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
            }
        } else if primary_down && self.pointer_was_down && pointer_moved {
            if let Some(id) = self.active_pointer_target.as_deref() {
                if !self.drag_started {
                    dispatch(root, id, UiEventKind::DragStart, &mut dispatched);
                    self.drag_started = true;
                }
                dispatch(root, id, UiEventKind::DragMove, &mut dispatched);
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

        if input.key_pressed("tab") {
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

        if let Some(focused) = self.focus.focused.as_deref() {
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
}
