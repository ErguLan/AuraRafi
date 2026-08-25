//! RafUI presentation for engine-wide settings.
//!
//! This module deliberately owns only composition. Draft state, validation and
//! persistence stay in SettingsSurfaceHost.

use raf_core::ai::{AgentMode, AiProvider};
use raf_core::config::{
    EngineSettings, Language, RenderExecutionPolicy, RenderQuality, ScriptLanguage, TargetPlatform,
    Theme, ViewportRenderMode,
};
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiAlign, UiCompactMode, UiEventBinding, UiEventKind, UiFlow,
    UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiRange, UiScrollAxis, UiSizeMode,
    UiSpacing, UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet,
    UiSurface, UiTextInput, UiTextStyle, UiToggle,
};

const CONTROL_WIDTH: f32 = 360.0;
const SEGMENT_MIN_WIDTH: f32 = 68.0;
const SETTINGS_ROW_HEIGHT: f32 = 56.0;
const ACCENT: [u8; 4] = [232, 133, 28, 255];
const AI_CARD_GAP: f32 = 10.0;

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

    pub const fn command(self) -> &'static str {
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

    pub const fn key(self) -> &'static str {
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

    pub const fn help_key(self) -> &'static str {
        match self {
            Self::Appearance => "settings.surface.help.appearance",
            Self::Performance => "settings.surface.help.performance",
            Self::Editor => "settings.surface.help.editor",
            Self::Viewport => "settings.surface.help.viewport",
            Self::Scripting => "settings.surface.help.scripting",
            Self::Ai => "settings.surface.help.ai",
            Self::Platform => "settings.surface.help.platform",
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

pub fn build_settings_surface(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    section: SettingsSection,
) -> UiSurface {
    build_settings_surface_with_api_keys(palette, settings, section, &[])
}

pub fn build_settings_surface_with_api_keys(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    section: SettingsSection,
    revealed_api_keys: &[AiProvider],
) -> UiSurface {
    let root = UiNode::new("settings.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            compact: UiCompactMode::Stack,
            responsive: vec![raf_ui::UiResponsiveRule {
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
        .with_child(navigation(palette, section))
        .with_child(workspace(palette, settings, section, revealed_api_keys));

    let mut surface = UiSurface::new("editor.engine-settings", palette, root);
    surface.style_sheet = settings_style_sheet(palette);
    surface
}

fn navigation(palette: StudioUiPalette, active: SettingsSection) -> UiNode {
    let tokens = palette.tokens();
    let mut node = UiNode::scroll_view("settings.navigation", UiScrollAxis::Vertical)
        .with_class("settings-navigation")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            basis: [224.0, 0.0],
            width_mode: UiSizeMode::Fixed,
            height_mode: UiSizeMode::Fill,
            min_size: [0.0, 0.0],
            padding: UiSpacing::same(16.0),
            gap: 4.0,
            overflow: UiOverflow::ScrollY,
            responsive: vec![raf_ui::UiResponsiveRule {
                max_width: 760.0,
                flow: Some(UiFlow::Column),
                basis: Some([0.0, 0.0]),
                padding: Some(UiSpacing::same(12.0)),
                gap: Some(4.0),
                compact: Some(UiCompactMode::Stack),
                grid_columns: None,
            }],
            ..UiLayout::default()
        })
        .with_child(
            UiNode::new("settings.navigation.title", UiNodeKind::Label)
                .with_text_key("app.engine_settings_title")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 26.0)),
        )
        .with_child(
            UiNode::new("settings.navigation.subtitle", UiNodeKind::Label)
                .with_text_key("settings.surface.subtitle")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 34.0)),
        )
        .with_child(
            UiNode::new("settings.navigation.divider", UiNodeKind::Separator)
                .with_layout(UiLayout::fixed(0.0, 1.0)),
        );

    for item in SettingsSection::ALL {
        node = node.with_child(
            UiNode::new(
                format!("settings.navigation.{}", item.key()),
                UiNodeKind::Button,
            )
            .with_class(if item == active {
                "settings-nav-active"
            } else {
                "settings-nav-button"
            })
            .with_layout(UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill))
            .with_text_key(item.key())
            .with_text_style(UiTextStyle::button(if item == active {
                ACCENT
            } else {
                tokens.text_muted
            }))
            .focusable()
            .with_event(UiEventBinding::command(UiEventKind::Click, item.command())),
        );
    }

    node.with_child(
        UiNode::new("settings.navigation.spacer", UiNodeKind::Panel).with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::default()
        }),
    )
}

fn workspace(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    section: SettingsSection,
    revealed_api_keys: &[AiProvider],
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("settings.workspace", UiNodeKind::Panel)
        .with_class("settings-workspace")
        .with_layout(UiLayout::fill(UiFlow::Column))
        .with_child(
            UiNode::new("settings.workspace.header", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 3.0,
                    padding: UiSpacing::xy(28.0, 15.0),
                    responsive: vec![raf_ui::UiResponsiveRule {
                        max_width: 760.0,
                        flow: Some(UiFlow::Column),
                        basis: Some([0.0, 76.0]),
                        padding: Some(UiSpacing::xy(12.0, 12.0)),
                        gap: Some(3.0),
                        compact: Some(UiCompactMode::Stack),
                        grid_columns: None,
                    }],
                    ..UiLayout::fixed(0.0, 76.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("settings.workspace.title", UiNodeKind::Label)
                        .with_text_key(section.key())
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout::fixed(0.0, 22.0)),
                )
                .with_child(
                    UiNode::new("settings.workspace.help", UiNodeKind::Label)
                        .with_text_key(section.help_key())
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fixed(0.0, 18.0)),
                ),
        )
        .with_child(
            UiNode::scroll_view("settings.workspace.scroll", UiScrollAxis::Vertical)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    grow: 1.0,
                    padding: UiSpacing::xy(28.0, 22.0),
                    overflow: UiOverflow::ScrollY,
                    responsive: vec![raf_ui::UiResponsiveRule {
                        max_width: 760.0,
                        flow: Some(UiFlow::Column),
                        basis: Some([0.0, 0.0]),
                        padding: Some(UiSpacing::xy(12.0, 14.0)),
                        gap: Some(10.0),
                        compact: Some(UiCompactMode::Stack),
                        grid_columns: None,
                    }],
                    ..UiLayout::fill(UiFlow::Column)
                })
                .with_child(section_content(
                    palette,
                    settings,
                    section,
                    revealed_api_keys,
                )),
        )
        .with_child(
            UiNode::new("settings.workspace.footer", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    justify_content: UiJustify::End,
                    align_items: UiAlign::Center,
                    gap: 8.0,
                    padding: UiSpacing::xy(28.0, 0.0),
                    responsive: vec![raf_ui::UiResponsiveRule {
                        max_width: 760.0,
                        flow: Some(UiFlow::Row),
                        basis: Some([0.0, 58.0]),
                        padding: Some(UiSpacing::xy(12.0, 0.0)),
                        gap: Some(6.0),
                        compact: Some(UiCompactMode::Wrap),
                        grid_columns: None,
                    }],
                    ..UiLayout::fixed(0.0, 62.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(action_button(
                    "settings.cancel",
                    "app.cancel",
                    "settings.cancel",
                    false,
                ))
                .with_child(action_button(
                    "settings.save",
                    "app.save_and_close",
                    "settings.save",
                    true,
                )),
        )
}

fn section_content(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    section: SettingsSection,
    revealed_api_keys: &[AiProvider],
) -> UiNode {
    match section {
        SettingsSection::Appearance => appearance(palette, settings),
        SettingsSection::Performance => performance(palette, settings),
        SettingsSection::Editor => editor(palette, settings),
        SettingsSection::Viewport => viewport(palette, settings),
        SettingsSection::Scripting => scripting(palette, settings),
        SettingsSection::Ai => ai(palette, settings, revealed_api_keys),
        SettingsSection::Platform => platform(palette, settings),
    }
}

fn appearance(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    card(palette, "settings.appearance")
        .with_child(toggle_row(
            palette,
            "settings.simple-mode",
            "settings.simple_mode",
            settings.simple_mode,
        ))
        .with_child(segment_row(
            palette,
            "settings.theme",
            "settings.theme",
            &[
                (
                    "settings.value.dark",
                    "settings.theme.dark",
                    settings.theme == Theme::Dark,
                ),
                (
                    "settings.value.light",
                    "settings.theme.light",
                    settings.theme == Theme::Light,
                ),
                (
                    "settings.value.system",
                    "settings.theme.system",
                    settings.theme == Theme::System,
                ),
            ],
        ))
        .with_child(range_row(
            palette,
            "settings.theme-experimental",
            "settings.theme_experimental",
            settings.theme_experimental,
            0.0,
            100.0,
            1.0,
            format!("{:.0}%", settings.theme_experimental),
        ))
        .with_child(range_row(
            palette,
            "settings.font_size",
            "settings.font_size",
            settings.font_size,
            10.0,
            24.0,
            1.0,
            format!("{:.0} px", settings.font_size),
        ))
        .with_child(toggle_row(
            palette,
            "settings.auto-ui-scale",
            "settings.auto_ui_scale",
            settings.auto_ui_scale,
        ))
        .with_child(range_row_disabled(
            palette,
            "settings.ui_scale",
            "settings.ui_scale",
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
            "settings.language",
            &[
                (
                    "settings.value.english",
                    "settings.language.english",
                    settings.language == Language::English,
                ),
                (
                    "settings.value.spanish",
                    "settings.language.spanish",
                    settings.language == Language::Spanish,
                ),
            ],
        ))
}

fn performance(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    card(palette, "settings.performance")
        .with_child(segment_row(
            palette,
            "settings.quality",
            "settings.quality",
            &[
                (
                    "settings.value.potato",
                    "settings.quality.potato",
                    settings.render_quality == RenderQuality::Potato,
                ),
                (
                    "settings.value.low",
                    "settings.quality.low",
                    settings.render_quality == RenderQuality::Low,
                ),
                (
                    "settings.value.medium",
                    "settings.quality.medium",
                    settings.render_quality == RenderQuality::Medium,
                ),
                (
                    "settings.value.high",
                    "settings.quality.high",
                    settings.render_quality == RenderQuality::High,
                ),
            ],
        ))
        .with_child(segment_row(
            palette,
            "settings.render_execution_policy",
            "settings.render_execution_policy",
            &[
                (
                    "settings.render_execution_policy.auto",
                    "settings.policy.auto",
                    settings.render_execution_policy == RenderExecutionPolicy::Auto,
                ),
                (
                    "settings.render_execution_policy.cpu_only",
                    "settings.policy.cpu_only",
                    settings.render_execution_policy == RenderExecutionPolicy::CpuOnly,
                ),
                (
                    "settings.render_execution_policy.gpu_preferred",
                    "settings.policy.gpu_preferred",
                    settings.render_execution_policy == RenderExecutionPolicy::GpuPreferred,
                ),
            ],
        ))
        .with_child(toggle_row(
            palette,
            "settings.vsync",
            "settings.vsync",
            settings.vsync,
        ))
        .with_child(toggle_row(
            palette,
            "settings.multithreading",
            "settings.multithreading",
            settings.multithreading,
        ))
        .with_child(toggle_row(
            palette,
            "settings.show-fps-counter",
            "settings.show_fps_counter",
            settings.show_fps_counter,
        ))
        .with_child(toggle_row(
            palette,
            "settings.fps-unlimited",
            "settings.fps_unlimited",
            settings.fps_limit == 0,
        ))
        .with_child(range_row_disabled(
            palette,
            "settings.fps_limit",
            "settings.fps_limit",
            settings.fps_limit.max(15) as f32,
            15.0,
            240.0,
            1.0,
            format!("{} fps", settings.fps_limit),
            settings.fps_limit == 0,
        ))
}

fn editor(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    card(palette, "settings.editor")
        .with_child(toggle_row(
            palette,
            "settings.show-grid",
            "settings.show_grid",
            settings.grid_visible,
        ))
        .with_child(toggle_row(
            palette,
            "settings.snap-grid",
            "settings.snap_to_grid",
            settings.snap_to_grid,
        ))
        .with_child(toggle_row(
            palette,
            "settings.inspector-live-transform-updates",
            "settings.inspector_live_transform_updates",
            settings.inspector_live_transform_updates,
        ))
        .with_child(range_row_disabled(
            palette,
            "settings.grid_size",
            "settings.grid_size",
            settings.grid_size,
            0.1,
            10.0,
            0.1,
            format!("{:.1} m", settings.grid_size),
            settings.simple_mode,
        ))
        .with_child(range_row_disabled(
            palette,
            "settings.grid_load_distance",
            "settings.grid_load_distance",
            settings.grid_load_distance,
            0.0,
            500.0,
            0.5,
            format!("{:.1} m", settings.grid_load_distance),
            settings.simple_mode,
        ))
        .with_child(range_row(
            palette,
            "settings.electronics-grid-step",
            "settings.electronics_grid_step_mm",
            settings.electronics_grid_step_mm,
            5.0,
            100.0,
            1.0,
            format!("{:.0} mm", settings.electronics_grid_step_mm),
        ))
        .with_child(range_row(
            palette,
            "settings.electronics-grid-opacity",
            "settings.electronics_grid_opacity",
            settings.electronics_grid_opacity,
            0.2,
            1.0,
            0.05,
            format!("{:.0}%", settings.electronics_grid_opacity * 100.0),
        ))
        .with_child(range_row_disabled(
            palette,
            "settings.auto-save",
            "settings.auto_save",
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
            "settings.units",
            &[
                (
                    "settings.metric",
                    "settings.units.metric",
                    settings.display_unit == raf_core::units::DisplayUnit::Metric,
                ),
                (
                    "settings.imperial",
                    "settings.units.imperial",
                    settings.display_unit == raf_core::units::DisplayUnit::Imperial,
                ),
            ],
        ))
        .with_child(toggle_row(
            palette,
            "settings.command-console",
            "settings.command_console_enabled",
            settings.command_console_enabled,
        ))
        .with_child(toggle_row_with_key(
            palette,
            "settings.hierarchy-icons",
            "settings.hierarchy_icons",
            "settings.hierarchy_show_icons",
            settings.hierarchy_show_icons,
        ))
        .with_child(toggle_row_with_key(
            palette,
            "settings.hierarchy-visibility",
            "settings.hierarchy_visibility",
            "settings.hierarchy_show_visibility",
            settings.hierarchy_show_visibility,
        ))
        .with_child(toggle_row_with_key(
            palette,
            "settings.hierarchy-locked",
            "settings.hierarchy_locked",
            "settings.hierarchy_show_locked",
            settings.hierarchy_show_locked,
        ))
        .with_child(toggle_row_with_key(
            palette,
            "settings.hierarchy-hidden",
            "settings.hierarchy_hidden",
            "settings.hierarchy_show_hidden",
            settings.hierarchy_show_hidden,
        ))
        .with_child(toggle_row_with_key(
            palette,
            "settings.hierarchy-auto-reveal",
            "settings.hierarchy_auto_reveal",
            "settings.hierarchy_auto_reveal_selection",
            settings.hierarchy_auto_reveal_selection,
        ))
        .with_child(toggle_row_with_key(
            palette,
            "settings.hierarchy-expand-selection",
            "settings.hierarchy_expand_selection",
            "settings.hierarchy_expand_on_select",
            settings.hierarchy_expand_on_select,
        ))
        .with_child(toggle_row(
            palette,
            "settings.hierarchy-animations",
            "settings.hierarchy_animations",
            settings.hierarchy_animations,
        ))
        .with_child(range_row(
            palette,
            "settings.hierarchy-row-height",
            "settings.hierarchy_row_height",
            settings.hierarchy_row_height,
            20.0,
            36.0,
            1.0,
            format!("{} px", settings.hierarchy_row_height.round()),
        ))
        .with_child(range_row(
            palette,
            "settings.hierarchy-indent-width",
            "settings.hierarchy_indent_width",
            settings.hierarchy_indent_width,
            8.0,
            28.0,
            1.0,
            format!("{} px", settings.hierarchy_indent_width.round()),
        ))
        .with_child(toggle_row_with_key(
            palette,
            "settings.electronics-status",
            "settings.electronics-status",
            "settings.electronics_show_status",
            settings.electronics_show_status,
        ))
}

fn viewport(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    card(palette, "settings.viewport")
        .with_child(segment_row(
            palette,
            "settings.viewport-render-mode",
            "settings.viewport_render_mode",
            &[
                (
                    "settings.viewport_render_mode.solid",
                    "settings.viewport_render_mode.solid",
                    settings.viewport_render_mode == ViewportRenderMode::Solid,
                ),
                (
                    "settings.viewport_render_mode.wireframe",
                    "settings.viewport_render_mode.wireframe",
                    settings.viewport_render_mode == ViewportRenderMode::Wireframe,
                ),
                (
                    "settings.viewport_render_mode.preview",
                    "settings.viewport_render_mode.preview",
                    settings.viewport_render_mode == ViewportRenderMode::Preview,
                ),
            ],
        ))
        .with_child(toggle_row(
            palette,
            "settings.viewport-labels",
            "settings.show_viewport_labels",
            settings.show_viewport_labels,
        ))
        .with_child(toggle_row(
            palette,
            "settings.surface-edges",
            "settings.solid_show_surface_edges",
            settings.solid_show_surface_edges,
        ))
        .with_child(toggle_row(
            palette,
            "settings.xray",
            "settings.solid_xray_mode",
            settings.solid_xray_mode,
        ))
        .with_child(toggle_row(
            palette,
            "settings.face-tonality",
            "settings.solid_face_tonality",
            settings.solid_face_tonality,
        ))
        .with_child(toggle_row(
            palette,
            "settings.invert-x",
            "settings.invert_mouse_x",
            settings.invert_mouse_x,
        ))
        .with_child(toggle_row(
            palette,
            "settings.invert-y",
            "settings.invert_mouse_y",
            settings.invert_mouse_y,
        ))
        .with_child(toggle_row(
            palette,
            "settings.focus-lock",
            "settings.focus_lock_enabled",
            settings.focus_lock_enabled,
        ))
        .with_child(toggle_row(
            palette,
            "settings.invert-ws",
            "settings.invert_ws",
            settings.invert_ws,
        ))
        .with_child(range_row(
            palette,
            "settings.wasd-speed",
            "settings.wasd_speed",
            settings.wasd_speed,
            0.05,
            5.0,
            0.1,
            format!("{:.2}x", settings.wasd_speed),
        ))
        .with_child(toggle_row(
            palette,
            "settings.uniform-scale",
            "settings.uniform_scale_by_default",
            settings.uniform_scale_by_default,
        ))
        .with_child(toggle_row(
            palette,
            "settings.multi-select-gizmo",
            "settings.multi_select_gizmo_enabled",
            settings.multi_select_gizmo_enabled,
        ))
        .with_child(range_row(
            palette,
            "settings.gizmo-growth",
            "settings.gizmo_growth_scale",
            settings.gizmo_growth_scale,
            0.0,
            100.0,
            1.0,
            format!("{:.0}%", settings.gizmo_growth_scale),
        ))
        .with_child(range_row(
            palette,
            "settings.move-sensitivity",
            "settings.move_sensitivity",
            settings.move_gizmo_sensitivity,
            0.25,
            4.0,
            0.05,
            format!("{:.1}", settings.move_gizmo_sensitivity),
        ))
        .with_child(range_row(
            palette,
            "settings.rotate-sensitivity",
            "settings.rotate_sensitivity",
            settings.rotate_gizmo_sensitivity,
            0.25,
            4.0,
            0.05,
            format!("{:.1}", settings.rotate_gizmo_sensitivity),
        ))
        .with_child(range_row(
            palette,
            "settings.scale-sensitivity",
            "settings.scale_sensitivity",
            settings.scale_gizmo_sensitivity,
            0.25,
            4.0,
            0.05,
            format!("{:.1}", settings.scale_gizmo_sensitivity),
        ))
}

fn scripting(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    card(palette, "settings.scripting")
        .with_child(toggle_row(
            palette,
            "settings.script-runtime",
            "settings.script_runtime_enabled",
            settings.script_runtime_enabled,
        ))
        .with_child(segment_row(
            palette,
            "settings.script-language",
            "settings.default_script_language",
            &[
                (
                    "app.project_script_language_rhai",
                    "settings.script_language.rhai",
                    settings.default_script_language == ScriptLanguage::Rhai,
                ),
                (
                    "app.project_script_language_cpp",
                    "settings.script_language.cpp",
                    settings.default_script_language == ScriptLanguage::Cpp,
                ),
                (
                    "app.project_script_language_nodes",
                    "settings.script_language.nodes",
                    settings.default_script_language == ScriptLanguage::Nodes,
                ),
            ],
        ))
        .with_child(toggle_row(
            palette,
            "settings.script-hot-reload",
            "settings.script_hot_reload",
            settings.script_hot_reload,
        ))
        .with_child(range_row(
            palette,
            "settings.script-timeout",
            "settings.script_timeout_ms",
            settings.script_timeout_ms as f32,
            10.0,
            1000.0,
            1.0,
            format!("{} ms", settings.script_timeout_ms),
        ))
        .with_child(text_row(
            palette,
            "settings.script-editor",
            "settings.script_external_editor",
        ))
}

fn ai(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    revealed_api_keys: &[AiProvider],
) -> UiNode {
    let mut root = UiNode::new("settings.ai.workspace", UiNodeKind::Panel).with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: AI_CARD_GAP,
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    });

    let summary_card = card(palette, "settings.ai_providers")
        .with_child(provider_default_row(palette, settings))
        .with_child(agent_mode_row(palette, settings))
        .with_child(
            UiNode::new("settings.agent-mode.help", UiNodeKind::Label)
                .with_text_key("settings.agent_mode_desc")
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(toggle_row(
            palette,
            "settings.ai-persist-credentials",
            "settings.ai_persist_credentials",
            settings.ai_persist_credentials,
        ))
        .with_child(
            UiNode::new("settings.ai-persist-credentials.help", UiNodeKind::Label)
                .with_text_key("settings.ai_persist_credentials_desc")
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(toggle_row(
            palette,
            "settings.command-console-ai",
            "settings.command_console_enabled",
            settings.command_console_enabled,
        ))
        .with_child(toggle_row(
            palette,
            "settings.agent-streaming",
            "settings.agent_streaming_enabled",
            settings.agent_streaming_enabled,
        ))
        .with_child(range_row(
            palette,
            "settings.agent-max-response-tokens",
            "settings.agent_max_response_tokens",
            settings.agent_max_response_tokens as f32,
            raf_core::config::AGENT_MAX_RESPONSE_TOKENS_MIN as f32,
            raf_core::config::AGENT_MAX_RESPONSE_TOKENS_MAX as f32,
            256.0,
            format!("{} tokens", settings.agent_max_response_tokens),
        ))
        .with_child(
            UiNode::new("settings.agent-max-response-tokens.help", UiNodeKind::Label)
                .with_text_key("settings.agent_max_response_tokens_desc")
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    root = root.with_child(summary_card);

    for provider in AiProvider::editor_supported() {
        if let Some(config) = settings
            .ai_providers
            .iter()
            .find(|config| config.provider == *provider)
        {
            root = root.with_child(provider_card(
                palette,
                config,
                revealed_api_keys.contains(provider),
            ));
        }
    }

    root.with_child(model_shortcuts_card(palette, settings))
}

fn provider_default_row(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    let tokens = palette.tokens();
    let provider_count = AiProvider::editor_supported().len().max(1) as f32;
    let gap = 4.0;
    let option_width = 112.0;
    let toolbar_width = option_width * provider_count + gap * (provider_count - 1.0);
    let mut options = UiNode::new("settings.ai_provider.default.control", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap,
            ..UiLayout::fixed(toolbar_width, 30.0)
        });
    for provider in AiProvider::editor_supported() {
        let id = provider_id(*provider);
        let selected = settings.default_ai_provider == *provider;
        options = options.with_child(
            UiNode::new(
                format!("settings.ai_provider.default.{id}"),
                UiNodeKind::Button,
            )
            .with_class(if selected {
                "settings-segment-active"
            } else {
                "settings-segment"
            })
            .with_layout(UiLayout {
                padding: UiSpacing::xy(8.0, 0.0),
                ..UiLayout::fixed(112.0, 30.0)
            })
            .with_text_value(provider.display_name())
            .with_text_style(UiTextStyle::button(if selected {
                ACCENT
            } else {
                tokens.text_muted
            }))
            .with_accessibility_label_key("settings.ai_provider_default")
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("settings.ai_provider.default.{id}"),
            )),
        );
    }
    row(
        palette,
        "settings.ai_provider.default".to_string(),
        "settings.ai_provider_default".to_string(),
        None,
        options,
    )
}

fn agent_mode_row(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    let tokens = palette.tokens();
    let option_width = 178.0;
    let mut control =
        UiNode::new("settings.agent-mode.control", UiNodeKind::Toolbar).with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            ..UiLayout::fixed(option_width * 2.0 + 4.0, 42.0)
        });
    for (mode, label_key, command) in [
        (
            AgentMode::Passive,
            "settings.agent_mode_passive",
            "settings.agent_mode.passive",
        ),
        (
            AgentMode::Active,
            "settings.agent_mode_active",
            "settings.agent_mode.active",
        ),
    ] {
        let selected = settings.agent_mode == mode;
        control = control.with_child(
            UiNode::new(format!("settings.agent-mode.{command}"), UiNodeKind::Button)
                .with_class(if selected {
                    "settings-mode-option-current"
                } else {
                    "settings-mode-option"
                })
                .with_layout(UiLayout::fixed(option_width, 42.0).with_text_safe_area(true))
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::button(if selected {
                    ACCENT
                } else {
                    tokens.text_muted
                }))
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, command)),
        );
    }
    row(
        palette,
        "settings.agent-mode".to_string(),
        "settings.agent_mode".to_string(),
        None,
        control,
    )
}

fn provider_card(
    palette: StudioUiPalette,
    config: &raf_core::ai::AiProviderConfig,
    revealed: bool,
) -> UiNode {
    let id = provider_id(config.provider);
    let panel = UiNode::new(format!("settings.ai_provider.{id}.card"), UiNodeKind::Panel)
        .with_class("settings-provider-card")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            padding: UiSpacing::same(10.0),
            max_size: [760.0, 0.0],
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(
                format!("settings.ai_provider.{id}.title"),
                UiNodeKind::Label,
            )
            .with_text_value(config.provider.display_name())
            .with_text_style(UiTextStyle::panel_title(palette.tokens().text))
            .with_layout(UiLayout::fit_content()),
        )
        .with_child(toggle_row_with_key(
            palette,
            format!("settings.ai_provider.{id}.enabled"),
            "settings.ai_provider_enabled",
            format!("settings.ai_provider.{id}"),
            config.enabled,
        ))
        .with_child(ai_text_row(
            palette,
            &format!("settings.ai_provider.{id}.base_url"),
            "settings.ai_provider_base_url",
            &config.base_url,
            false,
            None,
            None,
        ))
        .with_child(ai_text_row(
            palette,
            &format!("settings.ai_provider.{id}.model"),
            "settings.ai_provider_model",
            &config.model,
            false,
            None,
            None,
        ))
        .with_child(ai_text_row(
            palette,
            &format!("settings.ai_provider.{id}.api_key"),
            "settings.ai_provider_api_key",
            &config.api_key,
            !revealed,
            Some(&format!("settings.ai_provider.reveal.{id}")),
            Some(&format!("settings.ai_provider.clear.{id}")),
        ))
        .with_child(command_button(
            palette,
            &format!("settings.ai_provider.set-default.{id}"),
            "settings.ai_provider_set_default",
            &format!("settings.ai_provider.default.{id}"),
            "settings-secondary-button",
            136.0,
        ));
    panel
}

fn ai_text_row(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    _value: &str,
    password: bool,
    reveal_command: Option<&str>,
    clear_command: Option<&str>,
) -> UiNode {
    let button_width = if reveal_command.is_some() && clear_command.is_some() {
        113.0
    } else if reveal_command.is_some() || clear_command.is_some() {
        58.0
    } else {
        0.0
    };
    let mut control = UiNode::new(format!("{id}.control"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 5.0,
            ..UiLayout::fixed(CONTROL_WIDTH + button_width, 30.0)
        })
        .with_child(
            UiNode::text_input(
                format!("{id}.input"),
                UiTextInput {
                    value_key: id.to_string(),
                    placeholder_key: Some("settings.surface.input".to_string()),
                    max_length: 512,
                    multiline: false,
                    password,
                    submit_command: None,
                },
            )
            .with_class("settings-text-input")
            .with_layout(UiLayout::fixed(CONTROL_WIDTH, 30.0)),
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
            command,
            "settings-secondary-button",
            54.0,
        ));
    }
    if let Some(command) = clear_command {
        control = control.with_child(command_button(
            palette,
            &format!("{id}.clear"),
            "settings.ai_provider_clear",
            command,
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

fn model_shortcuts_card(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    let tokens = palette.tokens();
    let mut panel = card(palette, "settings.ai_models_title");
    if settings.agent_model_shortcuts.is_empty() {
        return panel.with_child(
            UiNode::new("settings.ai_models.empty", UiNodeKind::Label)
                .with_text_key("settings.surface.shortcuts_empty")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    }
    for (index, shortcut) in settings.agent_model_shortcuts.iter().enumerate() {
        let selected = shortcut.label == settings.default_agent_model;
        let mut shortcut_row = UiNode::new(
            format!("settings.ai_models.row.{index}"),
            UiNodeKind::Toolbar,
        )
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            overflow: UiOverflow::Clip,
            ..UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("settings.ai_models.{index}"), UiNodeKind::Label)
                .with_text_value(format!(
                    "{} | {} | {}",
                    shortcut.label,
                    shortcut.provider.display_name(),
                    shortcut.model_id
                ))
                .with_text_style(UiTextStyle::body(if selected {
                    ACCENT
                } else {
                    tokens.text
                }))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
        );
        shortcut_row = shortcut_row.with_child(command_button(
            palette,
            &format!("settings.ai_models.use.{index}"),
            "settings.ai_model_use",
            &format!("settings.ai_model.default:{index}"),
            "settings-secondary-button",
            72.0,
        ));
        shortcut_row = shortcut_row.with_child(command_button(
            palette,
            &format!("settings.ai_models.remove.{index}"),
            "settings.surface.remove",
            &format!("settings.ai_model.remove:{index}"),
            "settings-secondary-button",
            72.0,
        ));
        panel = panel.with_child(shortcut_row);
    }
    panel
}

fn command_button(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    command: &str,
    class: &str,
    width: f32,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_layout(UiLayout::fixed(width, 30.0))
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_accessibility_label_key(label_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn platform(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    card(palette, "settings.target_platform")
        .with_child(segment_row(
            palette,
            "settings.platform",
            "settings.platform",
            &[
                (
                    "settings.value.desktop",
                    "settings.platform.desktop",
                    settings.target_platform == TargetPlatform::Desktop,
                ),
                (
                    "settings.value.mobile",
                    "settings.platform.mobile",
                    settings.target_platform == TargetPlatform::Mobile,
                ),
                (
                    "settings.value.web",
                    "settings.platform.web",
                    settings.target_platform == TargetPlatform::Web,
                ),
                (
                    "settings.value.cloud",
                    "settings.platform.cloud",
                    settings.target_platform == TargetPlatform::Cloud,
                ),
                (
                    "settings.value.console",
                    "settings.platform.console",
                    settings.target_platform == TargetPlatform::Console,
                ),
            ],
        ))
        .with_child(toggle_row(
            palette,
            "settings.responsive",
            "settings.responsive_layout",
            settings.responsive_layout,
        ))
        .with_child(toggle_row(
            palette,
            "settings.headless",
            "settings.headless",
            settings.headless,
        ))
}

fn card(palette: StudioUiPalette, title_key: &str) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(title_key, UiNodeKind::Panel)
        .with_class("settings-card")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            padding: UiSpacing::same(14.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 3.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::new(format!("{title_key}.title"), UiNodeKind::Label)
                .with_text_key(title_key)
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 24.0)),
        )
}

fn toggle_row(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value: bool,
) -> UiNode {
    let label_key = label_key.into();
    toggle_row_with_key(palette, id, label_key.clone(), label_key, value)
}

fn toggle_row_with_key(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value_key: impl Into<String>,
    value: bool,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let value_key = value_key.into();
    let toggle = UiNode::toggle(
        format!("{id}.control"),
        UiToggle::new(value_key.clone(), value),
    )
    .with_class("settings-toggle")
    .with_layout(UiLayout::fixed(48.0, 28.0));
    row(palette, id, label_key, None, toggle).with_event(UiEventBinding {
        event: UiEventKind::Click,
        action: UiAction::SetToggle {
            key: value_key,
            value: !value,
        },
    })
}

fn range_row(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    display: String,
) -> UiNode {
    range_row_disabled(
        palette, id, label_key, value, min, max, step, display, false,
    )
}

fn range_row_disabled(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    display: String,
    disabled: bool,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let numeric_key = format!("{label_key}.text");
    let percentage = range_percentage(value, min, max);
    let display = format!("{display}  |  {percentage:.0}%");
    let control = UiNode::new(format!("{id}.control"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            responsive: vec![raf_ui::UiResponsiveRule {
                max_width: 360.0,
                flow: Some(UiFlow::Column),
                basis: Some([0.0, 60.0]),
                padding: None,
                gap: Some(4.0),
                compact: Some(UiCompactMode::Stack),
                grid_columns: None,
            }],
            ..UiLayout::fixed(CONTROL_WIDTH + 78.0, 30.0)
        })
        .with_child(
            UiNode::range(
                format!("{id}.slider"),
                UiRange::new(label_key.clone(), value, min, max, step),
            )
            .with_class("settings-range")
            .with_layout(UiLayout {
                grow: 1.0,
                responsive: vec![raf_ui::UiResponsiveRule {
                    max_width: 360.0,
                    flow: Some(UiFlow::None),
                    basis: Some([0.0, 28.0]),
                    padding: None,
                    gap: None,
                    compact: None,
                    grid_columns: None,
                }],
                ..UiLayout::fixed(CONTROL_WIDTH - 84.0, 28.0)
            })
            .disabled(disabled),
        )
        .with_child(
            UiNode::text_input(
                format!("{id}.value"),
                UiTextInput {
                    value_key: numeric_key,
                    placeholder_key: Some("settings.surface.input".to_string()),
                    max_length: 32,
                    multiline: false,
                    password: false,
                    submit_command: Some(format!("settings.commit_numeric:{label_key}")),
                },
            )
            .with_class("settings-numeric-input")
            .with_layout(UiLayout::fixed(78.0, 28.0))
            .disabled(disabled),
        );
    let row = row(palette, id, label_key, Some(display), control);
    if disabled {
        row.with_class("settings-row-disabled")
    } else {
        row
    }
}

fn range_percentage(value: f32, min: f32, max: f32) -> f32 {
    if max <= min {
        0.0
    } else {
        ((value - min) / (max - min) * 100.0).clamp(0.0, 100.0)
    }
}

fn text_row(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    row(
        palette,
        id.clone(),
        label_key.clone(),
        None,
        UiNode::text_input(
            format!("{id}.control"),
            UiTextInput {
                value_key: label_key,
                placeholder_key: Some("settings.surface.input".to_string()),
                max_length: 256,
                multiline: false,
                password: false,
                submit_command: None,
            },
        )
        .with_class("settings-text-input")
        .with_layout(UiLayout {
            responsive: vec![raf_ui::UiResponsiveRule {
                max_width: 360.0,
                flow: Some(UiFlow::None),
                basis: Some([0.0, 30.0]),
                padding: None,
                gap: None,
                compact: None,
                grid_columns: None,
            }],
            ..UiLayout::fixed(CONTROL_WIDTH, 30.0)
        }),
    )
}

fn segment_row(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    values: &[(&str, &str, bool)],
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let tokens = palette.tokens();
    let count = values.len().max(1) as f32;
    let gap = 2.0;
    let segment_width = ((CONTROL_WIDTH - gap * (count - 1.0)) / count).max(SEGMENT_MIN_WIDTH);
    let toolbar_width = segment_width * count + gap * (count - 1.0);
    let mut toolbar = UiNode::new(format!("{id}.control"), UiNodeKind::Toolbar)
        .with_class("settings-segments")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap,
            overflow: UiOverflow::Clip,
            ..UiLayout::fixed(toolbar_width, 30.0)
        });
    for (value_key, command, selected) in values {
        toolbar = toolbar.with_child(
            UiNode::new(format!("{id}.{command}"), UiNodeKind::Button)
                .with_class(if *selected {
                    "settings-segment-active"
                } else {
                    "settings-segment"
                })
                .with_layout(UiLayout {
                    padding: UiSpacing::xy(8.0, 0.0),
                    ..UiLayout::fixed(segment_width, 30.0)
                })
                .with_text_key(*value_key)
                .with_text_style(UiTextStyle::button(if *selected {
                    ACCENT
                } else {
                    tokens.text_muted
                }))
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, *command)),
        );
    }
    row(palette, id, label_key, None, toolbar)
}

fn row(
    palette: StudioUiPalette,
    id: String,
    label_key: String,
    value: Option<String>,
    control: UiNode,
) -> UiNode {
    let tokens = palette.tokens();
    let mut copy = UiNode::new(format!("{id}.copy"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 2.0,
            ..UiLayout::default()
        })
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout::fit_content()),
        );
    if let Some(value) = value {
        copy = copy.with_child(
            UiNode::new(format!("{id}.display"), UiNodeKind::Label)
                .with_text_value(value)
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    }
    UiNode::new(format!("{id}.row"), UiNodeKind::Panel)
        .with_class("settings-row")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            compact: UiCompactMode::Stack,
            align_items: UiAlign::Center,
            gap: 14.0,
            padding: UiSpacing::xy(4.0, 4.0),
            responsive: vec![raf_ui::UiResponsiveRule {
                max_width: 720.0,
                flow: Some(UiFlow::Column),
                basis: Some([0.0, 78.0]),
                padding: Some(UiSpacing::xy(4.0, 6.0)),
                gap: Some(6.0),
                compact: Some(UiCompactMode::Stack),
                grid_columns: None,
            }],
            ..UiLayout::fixed(0.0, SETTINGS_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(copy)
        .with_child(control)
}

fn action_button(id: &str, text_key: &str, command: &str, primary: bool) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if primary {
            "settings-primary-button"
        } else {
            "settings-secondary-button"
        })
        .with_layout(UiLayout::fixed(if primary { 136.0 } else { 94.0 }, 30.0))
        .with_text_key(text_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn settings_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("settings-nav-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-nav-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-nav-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-row".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-row-disabled".to_string()),
                UiStylePatch {
                    opacity: Some(0.55),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-segments".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-segment".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-segment-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-mode-option".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-mode-option-current".to_string()),
                UiStylePatch {
                    fill: Some([55, 43, 26, 255]),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-mode-option".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-mode-option-current".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-segment".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-segment-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some([255, 171, 54, 255]),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-segment".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-segment-active".to_string()),
                UiStylePatch {
                    border: Some([255, 171, 54, 255]),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-range".to_string()),
                UiStylePatch {
                    border: Some(tokens.border),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-range".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    text: Some([255, 255, 255, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-primary-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent),
                    text: Some([255, 255, 255, 255]),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-secondary-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    text: Some(tokens.text),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-text-input".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    text: Some(tokens.text),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-numeric-input".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    text: Some(tokens.text),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-toggle".to_string()),
                UiStylePatch {
                    border: Some(tokens.border),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-toggle".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    text: Some([255, 255, 255, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-toggle".to_string()),
                UiStylePatch {
                    border: Some([255, 171, 54, 255]),
                    text: Some([255, 255, 255, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_render::api_graphic_basic::ui_surface::UiSurfaceSession;

    fn find_node<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if node.id == id {
            return Some(node);
        }
        node.children.iter().find_map(|child| find_node(child, id))
    }

    fn collect_nodes_with_class<'a>(node: &'a UiNode, class: &str, output: &mut Vec<&'a UiNode>) {
        if node.classes.iter().any(|candidate| candidate == class) {
            output.push(node);
        }
        for child in &node.children {
            collect_nodes_with_class(child, class, output);
        }
    }

    fn collect_text_keys(node: &UiNode, output: &mut Vec<String>) {
        if let Some(text_key) = &node.text_key {
            output.push(text_key.clone());
        }
        for child in &node.children {
            collect_text_keys(child, output);
        }
    }

    #[test]
    fn settings_surface_keeps_all_legacy_sections_in_a_single_navigation_contract() {
        assert_eq!(SettingsSection::ALL.len(), 7);
        assert_eq!(
            SettingsSection::from_command("settings.section.viewport"),
            Some(SettingsSection::Viewport)
        );
    }

    #[test]
    fn settings_surface_uses_intrinsic_rows_without_vertical_text_columns() {
        let surface = build_settings_surface(
            StudioUiPalette::IndustrialDark,
            &EngineSettings::default(),
            SettingsSection::Appearance,
        );
        assert_eq!(surface.root.layout.width_mode, UiSizeMode::Fill);
        assert!(surface.root.children.len() >= 2);
    }

    #[test]
    fn settings_toggle_row_is_clickable_outside_the_switch() {
        let surface = build_settings_surface(
            StudioUiPalette::IndustrialDark,
            &EngineSettings::default(),
            SettingsSection::Editor,
        );
        let row = find_node(&surface.root, "settings.show-grid.row").expect("toggle row");

        assert!(row.interactive);
        assert!(row.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            UiAction::SetToggle { key, value }
                if key == "settings.show_grid" && !value
        )));
    }

    #[test]
    fn settings_range_values_are_literal_text_with_a_percentage_indicator() {
        let surface = build_settings_surface(
            StudioUiPalette::IndustrialDark,
            &EngineSettings::default(),
            SettingsSection::Editor,
        );
        let value = find_node(&surface.root, "settings.grid_size.display").expect("range value");

        assert!(value.text_key.is_none());
        assert!(value
            .text_value
            .as_deref()
            .is_some_and(|text| text.ends_with('%')));
    }

    #[test]
    fn settings_segments_have_stable_padded_hitboxes() {
        let surface = build_settings_surface(
            StudioUiPalette::IndustrialDark,
            &EngineSettings::default(),
            SettingsSection::Editor,
        );
        let segments = find_node(&surface.root, "settings.units.control").expect("segments");

        assert!(segments.children.iter().all(|segment| {
            segment.layout.width_mode == UiSizeMode::Fixed
                && segment.layout.height_mode == UiSizeMode::Fixed
                && segment.layout.padding.left >= 8.0
                && segment.layout.padding.right >= 8.0
        }));
        assert!(segments.layout.responsive.is_empty());
    }

    #[test]
    fn every_settings_section_keeps_rows_separated_and_labels_localized() {
        for section in SettingsSection::ALL {
            let surface = build_settings_surface(
                StudioUiPalette::IndustrialDark,
                &EngineSettings::default(),
                section,
            );
            let mut rows = Vec::new();
            collect_nodes_with_class(&surface.root, "settings-row", &mut rows);
            assert!(!rows.is_empty(), "section {section:?} has no settings rows");
            assert!(rows
                .iter()
                .all(|row| row.layout.basis[1] >= SETTINGS_ROW_HEIGHT));

            let mut segments = Vec::new();
            collect_nodes_with_class(&surface.root, "settings-segments", &mut segments);
            assert!(segments
                .iter()
                .all(|toolbar| toolbar.layout.responsive.is_empty()));

            let mut text_keys = Vec::new();
            collect_text_keys(&surface.root, &mut text_keys);
            assert!(text_keys
                .iter()
                .all(|key| !key.starts_with("settings.hierarchy_show_")
                    && !key.starts_with("settings.electronics_show_")));

            let mut session = UiSurfaceSession::default();
            let frame =
                session.build_layout_frame_at_scale(&surface, 1600, 2400, [0, 0, 0, 255], 1.0);
            let mut rects = rows
                .iter()
                .map(|row| {
                    frame
                        .layout_boxes
                        .iter()
                        .find(|layout| layout.id == row.id)
                        .unwrap_or_else(|| panic!("missing layout box {}", row.id))
                        .rect
                })
                .collect::<Vec<_>>();
            rects.sort_by(|left, right| left.y.total_cmp(&right.y));
            assert!(
                rects
                    .windows(2)
                    .all(|pair| { pair[1].y + 0.01 >= pair[0].y + pair[0].height }),
                "overlapping settings rows in {section:?}: {rects:?}"
            );
        }
    }

    #[test]
    fn automatic_ui_scale_disables_manual_range_input() {
        let mut settings = EngineSettings::default();
        settings.auto_ui_scale = true;
        let surface = build_settings_surface(
            StudioUiPalette::IndustrialDark,
            &settings,
            SettingsSection::Appearance,
        );
        let slider = find_node(&surface.root, "settings.ui_scale.slider").expect("ui scale slider");
        let input = find_node(&surface.root, "settings.ui_scale.value").expect("ui scale input");

        assert!(slider.disabled);
        assert!(input.disabled);
    }

    #[test]
    fn ai_surface_uses_intrinsic_vertical_tracks_for_provider_cards() {
        let surface = build_settings_surface(
            StudioUiPalette::IndustrialDark,
            &EngineSettings::default(),
            SettingsSection::Ai,
        );
        let workspace = find_node(&surface.root, "settings.ai.workspace").expect("AI workspace");
        assert_eq!(workspace.layout.height_mode, UiSizeMode::FitContent);

        for provider in AiProvider::editor_supported() {
            let id = provider_id(*provider);
            let card = find_node(&surface.root, &format!("settings.ai_provider.{id}.card"))
                .expect("provider card");
            assert_eq!(card.layout.height_mode, UiSizeMode::FitContent);
        }
    }

    #[test]
    fn ai_surface_exposes_provider_credential_persistence_toggle() {
        let surface = build_settings_surface(
            StudioUiPalette::IndustrialDark,
            &EngineSettings::default(),
            SettingsSection::Ai,
        );
        let toggle = find_node(&surface.root, "settings.ai-persist-credentials.row")
            .expect("provider credential persistence toggle");
        assert!(toggle.interactive);
        let help = find_node(&surface.root, "settings.ai-persist-credentials.help")
            .expect("provider credential persistence help");
        assert_eq!(
            help.text_key.as_deref(),
            Some("settings.ai_persist_credentials_desc")
        );
    }

    #[test]
    fn ai_summary_and_shortcut_cards_keep_vertical_flow_when_intrinsic() {
        let surface = build_settings_surface(
            StudioUiPalette::IndustrialDark,
            &EngineSettings::default(),
            SettingsSection::Ai,
        );
        let mut session = UiSurfaceSession::default();
        let frame = session.build_layout_frame_at_scale(&surface, 1600, 1200, [0, 0, 0, 255], 1.0);
        let ids = [
            "settings.ai_providers.title",
            "settings.ai_provider.default.row",
            "settings.agent-mode.row",
            "settings.command-console-ai.row",
            "settings.agent-streaming.row",
        ];
        let rects = ids
            .iter()
            .map(|id| {
                frame
                    .layout_boxes
                    .iter()
                    .find(|layout| layout.id == *id)
                    .unwrap_or_else(|| panic!("missing layout box {id}"))
                    .rect
            })
            .collect::<Vec<_>>();
        assert!(rects.windows(2).all(|pair| pair[0].y < pair[1].y));

        let shortcut = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "settings.ai_models_title")
            .expect("shortcut card title");
        let shortcut_empty = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "settings.ai_models.empty")
            .expect("shortcut empty state");
        assert!(shortcut.rect.y < shortcut_empty.rect.y);
    }
}
