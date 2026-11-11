//! Backend-neutral editor workbench layout.
//!
//! The native shell, RafUI hit testing, canvas input, and ApiGraphicBasic
//! composition all consume the same rectangles. This removes the old split
//! where separate presentation layers guessed placement and pointer ownership.

use raf_render::api_graphic_basic::CanvasTargetRect;

use crate::application_bar_surface::APPLICATION_BAR_HEIGHT;

pub const EDITOR_STATUS_HEIGHT: f32 = 28.0;
pub const ELECTRONICS_RAIL_WIDTH: f32 = 52.0;
pub const EDITOR_DOCK_COLLAPSED_HEIGHT: f32 = 32.0;
pub const EDITOR_DOCK_MIN_HEIGHT: f32 = 112.0;
/// Upper safety bound for the user-resizable downbar. The effective maximum
/// is still limited by the current window so a small viewport remains usable.
pub const EDITOR_DOCK_MAX_HEIGHT: f32 = 4096.0;
pub const EDITOR_DOCK_MIN_WORKSPACE_HEIGHT: f32 = 120.0;
/// Height of the reserved Electronics band when it is one row, in logical
/// points. It is the single control track plus the vertical padding that keeps
/// the row breathing inside the band.
pub const ELECTRONICS_TOOLBAR_HEIGHT: f32 = 64.0;
/// Height of the reserved Electronics band when the document row and the
/// view/history row stack, in logical points: two control tracks, the gap
/// between them and the vertical padding.
///
/// The band grows instead of scrolling because a horizontal scroll owner has no
/// reachable thumb in this compositor, so an overflowing band would hide
/// controls instead of moving them.
pub const ELECTRONICS_TOOLBAR_STACKED_HEIGHT: f32 = 76.0;
/// Canvas width from which the Electronics band prints localized labels next to
/// the controls whose glyph does not already state what they do.
///
/// The measured worst case is the Spanish PCB row at 794.1 points (Ubuntu
/// Medium 11px against `locales/es.json`, plus the padding the compositor forces
/// on a labelled control), so this leaves room for atlas rounding and for a
/// longer translation. It also covers the default 1600-point window, whose
/// Electronics canvas is 844 points.
pub const ELECTRONICS_TOOLBAR_LABELLED_WIDTH: f32 = 920.0;
/// Canvas width below which the Electronics band stacks into two rows.
///
/// The measured worst single glyph-only row is the PCB row at 506 points, so
/// every canvas above this width still gets one 46-point band.
pub const ELECTRONICS_TOOLBAR_STACKED_WIDTH: f32 = 590.0;

/// Height the Electronics band reserves for one canvas width.
///
/// The band is the only element between the application bar and the CAD canvas,
/// so its height has to be derived from the same number the toolbar surface
/// derives its density from. Anything else would let the document paint over the
/// renderer-owned canvas or leave a dead strip above it.
pub fn electronics_toolbar_height(canvas_width: f32) -> f32 {
    if canvas_width < ELECTRONICS_TOOLBAR_STACKED_WIDTH {
        ELECTRONICS_TOOLBAR_STACKED_HEIGHT
    } else {
        ELECTRONICS_TOOLBAR_HEIGHT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorWorkbenchKind {
    Game,
    Electronics,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EditorRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl EditorRect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(self, point: [f32; 2]) -> bool {
        point[0] >= self.x
            && point[1] >= self.y
            && point[0] < self.x + self.width
            && point[1] < self.y + self.height
    }

    pub fn local_point(self, point: [f32; 2]) -> Option<[f32; 2]> {
        self.contains(point)
            .then_some([point[0] - self.x, point[1] - self.y])
    }

    pub fn logical_size(self) -> [u32; 2] {
        [
            self.width.max(1.0).round() as u32,
            self.height.max(1.0).round() as u32,
        ]
    }

    pub fn to_physical(self, scale_factor: f32, target_size: [u32; 2]) -> CanvasTargetRect {
        let scale = scale_factor.max(0.25);
        let left = (self.x * scale).round().max(0.0) as u32;
        let top = (self.y * scale).round().max(0.0) as u32;
        let right = ((self.x + self.width) * scale).round().max(left as f32) as u32;
        let bottom = ((self.y + self.height) * scale).round().max(top as f32) as u32;
        CanvasTargetRect::new(
            left.min(target_size[0]),
            top.min(target_size[1]),
            right
                .min(target_size[0])
                .saturating_sub(left.min(target_size[0])),
            bottom
                .min(target_size[1])
                .saturating_sub(top.min(target_size[1])),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorLayoutRequest {
    pub logical_size: [f32; 2],
    pub kind: EditorWorkbenchKind,
    pub left_visible: bool,
    pub right_visible: bool,
    pub left_width: f32,
    pub right_width: f32,
    pub bottom_expanded: bool,
    pub bottom_height: f32,
}

impl EditorLayoutRequest {
    pub fn game(logical_size: [f32; 2]) -> Self {
        Self {
            logical_size,
            kind: EditorWorkbenchKind::Game,
            left_visible: true,
            right_visible: true,
            left_width: 430.0,
            right_width: 360.0,
            bottom_expanded: true,
            bottom_height: 238.0,
        }
    }

    pub fn electronics(logical_size: [f32; 2]) -> Self {
        Self {
            logical_size,
            kind: EditorWorkbenchKind::Electronics,
            left_visible: true,
            right_visible: false,
            left_width: 260.0,
            right_width: 300.0,
            bottom_expanded: false,
            bottom_height: 238.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorFrameLayout {
    pub window: EditorRect,
    pub application_bar: EditorRect,
    pub workspace: EditorRect,
    pub left_panel: Option<EditorRect>,
    pub canvas: EditorRect,
    pub right_panel: Option<EditorRect>,
    pub bottom_dock: EditorRect,
    pub status_bar: EditorRect,
}

impl EditorFrameLayout {
    /// Returns the rectangular ApiGraphicBasic/RafUI canvas below the native
    /// Electronics toolbar. Game continues to use `canvas` unchanged because
    /// its toolbar is a separate overlay policy.
    ///
    /// The reserved height depends on the canvas width, because the band stacks
    /// into two rows when one row cannot hold every control without scrolling.
    pub fn electronics_canvas(self) -> EditorRect {
        self.electronics_canvas_for_pcb(false)
    }

    /// Copper controls occupy real chrome, so labels, fit, input and rendering
    /// all use the same unobstructed drafting area.
    pub fn electronics_canvas_for_pcb(self, pcb_active: bool) -> EditorRect {
        Self::cad_content_rect(self.canvas, pcb_active)
    }

    pub fn cad_content_rect(area: EditorRect, pcb_active: bool) -> EditorRect {
        use crate::panels::electronics_desktop_surface::{
            DOCUMENT_HEIGHT, RULER_HEIGHT, RULER_WIDTH, TOOL_RAIL_WIDTH,
        };
        let rail = TOOL_RAIL_WIDTH.min(area.width.max(0.0));
        let left = (rail + if pcb_active { RULER_WIDTH } else { 0.0 }).min(area.width.max(0.0));
        let header = (DOCUMENT_HEIGHT
            + if pcb_active {
                crate::panels::pcb_tool_options_surface::pcb_tool_options_height(area.width - rail)
                    + RULER_HEIGHT
            } else {
                0.0
            })
        .min(area.height.max(0.0));
        EditorRect::new(
            area.x + left,
            area.y + header,
            (area.width - left).max(0.0),
            (area.height - header).max(0.0),
        )
    }

    pub fn electronics_toolbar_rect(self) -> EditorRect {
        EditorRect::new(
            0.0,
            self.application_bar.height,
            self.window.width,
            electronics_toolbar_height(self.window.width)
                .min((self.window.height - self.application_bar.height).max(0.0)),
        )
    }

    pub fn compute(request: EditorLayoutRequest) -> Self {
        let width = request.logical_size[0].max(1.0);
        let height = request.logical_size[1].max(1.0);
        let app_height = APPLICATION_BAR_HEIGHT.min(height);
        let status_height = EDITOR_STATUS_HEIGHT.min((height - app_height).max(0.0));
        let cad_band = if request.kind == EditorWorkbenchKind::Electronics {
            electronics_toolbar_height(width).min((height - app_height - status_height).max(0.0))
        } else {
            0.0
        };
        let workspace_y = app_height + cad_band;
        let available_below_bar = (height - workspace_y - status_height).max(0.0);
        let requested_dock = if request.bottom_expanded {
            request
                .bottom_height
                .clamp(EDITOR_DOCK_MIN_HEIGHT, EDITOR_DOCK_MAX_HEIGHT)
        } else {
            EDITOR_DOCK_COLLAPSED_HEIGHT
        };
        // Keep a compact authoring canvas visible, but allow the downbar to
        // cover almost the whole workbench when the user drags its splitter.
        // The previous fixed 320 px cap made the dock stop far too early on
        // tall windows. The real maximum is now derived from the window.
        let minimum_workspace_height = EDITOR_DOCK_MIN_WORKSPACE_HEIGHT.min(available_below_bar);
        let maximum_dock_height = (available_below_bar - minimum_workspace_height)
            .max(0.0)
            .max(EDITOR_DOCK_COLLAPSED_HEIGHT.min(available_below_bar))
            .min(EDITOR_DOCK_MAX_HEIGHT);
        let dock_height = requested_dock.min(maximum_dock_height);
        let workspace_height = (available_below_bar - dock_height).max(0.0);

        let (left_default_min, right_default_min, canvas_min): (f32, f32, f32) = match request.kind
        {
            EditorWorkbenchKind::Game => (280.0, 260.0, 320.0),
            EditorWorkbenchKind::Electronics => (ELECTRONICS_RAIL_WIDTH, 260.0, 360.0),
        };
        let requested_left = if request.left_visible {
            request.left_width.max(left_default_min)
        } else {
            0.0
        };
        let requested_right = if request.right_visible {
            request.right_width.max(right_default_min)
        } else {
            0.0
        };
        let (mut left_width, mut right_width) = fit_side_panels(
            width,
            requested_left,
            requested_right,
            request.left_visible,
            request.right_visible,
            canvas_min.min(width),
        );
        // A rail is an icon track, so proportional panel fitting must never
        // squeeze its buttons into half-width hit targets on narrow windows.
        if request.kind == EditorWorkbenchKind::Electronics
            && request.left_visible
            && requested_left <= ELECTRONICS_RAIL_WIDTH
        {
            left_width = ELECTRONICS_RAIL_WIDTH.min(width);
            right_width =
                requested_right.min((width - canvas_min.min(width) - left_width).max(0.0));
        }

        let application_bar = EditorRect::new(0.0, 0.0, width, app_height);
        let workspace = EditorRect::new(0.0, workspace_y, width, workspace_height);
        let bottom_y = workspace_y + workspace_height;
        // Electronics keeps its navigator as a full-height authoring rail.
        // The dock and status bar start after that rail, while Game keeps the
        // original full-width bottom composition.
        let electronics_left_rail = request.kind == EditorWorkbenchKind::Electronics
            && request.left_visible
            && left_width > 0.0;
        let bottom_x = if electronics_left_rail {
            left_width
        } else {
            0.0
        };
        let bottom_width = (width
            - bottom_x
            - if request.kind == EditorWorkbenchKind::Electronics {
                right_width
            } else {
                0.0
            })
        .max(0.0);
        let bottom_dock = EditorRect::new(bottom_x, bottom_y, bottom_width, dock_height);
        let status_bar = EditorRect::new(0.0, bottom_y + dock_height, width, status_height);
        let canvas = EditorRect::new(
            left_width,
            workspace_y,
            (width - left_width - right_width).max(0.0),
            workspace_height,
        );

        Self {
            window: EditorRect::new(0.0, 0.0, width, height),
            application_bar,
            workspace,
            left_panel: request.left_visible.then_some(EditorRect::new(
                0.0,
                workspace_y,
                left_width,
                if electronics_left_rail {
                    (height - workspace_y - status_height).max(0.0)
                } else {
                    workspace_height
                },
            )),
            canvas,
            right_panel: request.right_visible.then_some(EditorRect::new(
                width - right_width,
                workspace_y,
                right_width,
                if request.kind == EditorWorkbenchKind::Electronics {
                    (height - workspace_y - status_height).max(0.0)
                } else {
                    workspace_height
                },
            )),
            bottom_dock,
            status_bar,
        }
    }
}

fn fit_side_panels(
    total_width: f32,
    requested_left: f32,
    requested_right: f32,
    left_visible: bool,
    right_visible: bool,
    canvas_min: f32,
) -> (f32, f32) {
    let side_budget = (total_width - canvas_min).max(0.0);
    let requested_total = requested_left + requested_right;
    if requested_total <= side_budget || requested_total <= f32::EPSILON {
        return (requested_left, requested_right);
    }

    let visible_count = u8::from(left_visible) + u8::from(right_visible);
    if visible_count == 0 {
        return (0.0, 0.0);
    }
    if visible_count == 1 {
        return if left_visible {
            (side_budget, 0.0)
        } else {
            (0.0, side_budget)
        };
    }

    let left_share = requested_left / requested_total;
    (side_budget * left_share, side_budget * (1.0 - left_share))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn electronics_docks_frame_the_canvas_without_changing_the_game_layout() {
        let layout = EditorFrameLayout::compute(EditorLayoutRequest {
            right_visible: true,
            bottom_expanded: true,
            bottom_height: 220.0,
            ..EditorLayoutRequest::electronics([1600.0, 1000.0])
        });
        let toolbar = layout.electronics_toolbar_rect();
        assert_eq!(toolbar.x, 0.0);
        assert_eq!(toolbar.width, 1600.0);
        let right = layout.right_panel.unwrap();
        assert_eq!(right.y, toolbar.y + toolbar.height);
        assert_eq!(layout.left_panel.unwrap().y, right.y);
        assert_eq!(right.y + right.height, layout.status_bar.y);
        assert_eq!(layout.bottom_dock.x + layout.bottom_dock.width, right.x);
        let cad = layout.electronics_canvas_for_pcb(true);
        assert!(cad.y > toolbar.y + toolbar.height && cad.x > layout.canvas.x);
        assert_eq!(cad.y + cad.height, layout.bottom_dock.y);
        let game = EditorFrameLayout::compute(EditorLayoutRequest::game([1600.0, 1000.0]));
        assert_eq!(game.canvas.y, APPLICATION_BAR_HEIGHT);
        assert_eq!(game.bottom_dock.width, 1600.0);
        assert_eq!(game.right_panel.unwrap().height, game.workspace.height);
    }

    #[test]
    fn electronics_left_rail_runs_behind_its_bottom_dock() {
        let layout = EditorFrameLayout::compute(EditorLayoutRequest::electronics([1600.0, 900.0]));
        let left = layout.left_panel.expect("electronics left rail");

        assert_eq!(left.x, 0.0);
        assert_eq!(left.y, APPLICATION_BAR_HEIGHT + ELECTRONICS_TOOLBAR_HEIGHT);
        assert_eq!(
            left.height,
            900.0 - APPLICATION_BAR_HEIGHT - ELECTRONICS_TOOLBAR_HEIGHT - EDITOR_STATUS_HEIGHT
        );
        assert_eq!(layout.bottom_dock.x, left.width);
        assert_eq!(layout.status_bar.x, 0.0);
    }

    #[test]
    fn game_keeps_full_width_bottom_dock_and_workspace_height_left_panel() {
        let layout = EditorFrameLayout::compute(EditorLayoutRequest::game([1600.0, 900.0]));
        let left = layout.left_panel.expect("game left panel");

        assert_eq!(layout.bottom_dock.x, 0.0);
        assert_eq!(layout.bottom_dock.width, 1600.0);
        assert_eq!(layout.status_bar.x, 0.0);
        assert_eq!(left.height, layout.workspace.height);
    }

    #[test]
    fn electronics_canvas_starts_below_its_rafui_toolbar() {
        let layout = EditorFrameLayout::compute(EditorLayoutRequest::electronics([1600.0, 900.0]));
        let canvas = layout.electronics_canvas();

        assert_eq!(
            canvas.x,
            layout.canvas.x + crate::panels::electronics_desktop_surface::TOOL_RAIL_WIDTH
        );
        assert_eq!(
            canvas.y,
            layout.canvas.y + crate::panels::electronics_desktop_surface::DOCUMENT_HEIGHT
        );
        assert_eq!(
            canvas.width,
            layout.canvas.width - crate::panels::electronics_desktop_surface::TOOL_RAIL_WIDTH
        );
        assert_eq!(
            canvas.height,
            layout.canvas.height - crate::panels::electronics_desktop_surface::DOCUMENT_HEIGHT
        );
    }

    #[test]
    fn electronics_pcb_controls_are_reserved_in_the_shared_canvas_rectangle() {
        for width in [720.0, 1280.0, 1920.0] {
            let layout =
                EditorFrameLayout::compute(EditorLayoutRequest::electronics([width, 900.0]));
            let schematic = layout.electronics_canvas();
            let pcb = layout.electronics_canvas_for_pcb(true);
            let controls =
                crate::panels::pcb_tool_options_surface::pcb_tool_options_height(schematic.width);
            assert_eq!(
                pcb.y,
                schematic.y + controls + crate::panels::electronics_desktop_surface::RULER_HEIGHT
            );
            assert_eq!(
                pcb.height,
                schematic.height
                    - controls
                    - crate::panels::electronics_desktop_surface::RULER_HEIGHT
            );
            assert_eq!(
                pcb.width,
                schematic.width - crate::panels::electronics_desktop_surface::RULER_WIDTH
            );
            assert_eq!(pcb.y + pcb.height, schematic.y + schematic.height);
        }
    }

    #[test]
    fn electronics_starts_with_canvas_space_and_supports_a_compact_pcb_rail() {
        let request = EditorLayoutRequest::electronics([1600.0, 900.0]);
        let schematic = EditorFrameLayout::compute(request);
        assert!(schematic.right_panel.is_none());
        assert_eq!(schematic.bottom_dock.height, EDITOR_DOCK_COLLAPSED_HEIGHT);
        assert_eq!(schematic.canvas.width, 1340.0);
        let pcb = EditorFrameLayout::compute(EditorLayoutRequest {
            left_width: ELECTRONICS_RAIL_WIDTH,
            right_visible: true,
            ..request
        });
        assert_eq!(pcb.left_panel.unwrap().width, ELECTRONICS_RAIL_WIDTH);
        assert_eq!(pcb.canvas.width, 1248.0);
    }

    #[test]
    fn a_narrow_electronics_canvas_reserves_the_stacked_band() {
        // The default window keeps the one-row band, so the common case does not
        // give up canvas height.
        let wide = EditorFrameLayout::compute(EditorLayoutRequest::electronics([1600.0, 900.0]));
        assert!(wide.canvas.width >= ELECTRONICS_TOOLBAR_LABELLED_WIDTH);
        assert_eq!(
            electronics_toolbar_height(wide.canvas.width),
            ELECTRONICS_TOOLBAR_HEIGHT
        );

        // A window that leaves a canvas too narrow for one row reserves the two
        // row band instead, so no control is ever clipped out of reach.
        let narrow = EditorFrameLayout::compute(EditorLayoutRequest::electronics([760.0, 800.0]));
        assert!(narrow.canvas.width < ELECTRONICS_TOOLBAR_STACKED_WIDTH);
        assert_eq!(
            electronics_toolbar_height(narrow.canvas.width),
            ELECTRONICS_TOOLBAR_STACKED_HEIGHT
        );
        let canvas = narrow.electronics_canvas();
        assert_eq!(
            canvas.y,
            narrow.canvas.y + crate::panels::electronics_desktop_surface::DOCUMENT_HEIGHT
        );
        assert_eq!(
            canvas.height,
            narrow.canvas.height - crate::panels::electronics_desktop_surface::DOCUMENT_HEIGHT
        );
        // The label breakpoint sits between the two, so it never asks for a band
        // the layout would not reserve.
        assert!(ELECTRONICS_TOOLBAR_STACKED_WIDTH < ELECTRONICS_TOOLBAR_LABELLED_WIDTH);
        assert_eq!(
            electronics_toolbar_height(ELECTRONICS_TOOLBAR_LABELLED_WIDTH),
            ELECTRONICS_TOOLBAR_HEIGHT
        );
        assert_eq!(
            electronics_toolbar_height(ELECTRONICS_TOOLBAR_STACKED_WIDTH),
            ELECTRONICS_TOOLBAR_HEIGHT
        );
        assert_eq!(
            electronics_toolbar_height(ELECTRONICS_TOOLBAR_STACKED_WIDTH - 1.0),
            ELECTRONICS_TOOLBAR_STACKED_HEIGHT
        );
    }
}
