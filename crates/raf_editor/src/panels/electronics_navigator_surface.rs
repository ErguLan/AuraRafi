//! RafUI navigator and component library for the native Electronics workspace.
//!
//! This surface deliberately contains presentation only. Selecting a row or
//! a library card emits a semantic command; the native Electronics controller
//! owns the document mutation and placement.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiIcon, UiIconId, UiIconSize, UiSurface,
};
use raf_ui::{
    UiAlign, UiEventBinding, UiEventKind, UiFlow, UiFontWeight, UiImage, UiImageFit, UiImageSource,
    UiLayout, UiNode, UiNodeKind, UiOverflow, UiScrollAxis, UiSizeMode, UiSpacing, UiStylePatch,
    UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiTextInput, UiTextRole,
    UiTextStyle,
};

#[derive(Debug, Clone)]
pub struct ElectronicsNavigatorEntry {
    pub label: String,
    pub secondary: String,
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
        .with_layout(raf_ui::UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            padding: raf_ui::UiSpacing::xy(8.0, 8.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(header(palette, schematic_name))
        .with_child(tabs(palette, active_tab))
        .with_child(summary(palette, counts));

    if active_tab == "library" {
        root = root
            .with_child(search(query))
            .with_child(library_list(palette, library, query));
    } else {
        let rows = if active_tab == "wires" {
            wires
        } else {
            components
        };
        root = root.with_child(document_list(palette, rows, active_tab, schematic_name));
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
            ..UiLayout::fixed(0.0, 38.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("electronics.navigator.header.icon", UiNodeKind::Label)
                .with_icon(UiIcon::new(UiIconId::Schematic).with_size(UiIconSize::Small)),
        )
        .with_child(
            UiNode::new("electronics.navigator.header.title", UiNodeKind::Label)
                .with_text_value("PROJECT".to_string())
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
                .with_text_style(body_style(tokens.text_muted))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
        )
        .with_child(icon_button(
            "electronics.navigator.add",
            UiIconId::Add,
            "electronics.navigator.tab:library",
            "Open component library",
        ))
}

fn tabs(palette: StudioUiPalette, active: &str) -> UiNode {
    let mut row = UiNode::new("electronics.navigator.tabs", UiNodeKind::Toolbar)
        .with_class("electronics-nav-tabs")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 2.0,
            padding: UiSpacing::xy(2.0, 2.0),
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        });
    for (id, label, icon) in [
        ("project", "Project", UiIconId::Project),
        ("library", "Library", UiIconId::Assets),
        ("components", "Components", UiIconId::Schematic),
        ("wires", "Wires", UiIconId::Move),
    ] {
        let active_tab = active == id;
        let tab = UiNode::new(
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
        .with_text_value(label.to_string())
        .with_text_style(body_style(if active_tab {
            palette.tokens().text
        } else {
            palette.tokens().text_muted
        }))
        .with_layout(UiLayout {
            grow: 1.0,
            min_size: [48.0, 26.0],
            ..UiLayout::fixed(0.0, 26.0)
        })
        .with_tooltip_value(format!("{} view", label))
        .with_accessibility_label_key(label)
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            format!("electronics.navigator.tab:{id}"),
        ));
        row = row.with_child(tab);
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
            ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
        });
    for (id, label, value) in [
        ("components", "Components", counts.0),
        ("wires", "Wires", counts.1),
        ("nets", "Nets", counts.2),
    ] {
        row = row.with_child(
            UiNode::new(format!("electronics.summary.{id}"), UiNodeKind::Panel)
                .with_class("electronics-summary-card")
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 0.0,
                    padding: UiSpacing::xy(7.0, 4.0),
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 32.0)
                })
                .with_child(
                    UiNode::new(format!("electronics.summary.{id}.value"), UiNodeKind::Label)
                        .with_text_value(value.to_string())
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
                        .with_text_value(label.to_string())
                        .with_text_style(body_style(tokens.text_muted)),
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
            placeholder_key: Some("Search components...".to_string()),
            max_length: 256,
            multiline: false,
            password: false,
            submit_command: None,
        },
    )
    .with_class("electronics-search")
    .with_icon(UiIcon::new(UiIconId::Search).with_size(UiIconSize::Small))
    .with_layout(UiLayout {
        min_size: [80.0, 30.0],
        ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_text_value(query.to_string())
}

fn library_list(
    palette: StudioUiPalette,
    entries: &[ElectronicsLibraryEntry],
    query: &str,
) -> UiNode {
    let query = query.trim().to_lowercase();
    let mut list = UiNode::scroll_view("electronics.library.list", UiScrollAxis::Vertical)
        .with_class("electronics-list")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            padding: UiSpacing::xy(1.0, 2.0),
            grow: 1.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });
    let mut category = String::new();
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
            list = list.with_child(
                UiNode::new(
                    format!("electronics.library.category.{}", category.to_lowercase()),
                    UiNodeKind::Label,
                )
                .with_class("electronics-category")
                .with_text_value(category.clone())
                .with_text_style(body_style(palette.tokens().text_muted))
                .with_layout(UiLayout::fixed(0.0, 22.0).with_width_mode(UiSizeMode::Fill)),
            );
        }
        list = list.with_child(library_card(palette, entry));
    }
    if visible_count == 0 {
        let message = if query.is_empty() {
            "No components available"
        } else {
            "No components match this search"
        };
        list = list.with_child(
            UiNode::new("electronics.library.empty", UiNodeKind::Label)
                .with_text_value(message.to_string())
                .with_text_style(body_style(palette.tokens().text_muted))
                .with_layout(
                    UiLayout::fixed(0.0, 34.0)
                        .with_width_mode(UiSizeMode::Fill)
                        .with_text_safe_area(true),
                ),
        );
    }
    list
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
        .with_layout(UiLayout::fixed(34.0, 34.0))
    } else {
        UiNode::new(
            format!("electronics.library.card.{}.icon", entry.index),
            UiNodeKind::Label,
        )
        .with_icon(UiIcon::new(entry.icon).with_size(UiIconSize::Panel))
        .with_layout(UiLayout::fixed(34.0, 34.0))
    };
    UiNode::new(
        format!("electronics.library.card.{}", entry.index),
        UiNodeKind::Button,
    )
    .with_class("electronics-library-card")
    // Keep a readable payload on the interactive card itself. This is a
    // fallback for narrow retained layouts where a nested fit-content text
    // column can collapse; the card remains fully native RafUI.
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 8.0,
        padding: UiSpacing::xy(9.0, 7.0),
        ..UiLayout::fixed(0.0, 58.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(icon)
    .with_child(
        UiNode::new(
            format!("electronics.library.card.{}.text", entry.index),
            UiNodeKind::Panel,
        )
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 1.0,
            grow: 1.0,
            ..UiLayout::fixed(0.0, 40.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(
                format!("electronics.library.card.{}.title", entry.index),
                UiNodeKind::Label,
            )
            .with_text_value(title)
            .with_text_style(UiTextStyle {
                role: UiTextRole::Button,
                size_px: 11.0,
                line_height_px: 16.0,
                weight: UiFontWeight::Bold,
                color: tokens.text,
                inherit_color: false,
            })
            .with_layout(UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new(
                format!("electronics.library.card.{}.description", entry.index),
                UiNodeKind::Label,
            )
            .with_text_value(entry.description.clone())
            .with_text_style(body_style(tokens.text_muted))
            .with_layout(UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::Fill)),
        ),
    )
    .focusable()
    .with_tooltip_value(entry.description.clone())
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
) -> UiNode {
    let tokens = palette.tokens();
    let mut list = UiNode::scroll_view("electronics.document.list", UiScrollAxis::Vertical)
        .with_class("electronics-list")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            padding: UiSpacing::xy(1.0, 2.0),
            grow: 1.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });
    if active_tab == "project" {
        list = list
            .with_child(section_label("SCHEMATIC", tokens.text_muted))
            .with_child(document_row(
                palette,
                &ElectronicsNavigatorEntry {
                    label: schematic_name.to_string(),
                    secondary: "Active document".to_string(),
                    command: "electronics.navigator.tab:components".to_string(),
                    icon: UiIconId::Schematic,
                    active: true,
                },
                0,
            ));
    }
    for (index, entry) in entries.iter().enumerate() {
        let row_index = if active_tab == "project" {
            index + 1
        } else {
            index
        };
        list = list.with_child(document_row(palette, entry, row_index));
    }
    if entries.is_empty() && active_tab != "project" {
        list = list.with_child(
            UiNode::new("electronics.document.empty", UiNodeKind::Label)
                .with_text_value("No items in this view".to_string())
                .with_text_style(body_style(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)),
        );
    }
    list
}

fn document_row(
    palette: StudioUiPalette,
    entry: &ElectronicsNavigatorEntry,
    index: usize,
) -> UiNode {
    let tokens = palette.tokens();
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
        padding: UiSpacing::xy(8.0, 5.0),
        ..UiLayout::fixed(0.0, 38.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(
            format!("electronics.document.row.{index}.icon"),
            UiNodeKind::Label,
        )
        .with_icon(UiIcon::new(entry.icon).with_size(UiIconSize::Small)),
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
            basis: [0.0, 30.0],
            ..UiLayout::default()
        })
        .with_child(
            UiNode::new(
                format!("electronics.document.row.{index}.label"),
                UiNodeKind::Label,
            )
            .with_text_value(entry.label.clone())
            .with_text_style(body_style(tokens.text))
            .with_layout(UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new(
                format!("electronics.document.row.{index}.secondary"),
                UiNodeKind::Label,
            )
            .with_text_value(entry.secondary.clone())
            .with_text_style(body_style(tokens.text_muted))
            .with_layout(UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill)),
        ),
    )
    .focusable()
    .with_tooltip_value(format!("{} - {}", entry.label, entry.secondary))
    .with_accessibility_label_key(entry.label.clone())
    .with_event(UiEventBinding::command(
        UiEventKind::Click,
        entry.command.clone(),
    ))
}

fn section_label(label: &str, color: [u8; 4]) -> UiNode {
    UiNode::new(format!("electronics.section.{label}"), UiNodeKind::Label)
        .with_text_value(label.to_string())
        .with_text_style(body_style(color))
        .with_layout(UiLayout::fixed(0.0, 22.0).with_width_mode(UiSizeMode::Fill))
}

fn icon_button(id: &str, icon: UiIconId, command: &str, tooltip: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("electronics-icon-button")
        .with_layout(UiLayout::fixed(26.0, 26.0))
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_tooltip_value(tooltip.to_string())
        .with_accessibility_label_key(tooltip)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn body_style(color: [u8; 4]) -> UiTextStyle {
    UiTextStyle {
        role: UiTextRole::Body,
        size_px: 11.0,
        line_height_px: 15.0,
        weight: UiFontWeight::Regular,
        color,
        inherit_color: false,
    }
}

fn style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    let active = [116, 67, 24, 86];
    let mut rules = vec![
        class_rule(
            "electronics-navigator",
            tokens.background,
            tokens.border,
            tokens.text,
        ),
        class_rule(
            "electronics-nav-header",
            tokens.surface_raised,
            tokens.border,
            tokens.text,
        ),
        class_rule(
            "electronics-nav-tabs",
            tokens.surface,
            tokens.border,
            tokens.text,
        ),
        class_rule(
            "electronics-nav-tab",
            tokens.surface,
            tokens.border,
            tokens.text_muted,
        ),
        class_rule(
            "electronics-nav-tab-active",
            tokens.surface_raised,
            tokens.accent,
            tokens.text,
        ),
        class_rule(
            "electronics-summary",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text,
        ),
        class_rule(
            "electronics-summary-card",
            tokens.surface,
            tokens.border,
            tokens.text,
        ),
        class_rule(
            "electronics-search",
            tokens.surface_alt,
            tokens.border,
            tokens.text,
        ),
        class_rule("electronics-list", [0, 0, 0, 0], [0, 0, 0, 0], tokens.text),
        class_rule(
            "electronics-category",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text_muted,
        ),
        class_rule(
            "electronics-library-card",
            tokens.surface,
            tokens.border,
            tokens.text,
        ),
        class_rule(
            "electronics-document-row",
            tokens.surface,
            tokens.border,
            tokens.text,
        ),
        class_rule(
            "electronics-document-row-active",
            active,
            tokens.accent,
            tokens.text,
        ),
        class_rule(
            "electronics-icon-button",
            tokens.surface_alt,
            tokens.border,
            tokens.text_muted,
        ),
    ];
    rules.extend([
        hover_rule(
            "electronics-nav-tab",
            tokens.surface_raised,
            tokens.focus,
            tokens.text,
        ),
        hover_rule(
            "electronics-library-card",
            tokens.surface_raised,
            tokens.focus,
            tokens.text,
        ),
        hover_rule(
            "electronics-document-row",
            tokens.surface_raised,
            tokens.focus,
            tokens.text,
        ),
        hover_rule(
            "electronics-icon-button",
            tokens.surface_raised,
            tokens.focus,
            tokens.text,
        ),
    ]);
    UiStyleSheet { rules }
}

fn class_rule(class: &str, fill: [u8; 4], border: [u8; 4], text: [u8; 4]) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            border: Some(border),
            text: Some(text),
            border_width: Some(1.0),
            radius: Some(4.0),
            ..UiStylePatch::default()
        },
    )
    .when(UiStyleRuleState::Always)
}

fn hover_rule(class: &str, fill: [u8; 4], border: [u8; 4], text: [u8; 4]) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            border: Some(border),
            text: Some(text),
            ..UiStylePatch::default()
        },
    )
    .when(UiStyleRuleState::Hovered)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_node<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if node.id == id {
            return Some(node);
        }
        node.children.iter().find_map(|child| find_node(child, id))
    }

    #[test]
    fn library_card_keeps_icon_and_text_in_the_retained_tree() {
        let library = [ElectronicsLibraryEntry {
            index: 0,
            name: "Resistor".to_string(),
            category: "Passive".to_string(),
            description: "Standard resistor".to_string(),
            favorite: false,
            icon: UiIconId::Scale,
            image_key: Some("electronics://library/resistor.png".to_string()),
        }];
        let surface = build_electronics_navigator_surface(
            StudioUiPalette::IndustrialDark,
            "library",
            "",
            "Main Schematic",
            (0, 0, 0),
            &[],
            &[],
            &library,
        );

        let card = find_node(&surface.root, "electronics.library.card.0").expect("library card");
        assert_eq!(card.children.len(), 2);
        assert!(find_node(&surface.root, "electronics.library.card.0.image").is_some());
        let title = find_node(&surface.root, "electronics.library.card.0.title")
            .expect("library card title");
        assert_eq!(title.text_value.as_deref(), Some("Resistor"));
        let description = find_node(&surface.root, "electronics.library.card.0.description")
            .expect("library card description");
        assert_eq!(description.text_value.as_deref(), Some("Standard resistor"));
    }
}
