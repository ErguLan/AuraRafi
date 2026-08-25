use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::events::UiPointerButton;
use crate::focus::UiInputState;
use crate::geometry::UiRect;
use crate::icons::UiIconId;

/// Height reserved at the top of a floating panel for its drag handle.
pub const FLOATING_PANEL_TITLE_BAR_HEIGHT: f32 = 28.0;
/// Square interaction zone used to resize a floating panel from its lower-right corner.
pub const FLOATING_PANEL_RESIZE_HANDLE_SIZE: f32 = 14.0;

/// Version of the serializable beta bottom-dock document.
///
/// Version 3 intentionally resets the pre-stabilization split layouts once.
/// Those files can contain a visually valid-looking arrangement that still
/// has stale group interaction state and cannot be moved reliably.
pub const BOTTOM_DOCK_LAYOUT_VERSION: u32 = 3;
/// Maximum number of independently resized bottom-dock groups.
pub const MAX_BOTTOM_DOCK_GROUPS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DockSide {
    Left,
    Right,
    Top,
    Bottom,
    Center,
}

/// A semantic tab used by a bottom dock group. The tab owns no application
/// state; its identifier is the bridge between a retained surface and the
/// editor application boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockTab {
    pub id: String,
    pub title_key: String,
    pub icon: UiIconId,
}

impl DockTab {
    pub fn new(id: impl Into<String>, title_key: impl Into<String>, icon: UiIconId) -> Self {
        Self {
            id: id.into(),
            title_key: title_key.into(),
            icon,
        }
    }
}

/// One horizontal track in the bottom dock. `weight` is relative to the
/// other visible groups and `min_width` protects compact utility surfaces from
/// being squeezed into an unusable strip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DockTabGroup {
    pub id: String,
    pub tabs: Vec<DockTab>,
    pub active_tab: String,
    #[serde(default = "default_bottom_group_weight")]
    pub weight: f32,
    #[serde(default = "default_bottom_group_min_width")]
    pub min_width: f32,
}

fn default_bottom_group_weight() -> f32 {
    1.0
}

fn default_bottom_group_min_width() -> f32 {
    220.0
}

impl DockTabGroup {
    pub fn new(id: impl Into<String>, tabs: Vec<DockTab>) -> Self {
        let active_tab = tabs.first().map(|tab| tab.id.clone()).unwrap_or_default();
        Self {
            id: id.into(),
            tabs,
            active_tab,
            weight: default_bottom_group_weight(),
            min_width: default_bottom_group_min_width(),
        }
    }

    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight.max(0.01);
        self
    }

    pub fn with_min_width(mut self, min_width: f32) -> Self {
        self.min_width = min_width.max(1.0);
        self
    }

    pub fn select_tab(&mut self, tab_id: &str) -> bool {
        if self.tabs.iter().any(|tab| tab.id == tab_id) {
            let changed = self.active_tab != tab_id;
            self.active_tab = tab_id.to_string();
            changed
        } else {
            false
        }
    }

    pub fn active_tab(&self) -> Option<&DockTab> {
        self.tabs
            .iter()
            .find(|tab| tab.id == self.active_tab)
            .or_else(|| self.tabs.first())
    }
}

/// Persistable model for the beta downbar. It deliberately models only the
/// bottom horizontal composition; side/floating workspace docking remains in
/// [`DockLayout`] and can be composed with this model later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BottomDockLayout {
    #[serde(default = "default_bottom_dock_version")]
    pub version: u32,
    #[serde(default = "default_bottom_dock_height")]
    pub height: f32,
    #[serde(default = "default_bottom_dock_height")]
    pub expanded_height: f32,
    #[serde(default)]
    pub collapsed: bool,
    pub groups: Vec<DockTabGroup>,
}

fn default_bottom_dock_version() -> u32 {
    BOTTOM_DOCK_LAYOUT_VERSION
}

fn default_bottom_dock_height() -> f32 {
    238.0
}

impl BottomDockLayout {
    pub fn new(groups: Vec<DockTabGroup>) -> Self {
        Self {
            version: BOTTOM_DOCK_LAYOUT_VERSION,
            height: default_bottom_dock_height(),
            expanded_height: default_bottom_dock_height(),
            collapsed: false,
            groups,
        }
    }

    pub fn effective_height(&self, collapsed_height: f32) -> f32 {
        if self.collapsed {
            collapsed_height.max(0.0)
        } else {
            self.height.max(0.0)
        }
    }

    pub fn set_height(&mut self, height: f32, min_height: f32, max_height: f32) -> bool {
        let minimum = min_height.max(0.0);
        let maximum = max_height.max(minimum);
        let next = height.clamp(minimum, maximum);
        let changed = (self.height - next).abs() > f32::EPSILON;
        self.height = next;
        if !self.collapsed {
            self.expanded_height = next;
        }
        changed
    }

    pub fn toggle_collapsed(&mut self, min_height: f32, max_height: f32) {
        if self.collapsed {
            self.collapsed = false;
            self.height = self
                .expanded_height
                .clamp(min_height, max_height.max(min_height));
        } else {
            self.expanded_height = self.height;
            self.collapsed = true;
        }
    }

    pub fn select_tab(&mut self, group_id: &str, tab_id: &str) -> bool {
        self.groups
            .iter_mut()
            .find(|group| group.id == group_id)
            .is_some_and(|group| group.select_tab(tab_id))
    }

    /// Repairs a persisted layout before it is used by a host.
    ///
    /// Empty groups are not useful drop targets in the editor: they consume
    /// width, leave a misleading empty surface, and make the next frame's
    /// composition ambiguous. Normalization therefore removes them, clamps
    /// the group count, and restores a valid active tab for every group.
    pub fn normalize(&mut self) -> bool {
        let before = self.groups.clone();
        self.version = BOTTOM_DOCK_LAYOUT_VERSION;
        self.groups.retain(|group| !group.tabs.is_empty());
        self.groups.truncate(MAX_BOTTOM_DOCK_GROUPS);
        let mut group_ids = HashSet::new();
        let mut tab_ids = HashSet::new();
        for (index, group) in self.groups.iter_mut().enumerate() {
            if group.id.trim().is_empty() || !group_ids.insert(group.id.clone()) {
                let base = format!("bottom.group.{index}");
                let mut candidate = base.clone();
                let mut suffix = 2;
                while !group_ids.insert(candidate.clone()) {
                    candidate = format!("{base}.{suffix}");
                    suffix += 1;
                }
                group.id = candidate;
            }
            group.tabs.retain(|tab| tab_ids.insert(tab.id.clone()));
            group.weight = if group.weight.is_finite() {
                group.weight.max(0.01)
            } else {
                default_bottom_group_weight()
            };
            group.min_width = if group.min_width.is_finite() {
                group.min_width.max(1.0)
            } else {
                default_bottom_group_min_width()
            };
            if !group.tabs.iter().any(|tab| tab.id == group.active_tab) {
                group.active_tab = group
                    .tabs
                    .first()
                    .map(|tab| tab.id.clone())
                    .unwrap_or_default();
            }
        }
        self.groups.retain(|group| !group.tabs.is_empty());
        before != self.groups
    }

    /// Returns local rectangles for each visible group in left-to-right order.
    /// The resolver always consumes exactly `width`, so adjacent groups cannot
    /// overlap even when the window is narrower than their preferred minimums.
    pub fn resolve_columns(&self, width: f32, gap: f32) -> Vec<(String, UiRect)> {
        let visible = self.groups.iter().collect::<Vec<_>>();
        if visible.is_empty() || width <= 0.0 {
            return Vec::new();
        }

        let gap = gap.max(0.0);
        let content_width = (width - gap * visible.len().saturating_sub(1) as f32).max(0.0);
        let total_weight = visible
            .iter()
            .map(|group| group.weight.max(0.01))
            .sum::<f32>();
        let minimum_total = visible
            .iter()
            .map(|group| group.min_width.max(1.0))
            .sum::<f32>();
        let sizes = if minimum_total >= content_width {
            let size = content_width / visible.len() as f32;
            vec![size; visible.len()]
        } else {
            let extra = content_width - minimum_total;
            visible
                .iter()
                .map(|group| {
                    group.min_width.max(1.0) + extra * (group.weight.max(0.01) / total_weight)
                })
                .collect::<Vec<_>>()
        };

        let mut cursor = 0.0;
        visible
            .iter()
            .zip(sizes)
            .enumerate()
            .map(|(index, (group, size))| {
                let rect = UiRect::new(cursor, 0.0, size.max(0.0), 0.0);
                cursor += size.max(0.0);
                if index + 1 < visible.len() {
                    cursor += gap;
                }
                (group.id.clone(), rect)
            })
            .collect()
    }

    pub fn reorder_tab(&mut self, group_id: &str, from: usize, to: usize) -> bool {
        let Some(group) = self.groups.iter_mut().find(|group| group.id == group_id) else {
            return false;
        };
        if from >= group.tabs.len() || to >= group.tabs.len() || from == to {
            return false;
        }
        let tab = group.tabs.remove(from);
        group.tabs.insert(to, tab);
        true
    }

    /// Moves a tab within one group. The target index is evaluated after the
    /// source tab is removed, which makes drag previews stable while the tab
    /// crosses its neighboring slots.
    pub fn move_tab_within_group(
        &mut self,
        group_id: &str,
        tab_id: &str,
        target_index: usize,
    ) -> bool {
        let Some(group) = self.groups.iter_mut().find(|group| group.id == group_id) else {
            return false;
        };
        let Some(source_index) = group.tabs.iter().position(|tab| tab.id == tab_id) else {
            return false;
        };
        let tab = group.tabs.remove(source_index);
        let target_index = target_index.min(group.tabs.len());
        if source_index == target_index {
            group.tabs.insert(source_index, tab);
            return false;
        }
        group.tabs.insert(target_index, tab);
        true
    }

    /// Moves one tab between visible groups. If the source loses its last tab,
    /// that group is removed and the remaining columns reflow immediately.
    pub fn move_tab_to_group(
        &mut self,
        source_group_id: &str,
        target_group_id: &str,
        tab_id: &str,
    ) -> bool {
        if source_group_id == target_group_id
            || !self.groups.iter().any(|group| group.id == target_group_id)
        {
            return false;
        }
        let Some(source_index) = self
            .groups
            .iter()
            .position(|group| group.id == source_group_id)
        else {
            return false;
        };
        let Some(tab_index) = self.groups[source_index]
            .tabs
            .iter()
            .position(|tab| tab.id == tab_id)
        else {
            return false;
        };
        let tab = self.groups[source_index].tabs.remove(tab_index);
        if self.groups[source_index].active_tab == tab.id.as_str() {
            self.groups[source_index].active_tab = self.groups[source_index]
                .tabs
                .first()
                .map(|candidate| candidate.id.clone())
                .unwrap_or_default();
        }
        let source_empty = self.groups[source_index].tabs.is_empty();
        let target = self
            .groups
            .iter_mut()
            .find(|group| group.id == target_group_id)
            .expect("target group was checked above");
        if target.tabs.is_empty() {
            target.active_tab = tab.id.clone();
        }
        target.tabs.push(tab);
        if source_empty {
            self.groups.remove(source_index);
        }
        true
    }

    /// Resizes the boundary between two adjacent groups while preserving the
    /// total width and the minimum width contract of every group.
    pub fn resize_boundary(
        &mut self,
        left_group_id: &str,
        right_group_id: &str,
        width: f32,
        gap: f32,
        delta: f32,
    ) -> bool {
        let columns = self.resolve_columns(width, gap);
        let Some(left_index) = columns
            .iter()
            .position(|(group_id, _)| group_id == left_group_id)
        else {
            return false;
        };
        let Some(right_index) = columns
            .iter()
            .position(|(group_id, _)| group_id == right_group_id)
        else {
            return false;
        };
        if right_index != left_index + 1 {
            return false;
        }

        let left_width = columns[left_index].1.width;
        let right_width = columns[right_index].1.width;
        let left_min = self
            .groups
            .iter()
            .find(|group| group.id == left_group_id)
            .map(|group| group.min_width.max(1.0))
            .unwrap_or(1.0);
        let right_min = self
            .groups
            .iter()
            .find(|group| group.id == right_group_id)
            .map(|group| group.min_width.max(1.0))
            .unwrap_or(1.0);
        let pair_width = left_width + right_width;
        if pair_width < left_min + right_min {
            return false;
        }
        let next_left = (left_width + delta).clamp(left_min, pair_width - right_min);
        let next_right = pair_width - next_left;
        if (next_left - left_width).abs() <= f32::EPSILON {
            return false;
        }

        let mut desired = columns
            .iter()
            .map(|(group_id, rect)| (group_id.clone(), rect.width))
            .collect::<Vec<_>>();
        desired[left_index].1 = next_left;
        desired[right_index].1 = next_right;
        for (group_id, size) in desired {
            if let Some(group) = self.groups.iter_mut().find(|group| group.id == group_id) {
                group.weight = (size - group.min_width.max(1.0)).max(0.01);
            }
        }
        true
    }

    pub fn split_tab(
        &mut self,
        source_group_id: &str,
        tab_id: &str,
        new_group_id: impl Into<String>,
    ) -> bool {
        let new_group_id = new_group_id.into();
        if self.groups.iter().any(|group| group.id == new_group_id) {
            return false;
        }
        if self.groups.len() >= MAX_BOTTOM_DOCK_GROUPS {
            return false;
        }
        let Some(source_index) = self
            .groups
            .iter()
            .position(|group| group.id == source_group_id)
        else {
            return false;
        };
        let Some(tab_index) = self.groups[source_index]
            .tabs
            .iter()
            .position(|tab| tab.id == tab_id)
        else {
            return false;
        };
        let source_weight = self.groups[source_index].weight;
        let source_min_width = self.groups[source_index].min_width;
        let tab = self.groups[source_index].tabs.remove(tab_index);
        if self.groups[source_index].active_tab == tab.id.as_str() {
            self.groups[source_index].active_tab = self.groups[source_index]
                .tabs
                .first()
                .map(|candidate| candidate.id.clone())
                .unwrap_or_default();
        }
        let mut group = DockTabGroup::new(new_group_id, vec![tab]);
        group.weight = source_weight;
        group.min_width = source_min_width;
        let insert_at = if self.groups[source_index].tabs.is_empty() {
            self.groups.remove(source_index);
            source_index.min(self.groups.len())
        } else {
            source_index + 1
        };
        self.groups.insert(insert_at, group);
        true
    }

    /// Splits a tab into a new group and places that group beside a target
    /// track. Hosts use this for edge-drop gestures so the visual order
    /// follows the direction in which the user released the mouse.
    pub fn split_tab_next_to(
        &mut self,
        source_group_id: &str,
        tab_id: &str,
        new_group_id: impl Into<String>,
        target_group_id: &str,
        before: bool,
    ) -> bool {
        let new_group_id = new_group_id.into();
        if !self.split_tab(source_group_id, tab_id, new_group_id.clone()) {
            return false;
        }
        let Some(new_index) = self
            .groups
            .iter()
            .position(|group| group.id == new_group_id)
        else {
            return false;
        };
        let group = self.groups.remove(new_index);
        let Some(mut target_index) = self
            .groups
            .iter()
            .position(|candidate| candidate.id == target_group_id)
        else {
            self.groups.insert(new_index.min(self.groups.len()), group);
            return true;
        };
        if !before {
            target_index += 1;
        }
        self.groups
            .insert(target_index.min(self.groups.len()), group);
        true
    }

    pub fn merge_groups(&mut self, source_group_id: &str, target_group_id: &str) -> bool {
        if source_group_id == target_group_id {
            return false;
        }
        let Some(source_index) = self
            .groups
            .iter()
            .position(|group| group.id == source_group_id)
        else {
            return false;
        };
        if !self.groups.iter().any(|group| group.id == target_group_id) {
            return false;
        }
        let source = self.groups.remove(source_index);
        let Some(target) = self
            .groups
            .iter_mut()
            .find(|group| group.id == target_group_id)
        else {
            return false;
        };
        target.tabs.extend(source.tabs);
        if target.active_tab.is_empty() {
            target.active_tab = target
                .tabs
                .first()
                .map(|tab| tab.id.clone())
                .unwrap_or_default();
        }
        self.normalize();
        true
    }
}

/// Persistence policy for a dock. Fixed docks remain part of the workspace
/// structure; movable docks may be relocated or floated by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DockPanelPolicy {
    #[default]
    Movable,
    Fixed,
}

impl DockPanelPolicy {
    pub fn is_movable(self) -> bool {
        matches!(self, Self::Movable)
    }
}

fn default_allowed_dock_sides() -> Vec<DockSide> {
    vec![
        DockSide::Left,
        DockSide::Right,
        DockSide::Top,
        DockSide::Bottom,
    ]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DockPanel {
    pub id: String,
    pub title_key: String,
    pub side: DockSide,
    pub min_size: [f32; 2],
    pub preferred_size: [f32; 2],
    pub visible: bool,
    #[serde(default)]
    pub policy: DockPanelPolicy,
    #[serde(default = "default_allowed_dock_sides")]
    pub allowed_dock_sides: Vec<DockSide>,
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
            policy: DockPanelPolicy::Movable,
            allowed_dock_sides: default_allowed_dock_sides(),
        }
    }

    pub fn with_policy(mut self, policy: DockPanelPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn with_allowed_dock_sides(mut self, sides: impl IntoIterator<Item = DockSide>) -> Self {
        self.allowed_dock_sides = sides.into_iter().collect();
        self
    }

    pub fn accepts_side(&self, side: DockSide) -> bool {
        self.allowed_dock_sides.contains(&side)
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
    #[serde(default)]
    pub policy: DockPanelPolicy,
    #[serde(default = "default_allowed_dock_sides")]
    pub allowed_dock_sides: Vec<DockSide>,
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
            policy: DockPanelPolicy::Movable,
            allowed_dock_sides: default_allowed_dock_sides(),
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
        if !self.panels[index].policy.is_movable() {
            return false;
        }
        let panel = self.panels.remove(index);
        let mut floating = FloatingPanel::new(panel.id, panel.title_key, rect.clamp_inside(bounds));
        floating.min_size = panel.min_size;
        floating.policy = panel.policy;
        floating.allowed_dock_sides = panel.allowed_dock_sides;
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
        if !self.floating[index].policy.is_movable()
            || !self.floating[index].allowed_dock_sides.contains(&side)
        {
            return false;
        }
        let floating = self.floating.remove(index);
        self.panels.push(DockPanel {
            id: floating.id,
            title_key: floating.title_key,
            side,
            min_size: floating.min_size,
            preferred_size: [floating.rect.width, floating.rect.height],
            visible: floating.visible,
            policy: floating.policy,
            allowed_dock_sides: floating.allowed_dock_sides,
        });
        true
    }

    /// Moves a docked panel between permitted tracks without changing its
    /// identity or stored dimensions. Fixed infrastructure such as the bottom
    /// work dock intentionally rejects this operation.
    pub fn move_panel_to(&mut self, id: &str, side: DockSide) -> bool {
        if side == DockSide::Center {
            return false;
        }
        let Some(panel) = self.panels.iter_mut().find(|panel| panel.id == id) else {
            return false;
        };
        if !panel.policy.is_movable() || !panel.accepts_side(side) {
            return false;
        }
        let changed = panel.side != side;
        panel.side = side;
        changed
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

    fn bottom_group(id: &str, tab_id: &str) -> DockTabGroup {
        DockTabGroup::new(id, vec![DockTab::new(tab_id, "app.tab", UiIconId::Console)])
    }

    #[test]
    fn bottom_dock_columns_fill_width_without_overlap() {
        let mut layout = BottomDockLayout::new(vec![
            bottom_group("left", "output"),
            bottom_group("middle", "assets"),
            bottom_group("right", "project"),
        ]);
        layout.groups[0].min_width = 140.0;
        layout.groups[1].min_width = 180.0;
        layout.groups[2].min_width = 120.0;

        let columns = layout.resolve_columns(900.0, 4.0);
        assert_eq!(columns.len(), 3);
        assert_eq!(columns[0].1.x, 0.0);
        assert!((columns[2].1.right() - 900.0).abs() < 0.01);
        assert!(columns[0].1.right() <= columns[1].1.x);
        assert!(columns[1].1.right() <= columns[2].1.x);
    }

    #[test]
    fn bottom_dock_collapsed_state_restores_expanded_height() {
        let mut layout = BottomDockLayout::new(vec![bottom_group("console", "output")]);
        layout.set_height(320.0, 112.0, 420.0);
        layout.toggle_collapsed(112.0, 420.0);
        assert!(layout.collapsed);
        assert_eq!(layout.effective_height(32.0), 32.0);
        layout.toggle_collapsed(112.0, 420.0);
        assert!(!layout.collapsed);
        assert_eq!(layout.height, 320.0);
    }

    #[test]
    fn bottom_dock_can_split_and_merge_tabs() {
        let mut layout = BottomDockLayout::new(vec![DockTabGroup::new(
            "utility",
            vec![
                DockTab::new("output", "app.output", UiIconId::Console),
                DockTab::new("console", "app.studio_console", UiIconId::Console),
            ],
        )]);

        assert!(layout.split_tab("utility", "console", "console-only"));
        assert_eq!(layout.groups.len(), 2);
        assert!(layout.merge_groups("console-only", "utility"));
        assert_eq!(layout.groups.len(), 1);
        assert_eq!(layout.groups[0].tabs.len(), 2);
    }

    #[test]
    fn bottom_dock_moves_tabs_between_groups() {
        let mut layout = BottomDockLayout::new(vec![
            bottom_group("left", "console"),
            bottom_group("right", "assets"),
        ]);

        assert!(layout.move_tab_to_group("left", "right", "console"));
        assert_eq!(layout.groups.len(), 1);
        assert_eq!(layout.groups[0].id, "right");
        assert_eq!(layout.groups[0].tabs.len(), 2);
        assert_eq!(layout.groups[0].tabs[1].id, "console");
    }

    #[test]
    fn bottom_dock_reorders_tabs_within_a_group() {
        let mut layout = BottomDockLayout::new(vec![DockTabGroup::new(
            "main",
            vec![
                DockTab::new("console", "app.studio_console", UiIconId::Console),
                DockTab::new("assets", "app.studio_assets", UiIconId::Assets),
                DockTab::new("project", "app.studio_project", UiIconId::Project),
            ],
        )]);

        assert!(layout.move_tab_within_group("main", "console", 2));
        assert_eq!(
            layout.groups[0]
                .tabs
                .iter()
                .map(|tab| tab.id.as_str())
                .collect::<Vec<_>>(),
            vec!["assets", "project", "console"]
        );
    }

    #[test]
    fn bottom_dock_normalization_removes_empty_groups_and_caps_group_count() {
        let mut layout = BottomDockLayout::new(vec![
            bottom_group("one", "console"),
            DockTabGroup::new("empty", Vec::new()),
            bottom_group("two", "assets"),
            bottom_group("three", "project"),
            bottom_group("four", "settings"),
        ]);

        assert!(layout.normalize());
        assert_eq!(layout.groups.len(), MAX_BOTTOM_DOCK_GROUPS);
        assert!(!layout.groups.iter().any(|group| group.tabs.is_empty()));
        assert_eq!(layout.version, BOTTOM_DOCK_LAYOUT_VERSION);
    }

    #[test]
    fn bottom_dock_split_removes_source_when_it_loses_its_only_tab() {
        let mut layout = BottomDockLayout::new(vec![bottom_group("main", "console")]);

        assert!(layout.split_tab("main", "console", "console-only"));
        assert_eq!(layout.groups.len(), 1);
        assert_eq!(layout.groups[0].id, "console-only");
    }

    #[test]
    fn bottom_dock_resizes_adjacent_group_boundary() {
        let mut layout = BottomDockLayout::new(vec![
            bottom_group("left", "console"),
            bottom_group("middle", "assets"),
            bottom_group("right", "project"),
        ]);
        let before = layout.resolve_columns(1200.0, 4.0);
        let before_left = before[0].1.width;
        let before_middle = before[1].1.width;

        assert!(layout.resize_boundary("left", "middle", 1200.0, 4.0, 80.0));

        let after = layout.resolve_columns(1200.0, 4.0);
        assert!(after[0].1.width > before_left);
        assert!(after[1].1.width < before_middle);
        assert!((after[2].1.right() - 1200.0).abs() < 0.01);
    }

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
    fn fixed_panels_reject_undock_and_side_changes() {
        let workspace = UiRect::new(0.0, 0.0, 640.0, 480.0);
        let mut panel = DockPanel::new("bottom", "panel.bottom", DockSide::Bottom)
            .with_policy(DockPanelPolicy::Fixed)
            .with_allowed_dock_sides([DockSide::Bottom]);
        panel.preferred_size = [420.0, 180.0];
        let mut layout = DockLayout {
            panels: vec![panel],
            floating: Vec::new(),
        };

        assert!(!layout.undock_panel("bottom", UiRect::new(80.0, 80.0, 240.0, 160.0), workspace,));
        assert!(!layout.move_panel_to("bottom", DockSide::Left));
        assert_eq!(layout.panels[0].side, DockSide::Bottom);
    }

    #[test]
    fn movable_panel_retains_allowed_sides_after_float_round_trip() {
        let workspace = UiRect::new(0.0, 0.0, 640.0, 480.0);
        let panel = DockPanel::new("hierarchy", "panel.hierarchy", DockSide::Left)
            .with_allowed_dock_sides([DockSide::Left, DockSide::Right]);
        let mut layout = DockLayout {
            panels: vec![panel],
            floating: Vec::new(),
        };

        assert!(layout.undock_panel(
            "hierarchy",
            UiRect::new(120.0, 60.0, 220.0, 320.0),
            workspace,
        ));
        assert!(!layout.dock_floating("hierarchy", DockSide::Top));
        assert!(layout.dock_floating("hierarchy", DockSide::Right));
        assert_eq!(layout.panels[0].side, DockSide::Right);
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
