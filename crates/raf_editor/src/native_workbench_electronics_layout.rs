//! Session-local CAD presentation state; document geometry stays in the editor.
use super::*;
use raf_electronics::CadSurfaceKind;

pub(super) struct ElectronicsWorkspaceState {
    project: Option<(uuid::Uuid, PathBuf)>,
    pub(super) schematic_collapsed: bool,
    pub(super) pcb_open: bool,
    pub(super) pcb_active: bool,
    pub(super) collapsed_categories: std::collections::HashSet<String>,
    pub(super) inspector_page: String,
    pub(super) pcb_menu: Option<String>,
    inspector_pinned: bool,
    pub(super) inspector_hidden: bool,
    inspector_visible: bool,
    project_preferences: Option<(bool, bool)>,
}

impl Default for ElectronicsWorkspaceState {
    fn default() -> Self {
        Self {
            project: None,
            schematic_collapsed: false,
            pcb_open: true,
            pcb_active: false,
            collapsed_categories: Default::default(),
            inspector_page: "properties".into(),
            pcb_menu: None,
            inspector_pinned: true,
            inspector_hidden: false,
            inspector_visible: true,
            project_preferences: None,
        }
    }
}

impl NativeGameWorkbench {
    pub fn electronics_workspace_policy(
        &mut self,
        project: Option<&Project>,
        editor: Option<&NativeElectronicsEditor>,
    ) -> (bool, bool) {
        let scope = project.map(|p| (p.id, p.path.clone()));
        if self.electronics_workspace.project != scope {
            self.inspector_footprint_signature.clear();
            self.inspector_footprint_presentation = None;
            self.electronics_workspace = electronics_layout::ElectronicsWorkspaceState {
                project: scope,
                ..Default::default()
            };
        }
        let state = &mut self.electronics_workspace;
        let preferences = project.map_or((true, true), |p| {
            (
                p.settings.show_hierarchy_panel,
                p.settings.show_properties_panel,
            )
        });
        if state.project_preferences != Some(preferences) {
            state.schematic_collapsed = !preferences.0;
            if !preferences.0 {
                state.pcb_open = false;
            }
            state.inspector_hidden = !preferences.1;
            state.project_preferences = Some(preferences);
        }
        state.pcb_active = editor.is_some_and(|e| e.active_surface() == CadSurfaceKind::Pcb);
        if !state.pcb_active && state.pcb_menu.as_deref() == Some("rules") {
            state.pcb_menu = None;
        }
        let library = if state.pcb_active {
            state.pcb_open
        } else {
            !state.schematic_collapsed
        };
        let inspector = !state.inspector_hidden
            && (state.inspector_pinned || editor.is_some_and(|e| e.selection().is_some()));
        state.inspector_visible = inspector;
        (library, inspector)
    }

    pub(super) fn handle_electronics_workspace_command(&mut self, command: &str) -> bool {
        let library_visible = self
            .last_layout
            .and_then(|layout| layout.left_panel)
            .is_some_and(|rect| rect.width > crate::editor_layout::ELECTRONICS_RAIL_WIDTH + 0.5);
        let narrow = self
            .last_layout
            .is_some_and(|layout| layout.window.width < 980.0);
        let state = &mut self.electronics_workspace;
        if let Some(key) = command.strip_prefix("electronics.library.category.toggle:") {
            if !state.collapsed_categories.remove(key) {
                state.collapsed_categories.insert(key.to_owned());
            }
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(key) = command.strip_prefix("electronics.inspector.tab:") {
            if matches!(key, "properties" | "appearance" | "nets") {
                state.inspector_page = key.into();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
        }
        if let Some(key) = command.strip_prefix("electronics.pcb.menu:") {
            if key == "rules" {
                state.inspector_hidden = false;
                state.inspector_pinned = true;
            }
            state.pcb_menu = if state.pcb_menu.as_deref() == Some(key) {
                None
            } else {
                Some(key.into())
            };
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if command == "electronics.pcb.menu.close" {
            state.pcb_menu = None;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if [
            "electronics.pcb.layer:",
            "electronics.pcb.width:",
            "electronics.pcb.grid:",
            "electronics.pcb.zoom:",
        ]
        .iter()
        .any(|p| command.starts_with(p))
        {
            state.pcb_menu = None;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        match command {
            "electronics.workspace.library.toggle" => {
                if state.pcb_active {
                    state.pcb_open = !library_visible;
                } else {
                    state.schematic_collapsed = library_visible;
                }
                if narrow && !library_visible {
                    state.inspector_hidden = true;
                }
            }
            "electronics.workspace.inspector.pin" => {
                state.inspector_pinned = !state.inspector_pinned || state.inspector_hidden;
                state.inspector_hidden = false;
            }
            "electronics.workspace.inspector.toggle" => {
                state.inspector_hidden = state.inspector_visible;
                if !state.inspector_hidden {
                    state.inspector_pinned = true;
                }
            }
            _ => return false,
        }
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        true
    }

    pub(super) fn build_electronics_rail(&self) -> UiSurface {
        build_electronics_rail(self.palette, self.electronics_workspace.pcb_active)
    }
}

pub(crate) fn build_electronics_rail(palette: StudioUiPalette, pcb_active: bool) -> UiSurface {
    use crate::panels::electronics_surface::ElectronicsIconButton;
    let mut root = UiNode::new("electronics.rail", UiNodeKind::Panel)
        .with_layout(UiLayout {
            padding: raf_ui::UiSpacing::xy(7.0, 10.0),
            gap: 8.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(UiStyle::transparent().with_fill(palette.tokens().surface));
    for (id, icon, command, label) in [
        (
            "library",
            UiIconId::Assets,
            "electronics.navigator.tab:library",
            "app.electronics_open_library",
        ),
        (
            "project",
            UiIconId::Schematic,
            "electronics.navigator.tab:project",
            "app.electronics_project",
        ),
        (
            "inspector",
            UiIconId::Settings,
            "electronics.workspace.inspector.pin",
            "electronics.workspace.inspector_pin",
        ),
    ] {
        root = root.with_child(
            ElectronicsIconButton::new(
                &format!("electronics.rail.{id}"),
                icon,
                command,
                label,
                "electronics-rail-button",
                36.0,
                36.0,
            )
            .build(),
        );
    }
    if pcb_active {
        root = root.with_child(
            ElectronicsIconButton::new(
                "electronics.rail.layers",
                UiIconId::Grid,
                "electronics.navigator.tab:layers",
                "electronics.rail.layers",
                "electronics-rail-button",
                36.0,
                36.0,
            )
            .build(),
        );
    }
    let mut surface = UiSurface::new("electronics.rail", palette, root);
    use crate::panels::electronics_surface::{
        electronics_flat_class_rule, electronics_focus_rule, electronics_hover_rule,
    };
    let tokens = palette.tokens();
    surface.style_sheet = UiStyleSheet {
        rules: vec![
            electronics_flat_class_rule("electronics-rail-button", [0, 0, 0, 0], tokens.text_muted),
            electronics_hover_rule("electronics-rail-button", tokens),
            electronics_focus_rule("electronics-rail-button", tokens),
        ],
    };
    surface
}

/// One responsive application-bar policy shared by native composition and QA.
pub(crate) fn compact_application_bar(mut surface: UiSurface, width: f32) -> UiSurface {
    use raf_ui::{UiIcon, UiIconSize};
    let tokens = surface.palette.tokens();
    let children = &mut surface.root.children;
    if !children
        .iter()
        .any(|node| node.id == "application-bar.agents")
    {
        let index = children
            .iter()
            .position(|node| node.id == "application-bar.drag-region")
            .unwrap_or(children.len());
        children.insert(
            index,
            crate::agent_workspace_surface::button(
                "application-bar.agents",
                "lab.open",
                "agent.lab.open.simulation",
                &tokens,
            ),
        );
    }
    if let Some(index) = children.iter().position(|node| node.id == "application-bar.agents") {
        children[index] = crate::agent_workspace_surface::button(
            "application-bar.agents", "lab.open", "agent.lab.open.simulation", &tokens,
        );
        if width >= 1280.0 {
            children.insert(index + 1, crate::agent_workspace_surface::button(
                "application-bar.lab.device", "lab.open_device", "agent.lab.open.device", &tokens,
            ));
        }
    }
    children.retain(|node| {
        node.id != "application-bar.context"
            && (width >= 1500.0 || !node.id.starts_with("application-bar.saved"))
            && (width >= 1400.0 || !node.id.starts_with("application-bar.command-search"))
            && (width >= 1050.0 || node.id != "application-bar.product")
    });
    for node in children {
        if node.id == "application-bar.project" && width < 1200.0 {
            node.layout.basis[0] = 108.0;
        }
        if node.id.starts_with("application-bar.menu.") {
            node.tooltip_key = node.accessibility_label_key.clone();
            let compact = width < 1050.0;
            if compact {
                node.layout.basis[0] = 32.0;
            } else if node.id.ends_with("file") {
                node.layout.basis[0] = 68.0;
            }
            if let Some(content) = node.children.first_mut() {
                for child in &mut content.children {
                    if child.id.ends_with(".label") {
                        child.text_style = Some(raf_ui::UiTextStyle {
                            size_px: 13.0,
                            line_height_px: 18.0,
                            ..raf_ui::UiTextStyle::body(tokens.text)
                        });
                    }
                }
                content.children.retain(|child| {
                    if compact {
                        child.id.ends_with(".icon")
                    } else {
                        child.id.ends_with(".label")
                    }
                });
            }
        }
        if node.id == "application-bar.electronics-drc" {
            node.layout.basis[0] = 108.0;
        }
        if node.id == "application-bar.electronics-simulate" {
            node.layout.basis[0] = 92.0;
        }
        if width < 1050.0 {
            let icon = match node.id.as_str() {
                "application-bar.agents" => Some(UiIconId::Agent),
                "application-bar.settings" => Some(UiIconId::Settings),
                "application-bar.electronics-drc" => Some(UiIconId::Warning),
                "application-bar.electronics-simulate" => Some(UiIconId::Play),
                _ => None,
            };
            if let Some(icon) = icon {
                node.layout = UiLayout::fixed(36.0, 28.0);
                node.text_key = None;
                node.text_value = None;
                node.children.clear();
                node.icon = Some(UiIcon::new(icon).with_size(UiIconSize::Small));
                node.tooltip_key = node.accessibility_label_key.clone();
            }
        }
    }
    surface
}

pub(crate) fn menu_anchor(surface: &UiSurface, menu: &str) -> Option<f32> {
    // Menus precede the flexible drag/search tracks; their positions therefore
    // come directly from the same fixed layout descriptors used to paint them.
    let mut x = surface.root.layout.padding.left;
    for node in &surface.root.children {
        if node.id == format!("application-bar.menu.{menu}") {
            return Some(x);
        }
        x += node.layout.basis[0] + surface.root.layout.gap;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn electronics_application_commands_stay_inside_the_minimum_window() {
        use raf_render::api_graphic_basic::ui_surface::CpuUiSurfaceHost;
        for width in [900, 1280, 1600] {
            let surface = compact_application_bar(
                crate::application_bar_surface::build_application_bar_surface(
                    StudioUiPalette::IndustrialDark,
                    "RC Sensor",
                    ProjectType::Electronics,
                    None,
                ),
                width as f32,
            );
            let anchor = menu_anchor(&surface, "edit").unwrap();
            let mut host = CpuUiSurfaceHost::new(surface, [0, 0, 0, 0]);
            register_bar_images(host.images_mut());
            let capture = host.render(
                [
                    width,
                    crate::application_bar_surface::APPLICATION_BAR_HEIGHT as u32,
                ],
                |key| raf_core::i18n::t(key, Language::Spanish),
            );
            for id in [
                "application-bar.menu.file",
                "application-bar.menu.edit",
                "application-bar.agents",
                "application-bar.electronics-drc",
                "application-bar.electronics-simulate",
                "application-bar.settings",
                "application-bar.close",
            ] {
                let rect = capture
                    .frame
                    .layout_boxes
                    .iter()
                    .find(|node| node.id == id)
                    .unwrap()
                    .rect;
                assert!(
                    rect.width >= 28.0 && rect.right() <= width as f32 + 0.1,
                    "{width}: {id} {rect:?}"
                );
                if id == "application-bar.menu.edit" {
                    assert!((rect.x - anchor).abs() < 0.1);
                }
            }
        }
    }
}
