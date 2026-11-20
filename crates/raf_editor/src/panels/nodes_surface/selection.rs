//! Selection-scoped node actions rendered as a standalone context menu.

use super::*;

/// Window size of the node context menu surface. The compositor clamps the
/// stored pointer position against these dimensions.
pub const NODES_CONTEXT_MENU_WIDTH: f32 = 210.0;
pub const NODES_CONTEXT_MENU_HEIGHT: f32 = 148.0;

/// Builds the node context menu as a standalone window-level surface, mirroring
/// the Electronics and bottom-dock tab menus. Returns `None` when the target
/// node no longer exists (for example, deleted while the menu was open).
pub(super) fn build_context_menu(
    palette: StudioUiPalette,
    graph: &NodeGraph,
    node_id: NodeId,
    language: raf_core::Language,
) -> Option<UiSurface> {
    let node = graph.node(node_id)?;
    let tokens = palette.tokens();
    let descriptor = raf_nodes::catalog::descriptor_for_node(node);
    let title_key = descriptor
        .map(|descriptor| descriptor.label_key)
        .unwrap_or("nodes.node.unknown");
    let id = node_id.0.to_string();

    let mut root = UiNode::new("nodes.context-menu", UiNodeKind::Menu)
        .with_class("nodes-context-menu")
        .with_material(raf_ui::UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            padding: UiSpacing::same(6.0),
            ..UiLayout::fit_content()
        })
        .with_child(
            UiNode::new("nodes.context-menu.title", UiNodeKind::Label)
                .with_text_key(title_key)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::PanelTitle,
                    size_px: 12.0,
                    line_height_px: 16.0,
                    weight: UiFontWeight::Bold,
                    color: tokens.text,
                    inherit_color: false,
                })
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    padding: UiSpacing::xy(6.0, 4.0),
                    ..UiLayout::fixed(190.0, 24.0).with_width_mode(UiSizeMode::Fill)
                }),
        );

    let items = [
        (
            "duplicate",
            raf_core::i18n::t("app.duplicate_menu", language),
            UiIconId::Add,
            format!("nodes.duplicate.{id}"),
            tokens.text,
        ),
        (
            "disconnect-all",
            raf_core::i18n::t("nodes.menu.disconnect_all", language),
            UiIconId::Wire,
            format!("nodes.disconnect_all.{id}"),
            tokens.text,
        ),
        (
            "delete",
            raf_core::i18n::t("nodes.delete", language),
            UiIconId::Close,
            format!("nodes.delete.{id}"),
            tokens.danger,
        ),
    ];

    // Visual separator between constructive and destructive actions.
    root = root.with_child(
        UiNode::new("nodes.context-menu.sep", UiNodeKind::Panel)
            .with_layout(UiLayout::fixed(190.0, 1.0).with_width_mode(UiSizeMode::Fill))
            .with_style(UiStyle {
                fill: tokens.border,
                border: [0, 0, 0, 0],
                text: tokens.text,
                border_width: 0.0,
                radius: 0.0,
                opacity: 1.0,
            }),
    );

    for (item_id, label, icon, command, color) in items {
        root = root.with_child(
            UiNode::new(format!("nodes.context-menu.{item_id}"), UiNodeKind::Button)
                .with_class("nodes-context-item")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 8.0,
                    padding: UiSpacing::xy(8.0, 5.0),
                    ..UiLayout::fixed(190.0, 30.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_icon(
                    UiIcon::new(icon)
                        .with_size(UiIconSize::Small)
                        .with_tint(color),
                )
                .with_child(
                    UiNode::new(
                        format!("nodes.context-menu.{item_id}.label"),
                        UiNodeKind::Label,
                    )
                    .with_text_value(label)
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Button,
                        size_px: 12.0,
                        line_height_px: 16.0,
                        weight: UiFontWeight::Bold,
                        color,
                        inherit_color: false,
                    }),
                )
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, command)),
        );
    }

    let mut surface = UiSurface::new("editor.nodes.context-menu", palette, root);
    surface.style_sheet = UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-context-menu".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    text: Some(tokens.text),
                    border_width: Some(1.0),
                    radius: Some(6.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-context-item".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some([0, 0, 0, 0]),
                    text: Some(tokens.text),
                    border_width: Some(0.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-context-item".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        ],
    };
    Some(surface)
}
