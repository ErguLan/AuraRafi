//! RafUI application bar composition.
//!
//! This is the first native-shell surface: menus and settings entry points
//! are presented here, while the host translates commands to the application.
//! Build, Play and runtime-status controls are intentionally absent for now.

use raf_core::project::ProjectType;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId, UiIconSize, UiImage,
    UiImageFit, UiImageSource, UiLayout, UiNode, UiNodeKind, UiSizeMode, UiSpacing, UiStyle,
    UiSurface, UiTextInput, UiTextStyle,
};
use raf_ui::{UiMenu, UiMenuCommand, UiMenuItem};

const ACCENT: [u8; 4] = [232, 133, 28, 255];
pub const APPLICATION_BAR_HEIGHT: f32 = 34.0;
pub const APPLICATION_MENU_POPUP_WIDTH: f32 = 238.0;
const APPLICATION_MENU_ROW_HEIGHT: f32 = 30.0;
const APPLICATION_MENU_SEPARATOR_HEIGHT: f32 = 8.0;
const APPLICATION_MENU_GAP: f32 = 2.0;
const APPLICATION_MENU_PADDING: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentBarStatus {
    Ready,
    Thinking,
    Approval,
    Error,
}

pub fn build_application_bar_surface(
    palette: StudioUiPalette,
    project_name: &str,
    project_type: ProjectType,
    open_menu: Option<&str>,
    agent_status: AgentBarStatus,
) -> UiSurface {
    let tokens = palette.tokens();
    let context_key = match project_type {
        ProjectType::Game => "app.scene_view",
        ProjectType::Electronics => "app.schematic_view",
    };
    let root = UiNode::new("application-bar.root", UiNodeKind::Toolbar)
        .with_class("application-bar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: raf_ui::UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(8.0, 0.0),
            overflow: raf_ui::UiOverflow::Clip,
            ..UiLayout::fixed(0.0, APPLICATION_BAR_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle {
            fill: tokens.surface_alt,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 0.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::image(
                "application-bar.logo",
                UiImage {
                    source: UiImageSource::new("editor.top.logo"),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(28.0, 28.0)),
        )
        .with_child(
            UiNode::new("application-bar.product", UiNodeKind::Label)
                .with_text_key("app.product_name")
                .with_text_style(UiTextStyle::panel_title(ACCENT))
                .with_layout(UiLayout::fixed(30.0, 20.0)),
        )
        .with_child(
            UiNode::new(
                "application-bar.product-project-separator",
                UiNodeKind::Label,
            )
            .with_text_key("|")
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_layout(UiLayout::fixed(8.0, 20.0)),
        )
        .with_child(
            UiNode::new("application-bar.project", UiNodeKind::Label)
                .with_text_key(project_name.to_string())
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout::fixed(148.0, 20.0)),
        )
        .with_child(separator(
            "application-bar.project-separator",
            tokens.border,
        ))
        .with_child(menu_button(
            palette,
            "file",
            "app.file",
            "application.menu.file",
            56.0,
            open_menu == Some("file"),
        ))
        .with_child(menu_button(
            palette,
            "edit",
            "app.edit_menu",
            "application.menu.edit",
            56.0,
            open_menu == Some("edit"),
        ))
        .with_child(menu_button(
            palette,
            "view",
            "app.view_menu",
            "application.menu.view",
            60.0,
            open_menu == Some("view"),
        ))
        .with_child(menu_button(
            palette,
            "project",
            "app.project_menu",
            "application.menu.project",
            76.0,
            open_menu == Some("project"),
        ))
        .with_child(menu_button(
            palette,
            "help",
            "app.help_menu",
            "application.menu.help",
            60.0,
            open_menu == Some("help"),
        ))
        .with_child(agent_toolbar_button(palette, agent_status))
        .with_child(
            UiNode::new("application-bar.drag-region", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::default()
                })
                .with_event(UiEventBinding::command(
                    UiEventKind::DragStart,
                    "window.drag",
                )),
        )
        .with_child(command_search(palette))
        .with_child(
            UiNode::new("application-bar.context", UiNodeKind::Label)
                .with_text_key(context_key)
                .with_text_style(UiTextStyle::button(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(saved_status(palette))
        .with_child(action_button(
            palette,
            "application-bar.settings",
            "app.settings_menu",
            "editor.settings",
            78.0,
        ))
        .with_child(window_button(
            "application-bar.minimize",
            "window.minimize",
            "app.window.minimize",
        ))
        .with_child(window_button(
            "application-bar.maximize",
            "window.maximize",
            "app.window.maximize",
        ))
        .with_child(window_button(
            "application-bar.close",
            "window.close",
            "app.window.close",
        ));

    let mut surface = UiSurface::new("editor.application-bar", palette, root);
    surface.style_sheet = application_bar_style_sheet(palette);
    surface
}

/// Builds the retained popup for one application menu. The menu model is
/// shared with native adapters; this surface only turns that model into RafUI
/// nodes and emits the stable command ids on activation.
pub fn build_application_menu_popup_surface(palette: StudioUiPalette, menu: &UiMenu) -> UiSurface {
    let tokens = palette.tokens();
    let height = APPLICATION_MENU_PADDING * 2.0
        + APPLICATION_MENU_GAP * 2.0
        + menu
            .items
            .iter()
            .map(|item| match item {
                UiMenuItem::Separator => APPLICATION_MENU_SEPARATOR_HEIGHT,
                UiMenuItem::Command(_) | UiMenuItem::Submenu(_) => APPLICATION_MENU_ROW_HEIGHT,
            })
            .sum::<f32>();
    let mut root = UiNode::new("application-menu.popup", UiNodeKind::Panel)
        .with_class("application-menu-popup")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 0.0,
            padding: UiSpacing::same(APPLICATION_MENU_PADDING),
            overflow: raf_ui::UiOverflow::Clip,
            ..UiLayout::fixed(APPLICATION_MENU_POPUP_WIDTH, height)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 5.0,
            opacity: 1.0,
        });

    for (index, item) in menu.items.iter().enumerate() {
        root = root.with_child(match item {
            UiMenuItem::Separator => UiNode::new(
                format!("application-menu.separator.{index}"),
                UiNodeKind::Separator,
            )
            .with_class("application-menu-separator")
            .with_layout(UiLayout::fixed(0.0, APPLICATION_MENU_SEPARATOR_HEIGHT)),
            UiMenuItem::Command(command) => menu_command_row(palette, index, command),
            UiMenuItem::Submenu(submenu) => menu_submenu_row(palette, index, submenu),
        });
    }

    let mut surface = UiSurface::new(
        format!("editor.application-menu.{}", menu.id),
        palette,
        root,
    );
    surface.style_sheet = application_menu_style_sheet(palette);
    surface
}

fn menu_command_row(palette: StudioUiPalette, index: usize, command: &UiMenuCommand) -> UiNode {
    let tokens = palette.tokens();
    let mut row = UiNode::new(
        format!("application-menu.command.{index}.{}", command.id),
        UiNodeKind::Button,
    )
    .with_class("application-menu-row")
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: raf_ui::UiAlign::Center,
        gap: 7.0,
        padding: UiSpacing::xy(7.0, 0.0),
        ..UiLayout::fixed(0.0, APPLICATION_MENU_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
    })
    .disabled(!command.enabled)
    .with_accessibility_label_key(command.label_key.clone());

    if command.enabled {
        row = row.focusable().with_event(UiEventBinding::command(
            UiEventKind::Click,
            command.id.clone(),
        ));
    }
    row = row.with_child(
        UiNode::new(
            format!("application-menu.check.{index}.{}", command.id),
            UiNodeKind::Panel,
        )
        .with_layout(UiLayout::fixed(16.0, 22.0))
        .with_icon(UiIcon::new(UiIconId::Success).with_size(UiIconSize::Small))
        .with_style(UiStyle {
            fill: [0, 0, 0, 0],
            border: [0, 0, 0, 0],
            text: tokens.accent,
            border_width: 0.0,
            radius: 0.0,
            opacity: if command.checked { 1.0 } else { 0.0 },
        }),
    );
    row = row.with_child(
        UiNode::new(
            format!("application-menu.label.{index}.{}", command.id),
            UiNodeKind::Label,
        )
        .with_text_key(command.label_key.clone())
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::fit_content()
        }),
    );
    if let Some(accelerator) = command.accelerator.as_deref() {
        row = row.with_child(
            UiNode::new(
                format!("application-menu.accelerator.{index}.{}", command.id),
                UiNodeKind::Label,
            )
            .with_text_key(accelerator)
            .with_text_style(UiTextStyle::button(tokens.text_muted))
            .with_layout(UiLayout::fit_content()),
        );
    }
    row
}

fn menu_submenu_row(palette: StudioUiPalette, index: usize, submenu: &UiMenu) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(
        format!("application-menu.submenu.{index}.{}", submenu.id),
        UiNodeKind::Label,
    )
    .with_class("application-menu-row")
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: raf_ui::UiAlign::Center,
        padding: UiSpacing::xy(7.0, 0.0),
        ..UiLayout::fixed(0.0, APPLICATION_MENU_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
    })
    .with_text_key(submenu.label_key.clone())
    .with_text_style(UiTextStyle::button(tokens.text_muted))
}

fn application_menu_style_sheet(palette: StudioUiPalette) -> raf_ui::UiStyleSheet {
    let tokens = palette.tokens();
    raf_ui::UiStyleSheet {
        rules: vec![
            raf_ui::UiStyleRule::new(
                raf_ui::UiStyleSelector::Class("application-menu-row".to_string()),
                raf_ui::UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.surface_raised),
                    text: Some(tokens.text),
                    radius: Some(3.0),
                    ..raf_ui::UiStylePatch::default()
                },
            )
            .when(raf_ui::UiStyleRuleState::Always),
            raf_ui::UiStyleRule::new(
                raf_ui::UiStyleSelector::Class("application-menu-row".to_string()),
                raf_ui::UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent),
                    text: Some([255, 255, 255, 255]),
                    ..raf_ui::UiStylePatch::default()
                },
            )
            .when(raf_ui::UiStyleRuleState::Hovered),
            raf_ui::UiStyleRule::new(
                raf_ui::UiStyleSelector::Class("application-menu-row".to_string()),
                raf_ui::UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.surface_raised),
                    text: Some(tokens.text_muted),
                    opacity: Some(0.48),
                    ..raf_ui::UiStylePatch::default()
                },
            )
            .when(raf_ui::UiStyleRuleState::Disabled),
            raf_ui::UiStyleRule::new(
                raf_ui::UiStyleSelector::Class("application-menu-separator".to_string()),
                raf_ui::UiStylePatch {
                    fill: Some(tokens.border),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..raf_ui::UiStylePatch::default()
                },
            )
            .when(raf_ui::UiStyleRuleState::Always),
        ],
    }
}

fn agent_bar_rule(
    class: &str,
    fill: [u8; 4],
    border: [u8; 4],
    text: [u8; 4],
) -> raf_ui::UiStyleRule {
    raf_ui::UiStyleRule::new(
        raf_ui::UiStyleSelector::Class(class.to_string()),
        raf_ui::UiStylePatch {
            fill: Some(fill),
            border: Some(border),
            text: Some(text),
            border_width: Some(1.0),
            radius: Some(2.0),
            ..raf_ui::UiStylePatch::default()
        },
    )
    .when(raf_ui::UiStyleRuleState::Always)
}

fn separator(id: &str, color: [u8; 4]) -> UiNode {
    UiNode::new(id, UiNodeKind::Separator)
        .with_layout(UiLayout::fixed(1.0, 18.0))
        .with_style(UiStyle {
            fill: color,
            border: color,
            text: color,
            border_width: 0.0,
            radius: 0.0,
            opacity: 1.0,
        })
}

fn menu_button(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    command: &str,
    width: f32,
    open: bool,
) -> UiNode {
    let class = if open {
        "application-bar-menu application-bar-menu-open"
    } else {
        "application-bar-menu"
    };
    UiNode::new(format!("application-bar.menu.{id}"), UiNodeKind::Button)
        .with_class(class)
        .with_layout(UiLayout::fixed(width, 28.0))
        .with_child(labeled_content(
            palette,
            format!("application-bar.menu.{id}"),
            label_key,
            Some(format!("editor.top.{id}")),
        ))
        .with_accessibility_label_key(label_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_event(UiEventBinding::command(UiEventKind::ContextMenu, command))
}

fn action_button(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    command: &str,
    width: f32,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("application-bar-secondary-action")
        .with_layout(UiLayout::fixed(width, 28.0))
        .with_child(labeled_content(palette, id, label_key, None))
        .with_accessibility_label_key(label_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn agent_toolbar_button(palette: StudioUiPalette, status: AgentBarStatus) -> UiNode {
    let tokens = palette.tokens();
    let class = match status {
        AgentBarStatus::Ready => "application-bar-agent",
        AgentBarStatus::Thinking => "application-bar-agent-busy",
        AgentBarStatus::Approval => "application-bar-agent-warning",
        AgentBarStatus::Error => "application-bar-agent-error",
    };
    UiNode::new("application-bar.agent", UiNodeKind::Button)
        .with_class(class)
        .with_layout(UiLayout::fixed(76.0, 28.0))
        .with_child(
            UiNode::new("application-bar.agent.content", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: raf_ui::UiAlign::Center,
                    justify_content: raf_ui::UiJustify::Center,
                    gap: 5.0,
                    ..UiLayout::fill(UiFlow::Row)
                })
                .with_child(
                    UiNode::new("application-bar.agent.icon", UiNodeKind::Panel)
                        .with_icon(UiIcon::new(UiIconId::Agent).with_size(UiIconSize::Small))
                        .with_layout(UiLayout::fixed(16.0, 18.0)),
                )
                .with_child(
                    UiNode::new("application-bar.agent.label", UiNodeKind::Label)
                        .with_text_key("app.agent_tab")
                        .with_text_style(UiTextStyle::button(tokens.text))
                        .with_layout(UiLayout::fit_content()),
                ),
        )
        .with_tooltip_key("app.agent_title")
        .with_accessibility_label_key("app.agent_title")
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, "agent.open"))
}

fn command_search(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("application-bar.command-search", UiNodeKind::Panel)
        .with_class("application-bar-command-search")
        .with_layout(UiLayout::fixed(252.0, 28.0))
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: raf_ui::UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(4.0, 0.0),
            ..UiLayout::fixed(252.0, 28.0)
        })
        .with_child(
            UiNode::text_input(
                "application-bar.command-search.input",
                UiTextInput {
                    value_key: "application-bar.command-search.value".to_string(),
                    placeholder_key: Some("app.command_search".to_string()),
                    max_length: 256,
                    multiline: false,
                    password: false,
                    submit_command: Some("search.open".to_string()),
                },
            )
            .with_class("application-bar-command-input")
            .with_layout(UiLayout::fixed(206.0, 24.0)),
        )
        .with_child(
            UiNode::new("application-bar.command-search.shortcut", UiNodeKind::Label)
                .with_text_key("Ctrl+K")
                .with_text_style(UiTextStyle::button(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
}

fn saved_status(palette: StudioUiPalette) -> UiNode {
    UiNode::new("application-bar.saved", UiNodeKind::Panel)
        .with_class("application-bar-status")
        .with_layout(UiLayout::fixed(78.0, 28.0))
        .with_child(labeled_content(
            palette,
            "application-bar.saved",
            "app.saved",
            Some("editor.top.save".to_string()),
        ))
}

fn labeled_content(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: &str,
    icon_source: Option<String>,
) -> UiNode {
    let tokens = palette.tokens();
    let id = id.into();
    let mut content =
        UiNode::new(format!("{id}.content"), UiNodeKind::Panel).with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: raf_ui::UiAlign::Center,
            justify_content: raf_ui::UiJustify::Center,
            gap: 5.0,
            padding: UiSpacing::xy(5.0, 0.0),
            ..UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill)
        });
    if let Some(source) = icon_source {
        content = content.with_child(
            UiNode::image(
                format!("{id}.icon"),
                UiImage {
                    source: UiImageSource::new(source),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(14.0, 14.0)),
        );
    }
    content.with_child(
        UiNode::new(format!("{id}.label"), UiNodeKind::Label)
            .with_text_key(label_key)
            .with_text_style(UiTextStyle::button(tokens.text))
            .with_layout(UiLayout::fit_content()),
    )
}

fn window_button(id: &str, command: &str, tooltip: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("application-bar-window-button")
        .with_layout(UiLayout::fixed(28.0, 28.0))
        .with_tooltip_key(tooltip)
        .with_accessibility_label_key(tooltip)
        .with_child(
            UiNode::new(format!("{id}.content"), UiNodeKind::Panel)
                .with_layout(UiLayout {
                    justify_content: raf_ui::UiJustify::Center,
                    align_items: raf_ui::UiAlign::Center,
                    ..UiLayout::fill(UiFlow::Row)
                })
                .with_child(
                    UiNode::image(
                        format!("{id}.icon"),
                        UiImage {
                            source: UiImageSource::new(format!(
                                "editor.top.{}",
                                command.trim_start_matches("window.")
                            )),
                            fit: UiImageFit::Contain,
                            tint: None,
                        },
                    )
                    .with_layout(UiLayout::fixed(14.0, 14.0)),
                ),
        )
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn application_bar_style_sheet(palette: StudioUiPalette) -> raf_ui::UiStyleSheet {
    let tokens = palette.tokens();
    raf_ui::UiStyleSheet {
        rules: vec![
            raf_ui::UiStyleRule::new(
                raf_ui::UiStyleSelector::Class("application-bar-menu".to_string()),
                raf_ui::UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.surface_alt),
                    text: Some(tokens.text_muted),
                    radius: Some(2.0),
                    ..raf_ui::UiStylePatch::default()
                },
            )
            .when(raf_ui::UiStyleRuleState::Always),
            raf_ui::UiStyleRule::new(
                raf_ui::UiStyleSelector::Class("application-bar-menu".to_string()),
                raf_ui::UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    text: Some(tokens.text),
                    ..raf_ui::UiStylePatch::default()
                },
            )
            .when(raf_ui::UiStyleRuleState::Hovered),
            agent_bar_rule(
                "application-bar-agent",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            agent_bar_rule(
                "application-bar-agent-busy",
                tokens.surface_raised,
                tokens.accent,
                tokens.text,
            ),
            agent_bar_rule(
                "application-bar-agent-warning",
                tokens.surface_raised,
                tokens.warning,
                tokens.text,
            ),
            agent_bar_rule(
                "application-bar-agent-error",
                tokens.surface_raised,
                tokens.danger,
                tokens.text,
            ),
            raf_ui::UiStyleRule::new(
                raf_ui::UiStyleSelector::Class("application-bar-secondary-action".to_string()),
                raf_ui::UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.surface_alt),
                    text: Some(tokens.text_muted),
                    radius: Some(2.0),
                    ..raf_ui::UiStylePatch::default()
                },
            )
            .when(raf_ui::UiStyleRuleState::Always),
            raf_ui::UiStyleRule::new(
                raf_ui::UiStyleSelector::Class("application-bar-secondary-action".to_string()),
                raf_ui::UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    text: Some(tokens.text),
                    ..raf_ui::UiStylePatch::default()
                },
            )
            .when(raf_ui::UiStyleRuleState::Hovered),
            raf_ui::UiStyleRule::new(
                raf_ui::UiStyleSelector::Class("application-bar-command-search".to_string()),
                raf_ui::UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    text: Some(tokens.text_muted),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..raf_ui::UiStylePatch::default()
                },
            )
            .when(raf_ui::UiStyleRuleState::Always),
            raf_ui::UiStyleRule::new(
                raf_ui::UiStyleSelector::Class("application-bar-command-input".to_string()),
                raf_ui::UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    text: Some(tokens.text),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    ..raf_ui::UiStylePatch::default()
                },
            )
            .when(raf_ui::UiStyleRuleState::Always),
            raf_ui::UiStyleRule::new(
                raf_ui::UiStyleSelector::Class("application-bar-status".to_string()),
                raf_ui::UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.surface_alt),
                    text: Some(tokens.text_muted),
                    radius: Some(2.0),
                    ..raf_ui::UiStylePatch::default()
                },
            )
            .when(raf_ui::UiStyleRuleState::Always),
            raf_ui::UiStyleRule::new(
                raf_ui::UiStyleSelector::Class("application-bar-window-button".to_string()),
                raf_ui::UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.surface_alt),
                    text: Some(tokens.text_muted),
                    radius: Some(2.0),
                    ..raf_ui::UiStylePatch::default()
                },
            )
            .when(raf_ui::UiStyleRuleState::Always),
            raf_ui::UiStyleRule::new(
                raf_ui::UiStyleSelector::Class("application-bar-window-button".to_string()),
                raf_ui::UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    ..raf_ui::UiStylePatch::default()
                },
            )
            .when(raf_ui::UiStyleRuleState::Hovered),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_bar_does_not_advertise_build_or_play_controls() {
        let surface = build_application_bar_surface(
            StudioUiPalette::IndustrialDark,
            "Demo",
            ProjectType::Game,
            None,
            AgentBarStatus::Ready,
        );
        let serialized = format!("{surface:?}");
        assert!(!serialized.contains("Build"));
        assert!(!serialized.contains("Play"));
        assert!(serialized.contains("application.menu.file"));
        assert!(serialized.contains("editor.top.logo"));
        assert!(serialized.contains("app.product_name"));
        assert!(serialized.contains("application-bar.command-search"));
        assert!(serialized.contains("application-bar-secondary-action"));
        assert!(serialized.contains("application-bar.menu.file.content"));
        assert!(serialized.contains("application-bar.menu.file.icon"));
        assert!(serialized.contains("application-bar.menu.file.label"));
        assert!(serialized.contains("application-bar.saved.content"));
        assert!(serialized.contains("application-bar.drag-region"));
    }
}
