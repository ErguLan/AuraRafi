use serde::{Deserialize, Serialize};

use crate::events::UiPointerButton;
use crate::focus::UiInputState;
use crate::geometry::UiRect;

/// Height reserved at the top of a floating panel for its drag handle.
pub const FLOATING_PANEL_TITLE_BAR_HEIGHT: f32 = 28.0;
/// Square interaction zone used to resize a floating panel from its lower-right corner.
pub const FLOATING_PANEL_RESIZE_HANDLE_SIZE: f32 = 14.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DockSide {
    Left,
    Right,
    Top,
    Bottom,
    Center,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DockPanel {
    pub id: String,
    pub title_key: String,
    pub side: DockSide,
    pub min_size: [f32; 2],
    pub preferred_size: [f32; 2],
    pub visible: bool,
}

impl DockPanel {
    pub fn new(id: impl Into<String>, title_key: impl Into<String>, side: DockSide) -> Self {
        Self {
            id: id.into(),
            title_key: title_key.into(),
            side,
            min_size: [180.0, 120.0],
            preferred_size: [260.0, 320.0],
            visible: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FloatingPanel {
    pub id: String,
    pub title_key: String,
    pub rect: UiRect,
    pub min_size: [f32; 2],
    pub visible: bool,
    pub z_index: i16,
}

impl FloatingPanel {
    pub fn new(id: impl Into<String>, title_key: impl Into<String>, rect: UiRect) -> Self {
        Self {
            id: id.into(),
            title_key: title_key.into(),
            rect,
            min_size: [180.0, 120.0],
            visible: true,
            z_index: 0,
        }
    }

    pub fn clamp_to(mut self, bounds: UiRect) -> Self {
        self.rect = self.rect.clamp_inside(bounds);
        self
    }

    pub fn title_bar_rect(&self) -> UiRect {
        UiRect::new(
            self.rect.x,
            self.rect.y,
            self.rect.width,
            self.rect.height.min(FLOATING_PANEL_TITLE_BAR_HEIGHT),
        )
    }

    pub fn resize_handle_rect(&self) -> UiRect {
        let size = FLOATING_PANEL_RESIZE_HANDLE_SIZE
            .min(self.rect.width)
            .min(self.rect.height);
        UiRect::new(
            self.rect.right() - size,
            self.rect.bottom() - size,
            size,
            size,
        )
    }
}

/// A side that accepts a floating panel when its title bar is released near a
/// workspace boundary. Center is intentionally absent: releasing in the
/// center keeps the panel floating instead of creating an ambiguous dock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockDropTarget {
    Left,
    Right,
    Top,
    Bottom,
}

impl DockDropTarget {
    fn into_side(self) -> DockSide {
        match self {
            Self::Left => DockSide::Left,
            Self::Right => DockSide::Right,
            Self::Top => DockSide::Top,
            Self::Bottom => DockSide::Bottom,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DockLayout {
    pub panels: Vec<DockPanel>,
    pub floating: Vec<FloatingPanel>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DockLayoutEntry {
    pub id: String,
    pub rect: UiRect,
    pub floating: bool,
    pub z_index: i16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DockLayoutFrame {
    pub workspace: UiRect,
    pub center: UiRect,
    pub entries: Vec<DockLayoutEntry>,
}

/// Transient gesture state for a serializable `DockLayout`.
///
/// Layout data persists with the workspace; this controller remains per live
/// surface so input capture cannot leak into project files or another window.
#[derive(Debug, Clone, PartialEq)]
pub struct DockWorkspaceController {
    active_gesture: Option<DockWorkspaceGesture>,
    primary_was_down: bool,
    pub snap_distance: f32,
}

#[derive(Debug, Clone, PartialEq)]
enum DockWorkspaceGesture {
    Move {
        id: String,
        pointer_offset: [f32; 2],
    },
    Resize {
        id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockWorkspaceEvent {
    FloatingRaised { id: String },
    FloatingMoved { id: String },
    FloatingResized { id: String },
    FloatingDocked { id: String, side: DockSide },
}

impl Default for DockWorkspaceController {
    fn default() -> Self {
        Self {
            active_gesture: None,
            primary_was_down: false,
            snap_distance: 28.0,
        }
    }
}

impl DockLayout {
    pub fn studio_default() -> Self {
        Self {
            panels: vec![
                DockPanel::new("hierarchy", "panel.hierarchy", DockSide::Left),
                DockPanel::new("properties", "panel.properties", DockSide::Right),
                DockPanel::new("assets", "panel.assets", DockSide::Bottom),
            ],
            floating: Vec::new(),
        }
    }

    pub fn visible_docked_panels(&self, side: DockSide) -> impl Iterator<Item = &DockPanel> {
        self.panels
            .iter()
            .filter(move |panel| panel.visible && panel.side == side)
    }

    pub fn set_panel_visible(&mut self, id: &str, visible: bool) -> bool {
        if let Some(panel) = self.panels.iter_mut().find(|panel| panel.id == id) {
            panel.visible = visible;
            return true;
        }
        if let Some(panel) = self.floating.iter_mut().find(|panel| panel.id == id) {
            panel.visible = visible;
            return true;
        }
        false
    }

    pub fn clamp_floating_to(&mut self, bounds: UiRect) {
        for panel in &mut self.floating {
            panel.rect = panel.rect.clamp_inside(bounds);
        }
    }

    pub fn floating_panel(&self, id: &str) -> Option<&FloatingPanel> {
        self.floating.iter().find(|panel| panel.id == id)
    }

    pub fn undock_panel(&mut self, id: &str, rect: UiRect, bounds: UiRect) -> bool {
        let Some(index) = self.panels.iter().position(|panel| panel.id == id) else {
            return false;
        };
        let panel = self.panels.remove(index);
        let mut floating = FloatingPanel::new(panel.id, panel.title_key, rect.clamp_inside(bounds));
        floating.min_size = panel.min_size;
        floating.z_index = self
            .floating
            .iter()
            .map(|candidate| candidate.z_index)
            .max()
            .unwrap_or(-1)
            .saturating_add(1);
        self.floating.push(floating);
        true
    }

    pub fn dock_floating(&mut self, id: &str, side: DockSide) -> bool {
        if side == DockSide::Center || self.panels.iter().any(|panel| panel.id == id) {
            return false;
        }
        let Some(index) = self.floating.iter().position(|panel| panel.id == id) else {
            return false;
        };
        let floating = self.floating.remove(index);
        self.panels.push(DockPanel {
            id: floating.id,
            title_key: floating.title_key,
            side,
            min_size: floating.min_size,
            preferred_size: [floating.rect.width, floating.rect.height],
            visible: floating.visible,
        });
        true
    }

    /// Changes the persisted preferred dimension for one docked panel. The
    /// resolver remains responsible for keeping all tracks within the current
    /// workspace, so a resize never creates an invalid saved layout.
    pub fn resize_docked_to(&mut self, id: &str, size: f32, workspace: UiRect) -> bool {
        let Some(panel) = self.panels.iter_mut().find(|panel| panel.id == id) else {
            return false;
        };
        let (minimum, maximum, slot) = match panel.side {
            DockSide::Left | DockSide::Right => (
                panel.min_size[0],
                (workspace.width * 0.45).max(panel.min_size[0]),
                &mut panel.preferred_size[0],
            ),
            DockSide::Top | DockSide::Bottom => (
                panel.min_size[1],
                (workspace.height * 0.45).max(panel.min_size[1]),
                &mut panel.preferred_size[1],
            ),
            DockSide::Center => return false,
        };
        let next = size.max(minimum).min(maximum);
        let changed = (*slot - next).abs() > f32::EPSILON;
        *slot = next;
        changed
    }

    pub fn raise_floating(&mut self, id: &str) -> bool {
        let Some(index) = self.floating.iter().position(|panel| panel.id == id) else {
            return false;
        };
        let next_z = self
            .floating
            .iter()
            .map(|panel| panel.z_index)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.floating[index].z_index = next_z;
        self.floating.sort_by_key(|panel| panel.z_index);
        true
    }

    /// Moves a floating panel while keeping its full rectangle in the studio
    /// workspace. Raising happens as part of the drag so overlapping panels
    /// retain deterministic pointer priority.
    pub fn move_floating_to(&mut self, id: &str, position: [f32; 2], bounds: UiRect) -> bool {
        let Some(panel) = self.floating.iter_mut().find(|panel| panel.id == id) else {
            return false;
        };

        panel.rect.x = position[0];
        panel.rect.y = position[1];
        panel.rect = panel.rect.clamp_inside(bounds);
        self.raise_floating(id)
    }

    /// Resizes a floating panel from its top-left corner with a stable
    /// minimum size and workspace clamp.
    pub fn resize_floating_to(&mut self, id: &str, size: [f32; 2], bounds: UiRect) -> bool {
        let Some(panel) = self.floating.iter_mut().find(|panel| panel.id == id) else {
            return false;
        };

        let max_width = (bounds.right() - panel.rect.x).max(panel.min_size[0]);
        let max_height = (bounds.bottom() - panel.rect.y).max(panel.min_size[1]);
        panel.rect.width = size[0].max(panel.min_size[0]).min(max_width);
        panel.rect.height = size[1].max(panel.min_size[1]).min(max_height);
        panel.rect = panel.rect.clamp_inside(bounds);
        self.raise_floating(id)
    }

    pub fn drop_target_at(
        &self,
        workspace: UiRect,
        point: [f32; 2],
        snap_distance: f32,
    ) -> Option<DockDropTarget> {
        if !workspace.contains(point) {
            return None;
        }
        let snap_distance = snap_distance.max(0.0);
        let candidates = [
            (point[0] - workspace.x, DockDropTarget::Left),
            (workspace.right() - point[0], DockDropTarget::Right),
            (point[1] - workspace.y, DockDropTarget::Top),
            (workspace.bottom() - point[1], DockDropTarget::Bottom),
        ];
        candidates
            .into_iter()
            .filter(|(distance, _)| *distance <= snap_distance)
            .min_by(|(left, _), (right, _)| left.total_cmp(right))
            .map(|(_, target)| target)
    }

    /// Resolves persisted docking metadata into stable rectangles for a frame.
    /// Left/right consume width first, then top/bottom consume height from the
    /// remaining center. Multiple panels on one side split their track without
    /// changing the workspace dimensions between frames.
    pub fn resolve(&self, workspace: UiRect) -> DockLayoutFrame {
        let mut remaining = workspace;
        let mut entries = Vec::new();

        for side in [DockSide::Left, DockSide::Right] {
            let panels = self.visible_docked_panels(side).collect::<Vec<_>>();
            if panels.is_empty() {
                continue;
            }
            let requested = panels
                .iter()
                .map(|panel| panel.preferred_size[0].max(panel.min_size[0]))
                .fold(0.0_f32, f32::max);
            let width = requested.min((remaining.width * 0.45).max(0.0));
            if width <= 0.0 {
                continue;
            }
            let x = if side == DockSide::Left {
                remaining.x
            } else {
                remaining.right() - width
            };
            split_track(
                &panels,
                UiRect::new(x, remaining.y, width, remaining.height),
                true,
                &mut entries,
            );
            if side == DockSide::Left {
                remaining.x += width;
            }
            remaining.width = (remaining.width - width).max(0.0);
        }

        for side in [DockSide::Top, DockSide::Bottom] {
            let panels = self.visible_docked_panels(side).collect::<Vec<_>>();
            if panels.is_empty() {
                continue;
            }
            let requested = panels
                .iter()
                .map(|panel| panel.preferred_size[1].max(panel.min_size[1]))
                .fold(0.0_f32, f32::max);
            let height = requested.min((remaining.height * 0.45).max(0.0));
            if height <= 0.0 {
                continue;
            }
            let y = if side == DockSide::Top {
                remaining.y
            } else {
                remaining.bottom() - height
            };
            split_track(
                &panels,
                UiRect::new(remaining.x, y, remaining.width, height),
                false,
                &mut entries,
            );
            if side == DockSide::Top {
                remaining.y += height;
            }
            remaining.height = (remaining.height - height).max(0.0);
        }

        for panel in self.visible_docked_panels(DockSide::Center) {
            entries.push(DockLayoutEntry {
                id: panel.id.clone(),
                rect: remaining,
                floating: false,
                z_index: 0,
            });
        }
        for panel in self.floating.iter().filter(|panel| panel.visible) {
            entries.push(DockLayoutEntry {
                id: panel.id.clone(),
                rect: panel.rect.clamp_inside(workspace),
                floating: true,
                z_index: panel.z_index,
            });
        }
        entries.sort_by_key(|entry| (entry.floating, entry.z_index));

        DockLayoutFrame {
            workspace,
            center: remaining,
            entries,
        }
    }
}

impl DockWorkspaceController {
    /// Applies one input snapshot to floating-panel chrome. A caller can use
    /// the returned events to redraw only affected panel content or persist the
    /// workspace at an appropriate debounce boundary.
    pub fn update(
        &mut self,
        layout: &mut DockLayout,
        workspace: UiRect,
        input: &UiInputState,
    ) -> Vec<DockWorkspaceEvent> {
        let primary_down = input.button_down(UiPointerButton::Primary);
        let primary_pressed = input.button_pressed(UiPointerButton::Primary)
            || (primary_down && !self.primary_was_down);
        let primary_released = input.button_released(UiPointerButton::Primary)
            || (!primary_down && self.primary_was_down);
        let pointer = input.pointer_position;
        let mut events = Vec::new();

        if primary_pressed {
            if let Some(point) = pointer {
                if let Some((id, is_resize)) = floating_chrome_hit(layout, point) {
                    layout.raise_floating(&id);
                    events.push(DockWorkspaceEvent::FloatingRaised { id: id.clone() });
                    if is_resize {
                        self.active_gesture = Some(DockWorkspaceGesture::Resize { id });
                    } else if let Some(panel) = layout.floating_panel(&id) {
                        self.active_gesture = Some(DockWorkspaceGesture::Move {
                            id,
                            pointer_offset: [point[0] - panel.rect.x, point[1] - panel.rect.y],
                        });
                    }
                }
            }
        }

        if primary_down {
            if let (Some(point), Some(gesture)) = (pointer, self.active_gesture.as_ref()) {
                match gesture {
                    DockWorkspaceGesture::Move { id, pointer_offset } => {
                        if layout.move_floating_to(
                            id,
                            [point[0] - pointer_offset[0], point[1] - pointer_offset[1]],
                            workspace,
                        ) {
                            events.push(DockWorkspaceEvent::FloatingMoved { id: id.clone() });
                        }
                    }
                    DockWorkspaceGesture::Resize { id } => {
                        if let Some(panel) = layout.floating_panel(id) {
                            let size = [point[0] - panel.rect.x, point[1] - panel.rect.y];
                            if layout.resize_floating_to(id, size, workspace) {
                                events.push(DockWorkspaceEvent::FloatingResized { id: id.clone() });
                            }
                        }
                    }
                }
            }
        }

        if primary_released {
            if let (Some(point), Some(DockWorkspaceGesture::Move { id, .. })) =
                (pointer, self.active_gesture.take())
            {
                if let Some(target) = layout.drop_target_at(workspace, point, self.snap_distance) {
                    let side = target.into_side();
                    if layout.dock_floating(&id, side) {
                        events.push(DockWorkspaceEvent::FloatingDocked { id, side });
                    }
                }
            } else {
                self.active_gesture = None;
            }
        }

        self.primary_was_down = primary_down;
        events
    }

    pub fn cancel(&mut self) {
        self.active_gesture = None;
        self.primary_was_down = false;
    }
}

fn floating_chrome_hit(layout: &DockLayout, point: [f32; 2]) -> Option<(String, bool)> {
    layout
        .floating
        .iter()
        .filter(|panel| {
            panel.visible
                && (panel.resize_handle_rect().contains(point)
                    || panel.title_bar_rect().contains(point))
        })
        .max_by_key(|panel| panel.z_index)
        .map(|panel| (panel.id.clone(), panel.resize_handle_rect().contains(point)))
}

fn split_track(
    panels: &[&DockPanel],
    track: UiRect,
    vertical: bool,
    entries: &mut Vec<DockLayoutEntry>,
) {
    let length = if vertical { track.height } else { track.width };
    if panels.is_empty() || length <= 0.0 {
        return;
    }
    let axis = if vertical { 1 } else { 0 };
    let minimums = panels
        .iter()
        .map(|panel| panel.min_size[axis].max(0.0))
        .collect::<Vec<_>>();
    let preferred = panels
        .iter()
        .zip(minimums.iter().copied())
        .map(|(panel, minimum)| panel.preferred_size[axis].max(minimum))
        .collect::<Vec<_>>();
    let minimum_total = minimums.iter().sum::<f32>();
    let preferred_total = preferred.iter().sum::<f32>();
    let count = panels.len() as f32;
    let sizes = if minimum_total > length {
        // A tiny host window cannot physically honor every minimum. Split the
        // available track evenly rather than producing overlap or negatives.
        vec![length / count; panels.len()]
    } else if preferred_total <= length {
        let extra = (length - preferred_total) / count;
        preferred
            .iter()
            .map(|size| size + extra)
            .collect::<Vec<_>>()
    } else {
        let remaining = length - minimum_total;
        let flexible_total = preferred
            .iter()
            .zip(minimums.iter())
            .map(|(preferred, minimum)| preferred - minimum)
            .sum::<f32>();
        if flexible_total <= f32::EPSILON {
            vec![length / count; panels.len()]
        } else {
            preferred
                .iter()
                .zip(minimums.iter())
                .map(|(preferred, minimum)| {
                    minimum + remaining * ((preferred - minimum) / flexible_total)
                })
                .collect::<Vec<_>>()
        }
    };
    let mut cursor = if vertical { track.y } else { track.x };
    for (index, panel) in panels.iter().enumerate() {
        let size = if index + 1 == panels.len() {
            if vertical {
                track.bottom() - cursor
            } else {
                track.right() - cursor
            }
        } else {
            sizes[index].max(0.0)
        };
        let rect = if vertical {
            UiRect::new(track.x, cursor, track.width, size)
        } else {
            UiRect::new(cursor, track.y, size, track.height)
        };
        entries.push(DockLayoutEntry {
            id: panel.id.clone(),
            rect,
            floating: false,
            z_index: 0,
        });
        cursor += size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floating_panel_clamps_inside_workspace() {
        let panel = FloatingPanel::new(
            "tools",
            "panel.tools",
            UiRect::new(900.0, -30.0, 300.0, 240.0),
        )
        .clamp_to(UiRect::new(0.0, 0.0, 1024.0, 768.0));

        assert_eq!(panel.rect.x, 724.0);
        assert_eq!(panel.rect.y, 0.0);
    }

    #[test]
    fn floating_raise_moves_panel_to_top_z() {
        let mut layout = DockLayout {
            panels: Vec::new(),
            floating: vec![
                FloatingPanel::new("a", "panel.a", UiRect::new(0.0, 0.0, 100.0, 100.0)),
                FloatingPanel::new("b", "panel.b", UiRect::new(0.0, 0.0, 100.0, 100.0)),
            ],
        };
        layout.floating[0].z_index = 1;
        layout.floating[1].z_index = 2;

        assert!(layout.raise_floating("a"));

        assert_eq!(layout.floating.last().unwrap().id, "a");
    }

    #[test]
    fn moving_and_resizing_floating_panel_respects_workspace() {
        let bounds = UiRect::new(0.0, 0.0, 640.0, 480.0);
        let mut layout = DockLayout {
            panels: Vec::new(),
            floating: vec![FloatingPanel::new(
                "tools",
                "panel.tools",
                UiRect::new(20.0, 20.0, 200.0, 140.0),
            )],
        };

        assert!(layout.move_floating_to("tools", [600.0, 450.0], bounds));
        assert!(layout.resize_floating_to("tools", [20.0, 20.0], bounds));

        let panel = &layout.floating[0];
        assert!(panel.rect.right() <= bounds.right());
        assert!(panel.rect.bottom() <= bounds.bottom());
        assert!(panel.rect.width >= panel.min_size[0]);
        assert!(panel.rect.height >= panel.min_size[1]);
    }

    #[test]
    fn resolve_reserves_tracks_without_overlapping_center() {
        let layout = DockLayout {
            panels: vec![
                DockPanel::new("left", "panel.left", DockSide::Left),
                DockPanel::new("bottom", "panel.bottom", DockSide::Bottom),
                DockPanel::new("center", "panel.center", DockSide::Center),
            ],
            floating: Vec::new(),
        };
        let frame = layout.resolve(UiRect::new(0.0, 0.0, 1200.0, 800.0));
        let center = frame
            .entries
            .iter()
            .find(|entry| entry.id == "center")
            .unwrap();
        let left = frame
            .entries
            .iter()
            .find(|entry| entry.id == "left")
            .unwrap();
        let bottom = frame
            .entries
            .iter()
            .find(|entry| entry.id == "bottom")
            .unwrap();

        assert!(center.rect.x >= left.rect.right());
        assert!(center.rect.bottom() <= bottom.rect.y);
    }

    #[test]
    fn controller_docks_a_floating_panel_when_title_reaches_workspace_edge() {
        let workspace = UiRect::new(0.0, 0.0, 640.0, 480.0);
        let mut layout = DockLayout {
            panels: Vec::new(),
            floating: vec![FloatingPanel::new(
                "inspector",
                "panel.inspector",
                UiRect::new(200.0, 20.0, 220.0, 180.0),
            )],
        };
        let mut controller = DockWorkspaceController::default();

        controller.update(
            &mut layout,
            workspace,
            &UiInputState {
                pointer_position: Some([220.0, 34.0]),
                pointer_down: true,
                pointer_pressed_buttons: vec![UiPointerButton::Primary],
                ..UiInputState::default()
            },
        );
        controller.update(
            &mut layout,
            workspace,
            &UiInputState {
                pointer_position: Some([4.0, 34.0]),
                pointer_down: true,
                ..UiInputState::default()
            },
        );
        let events = controller.update(
            &mut layout,
            workspace,
            &UiInputState {
                pointer_position: Some([4.0, 34.0]),
                pointer_released_buttons: vec![UiPointerButton::Primary],
                ..UiInputState::default()
            },
        );

        assert!(events.iter().any(|event| matches!(
            event,
            DockWorkspaceEvent::FloatingDocked { id, side: DockSide::Left }
                if id == "inspector"
        )));
        assert!(layout.floating.is_empty());
        assert!(layout
            .panels
            .iter()
            .any(|panel| panel.id == "inspector" && panel.side == DockSide::Left));
    }

    #[test]
    fn undock_preserves_panel_identity_and_minimum_size() {
        let workspace = UiRect::new(0.0, 0.0, 640.0, 480.0);
        let mut layout = DockLayout {
            panels: vec![DockPanel::new("assets", "panel.assets", DockSide::Bottom)],
            floating: Vec::new(),
        };
        layout.panels[0].min_size = [240.0, 160.0];

        assert!(layout.undock_panel("assets", UiRect::new(500.0, 420.0, 300.0, 220.0), workspace,));

        assert!(layout.panels.is_empty());
        let floating = layout.floating_panel("assets").unwrap();
        assert_eq!(floating.min_size, [240.0, 160.0]);
        assert!(floating.rect.right() <= workspace.right());
        assert!(floating.rect.bottom() <= workspace.bottom());
    }

    #[test]
    fn crowded_track_never_overlaps_or_escapes_its_available_area() {
        let layout = DockLayout {
            panels: vec![
                DockPanel::new("left-a", "panel.left_a", DockSide::Left),
                DockPanel::new("left-b", "panel.left_b", DockSide::Left),
                DockPanel::new("left-c", "panel.left_c", DockSide::Left),
            ],
            floating: Vec::new(),
        };
        let frame = layout.resolve(UiRect::new(0.0, 0.0, 600.0, 180.0));
        let mut entries = frame
            .entries
            .iter()
            .filter(|entry| entry.id.starts_with("left-"))
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.rect.y.total_cmp(&right.rect.y));

        assert_eq!(entries.len(), 3);
        assert!(entries
            .windows(2)
            .all(|pair| pair[0].rect.bottom() <= pair[1].rect.y));
        assert!(entries.last().unwrap().rect.bottom() <= frame.workspace.bottom());
    }
}
