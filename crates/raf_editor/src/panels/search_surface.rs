//! Retained global search surface for the Game workbench.
//!
//! Search is intentionally a small overlay instead of another permanent dock.
//! The application owns the indexed results; this module owns presentation and
//! input routing through RafUI.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiIcon, UiIconId, UiIconSize, UiSurface,
};
use raf_ui::{
    UiAccessibilityRole, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiJustify, UiLayout, UiNode,
    UiNodeKind, UiOverflow, UiResponsiveRule, UiScrollAxis, UiSizeMode, UiSpacing, UiStyle,
    UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiTextInput,
    UiTextStyle,
};

#[derive(Debug, Clone, PartialEq)]
pub enum SearchResultKind {
    Command(String),
    Hierarchy(raf_core::scene::SceneNodeId),
    Asset(String),
    Project(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    pub label: String,
    pub detail: String,
    pub kind: SearchResultKind,
    pub icon: UiIconId,
}

/// Presentation state supplied by the host. Results are never described as
/// empty while a worker or search provider is still resolving them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchSurfaceState {
    Ready,
    Loading,
    Error(String),
}

impl Default for SearchSurfaceState {
    fn default() -> Self {
        Self::Ready
    }
}

/// Keeps the public builder useful for small embedders that do not need an
/// asynchronous state. The host uses `build_search_surface_with_state`.
pub fn build_search_surface(
    palette: StudioUiPalette,
    query: &str,
    results: &[SearchResult],
) -> UiSurface {
    build_search_surface_with_state(palette, query, results, &SearchSurfaceState::Ready)
}

pub fn build_search_surface_with_state(
    palette: StudioUiPalette,
    query: &str,
    results: &[SearchResult],
    state: &SearchSurfaceState,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut list = UiNode::scroll_view("search.results", UiScrollAxis::Vertical)
        .with_class("search-results")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            padding: UiSpacing::xy(4.0, 5.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });

    match state {
        SearchSurfaceState::Loading => {
            list = list.with_child(search_state_card(
                palette,
                UiIconId::Search,
                "app.loading",
                "app.search_hint",
                None,
            ));
        }
        SearchSurfaceState::Error(error) => {
            list = list.with_child(search_state_card(
                palette,
                UiIconId::Error,
                "app.error",
                "app.search_hint",
                Some(error),
            ));
        }
        SearchSurfaceState::Ready => {
            if results.is_empty() {
                let (title, hint) = if query.trim().is_empty() {
                    ("app.search_everywhere", "app.search_hint")
                } else {
                    ("app.search_no_results", "app.search_hint")
                };
                list = list.with_child(search_state_card(
                    palette,
                    UiIconId::Search,
                    title,
                    hint,
                    None,
                ));
            } else {
                let mut previous_section = None;
                for index in search_result_order(results) {
                    let result = &results[index];
                    let (section, section_key) = search_section(result);
                    if previous_section != Some(section) {
                        list = list.with_child(section_heading(
                            palette,
                            section_key,
                            format!("search.section.{section_key}.heading"),
                        ));
                        previous_section = Some(section);
                    }
                    list = list.with_child(result_node(palette, index, result));
                }
            }
        }
    }

    let root = UiNode::new("search.root", UiNodeKind::FloatingPanel)
        .with_class("global-search")
        .with_layout(
            UiLayout {
                flow: UiFlow::Column,
                gap: 0.0,
                padding: UiSpacing::xy(8.0, 8.0),
                ..UiLayout::fill(UiFlow::Column)
            }
            .responsive(UiResponsiveRule {
                max_width: 360.0,
                flow: None,
                basis: None,
                padding: Some(UiSpacing::xy(5.0, 5.0)),
                gap: Some(0.0),
                compact: None,
                grid_columns: None,
            }),
        )
        .with_style(UiStyle {
            fill: tokens.surface,
            border: tokens.focus,
            text: tokens.text,
            border_width: 1.0,
            radius: 10.0,
            opacity: 0.99,
        })
        .with_accessibility_role(UiAccessibilityRole::Dialog)
        .with_accessibility_label_key("app.search_everywhere")
        .with_child(search_header(palette))
        .with_child(search_summary(palette, query, results.len(), state))
        .with_child(list);

    let mut surface = UiSurface::new("editor.global-search", palette, root);
    surface.style_sheet = search_style_sheet(palette);
    surface
}

/// Results are grouped in a stable semantic order even when the application
/// builds them from different data sources. Returned indices remain the source
/// indices so activation cannot target a different row after grouping.
pub(crate) fn search_result_order(results: &[SearchResult]) -> Vec<usize> {
    [
        SearchSection::Commands,
        SearchSection::Project,
        SearchSection::Assets,
        SearchSection::Hierarchy,
    ]
    .into_iter()
    .flat_map(|section| {
        results
            .iter()
            .enumerate()
            .filter_map(move |(index, result)| {
                (search_section(result).0 == section).then_some(index)
            })
    })
    .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchSection {
    Commands,
    Project,
    Assets,
    Hierarchy,
}

fn search_section(result: &SearchResult) -> (SearchSection, &'static str) {
    match &result.kind {
        SearchResultKind::Command(_) => (SearchSection::Commands, "commands"),
        SearchResultKind::Project(_) => (SearchSection::Project, "project"),
        SearchResultKind::Asset(_) => (SearchSection::Assets, "assets"),
        SearchResultKind::Hierarchy(_) => (SearchSection::Hierarchy, "hierarchy"),
    }
}

fn search_header(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("search.header", UiNodeKind::Toolbar)
        .with_class("global-search-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(8.0, 6.0),
            min_size: [0.0, 40.0],
            ..UiLayout::fixed(0.0, 40.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("search.header.icon", UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(UiIconId::Search)
                        .with_size(UiIconSize::Toolbar)
                        .with_tint(tokens.accent_hot),
                )
                .with_layout(UiLayout::fixed(20.0, 24.0)),
        )
        .with_child(
            UiNode::text_input(
                "search.input",
                UiTextInput {
                    value_key: "search.query".to_string(),
                    placeholder_key: Some("app.search_everywhere".to_string()),
                    max_length: 256,
                    multiline: false,
                    password: false,
                    submit_command: Some("search.activate-first".to_string()),
                },
            )
            .with_class("global-search-input")
            .with_layout(UiLayout {
                grow: 1.0,
                min_size: [48.0, 28.0],
                ..UiLayout::fixed(0.0, 28.0)
            })
            .with_accessibility_label_key("app.search_everywhere")
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("ArrowDown".to_string()),
                "search.focus-first",
            )),
        )
        .with_child(
            UiNode::new("search.close", UiNodeKind::Button)
                .with_class("global-search-close")
                .with_icon(UiIcon::new(UiIconId::Close).with_size(UiIconSize::Small))
                .with_layout(UiLayout::fixed(28.0, 28.0))
                .with_tooltip_key("app.cancel")
                .with_accessibility_label_key("app.cancel")
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, "search.close")),
        )
}

fn search_summary(
    palette: StudioUiPalette,
    query: &str,
    result_count: usize,
    state: &SearchSurfaceState,
) -> UiNode {
    let tokens = palette.tokens();
    let text_key = match state {
        SearchSurfaceState::Loading => "app.loading",
        SearchSurfaceState::Error(_) => "app.error",
        SearchSurfaceState::Ready if query.trim().is_empty() => "app.search_summary",
        SearchSurfaceState::Ready => "app.search_results",
    };
    let mut row = UiNode::new("search.summary", UiNodeKind::Toolbar)
        .with_class("global-search-summary")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(10.0, 2.0),
            ..UiLayout::fit_content()
                .with_width_mode(UiSizeMode::Fill)
                .with_height_mode(UiSizeMode::FitContent)
        })
        .with_child(
            UiNode::new("search.summary.label", UiNodeKind::Label)
                .with_text_key(text_key)
                .with_layout(
                    UiLayout::fit_content()
                        .with_width_mode(UiSizeMode::Fill)
                        .with_height_mode(UiSizeMode::FitContent),
                )
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    if matches!(state, SearchSurfaceState::Ready) && result_count > 0 {
        row = row.with_child(
            UiNode::new("search.summary.count", UiNodeKind::Label)
                .with_text_value(result_count.to_string())
                .with_class("search-summary-count")
                .with_layout(UiLayout::fit_content())
                .with_text_style(UiTextStyle::button(tokens.accent_hot)),
        );
    }
    row
}

fn section_heading(palette: StudioUiPalette, section_key: &str, id: String) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id.clone(), UiNodeKind::Toolbar)
        .with_class("search-section-heading")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(8.0, 6.0),
            ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(format!("search.section.{section_key}"))
                .with_layout(UiLayout::fit_content())
                .with_text_style(UiTextStyle::panel_title(tokens.text_muted)),
        )
        .with_child(
            UiNode::new(format!("{id}.rule"), UiNodeKind::Separator)
                .with_class("search-section-rule")
                .with_layout(UiLayout {
                    grow: 1.0,
                    min_size: [12.0, 1.0],
                    ..UiLayout::fixed(0.0, 1.0)
                })
                .with_style(UiStyle {
                    fill: tokens.border,
                    border: tokens.border,
                    text: tokens.border,
                    border_width: 0.0,
                    radius: 0.0,
                    opacity: 0.9,
                }),
        )
}

fn result_node(palette: StudioUiPalette, index: usize, result: &SearchResult) -> UiNode {
    let tokens = palette.tokens();
    let (section, _) = search_section(result);
    let icon_tint = match section {
        SearchSection::Commands => tokens.accent_hot,
        SearchSection::Project => tokens.accent,
        SearchSection::Assets => tokens.positive,
        SearchSection::Hierarchy => tokens.warning,
    };
    UiNode::new(format!("search.result.{index}"), UiNodeKind::Button)
        .with_class("search-result")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Start,
            gap: 10.0,
            padding: UiSpacing::xy(10.0, 7.0),
            min_size: [0.0, 48.0],
            ..UiLayout::fit_content()
                .with_width_mode(UiSizeMode::Fill)
                .with_height_mode(UiSizeMode::FitContent)
        })
        .with_child(
            UiNode::new(format!("search.result.{index}.icon"), UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(result.icon)
                        .with_size(UiIconSize::Panel)
                        .with_tint(icon_tint),
                )
                .with_layout(UiLayout::fixed(24.0, 24.0)),
        )
        .with_child(
            UiNode::new(format!("search.result.{index}.content"), UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 1.0,
                    grow: 1.0,
                    min_size: [0.0, 0.0],
                    align_self: Some(UiAlign::Stretch),
                    ..UiLayout::fit_content()
                        .with_width_mode(UiSizeMode::Fill)
                        .with_height_mode(UiSizeMode::FitContent)
                })
                .with_child(
                    UiNode::new(format!("search.result.{index}.label"), UiNodeKind::Label)
                        .with_text_value(result.label.clone())
                        .with_layout(
                            UiLayout::fit_content()
                                .with_width_mode(UiSizeMode::Fill)
                                .with_height_mode(UiSizeMode::FitContent),
                        )
                        .with_text_style(UiTextStyle::button(tokens.text)),
                )
                .with_child(
                    UiNode::new(format!("search.result.{index}.detail"), UiNodeKind::Label)
                        .with_text_value(result.detail.clone())
                        .with_layout(
                            UiLayout::fit_content()
                                .with_width_mode(UiSizeMode::Fill)
                                .with_height_mode(UiSizeMode::FitContent),
                        )
                        .with_text_style(search_detail_style(tokens.text_muted)),
                ),
        )
        .with_accessibility_label_key("app.search_results")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            format!("search.activate:{index}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("Enter".to_string()),
            format!("search.activate:{index}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("ArrowDown".to_string()),
            format!("search.focus-next:{index}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("ArrowUp".to_string()),
            format!("search.focus-previous:{index}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("Home".to_string()),
            "search.focus-first",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("End".to_string()),
            "search.focus-last",
        ))
}

fn search_state_card(
    palette: StudioUiPalette,
    icon: UiIconId,
    title_key: &str,
    hint_key: &str,
    error: Option<&str>,
) -> UiNode {
    let tokens = palette.tokens();
    let mut card = UiNode::new("search.state", UiNodeKind::Panel)
        .with_class("search-state")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 8.0,
            padding: UiSpacing::xy(24.0, 24.0),
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(
            UiNode::new("search.state.icon", UiNodeKind::Label)
                .with_icon(UiIcon::new(icon).with_size(UiIconSize::Panel).with_tint(
                    if error.is_some() {
                        tokens.danger
                    } else {
                        tokens.accent
                    },
                ))
                .with_layout(UiLayout::fixed(28.0, 28.0)),
        )
        .with_child(
            UiNode::new("search.state.title", UiNodeKind::Label)
                .with_text_key(title_key)
                .with_layout(UiLayout::fit_content())
                .with_text_style(UiTextStyle::panel_title(tokens.text)),
        )
        .with_child(
            UiNode::new("search.state.hint", UiNodeKind::Label)
                .with_text_key(hint_key)
                .with_layout(
                    UiLayout::fit_content()
                        .with_width_mode(UiSizeMode::Fill)
                        .with_height_mode(UiSizeMode::FitContent),
                )
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    if let Some(error) = error.filter(|error| !error.trim().is_empty()) {
        card = card.with_child(
            UiNode::new("search.state.error", UiNodeKind::Label)
                .with_text_value(error.to_string())
                .with_layout(
                    UiLayout::fit_content()
                        .with_width_mode(UiSizeMode::Fill)
                        .with_height_mode(UiSizeMode::FitContent),
                )
                .with_text_style(UiTextStyle::body(tokens.danger)),
        );
    }
    card
}

fn search_detail_style(color: [u8; 4]) -> UiTextStyle {
    let mut style = UiTextStyle::body(color);
    style.size_px = 11.0;
    style.line_height_px = 15.0;
    style
}

fn search_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("global-search-header".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(7.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("global-search-input".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(5.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("global-search-input".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("global-search-close".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(5.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("global-search-close".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.focus),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("global-search-close".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.focus),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("search-result".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(5.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("search-result".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("search-result".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("search-state".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(7.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("global-search-summary".to_string()),
                UiStylePatch {
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
        ],
    }
}
