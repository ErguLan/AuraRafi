//! Retained RafUI document for the shared editor workbench.
//!
//! The document owns visual structure and typed shell commands only. Game and
//! Electronics supply different center surfaces and domain panel adapters, but
//! use the same docks, bottom work area, theme tokens, and focus contract.

use raf_core::project::ProjectType;
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    DockLayout, DockSide, StudioUiPalette, UiAction, UiAlign, UiCompactMode, UiEventBinding,
    UiEventKind, UiFlow, UiImage, UiImageFit, UiImageSource, UiLayout, UiNode, UiNodeKind,
    UiSpacing, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet,
    UiTextStyle,
};

use crate::editor_shell::{
    EditorShellLayout, PANEL_BOTTOM, PANEL_CENTER, PANEL_HIERARCHY, PANEL_PROPERTIES,
    PANEL_SESSIONS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorCenterSurface {
    Scene,
    Schematic,
    Pcb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorBottomDockTab {
    Console,
    Drc,
    Simulation,
    Assets,
    ProjectSettings,
    NodeEditor,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorInspectorTab {
    Properties,
    Sessions,
}

/// The stable visual inputs for a shared editor frame. Content is deliberately
/// absent: panel bodies remain independent surfaces that can move from the
/// transitional adapter to direct RafUI presentation independently.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorShellSurfaceModel {
    pub project_type: ProjectType,
    pub center_surface: EditorCenterSurface,
    pub bottom_tab: EditorBottomDockTab,
    pub inspector_tab: EditorInspectorTab,
    pub layout: DockLayout,
}

impl Default for EditorShellSurfaceModel {
    fn default() -> Self {
        Self {
            project_type: ProjectType::Game,
            center_surface: EditorCenterSurface::Scene,
            bottom_tab: EditorBottomDockTab::Console,
            inspector_tab: EditorInspectorTab::Properties,
            layout: EditorShellLayout::default().docks,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorShellIntent {
    SelectCenter(EditorCenterSurface),
    SelectBottom(EditorBottomDockTab),
    SelectInspector(EditorInspectorTab),
    TogglePanel(String),
    OpenSettings,
}

pub fn build_editor_shell_surface(
    palette: StudioUiPalette,
    model: &EditorShellSurfaceModel,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut root = UiNode::new("editor-shell.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.root_style())
        .with_child(global_toolbar(palette))
        .with_child(context_toolbar(palette, model))
        .with_child(workspace(palette, model))
        .with_child(bottom_dock(palette, model));

    for floating in model.layout.floating.iter().filter(|panel| panel.visible) {
        root = root.with_child(
            UiNode::new(
                format!("editor-shell.floating.{}", floating.id),
                UiNodeKind::FloatingPanel,
            )
            .with_layout(UiLayout::absolute(floating.rect).with_z_index(40 + floating.z_index))
            .with_style(palette.panel_style())
            .with_child(
                UiNode::new(
                    format!("editor-shell.floating.{}.title", floating.id),
                    UiNodeKind::Label,
                )
                .with_text_key(floating.title_key.clone())
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 28.0)),
            ),
        );
    }

    let mut surface = UiSurface::new("editor-shell", palette, root);
    surface.style_sheet = shell_style_sheet(palette);
    surface
}

/// Builds only the fixed bottom tab strip. The transitional editor uses this
/// as its first live shared-shell adapter while the existing tab bodies remain
/// attached to their current domain panels.
pub fn build_editor_bottom_tabs_surface(
    palette: StudioUiPalette,
    active: EditorBottomDockTab,
    electronics_tools: bool,
) -> UiSurface {
    let project_label = if electronics_tools {
        "app.electronics_project_tab"
    } else {
        "app.project_settings_tab"
    };
    let nodes_label = if electronics_tools {
        "app.electronics_nodes_tab"
    } else {
        "app.studio_node_editor"
    };
    let mut root = UiNode::new("editor-shell.bottom-tabs.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            compact: UiCompactMode::Wrap,
            align_items: UiAlign::Center,
            padding: UiSpacing::xy(4.0, 2.0),
            gap: 4.0,
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(palette.toolbar_style())
        .with_child(shell_bottom_button(
            "console",
            "app.studio_console",
            EditorBottomDockTab::Console,
            active,
            palette,
        ));

    if electronics_tools {
        root = root
            .with_child(shell_bottom_button(
                "drc",
                "app.electronics_drc",
                EditorBottomDockTab::Drc,
                active,
                palette,
            ))
            .with_child(shell_bottom_button(
                "simulation",
                "app.electronics_simulation",
                EditorBottomDockTab::Simulation,
                active,
                palette,
            ));
    }

    let root = root
        .with_child(shell_bottom_button(
            "assets",
            "app.studio_assets",
            EditorBottomDockTab::Assets,
            active,
            palette,
        ))
        .with_child(shell_bottom_button(
            "project-settings",
            project_label,
            EditorBottomDockTab::ProjectSettings,
            active,
            palette,
        ))
        .with_child(shell_bottom_button(
            "nodes",
            nodes_label,
            EditorBottomDockTab::NodeEditor,
            active,
            palette,
        ))
        .with_child(shell_bottom_button(
            "agent",
            "app.agent_tab",
            EditorBottomDockTab::Agent,
            active,
            palette,
        ));
    let mut surface = UiSurface::new("editor-shell.bottom-tabs", palette, root);
    surface.style_sheet = shell_style_sheet(palette);
    surface
}

/// Builds the shared inspector selector independently from either panel body.
/// It keeps Sessions as a shell-level workspace concern instead of coupling it
/// to a Game-only or Electronics-only properties implementation.
pub fn build_editor_inspector_tabs_surface(
    palette: StudioUiPalette,
    active: EditorInspectorTab,
) -> UiSurface {
    let root = UiNode::new("editor-shell.inspector-tabs.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            padding: UiSpacing::xy(4.0, 2.0),
            gap: 4.0,
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(palette.toolbar_style())
        .with_child(inspector_tab_button(
            "editor-shell.inspector.properties",
            "app.properties",
            "editor-shell.inspector.properties",
            active == EditorInspectorTab::Properties,
            "editor.shell.inspector.properties-icon",
            palette,
        ))
        .with_child(inspector_tab_button(
            "editor-shell.inspector.sessions",
            "app.sessions",
            "editor-shell.inspector.sessions",
            active == EditorInspectorTab::Sessions,
            "editor.shell.inspector.sessions-icon",
            palette,
        ));
    let mut surface = UiSurface::new("editor-shell.inspector-tabs", palette, root);
    surface.style_sheet = shell_style_sheet(palette);
    surface
}

fn inspector_tab_button(
    id: impl Into<String>,
    label_key: impl Into<String>,
    command: impl Into<String>,
    active: bool,
    icon_key: &str,
    palette: StudioUiPalette,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let class = if active {
        "editor-shell-button-icon-active"
    } else {
        "editor-shell-button"
    };
    UiNode::new(id.clone(), UiNodeKind::Button)
        .with_class(class)
        .with_accessibility_label_key(label_key.clone())
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(8.0, 4.0),
            min_size: [108.0, 28.0],
            ..UiLayout::default()
        })
        .with_child(
            UiNode::image(
                format!("{id}.icon"),
                UiImage {
                    source: UiImageSource::new(icon_key),
                    fit: UiImageFit::Contain,
                    tint: Some(if active {
                        [255, 255, 255, 255]
                    } else {
                        [226, 231, 238, 210]
                    }),
                },
            )
            .with_layout(UiLayout::fixed(14.0, 14.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::button(palette.tokens().text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::default()
                }),
        )
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

/// Builds the contextual center-mode strip. Game exposes Scene; Electronics
/// exposes Schematic and PCB without giving either domain a separate shell.
pub fn build_editor_context_tabs_surface(
    palette: StudioUiPalette,
    project_type: ProjectType,
    active: EditorCenterSurface,
) -> UiSurface {
    let model = EditorShellSurfaceModel {
        project_type,
        center_surface: active,
        ..EditorShellSurfaceModel::default()
    };
    let root = UiNode::new("editor-shell.context-tabs.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(palette.toolbar_style())
        .with_child(context_toolbar(palette, &model));
    let mut surface = UiSurface::new("editor-shell.context-tabs", palette, root);
    surface.style_sheet = shell_style_sheet(palette);
    surface
}

pub fn editor_shell_intent(action: UiAction) -> Option<EditorShellIntent> {
    let UiAction::Command { name } = action else {
        return None;
    };
    match name.as_str() {
        "editor-shell.center.scene" => {
            Some(EditorShellIntent::SelectCenter(EditorCenterSurface::Scene))
        }
        "editor-shell.center.schematic" => Some(EditorShellIntent::SelectCenter(
            EditorCenterSurface::Schematic,
        )),
        "editor-shell.center.pcb" => {
            Some(EditorShellIntent::SelectCenter(EditorCenterSurface::Pcb))
        }
        "editor-shell.bottom.console" => Some(EditorShellIntent::SelectBottom(
            EditorBottomDockTab::Console,
        )),
        "editor-shell.bottom.drc" => {
            Some(EditorShellIntent::SelectBottom(EditorBottomDockTab::Drc))
        }
        "editor-shell.bottom.simulation" => Some(EditorShellIntent::SelectBottom(
            EditorBottomDockTab::Simulation,
        )),
        "editor-shell.bottom.assets" => {
            Some(EditorShellIntent::SelectBottom(EditorBottomDockTab::Assets))
        }
        "editor-shell.bottom.project-settings" => Some(EditorShellIntent::SelectBottom(
            EditorBottomDockTab::ProjectSettings,
        )),
        "editor-shell.bottom.nodes" => Some(EditorShellIntent::SelectBottom(
            EditorBottomDockTab::NodeEditor,
        )),
        "editor-shell.bottom.agent" => {
            Some(EditorShellIntent::SelectBottom(EditorBottomDockTab::Agent))
        }
        "editor-shell.inspector.properties" => Some(EditorShellIntent::SelectInspector(
            EditorInspectorTab::Properties,
        )),
        "editor-shell.inspector.sessions" => Some(EditorShellIntent::SelectInspector(
            EditorInspectorTab::Sessions,
        )),
        "editor-shell.toggle-hierarchy" => {
            Some(EditorShellIntent::TogglePanel(PANEL_HIERARCHY.to_string()))
        }
        "editor-shell.toggle-properties" => {
            Some(EditorShellIntent::TogglePanel(PANEL_PROPERTIES.to_string()))
        }
        "editor-shell.settings" => Some(EditorShellIntent::OpenSettings),
        _ => None,
    }
}

fn global_toolbar(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("editor-shell.global-toolbar", UiNodeKind::Toolbar)
        .with_class("editor-shell-toolbar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            padding: UiSpacing::xy(12.0, 8.0),
            gap: 8.0,
            ..UiLayout::fixed(0.0, 40.0)
        })
        .with_child(shell_button(
            "editor-shell.file",
            "app.file",
            "editor-shell.file-menu",
            false,
            palette,
        ))
        .with_child(shell_button(
            "editor-shell.edit",
            "app.edit_menu",
            "editor-shell.edit-menu",
            false,
            palette,
        ))
        .with_child(shell_button(
            "editor-shell.view",
            "app.view_menu",
            "editor-shell.view-menu",
            false,
            palette,
        ))
        .with_child(
            UiNode::new("editor-shell.global-spacer", UiNodeKind::Panel).with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::default()
            }),
        )
        .with_child(shell_button(
            "editor-shell.settings",
            "app.settings_menu",
            "editor-shell.settings",
            false,
            palette,
        ))
        .with_child(
            UiNode::new("editor-shell.active-edge", UiNodeKind::Separator)
                .with_layout(UiLayout::fixed(2.0, 24.0))
                .with_style(raf_ui::UiStyle {
                    fill: tokens.accent,
                    border: tokens.accent,
                    text: tokens.accent,
                    border_width: 0.0,
                    radius: 0.0,
                    opacity: 1.0,
                }),
        )
}

fn context_toolbar(palette: StudioUiPalette, model: &EditorShellSurfaceModel) -> UiNode {
    let mut toolbar = UiNode::new("editor-shell.context-toolbar", UiNodeKind::Toolbar)
        .with_class("editor-shell-context")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            padding: UiSpacing::xy(12.0, 6.0),
            gap: 6.0,
            ..UiLayout::fixed(0.0, 34.0)
        });

    match model.project_type {
        ProjectType::Game => {
            toolbar = toolbar.with_child(shell_button(
                "editor-shell.center.scene",
                "app.scene_view",
                "editor-shell.center.scene",
                model.center_surface == EditorCenterSurface::Scene,
                palette,
            ));
        }
        ProjectType::Electronics => {
            toolbar = toolbar
                .with_child(context_mode_button(
                    "editor-shell.center.schematic",
                    "app.electronics_schematic_tab",
                    "editor-shell.center.schematic",
                    model.center_surface == EditorCenterSurface::Schematic,
                    94.0,
                    palette,
                ))
                .with_child(context_mode_button(
                    "editor-shell.center.pcb",
                    "app.electronics_pcb_tab",
                    "editor-shell.center.pcb",
                    model.center_surface == EditorCenterSurface::Pcb,
                    58.0,
                    palette,
                ));
        }
    }
    toolbar
}

fn workspace(palette: StudioUiPalette, model: &EditorShellSurfaceModel) -> UiNode {
    let mut workspace = UiNode::new("editor-shell.workspace", UiNodeKind::DockArea)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            compact: UiCompactMode::Stack,
            gap: 1.0,
            grow: 1.0,
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(palette.canvas_style());

    if let Some(column) = dock_column(palette, model, DockSide::Left) {
        workspace = workspace.with_child(column);
    }
    workspace = workspace.with_child(center_slot(palette, model));
    if let Some(column) = dock_column(palette, model, DockSide::Right) {
        workspace = workspace.with_child(column);
    }
    workspace
}

fn dock_column(
    palette: StudioUiPalette,
    model: &EditorShellSurfaceModel,
    side: DockSide,
) -> Option<UiNode> {
    let panels = model
        .layout
        .panels
        .iter()
        .filter(|panel| panel.visible && panel.side == side)
        .collect::<Vec<_>>();
    if panels.is_empty() {
        return None;
    }
    let width = panels
        .iter()
        .map(|panel| panel.preferred_size[0].max(panel.min_size[0]))
        .fold(0.0_f32, f32::max);
    let mut column = UiNode::new(
        format!("editor-shell.{side:?}.column").to_ascii_lowercase(),
        UiNodeKind::Panel,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        basis: [width, 0.0],
        min_size: [
            panels
                .iter()
                .map(|panel| panel.min_size[0])
                .fold(0.0, f32::max),
            0.0,
        ],
        gap: 1.0,
        ..UiLayout::default()
    })
    .with_style(palette.canvas_style());

    for panel in panels {
        let inspector_active = matches!(
            (panel.id.as_str(), model.inspector_tab),
            (PANEL_PROPERTIES, EditorInspectorTab::Properties)
                | (PANEL_SESSIONS, EditorInspectorTab::Sessions)
        );
        column = column.with_child(dock_panel(
            palette,
            panel.id.as_str(),
            panel.title_key.as_str(),
            inspector_active,
        ));
    }
    Some(column)
}

fn center_slot(palette: StudioUiPalette, model: &EditorShellSurfaceModel) -> UiNode {
    let visible = model
        .layout
        .panels
        .iter()
        .any(|panel| panel.id == PANEL_CENTER && panel.visible);
    UiNode::new("editor-shell.center", UiNodeKind::Canvas)
        .with_class("editor-shell-center")
        .with_layout(UiLayout {
            grow: 1.0,
            min_size: [320.0, 240.0],
            ..UiLayout::fill(UiFlow::None)
        })
        .with_style(palette.canvas_style())
        .disabled(!visible)
}

fn bottom_dock(palette: StudioUiPalette, model: &EditorShellSurfaceModel) -> UiNode {
    let project_label = if model.project_type == ProjectType::Electronics {
        "app.electronics_project_tab"
    } else {
        "app.project_settings_tab"
    };
    let nodes_label = if model.project_type == ProjectType::Electronics {
        "app.electronics_nodes_tab"
    } else {
        "app.studio_node_editor"
    };
    let height = model
        .layout
        .panels
        .iter()
        .find(|panel| panel.id == PANEL_BOTTOM)
        .filter(|panel| panel.visible)
        .map(|panel| panel.preferred_size[1].max(panel.min_size[1]))
        .unwrap_or(0.0);
    if height <= 0.0 {
        return UiNode::new("editor-shell.bottom-hidden", UiNodeKind::Panel)
            .with_layout(UiLayout::fixed(0.0, 0.0));
    }

    UiNode::new("editor-shell.bottom", UiNodeKind::Panel)
        .with_class("editor-shell-dock")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::xy(8.0, 6.0),
            gap: 6.0,
            ..UiLayout::fixed(0.0, height)
        })
        .with_style(palette.panel_style())
        .with_child(
            UiNode::new("editor-shell.bottom-tabs", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    compact: UiCompactMode::Wrap,
                    gap: 4.0,
                    ..UiLayout::fixed(0.0, 28.0)
                })
                .with_child(shell_bottom_button(
                    "console",
                    "app.studio_console",
                    EditorBottomDockTab::Console,
                    model.bottom_tab,
                    palette,
                ))
                .with_child(shell_bottom_button(
                    "assets",
                    "app.studio_assets",
                    EditorBottomDockTab::Assets,
                    model.bottom_tab,
                    palette,
                ))
                .with_child(shell_bottom_button(
                    "project-settings",
                    project_label,
                    EditorBottomDockTab::ProjectSettings,
                    model.bottom_tab,
                    palette,
                ))
                .with_child(shell_bottom_button(
                    "nodes",
                    nodes_label,
                    EditorBottomDockTab::NodeEditor,
                    model.bottom_tab,
                    palette,
                ))
                .with_child(shell_bottom_button(
                    "agent",
                    "app.agent_tab",
                    EditorBottomDockTab::Agent,
                    model.bottom_tab,
                    palette,
                )),
        )
        .with_child(
            UiNode::new("editor-shell.bottom-content", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fill(UiFlow::None)
                })
                .with_style(palette.canvas_style()),
        )
}

fn dock_panel(
    palette: StudioUiPalette,
    id: &str,
    title_key: &str,
    inspector_active: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let class = if inspector_active {
        "editor-shell-dock-active"
    } else {
        "editor-shell-dock"
    };
    UiNode::new(format!("editor-shell.dock.{id}"), UiNodeKind::Panel)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            padding: UiSpacing::same(8.0),
            gap: 6.0,
            overflow: raf_ui::UiOverflow::Clip,
            ..UiLayout::default()
        })
        .with_style(palette.panel_style())
        .with_child(
            UiNode::new(format!("editor-shell.dock.{id}.title"), UiNodeKind::Label)
                .with_text_key(title_key)
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 20.0)),
        )
}

fn shell_bottom_button(
    id: &str,
    label_key: &str,
    tab: EditorBottomDockTab,
    active: EditorBottomDockTab,
    palette: StudioUiPalette,
) -> UiNode {
    if let Some(icon_key) = bottom_tab_icon(id) {
        let tokens = palette.tokens();
        let label_key: String = label_key.into();
        let node_id = format!("editor-shell.bottom.{id}");
        let class = if tab == active {
            "editor-shell-button-icon-active"
        } else {
            "editor-shell-button"
        };
        return UiNode::new(node_id.clone(), UiNodeKind::Button)
            .with_class(class)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 6.0,
                padding: UiSpacing::xy(8.0, 4.0),
                min_size: [92.0, 28.0],
                ..UiLayout::default()
            })
            .with_child(
                UiNode::image(
                    format!("{node_id}.icon"),
                    UiImage {
                        source: UiImageSource::new(icon_key),
                        fit: UiImageFit::Contain,
                        tint: Some(if tab == active {
                            [255, 255, 255, 255]
                        } else {
                            [226, 231, 238, 210]
                        }),
                    },
                )
                .with_layout(UiLayout::fixed(16.0, 16.0)),
            )
            .with_child(
                UiNode::new(format!("{node_id}.label"), UiNodeKind::Label)
                    .with_text_key(label_key.clone())
                    .with_text_style(UiTextStyle::button(tokens.text))
                    .with_layout(UiLayout {
                        grow: 1.0,
                        ..UiLayout::default()
                    }),
            )
            .with_tooltip_key(label_key)
            .focusable()
            .with_event(UiEventBinding::command(UiEventKind::Click, node_id));
    }
    shell_button(
        format!("editor-shell.bottom.{id}"),
        label_key,
        format!("editor-shell.bottom.{id}"),
        tab == active,
        palette,
    )
}

fn bottom_tab_icon(id: &str) -> Option<&'static str> {
    match id {
        "console" => Some("editor.bottom.console"),
        "assets" => Some("editor.bottom.assets"),
        "project-settings" => Some("editor.bottom.project-settings"),
        "nodes" => Some("editor.bottom.nodes"),
        "agent" => Some("editor.bottom.agent"),
        _ => None,
    }
}

fn shell_button(
    id: impl Into<String>,
    label_key: impl Into<String>,
    command: impl Into<String>,
    active: bool,
    palette: StudioUiPalette,
) -> UiNode {
    let tokens = palette.tokens();
    let label_key = label_key.into();
    let class = if active {
        "editor-shell-button-active"
    } else {
        "editor-shell-button"
    };
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_text_key(label_key.clone())
        .with_tooltip_key(label_key.clone())
        .with_accessibility_label_key(label_key)
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_layout(UiLayout {
            padding: UiSpacing::xy(8.0, 4.0),
            min_size: [64.0, 24.0],
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn context_mode_button(
    id: impl Into<String>,
    label_key: impl Into<String>,
    command: impl Into<String>,
    active: bool,
    width: f32,
    palette: StudioUiPalette,
) -> UiNode {
    let tokens = palette.tokens();
    let label_key = label_key.into();
    let class = if active {
        "editor-shell-button-active"
    } else {
        "editor-shell-button"
    };
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_text_key(label_key.clone())
        .with_tooltip_key(label_key.clone())
        .with_accessibility_label_key(label_key)
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_layout(UiLayout {
            basis: [width, 24.0],
            min_size: [width, 24.0],
            max_size: [width, 24.0],
            padding: UiSpacing::xy(8.0, 4.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn shell_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("editor-shell-toolbar".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("editor-shell-context".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("editor-shell-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("editor-shell-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.focus),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("editor-shell-button-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("editor-shell-button-icon-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("editor-shell-dock-active".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_uses_one_center_slot_and_a_fixed_bottom_dock() {
        let surface = build_editor_shell_surface(
            StudioUiPalette::IndustrialDark,
            &EditorShellSurfaceModel::default(),
        );
        let frame = surface.build_frame(1440, 900, [8, 11, 15, 255]);

        assert!(frame
            .layout_boxes
            .iter()
            .any(|entry| entry.id == "editor-shell.center" && entry.kind == UiNodeKind::Canvas));
        assert!(frame
            .layout_boxes
            .iter()
            .any(|entry| entry.id == "editor-shell.bottom"));
    }

    #[test]
    fn electronics_context_has_schematic_and_pcb_modes() {
        let model = EditorShellSurfaceModel {
            project_type: ProjectType::Electronics,
            center_surface: EditorCenterSurface::Schematic,
            ..EditorShellSurfaceModel::default()
        };
        let surface = build_editor_shell_surface(StudioUiPalette::IndustrialDark, &model);

        assert!(surface
            .root
            .children
            .iter()
            .flat_map(|node| node.children.iter())
            .any(|node| node.id == "editor-shell.center.schematic"));
        assert!(surface
            .root
            .children
            .iter()
            .flat_map(|node| node.children.iter())
            .any(|node| node.id == "editor-shell.center.pcb"));
    }

    #[test]
    fn shell_commands_remain_typed_at_the_application_boundary() {
        assert_eq!(
            editor_shell_intent(UiAction::Command {
                name: "editor-shell.bottom.assets".to_string(),
            }),
            Some(EditorShellIntent::SelectBottom(EditorBottomDockTab::Assets))
        );
        assert_eq!(
            editor_shell_intent(UiAction::Command {
                name: "editor-shell.inspector.sessions".to_string(),
            }),
            Some(EditorShellIntent::SelectInspector(
                EditorInspectorTab::Sessions
            ))
        );
    }
}
