//! Lightweight retained UI surface for ApiGraphicBasic.
//!
//! This is the first non-egui UI contract for editor chrome and canvas
//! overlays. It stores layout and text keys as data, records simple geometry
//! into `BasicCommandList`, and can compose its cached bitmap text through a
//! direct WGPU target without depending on the legacy editor shell.

mod cpu_host;
mod cpu_renderer;
mod direct_host;
mod gpu_renderer;
mod native_input;
mod native_window;
mod presentation;
mod render;

use serde::{Deserialize, Serialize};

pub use cpu_host::{CpuUiSurfaceHost, DirectUiSurfaceCpuFrame};
pub use cpu_renderer::{UiSurfaceCpuMetrics, UiSurfaceCpuRenderer};
pub use direct_host::{DirectUiSurfaceFrame, DirectUiSurfaceHost};
pub use gpu_renderer::{UiSurfaceGpuMetrics, UiSurfaceGpuRenderer};
pub use native_input::NativeUiInputBridge;
pub use native_window::NativeUiWindowHost;
pub use presentation::{
    UiSurfaceDrawList, UiSurfacePaintCommand, UiSurfaceQuad, UiSurfaceTextQuad,
};
pub use raf_ui::{
    DockDropTarget, DockLayout, DockLayoutEntry, DockLayoutFrame, DockPanel, DockSide,
    DockWorkspaceController, DockWorkspaceEvent, FloatingPanel, StudioUiPalette, UiAction,
    UiDispatchedAction, UiEventBinding, UiEventKind, UiFlow, UiFocusPolicy, UiFocusState,
    UiHitRegion, UiHitResult, UiHitTestMode, UiInputState, UiInteractionState, UiLayout, UiNode,
    UiNodeKind, UiPointerButton, UiPositionMode, UiRect, UiSpacing, UiStyle, UiStylePatch,
    UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiTextAtlas, UiTextAtlasRect,
    UiTextAtlasRequest, UiTextAtlasSlot, UiTextAtlasSyncStats, UiTextStyle, UiTokens,
    UiVisualState, FLOATING_PANEL_RESIZE_HANDLE_SIZE, FLOATING_PANEL_TITLE_BAR_HEIGHT,
};
pub use render::{UiLayoutBox, UiSurfaceFrame};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSurface {
    pub id: String,
    pub palette: StudioUiPalette,
    pub root: UiNode,
    pub style_sheet: UiStyleSheet,
}

impl UiSurface {
    pub fn new(id: impl Into<String>, palette: StudioUiPalette, root: UiNode) -> Self {
        Self {
            id: id.into(),
            palette,
            root,
            style_sheet: UiStyleSheet::default(),
        }
    }

    pub fn build_frame(&self, width: u32, height: u32, clear_color: [u8; 4]) -> UiSurfaceFrame {
        self.build_frame_with_visual_state(width, height, clear_color, UiVisualState::default())
    }

    pub fn build_frame_with_visual_state(
        &self,
        width: u32,
        height: u32,
        clear_color: [u8; 4],
        visual_state: UiVisualState<'_>,
    ) -> UiSurfaceFrame {
        render::build_surface_frame(
            &self.root,
            &self.style_sheet,
            visual_state,
            width,
            height,
            clear_color,
        )
    }

    pub fn hit_test<'a>(
        &'a self,
        layout_boxes: &'a [UiLayoutBox],
        point: [f32; 2],
    ) -> Option<&'a UiLayoutBox> {
        layout_boxes
            .iter()
            .enumerate()
            .filter(|(_, layout_box)| {
                layout_box.interactive && !layout_box.disabled && layout_box.rect.contains(point)
            })
            .max_by_key(|(sequence, layout_box)| (layout_box.z_index, *sequence))
            .map(|(_, layout_box)| layout_box)
    }

    pub fn hit_test_regions(
        &self,
        hit_regions: &[UiHitRegion],
        point: [f32; 2],
    ) -> Option<UiHitResult> {
        raf_ui::hit_test(hit_regions, point, UiHitTestMode::InteractiveOnly)
    }
}

/// Stateful companion for a retained `UiSurface`.
///
/// The surface itself stays serializable and declarative. This session owns
/// only frame-local interaction and text-cache state, so it can be recreated
/// when an editor workspace changes without invalidating persisted layouts.
#[derive(Debug, Clone, Default)]
pub struct UiSurfaceSession {
    pub text_atlas: UiTextAtlas,
    pub interaction: UiInteractionState,
    pub focus_policy: UiFocusPolicy,
}

impl UiSurfaceSession {
    pub fn build_frame(
        &mut self,
        surface: &UiSurface,
        width: u32,
        height: u32,
        clear_color: [u8; 4],
    ) -> UiSurfaceFrame {
        let frame = surface.build_frame_with_visual_state(
            width,
            height,
            clear_color,
            UiVisualState::from_focus(&self.interaction.focus),
        );
        self.text_atlas.sync(&frame.text_requests);
        self.reconcile_focus(&frame);
        frame
    }

    pub fn build_frame_with_resolved_text<F>(
        &mut self,
        surface: &UiSurface,
        width: u32,
        height: u32,
        clear_color: [u8; 4],
        mut resolve: F,
    ) -> UiSurfaceFrame
    where
        F: FnMut(&str) -> String,
    {
        let frame = surface.build_frame_with_visual_state(
            width,
            height,
            clear_color,
            UiVisualState::from_focus(&self.interaction.focus),
        );
        let resolved_text = frame
            .text_requests
            .iter()
            .map(|request| resolve(&request.text_key))
            .collect::<Vec<_>>();
        self.text_atlas.sync_resolved(
            frame
                .text_requests
                .iter()
                .zip(resolved_text.iter())
                .map(|(request, text)| (request, text.as_str())),
        );
        self.reconcile_focus(&frame);
        frame
    }

    pub fn process_input(
        &mut self,
        surface: &UiSurface,
        frame: &UiSurfaceFrame,
        input: &UiInputState,
    ) -> Vec<UiDispatchedAction> {
        self.reconcile_focus(frame);
        self.interaction
            .update(&surface.root, &frame.hit_regions, input, &self.focus_policy)
    }

    fn reconcile_focus(&mut self, frame: &UiSurfaceFrame) {
        self.interaction.focus.blur_if_removed(
            frame
                .hit_regions
                .iter()
                .filter(|region| region.focusable && !region.disabled)
                .map(|region| region.id.as_str()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_layout_distributes_grow_child() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_style(palette.root_style())
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                padding: UiSpacing::same(8.0),
                gap: 4.0,
                ..UiLayout::fill(UiFlow::Column)
            })
            .with_child(
                UiNode::new("toolbar", UiNodeKind::Toolbar)
                    .with_layout(UiLayout::fixed(0.0, 32.0))
                    .with_style(palette.panel_style()),
            )
            .with_child(
                UiNode::new("canvas", UiNodeKind::Canvas)
                    .with_layout(UiLayout {
                        grow: 1.0,
                        ..UiLayout::default()
                    })
                    .with_style(palette.panel_style())
                    .interactive(),
            );
        let surface = UiSurface::new("test", palette, root);
        let frame = surface.build_frame(320, 200, [0, 0, 0, 255]);

        let canvas = frame
            .layout_boxes
            .iter()
            .find(|layout_box| layout_box.id == "canvas")
            .expect("canvas layout box");
        assert_eq!(canvas.rect.x, 8.0);
        assert_eq!(canvas.rect.y, 44.0);
        assert_eq!(canvas.rect.width, 304.0);
        assert_eq!(canvas.rect.height, 148.0);
        assert_eq!(
            surface
                .hit_test(&frame.layout_boxes, [20.0, 60.0])
                .map(|hit| hit.id.as_str()),
            Some("canvas")
        );
    }

    #[test]
    fn session_caches_surface_text_requests() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::new("title", UiNodeKind::Label)
                .with_text_key("app.project_settings")
                .with_text_style(UiTextStyle::panel_title([255, 255, 255, 255])),
        );
        let surface = UiSurface::new("session", palette, root);
        let mut session = UiSurfaceSession::default();

        session.build_frame(&surface, 320, 200, [0, 0, 0, 255]);

        assert_eq!(session.text_atlas.slot_count(), 1);
    }

    #[test]
    fn layout_honors_minimum_and_maximum_sizes_in_flow() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                ..UiLayout::fill(UiFlow::Row)
            })
            .with_child(
                UiNode::new("minimum", UiNodeKind::Panel).with_layout(UiLayout {
                    basis: [20.0, 0.0],
                    min_size: [96.0, 32.0],
                    ..UiLayout::default()
                }),
            )
            .with_child(
                UiNode::new("maximum", UiNodeKind::Panel).with_layout(UiLayout {
                    basis: [180.0, 0.0],
                    max_size: [72.0, 0.0],
                    ..UiLayout::default()
                }),
            );
        let surface = UiSurface::new("bounds", palette, root);
        let frame = surface.build_frame(320, 120, [0, 0, 0, 255]);

        let minimum = frame
            .layout_boxes
            .iter()
            .find(|entry| entry.id == "minimum")
            .unwrap();
        let maximum = frame
            .layout_boxes
            .iter()
            .find(|entry| entry.id == "maximum")
            .unwrap();
        assert_eq!(minimum.rect.width, 96.0);
        assert_eq!(maximum.rect.width, 72.0);
    }

    #[test]
    fn crowded_flow_compresses_without_sibling_overlap() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                gap: 8.0,
                ..UiLayout::fill(UiFlow::Row)
            })
            .with_child(
                UiNode::new("first", UiNodeKind::Panel).with_layout(UiLayout {
                    min_size: [120.0, 20.0],
                    ..UiLayout::default()
                }),
            )
            .with_child(
                UiNode::new("second", UiNodeKind::Panel).with_layout(UiLayout {
                    min_size: [120.0, 20.0],
                    ..UiLayout::default()
                }),
            );
        let surface = UiSurface::new("crowded", palette, root);
        let frame = surface.build_frame(180, 80, [0, 0, 0, 255]);
        let first = frame
            .layout_boxes
            .iter()
            .find(|entry| entry.id == "first")
            .unwrap();
        let second = frame
            .layout_boxes
            .iter()
            .find(|entry| entry.id == "second")
            .unwrap();

        assert!(first.rect.right() <= second.rect.x);
        assert!(second.rect.right() <= 180.0);
    }

    #[test]
    fn session_applies_hover_style_to_the_next_retained_frame() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::new("save", UiNodeKind::Button)
                .with_class("primary")
                .with_layout(UiLayout::fixed(80.0, 28.0)),
        );
        let mut surface = UiSurface::new("hover", palette, root);
        surface.style_sheet.rules = vec![
            UiStyleRule::new(
                UiStyleSelector::Class("primary".to_string()),
                UiStylePatch {
                    fill: Some([224, 116, 24, 255]),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("primary".to_string()),
                UiStylePatch {
                    fill: Some([255, 151, 46, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        ];
        let mut session = UiSurfaceSession::default();
        session
            .interaction
            .focus
            .set_hovered(Some("save".to_string()));

        let frame = session.build_frame(&surface, 240, 120, [0, 0, 0, 255]);
        let save = frame
            .layout_boxes
            .iter()
            .find(|entry| entry.id == "save")
            .unwrap();

        assert_eq!(save.style.fill, [255, 151, 46, 255]);
    }

    #[test]
    fn child_of_an_elevated_overlay_wins_hit_testing_over_base_canvas() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_child(
                UiNode::new("canvas", UiNodeKind::Canvas)
                    .with_layout(UiLayout::absolute(UiRect::new(0.0, 0.0, 240.0, 120.0)))
                    .interactive(),
            )
            .with_child(
                UiNode::new("menu-overlay", UiNodeKind::Overlay)
                    .with_layout(
                        UiLayout::absolute(UiRect::new(20.0, 20.0, 120.0, 70.0)).with_z_index(20),
                    )
                    .with_child(
                        UiNode::new("menu-action", UiNodeKind::Button)
                            .with_layout(UiLayout::fill(UiFlow::None))
                            .interactive(),
                    ),
            );
        let surface = UiSurface::new("overlay", palette, root);
        let frame = surface.build_frame(240, 120, [0, 0, 0, 255]);

        let hit = surface
            .hit_test_regions(&frame.hit_regions, [40.0, 40.0])
            .expect("overlay hit");
        assert_eq!(hit.id, "menu-action");
        assert_eq!(hit.z_index, 20);
    }
}
