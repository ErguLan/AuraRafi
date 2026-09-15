//! RafUI presentation for engine-wide settings.
//!
//! This module deliberately owns only composition. Draft state, validation and
//! persistence stay in SettingsSurfaceHost.

use raf_core::ai::{AgentMode, AiProvider};
use raf_core::config::{
    EngineSettings, Language, RenderExecutionPolicy, RenderQuality, ScriptLanguage, TargetPlatform,
    Theme,
};
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiAlign, UiCompactMode, UiEventBinding, UiEventKind, UiFlow,
    UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiRange, UiScrollAxis, UiSelect,
    UiSelectOption, UiSizeMode, UiSpacing, UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState,
    UiStyleSelector, UiStyleSheet, UiSurface, UiSurfaceMaterial, UiTextInput, UiTextOverflow,
    UiTextStyle, UiToggle,
};
use raf_ui::components::{search_field, select_trigger};
use raf_ui::{UiAccessibilityRole, UiFontWeight, UiRect, UiTextRole};

const CONTROL_WIDTH: f32 = 360.0;
const SETTINGS_NAV_WIDTH: f32 = 236.0;
const SETTINGS_CONTROL_MIN_WIDTH: f32 = 220.0;
const SETTINGS_RANGE_MIN_WIDTH: f32 = 180.0;
const SETTINGS_COMPACT_BREAKPOINT: f32 = 360.0;
const SETTINGS_WORKSPACE_BREAKPOINT: f32 = 440.0;
const SEGMENT_MIN_WIDTH: f32 = 68.0;
const SETTINGS_ROW_HEIGHT: f32 = 64.0;
const SETTINGS_STACKED_ROW_HEIGHT: f32 = 92.0;
const DEFAULT_MODAL_RECT: UiRect = UiRect::new(48.0, 48.0, 1080.0, 720.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Appearance,
    Performance,
    Editor,
    Viewport,
    Electronics,
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
    pub const ALL: [Self; 8] = [
        Self::Appearance,
        Self::Performance,
        Self::Editor,
        Self::Viewport,
        Self::Electronics,
        Self::Scripting,
        Self::Ai,
        Self::Platform,
    ];

    pub fn group_order(self) -> u8 {
        match self.group_key() {
            "settings.group.general" => 0,
            "settings.group.workspace" => 1,
            "settings.group.tools" => 2,
            "settings.group.system" => 3,
            _ => 99,
        }
    }

    pub const fn command(self) -> &'static str {
        match self {
            Self::Appearance => "settings.section.appearance",
            Self::Performance => "settings.section.performance",
            Self::Editor => "settings.section.editor",
            Self::Viewport => "settings.section.viewport",
            Self::Electronics => "settings.section.electronics",
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
            Self::Electronics => "settings.electronics",
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
            Self::Electronics => "settings.surface.help.electronics",
            Self::Scripting => "settings.surface.help.scripting",
            Self::Ai => "settings.surface.help.ai",
            Self::Platform => "settings.surface.help.platform",
        }
    }

    pub const fn group_key(self) -> &'static str {
        match self {
            Self::Appearance | Self::Performance | Self::Editor => "settings.group.general",
            Self::Viewport | Self::Electronics => "settings.group.workspace",
            Self::Scripting | Self::Ai => "settings.group.tools",
            Self::Platform => "settings.group.system",
        }
    }

    pub fn from_command(command: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|section| section.command() == command)
    }

    pub fn matches_query(self, query: &str) -> bool {
        let query = query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return true;
        }
        let terms: &[&str] = match self {
            Self::Appearance => &[
                "appearance",
                "apariencia",
                "theme",
                "tema",
                "language",
                "idioma",
                "scale",
                "escala",
                "transparency",
                "transparencia",
                "accessibility",
                "accesibilidad",
            ],
            Self::Performance => &[
                "performance",
                "rendimiento",
                "quality",
                "calidad",
                "fps",
                "vsync",
                "gpu",
                "cpu",
                "render",
            ],
            Self::Editor => &[
                "editor",
                "grid",
                "rejilla",
                "autosave",
                "auto-save",
                "save",
                "guardar",
                "hierarchy",
                "jerarquia",
                "units",
                "unidades",
            ],
            Self::Viewport => &[
                "viewport",
                "game",
                "juego",
                "camera",
                "camara",
                "gizmo",
                "mouse",
                "render mode",
            ],
            Self::Electronics => &[
                "electronics",
                "electronica",
                "schematic",
                "esquema",
                "grid",
                "rejilla",
                "status",
                "estado",
            ],
            Self::Scripting => &[
                "scripting",
                "scripts",
                "script",
                "rhai",
                "cpp",
                "editor externo",
                "external editor",
            ],
            Self::Ai => &[
                "ai",
                "ia",
                "provider",
                "proveedor",
                "model",
                "modelo",
                "agent",
                "agente",
                "token",
            ],
            Self::Platform => &[
                "platform",
                "plataforma",
                "desktop",
                "mobile",
                "web",
                "cloud",
                "console",
                "consola",
                "headless",
            ],
        };
        terms
            .iter()
            .any(|term| term.contains(query.as_str()) || query.contains(term))
    }
}

pub fn build_settings_surface(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    section: SettingsSection,
) -> UiSurface {
    build_settings_surface_with_state_and_api_keys(
        palette,
        settings,
        section,
        DEFAULT_MODAL_RECT,
        "",
        None,
        &[],
    )
}

/// Compatibility entry point for callers that need to temporarily reveal an
/// AI API key while the settings draft remains open.
pub fn build_settings_surface_with_api_keys(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    section: SettingsSection,
    revealed_api_keys: &[AiProvider],
) -> UiSurface {
    build_settings_surface_with_state_and_api_keys(
        palette,
        settings,
        section,
        DEFAULT_MODAL_RECT,
        "",
        None,
        revealed_api_keys,
    )
}

pub fn build_settings_surface_with_state(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    section: SettingsSection,
    modal_rect: UiRect,
    search_query: &str,
    open_select: Option<&str>,
) -> UiSurface {
    build_settings_surface_with_state_and_api_keys(
        palette,
        settings,
        section,
        modal_rect,
        search_query,
        open_select,
        &[],
    )
}

pub(crate) fn build_settings_surface_with_state_and_api_keys(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    section: SettingsSection,
    modal_rect: UiRect,
    search_query: &str,
    open_select: Option<&str>,
    revealed_api_keys: &[AiProvider],
) -> UiSurface {
    let tokens = palette.tokens();
    let root = UiNode::new("settings.root", UiNodeKind::Root)
        .with_layout(UiLayout::fill(UiFlow::None))
        .with_style(UiStyle::transparent())
        .with_child(
            UiNode::new("settings.backdrop", UiNodeKind::Overlay)
                .with_class("settings-backdrop")
                .with_material(UiSurfaceMaterial::BackdropScrim)
                .with_layout(UiLayout::fill(UiFlow::None).with_z_index(1))
                .with_style(UiStyle {
                    fill: tokens.background,
                    border: [0, 0, 0, 0],
                    text: tokens.text,
                    border_width: 0.0,
                    radius: 0.0,
                    opacity: 1.0,
                })
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "settings.dismiss",
                )),
        )
        .with_child(modal(
            palette,
            settings,
            section,
            modal_rect,
            search_query,
            open_select,
            revealed_api_keys,
        ));

    let mut surface = UiSurface::new("editor.engine-settings", palette, root);
    surface.style_sheet = settings_style_sheet(palette);
    surface
}

fn modal(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    active: SettingsSection,
    modal_rect: UiRect,
    search_query: &str,
    open_select: Option<&str>,
    revealed_api_keys: &[AiProvider],
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("settings.modal", UiNodeKind::FloatingPanel)
        .with_class("settings-modal")
        .with_material(UiSurfaceMaterial::ModalSurface)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 0.0,
            padding: UiSpacing::same(0.0),
            ..UiLayout::absolute(modal_rect).with_z_index(50)
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 6.0,
            opacity: 1.0,
        })
        .with_accessibility_role(UiAccessibilityRole::Dialog)
        .with_child(modal_header(palette))
        .with_child(
            UiNode::new("settings.modal.body", UiNodeKind::Panel)
                .with_class("settings-modal-body")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    grow: 1.0,
                    gap: 0.0,
                    ..UiLayout::default()
                })
                .with_child(navigation(palette, active, search_query))
                .with_child(workspace(
                    palette,
                    settings,
                    active,
                    open_select,
                    revealed_api_keys,
                )),
        )
        .with_child(modal_footer(palette))
        .with_child(settings_modal_resize_handle(palette, modal_rect))
}

fn modal_header(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("settings.modal.header", UiNodeKind::Toolbar)
        .with_class("settings-modal-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 12.0,
            padding: UiSpacing::xy(18.0, 10.0),
            ..UiLayout::fixed(0.0, 58.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("settings.modal.drag-handle", UiNodeKind::Button)
                .with_class("settings-modal-drag-handle")
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    grow: 1.0,
                    gap: 2.0,
                    ..UiLayout::default()
                })
                .with_child(
                    UiNode::new("settings.modal.title", UiNodeKind::Label)
                        .with_text_key("app.engine_settings_title")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout::fit_content()),
                )
                .with_child(
                    UiNode::new("settings.modal.subtitle", UiNodeKind::Label)
                        .with_text_key("settings.surface.draft_hint")
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fit_content()),
                )
                .with_accessibility_role(UiAccessibilityRole::Button)
                .with_event(UiEventBinding::command(
                    UiEventKind::DragStart,
                    "settings.modal.drag.start",
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::DragMove,
                    "settings.modal.drag.move",
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::DragEnd,
                    "settings.modal.drag.end",
                )),
        )
        .with_child(
            search_field(
                "settings.search",
                "settings.search",
                "settings.search_placeholder",
                None,
                palette,
            )
            .with_class("settings-search")
            .with_layout(UiLayout::fixed(250.0, 32.0)),
        )
        .with_child(action_button(
            palette,
            "settings.modal.close",
            "settings.close",
            "settings.dismiss",
            false,
        ))
}

fn navigation(palette: StudioUiPalette, active: SettingsSection, search_query: &str) -> UiNode {
    let tokens = palette.tokens();
    let mut node = UiNode::scroll_view("settings.navigation", UiScrollAxis::Vertical)
        .with_class("settings-navigation")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            basis: [SETTINGS_NAV_WIDTH, 0.0],
            width_mode: UiSizeMode::Fixed,
            height_mode: UiSizeMode::Fill,
            min_size: [SETTINGS_NAV_WIDTH, 0.0],
            padding: UiSpacing::same(16.0),
            gap: 4.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::default()
        })
        .with_child(
            UiNode::new("settings.navigation.divider", UiNodeKind::Separator)
                .with_layout(UiLayout::fixed(0.0, 1.0)),
        );

    let mut visible_count = 0;
    let mut last_group: Option<&'static str> = None;
    for item in SettingsSection::ALL {
        if !item.matches_query(search_query) {
            continue;
        }
        let group = item.group_key();
        if last_group != Some(group) {
            node = node.with_child(group_label(palette, group));
            last_group = Some(group);
        }
        visible_count += 1;
        let item_id = format!("settings.navigation.{}", item.key());
        let is_active = item == active;
        let button = UiNode::new(item_id.clone(), UiNodeKind::Button)
            .with_class(if is_active {
                "settings-nav-active"
            } else {
                "settings-nav-button"
            })
            .with_layout(UiLayout {
                padding: UiSpacing::xy(if is_active { 14.0 } else { 18.0 }, 0.0),
                ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_text_key(item.key())
            .with_text_style(UiTextStyle::button(if is_active {
                tokens.text
            } else {
                tokens.text_muted
            }))
            .with_accessibility_role(UiAccessibilityRole::Tab)
            .with_accessibility_selected(is_active)
            .focusable()
            .with_event(UiEventBinding::command(UiEventKind::Click, item.command()));
        if is_active {
            let indicator_id = format!("{item_id}.indicator");
            let indicator = UiNode::new(indicator_id, UiNodeKind::Panel)
                .with_class("settings-nav-indicator")
                .with_layout(UiLayout::absolute(UiRect::new(6.0, 8.0, 3.0, 18.0)));
            node = node.with_child(
                UiNode::new(format!("{item_id}.row"), UiNodeKind::Panel)
                    .with_class("settings-nav-item-active")
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        align_items: UiAlign::Center,
                        gap: 0.0,
                        padding: UiSpacing::ZERO,
                        ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
                    })
                    .with_child(indicator)
                    .with_child(button),
            );
        } else {
            node = node.with_child(button);
        }
    }

    if visible_count == 0 {
        node = node.with_child(
            UiNode::new("settings.navigation.no-results", UiNodeKind::Label)
                .with_text_key("settings.search_no_results")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
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
    open_select: Option<&str>,
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
                        max_width: SETTINGS_WORKSPACE_BREAKPOINT,
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
                        .with_layout(UiLayout::fixed(0.0, 22.0))
                        .with_text_overflow(UiTextOverflow::Ellipsis),
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
                        max_width: SETTINGS_WORKSPACE_BREAKPOINT,
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
                    open_select,
                    revealed_api_keys,
                )),
        )
}

fn modal_footer(palette: StudioUiPalette) -> UiNode {
    UiNode::new("settings.modal.footer", UiNodeKind::Toolbar)
        .with_class("settings-modal-footer")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            justify_content: UiJustify::End,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(18.0, 12.0),
            ..UiLayout::fixed(0.0, 58.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("settings.footer.spacer", UiNodeKind::Panel).with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::default()
            }),
        )
        .with_child(action_button_with_width(
            palette,
            "settings.reset-defaults",
            "settings.reset_defaults",
            "settings.reset_defaults",
            false,
            132.0,
        ))
        .with_child(action_button(
            palette,
            "settings.cancel",
            "app.cancel",
            "settings.cancel",
            false,
        ))
        .with_child(action_button(
            palette,
            "settings.apply",
            "settings.apply",
            "settings.apply",
            false,
        ))
        .with_child(action_button(
            palette,
            "settings.save",
            "app.save_and_close",
            "settings.save",
            true,
        ))
}

fn settings_modal_resize_handle(palette: StudioUiPalette, modal_rect: UiRect) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("settings.modal.resize-handle", UiNodeKind::Button)
        .with_class("settings-modal-resize-handle")
        .with_layout(
            UiLayout::absolute(UiRect::new(
                (modal_rect.width - 20.0).max(0.0),
                (modal_rect.height - 20.0).max(0.0),
                20.0,
                20.0,
            ))
            .with_z_index(60),
        )
        .with_style(UiStyle {
            fill: [0, 0, 0, 0],
            border: tokens.border,
            text: tokens.text_muted,
            border_width: 1.0,
            radius: 3.0,
            opacity: 1.0,
        })
        .with_accessibility_label_key("settings.resize")
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_event(UiEventBinding::command(
            UiEventKind::DragStart,
            "settings.modal.resize.start",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragMove,
            "settings.modal.resize.move",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragEnd,
            "settings.modal.resize.end",
        ))
}

fn section_content(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    section: SettingsSection,
    open_select: Option<&str>,
    revealed_api_keys: &[AiProvider],
) -> UiNode {
    match section {
        SettingsSection::Appearance => appearance(palette, settings),
        SettingsSection::Performance => performance(palette, settings, open_select),
        SettingsSection::Editor => editor(palette, settings),
        SettingsSection::Viewport => viewport(palette, settings, open_select),
        SettingsSection::Electronics => electronics(palette, settings),
        SettingsSection::Scripting => scripting(palette, settings, open_select),
        SettingsSection::Ai => ai(palette, settings, open_select, revealed_api_keys),
        SettingsSection::Platform => platform(palette, settings, open_select),
    }
}

fn appearance(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    let mut node = card(palette, "settings.appearance");
    node = node.with_child(group_label(palette, "settings.group.display"));
    node = node.with_child(segment_row_with_description(
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
        "settings.theme_desc",
    ));
    node = node.with_child(range_row_with_description(
        palette,
        "settings.font_size",
        "settings.font_size",
        settings.font_size,
        10.0,
        24.0,
        1.0,
        format!("{:.0} px", settings.font_size),
        "settings.font_size_desc",
    ));
    node = node.with_child(toggle_row_with_description(
        palette,
        "settings.auto-ui-scale",
        "settings.auto_ui_scale",
        settings.auto_ui_scale,
        "settings.auto_ui_scale_desc",
    ));
    node = node.with_child(range_row_disabled_with_description(
        palette,
        "settings.ui_scale",
        "settings.ui_scale",
        settings.ui_scale,
        0.5,
        3.0,
        0.1,
        format!("{:.1}x", settings.ui_scale),
        settings.auto_ui_scale,
        "settings.ui_scale_desc",
    ));
    node = node.with_child(range_row(
        palette,
        "settings.theme-experimental",
        "settings.theme_experimental",
        settings.theme_experimental,
        0.0,
        100.0,
        1.0,
        format!("{:.0}%", settings.theme_experimental),
    ));

    node = node.with_child(group_label(palette, "settings.group.accessibility"));
    node = node.with_child(toggle_row_with_description(
        palette,
        "settings.prefers-reduced-motion",
        "settings.prefers_reduced_motion",
        settings.prefers_reduced_motion,
        "settings.prefers_reduced_motion_desc",
    ));
    node = node.with_child(toggle_row_with_description(
        palette,
        "settings.high-contrast",
        "settings.high_contrast",
        settings.high_contrast,
        "settings.high_contrast_desc",
    ));
    node = node.with_child(toggle_row_with_description(
        palette,
        "settings.reduce-transparency",
        "settings.reduce_transparency",
        settings.reduce_transparency,
        "settings.reduce_transparency_desc",
    ));

    node = node.with_child(segment_row_with_description(
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
        "settings.language_desc",
    ));

    node.with_child(toggle_row_with_description(
        palette,
        "settings.simple-mode",
        "settings.simple_mode",
        settings.simple_mode,
        "settings.simple_mode_desc",
    ))
}

fn performance(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    open_select: Option<&str>,
) -> UiNode {
    card(palette, "settings.performance")
        .with_child(group_label(palette, "settings.group.quality"))
        .with_child(select_row(
            palette,
            "settings.quality",
            "settings.quality",
            "settings.quality",
            vec![
                UiSelectOption::new("potato", "settings.value.potato"),
                UiSelectOption::new("low", "settings.value.low"),
                UiSelectOption::new("medium", "settings.value.medium"),
                UiSelectOption::new("high", "settings.value.high"),
            ],
            match settings.render_quality {
                RenderQuality::Potato => 0,
                RenderQuality::Low => 1,
                RenderQuality::Medium => 2,
                RenderQuality::High => 3,
            },
            open_select,
        ))
        .with_child(select_row(
            palette,
            "settings.render_execution_policy",
            "settings.render_execution_policy",
            "settings.render_execution_policy",
            vec![
                UiSelectOption::new("auto", "settings.render_execution_policy.auto"),
                UiSelectOption::new("cpu_only", "settings.render_execution_policy.cpu_only"),
                UiSelectOption::new(
                    "gpu_preferred",
                    "settings.render_execution_policy.gpu_preferred",
                ),
            ],
            match settings.render_execution_policy {
                RenderExecutionPolicy::Auto => 0,
                RenderExecutionPolicy::CpuOnly => 1,
                RenderExecutionPolicy::GpuPreferred => 2,
            },
            open_select,
        ))
        .with_child(group_label(palette, "settings.group.frame_pacing"))
        .with_child(toggle_row_with_description(
            palette,
            "settings.vsync",
            "settings.vsync",
            settings.vsync,
            "settings.vsync_desc",
        ))
        .with_child(toggle_row_with_description(
            palette,
            "settings.multithreading",
            "settings.multithreading",
            settings.multithreading,
            "settings.multithreading_desc",
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
        .with_child(group_label(palette, "settings.group.editor_view"))
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
        .with_child(toggle_row_with_description(
            palette,
            "settings.inspector-live-transform-updates",
            "settings.inspector_live_transform_updates",
            settings.inspector_live_transform_updates,
            "settings.inspector_live_transform_updates_desc",
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
        .with_child(group_label(palette, "settings.group.files"))
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
                (
                    "settings.value.game_units",
                    "settings.units.game",
                    settings.display_unit == raf_core::units::DisplayUnit::Game,
                ),
            ],
        ))
        .with_child(toggle_row(
            palette,
            "settings.command-console",
            "settings.command_console_enabled",
            settings.command_console_enabled,
        ))
        .with_child(group_label(palette, "settings.group.hierarchy"))
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
}

fn electronics(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    card(palette, "settings.electronics")
        .with_child(group_label(palette, "settings.group.electronics"))
        .with_child(toggle_row(
            palette,
            "settings.electronics-grid-visible",
            "settings.show_grid",
            settings.grid_visible,
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
        .with_child(range_row_with_description(
            palette,
            "settings.electronics-grid-opacity",
            "settings.electronics_grid_opacity",
            settings.electronics_grid_opacity,
            0.2,
            1.0,
            0.05,
            format!("{:.0}%", settings.electronics_grid_opacity * 100.0),
            "settings.electronics_grid_opacity_desc",
        ))
        .with_child(toggle_row_with_key(
            palette,
            "settings.electronics-status",
            "settings.electronics-status",
            "settings.electronics_show_status",
            settings.electronics_show_status,
        ))
}

fn viewport(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    open_select: Option<&str>,
) -> UiNode {
    card(palette, "settings.viewport")
        .with_child(group_label(palette, "settings.group.game"))
        .with_child(select_row(
            palette,
            "settings.viewport-render-mode",
            "settings.viewport_render_mode",
            "settings.viewport_render_mode",
            vec![UiSelectOption::new(
                "solid",
                "settings.viewport_render_mode.solid",
            )],
            0,
            open_select,
        ))
        .with_child(toggle_row_with_description(
            palette,
            "settings.viewport-labels",
            "settings.show_viewport_labels",
            settings.show_viewport_labels,
            "settings.show_viewport_labels_desc",
        ))
        .with_child(toggle_row_with_description(
            palette,
            "settings.surface-edges",
            "settings.solid_show_surface_edges",
            settings.solid_show_surface_edges,
            "settings.solid_show_surface_edges_desc",
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
        .with_child(toggle_row_with_description(
            palette,
            "settings.invert-ws",
            "settings.invert_ws",
            settings.invert_ws,
            "settings.invert_ws_desc",
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
        .with_child(toggle_row_with_description(
            palette,
            "settings.uniform-scale",
            "settings.uniform_scale_by_default",
            settings.uniform_scale_by_default,
            "settings.uniform_scale_by_default_desc",
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

fn scripting(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    open_select: Option<&str>,
) -> UiNode {
    card(palette, "settings.scripting")
        .with_child(toggle_row(
            palette,
            "settings.script-runtime",
            "settings.script_runtime_enabled",
            settings.script_runtime_enabled,
        ))
        .with_child(select_row(
            palette,
            "settings.script-language",
            "settings.default_script_language",
            "settings.default_script_language",
            vec![
                UiSelectOption::new("rhai", "settings.script_language.rhai"),
                UiSelectOption::new("cpp", "settings.script_language.cpp"),
            ],
            match settings.default_script_language {
                ScriptLanguage::Cpp => 1,
                ScriptLanguage::Rhai | ScriptLanguage::Nodes => 0,
            },
            open_select,
        ))
        .with_child(toggle_row_with_description(
            palette,
            "settings.script-hot-reload",
            "settings.script_hot_reload",
            settings.script_hot_reload,
            "settings.script_hot_reload_desc",
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

fn platform(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    open_select: Option<&str>,
) -> UiNode {
    card(palette, "settings.target_platform")
        .with_child(group_label(palette, "settings.group.platform"))
        .with_child(select_row_with_description(
            palette,
            "settings.platform",
            "settings.target_platform",
            "settings.target_platform",
            vec![
                UiSelectOption::new("desktop", "settings.value.desktop"),
                UiSelectOption::new("mobile", "settings.value.mobile"),
                UiSelectOption::new("web", "settings.value.web"),
                UiSelectOption::new("cloud", "settings.value.cloud"),
                UiSelectOption::new("console", "settings.value.console"),
            ],
            match settings.target_platform {
                TargetPlatform::Desktop => 0,
                TargetPlatform::Mobile => 1,
                TargetPlatform::Web => 2,
                TargetPlatform::Cloud => 3,
                TargetPlatform::Console => 4,
            },
            open_select,
            "settings.target_platform_desc",
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

fn ai(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    open_select: Option<&str>,
    revealed_api_keys: &[AiProvider],
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

    for config in &settings.ai_providers {
        let provider = config.provider;
        let id = provider_id(provider);
        let card_id = format!("settings.ai-provider.{id}");
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
                .with_child(toggle_row_with_key(
                    palette,
                    format!("{card_id}.enabled"),
                    "settings.ai_provider_enabled",
                    format!("settings.ai_provider.{id}"),
                    config.enabled,
                ))
                .with_child(ai_provider_text_row(
                    palette,
                    &format!("{card_id}.base-url"),
                    "settings.ai_provider_base_url",
                    &format!("settings.ai_provider.{id}.base_url"),
                    false,
                    None,
                    None,
                ))
                .with_child(ai_provider_text_row(
                    palette,
                    &format!("{card_id}.model"),
                    "settings.ai_provider_model",
                    &format!("settings.ai_provider.{id}.model"),
                    false,
                    None,
                    None,
                ))
                .with_child(ai_provider_text_row(
                    palette,
                    &format!("{card_id}.api-key"),
                    "settings.ai_provider_api_key",
                    &format!("settings.ai_provider.{id}.api_key"),
                    !revealed_api_keys.contains(&provider),
                    Some(format!("settings.ai_provider.reveal.{id}")),
                    Some(format!("settings.ai_provider.clear.{id}")),
                ))
                .with_child(command_button(
                    palette,
                    &format!("{card_id}.set-default"),
                    "settings.ai_provider_set_default",
                    &format!("settings.ai_provider.default.{id}"),
                    "settings-secondary-button",
                    136.0,
                )),
        );
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
        .with_child(range_row_disabled_with_description(
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
        ))
        .with_child(range_row(
            palette,
            "settings.agent-max-response-tokens",
            "settings.agent_max_response_tokens",
            settings.agent_max_response_tokens as f32,
            raf_core::config::AGENT_MAX_RESPONSE_TOKENS_MIN as f32,
            raf_core::config::AGENT_MAX_RESPONSE_TOKENS_MAX as f32,
            256.0,
            settings.agent_max_response_tokens.to_string(),
        ))
        .with_child(group_label(palette, "settings.group.shortcuts"));
    for (index, shortcut) in settings.agent_model_shortcuts.iter().enumerate() {
        let selected = shortcut.label == settings.default_agent_model;
        let mut shortcut_row = UiNode::new(
            format!("settings.ai-models.row.{index}"),
            UiNodeKind::Toolbar,
        )
        .with_class("settings-ai-shortcut-row")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            ..UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("settings.ai-models.{index}"), UiNodeKind::Label)
                .with_text_value(format!(
                    "{}  |  {}  |  {}",
                    shortcut.label,
                    shortcut.provider.display_name(),
                    shortcut.model_id
                ))
                .with_text_style(UiTextStyle::body(if selected {
                    tokens.accent
                } else {
                    tokens.text_muted
                }))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
        )
        .with_child(command_button(
            palette,
            &format!("settings.ai-models.use.{index}"),
            "settings.ai_model_use",
            &format!("settings.ai_model.default:{index}"),
            "settings-secondary-button",
            72.0,
        ))
        .with_child(command_button(
            palette,
            &format!("settings.ai-models.remove.{index}"),
            "settings.surface.remove",
            &format!("settings.ai_model.remove:{index}"),
            "settings-secondary-button",
            72.0,
        ));
        if selected {
            shortcut_row = shortcut_row.with_class("settings-ai-shortcut-selected");
        }
        root = root.with_child(shortcut_row);
    }
    root
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

fn group_label(palette: StudioUiPalette, text_key: &str) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(format!("{text_key}.label"), UiNodeKind::Label)
        .with_class("settings-group-label")
        .with_layout(UiLayout::fixed(0.0, 22.0).with_width_mode(UiSizeMode::Fill))
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::panel_title(tokens.text_muted))
}

fn select_row(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value_key: impl Into<String>,
    options: Vec<UiSelectOption>,
    selected_index: usize,
    open_select: Option<&str>,
) -> UiNode {
    select_row_with_description(
        palette,
        id,
        label_key,
        value_key,
        options,
        selected_index,
        open_select,
        "",
    )
}

fn select_row_with_description(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value_key: impl Into<String>,
    options: Vec<UiSelectOption>,
    selected_index: usize,
    open_select: Option<&str>,
    description_key: impl Into<String>,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let value_key = value_key.into();
    let description_key = description_key.into();
    let has_description = !description_key.is_empty();
    let selected_index = selected_index.min(options.len().saturating_sub(1));
    let action_value_key = value_key.clone();
    let trigger_id = format!("{id}.control");
    let is_open = open_select == Some(trigger_id.as_str());
    let mut select = UiSelect::new(value_key, options.clone(), selected_index)
        .with_popup_id(format!("{id}.menu"));
    select.open = is_open;
    let trigger = select_trigger(trigger_id, select, palette)
        .with_class("settings-select-trigger")
        .with_layout(UiLayout {
            grow: 1.0,
            min_size: [SETTINGS_CONTROL_MIN_WIDTH, 32.0],
            max_size: [CONTROL_WIDTH, 32.0],
            ..UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_accessibility_label_key(label_key.clone());
    let mut control = UiNode::new(format!("{id}.control-wrap"), UiNodeKind::Panel)
        .with_class("settings-select-wrap")
        .with_layout(UiLayout {
            grow: 1.0,
            min_size: [SETTINGS_CONTROL_MIN_WIDTH, 32.0],
            max_size: [CONTROL_WIDTH, 32.0],
            ..UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(trigger);

    if is_open {
        let menu_height = (options.len() as f32 * 30.0).min(210.0);
        let tokens = palette.tokens();
        let mut menu = UiNode::new(format!("{id}.menu"), UiNodeKind::Menu)
            .with_class("settings-select-menu")
            .with_material(UiSurfaceMaterial::TranslucentRaised)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                ..UiLayout::absolute(UiRect::new(0.0, 34.0, CONTROL_WIDTH, menu_height.max(30.0)))
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
        for (index, option) in options.iter().enumerate() {
            let selected = index == selected_index;
            let option_id = format!("{id}.option.{}", option.value);
            menu = menu.with_child(
                UiNode::new(option_id, UiNodeKind::Button)
                    .with_class(if selected {
                        "settings-select-option-selected"
                    } else {
                        "settings-select-option"
                    })
                    .with_layout(UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill))
                    .with_text_key(option.label_key.clone())
                    .with_text_style(UiTextStyle::body(if selected {
                        tokens.text
                    } else {
                        tokens.text_muted
                    }))
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_accessibility_role(UiAccessibilityRole::Option)
                    .with_accessibility_selected(selected)
                    .focusable()
                    .with_event(UiEventBinding {
                        event: UiEventKind::Click,
                        action: UiAction::SetSelect {
                            key: action_value_key.clone(),
                            value: option.value.clone(),
                            index,
                        },
                    })
                    .with_event(UiEventBinding {
                        event: UiEventKind::Click,
                        action: UiAction::SetSelectOpen {
                            id: format!("{id}.control"),
                            open: false,
                        },
                    }),
            );
        }
        control = control.with_child(menu);
    }

    if has_description {
        row_with_description(palette, id, label_key, None, description_key, control)
    } else {
        row(palette, id, label_key, None, control)
    }
}

pub(crate) fn provider_id(provider: AiProvider) -> &'static str {
    match provider {
        AiProvider::Puerto => "puerto",
        AiProvider::OpenRouter => "openrouter",
        AiProvider::OpenAI => "openai",
        AiProvider::GenAI => "genai",
        AiProvider::Claude => "claude",
    }
}

fn card(palette: StudioUiPalette, title_key: &str) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(title_key, UiNodeKind::Panel)
        .with_class("settings-card")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 10.0,
            padding: UiSpacing::same(20.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 6.0,
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

fn toggle_row_with_description(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value: bool,
    description_key: impl Into<String>,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let description_key = description_key.into();
    let value_key = label_key.clone();
    let toggle = UiNode::toggle(
        format!("{id}.control"),
        UiToggle::new(value_key.clone(), value),
    )
    .with_class("settings-toggle")
    .with_layout(UiLayout::fixed(48.0, 28.0));
    row_with_description(palette, id, label_key, None, description_key, toggle).with_event(
        UiEventBinding {
            event: UiEventKind::Click,
            action: UiAction::SetToggle {
                key: value_key,
                value: !value,
            },
        },
    )
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

fn range_row_with_description(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    display: String,
    description_key: impl Into<String>,
) -> UiNode {
    range_row_disabled_with_description(
        palette,
        id,
        label_key,
        value,
        min,
        max,
        step,
        display,
        false,
        description_key,
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
    range_row_disabled_with_description(
        palette, id, label_key, value, min, max, step, display, disabled, "",
    )
}

fn range_row_disabled_with_description(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    display: String,
    disabled: bool,
    description_key: impl Into<String>,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let description_key = description_key.into();
    let has_description = !description_key.is_empty();
    let numeric_key = format!("{label_key}.text");
    let control = UiNode::new(format!("{id}.control"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            grow: 1.0,
            min_size: [SETTINGS_RANGE_MIN_WIDTH, 30.0],
            max_size: [CONTROL_WIDTH + 78.0, 30.0],
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::range(
                format!("{id}.slider"),
                UiRange::new(label_key.clone(), value, min, max, step),
            )
            .with_class("settings-range")
            .with_layout(UiLayout {
                grow: 1.0,
                min_size: [96.0, 28.0],
                ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
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
    let row = if has_description {
        row_with_description(
            palette,
            id,
            label_key,
            Some(display),
            description_key,
            control,
        )
    } else {
        row(palette, id, label_key, Some(display), control)
    };
    if disabled {
        row.with_class("settings-row-disabled")
    } else {
        row
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
            grow: 1.0,
            min_size: [SETTINGS_CONTROL_MIN_WIDTH, 30.0],
            max_size: [CONTROL_WIDTH, 30.0],
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        }),
    )
}

fn segment_row(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    values: &[(&str, &str, bool)],
) -> UiNode {
    segment_row_with_description(palette, id, label_key, values, "")
}

fn segment_row_with_description(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    values: &[(&str, &str, bool)],
    description_key: impl Into<String>,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let description_key = description_key.into();
    let has_description = !description_key.is_empty();
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
                    tokens.text
                } else {
                    tokens.text_muted
                }))
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, *command)),
        );
    }
    if has_description {
        row_with_description(palette, id, label_key, None, description_key, toolbar)
    } else {
        row(palette, id, label_key, None, toolbar)
    }
}

fn row(
    palette: StudioUiPalette,
    id: String,
    label_key: String,
    value: Option<String>,
    control: UiNode,
) -> UiNode {
    row_with_breakpoint(
        palette,
        id,
        label_key,
        value,
        None,
        control,
        SETTINGS_COMPACT_BREAKPOINT,
    )
}

fn row_with_description(
    palette: StudioUiPalette,
    id: String,
    label_key: String,
    value: Option<String>,
    description_key: String,
    control: UiNode,
) -> UiNode {
    row_with_breakpoint(
        palette,
        id,
        label_key,
        value,
        Some(description_key),
        control,
        SETTINGS_COMPACT_BREAKPOINT,
    )
}

fn row_with_breakpoint(
    palette: StudioUiPalette,
    id: String,
    label_key: String,
    value: Option<String>,
    description_key: Option<String>,
    control: UiNode,
    compact_breakpoint: f32,
) -> UiNode {
    let tokens = palette.tokens();
    let mut copy = UiNode::new(format!("{id}.copy"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 2.0,
            min_size: [96.0, 0.0],
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
    if let Some(description_key) = description_key {
        copy = copy.with_child(
            UiNode::new(format!("{id}.description"), UiNodeKind::Label)
                .with_text_key(description_key)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 11.0,
                    line_height_px: 14.0,
                    weight: UiFontWeight::Regular,
                    color: tokens.text_muted,
                    inherit_color: false,
                })
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    UiNode::new(format!("{id}.row"), UiNodeKind::Panel)
        .with_class("settings-row")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            compact: UiCompactMode::None,
            align_items: UiAlign::Center,
            gap: 14.0,
            padding: UiSpacing::xy(4.0, 4.0),
            responsive: vec![raf_ui::UiResponsiveRule {
                max_width: compact_breakpoint,
                flow: Some(UiFlow::Column),
                basis: Some([0.0, SETTINGS_STACKED_ROW_HEIGHT]),
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

fn action_button(
    palette: StudioUiPalette,
    id: &str,
    text_key: &str,
    command: &str,
    primary: bool,
) -> UiNode {
    action_button_with_width(
        palette,
        id,
        text_key,
        command,
        primary,
        if primary { 136.0 } else { 94.0 },
    )
}

fn action_button_with_width(
    palette: StudioUiPalette,
    id: &str,
    text_key: &str,
    command: &str,
    primary: bool,
    width: f32,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if primary {
            "settings-primary-button"
        } else {
            "settings-secondary-button"
        })
        .with_layout(UiLayout::fixed(width, 30.0))
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::button(if primary {
            [18, 18, 20, 255]
        } else {
            tokens.text
        }))
        .with_accessibility_role(UiAccessibilityRole::Button)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
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
        .with_accessibility_role(UiAccessibilityRole::Button)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn settings_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("settings-modal-header".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-modal-resize-handle".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some(tokens.border),
                    text: Some(tokens.text_muted),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-modal-resize-handle".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-modal-footer".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-navigation".to_string()),
                UiStylePatch {
                    fill: Some(tokens.canvas),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-workspace".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-group-label".to_string()),
                UiStylePatch {
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-search".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    text: Some(tokens.text),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-select-trigger".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    text: Some(tokens.text),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-select-trigger".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-select-menu".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(6.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-select-option".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-select-option-selected".to_string()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-select-option".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-select-option-selected".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-nav-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-nav-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-nav-indicator".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent),
                    border_width: Some(0.0),
                    radius: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-nav-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.canvas),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-row".to_string()),
                UiStylePatch {
                    fill: Some(tokens.canvas),
                    radius: Some(4.0),
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
                    border: Some(tokens.accent_hot),
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
                    border: Some(tokens.accent_hot),
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
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-primary-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent),
                    text: Some([18, 18, 20, 255]),
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
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("settings-toggle".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent_hot),
                    text: Some(tokens.text),
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
    use raf_render::api_graphic_basic::ui_surface::{UiInputState, UiSurfaceSession};

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
        assert_eq!(SettingsSection::ALL.len(), 8);
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
    fn settings_surface_uses_semantic_modal_materials_and_exposes_transparency_control() {
        let surface = build_settings_surface(
            StudioUiPalette::IndustrialDark,
            &EngineSettings::default(),
            SettingsSection::Appearance,
        );
        let backdrop = find_node(&surface.root, "settings.backdrop").expect("settings backdrop");
        let modal = find_node(&surface.root, "settings.modal").expect("settings modal");
        let toggle = find_node(&surface.root, "settings.reduce-transparency.row")
            .expect("reduce transparency row");

        assert_eq!(backdrop.material, UiSurfaceMaterial::BackdropScrim);
        assert_eq!(modal.material, UiSurfaceMaterial::ModalSurface);
        assert!(toggle.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            UiAction::SetToggle { key, value }
                if key == "settings.reduce_transparency" && *value
        )));
    }

    #[test]
    fn settings_modal_keeps_the_navigation_rail_and_exposes_a_resize_handle() {
        let surface = build_settings_surface(
            StudioUiPalette::IndustrialDark,
            &EngineSettings::default(),
            SettingsSection::Appearance,
        );
        let navigation = find_node(&surface.root, "settings.navigation").expect("navigation");
        assert_eq!(navigation.layout.basis[0], SETTINGS_NAV_WIDTH);
        assert!(navigation.layout.responsive.is_empty());

        let handle =
            find_node(&surface.root, "settings.modal.resize-handle").expect("modal resize handle");
        assert_eq!(
            handle.layout.rect,
            Some(UiRect::new(1060.0, 700.0, 20.0, 20.0))
        );
        assert_eq!(handle.layout.z_index, 60);
        assert!(handle.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            UiAction::Command { name } if name == "settings.modal.resize.start"
        )));
        assert!(handle.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            UiAction::Command { name } if name == "settings.modal.resize.move"
        )));
        assert!(handle.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            UiAction::Command { name } if name == "settings.modal.resize.end"
        )));
    }

    #[test]
    fn settings_rows_only_stack_at_a_genuinely_narrow_width() {
        let surface = build_settings_surface(
            StudioUiPalette::IndustrialDark,
            &EngineSettings::default(),
            SettingsSection::Editor,
        );
        let row = find_node(&surface.root, "settings.grid_size.row").expect("range row");
        assert_eq!(row.layout.compact, UiCompactMode::None);
        assert_eq!(row.layout.responsive.len(), 1);
        assert_eq!(
            row.layout.responsive[0].max_width,
            SETTINGS_COMPACT_BREAKPOINT
        );
        assert_eq!(
            row.layout.responsive[0].basis,
            Some([0.0, SETTINGS_STACKED_ROW_HEIGHT])
        );
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
    fn settings_range_values_are_literal_text_with_the_configured_unit() {
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
            .is_some_and(|text| text.ends_with(" m")));
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
    fn settings_is_a_movable_dialog_with_a_real_select_popup_contract() {
        let surface = build_settings_surface_with_state(
            StudioUiPalette::IndustrialDark,
            &EngineSettings::default(),
            SettingsSection::Performance,
            UiRect::new(32.0, 40.0, 920.0, 640.0),
            "",
            Some("settings.quality.control"),
        );
        let modal = find_node(&surface.root, "settings.modal").expect("settings modal");
        assert_eq!(modal.kind, UiNodeKind::FloatingPanel);
        assert_eq!(modal.layout.position_mode, raf_ui::UiPositionMode::Absolute);
        assert_eq!(
            modal.layout.rect,
            Some(UiRect::new(32.0, 40.0, 920.0, 640.0))
        );
        assert!(find_node(&surface.root, "settings.backdrop")
            .expect("settings backdrop")
            .event_handlers
            .iter()
            .any(|binding| matches!(
                &binding.action,
                UiAction::Command { name } if name == "settings.dismiss"
            )));

        let trigger = find_node(&surface.root, "settings.quality.control").expect("select");
        let select = trigger.control.select().expect("UiSelect control");
        assert!(select.open);
        assert_eq!(select.popup_id.as_deref(), Some("settings.quality.menu"));
        let option = find_node(&surface.root, "settings.quality.option.high").expect("option");
        assert!(option.focusable);
        assert!(option.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            UiAction::SetSelect { key, value, .. }
                if key == "settings.quality" && value == "high"
        )));
        let reset = find_node(&surface.root, "settings.reset-defaults").expect("reset defaults");
        assert!(reset.focusable);
        assert!(reset.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            UiAction::Command { name } if name == "settings.reset_defaults"
        )));
    }

    #[test]
    fn settings_select_popup_keeps_ordered_hitboxes_and_selection_indices() {
        let surface = build_settings_surface_with_state(
            StudioUiPalette::IndustrialDark,
            &EngineSettings::default(),
            SettingsSection::Performance,
            UiRect::new(32.0, 40.0, 920.0, 640.0),
            "",
            Some("settings.render_execution_policy.control"),
        );
        let menu = find_node(&surface.root, "settings.render_execution_policy.menu")
            .expect("render policy menu");
        assert_eq!(menu.layout.flow, UiFlow::Column);
        assert_eq!(
            menu.children
                .iter()
                .filter_map(|option| option.text_key.as_deref())
                .collect::<Vec<_>>(),
            vec![
                "settings.render_execution_policy.auto",
                "settings.render_execution_policy.cpu_only",
                "settings.render_execution_policy.gpu_preferred",
            ]
        );

        let mut session = UiSurfaceSession::default();
        let frame =
            session.build_frame_with_resolved_text(&surface, 1280, 800, [0, 0, 0, 255], |key| {
                key.to_string()
            });
        let mut option_rects = menu
            .children
            .iter()
            .filter_map(|option| {
                frame
                    .layout_boxes
                    .iter()
                    .find(|layout| layout.id == option.id)
                    .map(|layout| layout.rect)
            })
            .collect::<Vec<_>>();
        option_rects.sort_by(|left, right| left.y.total_cmp(&right.y));
        assert_eq!(option_rects.len(), 3);
        assert!(option_rects
            .iter()
            .all(|rect| (rect.height - 30.0).abs() < f32::EPSILON));
        assert!(option_rects
            .windows(2)
            .all(|pair| pair[1].y >= pair[0].bottom()));

        let option = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "settings.render_execution_policy.option.cpu_only")
            .expect("CPU-only option");
        let point = [option.rect.x + 12.0, option.rect.y + 12.0];
        let _ = session.process_input(
            &surface,
            &frame,
            &UiInputState {
                pointer_position: Some(point),
                pointer_down: true,
                time_seconds: 0.1,
                ..UiInputState::default()
            },
        );
        let actions = session.process_input(
            &surface,
            &frame,
            &UiInputState {
                pointer_position: Some(point),
                time_seconds: 0.2,
                ..UiInputState::default()
            },
        );
        assert!(actions.iter().any(|action| matches!(
            &action.action,
            UiAction::SetSelect {
                key,
                value,
                index: 1,
            } if key == "settings.render_execution_policy" && value == "cpu_only"
        )));
    }

    #[test]
    fn settings_omits_nodes_but_keeps_game_and_electronics_sections_distinct() {
        let mut text_keys = Vec::new();
        let game = build_settings_surface(
            StudioUiPalette::IndustrialDark,
            &EngineSettings::default(),
            SettingsSection::Viewport,
        );
        collect_text_keys(&game.root, &mut text_keys);
        assert!(text_keys.iter().all(|key| !key.contains("nodes")));
        assert!(find_node(&game.root, "settings.group.game.label").is_some());

        let electronics = build_settings_surface(
            StudioUiPalette::IndustrialDark,
            &EngineSettings::default(),
            SettingsSection::Electronics,
        );
        assert!(find_node(&electronics.root, "settings.group.electronics.label").is_some());
        assert!(find_node(&electronics.root, "settings.electronics-grid-step.row").is_some());
    }

    #[test]
    fn viewport_render_settings_expose_only_lit() {
        let surface = build_settings_surface_with_state(
            StudioUiPalette::IndustrialDark,
            &EngineSettings::default(),
            SettingsSection::Viewport,
            UiRect::new(32.0, 40.0, 920.0, 640.0),
            "",
            Some("settings.viewport-render-mode.control"),
        );
        let trigger = find_node(&surface.root, "settings.viewport-render-mode.control")
            .expect("viewport render select");
        let select = trigger.control.select().expect("UiSelect control");

        assert_eq!(select.options.len(), 1);
        assert_eq!(select.options[0].value, "solid");
        assert_eq!(
            select.options[0].label_key,
            "settings.viewport_render_mode.solid"
        );
        assert!(find_node(
            &surface.root,
            "settings.viewport-render-mode.option.wireframe"
        )
        .is_none());
        assert!(find_node(
            &surface.root,
            "settings.viewport-render-mode.option.preview"
        )
        .is_none());
    }
}
