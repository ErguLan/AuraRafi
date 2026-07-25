//! Window-level placement primitives for retained overlays.
//!
//! A tooltip, menu, popover, or drag preview belongs to an owning surface for
//! semantics, but it must not be trapped by that surface's clip rectangle.
//! This module keeps the placement contract in RafUI so every host can use the
//! same logical-point rules before composing the overlay into a higher layer.

use serde::{Deserialize, Serialize};

use crate::geometry::UiRect;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiOverlayLayer {
    Panel,
    Floating,
    Menu,
    Tooltip,
    DragPreview,
    Modal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiPlacement {
    BottomStart,
    BottomCenter,
    BottomEnd,
    TopStart,
    TopCenter,
    TopEnd,
    RightStart,
    LeftStart,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiOverlayPlacement {
    pub rect: UiRect,
    pub placement: UiPlacement,
    pub flipped: bool,
    pub shifted: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiOverlayRequest {
    pub id: String,
    pub layer: UiOverlayLayer,
    pub anchor: UiRect,
    pub size: [f32; 2],
    pub placement: UiPlacement,
    pub gap: f32,
}

impl UiOverlayRequest {
    pub fn new(
        id: impl Into<String>,
        layer: UiOverlayLayer,
        anchor: UiRect,
        size: [f32; 2],
        placement: UiPlacement,
    ) -> Self {
        Self {
            id: id.into(),
            layer,
            anchor,
            size,
            placement,
            gap: 0.0,
        }
    }

    pub fn with_gap(mut self, gap: f32) -> Self {
        self.gap = gap.max(0.0);
        self
    }
}

/// Window-level overlay registry. It deliberately stores only placement
/// requests; documents, textures, and domain actions remain owned by their
/// existing surface hosts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UiOverlayManager {
    requests: Vec<UiOverlayRequest>,
}

impl UiOverlayManager {
    pub fn submit(&mut self, request: UiOverlayRequest) {
        if let Some(existing) = self.requests.iter_mut().find(|item| item.id == request.id) {
            *existing = request;
        } else {
            self.requests.push(request);
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.requests.retain(|request| request.id != id);
    }

    pub fn clear(&mut self) {
        self.requests.clear();
    }

    pub fn placements(
        &self,
        viewport: UiRect,
    ) -> Vec<(String, UiOverlayLayer, UiOverlayPlacement)> {
        self.requests
            .iter()
            .map(|request| {
                (
                    request.id.clone(),
                    request.layer,
                    place_overlay(
                        request.anchor,
                        request.size,
                        viewport,
                        request.placement,
                        request.gap,
                    ),
                )
            })
            .collect()
    }
}

/// Places an overlay in the window coordinate space. The result is flipped
/// when the preferred side does not fit and shifted when the anchor is close
/// to an edge. The overlay is never constrained to the anchor surface's clip.
pub fn place_overlay(
    anchor: UiRect,
    overlay_size: [f32; 2],
    viewport: UiRect,
    placement: UiPlacement,
    gap: f32,
) -> UiOverlayPlacement {
    let size = [overlay_size[0].max(0.0), overlay_size[1].max(0.0)];
    let gap = gap.max(0.0);
    let preferred = placement_rect(anchor, size, placement, gap);
    let opposite = opposite_placement(placement);
    let preferred_fits = fits(preferred, viewport);
    let selected_placement = if preferred_fits { placement } else { opposite };
    let selected = placement_rect(anchor, size, selected_placement, gap);
    let rect = selected.clamp_inside(viewport);
    UiOverlayPlacement {
        rect,
        placement: selected_placement,
        flipped: selected_placement != placement,
        shifted: rect.x != selected.x || rect.y != selected.y,
    }
}

fn placement_rect(anchor: UiRect, size: [f32; 2], placement: UiPlacement, gap: f32) -> UiRect {
    let x = match placement {
        UiPlacement::BottomStart | UiPlacement::TopStart | UiPlacement::RightStart => anchor.x,
        UiPlacement::BottomCenter | UiPlacement::TopCenter => {
            anchor.x + (anchor.width - size[0]) * 0.5
        }
        UiPlacement::BottomEnd | UiPlacement::TopEnd => anchor.right() - size[0],
        UiPlacement::LeftStart => anchor.x - size[0] - gap,
    };
    let y = match placement {
        UiPlacement::BottomStart | UiPlacement::BottomCenter | UiPlacement::BottomEnd => {
            anchor.bottom() + gap
        }
        UiPlacement::TopStart | UiPlacement::TopCenter | UiPlacement::TopEnd => {
            anchor.y - size[1] - gap
        }
        UiPlacement::RightStart => anchor.y,
        UiPlacement::LeftStart => anchor.y,
    };
    let x = match placement {
        UiPlacement::RightStart => anchor.right() + gap,
        UiPlacement::LeftStart => anchor.x - size[0] - gap,
        _ => x,
    };
    UiRect::new(x, y, size[0], size[1])
}

fn opposite_placement(placement: UiPlacement) -> UiPlacement {
    match placement {
        UiPlacement::BottomStart => UiPlacement::TopStart,
        UiPlacement::BottomCenter => UiPlacement::TopCenter,
        UiPlacement::BottomEnd => UiPlacement::TopEnd,
        UiPlacement::TopStart => UiPlacement::BottomStart,
        UiPlacement::TopCenter => UiPlacement::BottomCenter,
        UiPlacement::TopEnd => UiPlacement::BottomEnd,
        UiPlacement::RightStart => UiPlacement::LeftStart,
        UiPlacement::LeftStart => UiPlacement::RightStart,
    }
}

fn fits(rect: UiRect, viewport: UiRect) -> bool {
    rect.x >= viewport.x
        && rect.y >= viewport.y
        && rect.right() <= viewport.right()
        && rect.bottom() <= viewport.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_flips_above_when_bottom_edge_is_unavailable() {
        let result = place_overlay(
            UiRect::new(40.0, 88.0, 24.0, 12.0),
            [96.0, 22.0],
            UiRect::new(0.0, 0.0, 160.0, 120.0),
            UiPlacement::BottomStart,
            8.0,
        );

        assert_eq!(result.placement, UiPlacement::TopStart);
        assert!(result.flipped);
        assert_eq!(result.rect.y, 58.0);
    }

    #[test]
    fn tooltip_shifts_inside_left_and_right_edges() {
        let result = place_overlay(
            UiRect::new(130.0, 20.0, 20.0, 12.0),
            [120.0, 22.0],
            UiRect::new(0.0, 0.0, 160.0, 120.0),
            UiPlacement::BottomStart,
            8.0,
        );

        assert!(result.shifted);
        assert_eq!(result.rect.x, 40.0);
    }
}
