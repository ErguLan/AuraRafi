//! Inspectable retained-frame diagnostics.
//!
//! This is intentionally data-only. Editor tooling can render it in any host,
//! while CI and golden tests can assert the same layout invariants without
//! opening a window.

use super::{UiDensityContract, UiEnvironment, UiSurfaceFrame};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UiSurfaceDiagnostics {
    pub layout_boxes: usize,
    pub interactive_regions: usize,
    pub focusable_regions: usize,
    pub text_requests: usize,
    pub image_regions: usize,
    pub overlay_regions: usize,
    pub clipped_boxes: usize,
    pub zero_sized_boxes: usize,
    pub max_z_index: i16,
    pub density: UiDensityContract,
}

impl UiSurfaceDiagnostics {
    pub fn from_frame(frame: &UiSurfaceFrame) -> Self {
        Self::from_frame_at_density(frame, UiEnvironment::default().density_contract())
    }

    pub fn from_frame_at_density(frame: &UiSurfaceFrame, density: UiDensityContract) -> Self {
        Self {
            layout_boxes: frame.layout_boxes.len(),
            interactive_regions: frame
                .hit_regions
                .iter()
                .filter(|region| region.interactive)
                .count(),
            focusable_regions: frame
                .layout_boxes
                .iter()
                .filter(|layout| layout.focusable && !layout.disabled)
                .count(),
            text_requests: frame.text_requests.len(),
            image_regions: frame
                .layout_boxes
                .iter()
                .filter(|layout| matches!(layout.kind, raf_ui::UiNodeKind::Image))
                .count(),
            overlay_regions: frame
                .layout_boxes
                .iter()
                .filter(|layout| {
                    matches!(
                        layout.kind,
                        raf_ui::UiNodeKind::Overlay
                            | raf_ui::UiNodeKind::Menu
                            | raf_ui::UiNodeKind::Tooltip
                            | raf_ui::UiNodeKind::FloatingPanel
                    )
                })
                .count(),
            clipped_boxes: frame
                .layout_boxes
                .iter()
                .filter(|layout| layout.clip_rect != layout.rect)
                .count(),
            zero_sized_boxes: frame
                .layout_boxes
                .iter()
                .filter(|layout| layout.rect.is_empty())
                .count(),
            max_z_index: frame
                .layout_boxes
                .iter()
                .map(|layout| layout.z_index)
                .max()
                .unwrap_or(0),
            density,
        }
    }

    pub fn has_layout_warnings(self) -> bool {
        self.zero_sized_boxes > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_graphic_basic::ui_surface::{StudioUiPalette, UiNode, UiNodeKind, UiSurface};

    #[test]
    fn diagnostics_expose_layout_and_interaction_counts() {
        let surface = UiSurface::new(
            "diagnostics",
            StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root).with_child(
                UiNode::new("button", UiNodeKind::Button)
                    .with_layout(raf_ui::UiLayout::fixed(80.0, 28.0))
                    .interactive(),
            ),
        );
        let frame = surface.build_frame(160, 80, [0, 0, 0, 255]);
        let diagnostics = UiSurfaceDiagnostics::from_frame(&frame);

        assert_eq!(diagnostics.layout_boxes, 2);
        assert_eq!(diagnostics.interactive_regions, 1);
        assert!(!diagnostics.has_layout_warnings());
    }
}
