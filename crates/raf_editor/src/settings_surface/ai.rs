//! AI provider, model, credential and agent preference settings.

use raf_core::ai::{AgentMode, AiProvider};
use raf_core::config::EngineSettings;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow,
    UiLayout, UiNode, UiNodeKind, UiScrollAxis, UiSelectOption, UiSizeMode, UiSpacing, UiStyle,
    UiSurfaceMaterial, UiTextInput, UiTextOverflow, UiTextStyle,
};
use raf_ui::UiRect;

use super::controls::{
    card, command_button, group_label, range_row_disabled_with_description, row, select_row,
    select_row_with_description, toggle_row, toggle_row_with_description, toggle_row_with_key,
};
use super::{CONTROL_WIDTH, SETTINGS_CONTROL_MIN_WIDTH};

pub(super) fn ai(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    open_select: Option<&str>,
    revealed_api_keys: &[AiProvider],
    model_filter: &str,
) -> UiNode {
    let tokens = palette.tokens();
    let provider_options = settings
        .ai_providers
        .iter()
        .map(|config| {
            UiSelectOption::new(
                provider_id(config.provider),
                format!("settings.value.{}", provider_id(config.provider)),
            )
        })
        .collect::<Vec<_>>();
    let default_provider_index = settings
        .ai_providers
        .iter()
        .position(|config| config.provider == settings.default_ai_provider)
        .unwrap_or(0);
    let mut root = card(palette, "settings.ai_providers")
        .with_child(group_label(palette, "settings.group.providers"))
        .with_child(toggle_row_with_description(
            palette,
            "settings.ai-persist-credentials",
            "settings.ai_persist_credentials",
            settings.ai_persist_credentials,
            "settings.ai_persist_credentials_desc",
        ))
        .with_child(select_row_with_description(
            palette,
            "settings.ai-provider-default",
            "settings.ai_provider_default",
            "settings.default_ai_provider",
            provider_options,
            default_provider_index,
            open_select,
            "settings.ai_provider_default_desc",
        ));

    // One focused provider editor instead of five credential forms at once.
    // The selector above is also the switch to another provider's editor.
    for config in settings
        .ai_providers
        .iter()
        .filter(|config| config.provider == settings.default_ai_provider)
    {
        let provider = config.provider;
        let id = provider_id(provider);
        let card_id = format!("settings.ai-provider.{id}");
        let has_catalog = matches!(
            provider,
            AiProvider::OpenRouter | AiProvider::OpenAI | AiProvider::OpenCode | AiProvider::OpenCodeGo
        );
        root = root.with_child(
            UiNode::new(card_id.clone(), UiNodeKind::Panel)
                .with_class("settings-ai-provider-card")
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 6.0,
                    padding: UiSpacing::same(10.0),
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new(format!("{card_id}.title"), UiNodeKind::Label)
                        .with_text_value(provider.display_name())
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout::fit_content()),
                )
                .with_child(
                    UiNode::new(format!("{card_id}.status"), UiNodeKind::Label)
                        .with_text_key(
                            if provider == AiProvider::OpenAI && settings.agent_openai_use_plan {
                                "settings.account.plan_selected"
                            } else if !matches!(
                                provider,
                                AiProvider::OpenAI | AiProvider::OpenRouter | AiProvider::OpenCode | AiProvider::OpenCodeGo | AiProvider::Puerto
                            ) {
                                "settings.ai_provider_status_unsupported"
                            } else if !config.enabled {
                                "settings.ai_provider_status_disabled"
                            } else if config.base_url.trim().is_empty()
                                || config.model.trim().is_empty()
                                || (provider != AiProvider::Puerto
                                    && config.api_key.trim().is_empty())
                            {
                                "settings.ai_provider_status_incomplete"
                            } else {
                                "settings.ai_provider_status_configured"
                            },
                        )
                        .with_text_overflow(UiTextOverflow::Wrap)
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
                )
                .with_child(toggle_row_with_key(
                    palette,
                    format!("{card_id}.enabled"),
                    "settings.ai_provider_enabled",
                    format!("settings.ai_provider.{id}"),
                    config.enabled,
                ))
                .with_child(ai_model_picker_row(
                    palette,
                    settings,
                    &card_id,
                    config,
                    has_catalog,
                    open_select,
                    model_filter,
                ))
                .with_child(if provider == AiProvider::OpenAI && settings.agent_openai_use_plan {
                    UiNode::new(format!("{card_id}.plan-note"), UiNodeKind::Label)
                        .with_text_key("ai_models.plan_only")
                        .with_text_overflow(UiTextOverflow::Wrap)
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill))
                } else {
                    ai_api_credentials(palette, config, revealed_api_keys)
                }),
        );
    }

    root = root
        .with_child(group_label(palette, "settings.external_studio.title"))
        .with_child(toggle_row_with_description(
            palette,
            "settings.external-studio.enabled",
            "settings.external_studio.enabled",
            settings.external_asset_studio.enabled,
            "settings.external_studio.description",
        ));
    for field in ["repository_url", "directory", "program", "entry_point"] {
        if !settings.external_asset_studio.enabled {
            break;
        }
        root = root.with_child(ai_provider_text_row(
            palette,
            &format!("settings.external-studio.{field}"),
            &format!("settings.external_studio.{field}"),
            &format!("settings.external_studio.{field}"),
            false,
            None,
            None,
        ));
    }
    root = root
        .with_child(group_label(palette, "settings.group.agent"))
        .with_child(select_row(
            palette,
            "settings.agent-mode",
            "settings.agent_mode",
            "settings.agent_mode",
            vec![
                UiSelectOption::new("inspect", "settings.agent_mode_inspect"),
                UiSelectOption::new("plan", "settings.agent_mode_plan"),
                UiSelectOption::new("active", "settings.agent_mode_active"),
            ],
            match settings.agent_mode {
                AgentMode::Inspect => 0,
                AgentMode::Plan => 1,
                AgentMode::Active => 2,
            },
            open_select,
        ))
        .with_child(
            UiNode::new("settings.agent-mode.help", UiNodeKind::Label)
                .with_text_key("settings.agent_mode_desc")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(toggle_row(
            palette,
            "settings.agent-streaming",
            "settings.agent_streaming_enabled",
            settings.agent_streaming_enabled,
        ))
        .with_child(toggle_row_with_description(
            palette,
            "settings.agent-tool-call-limit",
            "settings.agent_tool_call_limit_enabled",
            settings.agent_tool_call_limit_enabled,
            "settings.agent_tool_call_limit_enabled_desc",
        ))
        .with_child(if settings.agent_tool_call_limit_enabled {
            range_row_disabled_with_description(
                palette,
                "settings.agent-max-tool-calls",
                "settings.agent_max_tool_calls",
                settings.agent_max_tool_calls as f32,
                raf_core::config::AGENT_MAX_TOOL_CALLS_MIN as f32,
                raf_core::config::AGENT_MAX_TOOL_CALLS_MAX as f32,
                1.0,
                settings.agent_max_tool_calls.to_string(),
                !settings.agent_tool_call_limit_enabled,
                "settings.agent_max_tool_calls_desc",
            )
        } else {
            UiNode::new("settings.agent-max-tool-calls.hidden", UiNodeKind::Panel)
                .with_style(UiStyle::transparent())
                .with_layout(UiLayout::fixed(0.0, 0.0))
        })
        .with_child(toggle_row_with_description(
            palette,
            "settings.agent-response-limit",
            "settings.agent_response_limit_enabled",
            settings.agent_response_limit_enabled,
            "settings.agent_response_limit_enabled_desc",
        ))
        .with_child(if settings.agent_response_limit_enabled {
            range_row_disabled_with_description(
                palette,
                "settings.agent-max-response-tokens",
                "settings.agent_max_response_tokens",
                settings.agent_max_response_tokens as f32,
                raf_core::config::AGENT_MAX_RESPONSE_TOKENS_MIN as f32,
                raf_core::config::AGENT_MAX_RESPONSE_TOKENS_MAX as f32,
                256.0,
                settings.agent_max_response_tokens.to_string(),
                !settings.agent_response_limit_enabled,
                "settings.agent_response_limit_enabled_desc",
            )
        } else {
            UiNode::new(
                "settings.agent-max-response-tokens.hidden",
                UiNodeKind::Panel,
            )
            .with_style(UiStyle::transparent())
            .with_layout(UiLayout::fixed(0.0, 0.0))
        })
        .with_child(select_row_with_description(
            palette,
            "settings.agent-reasoning",
            "settings.agent_reasoning_effort",
            "settings.agent_reasoning_effort",
            vec![
                UiSelectOption::new("", "settings.reasoning.auto"),
                UiSelectOption::new("none", "settings.reasoning.none"),
                UiSelectOption::new("low", "settings.reasoning.low"),
                UiSelectOption::new("medium", "settings.reasoning.medium"),
                UiSelectOption::new("high", "settings.reasoning.high"),
                UiSelectOption::new("max", "settings.reasoning.max"),
            ],
            ["", "none", "low", "medium", "high", "max"]
                .iter()
                .position(|v| *v == settings.agent_reasoning_effort)
                .unwrap_or(0),
            open_select,
            "settings.agent_reasoning_effort_desc",
        ))
        .with_child(toggle_row_with_description(
            palette,
            "settings.agent-show-reasoning",
            "settings.agent_show_reasoning",
            settings.agent_show_reasoning,
            "settings.agent_show_reasoning_desc",
        ));
    root
}

/// The provider card model picker: a trigger with the active model id and a
/// searchable popup built from the shared picker data. Сustom ids stay in the
/// advanced text row beneath it.
fn ai_api_credentials(palette: StudioUiPalette, config: &raf_core::ai::AiProviderConfig, revealed: &[AiProvider]) -> UiNode {
    let provider = config.provider;
    let id = provider_id(provider);
    let card = format!("settings.ai-provider.{id}");
    let mut root = UiNode::new(format!("{card}.api-fields"), UiNodeKind::Panel)
        .with_layout(UiLayout { flow: UiFlow::Column, gap: 6.0, ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill) })
        .with_child(group_label(palette, "settings.ai_advanced"));
    for (field, key) in [("base-url", "base_url"), ("custom-model", "model"), ("api-key", "api_key")] {
        let secret = key == "api_key";
        root = root.with_child(ai_provider_text_row(palette, &format!("{card}.{field}"),
            &format!("settings.ai_provider_{key}"), &format!("settings.ai_provider.{id}.{key}"),
            secret && !revealed.contains(&provider),
            secret.then(|| format!("settings.ai_provider.reveal.{id}")),
            secret.then(|| format!("settings.ai_provider.clear.{id}"))));
    }
    root
}

fn ai_model_picker_row(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    card_id: &str,
    config: &raf_core::ai::AiProviderConfig,
    has_catalog: bool,
    open_select: Option<&str>,
    model_filter: &str,
) -> UiNode {
    let tokens = palette.tokens();
    let language = settings.language;
    let trigger_id = format!("{card_id}.model.trigger");
    let is_open = open_select == Some(trigger_id.as_str());
    let paint = crate::panels::ai_model_options::ModelRowPaint {
        text: tokens.text,
        muted: tokens.text_muted,
        accent_hot: tokens.accent_hot,
    };
    let actions = crate::panels::ai_model_options::ModelRowActions {
        node_prefix: "settings.ai-model.".to_string(),
        select_key: "settings.ai.model".to_string(),
        pin_command_prefix: "settings.ai_model.pin:".to_string(),
        close_select_id: Some(trigger_id.clone()),
    };
    let current = config.model.trim().to_string();
    if !is_open {
        let mut trigger = UiNode::new(trigger_id.clone(), UiNodeKind::Button)
            .with_class("settings-select-trigger settings-model-trigger")
            .with_layout(UiLayout {
                grow: 1.0,
                min_size: [SETTINGS_CONTROL_MIN_WIDTH, 32.0],
                max_size: [CONTROL_WIDTH, 32.0],
                ..UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_icon(raf_ui::UiIcon::new(raf_ui::UiIconId::ChevronDown).with_size(raf_ui::UiIconSize::Small))
            .with_accessibility_label_key("settings.ai_provider_model")
            .with_accessibility_role(raf_ui::UiAccessibilityRole::Combobox)
            .with_accessibility_expanded(false)
            .focusable()
            .with_event(UiEventBinding {
                event: UiEventKind::Click,
                action: raf_ui::UiAction::SetSelectOpen {
                    id: trigger_id.clone(),
                    open: true,
                },
            });
        if current.is_empty() {
            trigger = trigger.with_text_key("settings.ai_provider_model_unset");
        } else {
            trigger = trigger.with_text_value(current.clone());
        }
        trigger = trigger.with_text_overflow(UiTextOverflow::Ellipsis);
        return row(palette, format!("{card_id}.model"), "settings.ai_provider_model".to_string(), None, trigger);
    }

    let (state, items) =
        crate::panels::ai_model_options::collect_items(settings, config.provider, model_filter);
    let mut rows = Vec::new();
    if has_catalog {
        rows.push(
            UiNode::new(format!("{card_id}.model.refresh"), UiNodeKind::Button)
                .with_class("settings-select-option settings-model-refresh")
                .with_layout(
                    UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill),
                )
                .with_text_key("settings.ai_provider_refresh_models")
                .with_text_style(UiTextStyle::button(tokens.accent_hot))
                .with_accessibility_role(raf_ui::UiAccessibilityRole::MenuItem)
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("settings.ai_provider.refresh_catalog.{}", provider_id(config.provider)),
                )),
        );
        let status = crate::panels::ai_model_options::model_status_text(&state, language);
        if !status.is_empty() {
            rows.push(
                UiNode::new(format!("{card_id}.model.status"), UiNodeKind::Label)
                    .with_layout(UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill).padding_xy(10.0, 4.0))
                    .with_text_value(status)
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_text_style(UiTextStyle::button(tokens.text_muted)),
            );
        }
    }
    rows.push(
        UiNode::text_input(
            format!("{card_id}.model.filter"),
            raf_ui::UiTextInput {
                value_key: "settings.ai.model.filter".to_string(),
                placeholder_key: Some("ai_models.search_placeholder".to_string()),
                max_length: 256,
                multiline: false,
                password: false,
                submit_command: None,
            },
        )
        .with_class("settings-text-input")
        .with_accessibility_label_key("ai_models.search_placeholder")
        .with_layout(UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)),
    );
    let mut last_lane = 0u8;
    if items.is_empty() {
        rows.push(UiNode::new(format!("{card_id}.model.empty"), UiNodeKind::Label)
            .with_text_key("ai_models.empty")
            .with_text_overflow(UiTextOverflow::Wrap)
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_layout(UiLayout::fixed(0.0, 48.0).with_width_mode(UiSizeMode::Fill)));
    }
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
            rows.push(crate::panels::ai_model_options::model_header_row(
                &paint,
                header_key,
                language,
            ));
            last_lane = lane;
        }
        rows.push(crate::panels::ai_model_options::model_option_row(
            item,
            &paint,
            &actions,
            "settings-select-option",
        ));
    }
    let menu_height = (rows.len() as f32 * 30.0).max(60.0).min(340.0);
    let mut menu = UiNode::new(format!("{card_id}.model.menu"), UiNodeKind::Menu)
        .with_class("settings-select-menu settings-model-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            ..UiLayout::absolute(UiRect::new(0.0, 36.0, CONTROL_WIDTH, menu_height))
                .with_z_index(30)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 6.0,
            opacity: 1.0,
        });
    let mut scroll = UiNode::scroll_view(format!("{card_id}.model.scroll"), UiScrollAxis::Vertical)
        .with_layout(UiLayout::fill(UiFlow::Column));
    for row_node in rows { scroll = scroll.with_child(row_node); }
    menu = menu.with_child(scroll);
    let trigger = UiNode::new(trigger_id.clone(), UiNodeKind::Button)
        .with_class("settings-select-trigger settings-model-trigger")
        .with_layout(UiLayout {
            grow: 1.0,
            min_size: [SETTINGS_CONTROL_MIN_WIDTH, 32.0],
            max_size: [CONTROL_WIDTH, 32.0],
            ..UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_icon(raf_ui::UiIcon::new(raf_ui::UiIconId::ChevronDown).with_size(raf_ui::UiIconSize::Small))
        .with_accessibility_label_key("settings.ai_provider_model")
        .with_accessibility_role(raf_ui::UiAccessibilityRole::Combobox)
        .with_accessibility_expanded(true)
        .focusable()
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action: raf_ui::UiAction::SetSelectOpen {
                id: trigger_id.clone(),
                open: false,
            },
        });
    let trigger = if current.is_empty() {
        trigger.with_text_key("settings.ai_provider_model_unset")
    } else {
        trigger.with_text_value(current)
    };
    let trigger = trigger.with_text_overflow(UiTextOverflow::Ellipsis);
    let control = UiNode::new(format!("{card_id}.model.wrap"), UiNodeKind::Panel)
        .with_class("settings-select-wrap")
        .with_layout(UiLayout {
            grow: 1.0,
            min_size: [SETTINGS_CONTROL_MIN_WIDTH, 32.0],
            max_size: [CONTROL_WIDTH, 32.0],
            ..UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(trigger)
        .with_child(menu);
    row(
        palette,
        format!("{card_id}.model"),
        "settings.ai_provider_model".to_string(),
        None,
        control,
    )
}

/// Account data are supplied by the host; credentials stay in the provider layer.
pub(crate) fn account_controls(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    view: &raf_ai::chatgpt_auth::AuthView,
    _open_select: Option<&str>,
) -> UiNode {
    let tokens = palette.tokens();
    let mut root = UiNode::new("settings.account.controls", UiNodeKind::Panel)
        .with_layout(UiLayout { flow: UiFlow::Column, gap: 6.0, ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill) })
        .with_child(toggle_row_with_description(palette, "settings.account.plan", "settings.agent_openai_use_plan",
            settings.agent_openai_use_plan, "settings.account.plan_description"));
    let active = view.accounts.iter().any(|account| account.active && account.connected);
    root = root.with_child(UiNode::new("settings.account.status", UiNodeKind::Label)
        .with_text_key(if view.busy { "settings.account.connecting" } else if active { "settings.account.connected" } else { "settings.account.disconnected" })
        .with_text_style(UiTextStyle::body(tokens.text_muted))
        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)));
    for (index, account) in view.accounts.iter().enumerate() {
        let label = format!("{} · {}", account.label, index + 1);
        root = root.with_child(UiNode::new(format!("settings.account.profile.{index}"), UiNodeKind::Toolbar)
            .with_layout(UiLayout { flow: UiFlow::Row, align_items: UiAlign::Center, gap: 6.0,
                ..UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill) })
            .with_child(UiNode::new(format!("settings.account.profile.{index}.label"), UiNodeKind::Label)
                .with_text_value(label).with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle::body(if account.active { tokens.accent } else { tokens.text_muted }))
                .with_layout(UiLayout { grow: 1.0, ..UiLayout::fit_content() }))
            .with_child(command_button(palette, &format!("settings.account.profile.{index}.action"),
                if account.connected { "settings.account.select" } else { "settings.account.reconnect" },
                &format!("settings.account.{}:{index}", if account.connected { "select" } else { "reconnect" }),
                "settings-secondary-button", 105.0).disabled(view.busy)));
    }
    root = root.with_child(UiNode::new("settings.account.actions", UiNodeKind::Toolbar)
        .with_layout(UiLayout { flow: UiFlow::RowWrap, gap: 6.0, ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill) })
        .with_child(command_button(palette, "settings.account.connect", "settings.account.connect",
            "settings.account.connect", "settings-secondary-button", 184.0).disabled(view.busy))
        .with_child(command_button(palette, "settings.account.refresh", "settings.account.refresh",
            "settings.account.refresh", "settings-secondary-button", 124.0).disabled(view.busy || !active))
        .with_child(command_button(palette, "settings.account.signout", "settings.account.signout",
            "settings.account.signout", "settings-secondary-button", 124.0).disabled(view.busy || !active))
        .with_child(command_button(palette, "settings.account.usage", "settings.account.usage",
            "settings.account.usage", "settings-secondary-button", 135.0)));
    if let Some(message) = &view.message {
        root = root.with_child(UiNode::new("settings.account.error", UiNodeKind::Label)
            .with_text_key("settings.account.error_summary").with_tooltip_value(message.clone()).with_text_overflow(UiTextOverflow::Wrap)
            .with_text_style(UiTextStyle::body(tokens.warning))
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)));
    }
    root
}

pub(crate) fn account_welcome(palette: StudioUiPalette, rect: UiRect) -> UiNode {
    let tokens = palette.tokens();
    let width = 430.0_f32.min((rect.width - 32.0).max(1.0));
    let height = 170.0_f32.min((rect.height - 32.0).max(1.0));
    let mut layout = UiLayout::absolute(UiRect::new(rect.x + (rect.width - width) * 0.5,
        rect.y + (rect.height - height) * 0.5, width, height));
    layout.flow = UiFlow::Column;
    layout.gap = 10.0;
    layout.padding = UiSpacing::same(18.0);
    UiNode::new("settings.account.welcome", UiNodeKind::Overlay)
        .with_class("settings-ai-provider-card")
        .with_material(UiSurfaceMaterial::ModalSurface)
        .with_layout(layout.with_z_index(120))
        .with_accessibility_role(UiAccessibilityRole::Dialog)
        .with_accessibility_label_key("settings.account.welcome_title")
        .with_child(UiNode::new("settings.account.welcome.title", UiNodeKind::Label)
            .with_text_key("settings.account.welcome_title").with_text_style(UiTextStyle::panel_title(tokens.text))
            .with_layout(UiLayout::fit_content()))
        .with_child(UiNode::new("settings.account.welcome.body", UiNodeKind::Label)
            .with_text_key("settings.account.welcome_body").with_text_overflow(UiTextOverflow::Wrap)
            .with_text_style(UiTextStyle::body(tokens.text)).with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)))
        .with_child(command_button(palette, "settings.account.welcome.ok", "settings.account.welcome_ok",
            "settings.account.welcome_ack", "settings-primary-button", 112.0))
}

fn ai_provider_text_row(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    value_key: &str,
    password: bool,
    reveal_command: Option<String>,
    clear_command: Option<String>,
) -> UiNode {
    let button_width = match (reveal_command.is_some(), clear_command.is_some()) {
        (true, true) => 113.0,
        (true, false) | (false, true) => 58.0,
        (false, false) => 0.0,
    };
    let mut control = UiNode::new(format!("{id}.control"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 5.0,
            grow: 1.0,
            min_size: [SETTINGS_CONTROL_MIN_WIDTH + button_width, 30.0],
            max_size: [CONTROL_WIDTH + button_width, 30.0],
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::text_input(
                format!("{id}.input"),
                UiTextInput {
                    value_key: value_key.to_string(),
                    placeholder_key: Some("settings.surface.input".to_string()),
                    max_length: 512,
                    multiline: false,
                    password,
                    submit_command: None,
                },
            )
            .with_class("settings-text-input")
            .with_accessibility_label_key(label_key)
            .with_layout(UiLayout {
                grow: 1.0,
                min_size: [SETTINGS_CONTROL_MIN_WIDTH, 30.0],
                ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
            }),
        );
    if let Some(command) = reveal_command {
        control = control.with_child(command_button(
            palette,
            &format!("{id}.reveal"),
            if password {
                "settings.ai_provider_show"
            } else {
                "settings.ai_provider_hide"
            },
            &command,
            "settings-secondary-button",
            54.0,
        ));
    }
    if let Some(command) = clear_command {
        control = control.with_child(command_button(
            palette,
            &format!("{id}.clear"),
            "settings.ai_provider_clear",
            &command,
            "settings-secondary-button",
            54.0,
        ));
    }
    row(
        palette,
        id.to_string(),
        label_key.to_string(),
        None,
        control,
    )
}

pub(crate) fn provider_id(provider: AiProvider) -> &'static str {
    match provider {
        AiProvider::Puerto => "puerto",
        AiProvider::OpenRouter => "openrouter",
        AiProvider::OpenAI => "openai",
        AiProvider::OpenCode => "opencode",
        AiProvider::OpenCodeGo => "opencode_go",
        AiProvider::GenAI => "genai",
        AiProvider::Claude => "claude",
    }
}
