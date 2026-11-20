use crate::controls::UiControl;
use crate::focus::{UiFocusPolicy, UiInputState};
use crate::hit_test::UiHitRegion;
use crate::node::{UiAccessibilityRole, UiNode, UiNodeKind};

use super::keyboard::node_handles_key;
use super::routing::find_node;
use super::UiInteractionState;

impl UiInteractionState {
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

    pub(super) fn handle_tab_navigation(
        &mut self,
        root: &UiNode,
        hit_regions: &[UiHitRegion],
        input: &UiInputState,
        focus_policy: &UiFocusPolicy,
    ) {
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
    }
}

pub(super) fn collect_focusable_tabs(node: &UiNode, ids: &mut Vec<String>) {
    if node.accessibility_role == UiAccessibilityRole::Tab && node.focusable && !node.disabled {
        ids.push(node.id.clone());
    }
    for child in &node.children {
        collect_focusable_tabs(child, ids);
    }
}
