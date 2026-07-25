//! Retained RafUI document for global engine settings.
//!
//! This module defines presentation and typed control keys only. Persistence,
//! validation, and side effects stay in `SettingsSurfaceHost` and the app.

use raf_core::ai::{AgentMode, AiProvider, AiProviderConfig};
use raf_core::config::{
    EngineSettings, Language, RenderExecutionPolicy, RenderQuality, ScriptLanguage, TargetPlatform,
    Theme, ViewportRenderMode,
};
use raf_core::i18n::t;
use raf_core::units::DisplayUnit;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiAlign, UiCompactMode, UiEventBinding, UiEventKind, UiFlow,
    UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiRange, UiResponsiveRule, UiScrollAxis,
    UiSpacing, UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet,
    UiSurface, UiTextInput, UiTextStyle, UiToggle,
};

const CONTROL_HEIGHT: f32 = 34.0;
const RANGE_WIDTH: f32 = 244.0;
const SELECTED_TEXT_ORANGE: [u8; 4] = [232, 133, 28, 255];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Appearance,
    Performance,
    Editor,
    Viewport,
    Scripting,
    Ai,
    Platform,
}

impl Default for SettingsSection {
    fn default() -> Self {
        Self::Appearance
    }
}

impl SettingsSection {
    pub const ALL: [Self; 7] = [
        Self::Appearance,
        Self::Performance,
        Self::Editor,
        Self::Viewport,
        Self::Scripting,
        Self::Ai,
        Self::Platform,
    ];

    pub fn command(self) -> &'static str {
        match self {
            Self::Appearance => "settings.section.appearance",
            Self::Performance => "settings.section.performance",
            Self::Editor => "settings.section.editor",
            Self::Viewport => "settings.section.viewport",
            Self::Scripting => "settings.section.scripting",
            Self::Ai => "settings.section.ai",
            Self::Platform => "settings.section.platform",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Appearance => "settings.appearance",
            Self::Performance => "settings.performance",
            Self::Editor => "settings.editor",
            Self::Viewport => "settings.viewport",
            Self::Scripting => "settings.scripting",
            Self::Ai => "settings.ai_providers",
            Self::Platform => "settings.target_platform",
        }
    }

    pub fn from_command(command: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|section| section.command() == command)
    }
}

pub fn provider_id(provider: AiProvider) -> &'static str {
    match provider {
        AiProvider::Puerto => "puerto",
        AiProvider::OpenRouter => "openrouter",
        AiProvider::OpenAI => "openai",
        AiProvider::GenAI => "genai",
        AiProvider::Claude => "claude",
    }
}

pub fn provider_from_id(id: &str) -> Option<AiProvider> {
    match id {
        "puerto" => Some(AiProvider::Puerto),
        "openrouter" => Some(AiProvider::OpenRouter),
        "openai" => Some(AiProvider::OpenAI),
        "genai" => Some(AiProvider::GenAI),
        "claude" => Some(AiProvider::Claude),
        _ => None,
    }
}

pub fn build_settings_surface(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    section: SettingsSection,
    visible_api_keys: &[AiProvider],
) -> UiSurface {
    let tokens = palette.tokens();
    let root = UiNode::new("settings.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            compact: UiCompactMode::Stack,
            responsive: vec![UiResponsiveRule {
                max_width: 760.0,
                flow: Some(UiFlow::Column),
                basis: None,
                padding: None,
                gap: Some(0.0),
                compact: Some(UiCompactMode::Stack),
                grid_columns: None,
            }],
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(palette.root_style())
        .with_child(settings_navigation(palette, section))
        .with_child(settings_workspace(
            palette,
            settings,
            section,
            visible_api_keys,
        ));

    let mut surface = UiSurface::new("studio.settings", palette, root);
    surface.style_sheet = settings_style_sheet(palette, tokens);
    surface
}

fn settings_navigation(palette: StudioUiPalette, active: SettingsSection) -> UiNode {
    let tokens = palette.tokens();
    let mut navigation = UiNode::new("settings.navigation", UiNodeKind::Panel)
        .with_class("settings-navigation")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            basis: [224.0, 0.0],
            min_size: [204.0, 0.0],
            padding: UiSpacing::same(16.0),
            gap: 4.0,
            overflow: UiOverflow::Clip,
            responsive: vec![UiResponsiveRule {
                max_width: 760.0,
                flow: Some(UiFlow::RowWrap),
                basis: Some([0.0, 154.0]),
                padding: Some(UiSpacing::same(12.0)),
                gap: Some(5.0),
                compact: Some(UiCompactMode::Wrap),
                grid_columns: None,
            }],
            ..UiLayout::default()
        })
        .with_child(
            UiNode::new("settings.brand", UiNodeKind::Label)
                .with_text_key("app.engine_settings_title")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 26.0)),
        )
        .with_child(
            UiNode::new("settings.brand-detail", UiNodeKind::Label)
                .with_text_key("settings.surface.subtitle")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 42.0)),
        )
        .with_child(
            UiNode::new("settings.navigation-divider", UiNodeKind::Separator)
                .with_layout(UiLayout::fixed(0.0, 1.0)),
        );

    for section in SettingsSection::ALL {
        let class = if section == active {
            "settings-nav-active"
        } else {
            "settings-nav-button"
        };
        navigation = navigation.with_child(
            UiNode::new(
                format!("settings.nav.{}", section.key()),
                UiNodeKind::Button,
            )
            .with_text_key(section.key())
            .with_text_style(UiTextStyle::button(if section == active {
                SELECTED_TEXT_ORANGE
            } else {
                tokens.text
            }))
            .with_class(class)
            .with_layout(UiLayout::fixed(0.0, 34.0))
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                section.command(),
            )),
        );
    }

    navigation
        .with_child(
            UiNode::new("settings.navigation-spacer", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::default()
                })
                .with_style(UiStyle::transparent()),
        )
        .with_child(
            UiNode::new("settings.navigation-hint", UiNodeKind::Label)
                .with_text_key("settings.surface.draft_hint")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 40.0)),
        )
}

fn settings_workspace(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    section: SettingsSection,
    visible_api_keys: &[AiProvider],
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("settings.workspace", UiNodeKind::Panel)
        .with_class("settings-workspace")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(
            UiNode::new("settings.header", UiNodeKind::Toolbar)
                .with_class("settings-header")
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    padding: UiSpacing::xy(28.0, 15.0),
                    gap: 3.0,
                    ..UiLayout::fixed(0.0, 76.0)
                })
                .with_child(
                    UiNode::new("settings.header-title", UiNodeKind::Label)
                        .with_text_key(section.key())
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout::fixed(0.0, 22.0)),
                )
                .with_child(
                    UiNode::new("settings.header-help", UiNodeKind::Label)
                        .with_text_key(section_help_key(section))
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fixed(0.0, 18.0)),
                ),
        )
        .with_child(
            UiNode::scroll_view("settings.content", UiScrollAxis::Vertical)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    grow: 1.0,
                    padding: UiSpacing::xy(30.0, 24.0),
                    overflow: UiOverflow::ScrollY,
                    ..UiLayout::fill(UiFlow::Column)
                })
                .with_style(UiStyle::transparent())
                .with_child(section_content(
                    palette,
                    settings,
                    section,
                    visible_api_keys,
                )),
        )
        .with_child(
            UiNode::new("settings.footer", UiNodeKind::Toolbar)
                .with_class("settings-footer")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    justify_content: UiJustify::End,
                    align_items: UiAlign::Center,
                    padding: UiSpacing::xy(28.0, 0.0),
                    gap: 8.0,
                    ..UiLayout::fixed(0.0, 62.0)
                })
                .with_child(command_button(
                    "settings.cancel",
                    "app.cancel",
                    "settings.cancel",
                    94.0,
                    "settings-secondary-button",
                    tokens.text,
                ))
                .with_child(command_button(
                    "settings.save",
                    "app.save_and_close",
                    "settings.save",
                    134.0,
                    "settings-primary-button",
                    SELECTED_TEXT_ORANGE,
                )),
        )
}

fn section_content(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    section: SettingsSection,
    visible_api_keys: &[AiProvider],
) -> UiNode {
    match section {
        SettingsSection::Appearance => appearance_section(palette, settings),
        SettingsSection::Performance => performance_section(palette, settings),
        SettingsSection::Editor => editor_section(palette, settings),
        SettingsSection::Viewport => viewport_section(palette, settings),
        SettingsSection::Scripting => scripting_section(palette, settings),
        SettingsSection::Ai => ai_section(palette, settings, visible_api_keys),
        SettingsSection::Platform => platform_section(palette, settings),
    }
}

fn appearance_section(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    let tokens = palette.tokens();
    section_frame(palette, "settings.appearance")
        .with_child(toggle_row(
            palette,
            "settings.simple-mode",
            "settings.simple_mode",
            Some("settings.simple_mode_desc"),
            "settings.simple-mode",
            settings.simple_mode,
        ))
        .with_child(segment_row(
            palette,
            "settings.theme",
            None,
            &[
                segment(
                    "settings.theme.dark",
                    "settings.value.dark",
                    settings.theme == Theme::Dark,
                ),
                segment(
                    "settings.theme.light",
                    "settings.value.light",
                    settings.theme == Theme::Light,
                ),
                segment(
                    "settings.theme.system",
                    "settings.value.system",
                    settings.theme == Theme::System,
                ),
            ],
        ))
        .with_child(range_row(
            palette,
            "settings.theme_experimental",
            Some("settings.theme_experimental_desc"),
            "settings.theme-experimental",
            settings.theme_experimental,
            0.0,
            100.0,
            1.0,
            format!("{:.0}%", settings.theme_experimental),
            false,
        ))
        .with_child(range_row(
            palette,
            "settings.font_size",
            None,
            "settings.font-size",
            settings.font_size,
            10.0,
            24.0,
            1.0,
            format!("{:.0} px", settings.font_size),
            false,
        ))
        .with_child(toggle_row(
            palette,
            "settings.auto-ui-scale",
            "settings.auto_ui_scale",
            Some("settings.auto_ui_scale_desc"),
            "settings.auto-ui-scale",
            settings.auto_ui_scale,
        ))
        .with_child(range_row(
            palette,
            "settings.ui_scale",
            None,
            "settings.ui-scale",
            settings.ui_scale,
            0.5,
            3.0,
            0.1,
            format!("{:.1}x", settings.ui_scale),
            settings.auto_ui_scale,
        ))
        .with_child(segment_row(
            palette,
            "settings.language",
            None,
            &[
                segment(
                    "settings.language.english",
                    "settings.value.english",
                    settings.language == Language::English,
                ),
                segment(
                    "settings.language.spanish",
                    "settings.value.spanish",
                    settings.language == Language::Spanish,
                ),
            ],
        ))
        .with_child(
            UiNode::new("settings.appearance-footnote", UiNodeKind::Label)
                .with_text_key("settings.surface.theme_preview")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 26.0)),
        )
}

fn performance_section(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    section_frame(palette, "settings.performance")
        .with_child(segment_row(
            palette,
            "settings.quality",
            None,
            &[
                segment(
                    "settings.quality.potato",
                    "settings.value.potato",
                    settings.render_quality == RenderQuality::Potato,
                ),
                segment(
                    "settings.quality.low",
                    "settings.value.low",
                    settings.render_quality == RenderQuality::Low,
                ),
                segment(
                    "settings.quality.medium",
                    "settings.value.medium",
                    settings.render_quality == RenderQuality::Medium,
                ),
                segment(
                    "settings.quality.high",
                    "settings.value.high",
                    settings.render_quality == RenderQuality::High,
                ),
            ],
        ))
        .with_child(segment_row(
            palette,
            "settings.render_execution_policy",
            Some("settings.render_execution_policy_desc"),
            &[
                segment(
                    "settings.policy.auto",
                    "settings.render_execution_policy.auto",
                    settings.render_execution_policy == RenderExecutionPolicy::Auto,
                ),
                segment(
                    "settings.policy.cpu",
                    "settings.render_execution_policy.cpu_only",
                    settings.render_execution_policy == RenderExecutionPolicy::CpuOnly,
                ),
                segment(
                    "settings.policy.gpu",
                    "settings.render_execution_policy.gpu_preferred",
                    settings.render_execution_policy == RenderExecutionPolicy::GpuPreferred,
                ),
            ],
        ))
        .with_child(toggle_row(
            palette,
            "settings.fps-unlimited",
            "settings.fps_unlimited",
            None,
            "settings.fps-unlimited",
            settings.fps_limit == 0,
        ))
        .with_child(range_row(
            palette,
            "settings.fps_limit",
            None,
            "settings.fps-limit",
            settings.fps_limit.max(15) as f32,
            15.0,
            240.0,
            1.0,
            if settings.fps_limit == 0 {
                t("settings.fps_unlimited", settings.language)
            } else {
                format!("{} fps", settings.fps_limit)
            },
            settings.fps_limit == 0,
        ))
        .with_child(toggle_row(
            palette,
            "settings.show-fps-counter",
            "settings.show_fps_counter",
            None,
            "settings.show-fps-counter",
            settings.show_fps_counter,
        ))
        .with_child(toggle_row(
            palette,
            "settings.vsync",
            "settings.vsync",
            None,
            "settings.vsync",
            settings.vsync,
        ))
        .with_child(toggle_row(
            palette,
            "settings.multithreading",
            "settings.multithreading",
            None,
            "settings.multithreading",
            settings.multithreading,
        ))
}

fn editor_section(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    section_frame(palette, "settings.editor")
        .with_child(toggle_row(
            palette,
            "settings.show-grid",
            "settings.show_grid",
            None,
            "settings.show-grid",
            settings.grid_visible,
        ))
        .with_child(toggle_row(
            palette,
            "settings.snap-grid",
            "settings.snap_to_grid",
            None,
            "settings.snap-grid",
            settings.snap_to_grid,
        ))
        .with_child(range_row(
            palette,
            "settings.grid_size",
            None,
            "settings.grid-size",
            settings.grid_size,
            0.1,
            10.0,
            0.1,
            format!("{:.1} m", settings.grid_size),
            settings.simple_mode,
        ))
        .with_child(range_row(
            palette,
            "settings.grid_load_distance",
            None,
            "settings.grid-load-distance",
            settings.grid_load_distance,
            0.0,
            500.0,
            0.5,
            format!("{:.1} m", settings.grid_load_distance),
            settings.simple_mode,
        ))
        .with_child(range_row(
            palette,
            "settings.auto_save",
            None,
            "settings.auto-save",
            settings.auto_save_interval_seconds as f32,
            30.0,
            600.0,
            1.0,
            format!("{} s", settings.auto_save_interval_seconds),
            settings.simple_mode,
        ))
        .with_child(segment_row(
            palette,
            "settings.units",
            None,
            &[
                segment(
                    "settings.units.metric",
                    "settings.metric",
                    settings.display_unit == DisplayUnit::Metric,
                ),
                segment(
                    "settings.units.imperial",
                    "settings.imperial",
                    settings.display_unit == DisplayUnit::Imperial,
                ),
                segment(
                    "settings.units.game",
                    "settings.value.game_units",
                    settings.display_unit == DisplayUnit::Game,
                ),
            ],
        ))
        .with_child(toggle_row(
            palette,
            "settings.command-console",
            "settings.command_console_enabled",
            Some("settings.command_console_enabled_desc"),
            "settings.command-console",
            settings.command_console_enabled,
        ))
}

fn viewport_section(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    section_frame(palette, "settings.viewport")
        .with_child(segment_row(
            palette,
            "settings.viewport_render_mode",
            None,
            &[
                segment(
                    "settings.viewport.solid",
                    "settings.viewport_render_mode.solid",
                    settings.viewport_render_mode == ViewportRenderMode::Solid,
                ),
                segment(
                    "settings.viewport.wireframe",
                    "settings.viewport_render_mode.wireframe",
                    settings.viewport_render_mode == ViewportRenderMode::Wireframe,
                ),
                segment(
                    "settings.viewport.preview",
                    "settings.viewport_render_mode.preview",
                    settings.viewport_render_mode == ViewportRenderMode::Preview,
                ),
            ],
        ))
        .with_child(toggle_row(
            palette,
            "settings.viewport-labels",
            "settings.show_viewport_labels",
            None,
            "settings.viewport-labels",
            settings.show_viewport_labels,
        ))
        .with_child(toggle_row(
            palette,
            "settings.focus-lock",
            "settings.focus_lock_enabled",
            Some("settings.focus_lock_enabled_desc"),
            "settings.focus-lock",
            settings.focus_lock_enabled,
        ))
        .with_child(toggle_row(
            palette,
            "settings.solid-edges",
            "settings.solid_show_surface_edges",
            None,
            "settings.solid-edges",
            settings.solid_show_surface_edges,
        ))
        .with_child(toggle_row(
            palette,
            "settings.solid-xray",
            "settings.solid_xray_mode",
            None,
            "settings.solid-xray",
            settings.solid_xray_mode,
        ))
        .with_child(toggle_row(
            palette,
            "settings.solid-tonality",
            "settings.solid_face_tonality",
            None,
            "settings.solid-tonality",
            settings.solid_face_tonality,
        ))
        .with_child(section_divider(palette, "settings.viewport-input-divider"))
        .with_child(section_label(palette, "settings.surface.input"))
        .with_child(toggle_row(
            palette,
            "settings.invert-x",
            "settings.invert_mouse_x",
            None,
            "settings.invert-x",
            settings.invert_mouse_x,
        ))
        .with_child(toggle_row(
            palette,
            "settings.invert-y",
            "settings.invert_mouse_y",
            None,
            "settings.invert-y",
            settings.invert_mouse_y,
        ))
        .with_child(toggle_row(
            palette,
            "settings.invert-ws",
            "settings.invert_ws",
            None,
            "settings.invert-ws",
            settings.invert_ws,
        ))
        .with_child(range_row(
            palette,
            "settings.wasd_speed",
            None,
            "settings.wasd-speed",
            settings.wasd_speed,
            0.5,
            5.0,
            0.1,
            format!("{:.1}x", settings.wasd_speed),
            false,
        ))
        .with_child(section_divider(palette, "settings.viewport-gizmo-divider"))
        .with_child(section_label(palette, "settings.gizmo_controls"))
        .with_child(range_row(
            palette,
            "settings.move_sensitivity",
            None,
            "settings.move-sensitivity",
            settings.move_gizmo_sensitivity,
            0.25,
            4.0,
            0.05,
            format!("{:.2}x", settings.move_gizmo_sensitivity),
            false,
        ))
        .with_child(range_row(
            palette,
            "settings.rotate_sensitivity",
            None,
            "settings.rotate-sensitivity",
            settings.rotate_gizmo_sensitivity,
            0.25,
            4.0,
            0.05,
            format!("{:.2}x", settings.rotate_gizmo_sensitivity),
            false,
        ))
        .with_child(range_row(
            palette,
            "settings.scale_sensitivity",
            None,
            "settings.scale-sensitivity",
            settings.scale_gizmo_sensitivity,
            0.25,
            4.0,
            0.05,
            format!("{:.2}x", settings.scale_gizmo_sensitivity),
            false,
        ))
        .with_child(toggle_row(
            palette,
            "settings.uniform-scale",
            "settings.uniform_scale_by_default",
            None,
            "settings.uniform-scale",
            settings.uniform_scale_by_default,
        ))
        .with_child(range_row(
            palette,
            "settings.gizmo_growth_scale",
            None,
            "settings.gizmo-growth",
            settings.gizmo_growth_scale,
            0.0,
            100.0,
            1.0,
            format!("{:.0}%", settings.gizmo_growth_scale),
            false,
        ))
}

fn scripting_section(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    section_frame(palette, "settings.scripting")
        .with_child(toggle_row(
            palette,
            "settings.script-runtime",
            "settings.script_runtime_enabled",
            Some("settings.script_runtime_enabled_desc"),
            "settings.script-runtime",
            settings.script_runtime_enabled,
        ))
        .with_child(segment_row(
            palette,
            "settings.default_script_language",
            None,
            &[
                segment(
                    "settings.script-language.rhai",
                    "settings.value.rhai",
                    settings.default_script_language == ScriptLanguage::Rhai,
                ),
                segment(
                    "settings.script-language.cpp",
                    "settings.value.cpp_wasm",
                    settings.default_script_language == ScriptLanguage::Cpp,
                ),
                segment(
                    "settings.script-language.nodes",
                    "settings.value.visual_nodes",
                    settings.default_script_language == ScriptLanguage::Nodes,
                ),
            ],
        ))
        .with_child(toggle_row(
            palette,
            "settings.script-hot-reload",
            "settings.script_hot_reload",
            None,
            "settings.script-hot-reload",
            settings.script_hot_reload,
        ))
        .with_child(range_row(
            palette,
            "settings.script_timeout_ms",
            None,
            "settings.script-timeout",
            settings.script_timeout_ms as f32,
            10.0,
            1_000.0,
            1.0,
            format!("{} ms", settings.script_timeout_ms),
            false,
        ))
        .with_child(text_input_row(
            palette,
            "settings.script_external_editor",
            None,
            "settings.script-external-editor",
            false,
            false,
        ))
}

fn ai_section(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    visible_api_keys: &[AiProvider],
) -> UiNode {
    let mut section = section_frame(palette, "settings.ai_providers")
        .with_child(segment_row(
            palette,
            "settings.ai_provider_default",
            None,
            &[
                segment(
                    "settings.default-provider.openrouter",
                    "settings.value.openrouter",
                    settings.default_ai_provider == AiProvider::OpenRouter,
                ),
                segment(
                    "settings.default-provider.openai",
                    "settings.value.openai",
                    settings.default_ai_provider == AiProvider::OpenAI,
                ),
            ],
        ))
        .with_child(segment_row(
            palette,
            "settings.agent_mode",
            Some("settings.agent_mode_desc"),
            &[
                segment(
                    "settings.agent-mode.passive",
                    "settings.agent_mode_passive",
                    settings.agent_mode == AgentMode::Passive,
                ),
                segment(
                    "settings.agent-mode.active",
                    "settings.agent_mode_active",
                    settings.agent_mode == AgentMode::Active,
                ),
            ],
        ));

    if settings.agent_mode == AgentMode::Active {
        section = section.with_child(
            UiNode::new("settings.agent-warning", UiNodeKind::Panel)
                .with_class("settings-warning")
                .with_layout(UiLayout {
                    padding: UiSpacing::same(10.0),
                    ..UiLayout::fixed(0.0, 42.0)
                })
                .with_child(
                    UiNode::new("settings.agent-warning-copy", UiNodeKind::Label)
                        .with_text_key("app.agent_active_mode_warning")
                        .with_text_style(UiTextStyle::body(palette.tokens().text))
                        .with_layout(UiLayout::fixed(0.0, 20.0)),
                ),
        );
    }

    section = section
        .with_child(section_divider(palette, "settings.ai-shortcuts-divider"))
        .with_child(section_label(palette, "settings.ai_models_title"));
    if settings.agent_model_shortcuts.is_empty() {
        section = section.with_child(
            UiNode::new("settings.ai-shortcuts-empty", UiNodeKind::Label)
                .with_text_key("settings.surface.shortcuts_empty")
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
                .with_layout(UiLayout::fixed(0.0, 24.0)),
        );
    } else {
        for (index, shortcut) in settings.agent_model_shortcuts.iter().enumerate() {
            let label = format!(
                "{} | {} | {}",
                shortcut.label,
                shortcut.model_id,
                shortcut.provider.display_name()
            );
            section = section.with_child(
                UiNode::new(format!("settings.shortcut.{index}"), UiNodeKind::Panel)
                    .with_class("settings-shortcut")
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        align_items: UiAlign::Center,
                        padding: UiSpacing::xy(10.0, 0.0),
                        gap: 8.0,
                        ..UiLayout::fixed(0.0, 36.0)
                    })
                    .with_child(
                        UiNode::new(
                            format!("settings.shortcut.{index}.label"),
                            UiNodeKind::Label,
                        )
                        .with_text_key(label)
                        .with_text_style(UiTextStyle::body(palette.tokens().text))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::default()
                        }),
                    )
                    .with_child(command_button(
                        format!("settings.shortcut.{index}.remove"),
                        "settings.surface.remove",
                        format!("settings.shortcut.remove.{index}"),
                        74.0,
                        "settings-secondary-button",
                        palette.tokens().text,
                    )),
            );
        }
    }

    let providers: Vec<AiProvider> = if settings.simple_mode {
        vec![settings.default_ai_provider]
    } else {
        AiProvider::editor_supported().to_vec()
    };
    for provider in providers {
        let config = settings
            .ai_providers
            .iter()
            .find(|config| config.provider == provider)
            .cloned()
            .unwrap_or_else(|| AiProviderConfig::for_provider(provider));
        section = section.with_child(ai_provider_card(
            palette,
            settings.language,
            config,
            visible_api_keys.contains(&provider),
            settings.default_ai_provider == provider,
        ));
    }
    section
}

fn platform_section(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    let platform_help = match settings.target_platform {
        TargetPlatform::Desktop => "settings.platform.desktop",
        TargetPlatform::Mobile => "settings.platform.mobile",
        TargetPlatform::Web => "settings.platform.web",
        TargetPlatform::Cloud => "settings.platform.cloud",
        TargetPlatform::Console => "settings.platform.console",
    };
    section_frame(palette, "settings.target_platform")
        .with_child(segment_row(
            palette,
            "settings.platform",
            Some(platform_help),
            &[
                segment(
                    "settings.platform.desktop",
                    "settings.value.desktop",
                    settings.target_platform == TargetPlatform::Desktop,
                ),
                segment(
                    "settings.platform.mobile",
                    "settings.value.mobile",
                    settings.target_platform == TargetPlatform::Mobile,
                ),
                segment(
                    "settings.platform.web",
                    "settings.value.web",
                    settings.target_platform == TargetPlatform::Web,
                ),
                segment(
                    "settings.platform.cloud",
                    "settings.value.cloud",
                    settings.target_platform == TargetPlatform::Cloud,
                ),
                segment(
                    "settings.platform.console",
                    "settings.value.console",
                    settings.target_platform == TargetPlatform::Console,
                ),
            ],
        ))
        .with_child(toggle_row(
            palette,
            "settings.responsive-layout",
            "settings.responsive_layout",
            None,
            "settings.responsive-layout",
            settings.responsive_layout,
        ))
        .with_child(toggle_row(
            palette,
            "settings.headless",
            "settings.headless",
            None,
            "settings.headless",
            settings.headless,
        ))
}

fn ai_provider_card(
    palette: StudioUiPalette,
    lang: Language,
    config: AiProviderConfig,
    show_key: bool,
    is_default: bool,
) -> UiNode {
    let provider = provider_id(config.provider);
    let mut card = UiNode::new(format!("settings.provider.{provider}"), UiNodeKind::Panel)
        .with_class(if is_default {
            "settings-provider-default"
        } else {
            "settings-provider"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(14.0),
            gap: 8.0,
            ..UiLayout::fixed(0.0, 306.0)
        })
        .with_child(
            UiNode::new(
                format!("settings.provider.{provider}.head"),
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                ..UiLayout::fixed(0.0, 22.0)
            })
            .with_style(UiStyle::transparent())
            .with_child(
                UiNode::new(
                    format!("settings.provider.{provider}.name"),
                    UiNodeKind::Label,
                )
                .with_text_key(config.provider.display_name())
                .with_text_style(UiTextStyle::panel_title(palette.tokens().text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::default()
                }),
            )
            .with_child(toggle_control(
                palette,
                format!("settings.provider.{provider}.enabled"),
                format!("settings.ai.{provider}.enabled"),
                config.enabled,
                false,
            )),
        )
        .with_child(
            UiNode::new(
                format!("settings.provider.{provider}.description"),
                UiNodeKind::Label,
            )
            .with_text_key(if lang == Language::Spanish {
                config.provider.description_es()
            } else {
                config.provider.description()
            })
            .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
            .with_layout(UiLayout::fixed(0.0, 20.0)),
        )
        .with_child(text_input_row(
            palette,
            "settings.ai_provider_base_url",
            None,
            format!("settings.ai.{provider}.base-url"),
            false,
            false,
        ))
        .with_child(text_input_row(
            palette,
            "settings.ai_provider_model",
            None,
            format!("settings.ai.{provider}.model"),
            false,
            false,
        ))
        .with_child(form_row_with_id(
            palette,
            &format!("settings.provider.{provider}.api-key"),
            "settings.ai_provider_api_key",
            None,
            UiNode::new(
                format!("settings.provider.{provider}.key-control"),
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 6.0,
                ..UiLayout::fixed(0.0, CONTROL_HEIGHT)
            })
            .with_style(UiStyle::transparent())
            .with_child(text_input_control(
                palette,
                format!("settings.ai.{provider}.api-key"),
                !show_key,
                false,
            ))
            .with_child(command_button(
                format!("settings.provider.{provider}.key-visibility"),
                if show_key {
                    "settings.ai_provider_hide"
                } else {
                    "settings.ai_provider_show"
                },
                format!("settings.provider.toggle-key.{provider}"),
                68.0,
                "settings-secondary-button",
                palette.tokens().text,
            )),
        ));
    if !is_default {
        card = card.with_child(command_button(
            format!("settings.provider.{provider}.default"),
            "settings.ai_provider_set_default",
            format!("settings.provider.default.{provider}"),
            148.0,
            "settings-secondary-button",
            palette.tokens().text,
        ));
    }
    card
}

fn section_frame(_palette: StudioUiPalette, id: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("settings-section")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 10.0,
            padding: UiSpacing::ZERO,
            ..UiLayout::default()
        })
        .with_style(UiStyle::transparent())
}

fn section_divider(palette: StudioUiPalette, id: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Separator)
        .with_layout(UiLayout {
            basis: [0.0, 1.0],
            ..UiLayout::default()
        })
        .with_style(UiStyle {
            fill: palette.tokens().border,
            border: [0, 0, 0, 0],
            text: palette.tokens().text,
            border_width: 0.0,
            radius: 0.0,
            opacity: 1.0,
        })
}

fn section_label(palette: StudioUiPalette, label_key: &str) -> UiNode {
    UiNode::new(format!("settings.label.{label_key}"), UiNodeKind::Label)
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::panel_title(palette.tokens().text))
        .with_layout(UiLayout::fixed(0.0, 22.0))
}

fn form_row(
    palette: StudioUiPalette,
    label_key: &str,
    help_key: Option<&str>,
    control: UiNode,
) -> UiNode {
    form_row_with_id(palette, label_key, label_key, help_key, control)
}

fn form_row_with_id(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    help_key: Option<&str>,
    control: UiNode,
) -> UiNode {
    let tokens = palette.tokens();
    let row_height = if help_key.is_some() { 66.0 } else { 46.0 };
    let compact_height = if help_key.is_some() { 106.0 } else { 70.0 };
    let mut label = UiNode::new(format!("{id}.copy"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            basis: [280.0, 0.0],
            min_size: [200.0, 0.0],
            gap: 2.0,
            ..UiLayout::default()
        })
        .with_style(UiStyle::transparent())
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 18.0)),
        );
    if let Some(help_key) = help_key {
        label = label.with_child(
            UiNode::new(format!("{id}.help"), UiNodeKind::Label)
                .with_text_key(help_key)
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 34.0)),
        );
    }
    UiNode::new(format!("{id}.row"), UiNodeKind::Panel)
        .with_class("settings-row")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            basis: [0.0, row_height],
            min_size: [0.0, row_height],
            align_items: UiAlign::Center,
            gap: 24.0,
            padding: UiSpacing::xy(0.0, 6.0),
            compact: UiCompactMode::Stack,
            responsive: vec![UiResponsiveRule {
                max_width: 720.0,
                flow: Some(UiFlow::Column),
                basis: Some([0.0, compact_height]),
                padding: None,
                gap: Some(6.0),
                compact: Some(UiCompactMode::Stack),
                grid_columns: None,
            }],
            ..UiLayout::default()
        })
        .with_style(UiStyle::transparent())
        .with_child(label)
        .with_child(
            UiNode::new(format!("{id}.control"), UiNodeKind::Panel)
                .with_layout(UiLayout {
                    grow: 1.0,
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    ..UiLayout::default()
                })
                .with_style(UiStyle::transparent())
                .with_child(control),
        )
}

fn toggle_row(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    help_key: Option<&str>,
    value_key: &str,
    value: bool,
) -> UiNode {
    form_row(
        palette,
        label_key,
        help_key,
        toggle_control(palette, id, value_key, value, false),
    )
}

fn toggle_control(
    palette: StudioUiPalette,
    id: impl Into<String>,
    value_key: impl Into<String>,
    value: bool,
    disabled: bool,
) -> UiNode {
    let id = id.into();
    let tokens = palette.tokens();
    UiNode::toggle(id.clone(), UiToggle::new(value_key, value))
        .with_class(if value {
            "settings-toggle-on"
        } else {
            "settings-toggle"
        })
        .disabled(disabled)
        .with_layout(UiLayout {
            flow: UiFlow::None,
            ..UiLayout::fixed(42.0, 24.0)
        })
        .with_child(
            UiNode::new(format!("{id}.knob"), UiNodeKind::Panel)
                .with_layout(UiLayout::absolute(
                    raf_render::api_graphic_basic::ui_surface::UiRect::new(
                        if value { 21.0 } else { 3.0 },
                        3.0,
                        18.0,
                        18.0,
                    ),
                ))
                .with_style(UiStyle {
                    fill: if value {
                        [255, 255, 255, 255]
                    } else {
                        tokens.text_muted
                    },
                    border: [0, 0, 0, 0],
                    text: tokens.text,
                    border_width: 0.0,
                    radius: 9.0,
                    opacity: 1.0,
                }),
        )
}

fn range_row(
    palette: StudioUiPalette,
    label_key: &str,
    help_key: Option<&str>,
    value_key: &str,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    value_label: String,
    disabled: bool,
) -> UiNode {
    let control = UiNode::new(format!("{value_key}.range-control"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            grow: 1.0,
            basis: [0.0, CONTROL_HEIGHT],
            min_size: [RANGE_WIDTH + 86.0, CONTROL_HEIGHT],
            align_items: UiAlign::Center,
            gap: 12.0,
            ..UiLayout::default()
        })
        .with_style(UiStyle::transparent())
        .with_child(range_control(
            palette, value_key, value, min, max, step, disabled,
        ))
        .with_child(
            UiNode::new(format!("{value_key}.value"), UiNodeKind::Label)
                .with_text_key(value_label)
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
                .with_layout(UiLayout::fixed(74.0, 20.0)),
        );
    form_row(palette, label_key, help_key, control)
}

fn range_control(
    palette: StudioUiPalette,
    value_key: &str,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    disabled: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let range = UiRange::new(value_key, value, min, max, step);
    let fraction = range.fraction();
    let track_width = RANGE_WIDTH - 14.0;
    let fill_width = (track_width * fraction).max(2.0);
    let thumb_x = (track_width * fraction).clamp(0.0, track_width) + 3.0;
    UiNode::range(format!("{value_key}.range"), range)
        .with_class("settings-range")
        .disabled(disabled)
        .with_layout(UiLayout {
            flow: UiFlow::None,
            ..UiLayout::fixed(RANGE_WIDTH, CONTROL_HEIGHT)
        })
        .with_child(
            UiNode::new(format!("{value_key}.track"), UiNodeKind::Panel)
                .with_layout(UiLayout::absolute(
                    raf_render::api_graphic_basic::ui_surface::UiRect::new(
                        4.0,
                        15.0,
                        track_width,
                        4.0,
                    ),
                ))
                .with_class("settings-range-track"),
        )
        .with_child(
            UiNode::new(format!("{value_key}.fill"), UiNodeKind::Panel)
                .with_layout(UiLayout::absolute(
                    raf_render::api_graphic_basic::ui_surface::UiRect::new(
                        4.0, 15.0, fill_width, 4.0,
                    ),
                ))
                .with_class("settings-range-fill"),
        )
        .with_child(
            UiNode::new(format!("{value_key}.thumb"), UiNodeKind::Panel)
                .with_layout(UiLayout::absolute(
                    raf_render::api_graphic_basic::ui_surface::UiRect::new(
                        thumb_x, 9.0, 16.0, 16.0,
                    ),
                ))
                .with_style(UiStyle {
                    fill: [255, 255, 255, 255],
                    border: tokens.border,
                    text: tokens.text,
                    border_width: 1.0,
                    radius: 8.0,
                    opacity: 1.0,
                }),
        )
}

#[derive(Debug, Clone, Copy)]
struct Segment<'a> {
    command: &'a str,
    label_key: &'a str,
    selected: bool,
}

fn segment<'a>(command: &'a str, label_key: &'a str, selected: bool) -> Segment<'a> {
    Segment {
        command,
        label_key,
        selected,
    }
}

fn segment_row(
    palette: StudioUiPalette,
    label_key: &str,
    help_key: Option<&str>,
    segments: &[Segment<'_>],
) -> UiNode {
    let tokens = palette.tokens();
    let mut control = UiNode::new(format!("{label_key}.segments"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::RowWrap,
            grow: 1.0,
            min_size: [0.0, CONTROL_HEIGHT],
            align_items: UiAlign::Center,
            gap: 6.0,
            compact: UiCompactMode::Wrap,
            ..UiLayout::default()
        })
        .with_style(UiStyle::transparent());
    for segment in segments {
        control = control.with_child(command_button(
            format!("{label_key}.{}", segment.command),
            segment.label_key,
            segment.command,
            0.0,
            if segment.selected {
                "settings-segment-active"
            } else {
                "settings-segment"
            },
            if segment.selected {
                SELECTED_TEXT_ORANGE
            } else {
                tokens.text
            },
        ));
    }
    form_row(palette, label_key, help_key, control)
}

fn text_input_row(
    palette: StudioUiPalette,
    label_key: &str,
    help_key: Option<&str>,
    value_key: impl Into<String>,
    password: bool,
    disabled: bool,
) -> UiNode {
    let value_key = value_key.into();
    form_row_with_id(
        palette,
        &value_key,
        label_key,
        help_key,
        text_input_control(palette, value_key.clone(), password, disabled),
    )
}

fn text_input_control(
    palette: StudioUiPalette,
    value_key: impl Into<String>,
    password: bool,
    disabled: bool,
) -> UiNode {
    let value_key = value_key.into();
    UiNode::text_input(
        format!("{value_key}.input"),
        UiTextInput {
            value_key,
            placeholder_key: None,
            max_length: 2_048,
            multiline: false,
            password,
            submit_command: None,
        },
    )
    .with_class("settings-input")
    .disabled(disabled)
    .with_layout(UiLayout::fixed(248.0, CONTROL_HEIGHT))
    .with_style(palette.subtle_panel_style())
}

fn command_button(
    id: impl Into<String>,
    label_key: &str,
    command: impl Into<String>,
    width: f32,
    class: &str,
    color: [u8; 4],
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button(color))
        .with_class(class)
        .with_layout(UiLayout {
            basis: [width, CONTROL_HEIGHT],
            min_size: [if width > 0.0 { width } else { 70.0 }, CONTROL_HEIGHT],
            padding: UiSpacing::xy(10.0, 0.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action: UiAction::Command {
                name: command.into(),
            },
        })
}

fn section_help_key(section: SettingsSection) -> &'static str {
    match section {
        SettingsSection::Appearance => "settings.surface.help.appearance",
        SettingsSection::Performance => "settings.surface.help.performance",
        SettingsSection::Editor => "settings.surface.help.editor",
        SettingsSection::Viewport => "settings.surface.help.viewport",
        SettingsSection::Scripting => "settings.surface.help.scripting",
        SettingsSection::Ai => "settings.surface.help.ai",
        SettingsSection::Platform => "settings.surface.help.platform",
    }
}

fn settings_style_sheet(
    palette: StudioUiPalette,
    tokens: raf_render::api_graphic_basic::ui_surface::UiTokens,
) -> UiStyleSheet {
    let toggle_fill = match palette {
        StudioUiPalette::IndustrialDark => [58, 67, 77, 255],
        StudioUiPalette::PaperLight => [190, 194, 198, 255],
    };
    UiStyleSheet {
        rules: vec![
            style_rule(
                "settings-navigation",
                tokens.surface,
                tokens.border,
                1.0,
                0.0,
            ),
            style_rule(
                "settings-workspace",
                tokens.background,
                [0, 0, 0, 0],
                0.0,
                0.0,
            ),
            style_rule("settings-header", tokens.surface, tokens.border, 1.0, 0.0),
            style_rule("settings-footer", tokens.surface, tokens.border, 1.0, 0.0),
            style_rule("settings-nav-button", [0, 0, 0, 0], [0, 0, 0, 0], 0.0, 4.0),
            style_rule(
                "settings-nav-active",
                tokens.selection,
                tokens.accent,
                1.0,
                4.0,
            ),
            style_rule(
                "settings-primary-button",
                tokens.accent,
                tokens.accent_hot,
                1.0,
                4.0,
            ),
            style_rule(
                "settings-secondary-button",
                tokens.surface_alt,
                tokens.border,
                1.0,
                4.0,
            ),
            style_rule(
                "settings-segment",
                tokens.surface_alt,
                tokens.border,
                1.0,
                4.0,
            ),
            style_rule(
                "settings-segment-active",
                tokens.accent,
                tokens.accent_hot,
                1.0,
                4.0,
            ),
            style_rule(
                "settings-input",
                tokens.surface_raised,
                tokens.border,
                1.0,
                4.0,
            ),
            style_rule("settings-toggle", toggle_fill, tokens.border, 1.0, 12.0),
            style_rule(
                "settings-toggle-on",
                tokens.accent,
                tokens.accent_hot,
                1.0,
                12.0,
            ),
            style_rule(
                "settings-range-track",
                tokens.surface_alt,
                [0, 0, 0, 0],
                0.0,
                2.0,
            ),
            style_rule(
                "settings-range-fill",
                [255, 255, 255, 255],
                [0, 0, 0, 0],
                0.0,
                2.0,
            ),
            style_rule("settings-provider", tokens.surface, tokens.border, 1.0, 6.0),
            style_rule(
                "settings-provider-default",
                tokens.surface,
                tokens.accent,
                1.0,
                6.0,
            ),
            style_rule(
                "settings-shortcut",
                tokens.surface_alt,
                tokens.border,
                1.0,
                4.0,
            ),
            style_rule(
                "settings-warning",
                tokens.surface_alt,
                tokens.warning,
                1.0,
                4.0,
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-nav-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-secondary-button".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    fill: Some(tokens.surface_raised),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-range".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
        ],
    }
}

fn style_rule(
    class: &str,
    fill: [u8; 4],
    border: [u8; 4],
    border_width: f32,
    radius: f32,
) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            border: Some(border),
            border_width: Some(border_width),
            radius: Some(radius),
            ..UiStylePatch::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_render::api_graphic_basic::ui_surface::UiControl;

    fn find_node<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if node.id == id {
            return Some(node);
        }
        node.children.iter().find_map(|child| find_node(child, id))
    }

    #[test]
    fn settings_surface_keeps_navigation_and_draft_controls_retained() {
        let settings = EngineSettings::default();
        let surface = build_settings_surface(
            StudioUiPalette::IndustrialDark,
            &settings,
            SettingsSection::Appearance,
            &[],
        );

        assert!(find_node(&surface.root, "settings.navigation").is_some());
        assert!(find_node(&surface.root, "settings.content").is_some());
        assert!(matches!(
            find_node(&surface.root, "settings.simple-mode")
                .expect("simple mode toggle")
                .control,
            UiControl::Toggle(_)
        ));
        assert!(
            find_node(&surface.root, "settings.ui-scale.range")
                .expect("manual scale range")
                .disabled
        );
    }

    #[test]
    fn settings_surface_preserves_the_legacy_engine_setting_controls() {
        let settings = EngineSettings::default();
        let cases: &[(SettingsSection, &[&str])] = &[
            (
                SettingsSection::Appearance,
                &[
                    "settings.simple-mode",
                    "settings.theme-experimental.range",
                    "settings.font-size.range",
                    "settings.ui-scale.range",
                ],
            ),
            (
                SettingsSection::Performance,
                &[
                    "settings.fps-unlimited",
                    "settings.fps-limit.range",
                    "settings.show-fps-counter",
                    "settings.vsync",
                    "settings.multithreading",
                ],
            ),
            (
                SettingsSection::Editor,
                &[
                    "settings.show-grid",
                    "settings.snap-grid",
                    "settings.grid-size.range",
                    "settings.grid-load-distance.range",
                    "settings.auto-save.range",
                    "settings.command-console",
                ],
            ),
            (
                SettingsSection::Viewport,
                &[
                    "settings.viewport-labels",
                    "settings.focus-lock",
                    "settings.solid-edges",
                    "settings.solid-xray",
                    "settings.solid-tonality",
                    "settings.invert-x",
                    "settings.invert-y",
                    "settings.invert-ws",
                    "settings.wasd-speed.range",
                    "settings.move-sensitivity.range",
                    "settings.rotate-sensitivity.range",
                    "settings.scale-sensitivity.range",
                    "settings.uniform-scale",
                    "settings.gizmo-growth.range",
                ],
            ),
            (
                SettingsSection::Scripting,
                &[
                    "settings.script-runtime",
                    "settings.script-hot-reload",
                    "settings.script-timeout.range",
                    "settings.script-external-editor.input",
                ],
            ),
            (
                SettingsSection::Ai,
                &[
                    "settings.ai_provider_default.settings.default-provider.openrouter",
                    "settings.ai_provider_default.settings.default-provider.openai",
                    "settings.agent_mode.settings.agent-mode.passive",
                    "settings.agent_mode.settings.agent-mode.active",
                    "settings.provider.openrouter.enabled",
                    "settings.provider.openai.enabled",
                ],
            ),
            (
                SettingsSection::Platform,
                &[
                    "settings.responsive-layout",
                    "settings.headless",
                    "settings.platform.settings.platform.desktop",
                    "settings.platform.settings.platform.mobile",
                    "settings.platform.settings.platform.web",
                    "settings.platform.settings.platform.cloud",
                    "settings.platform.settings.platform.console",
                ],
            ),
        ];

        for (section, expected_ids) in cases {
            let surface =
                build_settings_surface(StudioUiPalette::IndustrialDark, &settings, *section, &[]);
            for id in *expected_ids {
                assert!(
                    find_node(&surface.root, id).is_some(),
                    "missing retained Settings control {id} in {section:?}"
                );
            }
        }
    }

    #[test]
    fn composite_setting_controls_reserve_the_available_form_column() {
        let settings = EngineSettings::default();
        let performance = build_settings_surface(
            StudioUiPalette::IndustrialDark,
            &settings,
            SettingsSection::Performance,
            &[],
        );
        let appearance = build_settings_surface(
            StudioUiPalette::IndustrialDark,
            &settings,
            SettingsSection::Appearance,
            &[],
        );

        let range = find_node(&performance.root, "settings.fps-limit.range-control")
            .expect("FPS slider control exists");
        let segments =
            find_node(&appearance.root, "settings.theme.segments").expect("theme segments exist");

        assert_eq!(range.layout.grow, 1.0);
        assert!(range.layout.min_size[0] >= RANGE_WIDTH);
        assert_eq!(segments.layout.grow, 1.0);
    }
}
