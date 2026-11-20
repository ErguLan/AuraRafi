use crate::events::{UiEventKind, UiPointerButton};
use crate::focus::{UiFocusPolicy, UiInputState};
use crate::hit_test::UiHitRegion;
use crate::node::UiNode;

use super::{UiDispatchedAction, UiInteractionState};

impl UiInteractionState {
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
        let mut dispatched = Vec::new();
        let (hovered, hovered_id, pointer_moved) =
            self.update_pointer_hover(root, hit_regions, input, &mut dispatched);

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
            self.handle_primary_press(
                root,
                hit_regions,
                input,
                hovered.as_ref(),
                &mut text_hit_test,
                &mut dispatched,
            );
        }
        if primary_down && self.pointer_was_down && pointer_moved {
            self.handle_primary_drag(
                root,
                hit_regions,
                input,
                &mut text_hit_test,
                &mut dispatched,
            );
        }
        if primary_released {
            self.handle_primary_release(root, input, hovered_id.as_deref(), &mut dispatched);
        }

        if input.button_pressed(UiPointerButton::Secondary) {
            self.handle_secondary_press(root, hovered.as_ref(), &mut dispatched);
        }

        self.handle_tab_navigation(root, hit_regions, input, focus_policy);

        self.handle_wheel_scroll(
            root,
            hit_regions,
            input,
            hovered_id.as_deref(),
            &mut dispatched,
        );

        self.handle_focused_key(root, input, &mut visual_lines, &mut dispatched);

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
}

pub(super) fn dispatch(
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

pub(super) fn event_matches(binding: &UiEventKind, event: &UiEventKind) -> bool {
    match (binding, event) {
        (UiEventKind::KeyPress(left), UiEventKind::KeyPress(right)) => {
            left.eq_ignore_ascii_case(right)
        }
        _ => binding == event,
    }
}

pub(super) fn find_node<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
    if node.id == id {
        return Some(node);
    }
    node.children.iter().find_map(|child| find_node(child, id))
}
