use crate::events::{UiAction, UiEventKind};
use crate::focus::UiInputState;
use crate::hit_test::UiHitRegion;
use crate::node::UiNode;
use crate::state::UiControlState;

use super::{UiDispatchedAction, UiInteractionState};

const GENERATED_SCROLLBAR_PREFIX: &str = "__rafui.scrollbar.";

impl UiInteractionState {
    pub(super) fn handle_wheel_scroll(
        &mut self,
        root: &UiNode,
        hit_regions: &[UiHitRegion],
        input: &UiInputState,
        hovered_id: Option<&str>,
        dispatched: &mut Vec<UiDispatchedAction>,
    ) {
        if input.scroll_delta != [0.0, 0.0] {
            let scroll_container = input
                .pointer_position
                .and_then(|point| find_scroll_container_at(root, hit_regions, point))
                .or_else(|| {
                    hovered_id.and_then(|id| find_scroll_container(root, id))
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
    }
}

pub(super) fn is_scrollbar_target(id: &str) -> bool {
    id.starts_with(GENERATED_SCROLLBAR_PREFIX)
}

pub(super) fn scrollbar_target(id: &str) -> Option<(&str, bool)> {
    let suffix = id.strip_prefix(GENERATED_SCROLLBAR_PREFIX)?;
    if let Some(scroll_id) = suffix.strip_suffix(".thumb") {
        return Some((scroll_id, true));
    }
    suffix
        .strip_suffix(".track")
        .map(|scroll_id| (scroll_id, false))
}

pub(super) fn scrollbar_region<'a>(
    hit_regions: &'a [UiHitRegion],
    scroll_id: &str,
    thumb: bool,
) -> Option<&'a UiHitRegion> {
    let suffix = if thumb { ".thumb" } else { ".track" };
    let id = format!("{GENERATED_SCROLLBAR_PREFIX}{scroll_id}{suffix}");
    hit_regions.iter().find(|region| region.id == id)
}

pub(super) fn dispatch_scrollbar_position(
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
