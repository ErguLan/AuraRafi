//! Hub action mapping, context menus and the Get in dropdown.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiImage, UiImageFit,
    UiImageSource, UiLayout, UiNode, UiNodeKind, UiSizeMode, UiSpacing, UiSurfaceMaterial,
    UiTextStyle,
};
use serde_json::json;

use super::*;

pub(crate) fn hub_context_menu_surface(
    palette: StudioUiPalette,
    project: &HubSurfaceProject,
    position: [f32; 2],
    confirm_forget: bool,
) -> UiNode {
    let tokens = hub_tokens(palette);
    UiNode::new("hub.context-menu", UiNodeKind::Menu)
        .with_class("hub-context-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(
            UiLayout {
                flow: UiFlow::Column,
                padding: UiSpacing::same(6.0),
                gap: 2.0,
                ..UiLayout::absolute(raf_render::api_graphic_basic::ui_surface::UiRect::new(
                    position[0],
                    position[1],
                    HUB_CONTEXT_MENU_SIZE[0],
                    HUB_CONTEXT_MENU_SIZE[1],
                ))
            }
            .with_z_index(100),
        )
        .with_child(
            UiNode::new("hub.context-title", UiNodeKind::Label)
                .with_text_key("hub.context.title")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    padding: UiSpacing::xy(10.0, 0.0),
                    ..UiLayout::fixed(0.0, 26.0)
                }),
        )
        .with_child(hub_menu_divider("hub.context-divider-top"))
        .with_child(hub_menu_row(
            palette,
            "hub.context-open",
            "app.hub_open",
            project,
            "open",
            "editor.hub.context-open",
        ))
        .with_child(hub_menu_row(
            palette,
            "hub.context-agent",
            "agent_workspace.work_with_ai",
            project,
            "agent",
            "editor.hub.agent",
        ))
        .with_child(hub_menu_row(
            palette,
            "hub.context-duplicate",
            "app.hub_duplicate",
            project,
            "duplicate",
            "editor.hub.context-duplicate",
        ))
        .with_child(hub_menu_divider("hub.context-divider-bottom"))
        .with_child(hub_menu_row(
            palette,
            "hub.context-forget",
            if confirm_forget {
                "app.hub_remove_confirm"
            } else {
                "app.hub_delete"
            },
            project,
            "forget",
            "editor.hub.context-forget",
        ))
}

/// One desktop-grade context-menu row: borderless, full-width, with a semantic
/// icon. Resting fill is transparent; hover illumination comes from the Hub
/// stylesheet so the menu reads as a menu rather than a stack of buttons.
pub(crate) fn hub_menu_row(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    project: &HubSurfaceProject,
    action: &str,
    icon_key: &str,
) -> UiNode {
    let destructive = action == "forget";
    let class = if destructive {
        "hub-menu-row-danger"
    } else {
        "hub-menu-row"
    };
    let text_color = if destructive {
        palette.tokens().danger
    } else {
        palette.tokens().text
    };
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 10.0,
            padding: UiSpacing::xy(10.0, 0.0),
            basis: [0.0, 32.0],
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fixed,
            ..UiLayout::default()
        })
        .focusable()
        .with_event(project_action(project, action))
        .with_child(
            UiNode::image(
                format!("{id}.icon"),
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new(icon_key),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(16.0, 16.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::button(text_color))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 18.0)
                }),
        )
}

pub(crate) fn hub_menu_divider(id: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("hub-menu-divider")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            basis: [0.0, 1.0],
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fixed,
            ..UiLayout::default()
        })
}

/// "Get in" dropdown: the single entry point for creating a project or opening
/// one that lives anywhere on disk.
pub(crate) fn hub_get_in_menu_surface(palette: StudioUiPalette, position: [f32; 2]) -> UiNode {
    UiNode::new("hub.getin-menu", UiNodeKind::Menu)
        .with_class("hub-context-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(
            UiLayout {
                flow: UiFlow::Column,
                padding: UiSpacing::same(6.0),
                gap: 2.0,
                ..UiLayout::absolute(raf_render::api_graphic_basic::ui_surface::UiRect::new(
                    position[0],
                    position[1],
                    HUB_GET_IN_MENU_SIZE[0],
                    HUB_GET_IN_MENU_SIZE[1],
                ))
            }
            .with_z_index(100),
        )
        .with_child(hub_menu_command_row(
            palette,
            "hub.getin.new",
            "app.hub_new_project",
            "hub.getin.new",
            "editor.hub.plus",
        ))
        .with_child(hub_menu_command_row(
            palette,
            "hub.getin.open-folder",
            "app.hub_open_folder",
            "hub.getin.open-folder",
            "editor.hub.folder",
        ))
        .with_child(hub_menu_command_row(
            palette,
            "hub.getin.open-file",
            "app.hub_open_ron",
            "hub.getin.open-file",
            "editor.hub.project-file",
        ))
}

pub(crate) fn hub_menu_command_row(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    command: &str,
    icon_key: &str,
) -> UiNode {
    let tokens = hub_tokens(palette);
    UiNode::new(id, UiNodeKind::Button)
        .with_class("hub-menu-row")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 10.0,
            padding: UiSpacing::xy(10.0, 0.0),
            basis: [0.0, 32.0],
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fixed,
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_child(
            UiNode::image(
                format!("{id}.icon"),
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new(icon_key),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(16.0, 16.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::button(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 18.0)
                }),
        )
}

pub(crate) fn project_action(project: &HubSurfaceProject, action: &str) -> UiEventBinding {
    UiEventBinding {
        event: UiEventKind::Click,
        action: UiAction::Custom {
            channel: "hub.project".to_string(),
            payload: json!({
                "action": action,
                "path": project.path.to_string_lossy(),
            }),
        },
    }
}

pub(crate) fn project_context_action(project: &HubSurfaceProject) -> UiEventBinding {
    project_menu_event(project, UiEventKind::ContextMenu)
}

pub(crate) fn project_menu_action(project: &HubSurfaceProject) -> UiEventBinding {
    project_menu_event(project, UiEventKind::Click)
}

pub(crate) fn project_menu_event(
    project: &HubSurfaceProject,
    event: UiEventKind,
) -> UiEventBinding {
    UiEventBinding {
        event,
        action: UiAction::Custom {
            channel: "hub.project".to_string(),
            payload: json!({
                "action": "menu",
                "path": project.path.to_string_lossy(),
            }),
        },
    }
}

pub(crate) fn project_hover_event(
    project: &HubSurfaceProject,
    event: UiEventKind,
) -> UiEventBinding {
    let action = match &event {
        UiEventKind::HoverEnter => "hover",
        UiEventKind::HoverLeave => "leave",
        _ => return project_menu_event(project, event),
    };
    UiEventBinding {
        event,
        action: UiAction::Custom {
            channel: "hub.project".to_string(),
            payload: json!({
                "action": action,
                "path": project.path.to_string_lossy(),
            }),
        },
    }
}
