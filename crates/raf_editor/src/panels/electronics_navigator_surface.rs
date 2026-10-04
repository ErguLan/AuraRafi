//! RafUI navigator and component library for the native Electronics workspace.
//!
//! This surface deliberately contains presentation only. Selecting a row or
//! a library card emits a semantic command; the native Electronics controller
//! owns the document mutation and placement.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiIcon, UiIconId, UiIconSize, UiSurface,
};
use raf_ui::{
    UiAccessibilityRole, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiFontWeight, UiImage,
    UiImageFit, UiImageSource, UiLayout, UiNode, UiNodeKind, UiOverflow, UiScrollAxis, UiSizeMode,
    UiSpacing, UiStylePatch, UiStyleRuleState, UiStyleSheet, UiTextInput, UiTextOverflow,
    UiTextRole, UiTextStyle,
};

use crate::panels::electronics_surface::{
    electronics_active_rule, electronics_body_style, electronics_class_rule,
    electronics_disabled_rule, electronics_flat_class_rule, electronics_focus_rule,
    electronics_hover_rule, electronics_state_rule, ElectronicsIconButton,
    ELECTRONICS_BODY_LINE_HEIGHT, ELECTRONICS_CONTROL_HEIGHT, ELECTRONICS_ITEM_HEIGHT,
    ELECTRONICS_ROW_GLYPH_TRACK, ELECTRONICS_ROW_HEIGHT, PANEL_PADDING_X, PANEL_PADDING_Y,
};

/// Height of one document row, in logical points.
const DOCUMENT_ROW_HEIGHT: f32 = ELECTRONICS_ITEM_HEIGHT;
/// Vertical padding of a document row, in logical points.
const DOCUMENT_ROW_PADDING_Y: f32 =
    (DOCUMENT_ROW_HEIGHT - ELECTRONICS_BODY_LINE_HEIGHT * 2.0) * 0.5;
/// Height of one library card, in logical points.
const LIBRARY_CARD_HEIGHT: f32 = ELECTRONICS_ITEM_HEIGHT - 2.0;
/// Minimum width of a navigator summary cell so its number is never clipped.
const SUMMARY_MIN_WIDTH: f32 = 62.0;
/// Size of the header icon button, in logical points.
const ICON_BUTTON_SIZE: f32 = ELECTRONICS_CONTROL_HEIGHT;
/// Height of the metric strip, in logical points.
const SUMMARY_ROW_HEIGHT: f32 = 34.0;

#[derive(Debug, Clone)]
pub struct ElectronicsNavigatorEntry {
    pub label: String,
    pub secondary: String,
    pub secondary_key: Option<&'static str>,
    pub command: String,
    pub icon: UiIconId,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct ElectronicsLibraryEntry {
    pub index: usize,
    pub name: String,
    pub category: String,
    pub description: String,
    pub favorite: bool,
    pub icon: UiIconId,
    pub image_key: Option<String>,
}

pub fn build_electronics_navigator_surface(
    palette: StudioUiPalette,
    active_tab: &str,
    query: &str,
    schematic_name: &str,
    counts: (usize, usize, usize),
    components: &[ElectronicsNavigatorEntry],
    wires: &[ElectronicsNavigatorEntry],
    library: &[ElectronicsLibraryEntry],
) -> UiSurface {
    let mut root = UiNode::new("electronics.navigator", UiNodeKind::Panel)
        .with_class("electronics-navigator")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            padding: UiSpacing::xy(PANEL_PADDING_X, PANEL_PADDING_Y),
            overflow: UiOverflow::Clip,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(header(palette, schematic_name))
        .with_child(tabs(palette, active_tab))
        .with_child(summary(palette, counts));

    // The search field filters every list in the navigator, not only the
    // catalog: components and wires are the two long lists.
    if matches!(active_tab, "library" | "components" | "wires") {
        root = root.with_child(search(query));
    }

    if active_tab == "library" {
        root = root.with_child(library_list(palette, library, query));
    } else {
        let rows = if active_tab == "wires" {
            wires
        } else {
            components
        };
        // The project tab shows the document list, not a searchable list, so a
        // query left over from another tab must not filter it silently.
        let list_query = if active_tab == "project" { "" } else { query };
        root = root.with_child(document_list(
            palette,
            rows,
            active_tab,
            schematic_name,
            list_query,
        ));
    }

    let mut surface = UiSurface::new("electronics.navigator", palette, root);
    surface.style_sheet = style_sheet(palette);
    surface
}

fn header(palette: StudioUiPalette, schematic_name: &str) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("electronics.navigator.header", UiNodeKind::Toolbar)
        .with_class("electronics-nav-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(8.0, 6.0),
            ..UiLayout::fixed(0.0, ELECTRONICS_ITEM_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("electronics.navigator.header.icon", UiNodeKind::Label)
                .with_icon(UiIcon::new(UiIconId::Schematic).with_size(UiIconSize::Small))
                // A row child with no authored track resolves to zero width and
                // its glyph is never painted, so the leading icon owns one.
                .with_layout(UiLayout::fixed(
                    ELECTRONICS_ROW_GLYPH_TRACK,
                    ELECTRONICS_ITEM_HEIGHT - 12.0,
                )),
        )
        .with_child(
            UiNode::new("electronics.navigator.header.title", UiNodeKind::Label)
                .with_text_key("app.electronics_project")
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::PanelTitle,
                    size_px: 13.0,
                    line_height_px: 17.0,
                    weight: UiFontWeight::Bold,
                    color: tokens.text,
                    inherit_color: false,
                })
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("electronics.navigator.header.name", UiNodeKind::Label)
                .with_text_value(schematic_name.to_string())
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(electronics_body_style(tokens.text_muted))
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            ElectronicsIconButton::new(
                "electronics.navigator.add",
                UiIconId::Add,
                "electronics.navigator.tab:library",
                "app.electronics_open_library",
                "electronics-icon-button",
                ICON_BUTTON_SIZE,
                ICON_BUTTON_SIZE,
            )
            .build(),
        )
}

fn tabs(palette: StudioUiPalette, active: &str) -> UiNode {
    let mut row = UiNode::new("electronics.navigator.tabs", UiNodeKind::Toolbar)
        .with_class("electronics-nav-tabs")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 2.0,
            padding: UiSpacing::xy(2.0, 0.0),
            ..UiLayout::fixed(0.0, ELECTRONICS_CONTROL_HEIGHT).with_width_mode(UiSizeMode::Fill)
        });
    for (id, label_key, icon) in [
        ("project", "app.electronics_project_tab", UiIconId::Project),
        ("library", "app.electronics_library", UiIconId::Assets),
        (
            "components",
            "app.schematic_components",
            UiIconId::Schematic,
        ),
        ("wires", "app.schematic_wires", UiIconId::Move),
    ] {
        let active_tab = active == id;
        row = row.with_child(
            UiNode::new(
                format!("electronics.navigator.tab.{id}"),
                UiNodeKind::Button,
            )
            .with_class("electronics-nav-tab")
            .with_class(if active_tab {
                "electronics-nav-tab-active"
            } else {
                ""
            })
            .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
            .with_text_key(label_key)
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_text_style(electronics_body_style(if active_tab {
                palette.tokens().text
            } else {
                palette.tokens().text_muted
            }))
            .with_layout(UiLayout {
                grow: 1.0,
                min_size: [48.0, ELECTRONICS_CONTROL_HEIGHT],
                ..UiLayout::fixed(0.0, ELECTRONICS_CONTROL_HEIGHT)
            })
            .with_tooltip_key(label_key)
            .with_accessibility_label_key(label_key)
            // A tab role is what puts the strip into the roving arrow-key
            // order of the shared RafUI keyboard traversal.
            .with_accessibility_role(UiAccessibilityRole::Tab)
            .with_accessibility_selected(active_tab)
            .with_accessibility_expanded(active_tab)
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("electronics.navigator.tab:{id}"),
            )),
        );
    }
    row
}

fn summary(palette: StudioUiPalette, counts: (usize, usize, usize)) -> UiNode {
    let tokens = palette.tokens();
    let mut row = UiNode::new("electronics.navigator.summary", UiNodeKind::Toolbar)
        .with_class("electronics-summary")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 4.0,
            ..UiLayout::fixed(0.0, SUMMARY_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
        });
    for (id, label_key, value) in [
        ("components", "app.schematic_components", counts.0),
        ("wires", "app.schematic_wires", counts.1),
        ("nets", "app.schematic_nets", counts.2),
    ] {
        row = row.with_child(
            UiNode::new(format!("electronics.summary.{id}"), UiNodeKind::Panel)
                .with_class("electronics-summary-card")
                .with_accessibility_role(UiAccessibilityRole::Status)
                .with_accessibility_label_key(label_key)
                // The count is a runtime value, so the cell declares a minimum
                // track and content sizing instead of a guessed width.
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 0.0,
                    padding: UiSpacing::xy(7.0, 4.0),
                    grow: 1.0,
                    min_size: [SUMMARY_MIN_WIDTH, 32.0],
                    ..UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::MinContent)
                })
                .with_child(
                    UiNode::new(format!("electronics.summary.{id}.value"), UiNodeKind::Label)
                        .with_text_value(value.to_string())
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::PanelTitle,
                            size_px: 13.0,
                            line_height_px: 15.0,
                            weight: UiFontWeight::Bold,
                            color: tokens.text,
                            inherit_color: false,
                        }),
                )
                .with_child(
                    UiNode::new(format!("electronics.summary.{id}.label"), UiNodeKind::Label)
                        .with_text_key(label_key)
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_text_style(electronics_body_style(tokens.text_muted)),
                ),
        );
    }
    row
}

fn search(query: &str) -> UiNode {
    UiNode::text_input(
        "electronics.library.search",
        UiTextInput {
            value_key: "electronics.library.search".to_string(),
            placeholder_key: Some("app.electronics_search_components".to_string()),
            max_length: 256,
            multiline: false,
            password: false,
            submit_command: None,
        },
    )
    .with_class("electronics-search")
    .with_icon(UiIcon::new(UiIconId::Search).with_size(UiIconSize::Small))
    .with_layout(UiLayout {
        min_size: [80.0, ELECTRONICS_CONTROL_HEIGHT],
        ..UiLayout::fixed(0.0, ELECTRONICS_CONTROL_HEIGHT).with_width_mode(UiSizeMode::Fill)
    })
    .with_accessibility_role(UiAccessibilityRole::Textbox)
    .with_accessibility_label_key("app.electronics_search_components")
    .with_text_value(query.to_string())
}

fn library_list(
    palette: StudioUiPalette,
    entries: &[ElectronicsLibraryEntry],
    query: &str,
) -> UiNode {
    let query = query.trim().to_lowercase();
    let mut list = list_node("electronics.library.list");
    let mut category = String::new();
    let mut group = 0usize;
    let mut visible_count = 0usize;
    for entry in entries.iter().filter(|entry| {
        query.is_empty()
            || entry.name.to_lowercase().contains(&query)
            || entry.category.to_lowercase().contains(&query)
            || entry.description.to_lowercase().contains(&query)
    }) {
        visible_count += 1;
        if category != entry.category {
            category = entry.category.clone();
            list = list.with_child(category_label(palette, group, &category));
            group += 1;
        }
        list = list.with_child(library_card(palette, entry));
    }
    if visible_count == 0 {
        list = list.with_child(empty_row(
            palette,
            "electronics.library.empty",
            if query.is_empty() {
                "app.electronics_no_components"
            } else {
                "app.electronics_no_components_match"
            },
        ));
    }
    list
}

/// Catalog group header.
///
/// The catalog category is backend data in English, so it is resolved through a
/// real i18n key instead of uppercasing the raw value. The identity comes from
/// the group ordinal: two backend spellings that resolve to the same key
/// ("passive" and "passives") would otherwise emit the same retained id twice.
fn category_label(palette: StudioUiPalette, group: usize, category: &str) -> UiNode {
    let category_key = library_category_key(category);
    UiNode::new(
        format!("electronics.library.category.{group}"),
        UiNodeKind::Label,
    )
    .with_class("electronics-category")
    .with_text_key(category_key)
    .with_text_overflow(UiTextOverflow::Ellipsis)
    .with_text_style(UiTextStyle {
        role: UiTextRole::Label,
        size_px: 10.0,
        line_height_px: 14.0,
        weight: UiFontWeight::Bold,
        color: palette.tokens().accent,
        inherit_color: false,
    })
    .with_layout(
        UiLayout::fixed(0.0, ELECTRONICS_ROW_HEIGHT)
            .with_width_mode(UiSizeMode::Fill)
            .with_text_safe_area(true),
    )
}

/// Localized key for a backend catalog category.
fn library_category_key(category: &str) -> &'static str {
    match category.to_ascii_lowercase().as_str() {
        "passive" | "passives" => "app.electronics_passive",
        "diode" | "diodes" => "app.electronics_diodes",
        "magnet" | "magnets" => "app.electronics_magnets",
        "power" => "app.electronics_power",
        _ => "app.electronics_other",
    }
}

fn library_card(palette: StudioUiPalette, entry: &ElectronicsLibraryEntry) -> UiNode {
    let tokens = palette.tokens();
    let title = if entry.favorite {
        format!("*  {}", entry.name)
    } else {
        entry.name.clone()
    };
    let icon = if let Some(image_key) = entry.image_key.as_deref() {
        UiNode::image(
            format!("electronics.library.card.{}.image", entry.index),
            UiImage {
                source: UiImageSource::new(image_key),
                fit: UiImageFit::Contain,
                tint: None,
            },
        )
        .with_layout(UiLayout::fixed(24.0, 24.0))
    } else {
        UiNode::new(
            format!("electronics.library.card.{}.icon", entry.index),
            UiNodeKind::Label,
        )
        .with_icon(UiIcon::new(entry.icon).with_size(UiIconSize::Small))
        .with_layout(UiLayout::fixed(24.0, 24.0))
    };
    UiNode::new(
        format!("electronics.library.card.{}", entry.index),
        UiNodeKind::Button,
    )
    .with_class("electronics-library-card")
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 7.0,
        padding: UiSpacing::xy(7.0, 3.0),
        ..UiLayout::fixed(0.0, LIBRARY_CARD_HEIGHT).with_width_mode(UiSizeMode::Fill)
    })
    .with_accessibility_role(UiAccessibilityRole::Button)
    .with_accessibility_label_key("app.electronics_place_component")
    .with_accessibility_description_key("electronics.tooltip.place")
    .with_child(icon)
    .with_child(
        UiNode::new(
            format!("electronics.library.card.{}.text", entry.index),
            UiNodeKind::Panel,
        )
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 0.0,
            grow: 1.0,
            ..UiLayout::fixed(0.0, ELECTRONICS_BODY_LINE_HEIGHT * 2.0)
                .with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(
                format!("electronics.library.card.{}.title", entry.index),
                UiNodeKind::Label,
            )
            .with_text_value(title)
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_text_style(UiTextStyle {
                role: UiTextRole::Button,
                size_px: 11.0,
                line_height_px: 15.0,
                weight: UiFontWeight::Bold,
                color: tokens.text,
                inherit_color: false,
            })
            .with_layout(
                UiLayout::fixed(0.0, ELECTRONICS_BODY_LINE_HEIGHT)
                    .with_width_mode(UiSizeMode::Fill),
            ),
        )
        .with_child(
            UiNode::new(
                format!("electronics.library.card.{}.description", entry.index),
                UiNodeKind::Label,
            )
            .with_text_value(entry.description.clone())
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_text_style(electronics_body_style(tokens.text_muted))
            .with_layout(
                UiLayout::fixed(0.0, ELECTRONICS_BODY_LINE_HEIGHT)
                    .with_width_mode(UiSizeMode::Fill),
            ),
        ),
    )
    .focusable()
    .with_tooltip_value(entry.description.clone())
    .with_event(UiEventBinding::command(
        UiEventKind::DragStart,
        format!("electronics.library.drag.start.{}", entry.index),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::DragMove,
        format!("electronics.library.drag.move.{}", entry.index),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::DragEnd,
        format!("electronics.library.drag.end.{}", entry.index),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::Click,
        format!("electronics.place.template.{}", entry.index),
    ))
}

fn document_list(
    palette: StudioUiPalette,
    entries: &[ElectronicsNavigatorEntry],
    active_tab: &str,
    schematic_name: &str,
    query: &str,
) -> UiNode {
    let tokens = palette.tokens();
    let mut list = list_node("electronics.document.list");
    if active_tab == "project" {
        list = list
            .with_child(section_label(
                "app.electronics_schematic_section",
                tokens.text_muted,
            ))
            .with_child(document_row(
                palette,
                &ElectronicsNavigatorEntry {
                    label: schematic_name.to_string(),
                    secondary: String::new(),
                    secondary_key: Some("app.electronics_active_document"),
                    command: "electronics.navigator.tab:components".to_string(),
                    icon: UiIconId::Schematic,
                    active: true,
                },
                0,
                "project",
            ));
    }
    let query = query.trim().to_lowercase();
    let mut visible = 0usize;
    for (index, entry) in entries.iter().enumerate() {
        if !query.is_empty() && !entry_matches(entry, &query) {
            continue;
        }
        visible += 1;
        let row_index = if active_tab == "project" {
            index + 1
        } else {
            index
        };
        list = list.with_child(document_row(palette, entry, row_index, active_tab));
    }
    if visible == 0 && active_tab != "project" {
        let message = if query.is_empty() {
            "app.electronics_no_items"
        } else {
            "app.electronics_no_items_match"
        };
        list = list.with_child(empty_row(palette, "electronics.document.empty", message));
    }
    list
}

/// Whether a document row matches the navigator query.
fn entry_matches(entry: &ElectronicsNavigatorEntry, query: &str) -> bool {
    entry.label.to_lowercase().contains(query) || entry.secondary.to_lowercase().contains(query)
}

fn list_node(id: &str) -> UiNode {
    UiNode::scroll_view(id, UiScrollAxis::Vertical)
        .with_class("electronics-list")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            padding: UiSpacing::xy(1.0, 2.0),
            grow: 1.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        })
}

fn empty_row(palette: StudioUiPalette, id: &str, message_key: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Label)
        .with_text_key(message_key)
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_text_style(electronics_body_style(palette.tokens().text_muted))
        .with_layout(
            UiLayout::fixed(0.0, ELECTRONICS_ROW_HEIGHT)
                .with_width_mode(UiSizeMode::Fill)
                .with_text_safe_area(true),
        )
}

fn document_row(
    palette: StudioUiPalette,
    entry: &ElectronicsNavigatorEntry,
    index: usize,
    active_tab: &str,
) -> UiNode {
    let tokens = palette.tokens();
    let secondary_id = format!("electronics.document.row.{index}.secondary");
    let mut secondary = UiNode::new(secondary_id, UiNodeKind::Label)
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_text_style(electronics_body_style(tokens.text_muted))
        .with_layout(
            UiLayout::fixed(0.0, ELECTRONICS_BODY_LINE_HEIGHT).with_width_mode(UiSizeMode::Fill),
        );
    secondary = if let Some(key) = entry.secondary_key {
        secondary.with_text_key(key)
    } else {
        secondary.with_text_value(entry.secondary.clone())
    };
    UiNode::new(
        format!("electronics.document.row.{index}"),
        UiNodeKind::Button,
    )
    .with_class("electronics-document-row")
    .with_class(if entry.active {
        "electronics-document-row-active"
    } else {
        ""
    })
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 7.0,
        // The vertical padding is what leaves exactly two body line boxes inside
        // the shared two-line item height.
        padding: UiSpacing::xy(8.0, DOCUMENT_ROW_PADDING_Y),
        ..UiLayout::fixed(0.0, DOCUMENT_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(
            format!("electronics.document.row.{index}.icon"),
            UiNodeKind::Label,
        )
        .with_icon(UiIcon::new(entry.icon).with_size(UiIconSize::Small))
        .with_layout(UiLayout::fixed(
            ELECTRONICS_ROW_GLYPH_TRACK,
            ELECTRONICS_ITEM_HEIGHT - 2.0 * DOCUMENT_ROW_PADDING_Y,
        )),
    )
    .with_child(
        UiNode::new(
            format!("electronics.document.row.{index}.text"),
            UiNodeKind::Panel,
        )
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 0.0,
            grow: 1.0,
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fixed,
            basis: [0.0, ELECTRONICS_BODY_LINE_HEIGHT * 2.0],
            ..UiLayout::default()
        })
        .with_child(
            UiNode::new(
                format!("electronics.document.row.{index}.label"),
                UiNodeKind::Label,
            )
            .with_text_value(entry.label.clone())
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_text_style(electronics_body_style(tokens.text))
            .with_layout(
                UiLayout::fixed(0.0, ELECTRONICS_BODY_LINE_HEIGHT)
                    .with_width_mode(UiSizeMode::Fill),
            ),
        )
        .with_child(secondary),
    )
    .with_accessibility_role(UiAccessibilityRole::Button)
    // The row label is a runtime value, so the accessible name is the localized
    // kind of the row and the rendered text stays the designator and value.
    .with_accessibility_label_key(row_kind_key(active_tab))
    .with_accessibility_selected(entry.active)
    .focusable()
    .with_tooltip_value(format!("{} - {}", entry.label, entry.secondary))
    .with_event(UiEventBinding::command(
        UiEventKind::Click,
        entry.command.clone(),
    ))
}

/// Localized name of what a navigator row represents.
fn row_kind_key(active_tab: &str) -> &'static str {
    match active_tab {
        "wires" => "app.electronics_wire",
        "project" => "app.electronics_schematic_tab",
        _ => "app.electronics_component",
    }
}

fn section_label(label_key: &str, color: [u8; 4]) -> UiNode {
    UiNode::new(
        format!("electronics.section.{label_key}"),
        UiNodeKind::Label,
    )
    .with_text_key(label_key)
    .with_text_overflow(UiTextOverflow::Ellipsis)
    .with_text_style(electronics_body_style(color))
    .with_layout(
        UiLayout::fixed(0.0, ELECTRONICS_ROW_HEIGHT)
            .with_width_mode(UiSizeMode::Fill)
            .with_text_safe_area(true),
    )
}

fn style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    let mut rules = vec![
        electronics_class_rule(
            "electronics-navigator",
            tokens.background,
            tokens.border,
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-nav-header",
            tokens.surface_raised,
            tokens.border,
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-nav-tabs",
            tokens.surface_alt,
            tokens.border,
            tokens.text,
        ),
        electronics_flat_class_rule("electronics-nav-tab", [0, 0, 0, 0], tokens.text_muted),
        electronics_class_rule(
            "electronics-nav-tab-active",
            tokens.surface_raised,
            tokens.border,
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-summary",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-summary-card",
            tokens.surface_alt,
            tokens.border,
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-search",
            tokens.surface_alt,
            tokens.border,
            tokens.text,
        ),
        electronics_class_rule("electronics-list", [0, 0, 0, 0], [0, 0, 0, 0], tokens.text),
        electronics_class_rule(
            "electronics-category",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text_muted,
        ),
        electronics_flat_class_rule("electronics-library-card", tokens.surface_alt, tokens.text),
        electronics_flat_class_rule("electronics-document-row", [0, 0, 0, 0], tokens.text),
        // The active row uses the selection token instead of a hand-mixed tint.
        electronics_class_rule(
            "electronics-document-row-active",
            tokens.selection,
            tokens.accent,
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-icon-button",
            tokens.surface_alt,
            tokens.border,
            tokens.text_muted,
        ),
    ];
    for class in [
        "electronics-nav-tab",
        "electronics-nav-tab-active",
        "electronics-library-card",
        "electronics-document-row",
        "electronics-document-row-active",
        "electronics-icon-button",
    ] {
        rules.push(electronics_hover_rule(class, tokens));
        rules.push(electronics_focus_rule(class, tokens));
        rules.push(electronics_active_rule(class, tokens));
    }
    rules.push(electronics_hover_rule("electronics-summary-card", tokens));
    rules.push(electronics_hover_rule("electronics-search", tokens));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Hovered,
        "electronics-search",
        UiStylePatch {
            fill: Some(tokens.surface_raised),
            border: Some(tokens.accent),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_focus_rule("electronics-search", tokens));
    rules.push(electronics_disabled_rule("electronics-icon-button", tokens));
    UiStyleSheet { rules }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::electronics_surface::assert_electronics_layout_gate;
    use raf_ui::UiAction;
    use raf_ui::UiStyleSelector;

    fn palette() -> StudioUiPalette {
        StudioUiPalette::IndustrialDark
    }

    fn entry(index: usize, active: bool) -> ElectronicsNavigatorEntry {
        ElectronicsNavigatorEntry {
            label: format!("R{index}  10k"),
            secondary: "Passive".to_string(),
            secondary_key: None,
            command: format!("electronics.navigator.select.{index}"),
            icon: UiIconId::Schematic,
            active,
        }
    }

    fn library() -> Vec<ElectronicsLibraryEntry> {
        vec![ElectronicsLibraryEntry {
            index: 0,
            name: "Resistor".to_string(),
            category: "Passive".to_string(),
            description: "Standard resistor".to_string(),
            favorite: false,
            icon: UiIconId::Scale,
            image_key: Some("electronics://library/resistor.png".to_string()),
        }]
    }

    fn build(
        tab: &str,
        query: &str,
        components: &[ElectronicsNavigatorEntry],
        wires: &[ElectronicsNavigatorEntry],
    ) -> UiSurface {
        build_electronics_navigator_surface(
            palette(),
            tab,
            query,
            "Main Schematic",
            (components.len(), wires.len(), 0),
            components,
            wires,
            &library(),
        )
    }

    #[test]
    fn library_card_keeps_icon_and_text_in_the_retained_tree() {
        let surface = build("library", "", &[], &[]);
        let card = surface
            .root
            .find("electronics.library.card.0")
            .expect("library card");
        assert_eq!(card.children.len(), 2);
        assert!(surface
            .root
            .find("electronics.library.card.0.image")
            .is_some());
        let title = surface
            .root
            .find("electronics.library.card.0.title")
            .expect("library card title");
        assert_eq!(title.text_value.as_deref(), Some("Resistor"));
        let description = surface
            .root
            .find("electronics.library.card.0.description")
            .expect("library card description");
        assert_eq!(description.text_value.as_deref(), Some("Standard resistor"));
        assert!(card.event_handlers.iter().any(|event| {
            event.event == UiEventKind::DragStart
                && matches!(&event.action, UiAction::Command { name }
                    if name == "electronics.library.drag.start.0")
        }));
        assert!(card.event_handlers.iter().any(|event| {
            event.event == UiEventKind::DragEnd
                && matches!(&event.action, UiAction::Command { name }
                    if name == "electronics.library.drag.end.0")
        }));
    }

    #[test]
    fn tabs_declare_the_tab_role_so_arrow_keys_reach_them() {
        let surface = build("components", "", &[], &[]);
        let tab = surface
            .root
            .find("electronics.navigator.tab.components")
            .expect("components tab");
        assert_eq!(tab.accessibility_role, UiAccessibilityRole::Tab);
        assert_eq!(tab.accessibility_selected, Some(true));
        assert_eq!(tab.accessibility_expanded, Some(true));
        let project = surface
            .root
            .find("electronics.navigator.tab.project")
            .expect("project tab");
        assert_eq!(project.accessibility_selected, Some(false));
    }

    #[test]
    fn a_row_never_uses_a_runtime_value_as_an_accessibility_key() {
        let surface = build("components", "", &[entry(0, true), entry(1, false)], &[]);
        let row = surface
            .root
            .find("electronics.document.row.0")
            .expect("row");
        assert_eq!(
            row.accessibility_label_key.as_deref(),
            Some("app.electronics_component")
        );
        assert_eq!(row.accessibility_selected, Some(true));
        let inactive = surface
            .root
            .find("electronics.document.row.1")
            .expect("row");
        assert_eq!(inactive.accessibility_selected, Some(false));
    }

    #[test]
    fn the_search_field_filters_components_and_wires_too() {
        let matching = build("components", "r0", &[entry(0, false)], &[]);
        assert!(matching.root.find("electronics.document.empty").is_none());
        assert!(matching.root.find("electronics.document.row.0").is_some());
        let filtered = build("components", "zzz", &[entry(0, false)], &[]);
        assert!(filtered.root.find("electronics.document.empty").is_some());
        let wires = build("wires", "passive", &[], &[entry(0, false)]);
        assert!(wires.root.find("electronics.document.row.0").is_some());
    }

    #[test]
    fn the_project_tab_is_never_filtered_by_a_query_from_another_tab() {
        let surface = build("project", "zzz", &[entry(0, false)], &[]);
        assert!(surface.root.find("electronics.document.row.0").is_some());
        assert!(surface.root.find("electronics.document.empty").is_none());
    }

    #[test]
    fn library_categories_use_a_localized_key_instead_of_an_uppercased_value() {
        let surface = build("library", "", &[], &[]);
        let category = surface
            .root
            .find("electronics.library.category.0")
            .expect("category header");
        assert_eq!(
            category.text_key.as_deref(),
            Some("app.electronics_passive")
        );
        assert!(category.text_value.is_none());
        assert_eq!(library_category_key("Diodes"), "app.electronics_diodes");
        assert_eq!(library_category_key("unknown"), "app.electronics_other");
    }

    #[test]
    fn two_backend_spellings_of_one_category_never_share_a_retained_id() {
        let mut entries = library();
        entries.push(ElectronicsLibraryEntry {
            index: 1,
            name: "Capacitor".to_string(),
            category: "passives".to_string(),
            description: "Standard capacitor".to_string(),
            favorite: false,
            icon: UiIconId::Scale,
            image_key: None,
        });
        let surface = build_electronics_navigator_surface(
            palette(),
            "library",
            "",
            "Main Schematic",
            (0, 0, 0),
            &[],
            &[],
            &entries,
        );
        assert!(surface
            .root
            .find("electronics.library.category.0")
            .is_some());
        assert!(surface
            .root
            .find("electronics.library.category.1")
            .is_some());
        assert_electronics_layout_gate(&surface, 224, 640);
    }

    #[test]
    fn the_navigator_passes_the_retained_layout_gate() {
        for tab in ["project", "library", "components", "wires"] {
            let surface = build(tab, "", &[entry(0, true), entry(1, false)], &[]);
            assert_electronics_layout_gate(&surface, 224, 720);
        }
    }

    #[test]
    fn every_two_line_line_box_fits_the_row_it_is_measured_against() {
        let surface = build("components", "", &[entry(0, false)], &[]);
        let row = surface
            .root
            .find("electronics.document.row.0")
            .expect("document row");
        let label = surface
            .root
            .find("electronics.document.row.0.label")
            .expect("row label");
        let secondary = surface
            .root
            .find("electronics.document.row.0.secondary")
            .expect("row secondary");
        assert_eq!(
            label.layout.basis[1] + secondary.layout.basis[1],
            row.layout.basis[1] - 2.0 * DOCUMENT_ROW_PADDING_Y,
            "two body lines must fit the padded content box of the row"
        );
        assert_eq!(label.layout.basis[1], ELECTRONICS_BODY_LINE_HEIGHT);
        assert_eq!(secondary.layout.basis[1], ELECTRONICS_BODY_LINE_HEIGHT);

        let card = build("library", "", &[], &[]);
        let library_card = card
            .root
            .find("electronics.library.card.0")
            .expect("library card");
        let title = card
            .root
            .find("electronics.library.card.0.title")
            .expect("card title");
        let description = card
            .root
            .find("electronics.library.card.0.description")
            .expect("card description");
        assert_eq!(title.layout.basis[1], ELECTRONICS_BODY_LINE_HEIGHT);
        assert_eq!(description.layout.basis[1], ELECTRONICS_BODY_LINE_HEIGHT);
        assert_eq!(
            library_card.layout.basis[1], LIBRARY_CARD_HEIGHT,
            "the card keeps one shared two-line item height"
        );
    }

    #[test]
    fn every_navigator_control_shares_the_family_control_height() {
        let surface = build("components", "r", &[entry(0, false)], &[]);
        for id in [
            "electronics.navigator.tab.components",
            "electronics.library.search",
            "electronics.navigator.add",
        ] {
            let node = surface.root.find(id).expect("control");
            assert_eq!(node.layout.basis[1], ELECTRONICS_CONTROL_HEIGHT, "{id}");
        }
        assert_eq!(DOCUMENT_ROW_HEIGHT, ELECTRONICS_ITEM_HEIGHT);
    }

    #[test]
    fn summary_cells_declare_a_minimum_track_instead_of_a_guessed_width() {
        let surface = build("project", "", &[], &[]);
        let card = surface
            .root
            .find("electronics.summary.components")
            .expect("summary card");
        assert_eq!(card.layout.min_size[0], SUMMARY_MIN_WIDTH);
    }

    #[test]
    fn every_interactive_navigator_control_has_a_focus_rule() {
        let surface = build("components", "", &[entry(0, false)], &[]);
        for class in [
            "electronics-nav-tab",
            "electronics-document-row",
            "electronics-library-card",
            "electronics-icon-button",
        ] {
            assert!(
                surface.style_sheet.rules.iter().any(|rule| {
                    rule.selector == UiStyleSelector::Class(class.to_string())
                        && rule.state == UiStyleRuleState::Focused
                }),
                "{class} needs a focus ring"
            );
            assert!(
                surface.style_sheet.rules.iter().any(|rule| {
                    rule.selector == UiStyleSelector::Class(class.to_string())
                        && rule.state == UiStyleRuleState::Active
                }),
                "{class} needs a pressed state"
            );
        }
    }

    #[test]
    fn the_active_row_tint_comes_from_the_selection_token() {
        for palette in [StudioUiPalette::IndustrialDark, StudioUiPalette::PaperLight] {
            let surface = build_electronics_navigator_surface(
                palette,
                "components",
                "",
                "Main",
                (0, 0, 0),
                &[],
                &[],
                &[],
            );
            let rule = surface
                .style_sheet
                .rules
                .iter()
                .find(|rule| {
                    rule.selector
                        == UiStyleSelector::Class("electronics-document-row-active".to_string())
                })
                .expect("active row rule");
            let fill = rule.patch.fill.expect("active row fill");
            let tokens = palette.tokens();
            assert_eq!(fill[0], tokens.selection[0]);
            assert_eq!(fill[3], tokens.selection[3]);
        }
    }

    #[test]
    fn the_summary_status_role_is_never_emitted_on_an_unreachable_node() {
        let surface = build("project", "", &[], &[]);
        let card = surface
            .root
            .find("electronics.summary.components")
            .expect("summary card");
        assert_eq!(card.accessibility_role, UiAccessibilityRole::Status);
        assert!(
            !card.focusable,
            "a status region must not enter the focus order"
        );
    }
}
