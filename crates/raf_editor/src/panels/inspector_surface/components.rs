//! Inspector shell and reusable chrome: header, tabs, session rows, and status actions.

use super::*;

pub(super) fn header(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("inspector.header", UiNodeKind::Toolbar)
        .with_class("inspector-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            ..UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("inspector.header.icon", UiNodeKind::Label)
                .with_icon(UiIcon::new(UiIconId::Entity))
                .with_layout(UiLayout::fixed(20.0, 22.0)),
        )
        .with_child(
            UiNode::new("inspector.header.title", UiNodeKind::Label)
                .with_text_key("app.inspector")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
        )
        .with_child(icon_button(
            palette,
            "inspector.close",
            UiIconId::Close,
            "inspector.panel.toggle",
            "ui.close",
        ))
}

pub(super) fn tabs(palette: StudioUiPalette, active: InspectorTab) -> UiNode {
    UiNode::new("inspector.tabs", UiNodeKind::Toolbar)
        .with_class("inspector-tabs")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 2.0,
            padding: UiSpacing::same(2.0),
            ..UiLayout::fixed(0.0, 22.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(tab_button(
            palette,
            "inspector.properties-tab",
            "app.properties",
            UiIconId::Settings,
            "electronics.tooltip.inspector.properties",
            "inspector.tab:properties",
            active == InspectorTab::Properties,
        ))
        .with_child(tab_button(
            palette,
            "inspector.sessions-tab",
            "app.sessions",
            UiIconId::Scene,
            "electronics.tooltip.inspector.sessions",
            "inspector.tab:sessions",
            active == InspectorTab::Sessions,
        ))
}

fn tab_button(
    palette: StudioUiPalette,
    id: &str,
    text_key: &str,
    icon: UiIconId,
    tooltip_key: &str,
    command: &str,
    active: bool,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if active {
            "inspector-tab-active"
        } else {
            "inspector-tab"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 4.0,
            grow: 1.0,
            min_size: [64.0, 20.0],
            padding: UiSpacing::xy(6.0, 0.0),
            ..UiLayout::fixed(0.0, 20.0)
        })
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::button(if active {
            palette.tokens().text
        } else {
            palette.tokens().text_muted
        }))
        .with_tooltip_key(tooltip_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

pub(super) fn session_context_bar(palette: StudioUiPalette, sessions: &ProjectSessionRegistry) -> UiNode {
    let tokens = palette.tokens();
    let active_name = sessions
        .sessions
        .iter()
        .find(|session| session.id == sessions.active_session)
        .map(|session| session.name.clone())
        .unwrap_or_default();
    let count = sessions.sessions.len();
    UiNode::new("inspector.session.context", UiNodeKind::Toolbar)
        .with_class("inspector-session-context")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(6.0, 2.0),
            ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("inspector.session.context.icon", UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(UiIconId::Scene)
                        .with_size(UiIconSize::Small)
                        .with_tint(tokens.accent),
                )
                .with_layout(UiLayout::fixed(16.0, 16.0)),
        )
        .with_child(
            UiNode::new("inspector.session.context.name", UiNodeKind::Label)
                .with_text_value(active_name)
                .with_text_style(UiTextStyle::button(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
        )
        .with_child(
            UiNode::new("inspector.session.context.count", UiNodeKind::Label)
                .with_text_value(format!("{count}"))
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("inspector.session.context.manage", UiNodeKind::Button)
                .with_class("inspector-session-manage")
                .with_layout(UiLayout::fixed(64.0, 20.0))
                .with_text_key("app.sessions")
                .with_text_style(UiTextStyle::button(tokens.text_muted))
                .with_tooltip_key("electronics.tooltip.inspector.sessions")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "inspector.tab:sessions",
                )),
        )
}
pub(crate) fn sessions_content(
    palette: StudioUiPalette,
    registry: &ProjectSessionRegistry,
) -> UiNode {
    let tokens = palette.tokens();
    let mut list = UiNode::scroll_view("inspector.sessions.list", UiScrollAxis::Vertical)
        .with_class("inspector-session-list")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            grow: 1.0,
            padding: UiSpacing {
                left: 2.0,
                right: 4.0,
                top: 0.0,
                bottom: 4.0,
            },
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });

    for session in &registry.sessions {
        list = list.with_child(session_row(palette, registry, session));
    }

    let count_text = format!(" ({})", registry.sessions.len());
    let controls = UiNode::new("inspector.sessions.controls", UiNodeKind::Panel)
        .with_class("inspector-sessions-controls")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("inspector.sessions.summary", UiNodeKind::Toolbar)
                .with_class("inspector-sessions-heading")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 6.0,
                    ..UiLayout::fixed(0.0, 22.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("inspector.sessions.summary.icon", UiNodeKind::Label)
                        .with_icon(UiIcon::new(UiIconId::Folder).with_size(UiIconSize::Small))
                        .with_layout(UiLayout::fixed(16.0, 16.0)),
                )
                .with_child(
                    UiNode::new("inspector.sessions.summary.label", UiNodeKind::Label)
                        .with_text_key("app.session_registry")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout::fit_content()),
                )
                .with_child(
                    UiNode::new("inspector.sessions.summary.count", UiNodeKind::Label)
                        .with_text_value(count_text)
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::fit_content()
                        }),
                ),
        )
        .with_child(
            UiNode::new("inspector.session.create-row", UiNodeKind::Toolbar)
                .with_class("inspector-session-create-row")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 4.0,
                    ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::text_input(
                        "inspector.session.new_name",
                        UiTextInput {
                            value_key: "inspector.session.new_name".to_string(),
                            placeholder_key: Some("app.session_name".to_string()),
                            max_length: 128,
                            multiline: false,
                            password: false,
                            submit_command: None,
                        },
                    )
                    .with_class("inspector-input")
                    .with_layout(UiLayout {
                        grow: 1.0,
                        min_size: [80.0, 24.0],
                        ..UiLayout::fixed(0.0, 24.0)
                    }),
                )
                .with_child(session_create_button(palette)),
        );

    UiNode::new("inspector.sessions.content", UiNodeKind::Panel)
        .with_class("inspector-sessions-content")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 8.0,
            padding: UiSpacing::xy(6.0, 6.0),
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(controls)
        .with_child(list)
}

fn session_create_button(palette: StudioUiPalette) -> UiNode {
    UiNode::new("inspector.session.create", UiNodeKind::Button)
        .with_class("inspector-session-create")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, 24.0)
        })
        .with_icon(UiIcon::new(UiIconId::Add).with_size(UiIconSize::Small))
        .with_text_key("app.session_create")
        .with_text_style(UiTextStyle::button(palette.accent_style().text))
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "inspector.session.create",
        ))
}

fn session_icon_action(
    id: &str,
    icon: UiIconId,
    tooltip: &str,
    command: String,
    is_delete: bool,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if is_delete {
            "inspector-session-delete-btn"
        } else {
            "inspector-session-action-btn"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            ..UiLayout::fixed(24.0, 24.0)
        })
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_tooltip_key(tooltip)
        .with_accessibility_label_key(tooltip)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn session_row(
    palette: StudioUiPalette,
    registry: &ProjectSessionRegistry,
    session: &raf_core::session::ProjectSession,
) -> UiNode {
    let active = registry.active_session == session.id;
    let tokens = palette.tokens();
    let id = session.id.0;

    let mut row = UiNode::new(format!("inspector.session.row.{}", id), UiNodeKind::Panel)
        .with_class(if active {
            "inspector-session-active"
        } else {
            "inspector-session-row"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(6.0, 3.0),
            ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
        });

    row = row.with_child(
        UiNode::new(format!("inspector.session.icon.{}", id), UiNodeKind::Label)
            .with_icon(
                UiIcon::new(session_kind_icon(session.kind))
                    .with_size(UiIconSize::Small)
                    .with_tint(if active {
                        tokens.accent
                    } else {
                        tokens.text_muted
                    }),
            )
            .with_layout(UiLayout::fixed(16.0, 16.0)),
    );

    row = row.with_child(
        UiNode::new(format!("inspector.session.name.{}", id), UiNodeKind::Label)
            .with_text_value(session.name.clone())
            .with_text_style(UiTextStyle::button(tokens.text))
            .with_layout(UiLayout::fit_content()),
    );

    row = row.with_child(
        UiNode::new(format!("inspector.session.kind.{}", id), UiNodeKind::Label)
            .with_class("inspector-session-kind-chip")
            .with_text_key(session_kind_label(session.kind))
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_layout(UiLayout {
                padding: UiSpacing::xy(5.0, 1.0),
                ..UiLayout::fit_content()
            }),
    );

    row = row.with_child(
        UiNode::new(
            format!("inspector.session.spacer.{}", id),
            UiNodeKind::Panel,
        )
        .with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::fixed(0.0, 0.0)
        }),
    );

    if active {
        row = row.with_child(
            UiNode::new(
                format!("inspector.session.active.{}", id),
                UiNodeKind::Label,
            )
            .with_class("inspector-session-active-badge")
            .with_text_key("app.session_active")
            .with_text_style(UiTextStyle::body(tokens.accent))
            .with_layout(UiLayout {
                padding: UiSpacing::xy(6.0, 1.0),
                ..UiLayout::fit_content()
            }),
        );
    }

    let mut actions = UiNode::new(
        format!("inspector.session.actions.{}", id),
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 2.0,
        ..UiLayout::fit_content()
    });

    if !active {
        actions = actions.with_child(session_icon_action(
            &format!("inspector.session.open.{id}"),
            UiIconId::Play,
            "app.session_open",
            format!("inspector.session.open:{id}"),
            false,
        ));
    }

    actions = actions.with_child(session_icon_action(
        &format!("inspector.session.duplicate.{id}"),
        UiIconId::Add,
        "app.session_duplicate",
        format!("inspector.session.duplicate:{id}"),
        false,
    ));

    if !active {
        actions = actions.with_child(session_icon_action(
            &format!("inspector.session.remove.{id}"),
            UiIconId::Trash,
            "app.session_remove",
            format!("inspector.session.remove:{id}"),
            true,
        ));
    }

    row.with_child(actions)
}

fn session_kind_icon(kind: ProjectSessionKind) -> UiIconId {
    match kind {
        ProjectSessionKind::World => UiIconId::Scene,
        ProjectSessionKind::Interface => UiIconId::Node,
        ProjectSessionKind::ElectronicsDesign => UiIconId::Schematic,
    }
}

fn session_kind_label(kind: ProjectSessionKind) -> &'static str {
    match kind {
        ProjectSessionKind::World => "app.session_kind_world",
        ProjectSessionKind::Interface => "app.session_kind_interface",
        ProjectSessionKind::ElectronicsDesign => "app.session_kind_electronics",
    }
}
pub(super) fn node_info(
    palette: StudioUiPalette,
    scene: &SceneGraph,
    node: &raf_core::scene::SceneNode,
) -> UiNode {
    let parent = node
        .parent
        .and_then(|parent| scene.get(parent).map(|parent| parent.name.clone()))
        .unwrap_or_else(|| "-".to_string());
    UiNode::new("inspector.node-info", UiNodeKind::Panel)
        .with_class("inspector-node-info")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(metadata_line(palette, "app.type", node.primitive.label()))
        .with_child(metadata_line(palette, "app.parent", &parent))
        .with_child(metadata_line(
            palette,
            "app.children",
            &node.children.len().to_string(),
        ))
        .with_child(metadata_line(
            palette,
            "app.scripts",
            &node.scripts.len().to_string(),
        ))
}
pub(super) fn metadata_key_line(palette: StudioUiPalette, label_key: &str, value_key: &str) -> UiNode {
    UiNode::new(
        format!("inspector.metadata.{label_key}"),
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        gap: 5.0,
        ..UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(
            format!("inspector.metadata.{label_key}.label"),
            UiNodeKind::Label,
        )
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
        .with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::fit_content()
        }),
    )
    .with_child(
        UiNode::new(
            format!("inspector.metadata.{label_key}.value"),
            UiNodeKind::Label,
        )
        .with_text_key(value_key)
        .with_text_style(UiTextStyle::body(palette.tokens().text))
        .with_layout(UiLayout::fit_content()),
    )
}

pub(super) fn metadata_line(palette: StudioUiPalette, label_key: &str, value: &str) -> UiNode {
    UiNode::new(
        format!("inspector.metadata.{label_key}"),
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        gap: 5.0,
        ..UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(
            format!("inspector.metadata.{label_key}.label"),
            UiNodeKind::Label,
        )
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
        .with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::fit_content()
        }),
    )
    .with_child(
        UiNode::new(
            format!("inspector.metadata.{label_key}.value"),
            UiNodeKind::Label,
        )
        .with_text_value(value.to_string())
        .with_text_style(UiTextStyle::body(palette.tokens().text))
        .with_layout(UiLayout {
            min_size: [72.0, 18.0],
            max_size: [210.0, 20.0],
            overflow: UiOverflow::Clip,
            ..UiLayout::fit_content()
        })
        .with_tooltip_value(value.to_string()),
    )
}
pub(super) fn status_row(palette: StudioUiPalette, id: SceneNodeId, visible: bool, locked: bool) -> UiNode {
    UiNode::new("inspector.status", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(status_button(
            palette,
            "inspector.visibility",
            if visible { "app.visible" } else { "app.hidden" },
            format!("inspector.visibility:{id}", id = id.0),
            visible,
            if visible {
                UiIconId::Eye
            } else {
                UiIconId::EyeOff
            },
        ))
        .with_child(status_button(
            palette,
            "inspector.lock",
            if locked { "app.locked" } else { "app.unlocked" },
            format!("inspector.lock:{id}", id = id.0),
            locked,
            if locked {
                UiIconId::Lock
            } else {
                UiIconId::Unlock
            },
        ))
}

fn status_button(
    palette: StudioUiPalette,
    id: &str,
    text_key: &str,
    command: String,
    active: bool,
    icon: UiIconId,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if active {
            "inspector-status-on"
        } else {
            "inspector-status-off"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: raf_ui::UiJustify::Center,
            gap: 4.0,
            grow: 1.0,
            min_size: [72.0, 26.0],
            ..UiLayout::fixed(0.0, 26.0)
        })
        .with_icon(
            UiIcon::new(icon)
                .with_size(raf_render::api_graphic_basic::ui_surface::UiIconSize::Small),
        )
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::button(if active {
            tokens.text
        } else {
            tokens.text_muted
        }))
        .with_tooltip_key(text_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}
pub(crate) fn action_button(palette: StudioUiPalette, id: &str, text_key: &str, command: String) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("inspector-button")
        .with_layout(UiLayout {
            grow: 1.0,
            min_size: [80.0, 28.0],
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, 28.0)
        })
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}
fn icon_button(
    _palette: StudioUiPalette,
    id: &str,
    icon: UiIconId,
    command: &str,
    tooltip: &str,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("inspector-icon-button")
        .with_layout(UiLayout::fixed(24.0, 26.0))
        .with_icon(UiIcon::new(icon))
        .with_tooltip_key(tooltip)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}
