//! Agent mode selection for the Agent surface.

use raf_core::ai::AgentMode;
use raf_core::config::EngineSettings;
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAction, UiEventBinding, UiEventKind, UiFlow, UiLayout,
    UiNode, UiNodeKind, UiOverflow, UiRect, UiSelectOption, UiSizeMode, UiSpacing,
    UiSurfaceMaterial, UiTextStyle,
};

use super::layout::{selector_widths_with_variant, AgentSurfaceVariant, REVIEW_CHIP_WIDTH};
use super::models::agent_select_trigger;

pub(super) fn build_mode_menu(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    width: f32,
    variant: AgentSurfaceVariant,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let (_, mode_width) = selector_widths_with_variant(width, variant);
    let right_reserved = if variant == AgentSurfaceVariant::Dock && width >= 560.0 {
        50.0 + REVIEW_CHIP_WIDTH + 8.0
    } else {
        50.0
    };
    let mut menu_layout = UiLayout::absolute(UiRect::new(
        (width - right_reserved - mode_width).max(8.0),
        48.0,
        mode_width,
        102.0,
    ));
    menu_layout.flow = UiFlow::Column;
    menu_layout.overflow = UiOverflow::Clip;
    UiNode::new("agent.mode.menu", UiNodeKind::Menu)
        .with_class("agent-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(menu_layout.with_z_index(30))
        .with_accessibility_role(UiAccessibilityRole::Menu)
        .with_child(mode_option(
            tokens,
            "app.agent_mode_inspect",
            "inspect",
            0,
            settings.agent_mode == AgentMode::Inspect,
        ))
        .with_child(mode_option(
            tokens,
            "app.agent_mode_plan",
            "plan",
            1,
            settings.agent_mode == AgentMode::Plan,
        ))
        .with_child(mode_option(
            tokens,
            "app.agent_mode_active",
            "active",
            2,
            settings.agent_mode == AgentMode::Active,
        ))
}

pub(super) fn mode_option(
    tokens: raf_render::api_graphic_basic::ui_surface::UiTokens,
    label_key: &str,
    value: &str,
    index: usize,
    selected: bool,
) -> UiNode {
    UiNode::new(format!("agent.mode.option.{value}"), UiNodeKind::Button)
        .with_class("agent-menu-item")
        .with_layout(UiLayout {
            padding: UiSpacing::xy(10.0, 8.0),
            ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_accessibility_role(UiAccessibilityRole::Option)
        .with_accessibility_selected(selected)
        .focusable()
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action: UiAction::SetSelect {
                key: "agent.mode".to_string(),
                value: value.to_string(),
                index,
            },
        })
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action: UiAction::SetSelectOpen {
                id: "agent.mode.trigger".to_string(),
                open: false,
            },
        })
}

pub(super) fn mode_selector_button(
    id: &str,
    settings: &EngineSettings,
    mode_key: &str,
    open: bool,
    width: f32,
) -> UiNode {
    let options = vec![
        UiSelectOption::new("inspect", "app.agent_mode_inspect"),
        UiSelectOption::new("plan", "app.agent_mode_plan"),
        UiSelectOption::new("active", "app.agent_mode_active"),
    ];
    let (selected_index, mode_class) = match settings.agent_mode {
        AgentMode::Inspect => (0, "agent-mode-inspect"),
        AgentMode::Plan => (1, "agent-mode-plan"),
        AgentMode::Active => (2, "agent-mode-active"),
    };
    let value = t(mode_key, settings.language);
    let tooltip = t("app.agent_mode", settings.language);
    agent_select_trigger(
        id,
        "agent.mode",
        options,
        selected_index,
        &value,
        &tooltip,
        open,
        width,
        "agent.mode.menu",
        Some(mode_class),
    )
}
