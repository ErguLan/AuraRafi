//! Backend-neutral editor workbench layout.
//!
//! The native shell, RafUI hit testing, canvas input, and ApiGraphicBasic
//! composition all consume the same rectangles. This removes the old split
//! where separate presentation layers guessed placement and pointer ownership.

use raf_render::api_graphic_basic::CanvasTargetRect;

use crate::application_bar_surface::APPLICATION_BAR_HEIGHT;

pub const EDITOR_STATUS_HEIGHT: f32 = 28.0;
pub const EDITOR_DOCK_COLLAPSED_HEIGHT: f32 = 32.0;
pub const EDITOR_DOCK_MIN_HEIGHT: f32 = 112.0;
/// Upper safety bound for the user-resizable downbar. The effective maximum
/// is still limited by the current window so a small viewport remains usable.
pub const EDITOR_DOCK_MAX_HEIGHT: f32 = 4096.0;
pub const EDITOR_DOCK_MIN_WORKSPACE_HEIGHT: f32 = 120.0;
pub const ELECTRONICS_TOOLBAR_HEIGHT: f32 = 46.0;

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
            right_visible: true,
            // Electronics needs enough room for designators, values and
            // inspector fields. Keep this wider only for the CAD workbench;
            // Game keeps its own compact/default layout below.
            left_width: 356.0,
            right_width: 400.0,
            bottom_expanded: true,
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
    pub fn electronics_canvas(self) -> EditorRect {
        let toolbar_height = ELECTRONICS_TOOLBAR_HEIGHT.min(self.canvas.height.max(0.0));
        EditorRect::new(
            self.canvas.x,
            self.canvas.y + toolbar_height,
            self.canvas.width,
            (self.canvas.height - toolbar_height).max(0.0),
        )
    }

    pub fn compute(request: EditorLayoutRequest) -> Self {
        let width = request.logical_size[0].max(1.0);
        let height = request.logical_size[1].max(1.0);
        let app_height = APPLICATION_BAR_HEIGHT.min(height);
        let status_height = EDITOR_STATUS_HEIGHT.min((height - app_height).max(0.0));
        let available_below_bar = (height - app_height - status_height).max(0.0);
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
            EditorWorkbenchKind::Electronics => (224.0, 260.0, 360.0),
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
        let (left_width, right_width) = fit_side_panels(
            width,
            requested_left,
            requested_right,
            request.left_visible,
            request.right_visible,
            canvas_min.min(width),
        );

        let application_bar = EditorRect::new(0.0, 0.0, width, app_height);
        let workspace = EditorRect::new(0.0, app_height, width, workspace_height);
        let bottom_y = app_height + workspace_height;
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
        let bottom_width = (width - bottom_x).max(0.0);
        let bottom_dock = EditorRect::new(bottom_x, bottom_y, bottom_width, dock_height);
        let status_bar = EditorRect::new(
            bottom_x,
            bottom_y + dock_height,
            bottom_width,
            status_height,
        );
        let canvas = EditorRect::new(
            left_width,
            app_height,
            (width - left_width - right_width).max(0.0),
            workspace_height,
        );

        Self {
            window: EditorRect::new(0.0, 0.0, width, height),
            application_bar,
            workspace,
            left_panel: request.left_visible.then_some(EditorRect::new(
                0.0,
                app_height,
                left_width,
                if electronics_left_rail {
                    (height - app_height).max(0.0)
                } else {
                    workspace_height
                },
            )),
            canvas,
            right_panel: request.right_visible.then_some(EditorRect::new(
                width - right_width,
                app_height,
                right_width,
                workspace_height,
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
    fn electronics_left_rail_runs_behind_its_bottom_dock() {
        let layout = EditorFrameLayout::compute(EditorLayoutRequest::electronics([1600.0, 900.0]));
        let left = layout.left_panel.expect("electronics left rail");

        assert_eq!(left.x, 0.0);
        assert_eq!(left.y, APPLICATION_BAR_HEIGHT);
        assert_eq!(left.height, 900.0 - APPLICATION_BAR_HEIGHT);
        assert_eq!(layout.bottom_dock.x, left.width);
        assert_eq!(layout.status_bar.x, left.width);
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

        assert_eq!(canvas.x, layout.canvas.x);
        assert_eq!(canvas.y, layout.canvas.y + ELECTRONICS_TOOLBAR_HEIGHT);
        assert_eq!(canvas.width, layout.canvas.width);
        assert_eq!(
            canvas.height,
            layout.canvas.height - ELECTRONICS_TOOLBAR_HEIGHT
        );
    }
}
