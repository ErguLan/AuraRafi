//! RafUI navigator and component library for the native Electronics workspace.
//!
//! This surface deliberately contains presentation only. Selecting a row or
//! a library card emits a semantic command; the native Electronics controller
//! owns the document mutation and placement.

use raf_electronics::CadSurfaceKind;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiIcon, UiIconId, UiIconSize, UiSurface,
};
use raf_ui::{
    UiAccessibilityRole, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiFontWeight, UiImage,
    UiImageFit, UiImageSource, UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiScrollAxis,
    UiSizeMode, UiSpacing, UiStylePatch, UiStyleRuleState, UiStyleSheet, UiTextInput,
    UiTextOverflow, UiTextRole, UiTextStyle,
};

use crate::electronics_controller::PcbLayerVisibility;
use crate::panels::electronics_surface::{
    electronics_active_rule, electronics_body_style, electronics_class_rule,
    electronics_disabled_rule, electronics_flat_class_rule, electronics_focus_rule,
    electronics_hover_rule, electronics_state_rule, with_alpha, ElectronicsIconButton,
    ELECTRONICS_BODY_LINE_HEIGHT, ELECTRONICS_CONTROL_HEIGHT, ELECTRONICS_ITEM_HEIGHT,
    ELECTRONICS_ROW_GLYPH_TRACK, ELECTRONICS_ROW_HEIGHT, PANEL_PADDING_X, PANEL_PADDING_Y,
};

/// Height of one document row, in logical points.
const DOCUMENT_ROW_HEIGHT: f32 = ELECTRONICS_ITEM_HEIGHT;
/// Vertical padding of a document row, in logical points.
const DOCUMENT_ROW_PADDING_Y: f32 =
    (DOCUMENT_ROW_HEIGHT - ELECTRONICS_BODY_LINE_HEIGHT * 2.0) * 0.5;
/// Height of one library card, in logical points.
const LIBRARY_CARD_HEIGHT: f32 = 58.0;
/// Size of the header icon button, in logical points.
const ICON_BUTTON_SIZE: f32 = ELECTRONICS_CONTROL_HEIGHT;

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
    surface: CadSurfaceKind,
    layer_visibility: PcbLayerVisibility,
) -> UiSurface {
    build_electronics_navigator_surface_with_state(
        palette,
        active_tab,
        query,
        schematic_name,
        counts,
        components,
        wires,
        library,
        surface,
        layer_visibility,
        NavigatorDesktopState::default(),
    )
}

#[derive(Default, Clone, Copy)]
pub struct NavigatorDesktopState<'a> {
    pub collapsed: Option<&'a std::collections::HashSet<String>>,
    pub footprints_open: bool,
    pub footprints: Option<&'a crate::panels::electronics_footprint_surface::FootprintChooserModel>,
}
impl NavigatorDesktopState<'_> {
    fn closed(self, key: &str) -> bool {
        self.collapsed
            .is_some_and(|set| set.contains(&key.to_ascii_lowercase()))
    }
}

pub fn build_electronics_navigator_surface_with_state(
    palette: StudioUiPalette,
    active_tab: &str,
    query: &str,
    schematic_name: &str,
    counts: (usize, usize, usize),
    components: &[ElectronicsNavigatorEntry],
    wires: &[ElectronicsNavigatorEntry],
    library: &[ElectronicsLibraryEntry],
    surface: CadSurfaceKind,
    layer_visibility: PcbLayerVisibility,
    desktop: NavigatorDesktopState<'_>,
) -> UiSurface {
    let active_tab = normalize_tab(active_tab, surface);
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
        .with_child(project_documents(palette, schematic_name, surface, desktop))
        .with_child(
            UiNode::new("electronics.navigator.library-heading", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 8.0,
                    padding: UiSpacing::xy(8.0, 0.0),
                    ..UiLayout::fixed(0.0, 34.0).fill_width()
                })
                .with_style(raf_ui::UiStyle::transparent().with_fill(
                    crate::panels::electronics_surface::electronics_tokens(palette).surface_raised,
                ))
                .with_child(
                    UiNode::new(
                        "electronics.navigator.library-heading.icon",
                        UiNodeKind::Label,
                    )
                    .with_icon(UiIcon::new(UiIconId::Assets))
                    .with_layout(UiLayout::fixed(18.0, 20.0)),
                )
                .with_child(
                    UiNode::new("electronics.navigator.library-title", UiNodeKind::Label)
                        .with_text_key("electronics.desktop.library")
                        .with_text_style(UiTextStyle::panel_title(
                            crate::panels::electronics_surface::electronics_tokens(palette).text,
                        ))
                        .with_layout(UiLayout::fixed(0.0, 22.0).fill_width().grow(1.0)),
                ),
        )
        .with_child(desktop_library_tabs(palette, desktop));
    let _ = counts;

    if let Some(model) = desktop.footprints.filter(|_| desktop.footprints_open) {
        root = root.with_child(
            crate::panels::electronics_footprint_surface::footprint_library_content(palette, model),
        );
        let mut surface = UiSurface::new("electronics.navigator", palette, root);
        surface.style_sheet = style_sheet(palette);
        surface.style_sheet.rules.extend(
            crate::panels::electronics_footprint_surface::footprint_chooser_style_sheet(palette)
                .rules,
        );
        return surface;
    }

    // The search field filters every book list in the navigator, not only the
    // catalog: components and wires are the two long lists.
    if matches!(active_tab, "library" | "components" | "wires" | "objects") {
        root = root.with_child(search(query));
    }

    match active_tab {
        "library" => {
            root = root.with_child(library_list(palette, library, query, desktop));
        }
        "layers" => {
            root = root.with_child(layers_list(palette, layer_visibility));
        }
        // The board inventory presents placements and copper runs as one list
        // so the rail keeps one strong finding point for every PCB object.
        "objects" => {
            root = root.with_child(document_objects_list(palette, components, wires, query));
        }
        "project" => {
            root = root.with_child(document_list(palette, components, schematic_name));
        }
        "components" => {
            root = root.with_child(entries_list(
                palette,
                components,
                query,
                DocumentRowKind::Component,
            ));
        }
        _ => {
            root = root.with_child(entries_list(palette, wires, query, DocumentRowKind::Wire));
        }
    }

    if active_tab == "library" {
        let mut recent = UiNode::new("electronics.library.favorites", UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 5.0,
                ..UiLayout::fit_content().fill_width()
            });
        for entry in library.iter().filter(|e| e.favorite).take(2) {
            recent = recent.with_child(library_card(palette, entry));
        }
        if !recent.children.is_empty() {
            root = root.with_child(recent);
        }
    }
    let mut surface = UiSurface::new("electronics.navigator", palette, root);
    surface.style_sheet = style_sheet(palette);
    surface
}

fn project_documents(
    palette: StudioUiPalette,
    name: &str,
    active: CadSurfaceKind,
    desktop: NavigatorDesktopState<'_>,
) -> UiNode {
    let t = crate::panels::electronics_surface::electronics_tokens(palette);
    let mut tree =
        UiNode::new("electronics.navigator.documents", UiNodeKind::Panel).with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 1.0,
            padding: UiSpacing::xy(6.0, 4.0),
            ..UiLayout::fit_content().fill_width()
        });
    tree = tree.with_child(
        UiNode::new("electronics.navigator.project-name", UiNodeKind::Label)
            .with_text_value(name)
            .with_icon(UiIcon::new(UiIconId::Project))
            .with_text_style(UiTextStyle::body(t.text))
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_layout(UiLayout::fixed(0.0, 30.0).fill_width()),
    );
    for (id, key, icon, cmd, selected) in [
        (
            "schematic",
            "app.electronics_schematic_tab",
            UiIconId::Schematic,
            "electronics.mode.schematic",
            active == CadSurfaceKind::Schematic,
        ),
        (
            "pcb",
            "app.electronics_pcb_tab",
            UiIconId::Pcb,
            "electronics.mode.pcb",
            active == CadSurfaceKind::Pcb,
        ),
    ] {
        let closed = desktop.closed(&format!("@{id}"));
        tree = tree.with_child(
            UiNode::new(
                format!("electronics.navigator.folder.{id}"),
                UiNodeKind::Button,
            )
            .with_class("electronics-document-row")
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 6.0,
                padding: UiSpacing::xy(10.0, 0.0),
                ..UiLayout::fixed(0.0, 28.0).fill_width()
            })
            .with_child(
                UiNode::new(format!("{id}.chevron"), UiNodeKind::Label)
                    .with_icon(UiIcon::new(if closed {
                        UiIconId::ChevronRight
                    } else {
                        UiIconId::ChevronDown
                    }))
                    .with_layout(UiLayout::fixed(12.0, 18.0)),
            )
            .with_child(
                UiNode::new(format!("{id}.icon"), UiNodeKind::Label)
                    .with_icon(UiIcon::new(icon).with_tint(t.accent))
                    .with_layout(UiLayout::fixed(18.0, 20.0)),
            )
            .with_child(
                UiNode::new(format!("{id}.title"), UiNodeKind::Label)
                    .with_text_key(key)
                    .with_text_style(UiTextStyle::body(t.text))
                    .with_layout(UiLayout::fixed(0.0, 22.0).fill_width().grow(1.0)),
            )
            .with_accessibility_role(UiAccessibilityRole::Button)
            .with_accessibility_label_key(key)
            .with_accessibility_expanded(!closed)
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("electronics.library.category.toggle:@{id}"),
            )),
        );
        if !closed {
            tree = tree.with_child(
                UiNode::new(
                    format!("electronics.navigator.document.{id}"),
                    UiNodeKind::Button,
                )
                .with_class("electronics-document-row")
                .with_class(if selected {
                    "electronics-document-row-active"
                } else {
                    ""
                })
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 6.0,
                    padding: UiSpacing::xy(38.0, 0.0),
                    ..UiLayout::fixed(0.0, 28.0).fill_width()
                })
                .with_icon(UiIcon::new(icon))
                .with_text_value(format!(
                    "{name} · {}",
                    if id == "pcb" { "PCB" } else { "SCH" }
                ))
                .with_text_style(electronics_body_style(t.text))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_accessibility_role(UiAccessibilityRole::Button)
                .with_accessibility_label_key(key)
                .with_accessibility_selected(selected)
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, cmd)),
            );
        }
    }
    tree = tree.with_child(
        UiNode::new(
            "electronics.navigator.project.inventory",
            UiNodeKind::Button,
        )
        .with_class("electronics-document-row")
        .with_layout(UiLayout {
            padding: UiSpacing::xy(26.0, 0.0),
            ..UiLayout::fixed(0.0, 28.0).fill_width()
        })
        .with_icon(UiIcon::new(UiIconId::Entity))
        .with_text_key("electronics.rail.objects")
        .with_text_style(electronics_body_style(t.text_muted))
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_label_key("electronics.rail.objects")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            if active == CadSurfaceKind::Pcb {
                "electronics.navigator.tab:objects"
            } else {
                "electronics.navigator.tab:components"
            },
        )),
    );
    if active == CadSurfaceKind::Schematic {
        tree = tree.with_child(
            UiNode::new("electronics.navigator.project.wires", UiNodeKind::Button)
                .with_class("electronics-document-row")
                .with_layout(UiLayout {
                    padding: UiSpacing::xy(26.0, 0.0),
                    ..UiLayout::fixed(0.0, 28.0).fill_width()
                })
                .with_icon(UiIcon::new(UiIconId::Wire))
                .with_text_key("app.schematic_wires")
                .with_text_style(electronics_body_style(t.text_muted))
                .with_accessibility_role(UiAccessibilityRole::Button)
                .with_accessibility_label_key("app.schematic_wires")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "electronics.navigator.tab:wires",
                )),
        );
    }
    tree
}

fn desktop_library_tabs(palette: StudioUiPalette, desktop: NavigatorDesktopState<'_>) -> UiNode {
    let t = crate::panels::electronics_surface::electronics_tokens(palette);
    let mut row =
        UiNode::new("electronics.navigator.tabs", UiNodeKind::Toolbar).with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 1.0,
            ..UiLayout::fixed(0.0, 34.0).fill_width()
        });
    for (id, key, command, active) in [
        (
            "library",
            "app.schematic_components",
            "electronics.navigator.tab:library",
            !desktop.footprints_open,
        ),
        (
            "footprints",
            "electronics.desktop.footprints",
            "electronics.footprint.open",
            desktop.footprints_open,
        ),
    ] {
        row = row.with_child(
            UiNode::new(
                format!("electronics.navigator.tab.{id}"),
                UiNodeKind::Button,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                align_items: UiAlign::Stretch,
                justify_content: UiJustify::End,
                ..UiLayout::fixed(0.0, 34.0).fill_width().grow(1.0)
            })
            .with_style(raf_ui::UiStyle::transparent().with_fill(if active {
                t.surface_raised
            } else {
                t.surface
            }))
            .with_text_key(key)
            .with_text_style(UiTextStyle {
                size_px: 13.0,
                line_height_px: 18.0,
                ..UiTextStyle::body(t.text)
            })
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_child(
                UiNode::new(format!("{id}.edge"), UiNodeKind::Separator)
                    .with_layout(UiLayout::fixed(0.0, 2.0).fill_width())
                    .with_style(raf_ui::UiStyle::transparent().with_fill(if active {
                        t.accent
                    } else {
                        [0, 0, 0, 0]
                    })),
            )
            .with_accessibility_role(UiAccessibilityRole::Tab)
            .with_accessibility_label_key(key)
            .with_accessibility_selected(active)
            .with_tooltip_key(key)
            .focusable()
            .with_event(UiEventBinding::command(UiEventKind::Click, command)),
        );
    }
    row
}

/// Resolves a stored tab id against the active surface.
///
/// A session may persist the tab of the other surface ("wires" open when the
/// user pressed PCB), so the rail self-heals to the document inventory of the
/// surface instead of rendering a tab that does not exist.
fn normalize_tab(active_tab: &str, surface: CadSurfaceKind) -> &'static str {
    match surface {
        CadSurfaceKind::Schematic => match active_tab {
            "project" | "components" | "wires" | "library" => mapper(active_tab),
            _ => "components",
        },
        CadSurfaceKind::Pcb => match active_tab {
            "project" | "objects" | "layers" | "library" => mapper(active_tab),
            _ => "objects",
        },
    }
}

/// Maps a shelved tab id to the static label the same id uses.
fn mapper(tab: &str) -> &'static str {
    match tab {
        "project" => "project",
        "components" => "components",
        "wires" => "wires",
        "objects" => "objects",
        "layers" => "layers",
        _ => "library",
    }
}

fn header(palette: StudioUiPalette, _schematic_name: &str) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    UiNode::new("electronics.navigator.header", UiNodeKind::Toolbar)
        .with_class("electronics-nav-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(8.0, 6.0),
            ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("electronics.navigator.header.icon", UiNodeKind::Label)
                .with_icon(UiIcon::new(UiIconId::Schematic).with_size(UiIconSize::Small))
                // A row child with no authored track resolves to zero width and
                // its glyph is never painted, so the leading icon owns one.
                .with_layout(UiLayout::fixed(ELECTRONICS_ROW_GLYPH_TRACK, 34.0 - 12.0)),
        )
        .with_child(
            UiNode::new("electronics.navigator.header.title", UiNodeKind::Label)
                .with_text_key("electronics.desktop.project")
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::PanelTitle,
                    size_px: 13.0,
                    line_height_px: 17.0,
                    weight: UiFontWeight::Bold,
                    color: tokens.text,
                    inherit_color: false,
                })
                .with_layout(UiLayout::fixed(0.0, 22.0).fill_width().grow(1.0)),
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
        .with_child(
            ElectronicsIconButton::new(
                "electronics.navigator.close",
                UiIconId::ChevronLeft,
                "electronics.workspace.library.toggle",
                "electronics.workspace.library_toggle",
                "electronics-icon-button",
                ICON_BUTTON_SIZE,
                ICON_BUTTON_SIZE,
            )
            .build(),
        )
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
    desktop: NavigatorDesktopState<'_>,
) -> UiNode {
    let t = crate::panels::electronics_surface::electronics_tokens(palette);
    let query = query.trim().to_lowercase();
    let mut list = list_node("electronics.library.list");
    list.layout.gap = 0.0;
    list.layout.padding = UiSpacing::xy(0.0, 4.0);
    let mut category = String::new();
    let mut group = 0;
    let mut visible = 0;
    for entry in entries.iter().filter(|e| {
        query.is_empty()
            || e.name.to_lowercase().contains(&query)
            || e.category.to_lowercase().contains(&query)
            || e.description.to_lowercase().contains(&query)
    }) {
        let closed = desktop.closed(&entry.category) && query.is_empty();
        if category != entry.category {
            category = entry.category.clone();
            list = list.with_child(
                UiNode::new(
                    format!("electronics.library.category.{group}"),
                    UiNodeKind::Button,
                )
                .with_class("electronics-document-row")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 6.0,
                    padding: UiSpacing::xy(2.0, 0.0),
                    ..UiLayout::fixed(0.0, 28.0).fill_width()
                })
                .with_child(
                    UiNode::new(format!("category.{group}.chevron"), UiNodeKind::Label)
                        .with_icon(UiIcon::new(if closed {
                            UiIconId::ChevronRight
                        } else {
                            UiIconId::ChevronDown
                        }))
                        .with_layout(UiLayout::fixed(12.0, 18.0)),
                )
                .with_child(
                    UiNode::new(format!("category.{group}.folder"), UiNodeKind::Label)
                        .with_icon(UiIcon::new(UiIconId::Folder).with_tint(t.accent))
                        .with_layout(UiLayout::fixed(18.0, 20.0)),
                )
                .with_child(
                    UiNode::new(format!("category.{group}.title"), UiNodeKind::Label)
                        .with_text_key(library_category_key(&category))
                        .with_text_style(electronics_body_style(t.text))
                        .with_layout(UiLayout::fixed(0.0, 22.0).fill_width().grow(1.0)),
                )
                .with_accessibility_role(UiAccessibilityRole::Button)
                .with_accessibility_label_key(library_category_key(&category))
                .with_accessibility_expanded(!closed)
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!(
                        "electronics.library.category.toggle:{}",
                        category.to_ascii_lowercase()
                    ),
                )),
            );
            group += 1;
        }
        if closed {
            continue;
        }
        visible += 1;
        list = list.with_child(
            UiNode::new(
                format!("electronics.library.card.{}", entry.index),
                UiNodeKind::Button,
            )
            .with_class("electronics-library-card")
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 8.0,
                padding: UiSpacing::xy(29.0, 0.0),
                ..UiLayout::fixed(0.0, 28.0).fill_width()
            })
            .with_child(if let Some(key) = entry.image_key.as_deref() {
                UiNode::image(
                    format!("electronics.library.card.{}.icon", entry.index),
                    UiImage {
                        options: Default::default(),
                        source: UiImageSource::new(key),
                        fit: UiImageFit::Contain,
                        tint: Some(t.accent),
                    },
                )
                .with_layout(UiLayout::fixed(24.0, 24.0))
            } else {
                UiNode::new(
                    format!("electronics.library.card.{}.icon", entry.index),
                    UiNodeKind::Label,
                )
                .with_icon(
                    UiIcon::new(entry.icon)
                        .with_size(UiIconSize::Custom(24))
                        .with_tint(t.accent),
                )
                .with_layout(UiLayout::fixed(24.0, 24.0))
            })
            .with_child(
                UiNode::new(
                    format!("electronics.library.card.{}.title", entry.index),
                    UiNodeKind::Label,
                )
                .with_text_value(&entry.name)
                .with_text_style(electronics_body_style(t.text))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fixed(0.0, 22.0).fill_width().grow(1.0)),
            )
            .with_accessibility_role(UiAccessibilityRole::Button)
            .with_accessibility_label_key("app.electronics_place_component")
            .with_tooltip_value(&entry.description)
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("electronics.place.template.{}", entry.index),
            ))
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
            )),
        );
    }
    if visible == 0 && !query.is_empty() {
        list = list.with_child(empty_row(
            palette,
            "electronics.library.empty",
            "app.electronics_no_components_match",
        ));
    }
    list
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
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let title = entry.name.clone();
    let icon = if let Some(image_key) = entry.image_key.as_deref() {
        UiNode::image(
            format!("electronics.library.favorite.{}.image", entry.index),
            UiImage {
                options: Default::default(),
                source: UiImageSource::new(image_key),
                fit: UiImageFit::Contain,
                tint: None,
            },
        )
        .with_layout(UiLayout::fixed(36.0, 36.0))
    } else {
        UiNode::new(
            format!("electronics.library.favorite.{}.icon", entry.index),
            UiNodeKind::Label,
        )
        .with_icon(UiIcon::new(entry.icon).with_size(UiIconSize::Custom(28)))
        .with_layout(UiLayout::fixed(36.0, 36.0))
    };
    UiNode::new(
        format!("electronics.library.favorite.{}", entry.index),
        UiNodeKind::Button,
    )
    .with_class("electronics-library-card")
    .with_class("electronics-library-preview")
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
            format!("electronics.library.favorite.{}.text", entry.index),
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
                format!("electronics.library.favorite.{}.title", entry.index),
                UiNodeKind::Label,
            )
            .with_text_value(title)
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_text_style(UiTextStyle {
                role: UiTextRole::Button,
                size_px: 13.0,
                line_height_px: 18.0,
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
                format!("electronics.library.favorite.{}.description", entry.index),
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
    schematic_name: &str,
) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let mut list = list_node("electronics.document.list");
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
            DocumentRowKind::Project,
        ));
    // The project tab shows the document list, not a searchable one, so a
    // query left over from another tab must not filter it silently.
    list = list.with_child(section_label("electronics.rail.documents", tokens.accent));
    for (index, entry) in entries.iter().enumerate() {
        list = list.with_child(document_row(
            palette,
            entry,
            index + 1,
            DocumentRowKind::Component,
        ));
    }
    list
}

/// PCB document inventory: placements and copper runs as one list.
///
/// The board tracks are navigable targets like the components, so a section
/// header separates the two instead of giving them separate tabs.
fn document_objects_list(
    palette: StudioUiPalette,
    components: &[ElectronicsNavigatorEntry],
    traces: &[ElectronicsNavigatorEntry],
    query: &str,
) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let query = query.trim().to_lowercase();
    let matches =
        |entry: &ElectronicsNavigatorEntry| query.is_empty() || entry_matches(entry, &query);
    let mut list = list_node("electronics.document.list");
    let mut visible = 0usize;
    let mut stream = 0usize;
    if components.iter().any(|entry| matches(entry)) {
        list = list.with_child(section_label("electronics.rail.components", tokens.accent));
        for entry in components.iter().filter(|entry| matches(entry)) {
            visible += 1;
            list = list.with_child(document_row(
                palette,
                entry,
                stream,
                DocumentRowKind::Component,
            ));
            stream += 1;
        }
    }
    if traces.iter().any(|entry| matches(entry)) {
        list = list.with_child(section_label("electronics.rail.traces", tokens.accent));
        for entry in traces.iter().filter(|entry| matches(entry)) {
            visible += 1;
            list = list.with_child(document_row(
                palette,
                entry,
                format!("t.{stream}"),
                DocumentRowKind::Trace,
            ));
            stream += 1;
        }
    }
    if visible == 0 {
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

/// One flat, searchable document list: the schematic Components and Wires tabs.
fn entries_list(
    palette: StudioUiPalette,
    entries: &[ElectronicsNavigatorEntry],
    query: &str,
    kind: DocumentRowKind,
) -> UiNode {
    let query = query.trim().to_lowercase();
    let mut list = list_node("electronics.document.list");
    let mut visible = 0usize;
    for (index, entry) in entries.iter().enumerate() {
        if !query.is_empty() && !entry_matches(entry, &query) {
            continue;
        }
        visible += 1;
        list = list.with_child(document_row(palette, entry, index, kind));
    }
    if visible == 0 {
        let message = if query.is_empty() {
            "app.electronics_no_items"
        } else {
            "app.electronics_no_items_match"
        };
        list = list.with_child(empty_row(palette, "electronics.document.empty", message));
    }
    list
}

/// The PCB visibility sheet of the Layers tab.
///
/// One row per presentation filter: the toggle is the whole row, the state is
/// its own accessibility flag instead of a live color patch, and the trailing
/// count tells the user what is actually being pruned right now.
fn layers_list(palette: StudioUiPalette, visibility: PcbLayerVisibility) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let rows: [(&str, &'static str, UiIconId, bool); 5] = [
        (
            "components",
            "electronics.rail.layer.components",
            UiIconId::Entity,
            visibility.components,
        ),
        (
            "top",
            "electronics.rail.layer.top",
            UiIconId::Pcb,
            visibility.top_traces,
        ),
        (
            "bottom",
            "electronics.rail.layer.bottom",
            UiIconId::Pcb,
            visibility.bottom_traces,
        ),
        (
            "silk",
            "electronics.rail.layer.silk",
            UiIconId::Frame,
            visibility.silkscreen,
        ),
        (
            "airwires",
            "electronics.rail.layer.airwires",
            UiIconId::Wire,
            visibility.airwires,
        ),
    ];
    let mut list = list_node("electronics.layers.list");
    for (key, label_key, icon, visible) in rows {
        let state_key = if visible {
            "electronics.rail.layer.on"
        } else {
            "electronics.rail.layer.off"
        };
        list = list.with_child(
            UiNode::new(format!("electronics.layers.row.{key}"), UiNodeKind::Button)
                .with_class("electronics-layers-row")
                .with_class(if visible {
                    "electronics-layers-row-on"
                } else {
                    ""
                })
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 7.0,
                    padding: UiSpacing::xy(8.0, 3.0),
                    ..UiLayout::fixed(0.0, ELECTRONICS_ITEM_HEIGHT)
                        .with_width_mode(UiSizeMode::Fill)
                })
                .with_accessibility_role(UiAccessibilityRole::Checkbox)
                .with_accessibility_label_key(label_key)
                .with_accessibility_checked(visible)
                .with_tooltip_key(label_key)
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("electronics.layer.toggle.{key}"),
                ))
                .with_child(
                    UiNode::new(
                        format!("electronics.layers.row.{key}.icon"),
                        UiNodeKind::Label,
                    )
                    .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small).with_tint(
                        if visible {
                            tokens.text
                        } else {
                            tokens.text_muted
                        },
                    ))
                    .with_layout(UiLayout::fixed(
                        ELECTRONICS_ROW_GLYPH_TRACK,
                        ELECTRONICS_ITEM_HEIGHT - 6.0,
                    )),
                )
                .with_child(
                    UiNode::new(
                        format!("electronics.layers.row.{key}.label"),
                        UiNodeKind::Label,
                    )
                    .with_text_key(label_key)
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_text_style(electronics_body_style(tokens.text))
                    .with_layout(
                        UiLayout::fixed(0.0, ELECTRONICS_BODY_LINE_HEIGHT)
                            .with_width_mode(UiSizeMode::Fill),
                    ),
                )
                .with_child(
                    UiNode::new(
                        format!("electronics.layers.row.{key}.state"),
                        UiNodeKind::Label,
                    )
                    .with_text_key(state_key)
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_text_style(electronics_body_style(tokens.text_muted)),
                ),
        );
    }
    list = list.with_child(
        UiNode::new("electronics.layers.hint", UiNodeKind::Label)
            .with_text_key("electronics.rail.layer.hint")
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_text_style(electronics_body_style(tokens.text_muted))
            .with_layout(
                UiLayout::fixed(0.0, ELECTRONICS_ROW_HEIGHT)
                    .with_width_mode(UiSizeMode::Fill)
                    .with_text_safe_area(true),
            ),
    );
    list
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
        .with_text_style(electronics_body_style(
            crate::panels::electronics_surface::electronics_tokens(palette).text_muted,
        ))
        .with_layout(
            UiLayout::fixed(0.0, ELECTRONICS_ROW_HEIGHT)
                .with_width_mode(UiSizeMode::Fill)
                .with_text_safe_area(true),
        )
}

/// What a navigator row represents, so its accessible name never depends on
/// which tab rendered it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocumentRowKind {
    Project,
    Component,
    Wire,
    Trace,
}

impl DocumentRowKind {
    fn accessibility_key(self) -> &'static str {
        match self {
            Self::Project => "app.electronics_schematic_tab",
            Self::Component => "app.electronics_component",
            Self::Wire => "app.electronics_wire",
            Self::Trace => "electronics.rail.trace",
        }
    }
}

fn document_row(
    palette: StudioUiPalette,
    entry: &ElectronicsNavigatorEntry,
    index: impl std::fmt::Display,
    kind: DocumentRowKind,
) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
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
    .with_accessibility_label_key(kind.accessibility_key())
    .with_accessibility_selected(entry.active)
    .focusable()
    .with_tooltip_value(format!("{} - {}", entry.label, entry.secondary))
    .with_event(UiEventBinding::command(
        UiEventKind::Click,
        entry.command.clone(),
    ))
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
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
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
            with_alpha(tokens.border, 72),
            tokens.text,
        ),
        electronics_flat_class_rule("electronics-nav-tabs", [0, 0, 0, 0], tokens.text),
        electronics_flat_class_rule("electronics-nav-tab", [0, 0, 0, 0], tokens.text_muted),
        electronics_flat_class_rule(
            "electronics-nav-tab-active",
            with_alpha(tokens.accent, 22),
            tokens.text,
        ),
        electronics_class_rule(
            "electronics-summary",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text,
        ),
        electronics_flat_class_rule(
            "electronics-summary-card",
            with_alpha(tokens.surface_alt, 112),
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
        electronics_flat_class_rule("electronics-library-card", [0, 0, 0, 0], tokens.text),
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
        electronics_flat_class_rule("electronics-layers-row", [0, 0, 0, 0], tokens.text),
        // A hidden layer dims but keeps a full-color text so its name stays
        // readable while waiting to be re-enabled.
        electronics_flat_class_rule(
            "electronics-layers-row-on",
            with_alpha(tokens.surface_raised, 86),
            tokens.text,
        ),
    ];
    for class in [
        "electronics-nav-tab",
        "electronics-nav-tab-active",
        "electronics-library-card",
        "electronics-document-row",
        "electronics-document-row-active",
        "electronics-icon-button",
        "electronics-layers-row",
    ] {
        rules.push(electronics_hover_rule(class, tokens));
        rules.push(electronics_focus_rule(class, tokens));
        rules.push(electronics_active_rule(class, tokens));
    }
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
    rules.push(electronics_state_rule(
        UiStyleRuleState::Always,
        "electronics-nav-tab-active",
        UiStylePatch {
            text: Some(tokens.accent),
            ..UiStylePatch::default()
        },
    ));
    rules.extend([
        electronics_class_rule(
            "electronics-library-preview",
            tokens.surface_alt,
            tokens.border,
            tokens.text,
        ),
        electronics_hover_rule("electronics-library-preview", tokens),
        electronics_focus_rule("electronics-library-preview", tokens),
    ]);
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
            raf_electronics::CadSurfaceKind::Schematic,
            crate::electronics_controller::PcbLayerVisibility::default(),
        )
    }

    #[test]
    fn footprint_tab_replaces_the_component_catalog_and_keeps_the_shared_query() {
        use crate::panels::electronics_footprint_surface::{
            FootprintChooserCandidate, FootprintChooserModel,
        };
        let model = FootprintChooserModel {
            candidates: vec![FootprintChooserCandidate {
                id: "physical-pad-pattern".into(),
                name: "Physical pattern".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let surface = build_electronics_navigator_surface_with_state(
            palette(),
            "library",
            "",
            "PCB",
            (0, 0, 0),
            &[],
            &[],
            &library(),
            CadSurfaceKind::Pcb,
            PcbLayerVisibility::default(),
            NavigatorDesktopState {
                footprints_open: true,
                footprints: Some(&model),
                ..Default::default()
            },
        );
        assert!(surface.root.find("electronics.library.list").is_none());
        assert!(surface.root.find("electronics.library.favorites").is_none());
        assert!(surface
            .root
            .find("electronics.footprint.candidate.0")
            .is_some());
        let search = surface
            .root
            .find("electronics.footprint.query.input")
            .unwrap()
            .control
            .text_input()
            .unwrap();
        assert_eq!(search.value_key, "electronics.footprint.query");
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
            .find("electronics.library.card.0.icon")
            .is_some());
        let title = surface
            .root
            .find("electronics.library.card.0.title")
            .expect("library card title");
        assert_eq!(title.text_value.as_deref(), Some("Resistor"));
        assert_eq!(card.tooltip_value.as_deref(), Some("Standard resistor"));
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
        let surface = build("library", "", &[], &[]);
        for (id, selected) in [("library", true), ("footprints", false)] {
            let tab = surface
                .root
                .find(&format!("electronics.navigator.tab.{id}"))
                .unwrap();
            assert_eq!(tab.accessibility_role, UiAccessibilityRole::Tab);
            assert_eq!(tab.accessibility_selected, Some(selected));
            assert!(tab.focusable);
        }
    }

    #[test]
    fn navigator_header_actions_keep_library_toggle_and_add_commands_reachable() {
        let surface = build("library", "", &[], &[]);
        for (id, command, label_key) in [
            (
                "electronics.navigator.close",
                "electronics.workspace.library.toggle",
                "electronics.workspace.library_toggle",
            ),
            (
                "electronics.navigator.add",
                "electronics.navigator.tab:library",
                "app.electronics_open_library",
            ),
        ] {
            let button = surface.root.find(id).expect("header action");
            assert_eq!(button.accessibility_role, UiAccessibilityRole::Button);
            assert_eq!(button.accessibility_label_key.as_deref(), Some(label_key));
            assert!(button.event_handlers.iter().any(|event| {
                matches!(&event.action, raf_ui::UiAction::Command { name } if name == command)
            }));
        }
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
            category.children.iter().find_map(|n| n.text_key.as_deref()),
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
            raf_electronics::CadSurfaceKind::Schematic,
            crate::electronics_controller::PcbLayerVisibility::default(),
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
        for tab in ["project", "objects", "layers", "library"] {
            let surface = build_electronics_navigator_surface(
                palette(),
                tab,
                "",
                "Main Schematic",
                (1, 1, 1),
                &[entry(0, true)],
                &[entry(0, false)],
                &library(),
                raf_electronics::CadSurfaceKind::Pcb,
                crate::electronics_controller::PcbLayerVisibility {
                    top_traces: false,
                    ..Default::default()
                },
            );
            assert_electronics_layout_gate(&surface, 224, 720);
        }
    }

    #[test]
    fn library_cpu_fixture_renders_at_the_open_rail_width_and_can_be_captured() {
        use raf_render::api_graphic_basic::ui_surface::CpuUiSurfaceHost;

        // Test-only catalog rows: no user asset or library metadata is implied.
        let library = vec![
            ElectronicsLibraryEntry {
                index: 0,
                name: "Resistor 0805".into(),
                category: "Passive".into(),
                description: "Chip resistor · 2012 metric".into(),
                favorite: true,
                icon: UiIconId::Scale,
                image_key: None,
            },
            ElectronicsLibraryEntry {
                index: 1,
                name: "Capacitor 0805".into(),
                category: "Passives".into(),
                description: "Ceramic capacitor".into(),
                favorite: false,
                icon: UiIconId::Scale,
                image_key: None,
            },
            ElectronicsLibraryEntry {
                index: 2,
                name: "LED".into(),
                category: "Diodes".into(),
                description: "Light-emitting diode".into(),
                favorite: false,
                icon: UiIconId::Warning,
                image_key: None,
            },
            ElectronicsLibraryEntry {
                index: 3,
                name: "Ground".into(),
                category: "Power".into(),
                description: "0 V reference".into(),
                favorite: false,
                icon: UiIconId::Pcb,
                image_key: None,
            },
        ];
        let surface = build_electronics_navigator_surface(
            palette(),
            "library",
            "",
            "codex_electronics_qa",
            (4, 5, 3),
            &[entry(0, true)],
            &[entry(0, false)],
            &library,
            raf_electronics::CadSurfaceKind::Schematic,
            crate::electronics_controller::PcbLayerVisibility::default(),
        );
        assert_electronics_layout_gate(&surface, 260, 760);

        let clear = [12, 15, 21, 255];
        let mut host = CpuUiSurfaceHost::new(surface, clear);
        let frame = host.render([260, 760], |key| {
            raf_core::i18n::t(key, raf_core::Language::Spanish)
        });
        for id in [
            "electronics.library.search",
            "electronics.library.category.0",
            "electronics.library.card.0",
            "electronics.library.card.3",
        ] {
            let node = frame
                .frame
                .layout_boxes
                .iter()
                .find(|node| node.id == id)
                .expect("visible library node");
            assert!(node.rect.width > 0.0 && node.rect.height > 0.0, "{id}");
            assert!(
                node.rect.x >= 0.0 && node.rect.right() <= 260.1,
                "{id}: {:?}",
                node.rect
            );
        }
        assert!(frame
            .pixels
            .chunks_exact(4)
            .any(|pixel| pixel != clear.as_slice()));

        if let Some(dir) = std::env::var_os("RAF_ELECTRONICS_QA_DIR") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).expect("create Electronics QA capture directory");
            image::save_buffer(
                dir.join("electronics-navigator-library.png"),
                frame.pixels,
                frame.size[0],
                frame.size[1],
                image::ColorType::Rgba8,
            )
            .expect("save optional library CPU capture");
        }
    }

    #[test]
    fn pcb_tabs_self_heal_a_stored_schematic_tab_in_the_objects_list() {
        let surface = build_electronics_navigator_surface(
            palette(),
            "wires",
            "",
            "Main Schematic",
            (1, 1, 1),
            &[entry(0, false)],
            &[entry(0, false)],
            &library(),
            raf_electronics::CadSurfaceKind::Pcb,
            crate::electronics_controller::PcbLayerVisibility::default(),
        );
        assert!(surface
            .root
            .find("electronics.navigator.project.inventory")
            .is_some());
        assert!(surface.root.find("electronics.document.list").is_some());
    }

    #[test]
    fn layers_rows_toggle_through_the_controller_command() {
        let surface = build_electronics_navigator_surface(
            palette(),
            "layers",
            "",
            "Main Schematic",
            (0, 1, 0),
            &[],
            &[],
            &library(),
            raf_electronics::CadSurfaceKind::Pcb,
            crate::electronics_controller::PcbLayerVisibility {
                airwires: false,
                ..Default::default()
            },
        );
        let airwires = surface
            .root
            .find("electronics.layers.row.airwires")
            .expect("airwires row");
        assert_eq!(
            airwires.accessibility_role,
            UiAccessibilityRole::Checkbox,
            "a visibility row is a checkbox"
        );
        assert_eq!(airwires.accessibility_checked, Some(false));
        assert!(airwires.event_handlers.iter().any(|event| {
            matches!(&event.action, UiAction::Command { name } if name == "electronics.layer.toggle.airwires")
        }));
        let components = surface
            .root
            .find("electronics.layers.row.components")
            .expect("components layer row");
        assert_eq!(components.accessibility_checked, Some(true));
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
    }

    #[test]
    fn every_navigator_control_shares_the_family_control_height() {
        let surface = build("components", "r", &[entry(0, false)], &[]);
        for id in ["electronics.library.search", "electronics.navigator.add"] {
            let node = surface.root.find(id).expect("control");
            assert_eq!(node.layout.basis[1], ELECTRONICS_CONTROL_HEIGHT, "{id}");
        }
        assert_eq!(DOCUMENT_ROW_HEIGHT, ELECTRONICS_ITEM_HEIGHT);
    }

    #[test]
    fn collapsed_catalog_preserves_group_and_hides_its_parts() {
        let collapsed = std::collections::HashSet::from(["passive".to_string()]);
        let surface = build_electronics_navigator_surface_with_state(
            palette(),
            "library",
            "",
            "Main",
            (0, 0, 0),
            &[],
            &[],
            &library(),
            CadSurfaceKind::Schematic,
            PcbLayerVisibility::default(),
            NavigatorDesktopState {
                collapsed: Some(&collapsed),
                footprints_open: false,
                footprints: None,
            },
        );
        assert!(surface
            .root
            .find("electronics.library.category.0")
            .is_some());
        assert!(surface.root.find("electronics.library.card.0").is_none());
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
                raf_electronics::CadSurfaceKind::Schematic,
                crate::electronics_controller::PcbLayerVisibility::default(),
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
            let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
            assert_eq!(fill[0], tokens.selection[0]);
            assert_eq!(fill[3], tokens.selection[3]);
        }
    }

    #[test]
    fn search_reveals_matching_parts_inside_collapsed_categories() {
        let collapsed = std::collections::HashSet::from(["passive".to_string()]);
        let surface = build_electronics_navigator_surface_with_state(
            palette(),
            "library",
            "resistor",
            "Main",
            (0, 0, 0),
            &[],
            &[],
            &library(),
            CadSurfaceKind::Schematic,
            PcbLayerVisibility::default(),
            NavigatorDesktopState {
                collapsed: Some(&collapsed),
                footprints_open: false,
                footprints: None,
            },
        );
        assert!(
            surface.root.find("electronics.library.card.0").is_some(),
            "Search reveals matching parts inside collapsed groups"
        );
    }
}
