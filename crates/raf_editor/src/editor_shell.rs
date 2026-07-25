//! Persisted structural layout for the shared Game and Electronics workbench.
//!
//! This module deliberately stores only dock geometry and policy. It has no
//! references to scene, CAD, renderer, or panel implementation state, so the
//! same workspace contract can outlive the temporary eframe panel adapters.

use raf_ui::{DockLayout, DockPanel, DockPanelPolicy, DockSide, UiRect};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const EDITOR_SHELL_LAYOUT_FILE: &str = "editor_shell.ron";
const EDITOR_SHELL_LAYOUT_VERSION: u32 = 3;

pub const PANEL_HIERARCHY: &str = "hierarchy";
pub const PANEL_PROPERTIES: &str = "properties";
pub const PANEL_SESSIONS: &str = "sessions";
pub const PANEL_BOTTOM: &str = "bottom";
pub const PANEL_CENTER: &str = "center";

/// Serializable workbench state shared by Game and Electronics projects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorShellLayout {
    #[serde(default = "default_layout_version")]
    pub version: u32,
    #[serde(default = "default_dock_layout")]
    pub docks: DockLayout,
}

impl Default for EditorShellLayout {
    fn default() -> Self {
        Self {
            version: EDITOR_SHELL_LAYOUT_VERSION,
            docks: default_dock_layout(),
        }
    }
}

impl EditorShellLayout {
    pub fn load(project_root: &Path) -> Self {
        let path = project_root.join(EDITOR_SHELL_LAYOUT_FILE);
        let Ok(contents) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(mut layout) = ron::from_str::<Self>(&contents) else {
            return Self::default();
        };
        layout.repair();
        layout
    }

    pub fn save(&self, project_root: &Path) -> Result<(), String> {
        let path = project_root.join(EDITOR_SHELL_LAYOUT_FILE);
        let pretty = ron::ser::PrettyConfig::default();
        let contents = ron::ser::to_string_pretty(self, pretty)
            .map_err(|error| format!("could not serialize editor shell: {error}"))?;
        std::fs::write(path, contents)
            .map_err(|error| format!("could not save editor shell layout: {error}"))
    }

    /// Keeps old projects and incomplete saved layouts usable. The structural
    /// panels always exist, while user-chosen dimensions and supported dock
    /// locations remain intact.
    pub fn repair(&mut self) {
        let previous_version = self.version;
        self.version = EDITOR_SHELL_LAYOUT_VERSION;
        ensure_panel(
            &mut self.docks,
            structural_panel(PANEL_HIERARCHY, "app.hierarchy", DockSide::Left),
        );
        ensure_panel(
            &mut self.docks,
            structural_panel(PANEL_PROPERTIES, "app.properties", DockSide::Right),
        );
        ensure_panel(
            &mut self.docks,
            structural_panel(PANEL_SESSIONS, "app.sessions", DockSide::Right),
        );
        ensure_panel(&mut self.docks, fixed_bottom_panel());
        ensure_panel(&mut self.docks, fixed_center_panel());

        normalize_fixed_panel(&mut self.docks, PANEL_BOTTOM, DockSide::Bottom);
        normalize_fixed_panel(&mut self.docks, PANEL_CENTER, DockSide::Center);

        // Versions 1 and 2 let the temporary Egui adapter persist oversized
        // work docks. Normalize that accidental geometry once, while keeping
        // every resize the user makes with the corrected shell afterwards.
        if previous_version < EDITOR_SHELL_LAYOUT_VERSION {
            if let Some(bottom) = self
                .docks
                .panels
                .iter_mut()
                .find(|panel| panel.id == PANEL_BOTTOM)
            {
                bottom.preferred_size[1] = bottom.preferred_size[1].clamp(90.0, 150.0);
            }
            if let Some(hierarchy) = self
                .docks
                .panels
                .iter_mut()
                .find(|panel| panel.id == PANEL_HIERARCHY)
            {
                hierarchy.preferred_size[0] = hierarchy.preferred_size[0].clamp(248.0, 292.0);
            }
            if let Some(properties) = self
                .docks
                .panels
                .iter_mut()
                .find(|panel| panel.id == PANEL_PROPERTIES)
            {
                properties.preferred_size[0] = properties.preferred_size[0].clamp(260.0, 320.0);
            }
        }

        for panel in &mut self.docks.panels {
            if panel.allowed_dock_sides.is_empty() {
                panel.allowed_dock_sides = allowed_sides_for(&panel.id);
            }
        }
        for panel in &mut self.docks.floating {
            if panel.allowed_dock_sides.is_empty() {
                panel.allowed_dock_sides = allowed_sides_for(&panel.id);
            }
        }
    }

    /// Synchronizes legacy project visibility preferences during the adapter
    /// phase. The same visibility values later feed the native RafUI shell.
    pub fn sync_legacy_visibility(&mut self, hierarchy_visible: bool, properties_visible: bool) {
        self.docks
            .set_panel_visible(PANEL_HIERARCHY, hierarchy_visible);
        self.docks
            .set_panel_visible(PANEL_PROPERTIES, properties_visible);
    }

    pub fn preferred_width(&self, id: &str, fallback: f32) -> f32 {
        self.docks
            .panels
            .iter()
            .find(|panel| panel.id == id)
            .map(|panel| panel.preferred_size[0].max(panel.min_size[0]))
            .unwrap_or(fallback)
    }

    pub fn preferred_height(&self, id: &str, fallback: f32) -> f32 {
        self.docks
            .panels
            .iter()
            .find(|panel| panel.id == id)
            .map(|panel| panel.preferred_size[1].max(panel.min_size[1]))
            .unwrap_or(fallback)
    }

    /// Records dimensions returned by a temporary host. The retained shell
    /// remains the source of truth even while individual panel bodies are
    /// still rendered through the transitional adapter.
    pub fn observe_host_rect(&mut self, id: &str, rect: UiRect, workspace: UiRect) -> bool {
        let dimension = self
            .docks
            .panels
            .iter()
            .find(|panel| panel.id == id)
            .map(|panel| match panel.side {
                DockSide::Left | DockSide::Right => rect.width,
                DockSide::Top | DockSide::Bottom => rect.height,
                DockSide::Center => 0.0,
            })
            .unwrap_or(0.0);
        self.docks.resize_docked_to(id, dimension, workspace)
    }
}

fn default_layout_version() -> u32 {
    EDITOR_SHELL_LAYOUT_VERSION
}

fn default_dock_layout() -> DockLayout {
    DockLayout {
        panels: vec![
            structural_panel(PANEL_HIERARCHY, "app.hierarchy", DockSide::Left),
            structural_panel(PANEL_PROPERTIES, "app.properties", DockSide::Right),
            structural_panel(PANEL_SESSIONS, "app.sessions", DockSide::Right),
            fixed_bottom_panel(),
            fixed_center_panel(),
        ],
        floating: Vec::new(),
    }
}

fn structural_panel(id: &str, title_key: &str, side: DockSide) -> DockPanel {
    let mut panel = DockPanel::new(id, title_key, side)
        .with_allowed_dock_sides([DockSide::Left, DockSide::Right]);
    panel.min_size = [180.0, 160.0];
    panel.preferred_size = match id {
        PANEL_HIERARCHY => [220.0, 480.0],
        PANEL_PROPERTIES => [300.0, 420.0],
        PANEL_SESSIONS => [280.0, 260.0],
        _ => panel.preferred_size,
    };
    panel
}

fn fixed_bottom_panel() -> DockPanel {
    let mut panel = DockPanel::new(PANEL_BOTTOM, "app.bottom_panel", DockSide::Bottom)
        .with_policy(DockPanelPolicy::Fixed)
        .with_allowed_dock_sides([DockSide::Bottom]);
    panel.min_size = [360.0, 90.0];
    panel.preferred_size = [720.0, 144.0];
    panel
}

fn fixed_center_panel() -> DockPanel {
    let mut panel = DockPanel::new(PANEL_CENTER, "app.center_surface", DockSide::Center)
        .with_policy(DockPanelPolicy::Fixed)
        .with_allowed_dock_sides([]);
    panel.min_size = [320.0, 240.0];
    panel.preferred_size = [960.0, 640.0];
    panel
}

fn allowed_sides_for(id: &str) -> Vec<DockSide> {
    match id {
        PANEL_HIERARCHY | PANEL_PROPERTIES | PANEL_SESSIONS => {
            vec![DockSide::Left, DockSide::Right]
        }
        PANEL_BOTTOM => vec![DockSide::Bottom],
        _ => Vec::new(),
    }
}

fn ensure_panel(layout: &mut DockLayout, panel: DockPanel) {
    if layout.panels.iter().any(|existing| existing.id == panel.id)
        || layout
            .floating
            .iter()
            .any(|existing| existing.id == panel.id)
    {
        return;
    }
    layout.panels.push(panel);
}

fn normalize_fixed_panel(layout: &mut DockLayout, id: &str, side: DockSide) {
    if let Some(index) = layout.floating.iter().position(|panel| panel.id == id) {
        let floating = layout.floating.remove(index);
        layout.panels.retain(|panel| panel.id != id);
        let mut panel = if id == PANEL_BOTTOM {
            fixed_bottom_panel()
        } else {
            fixed_center_panel()
        };
        panel.visible = floating.visible;
        panel.preferred_size = [floating.rect.width, floating.rect.height];
        layout.panels.push(panel);
        return;
    }

    if let Some(panel) = layout.panels.iter_mut().find(|panel| panel.id == id) {
        panel.side = side;
        panel.policy = DockPanelPolicy::Fixed;
        panel.allowed_dock_sides = allowed_sides_for(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_shell_has_one_center_and_fixed_bottom_work_dock() {
        let layout = EditorShellLayout::default();

        assert!(layout
            .docks
            .panels
            .iter()
            .any(|panel| panel.id == PANEL_CENTER && panel.side == DockSide::Center));
        assert!(layout
            .docks
            .panels
            .iter()
            .any(|panel| panel.id == PANEL_BOTTOM && panel.policy == DockPanelPolicy::Fixed));
    }

    #[test]
    fn only_supporting_docks_can_change_side_or_float() {
        let workspace = UiRect::new(0.0, 0.0, 1280.0, 800.0);
        let mut layout = EditorShellLayout::default();

        assert!(layout.docks.move_panel_to(PANEL_HIERARCHY, DockSide::Right));
        assert!(!layout.docks.move_panel_to(PANEL_BOTTOM, DockSide::Left));
        assert!(layout.docks.undock_panel(
            PANEL_PROPERTIES,
            UiRect::new(700.0, 80.0, 260.0, 380.0),
            workspace,
        ));
        assert!(!layout.docks.undock_panel(
            PANEL_BOTTOM,
            UiRect::new(100.0, 400.0, 300.0, 180.0),
            workspace,
        ));
    }

    #[test]
    fn repair_keeps_existing_dimensions_and_adds_missing_structural_docks() {
        let mut layout = EditorShellLayout {
            version: EDITOR_SHELL_LAYOUT_VERSION,
            docks: DockLayout {
                panels: vec![{
                    let mut panel =
                        DockPanel::new(PANEL_HIERARCHY, "app.hierarchy", DockSide::Left);
                    panel.preferred_size = [320.0, 500.0];
                    panel
                }],
                floating: Vec::new(),
            },
        };

        layout.repair();

        assert_eq!(layout.version, EDITOR_SHELL_LAYOUT_VERSION);
        assert_eq!(layout.preferred_width(PANEL_HIERARCHY, 0.0), 320.0);
        assert!(layout
            .docks
            .panels
            .iter()
            .any(|panel| panel.id == PANEL_SESSIONS));
    }

    #[test]
    fn repair_returns_fixed_infrastructure_from_a_floating_layout() {
        let mut layout = EditorShellLayout::default();
        assert!(layout.docks.undock_panel(
            PANEL_PROPERTIES,
            UiRect::new(680.0, 80.0, 280.0, 360.0),
            UiRect::new(0.0, 0.0, 1280.0, 800.0),
        ));

        let bottom_index = layout
            .docks
            .panels
            .iter()
            .position(|panel| panel.id == PANEL_BOTTOM)
            .expect("bottom panel");
        let bottom = layout.docks.panels.remove(bottom_index);
        layout.docks.floating.push(raf_ui::FloatingPanel {
            id: bottom.id,
            title_key: bottom.title_key,
            rect: UiRect::new(480.0, 240.0, 360.0, 180.0),
            min_size: bottom.min_size,
            visible: true,
            z_index: 2,
            policy: DockPanelPolicy::Movable,
            allowed_dock_sides: vec![DockSide::Left],
        });

        layout.repair();

        let bottom = layout
            .docks
            .panels
            .iter()
            .find(|panel| panel.id == PANEL_BOTTOM)
            .expect("repaired bottom");
        assert_eq!(bottom.side, DockSide::Bottom);
        assert_eq!(bottom.policy, DockPanelPolicy::Fixed);
        assert_eq!(bottom.allowed_dock_sides, vec![DockSide::Bottom]);
        assert!(layout
            .docks
            .floating
            .iter()
            .all(|panel| panel.id != PANEL_BOTTOM));
    }

    #[test]
    fn repair_compacts_legacy_electronics_docks_once() {
        let mut layout = EditorShellLayout::default();
        layout.version = 2;
        layout
            .docks
            .panels
            .iter_mut()
            .find(|panel| panel.id == PANEL_BOTTOM)
            .expect("bottom panel")
            .preferred_size[1] = 420.0;
        layout
            .docks
            .panels
            .iter_mut()
            .find(|panel| panel.id == PANEL_HIERARCHY)
            .expect("hierarchy panel")
            .preferred_size[0] = 372.0;
        layout
            .docks
            .panels
            .iter_mut()
            .find(|panel| panel.id == PANEL_PROPERTIES)
            .expect("properties panel")
            .preferred_size[0] = 384.0;

        layout.repair();

        assert_eq!(layout.version, EDITOR_SHELL_LAYOUT_VERSION);
        assert_eq!(layout.preferred_height(PANEL_BOTTOM, 0.0), 150.0);
        assert_eq!(layout.preferred_width(PANEL_HIERARCHY, 0.0), 292.0);
        assert_eq!(layout.preferred_width(PANEL_PROPERTIES, 0.0), 320.0);
    }
}
