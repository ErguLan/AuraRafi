//! Retained Hierarchy row nodes: overlap, guides, labels and row actions.

use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiIcon, UiIconId, UiIconSize};
use raf_ui::{
    UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiFontWeight, UiJustify, UiLayout,
    UiNode, UiNodeKind, UiOverflow, UiSizeMode, UiSpacing, UiStyle, UiTextInput, UiTextOverflow,
    UiTextRole, UiTextStyle,
};

use super::super::hierarchy_model::HierarchyRow;
use super::badges;
use super::icon_button;

pub(super) fn row_node(
    palette: StudioUiPalette,
    row: &HierarchyRow,
    selected: &[raf_core::scene::SceneNodeId],
    renaming: Option<(raf_core::scene::SceneNodeId, &str)>,
    row_height: f32,
    indent_width: f32,
    show_icons: bool,
    show_visibility: bool,
    show_locked: bool,
    show_script_indicators: bool,
    drop_target: Option<raf_core::scene::SceneNodeId>,
) -> UiNode {
    let is_selected = selected.contains(&row.id);
    if let Some((index, link)) = &row.script_link {
        let tokens = palette.tokens();
        let key = format!("hierarchy.script.{}.{}", row.id.0, index);
        let status = match link.kind {
            crate::script_relationships::ScriptLinkKind::Attached => {
                "app.hierarchy_script_attached"
            }
            crate::script_relationships::ScriptLinkKind::Reference => {
                "app.hierarchy_script_reference"
            }
        };
        return UiNode::new(&key, UiNodeKind::Toolbar)
            .with_class("hierarchy-row")
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 4.0,
                padding: UiSpacing {
                    left: row.depth as f32 * indent_width.max(8.0) + 24.0,
                    right: 6.0,
                    top: 1.0,
                    bottom: 1.0,
                },
                ..UiLayout::fixed(0.0, row_height).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new(format!("{key}.open"), UiNodeKind::Button)
                    .with_class("hierarchy-row-label")
                    .with_layout(UiLayout {
                        grow: 1.0,
                        min_size: [24.0, row_height - 2.0],
                        overflow: UiOverflow::Clip,
                        ..UiLayout::fixed(0.0, row_height - 2.0)
                    })
                    .with_icon(
                        UiIcon::new(UiIconId::Script)
                            .with_size(UiIconSize::Small)
                            .with_tint(tokens.text_muted),
                    )
                    .with_text_value(&row.name)
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_text_style(compact_body_style(tokens.text_muted))
                    .with_tooltip_value(&link.path)
                    .with_accessibility_description_key(status)
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        format!("hierarchy.script.open:{}:{index}", row.id.0),
                    )),
            )
            .with_child(
                UiNode::new(format!("{key}.kind"), UiNodeKind::Label)
                    .with_layout(UiLayout::fixed(76.0, row_height - 2.0))
                    .with_text_key(status)
                    .with_tooltip_key(status)
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_text_style(compact_body_style(tokens.text_muted)),
            );
    }
    let class = if drop_target == Some(row.id) {
        "hierarchy-row-drop-target"
    } else if is_selected {
        "hierarchy-row-selected"
    } else if !row.visible {
        "hierarchy-row-hidden"
    } else if row.locked {
        "hierarchy-row-locked"
    } else {
        "hierarchy-row"
    };
    let mut root = UiNode::new(format!("hierarchy.row.{}", row.id.0), UiNodeKind::Toolbar)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 3.0,
            padding: UiSpacing::xy(3.0, 1.0),
            ..UiLayout::fixed(0.0, row_height).with_width_mode(UiSizeMode::Fill)
        });

    root = root.with_child(
        UiNode::new(
            format!("hierarchy.row.{}.selection-edge", row.id.0),
            UiNodeKind::Panel,
        )
        .with_class(if is_selected {
            "hierarchy-selection-edge"
        } else {
            "hierarchy-selection-edge-idle"
        })
        .with_layout(UiLayout::fixed(2.0, row_height - 4.0)),
    );

    for level in 0..row.depth {
        root = root.with_child(indent_guide(
            palette,
            row.id,
            level,
            row_height,
            indent_width,
        ));
    }
    if row.has_children {
        root = root.with_child(
            UiNode::new(
                format!("hierarchy.row.{}.expand", row.id.0),
                UiNodeKind::Button,
            )
            .with_class("hierarchy-expand")
            .with_class("hierarchy-row-action")
            .with_class(if is_selected {
                "hierarchy-row-action-selected"
            } else {
                ""
            })
            .with_layout(UiLayout::fixed(18.0, row_height - 2.0))
            .with_icon(
                UiIcon::new(if row.expanded {
                    UiIconId::ChevronDown
                } else {
                    UiIconId::ChevronRight
                })
                .with_size(UiIconSize::Small),
            )
            .with_tooltip_key(if row.expanded {
                "app.collapse_menu"
            } else {
                "app.expand_menu"
            })
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("hierarchy.expand:{}", row.id.0),
            )),
        );
    } else {
        root = root.with_child(
            UiNode::new(
                format!("hierarchy.row.{}.expand-spacer", row.id.0),
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout::fixed(18.0, row_height)),
        );
    }

    if let Some((rename_id, _rename_value)) = renaming {
        if rename_id == row.id {
            root = root.with_child(
                UiNode::text_input(
                    format!("hierarchy.rename.control.{}", row.id.0),
                    UiTextInput {
                        value_key: format!("hierarchy.rename.{}", row.id.0),
                        placeholder_key: Some("app.rename".to_string()),
                        max_length: 256,
                        multiline: false,
                        password: false,
                        submit_command: Some(format!("hierarchy.rename.commit:{}", row.id.0)),
                    },
                )
                .with_class("hierarchy-rename-input")
                .with_layout(UiLayout {
                    grow: 1.0,
                    min_size: [72.0, row_height - 2.0],
                    overflow: UiOverflow::Clip,
                    ..UiLayout::fixed(0.0, row_height - 2.0)
                }),
            );
            return root;
        }
    }

    let mut main = UiNode::new(
        format!("hierarchy.row.{}.select", row.id.0),
        UiNodeKind::Button,
    )
    .with_class("hierarchy-row-label")
    .with_layout(UiLayout {
        grow: 1.0,
        min_size: [64.0, row_height - 2.0],
        padding: UiSpacing::xy(4.0, 0.0),
        overflow: UiOverflow::Clip,
        ..UiLayout::fixed(0.0, row_height - 2.0)
    })
    .with_text_value(row.name.clone())
    .with_text_overflow(UiTextOverflow::Ellipsis)
    .with_tooltip_value(row.name.clone())
    .with_text_style(compact_body_style(if is_selected {
        [255, 247, 232, 255]
    } else {
        [220, 224, 230, 255]
    }))
    .focusable()
    .with_event(UiEventBinding::command(
        UiEventKind::Click,
        format!("hierarchy.select:{}", row.id.0),
    ))
    .with_event(UiEventBinding {
        event: UiEventKind::DoubleClick,
        action: UiAction::Command {
            name: format!("hierarchy.rename.begin:{}", row.id.0),
        },
    })
    .with_event(UiEventBinding {
        event: UiEventKind::ContextMenu,
        action: UiAction::OpenMenu {
            id: format!("hierarchy.menu:{}", row.id.0),
        },
    })
    .with_event(UiEventBinding::command(
        UiEventKind::DragStart,
        format!("hierarchy.drag.start:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::DragMove,
        format!("hierarchy.drag.move:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::DragEnd,
        format!("hierarchy.drag.end:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::PointerMove,
        format!("hierarchy.drag.over:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("f2".to_string()),
        format!("hierarchy.rename.begin:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("delete".to_string()),
        format!("hierarchy.delete:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("arrowup".to_string()),
        format!("hierarchy.navigate:up:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("arrowdown".to_string()),
        format!("hierarchy.navigate:down:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("arrowleft".to_string()),
        format!("hierarchy.navigate:left:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("arrowright".to_string()),
        format!("hierarchy.navigate:right:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("home".to_string()),
        format!("hierarchy.navigate:home:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("end".to_string()),
        format!("hierarchy.navigate:end:{}", row.id.0),
    ));
    if show_icons {
        let icon_tint = if is_selected {
            palette.tokens().accent_hot
        } else {
            palette.tokens().text_muted
        };
        main = main.with_icon(
            UiIcon::new(row_icon(row))
                .with_size(UiIconSize::Small)
                .with_tint(icon_tint),
        );
    }
    root = root.with_child(main);
    if row.insertion_parent {
        root = root.with_child(
            UiNode::new(format!("hierarchy.parent.{}", row.id.0), UiNodeKind::Label)
                .with_icon(UiIcon::new(UiIconId::Add).with_size(UiIconSize::Small))
                .with_tooltip_key("hierarchy.insertion_parent")
                .with_layout(UiLayout::fixed(18.0, row_height - 2.0)),
        );
    }
    if show_script_indicators && row.has_scripts {
        root = root.with_child(
            UiNode::new(
                format!("hierarchy.script-indicator.{}", row.id.0),
                UiNodeKind::Label,
            )
            .with_icon(
                UiIcon::new(UiIconId::Script)
                    .with_size(UiIconSize::Small)
                    .with_tint(palette.tokens().accent_hot),
            )
            .with_tooltip_key("hierarchy.badge.scripts")
            .with_layout(UiLayout::fixed(16.0, row_height - 2.0)),
        );
    }
    root = badges::append_badges(root, row, row_height);
    if show_visibility {
        root = root.with_child(
            icon_button(
                palette,
                &format!("hierarchy.row.{}.visibility", row.id.0),
                if row.visible {
                    UiIconId::Eye
                } else {
                    UiIconId::EyeOff
                },
                format!("hierarchy.visibility:{}", row.id.0),
                "app.visible",
            )
            .with_class("hierarchy-row-action")
            .with_class(if is_selected {
                "hierarchy-row-action-selected"
            } else {
                ""
            }),
        );
    }
    if show_locked {
        root = root.with_child(
            UiNode::new(
                format!("hierarchy.row.{}.lock", row.id.0),
                UiNodeKind::Button,
            )
            .with_class("hierarchy-icon-button")
            .with_class("hierarchy-row-action")
            .with_class(if is_selected {
                "hierarchy-row-action-selected"
            } else {
                ""
            })
            .with_layout(UiLayout::fixed(24.0, row_height - 2.0))
            .with_icon(
                UiIcon::new(if row.locked {
                    UiIconId::Lock
                } else {
                    UiIconId::Unlock
                })
                .with_size(UiIconSize::Small),
            )
            .with_tooltip_key("app.hierarchy_lock")
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("hierarchy.lock:{}", row.id.0),
            )),
        );
    }
    root.with_child(
        icon_button(
            palette,
            &format!("hierarchy.row.{}.menu", row.id.0),
            UiIconId::More,
            format!("hierarchy.menu:{}", row.id.0),
            "app.more_menu",
        )
        .with_class("hierarchy-row-action")
        .with_class(if is_selected {
            "hierarchy-row-action-selected"
        } else {
            ""
        }),
    )
}

pub(super) fn compact_body_style(color: [u8; 4]) -> UiTextStyle {
    UiTextStyle {
        // Hierarchy labels are compact controls, not prose. Keeping them in
        // the single-line role prevents narrow panels from wrapping names
        // one character at a time.
        role: UiTextRole::Button,
        size_px: 12.0,
        line_height_px: 16.0,
        weight: UiFontWeight::Regular,
        color,
        inherit_color: false,
    }
}

pub(super) fn row_icon(row: &HierarchyRow) -> UiIconId {
    // The kind registry owns the icon mapping so queries, badges and rows
    // cannot drift apart.
    row.kind.icon(row.primitive)
}

fn indent_guide(
    palette: StudioUiPalette,
    id: raf_core::scene::SceneNodeId,
    level: usize,
    row_height: f32,
    indent_width: f32,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(
        format!("hierarchy.row.{}.indent.{}", id.0, level),
        UiNodeKind::Panel,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Stretch,
        justify_content: UiJustify::End,
        ..UiLayout::fixed(indent_width.max(8.0), row_height)
    })
    .with_child(
        UiNode::new(
            format!("hierarchy.row.{}.indent.{}.line", id.0, level),
            UiNodeKind::Panel,
        )
        .with_layout(UiLayout::fixed(1.0, row_height))
        .with_style(UiStyle {
            fill: tokens.border,
            border: tokens.border,
            text: tokens.border,
            border_width: 0.0,
            radius: 0.0,
            // Indentation is a rhythm cue, not a divider. The prescribed
            // hierarchy guide alpha keeps nested trees readable without
            // competing with row selection.
            opacity: 0.16,
        }),
    )
}
