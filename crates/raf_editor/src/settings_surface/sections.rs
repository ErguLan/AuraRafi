//! Section registry, navigation rail and modal chrome composition.

use raf_core::ai::AiProvider;
use raf_core::config::EngineSettings;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout,
    UiNode, UiNodeKind, UiOverflow, UiScrollAxis, UiSizeMode, UiSpacing, UiStyle, UiTextStyle,
};
use raf_ui::components::search_field;
use raf_ui::UiRect;

use super::ai::ai;
use super::controls::{action_button, group_label, quiet_button};
use super::general::{appearance, editor, platform, scripting};
use super::graphics::{performance, viewport};
use super::hardware::electronics;
use super::runtime::runtime_preferences;
use super::SETTINGS_NAV_WIDTH;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Appearance,
    Performance,
    Editor,
    Viewport,
    Electronics,
    Scripting,
    Runtime,
    Ai,
    Platform,
}

impl Default for SettingsSection {
    fn default() -> Self {
        Self::Appearance
    }
}

impl SettingsSection {
    pub const ALL: [Self; 9] = [
        Self::Appearance,
        Self::Performance,
        Self::Editor,
        Self::Viewport,
        Self::Electronics,
        Self::Scripting,
        Self::Runtime,
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
            Self::Runtime => "settings.section.runtime",
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
            Self::Runtime => "runtime.settings",
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
            Self::Runtime => "settings.surface.help.runtime",
            Self::Ai => "settings.surface.help.ai",
            Self::Platform => "settings.surface.help.platform",
        }
    }

    pub const fn group_key(self) -> &'static str {
        match self {
            Self::Appearance | Self::Performance | Self::Editor => "settings.group.general",
            Self::Viewport | Self::Electronics => "settings.group.workspace",
            Self::Scripting | Self::Runtime | Self::Ai => "settings.group.tools",
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
                "texture",
                "textura",
                "packet",
                "paquete",
                "upload",
                "subida",
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
            Self::Runtime => &[
                "runtime",
                "play",
                "pause",
                "ejecucion",
                "ventana",
                "compilacion",
                "instances",
                "instancias",
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

pub(super) fn modal_header(palette: StudioUiPalette) -> UiNode {
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

pub(super) fn navigation(
    palette: StudioUiPalette,
    active: SettingsSection,
    search_query: &str,
) -> UiNode {
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

pub(super) fn modal_footer(palette: StudioUiPalette, draft_dirty: bool) -> UiNode {
    let tokens = palette.tokens();
    let status_color = if draft_dirty {
        tokens.accent
    } else {
        tokens.text_muted
    };
    let status = UiNode::new("settings.footer.status", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            ..UiLayout::fit_content()
        })
        .with_child(
            UiNode::new("settings.footer.dot", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(7.0, 7.0))
                .with_style(UiStyle {
                    fill: status_color,
                    border: status_color,
                    text: tokens.text_muted,
                    border_width: 0.0,
                    radius: 3.5,
                    opacity: 1.0,
                }),
        )
        .with_child(
            UiNode::new("settings.footer.status-label", UiNodeKind::Label)
                .with_text_key(if draft_dirty {
                    "app.unsaved_changes_title"
                } else {
                    "settings.all_saved"
                })
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    UiNode::new("settings.modal.footer", UiNodeKind::Toolbar)
        .with_class("settings-modal-footer")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(18.0, 10.0),
            ..UiLayout::fixed(0.0, 56.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(status)
        .with_child(quiet_button(
            palette,
            "settings.reset-defaults",
            "settings.reset_defaults",
            "settings.reset_defaults",
            132.0,
        ))
        .with_child(
            UiNode::new("settings.footer.spacer", UiNodeKind::Panel).with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::default()
            }),
        )
        .with_child(quiet_button(
            palette,
            "settings.cancel",
            "app.cancel",
            "settings.cancel",
            94.0,
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

pub(super) fn settings_modal_resize_handle(palette: StudioUiPalette, modal_rect: UiRect) -> UiNode {
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

pub(super) fn section_content(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    section: SettingsSection,
    open_select: Option<&str>,
    revealed_api_keys: &[AiProvider],
    model_filter: &str,
) -> UiNode {
    match section {
        SettingsSection::Appearance => appearance(palette, settings, open_select),
        SettingsSection::Performance => performance(palette, settings, open_select),
        SettingsSection::Editor => editor(palette, settings),
        SettingsSection::Viewport => viewport(palette, settings, open_select),
        SettingsSection::Electronics => electronics(palette, settings),
        SettingsSection::Scripting => scripting(palette, settings, open_select),
        SettingsSection::Runtime => runtime_preferences(palette, settings, open_select),
        SettingsSection::Ai => ai(
            palette,
            settings,
            open_select,
            revealed_api_keys,
            model_filter,
        ),
        SettingsSection::Platform => platform(palette, settings, open_select),
    }
}
