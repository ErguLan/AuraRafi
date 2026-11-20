//! Hub dialog primitives, adoption/open-error overlays and the project opening card.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiJustify, UiLayout, UiNode,
    UiNodeKind, UiSizeMode, UiSpacing, UiStyle, UiTextStyle,
};

use super::*;

pub(crate) fn hub_dialog_overlay(model: &HubSurfaceModel, id: &str, card: UiNode) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(
            UiLayout {
                flow: UiFlow::Column,
                align_items: UiAlign::Center,
                justify_content: UiJustify::Center,
                ..UiLayout::absolute(raf_ui::UiRect::new(
                    0.0,
                    0.0,
                    model.window_size[0],
                    model.window_size[1],
                ))
            }
            .with_z_index(110),
        )
        .with_style(UiStyle {
            fill: [0, 0, 0, 180],
            ..UiStyle::transparent()
        })
        .with_child(card)
}

pub(crate) fn hub_dialog_card(palette: StudioUiPalette, id: &str, width: f32) -> UiNode {
    let tokens = hub_tokens(palette);
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 12.0,
            padding: UiSpacing::same(24.0),
            basis: [width, 0.0],
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fixed)
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            border: tokens.border,
            border_width: 1.0,
            radius: 10.0,
            ..UiStyle::transparent()
        })
        .with_accessibility_role(raf_ui::UiAccessibilityRole::Dialog)
}

pub(crate) fn hub_dialog_primary_button(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    command: &str,
    width: f32,
) -> UiNode {
    let tokens = hub_tokens(palette);
    UiNode::new(id, UiNodeKind::Button)
        .with_class("hub-primary-button")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            ..UiLayout::fixed(width, 32.0)
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::button([18, 18, 20, 255]))
                .with_layout(UiLayout::fit_content()),
        )
        .with_style(UiStyle {
            fill: tokens.accent,
            border: tokens.accent_hot,
            text: [18, 18, 20, 255],
            border_width: 1.0,
            radius: 4.0,
            opacity: 1.0,
        })
}

pub(crate) fn hub_adopt_overlay(
    palette: StudioUiPalette,
    model: &HubSurfaceModel,
    adopt: &HubPendingAdopt,
) -> UiNode {
    let tokens = hub_tokens(palette);
    let card = hub_dialog_card(palette, "hub.adopt.card", 540.0)
        .with_accessibility_label_key("app.hub_adopt_title")
        .with_child(
            UiNode::new("hub.adopt.title", UiNodeKind::Label)
                .with_text_key("app.hub_adopt_title")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("hub.adopt.path", UiNodeKind::Label)
                .with_text_value(adopt.path.display().to_string())
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_text_overflow(raf_ui::UiTextOverflow::Wrap)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new("hub.adopt.type", UiNodeKind::Label)
                .with_text_key("hub.adopt.type")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("hub.adopt.note", UiNodeKind::Label)
                .with_text_key("app.hub_adopt_note")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_text_overflow(raf_ui::UiTextOverflow::Wrap)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new("hub.adopt.actions", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    justify_content: UiJustify::End,
                    gap: 8.0,
                    ..UiLayout::fit_content()
                })
                .with_child(hub_command_button(
                    "hub.adopt.cancel",
                    "app.cancel",
                    "hub.adopt.cancel",
                    120.0,
                    32.0,
                    "hub-project-menu",
                ))
                .with_child(hub_dialog_primary_button(
                    palette,
                    "hub.adopt.confirm",
                    "app.hub_adopt_confirm",
                    "hub.adopt.confirm",
                    170.0,
                )),
        );
    hub_dialog_overlay(model, "hub.adopt.overlay", card)
}

pub(crate) fn hub_open_error_overlay(
    palette: StudioUiPalette,
    model: &HubSurfaceModel,
    error: &str,
) -> UiNode {
    let tokens = hub_tokens(palette);
    let card = hub_dialog_card(palette, "hub.open-error.card", 520.0)
        .with_accessibility_label_key("app.hub_open_error_title")
        .with_child(
            UiNode::new("hub.open-error.title", UiNodeKind::Label)
                .with_text_key("app.hub_open_error_title")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("hub.open-error.detail", UiNodeKind::Label)
                .with_text_value(error.to_string())
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_text_overflow(raf_ui::UiTextOverflow::Wrap)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new("hub.open-error.actions", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    justify_content: UiJustify::End,
                    ..UiLayout::fit_content()
                })
                .with_child(hub_command_button(
                    "hub.open-error.close",
                    "app.hub_close",
                    "hub.open-error.close",
                    110.0,
                    32.0,
                    "hub-project-menu",
                )),
        );
    hub_dialog_overlay(model, "hub.open-error.overlay", card)
}

pub(crate) fn hub_project_opening(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    let tokens = hub_tokens(palette);
    let failed = model.opening_error.is_some();
    let mut card = UiNode::new("hub.open.card", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 14.0,
            padding: UiSpacing::same(24.0),
            basis: [440.0, 0.0],
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fixed)
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            border: tokens.border,
            border_width: 1.0,
            radius: 10.0,
            ..UiStyle::transparent()
        })
        .with_accessibility_role(raf_ui::UiAccessibilityRole::Dialog)
        .with_accessibility_label_key(if failed {
            "agent_workspace.load_failed"
        } else {
            "agent_workspace.loading"
        });
    if !failed {
        card = card.with_child(crate::agent_workspace_surface::loading_ring(
            palette,
            model.opening_step,
        ));
    }
    card = card
        .with_child(
            UiNode::new("hub.open.title", UiNodeKind::Label)
                .with_text_key(if failed {
                    "agent_workspace.load_failed"
                } else {
                    "agent_workspace.loading"
                })
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("hub.open.project", UiNodeKind::Label)
                .with_text_value(model.opening_project.clone().unwrap_or_default())
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    if failed {
        card = card.with_child(
            UiNode::new("hub.open.error", UiNodeKind::Label)
                .with_text_key("hub.open.error")
                .with_text_overflow(raf_ui::UiTextOverflow::Wrap)
                .with_text_style(UiTextStyle::body(tokens.warning))
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    let mut actions = UiNode::new("hub.open.actions", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 8.0,
            ..UiLayout::fit_content()
        })
        .with_child(hub_command_button(
            "hub.open.cancel",
            "app.cancel",
            "hub.open.cancel",
            110.0,
            32.0,
            "hub-project-menu",
        ));
    if failed {
        actions = actions.with_child(hub_command_button(
            "hub.open.retry",
            "agent_workspace.retry",
            "hub.open.retry",
            110.0,
            32.0,
            "hub-secondary-button",
        ));
    }
    UiNode::new("hub.open.overlay", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            ..UiLayout::absolute(raf_ui::UiRect::new(
                0.0,
                0.0,
                model.opening_size[0],
                model.opening_size[1],
            ))
            .with_z_index(120)
        })
        .with_style(UiStyle {
            fill: [0, 0, 0, 180],
            ..UiStyle::transparent()
        })
        .with_child(card.with_child(actions))
}
