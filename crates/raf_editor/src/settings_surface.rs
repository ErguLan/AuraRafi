//! RafUI presentation for engine-wide settings.
//!
//! This module deliberately owns only composition. Draft state, validation and
//! persistence stay in SettingsSurfaceHost.

use raf_core::ai::AiProvider;
use raf_core::config::{EngineSettings, Language};
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAlign, UiCompactMode, UiEventBinding, UiEventKind,
    UiFlow, UiLayout, UiNode, UiNodeKind, UiOverflow, UiScrollAxis, UiSizeMode, UiSpacing, UiStyle,
    UiSurface, UiSurfaceMaterial, UiTextOverflow, UiTextStyle,
};
use raf_ui::UiRect;

mod ai;
mod controls;
mod general;
mod graphics;
mod hardware;
mod impact;
mod runtime;
mod sections;
mod style;
#[cfg(test)]
mod tests;

pub(crate) use ai::{account_controls, account_welcome, provider_id};
pub use sections::SettingsSection;

use sections::{
    modal_footer, modal_header, navigation, section_content, settings_modal_resize_handle,
};
use style::settings_style_sheet;

const CONTROL_WIDTH: f32 = 320.0;
const SETTINGS_NAV_WIDTH: f32 = 236.0;
const SETTINGS_CONTROL_MIN_WIDTH: f32 = 220.0;
const SETTINGS_RANGE_MIN_WIDTH: f32 = 150.0;
/// Below this width a settings row stacks so the control and the impact
/// badges keep their intrinsic width instead of clipping each other.
const SETTINGS_COMPACT_BREAKPOINT: f32 = 640.0;
const SETTINGS_WORKSPACE_BREAKPOINT: f32 = 440.0;
const SETTINGS_CONTENT_MAX_WIDTH: f32 = 900.0;
const SEGMENT_MIN_WIDTH: f32 = 68.0;
const SETTINGS_ROW_HEIGHT: f32 = 58.0;
const SETTINGS_STACKED_ROW_HEIGHT: f32 = 84.0;
const SETTINGS_IMPACT_ROW_HEIGHT: f32 = 70.0;
const SETTINGS_IMPACT_STACKED_ROW_HEIGHT: f32 = 110.0;
pub(crate) const PERFORMANCE_FILTER_COMMAND: &str = "settings.performance_filter.toggle";
const DEFAULT_MODAL_RECT: UiRect = UiRect::new(48.0, 48.0, 1080.0, 720.0);

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
        "",
        false,
        false,
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
        "",
        false,
        false,
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
        "",
        false,
        false,
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
    model_filter: &str,
    draft_dirty: bool,
    performance_only: bool,
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
        );
    let mut modal_node = modal(
        palette,
        settings,
        section,
        modal_rect,
        search_query,
        open_select,
        revealed_api_keys,
        model_filter,
        draft_dirty,
        performance_only,
    );
    apply_filters(
        &mut modal_node,
        search_query,
        performance_only,
        settings.language,
        palette,
    );
    let root = root.with_child(modal_node);

    let mut surface = UiSurface::new("editor.engine-settings", palette, root);
    surface.style_sheet = settings_style_sheet(palette);
    surface
}

/// Hides rows that do not match the active search or the performance filter
/// and removes empty groups. Search matching happens against the same
/// localized copy the user sees, so it follows the current language
/// automatically.
fn apply_filters(
    modal: &mut UiNode,
    query: &str,
    performance_only: bool,
    language: Language,
    palette: StudioUiPalette,
) {
    let query = query.trim().to_lowercase();
    if query.is_empty() && !performance_only {
        return;
    }
    let Some(scroll) = modal.find_mut("settings.workspace.scroll") else {
        return;
    };
    let Some(card) = scroll.children.first_mut() else {
        return;
    };
    let tokens = palette.tokens();
    let no_results_key = if query.is_empty() {
        "settings.impact.no_results"
    } else {
        "settings.no_results"
    };
    // Sections such as AI or Scripting contain custom cards, not only rows.
    // Replace their whole body with a clean empty state instead of leaving
    // headings and controls behind when the performance filter is active.
    if performance_only && impact::summarize(card).performance == 0 {
        card.children.clear();
        card.children
            .push(no_results_node(tokens.text_muted, no_results_key));
        return;
    }
    let visible = filter_settings_rows(card, &query, performance_only, language);
    hide_empty_groups(card);
    if visible == 0 {
        card.children
            .push(no_results_node(tokens.text_muted, no_results_key));
    }
}

fn no_results_node(color: [u8; 4], text_key: &str) -> UiNode {
    UiNode::new("settings.no-results", UiNodeKind::Label)
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::body(color))
        .with_layout(UiLayout::fixed(0.0, 40.0))
}

fn filter_settings_rows(
    node: &mut UiNode,
    query: &str,
    performance_only: bool,
    language: Language,
) -> usize {
    if node.classes.iter().any(|class| class == "settings-row") {
        let matches_query = query.is_empty() || node_matches_query(node, query, language);
        let matches_performance = !performance_only || impact::row_has_performance(node);
        let matched = matches_query && matches_performance;
        node.hidden = !matched;
        return usize::from(matched);
    }
    let mut visible = 0;
    for child in &mut node.children {
        visible += filter_settings_rows(child, query, performance_only, language);
    }
    visible
}

fn node_matches_query(node: &UiNode, query: &str, language: Language) -> bool {
    let mut text = String::new();
    collect_node_text(node, &mut text, language);
    text.to_lowercase().contains(query)
}

fn collect_node_text(node: &UiNode, out: &mut String, language: Language) {
    if let Some(key) = &node.text_key {
        out.push_str(&t(key, language));
        out.push(' ');
    }
    if let Some(value) = &node.text_value {
        out.push_str(value);
        out.push(' ');
    }
    for child in &node.children {
        collect_node_text(child, out, language);
    }
}

fn hide_empty_groups(card: &mut UiNode) {
    let mut index = 0;
    while index < card.children.len() {
        let is_group = card.children[index]
            .classes
            .iter()
            .any(|class| class == "settings-group-label");
        if !is_group {
            index += 1;
            continue;
        }
        let start = index + 1;
        let mut end = start;
        while end < card.children.len()
            && !card.children[end]
                .classes
                .iter()
                .any(|class| class == "settings-group-label")
        {
            end += 1;
        }
        if !card.children[start..end].iter().any(|child| !child.hidden) {
            card.children[index].hidden = true;
        }
        index = end;
    }
}

fn modal(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    active: SettingsSection,
    modal_rect: UiRect,
    search_query: &str,
    open_select: Option<&str>,
    revealed_api_keys: &[AiProvider],
    model_filter: &str,
    draft_dirty: bool,
    performance_only: bool,
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
                    model_filter,
                    performance_only,
                )),
        )
        .with_child(modal_footer(palette, draft_dirty))
        .with_child(settings_modal_resize_handle(palette, modal_rect))
}

fn workspace(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    section: SettingsSection,
    open_select: Option<&str>,
    revealed_api_keys: &[AiProvider],
    model_filter: &str,
    performance_only: bool,
) -> UiNode {
    let content = section_content(
        palette,
        settings,
        section,
        open_select,
        revealed_api_keys,
        model_filter,
    );
    let summary = impact::summarize(&content);
    UiNode::new("settings.workspace", UiNodeKind::Panel)
        .with_class("settings-workspace")
        .with_layout(UiLayout::fill(UiFlow::Column))
        .with_child(workspace_header(
            palette,
            settings.language,
            section,
            summary,
            performance_only,
        ))
        .with_child(
            UiNode::scroll_view("settings.workspace.scroll", UiScrollAxis::Vertical)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    align_items: UiAlign::Center,
                    grow: 1.0,
                    padding: UiSpacing::xy(24.0, 16.0),
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
                .with_child(content),
        )
}

fn workspace_header(
    palette: StudioUiPalette,
    language: Language,
    section: SettingsSection,
    summary: impact::ImpactSummary,
    performance_only: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let has_impact_rows = summary.performance > 0;
    let show_filter = has_impact_rows || performance_only;
    let mut copy = UiNode::new("settings.workspace.header-copy", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 3.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle::transparent())
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
        );
    if has_impact_rows {
        copy = copy.with_child(impact::summary_label(palette, summary, language));
    }
    let mut content = UiNode::new("settings.workspace.header-content", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 14.0,
            padding: UiSpacing::xy(15.0, 15.0),
            max_size: [SETTINGS_CONTENT_MAX_WIDTH, 0.0],
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle::transparent())
        .with_child(copy);
    if show_filter {
        content = content.with_child(impact::performance_filter_button(palette, performance_only));
    }
    UiNode::new("settings.workspace.header", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            gap: 0.0,
            responsive: vec![raf_ui::UiResponsiveRule {
                max_width: SETTINGS_WORKSPACE_BREAKPOINT,
                flow: Some(UiFlow::Column),
                basis: Some([0.0, 92.0]),
                padding: Some(UiSpacing::xy(0.0, 12.0)),
                gap: Some(0.0),
                compact: Some(UiCompactMode::Stack),
                grid_columns: None,
            }],
            ..UiLayout::fixed(0.0, 92.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(content)
}
