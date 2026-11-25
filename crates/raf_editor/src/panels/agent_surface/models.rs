//! Provider and model picker for the Agent surface.

use raf_core::config::EngineSettings;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId,
    UiIconSize, UiLayout, UiNode, UiNodeKind, UiOverflow, UiRect, UiSelect, UiSelectOption,
    UiSizeMode, UiSpacing, UiSurfaceMaterial, UiTextOverflow, UiTextStyle,
};

use crate::panels::ai_chat::AgentPanel;

use super::layout::{selector_widths_with_variant, AgentSurfaceVariant, REVIEW_CHIP_WIDTH};

pub(super) fn build_model_menu(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    settings: &EngineSettings,
    width: f32,
    variant: AgentSurfaceVariant,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let provider = panel
        .effective_provider(settings)
        .map(|config| config.provider)
        .unwrap_or(settings.default_ai_provider);
    let plan_owned =
        settings.agent_openai_use_plan && provider == raf_ai::provider::AiProvider::OpenAI;
    let has_catalog = plan_owned
        || matches!(
            provider,
            raf_ai::provider::AiProvider::OpenRouter
                | raf_ai::provider::AiProvider::OpenAI
                | raf_ai::provider::AiProvider::OpenCode
                | raf_ai::provider::AiProvider::OpenCodeGo
        );
    let (state, items) = crate::panels::ai_model_options::collect_items(settings, provider, "");
    let paint = crate::panels::ai_model_options::ModelRowPaint {
        text: tokens.text,
        muted: tokens.text_muted,
        accent_hot: tokens.accent_hot,
    };
    let actions = crate::panels::ai_model_options::ModelRowActions {
        node_prefix: "agent.model.".to_string(),
        select_key: "agent.model".to_string(),
        pin_command_prefix: "agent.model.pin:".to_string(),
        close_select_id: Some("agent.model.trigger".to_string()),
    };
    let status = crate::panels::ai_model_options::model_status_text(&state, settings.language);
    let pinned_count = items.iter().filter(|item| item.pinned).count();
    let custom_count = items.iter().filter(|item| item.custom).count();
    let plan_count = items
        .iter()
        .filter(|item| item.value.starts_with("account:"))
        .count();
    let catalog_count = items
        .iter()
        .filter(|item| !item.pinned && !item.custom && !item.value.starts_with("account:"))
        .count();
    let custom_rows = if plan_owned { 0 } else { 3 };
    let row_total = usize::from(has_catalog)
        + usize::from(!status.is_empty())
        + items.len()
        + usize::from(pinned_count > 0)
        + usize::from(plan_count > 0)
        + usize::from(catalog_count > 0)
        + usize::from(custom_count > 0)
        + custom_rows;
    let menu_height = (row_total as f32 * 34.0).clamp(68.0, 340.0);
    let (model_width, mode_width) = selector_widths_with_variant(width, variant);
    let right_reserved = if variant == AgentSurfaceVariant::Dock && width >= 560.0 {
        50.0 + REVIEW_CHIP_WIDTH + 8.0
    } else {
        50.0
    };
    let mut menu_layout = UiLayout::absolute(UiRect::new(
        (width - right_reserved - mode_width - 8.0 - model_width).max(8.0),
        48.0,
        model_width,
        menu_height,
    ));
    menu_layout.flow = UiFlow::Column;
    menu_layout.overflow = UiOverflow::ScrollY;
    let mut menu = UiNode::new("agent.model.menu", UiNodeKind::Menu)
        .with_class("agent-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(menu_layout.with_z_index(30))
        .with_accessibility_role(UiAccessibilityRole::Menu);

    if has_catalog {
        menu = menu.with_child(
            UiNode::new("agent.model.catalog.refresh", UiNodeKind::Button)
                .with_class("agent-menu-item-accent")
                .with_layout(UiLayout {
                    padding: UiSpacing::xy(10.0, 8.0),
                    ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_text_key("agent.account.refresh_models")
                .with_text_style(UiTextStyle::button(tokens.accent_hot))
                .with_accessibility_role(UiAccessibilityRole::MenuItem)
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "agent.catalog.refresh",
                )),
        );
    }
    if !status.is_empty() {
        menu = menu.with_child(
            UiNode::new("agent.model.catalog.status", UiNodeKind::Label)
                .with_layout(
                    UiLayout::fixed(0.0, 30.0)
                        .with_width_mode(UiSizeMode::Fill)
                        .padding_xy(10.0, 4.0),
                )
                .with_text_value(status)
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle::button(tokens.text_muted)),
        );
    }

    // Custom id escape hatch first: one field, one confirmation, pins the
    // model. Actions precede the scrollable catalog so the entry stays
    // reachable without scrolling a long model list.
    // Plan requests may only use the selected account's live catalog.
    if !plan_owned {
        menu = menu
            .with_child(
                UiNode::new("agent.model.custom.divider", UiNodeKind::Separator)
                    .with_layout(UiLayout::fixed(0.0, 8.0).with_width_mode(UiSizeMode::Fill)),
            )
            .with_child(model_text_input(
                "agent.new-model.id",
                "app.agent_model_id",
                panel.new_model_id.clone(),
            ))
            .with_child(
                UiNode::new("agent.model.confirm", UiNodeKind::Button)
                    .with_class("agent-menu-item-accent")
                    .with_layout(UiLayout {
                        padding: UiSpacing::xy(10.0, 8.0),
                        ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
                    })
                    .with_text_key("app.agent_add_model_confirm")
                    .with_text_style(UiTextStyle::button(tokens.accent_hot))
                    .with_accessibility_role(UiAccessibilityRole::MenuItem)
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        "agent.model.confirm",
                    )),
            );
    }
    if let Some(error) = panel.new_model_error.as_deref() {
        menu = menu.with_child(
            UiNode::new("agent.model.error", UiNodeKind::Label)
                .with_layout(
                    UiLayout::fixed(0.0, 30.0)
                        .with_width_mode(UiSizeMode::Fill)
                        .padding_xy(10.0, 4.0),
                )
                .with_text_value(error.to_string())
                .with_text_style(UiTextStyle::button(tokens.danger)),
        );
    }

    let mut last_lane = 0u8;
    for item in &items {
        let lane = if item.value.starts_with("account:") {
            3u8
        } else if item.pinned {
            1u8
        } else if item.custom {
            4u8
        } else {
            2u8
        };
        if lane != last_lane {
            let header_key = match lane {
                1 => "ai_models.pinned",
                2 => "ai_models.catalog",
                3 => "ai_models.plan",
                _ => "ai_models.custom",
            };
            menu = menu.with_child(crate::panels::ai_model_options::model_header_row(
                &paint,
                header_key,
                settings.language,
            ));
            last_lane = lane;
        }
        menu = menu.with_child(crate::panels::ai_model_options::model_option_row(
            item,
            &paint,
            &actions,
            "agent-menu-item",
        ));
    }

    // Custom id escape hatch: one field, one confirmation, pins the model.
    menu
}

pub(super) fn model_text_input(id: &str, label_key: &str, value: String) -> UiNode {
    UiNode::new(format!("{id}.field"), UiNodeKind::Panel)
        .with_class("agent-form-field")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::text_input(
                id,
                raf_render::api_graphic_basic::ui_surface::UiTextInput {
                    value_key: id.to_string(),
                    placeholder_key: None,
                    max_length: 256,
                    multiline: false,
                    password: false,
                    submit_command: None,
                },
            )
            .with_class("agent-form-input")
            .focusable()
            .with_layout(UiLayout {
                padding: UiSpacing::xy(9.0, 7.0),
                ..UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_text_value(value),
        )
}

pub(super) fn model_selector_button(
    id: &str,
    value: &str,
    tooltip: &str,
    open: bool,
    width: f32,
) -> UiNode {
    let options = vec![UiSelectOption::new(
        crate::panels::ai_chat::MODEL_SENTINEL,
        crate::panels::ai_chat::MODEL_SENTINEL,
    )];
    agent_select_trigger(
        id,
        "agent.model",
        options,
        0,
        value,
        tooltip,
        open,
        width,
        "agent.model.menu",
        None,
    )
}

pub(super) fn agent_select_trigger(
    id: &str,
    value_key: &str,
    options: Vec<UiSelectOption>,
    selected_index: usize,
    value: &str,
    tooltip: &str,
    open: bool,
    width: f32,
    popup_id: &str,
    extra_class: Option<&str>,
) -> UiNode {
    let mut select = UiSelect::new(value_key, options, selected_index).with_popup_id(popup_id);
    select.open = open;
    let node = UiNode::select(id, select).with_class("agent-selector");
    let node = if let Some(extra_class) = extra_class {
        node.with_class(extra_class)
    } else {
        node
    };
    node.with_layout(UiLayout {
        flow: UiFlow::Row,
        gap: 4.0,
        padding: UiSpacing::xy(8.0, 5.0),
        width_mode: UiSizeMode::Fixed,
        height_mode: UiSizeMode::Fixed,
        basis: [width, 32.0],
        ..UiLayout::default()
    })
    .with_text_value(value.to_string())
    .with_text_overflow(UiTextOverflow::Ellipsis)
    .with_icon(UiIcon::new(UiIconId::ChevronDown).with_size(UiIconSize::Small))
    .with_tooltip_value(tooltip.to_string())
    .with_accessibility_role(UiAccessibilityRole::Combobox)
    .with_accessibility_expanded(open)
    .focusable()
}
