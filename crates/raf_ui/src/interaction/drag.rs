use crate::events::{UiAction, UiEventKind};
use crate::node::UiNode;

use super::routing::{dispatch, event_matches, find_node};
use super::UiDispatchedAction;

pub(super) const DRAG_THRESHOLD_PX: f32 = 4.0;

pub(super) fn captured_actions(root: &UiNode, target_id: &str, event: &UiEventKind) -> Vec<UiAction> {
    find_node(root, target_id)
        .into_iter()
        .flat_map(|node| node.event_handlers.iter())
        .filter(|binding| event_matches(&binding.event, event))
        .map(|binding| binding.action.clone())
        .collect()
}

pub(super) fn dispatch_captured(
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
