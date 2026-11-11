//! Native Electronics inspector surface.
//!
//! The backend projects the selected object as `(field name, value)` pairs.
//! Field names are backend data, so they are only used as lookups; every label
//! the user reads is an i18n key resolved by this surface, and a renamed
//! backend field can no longer leak raw English into the panel.

use raf_core::session::ProjectSessionRegistry;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiIcon, UiIconId, UiIconSize, UiImage, UiImageFit, UiImageSource, UiSurface,
};
use raf_ui::{
    UiAccessibilityRole, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiFontWeight, UiLayout,
    UiNode, UiNodeKind, UiOverflow, UiSizeMode, UiSpacing, UiStyleSheet, UiTextInput,
    UiTextOverflow, UiTextRole, UiTextStyle,
};

use crate::panels::electronics_surface::{
    electronics_active_rule, electronics_body_style, electronics_class_rule,
    electronics_focus_rule, electronics_hover_rule, electronics_state_rule, with_alpha,
    ELECTRONICS_CONTROL_HEIGHT, ELECTRONICS_ROW_HEIGHT, PANEL_PADDING_X, PANEL_PADDING_Y,
};
use crate::panels::inspector_surface::{inspector_style_sheet, sessions_content, InspectorTab};

/// Height of one property row, in logical points.
const FIELD_ROW_HEIGHT: f32 = ELECTRONICS_ROW_HEIGHT;
/// Height of the editable value field, in logical points.
const VALUE_INPUT_HEIGHT: f32 = ELECTRONICS_CONTROL_HEIGHT;
/// Height of a read-only value line, in logical points.
const VALUE_LABEL_HEIGHT: f32 = ELECTRONICS_ROW_HEIGHT;
/// Height of a section title, in logical points. It matches the body line box so
/// the title never paints its descenders into the first field.
const SECTION_TITLE_HEIGHT: f32 = 30.0;

#[derive(Debug, Clone)]
pub struct ElectronicsInspectorModel {
    pub title: String,
    pub kind: String,
    pub value: String,
    pub category: String,
    pub position: String,
    pub rotation: String,
    pub footprint: String,
    pub pins: Vec<(String, String)>,
    pub locked: bool,
    pub editable_value: bool,
    pub identity_label: String,
    pub reference: Option<String>,
    pub pcb_labels: Option<(bool, bool)>,
    pub trace_details: Vec<(String, String)>,
    pub transform_target: Option<String>,
}

/// Optional Electronics data needed to show physical footprint details.
///
/// The image key must already be registered in the retained surface image
/// store and must come from the selected asset's real geometry. An absent key
/// hides the preview instead of showing a generic or fabricated illustration.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ElectronicsInspectorPresentation {
    pub component_symbol_image_key: Option<String>,
    pub component_name: Option<String>,
    pub board_label_visibility: Option<(bool, bool)>,
    pub footprint_name: Option<String>,
    pub footprint_dimensions_mm: Option<[f32; 2]>,
    pub footprint_pad_count: Option<usize>,
    pub footprint_preview_image_key: Option<String>,
    /// Component pin label to physical pad number. Empty values remain visible
    /// as unmatched data instead of being replaced by a guessed assignment.
    pub pin_pad_mapping: Vec<(String, String)>,
}

pub fn build_electronics_inspector_surface(
    palette: StudioUiPalette,
    title: &str,
    fields: &[(String, String)],
    pins: &[(String, String)],
    sessions: &ProjectSessionRegistry,
    tab: InspectorTab,
) -> UiSurface {
    build_electronics_inspector_surface_with_fabrication_status(
        palette, title, fields, pins, sessions, tab, None,
    )
}

pub fn build_electronics_inspector_surface_with_fabrication_status(
    palette: StudioUiPalette,
    title: &str,
    fields: &[(String, String)],
    pins: &[(String, String)],
    sessions: &ProjectSessionRegistry,
    tab: InspectorTab,
    fabrication_status: Option<&str>,
) -> UiSurface {
    build_electronics_inspector_surface_with_presentation(
        palette,
        title,
        fields,
        pins,
        sessions,
        tab,
        fabrication_status,
        None,
    )
}

/// Builds the inspector with optional, real PCB footprint presentation data.
/// The legacy builder signatures remain wrappers so existing callers need not
/// provide footprint data until the editor has loaded it for the selection.
pub fn build_electronics_inspector_surface_with_presentation(
    palette: StudioUiPalette,
    title: &str,
    fields: &[(String, String)],
    pins: &[(String, String)],
    sessions: &ProjectSessionRegistry,
    tab: InspectorTab,
    fabrication_status: Option<&str>,
    presentation: Option<&ElectronicsInspectorPresentation>,
) -> UiSurface {
    build_electronics_inspector_surface_with_page(
        palette,
        title,
        fields,
        pins,
        sessions,
        if tab == InspectorTab::Sessions {
            "sessions"
        } else {
            "properties"
        },
        fabrication_status,
        presentation,
    )
}

pub fn build_electronics_inspector_surface_with_page(
    palette: StudioUiPalette,
    title: &str,
    fields: &[(String, String)],
    pins: &[(String, String)],
    sessions: &ProjectSessionRegistry,
    page: &str,
    fabrication_status: Option<&str>,
    presentation: Option<&ElectronicsInspectorPresentation>,
) -> UiSurface {
    let model = if fields.is_empty() {
        None
    } else {
        let editable_value = field_value(fields, "Editable")
            .map(|value| value == "true")
            // Without the backend telling us the field is writable, present it read
            // only. A missing `Editable` entry must never render an input that
            // silently discards what the user types.
            .unwrap_or(false);
        Some(ElectronicsInspectorModel {
            title: title.to_string(),
            trace_details: [
                ("Trace width", "electronics.desktop.track_width"),
                ("Trace length", "electronics.desktop.length"),
                ("Trace layer", "electronics.desktop.active_layer"),
                ("Trace vertices", "electronics.desktop.vertices"),
                ("Via diameter", "electronics.pcb.rules_via_diameter"),
                ("Via drill", "electronics.pcb.rules_via_drill"),
                ("Position", "app.position"),
            ]
            .into_iter()
            .filter_map(|(field, key)| {
                field_value(fields, field).map(|v| (key.to_owned(), v.to_owned()))
            })
            .collect(),
            reference: field_value(fields, "Reference").map(str::to_owned),
            pcb_labels: field_value(fields, "Show reference").map(|v| {
                (
                    v == "true",
                    field_value(fields, "Show value") == Some("true"),
                )
            }),
            transform_target: field_value(fields, "Transform target").map(str::to_owned),
            kind: field_value(fields, "Category")
                .unwrap_or("Selected item")
                .to_string(),
            value: field_value(fields, "Value").unwrap_or("-").to_string(),
            category: field_value(fields, "Category").unwrap_or("-").to_string(),
            position: field_value(fields, "Position").unwrap_or("-").to_string(),
            rotation: field_value(fields, "Rotation").unwrap_or("-").to_string(),
            footprint: field_value(fields, "Footprint").unwrap_or("-").to_string(),
            pins: pins.to_vec(),
            locked: !editable_value,
            editable_value,
            identity_label: field_value(fields, "Identity label")
                .unwrap_or("Value")
                .to_string(),
        })
    };
    let root = UiNode::new("electronics.inspector", UiNodeKind::Panel)
        .with_class("electronics-inspector")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            padding: UiSpacing::xy(PANEL_PADDING_X + 2.0, PANEL_PADDING_Y),
            overflow: UiOverflow::Clip,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(desktop_tabs(palette, page))
        .with_child(match (page, model.as_ref()) {
            ("sessions", _) => sessions_content(palette, sessions),
            ("appearance", None) => appearance_panel(palette, None, presentation),
            (_, None) => empty_panel(palette),
            (_, Some(model)) => {
                selected_panel(palette, model, fabrication_status, presentation, page)
            }
        });
    let mut surface = UiSurface::new("electronics.inspector", palette, root);
    let mut styles = inspector_style_sheet(palette);
    styles.rules.extend(style_sheet(palette).rules);
    surface.style_sheet = styles;
    surface
}

fn desktop_tabs(palette: StudioUiPalette, page: &str) -> UiNode {
    let t = crate::panels::electronics_surface::electronics_tokens(palette);
    let mut row =
        UiNode::new("electronics.inspector.tabs", UiNodeKind::Toolbar).with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 1.0,
            ..UiLayout::fixed(0.0, 38.0).fill_width()
        });
    for (id, key) in [
        ("properties", "app.properties"),
        ("appearance", "electronics.desktop.appearance"),
        ("nets", "electronics.rail.nets"),
    ] {
        let active = page == id;
        row = row.with_child(
            UiNode::new(
                format!("electronics.inspector.{id}-tab"),
                UiNodeKind::Button,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                justify_content: raf_ui::UiJustify::End,
                padding: UiSpacing::xy(3.0, 0.0),
                ..UiLayout::fixed(0.0, 38.0).fill_width().grow(1.0)
            })
            .with_text_key(key)
            .with_text_style(electronics_body_style(if active {
                t.text
            } else {
                t.text_muted
            }))
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_style(raf_ui::UiStyle::transparent().with_fill(if active {
                t.surface_raised
            } else {
                t.surface
            }))
            .with_child(
                UiNode::new(
                    format!("electronics.inspector.{id}.edge"),
                    UiNodeKind::Separator,
                )
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
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("electronics.inspector.tab:{id}"),
            )),
        );
    }
    row
}

/// Backend field name lookup. Only the backend projection uses these strings.
fn field_value<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(label, _)| label == key)
        .map(|(_, value)| value.as_str())
}

fn empty_panel(palette: StudioUiPalette) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    UiNode::new("electronics.inspector.empty", UiNodeKind::Panel)
        .with_class("electronics-inspector-empty")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(18.0, 24.0),
            grow: 1.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(
            UiNode::new("electronics.inspector.empty.icon", UiNodeKind::Label)
                .with_icon(UiIcon::new(UiIconId::Select).with_size(UiIconSize::Panel)),
        )
        .with_child(
            UiNode::new("electronics.inspector.empty.title", UiNodeKind::Label)
                .with_text_key("app.electronics_no_selection")
                .with_text_style(UiTextStyle {
                    role: UiTextRole::PanelTitle,
                    size_px: 14.0,
                    line_height_px: 18.0,
                    weight: UiFontWeight::Bold,
                    color: tokens.text,
                    inherit_color: false,
                }),
        )
        .with_child(
            UiNode::new("electronics.inspector.empty.body", UiNodeKind::Label)
                .with_text_key("app.electronics_inspector_select_hint")
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_text_style(electronics_body_style(tokens.text_muted))
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        )
}

fn selected_panel(
    palette: StudioUiPalette,
    model: &ElectronicsInspectorModel,
    fabrication_status: Option<&str>,
    presentation: Option<&ElectronicsInspectorPresentation>,
    page: &str,
) -> UiNode {
    if matches!(model.kind.as_str(), "PCB trace" | "PCB via") && page != "appearance" {
        return trace_panel(palette, model);
    }
    let mut panel = UiNode::scroll_view(
        "electronics.inspector.scroll",
        raf_ui::UiScrollAxis::Vertical,
    )
    .with_class("electronics-inspector-scroll")
    .with_layout(UiLayout {
        gap: 8.0,
        padding: UiSpacing {
            right: 16.0,
            ..UiSpacing::same(0.0)
        },
        ..UiLayout::fill(UiFlow::Column)
    })
    .with_child(component_header(palette, model, presentation));
    let mut identity = editable_identity(palette, model);
    identity
        .children
        .retain(|node| node.id != "electronics.inspector.section-title.identity");
    panel = panel
        .with_child(identity)
        .with_child(footprint_section(
            palette,
            &model.footprint,
            fabrication_status,
            presentation,
        ))
        .with_child(if model.transform_target.is_some() {
            transform_section(palette, model)
        } else {
            section(
                palette,
                "placement",
                "app.electronics_placement",
                &[
                    ("app.position", FieldValue::text(&model.position)),
                    ("app.rotation", FieldValue::text(&model.rotation)),
                    (
                        "app.electronics_state",
                        FieldValue::Key(if model.locked {
                            "app.electronics_locked"
                        } else {
                            "app.electronics_editable"
                        }),
                    ),
                ],
            )
        });
    let pin_pad_mapping = presentation
        .map(|presentation| presentation.pin_pad_mapping.as_slice())
        .unwrap_or_default();
    let pin_heading = "app.electronics_pins";

    let mut pins = UiNode::new("electronics.inspector.pins", UiNodeKind::Panel)
        .with_class("electronics-inspector-section")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            padding: UiSpacing::xy(8.0, 6.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(section_title(palette, "pins", pin_heading));
    for (index, (name, net)) in model.pins.iter().enumerate() {
        let pad = pin_pad_mapping
            .iter()
            .find(|(pin, _)| pin == name)
            .map(|(_, pad)| pad.as_str());
        pins = pins.with_child(pin_row(palette, index, name, Some(net), pad));
    }
    for (index, (pin, pad)) in pin_pad_mapping.iter().enumerate() {
        if model.pins.iter().any(|(name, _)| name == pin) {
            continue;
        }
        pins = pins.with_child(pin_row(
            palette,
            model.pins.len() + index,
            pin,
            None,
            Some(pad),
        ));
    }
    if page == "nets" {
        let selected = panel.children.remove(0);
        panel.children.clear();
        panel = panel.with_child(selected).with_child(pins);
    } else if page == "appearance" {
        let selected = panel.children.remove(0);
        panel.children.clear();
        panel = panel.with_child(selected).with_child(appearance_panel(
            palette,
            Some(model),
            presentation,
        ));
    }
    if page != "nets" {
        panel = panel.with_child(preview_panel(palette, presentation));
    }
    panel
}

/// Numeric edits carry their original document surface and component UUID.
/// Enter commits once; Escape drops the draft without changing the document.
fn component_header(
    palette: StudioUiPalette,
    model: &ElectronicsInspectorModel,
    presentation: Option<&ElectronicsInspectorPresentation>,
) -> UiNode {
    let t = crate::panels::electronics_surface::electronics_tokens(palette);
    let symbol =
        if let Some(key) = presentation.and_then(|p| p.component_symbol_image_key.as_deref()) {
            UiNode::image(
                "electronics.inspector.selected.icon",
                UiImage {
                    source: UiImageSource::new(key),
                    fit: UiImageFit::Contain,
                    tint: Some(t.accent),
                    options: Default::default(),
                },
            )
        } else {
            UiNode::new("electronics.inspector.selected.icon", UiNodeKind::Label).with_icon(
                UiIcon::new(inspector_icon_for_kind(&model.kind))
                    .with_size(UiIconSize::Custom(32))
                    .with_tint(t.accent),
            )
        };
    let mut kind = UiNode::new("electronics.inspector.selected.kind", UiNodeKind::Label)
        .with_text_style(electronics_body_style(t.text_muted))
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_layout(UiLayout::fixed(0.0, 20.0).fill_width());
    kind = if let Some(name) = presentation.and_then(|p| p.component_name.as_deref()) {
        kind.with_text_value(name)
    } else if model.kind == "PCB via" {
        kind.with_text_key("electronics.pcb.via_label")
    } else if model.kind == "PCB component" {
        kind.with_text_key("electronics.desktop.pcb_component")
    } else {
        kind.with_text_value(&model.kind)
    };
    UiNode::new("electronics.inspector.selected", UiNodeKind::Panel)
        .with_class("electronics-inspector-selected")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            ..UiLayout::fit_content().fill_width()
        })
        .with_child(section_title(
            palette,
            "selected",
            if model.kind == "PCB trace" {
                "electronics.desktop.trace"
            } else {
                "electronics.desktop.component"
            },
        ))
        .with_child(
            UiNode::new("electronics.inspector.selected.header", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 10.0,
                    padding: UiSpacing::xy(8.0, 4.0),
                    ..UiLayout::fixed(0.0, 60.0).fill_width()
                })
                .with_child(symbol.with_layout(UiLayout::fixed(42.0, 42.0)))
                .with_child(
                    UiNode::new("electronics.inspector.selected.text", UiNodeKind::Panel)
                        .with_layout(UiLayout {
                            flow: UiFlow::Column,
                            gap: 2.0,
                            ..UiLayout::fixed(0.0, 46.0).fill_width().grow(1.0)
                        })
                        .with_child(
                            UiNode::new("electronics.inspector.selected.title", UiNodeKind::Label)
                                .with_text_value(model.reference.as_deref().unwrap_or(&model.title))
                                .with_text_style(UiTextStyle {
                                    size_px: 18.0,
                                    line_height_px: 24.0,
                                    ..UiTextStyle::panel_title(t.text)
                                })
                                .with_text_overflow(UiTextOverflow::Ellipsis)
                                .with_layout(UiLayout::fixed(0.0, 24.0).fill_width()),
                        )
                        .with_child(kind),
                ),
        )
}

fn trace_panel(palette: StudioUiPalette, model: &ElectronicsInspectorModel) -> UiNode {
    let t = crate::panels::electronics_surface::electronics_tokens(palette);
    let mut panel = UiNode::scroll_view(
        "electronics.inspector.trace",
        raf_ui::UiScrollAxis::Vertical,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 4.0,
        padding: UiSpacing::same(8.0),
        ..UiLayout::fill(UiFlow::Column)
    })
    .with_child(section_title(palette, "trace", if model.kind == "PCB via" { "electronics.pcb.via_label" } else { "electronics.desktop.trace" }))
    .with_child(
        UiNode::new("electronics.inspector.trace.name", UiNodeKind::Label)
            .with_icon(
                UiIcon::new(if model.kind == "PCB via" { UiIconId::CadVia } else { UiIconId::CadRoute })
                    .with_size(UiIconSize::Custom(28))
                    .with_tint(t.accent),
            )
            .with_text_value(&model.title)
            .with_text_style(UiTextStyle {
                size_px: 16.0,
                line_height_px: 22.0,
                ..UiTextStyle::panel_title(t.text)
            })
            .with_layout(UiLayout::fixed(0.0, 46.0).fill_width()),
    );
    for (index, (key, value)) in
        std::iter::once(("app.schematic_net".to_owned(), model.value.clone()))
            .chain(model.trace_details.iter().cloned())
            .enumerate()
    {
        panel = panel.with_child(
            UiNode::new(
                format!("electronics.inspector.trace.row.{index}"),
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                gap: 10.0,
                align_items: UiAlign::Center,
                ..UiLayout::fixed(0.0, 32.0).fill_width()
            })
            .with_child(
                UiNode::new(
                    format!("electronics.inspector.trace.key.{index}"),
                    UiNodeKind::Label,
                )
                .with_text_key(key)
                .with_text_style(electronics_body_style(t.text_muted))
                .with_layout(UiLayout::fixed(96.0, 24.0)),
            )
            .with_child(
                UiNode::new(
                    format!("electronics.inspector.trace.value.{index}"),
                    UiNodeKind::Label,
                )
                .with_text_value(value)
                .with_text_style(electronics_body_style(t.text))
                .with_layout(UiLayout::fixed(0.0, 28.0).fill_width().grow(1.0)),
            ),
        );
    }
    panel.with_child(
        UiNode::new("electronics.inspector.trace.highlight", UiNodeKind::Button)
            .with_class("electronics-inspector-footprint-action")
            .with_icon(UiIcon::new(UiIconId::Eye))
            .with_text_key("electronics.desktop.highlight_net")
            .with_text_style(electronics_body_style(t.text))
            .with_layout(UiLayout::fixed(0.0, 34.0).fill_width())
            .with_accessibility_role(UiAccessibilityRole::Button)
            .with_accessibility_label_key("electronics.desktop.highlight_net")
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("electronics.net.highlight:{}", model.value),
            )),
    )
}

fn preview_panel(
    palette: StudioUiPalette,
    presentation: Option<&ElectronicsInspectorPresentation>,
) -> UiNode {
    let t = crate::panels::electronics_surface::electronics_tokens(palette);
    let mut panel = UiNode::new("electronics.inspector.preview-beta", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 7.0,
            padding: UiSpacing::xy(8.0, 4.0),
            ..UiLayout::fit_content().fill_width()
        })
        .with_child(section_title(
            palette,
            "preview",
            "electronics.footprint.preview",
        ));
    if let Some(key) = presentation.and_then(|p| p.footprint_preview_image_key.as_deref()) {
        panel = panel.with_child(
            UiNode::image(
                "electronics.inspector.footprint.preview",
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new(key),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_class("electronics-inspector-footprint-preview")
            .with_accessibility_role(UiAccessibilityRole::Image)
            .with_accessibility_label_key("electronics.footprint.preview")
            .with_layout(UiLayout::fixed(0.0, 100.0).fill_width()),
        );
    }
    panel.with_child(
        UiNode::new("electronics.inspector.preview-open", UiNodeKind::Button)
            .with_icon(UiIcon::new(UiIconId::Entity))
            .with_text_key("electronics.desktop.preview_beta")
            .with_text_style(electronics_body_style(t.text_muted))
            .with_layout(UiLayout::fixed(0.0, 32.0).fill_width())
            .with_tooltip_key("electronics.desktop.preview_pending_short")
            .with_accessibility_role(UiAccessibilityRole::Button)
            .with_accessibility_label_key("electronics.desktop.preview_beta")
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                "electronics.pcb.preview.toggle",
            )),
    )
}

fn legend_appearance(palette: StudioUiPalette, visibility: (bool, bool)) -> UiNode {
    let t = crate::panels::electronics_surface::electronics_tokens(palette);
    let (reference, value) = visibility;
    let mut panel = UiNode::new("electronics.inspector.legend", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 7.0,
            padding: UiSpacing::same(8.0),
            ..UiLayout::fit_content().fill_width()
        })
        .with_child(section_title(
            palette,
            "legend",
            "electronics.desktop.board_view",
        ));
    for (id, key, visible) in [
        (
            "reference",
            "electronics.desktop.show_all_references",
            reference,
        ),
        ("value", "electronics.desktop.show_all_values", value),
    ] {
        panel = panel.with_child(
            UiNode::new(
                format!("electronics.inspector.legend.{id}.row"),
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                ..UiLayout::fixed(0.0, 28.0).fill_width()
            })
            .with_child(
                UiNode::new(
                    format!("electronics.inspector.legend.{id}.label"),
                    UiNodeKind::Label,
                )
                .with_text_key(key)
                .with_text_style(electronics_body_style(t.text))
                .with_layout(UiLayout::fixed(0.0, 24.0).fill_width().grow(1.0)),
            )
            .with_child(
                UiNode::new(
                    format!("electronics.inspector.legend.{id}"),
                    UiNodeKind::Button,
                )
                .with_layout(UiLayout::fixed(40.0, 22.0))
                .with_style(raf_ui::UiStyle {
                    fill: if visible { t.accent } else { t.surface_raised },
                    border: t.border,
                    text: t.text,
                    border_width: 1.0,
                    radius: 11.0,
                    opacity: 1.0,
                })
                .with_child(
                    UiNode::new(
                        format!("electronics.inspector.legend.{id}.knob"),
                        UiNodeKind::Panel,
                    )
                    .with_layout(UiLayout::absolute(raf_ui::UiRect::new(
                        if visible { 21.0 } else { 3.0 },
                        3.0,
                        16.0,
                        16.0,
                    )))
                    .with_style(raf_ui::UiStyle {
                        fill: t.text,
                        border: [0, 0, 0, 0],
                        text: t.text,
                        border_width: 0.0,
                        radius: 8.0,
                        opacity: 1.0,
                    }),
                )
                .with_accessibility_role(UiAccessibilityRole::Checkbox)
                .with_accessibility_label_key(key)
                .with_accessibility_checked(visible)
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("electronics.pcb.labels:{id}:{}", !visible),
                )),
            ),
        );
    }
    panel
}

fn appearance_panel(
    palette: StudioUiPalette,
    model: Option<&ElectronicsInspectorModel>,
    presentation: Option<&ElectronicsInspectorPresentation>,
) -> UiNode {
    let t = crate::panels::electronics_surface::electronics_tokens(palette);
    let mut panel =
        UiNode::new("electronics.inspector.appearance", UiNodeKind::Panel).with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 10.0,
            ..UiLayout::fit_content().fill_width()
        });
    if let Some(model) = model {
        if let Some(target) = model.transform_target.as_deref() {
            panel = panel.with_child(
                UiNode::new("electronics.inspector.component-lock", UiNodeKind::Button)
                    .with_icon(UiIcon::new(UiIconId::Lock))
                    .with_text_key(if model.locked {
                        "electronics.desktop.unlock"
                    } else {
                        "electronics.desktop.lock"
                    })
                    .with_text_style(electronics_body_style(t.text))
                    .with_layout(UiLayout::fixed(0.0, 32.0).fill_width())
                    .with_accessibility_role(UiAccessibilityRole::Button)
                    .with_accessibility_label_key("electronics.desktop.lock")
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        format!("electronics.component.lock:{target}:{}", !model.locked),
                    )),
            );
        }
    }
    if let Some(visibility) = model
        .and_then(|m| m.pcb_labels)
        .or_else(|| presentation.and_then(|p| p.board_label_visibility))
    {
        panel = panel.with_child(legend_appearance(palette, visibility));
    }
    panel.with_child(
        UiNode::new(
            "electronics.inspector.appearance.layers",
            UiNodeKind::Button,
        )
        .with_icon(UiIcon::new(UiIconId::Pcb))
        .with_text_key("electronics.rail.layers")
        .with_text_style(electronics_body_style(t.text))
        .with_layout(UiLayout::fixed(0.0, 32.0).fill_width())
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_label_key("electronics.rail.layers")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "electronics.navigator.tab:layers",
        )),
    )
}

fn transform_section(palette: StudioUiPalette, model: &ElectronicsInspectorModel) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let target = model.transform_target.as_deref().unwrap_or_default();
    let mut section = UiNode::new("electronics.inspector.placement", UiNodeKind::Panel)
        .with_class("electronics-inspector-section")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            padding: UiSpacing::xy(8.0, 4.0),
            ..UiLayout::fit_content().fill_width()
        })
        .with_child(section_title(
            palette,
            "placement",
            "app.electronics_placement",
        ));
    for (axis, key) in [("x", "X"), ("y", "Y"), ("rotation", "app.rotation")] {
        let id = format!("electronics.inspector.transform.{target}:{axis}");
        let mut field = UiNode::text_input(
            id.clone(),
            UiTextInput {
                value_key: id.clone(),
                placeholder_key: None,
                max_length: 24,
                multiline: false,
                password: false,
                submit_command: Some(format!(
                    "electronics.inspector.transform.commit:{target}:{axis}"
                )),
            },
        )
        .with_class("electronics-inspector-input")
        .with_layout(
            UiLayout::fixed(0.0, 28.0)
                .fill_width()
                .grow(1.0)
                .with_text_safe_area(true),
        )
        .with_text_style(electronics_body_style(tokens.text))
        .with_accessibility_role(UiAccessibilityRole::Textbox)
        .with_accessibility_label_key(key)
        .with_tooltip_key("electronics.desktop.transform_help");
        field.disabled = model.locked;
        section = section.with_child(
            UiNode::new(format!("{id}.row"), UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 8.0,
                    ..UiLayout::fixed(0.0, 28.0).fill_width()
                })
                .with_child(
                    UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                        .with_text_key(key)
                        .with_text_style(electronics_body_style(tokens.text_muted))
                        .with_layout(UiLayout::fixed(76.0, 24.0)),
                )
                .with_child(field)
                .with_child(
                    UiNode::new(format!("{id}.unit"), UiNodeKind::Label)
                        .with_text_value(if axis == "rotation" { "°" } else { "mm" })
                        .with_text_style(electronics_body_style(tokens.text_muted))
                        .with_layout(UiLayout {
                            min_size: [28.0, 0.0],
                            ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::FitContent)
                        }),
                ),
        );
    }
    let mut actions = UiNode::new(
        "electronics.inspector.transform.actions",
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        gap: 5.0,
        ..UiLayout::fixed(0.0, 30.0).fill_width()
    });
    let rotation = model
        .rotation
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<f32>().ok());
    if let Some(rotation) = rotation {
        for (id, delta, label) in [("left", -90.0, "−90°"), ("right", 90.0, "+90°")] {
            let mut button = UiNode::new(
                format!("electronics.inspector.rotate.{id}"),
                UiNodeKind::Button,
            )
            .with_class("electronics-inspector-footprint-action")
            .with_text_value(label)
            .with_text_style(electronics_body_style(tokens.text))
            .with_layout(UiLayout::fixed(64.0, 30.0))
            .with_accessibility_role(UiAccessibilityRole::Button)
            .with_accessibility_label_key("app.rotation")
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!(
                    "electronics.transform:{target}:rotation:{}",
                    rotation + delta
                ),
            ));
            button.disabled = model.locked;
            actions = actions.with_child(button);
        }
    }
    actions = actions.with_child(
        UiNode::new("electronics.inspector.pin", UiNodeKind::Button)
            .with_icon(UiIcon::new(UiIconId::Lock))
            .with_text_key(if model.locked {
                "electronics.desktop.unlock"
            } else {
                "electronics.desktop.lock"
            })
            .with_class("electronics-inspector-footprint-action")
            .with_text_style(electronics_body_style(tokens.text))
            .with_layout(UiLayout::fixed(0.0, 30.0).fill_width().grow(1.0))
            .with_accessibility_role(UiAccessibilityRole::Button)
            .with_accessibility_label_key("electronics.desktop.lock")
            .with_accessibility_selected(model.locked)
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("electronics.component.lock:{target}:{}", !model.locked),
            )),
    );
    section = section.with_child(actions);
    section
}

fn pin_row(
    palette: StudioUiPalette,
    index: usize,
    pin: &str,
    net: Option<&str>,
    pad: Option<&str>,
) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let mut row = UiNode::new(
        format!("electronics.inspector.pin.{index}"),
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 6.0,
        ..UiLayout::fixed(0.0, ELECTRONICS_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(
            format!("electronics.inspector.pin.{index}.name"),
            UiNodeKind::Label,
        )
        .with_text_value(pin.to_string())
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_text_style(electronics_body_style(tokens.text))
        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::FitContent)),
    );

    if let Some(pad) = pad {
        row = row
            .with_child(
                UiNode::new(
                    format!("electronics.inspector.pin.{index}.arrow"),
                    UiNodeKind::Label,
                )
                .with_text_value(">")
                .with_text_style(electronics_body_style(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
            )
            .with_child(
                UiNode::new(
                    format!("electronics.inspector.pin.{index}.pad"),
                    UiNodeKind::Label,
                )
                .with_text_value(if pad.is_empty() { "—" } else { pad }.to_string())
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(electronics_body_style(tokens.text))
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::FitContent)),
            );
    }

    if let Some(net) = net {
        let mut net_node = UiNode::new(
            format!("electronics.inspector.pin.{index}.net"),
            UiNodeKind::Label,
        )
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_text_style(electronics_body_style(tokens.text_muted))
        .with_layout(
            UiLayout::fit_content()
                .with_width_mode(UiSizeMode::Fill)
                .grow(1.0),
        );
        net_node = if net.is_empty() {
            net_node.with_text_key("app.electronics_unconnected")
        } else {
            net_node.with_text_value(net.to_string())
        };
        row = row.with_child(net_node);
    }
    if let Some(net) = net.filter(|n| !n.is_empty()) {
        row = row
            .with_accessibility_role(UiAccessibilityRole::Button)
            .with_accessibility_label_key("electronics.rail.nets")
            .with_tooltip_value(net)
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("electronics.net.highlight:{net}"),
            ));
    }
    row
}

fn footprint_section(
    palette: StudioUiPalette,
    footprint: &str,
    fabrication_status: Option<&str>,
    presentation: Option<&ElectronicsInspectorPresentation>,
) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let footprint_name = presentation
        .and_then(|presentation| presentation.footprint_name.as_deref())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(footprint);
    let mut section = UiNode::new("electronics.inspector.footprint", UiNodeKind::Panel)
        .with_class("electronics-inspector-section")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fit_content().fill_width()
        })
        .with_child(
            UiNode::new("electronics.inspector.footprint.row", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 6.0,
                    ..UiLayout::fixed(0.0, 32.0).fill_width()
                })
                .with_child(
                    UiNode::new("electronics.inspector.footprint.title", UiNodeKind::Label)
                        .with_text_key("app.schematic_footprint")
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_text_style(electronics_body_style(tokens.text_muted))
                        .with_layout(UiLayout::fixed(76.0, 24.0)),
                )
                .with_child(
                    UiNode::new("electronics.inspector.footprint.value", UiNodeKind::Label)
                        .with_text_value(footprint_name)
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_tooltip_value(footprint_name)
                        .with_text_style(electronics_body_style(tokens.text))
                        .with_class("electronics-inspector-input")
                        .with_layout(UiLayout::fixed(0.0, 30.0).fill_width().grow(1.0)),
                )
                .with_child(
                    UiNode::new("electronics.inspector.footprint.change", UiNodeKind::Button)
                        .with_class("electronics-inspector-footprint-action")
                        .with_text_value("…")
                        .with_text_style(electronics_body_style(tokens.text))
                        .with_layout(UiLayout::fixed(30.0, 30.0))
                        .with_accessibility_role(UiAccessibilityRole::Button)
                        .with_accessibility_label_key("electronics.footprint.change")
                        .with_tooltip_key("electronics.footprint.change")
                        .focusable()
                        .with_event(UiEventBinding::command(
                            UiEventKind::Click,
                            "electronics.footprint.open",
                        )),
                ),
        );

    if let Some(dimensions_row) =
        presentation.and_then(|presentation| footprint_dimensions_row(palette, presentation))
    {
        section = section.with_child(dimensions_row);
    }

    if fabrication_status.is_some() {
        section = section.with_child(
            UiNode::new(
                "electronics.inspector.fabrication.export",
                UiNodeKind::Button,
            )
            .with_class("electronics-inspector-footprint-action")
            .with_layout(
                UiLayout::fixed(0.0, ELECTRONICS_CONTROL_HEIGHT).with_width_mode(UiSizeMode::Fill),
            )
            .with_icon(
                UiIcon::new(UiIconId::Pcb)
                    .with_size(UiIconSize::Small)
                    .with_tint(tokens.accent),
            )
            .with_text_key("electronics.fabrication.export")
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_text_style(UiTextStyle::button(tokens.text))
            .with_accessibility_role(UiAccessibilityRole::Button)
            .with_accessibility_label_key("electronics.fabrication.export")
            .with_accessibility_description_key("electronics.fabrication.export_help")
            .with_tooltip_key("electronics.fabrication.export_help")
            .focusable()
            .with_event(raf_ui::UiEventBinding::command(
                raf_ui::UiEventKind::Click,
                "electronics.fabrication.export",
            )),
        );
    }
    if let Some(status) = fabrication_status.filter(|status| !status.trim().is_empty()) {
        section = section.with_child(
            UiNode::new(
                "electronics.inspector.fabrication.status",
                UiNodeKind::Label,
            )
            .with_text_value(status.to_string())
            .with_text_overflow(UiTextOverflow::Wrap)
            .with_text_style(electronics_body_style(tokens.text_muted))
            .with_accessibility_role(UiAccessibilityRole::Status)
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    section
}

fn footprint_dimensions_row(
    palette: StudioUiPalette,
    presentation: &ElectronicsInspectorPresentation,
) -> Option<UiNode> {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let dimensions = presentation
        .footprint_dimensions_mm
        .and_then(|[width, height]| Some((format_mm(width)?, format_mm(height)?)));
    let pad_count = presentation.footprint_pad_count;
    if dimensions.is_none() && pad_count.is_none() {
        return None;
    }

    let mut row = UiNode::new(
        "electronics.inspector.footprint.dimensions",
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 5.0,
        ..UiLayout::fixed(0.0, ELECTRONICS_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
    });
    if let Some((width, height)) = dimensions {
        row = row
            .with_child(
                UiNode::new(
                    "electronics.inspector.footprint.dimensions.label",
                    UiNodeKind::Label,
                )
                .with_text_key("electronics.footprint.dimensions")
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(electronics_body_style(tokens.text_muted))
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::FitContent)),
            )
            .with_child(
                UiNode::new(
                    "electronics.inspector.footprint.dimensions.value",
                    UiNodeKind::Label,
                )
                .with_text_value(format!("{width} × {height} mm"))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(electronics_body_style(tokens.text))
                .with_layout(
                    UiLayout::fit_content()
                        .with_width_mode(UiSizeMode::Fill)
                        .grow(1.0),
                ),
            );
    }

    if let Some(count) = pad_count {
        row = row
            .with_child(
                UiNode::new(
                    "electronics.inspector.footprint.pad-count.value",
                    UiNodeKind::Label,
                )
                .with_text_value(count.to_string())
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(electronics_body_style(tokens.text))
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::FitContent)),
            )
            .with_child(
                UiNode::new(
                    "electronics.inspector.footprint.pad-count.label",
                    UiNodeKind::Label,
                )
                .with_text_key("electronics.footprint.pads")
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(electronics_body_style(tokens.text_muted))
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::FitContent)),
            );
    }

    Some(row)
}

fn format_mm(value: f32) -> Option<String> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    let mut value = format!("{value:.2}");
    while value.ends_with('0') {
        value.pop();
    }
    if value.ends_with('.') {
        value.pop();
    }
    Some(value)
}

/// Icon for a backend category value.
///
/// The value is runtime data, so this stays a tolerant icon heuristic and never
/// becomes user-visible text.
fn inspector_icon_for_kind(kind: &str) -> UiIconId {
    let kind = kind.to_ascii_lowercase();
    if kind.contains("trace") {
        UiIconId::CadRoute
    } else if kind.contains("pcb") {
        UiIconId::Pcb
    } else if kind.contains("wire") || kind.contains("connection") {
        UiIconId::Wire
    } else {
        UiIconId::Schematic
    }
}

fn editable_identity(palette: StudioUiPalette, model: &ElectronicsInspectorModel) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let value = if model.editable_value {
        UiNode::text_input(
            "electronics.inspector.value",
            UiTextInput {
                value_key: "electronics.inspector.value".to_string(),
                placeholder_key: Some("electronics.inspector.value.placeholder".to_string()),
                max_length: 128,
                multiline: false,
                password: false,
                submit_command: Some("electronics.inspector.value.commit".to_string()),
            },
        )
        .with_class("electronics-inspector-input")
        .with_accessibility_role(UiAccessibilityRole::Textbox)
        .with_accessibility_label_key(identity_field_key(&model.identity_label))
        // The native host applies Enter or focus-loss to the original
        // component identity; Escape restores the document's value.
        .with_accessibility_description_key("electronics.tooltip.inspector.value_commit")
        .with_tooltip_key("electronics.tooltip.inspector.value_commit")
        .with_layout(
            UiLayout::fixed(0.0, VALUE_INPUT_HEIGHT)
                .with_width_mode(UiSizeMode::Fill)
                .with_text_safe_area(true),
        )
        .with_text_style(electronics_body_style(tokens.text))
    } else {
        UiNode::new("electronics.inspector.identity.value", UiNodeKind::Label)
            .with_text_value(model.value.clone())
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_text_style(electronics_body_style(tokens.text))
            .with_layout(UiLayout::fixed(0.0, VALUE_LABEL_HEIGHT).with_width_mode(UiSizeMode::Fill))
    };
    let mut panel = UiNode::new("electronics.inspector.identity", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            padding: UiSpacing::xy(8.0, 8.0),
            ..UiLayout::fit_content().fill_width()
        })
        .with_child(section_title(
            palette,
            "identity",
            "electronics.desktop.component",
        ))
        .with_child(
            UiNode::new(
                "electronics.inspector.identity.value-row",
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                gap: 8.0,
                align_items: UiAlign::Center,
                ..UiLayout::fixed(0.0, VALUE_INPUT_HEIGHT).fill_width()
            })
            .with_child(
                identity_label(palette, &model.identity_label)
                    .with_layout(UiLayout::fixed(76.0, 24.0)),
            )
            .with_child(
                value.with_layout(
                    UiLayout::fixed(0.0, VALUE_INPUT_HEIGHT)
                        .fill_width()
                        .grow(1.0),
                ),
            ),
        );
    if let Some(reference) = model.reference.as_deref() {
        panel = panel.with_child(
            UiNode::new("electronics.inspector.reference.row", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 8.0,
                    ..UiLayout::fixed(0.0, 30.0).fill_width()
                })
                .with_child(
                    UiNode::new("electronics.inspector.reference.label", UiNodeKind::Label)
                        .with_text_key("electronics.desktop.reference")
                        .with_text_style(electronics_body_style(tokens.text_muted))
                        .with_layout(UiLayout::fixed(76.0, 24.0)),
                )
                .with_child(
                    UiNode::new("electronics.inspector.reference.value", UiNodeKind::Label)
                        .with_text_value(reference)
                        .with_text_style(electronics_body_style(tokens.text))
                        .with_layout(UiLayout::fixed(0.0, 28.0).fill_width().grow(1.0)),
                ),
        );
    }
    panel
}

/// One grouped property block.
fn section(
    palette: StudioUiPalette,
    id: &str,
    title_key: &'static str,
    fields: &[(&'static str, FieldValue)],
) -> UiNode {
    let mut section = UiNode::new(
        format!("electronics.inspector.section.{id}"),
        UiNodeKind::Panel,
    )
    .with_class("electronics-inspector-section")
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 3.0,
        padding: UiSpacing::xy(8.0, 7.0),
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    })
    .with_child(section_title(palette, id, title_key));
    for (index, (label_key, value)) in fields.iter().enumerate() {
        section = section.with_child(
            UiNode::new(
                format!("electronics.inspector.{id}.field.{index}"),
                UiNodeKind::Toolbar,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 6.0,
                ..UiLayout::fixed(0.0, FIELD_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new(
                    format!("electronics.inspector.{id}.field.{index}.label"),
                    UiNodeKind::Label,
                )
                .with_text_key(*label_key)
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(electronics_body_style(
                    crate::panels::electronics_surface::electronics_tokens(palette).text_muted,
                ))
                .with_layout(UiLayout::fixed(96.0, 24.0)),
            )
            .with_child(field_value_node(palette, id, index, value)),
        );
    }
    section
}

/// How one property value reaches the document.
///
/// The distinction is explicit because a formatted runtime value ("12.0, 4.0",
/// a net name) must never be guessed into a localization key, and a catalog
/// key must never be shown raw.
#[derive(Debug, Clone)]
enum FieldValue {
    /// Localized catalog key.
    Key(&'static str),
    /// Runtime value coming from the document.
    Text(String),
}

impl FieldValue {
    fn text(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

/// Value cell of a property row.
fn field_value_node(
    palette: StudioUiPalette,
    section_id: &str,
    index: usize,
    value: &FieldValue,
) -> UiNode {
    let node = UiNode::new(
        format!("electronics.inspector.{section_id}.field.{index}.value"),
        UiNodeKind::Label,
    )
    .with_text_overflow(UiTextOverflow::Ellipsis)
    .with_text_style(electronics_body_style(
        crate::panels::electronics_surface::electronics_tokens(palette).text,
    ))
    .with_layout(UiLayout::fixed(0.0, 24.0).fill_width().grow(1.0));
    let node =
        node.with_style(raf_ui::UiStyle::transparent().with_fill(
            crate::panels::electronics_surface::electronics_tokens(palette).surface_raised,
        ));
    match value {
        FieldValue::Key(key) => node.with_text_key(*key),
        FieldValue::Text(value) => node.with_text_value(value.clone()),
    }
}

fn section_title(palette: StudioUiPalette, id: &str, title_key: &'static str) -> UiNode {
    UiNode::new(
        format!("electronics.inspector.section-title.{id}"),
        UiNodeKind::Label,
    )
    .with_text_key(title_key)
    .with_text_overflow(UiTextOverflow::Ellipsis)
    .with_text_style(electronics_body_style(
        crate::panels::electronics_surface::electronics_tokens(palette).text,
    ))
    .with_style(
        raf_ui::UiStyle::transparent().with_fill(
            crate::panels::electronics_surface::electronics_tokens(palette).surface_raised,
        ),
    )
    .with_layout(UiLayout::fixed(0.0, SECTION_TITLE_HEIGHT).with_width_mode(UiSizeMode::Fill))
}

/// Label of the identity value. The backend sends a value, not a key, so it is
/// mapped to the closest real key instead of being shown raw.
fn identity_label(palette: StudioUiPalette, label: &str) -> UiNode {
    UiNode::new(
        "electronics.inspector.identity.value-label",
        UiNodeKind::Label,
    )
    .with_text_key(identity_field_key(label))
    .with_text_overflow(UiTextOverflow::Ellipsis)
    .with_text_style(electronics_body_style(
        crate::panels::electronics_surface::electronics_tokens(palette).text_muted,
    ))
}

fn identity_field_key(backend_label: &str) -> &'static str {
    match backend_label.to_ascii_lowercase().as_str() {
        "net" | "net name" => "app.schematic_net",
        "footprint" => "app.schematic_footprint",
        _ => "app.value",
    }
}

pub(crate) fn style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let classes = [
        (
            "electronics-inspector",
            tokens.background,
            tokens.border,
            tokens.text,
        ),
        (
            "electronics-inspector-header",
            tokens.surface_raised,
            with_alpha(tokens.border, 72),
            tokens.text,
        ),
        (
            "electronics-inspector-empty",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text,
        ),
        (
            "electronics-inspector-scroll",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text,
        ),
        (
            "electronics-inspector-selected",
            tokens.surface_raised,
            tokens.accent,
            tokens.text,
        ),
        (
            "electronics-inspector-section",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text,
        ),
        (
            "electronics-inspector-input",
            tokens.surface_alt,
            tokens.border,
            tokens.text,
        ),
        (
            "electronics-inspector-footprint-action",
            tokens.surface_alt,
            with_alpha(tokens.border, 112),
            tokens.text,
        ),
        (
            "electronics-inspector-footprint-preview",
            tokens.canvas,
            with_alpha(tokens.border, 96),
            tokens.text,
        ),
        (
            "electronics-icon-button",
            tokens.surface_alt,
            with_alpha(tokens.border, 88),
            tokens.text_muted,
        ),
    ];
    let mut rules: Vec<raf_ui::UiStyleRule> = classes
        .into_iter()
        .map(|(class, fill, border, text)| electronics_class_rule(class, fill, border, text))
        .collect();
    // The shared inspector sheet owns the tabs. These Electronics controls
    // keep their own focus and pressed states inside the rail.
    rules.push(electronics_hover_rule(
        "electronics-inspector-input",
        tokens,
    ));
    rules.push(electronics_focus_rule(
        "electronics-inspector-input",
        tokens,
    ));
    rules.push(electronics_active_rule(
        "electronics-inspector-input",
        tokens,
    ));
    rules.push(electronics_hover_rule(
        "electronics-inspector-footprint-action",
        tokens,
    ));
    rules.push(electronics_focus_rule(
        "electronics-inspector-footprint-action",
        tokens,
    ));
    rules.push(electronics_active_rule(
        "electronics-inspector-footprint-action",
        tokens,
    ));
    rules.push(electronics_hover_rule("electronics-icon-button", tokens));
    rules.push(electronics_focus_rule("electronics-icon-button", tokens));
    rules.push(electronics_active_rule("electronics-icon-button", tokens));
    rules.push(electronics_state_rule(
        raf_ui::UiStyleRuleState::Always,
        "electronics-inspector-selected",
        raf_ui::UiStylePatch {
            fill: Some([0, 0, 0, 0]),
            border: Some([0, 0, 0, 0]),
            border_width: Some(0.0),
            ..Default::default()
        },
    ));
    UiStyleSheet { rules }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::electronics_surface::assert_electronics_layout_gate;
    use raf_ui::UiStyleSelector;
    use raf_ui::UiTokens;

    fn fields() -> Vec<(String, String)> {
        vec![
            ("Value".to_string(), "10k".to_string()),
            ("Category".to_string(), "Passive".to_string()),
            ("Position".to_string(), "12.0, 4.0".to_string()),
            ("Rotation".to_string(), "90 deg".to_string()),
            ("Footprint".to_string(), "R_0805".to_string()),
            ("Identity label".to_string(), "Net".to_string()),
            ("Editable".to_string(), "true".to_string()),
        ]
    }

    fn surface(tab: InspectorTab) -> UiSurface {
        build_electronics_inspector_surface(
            StudioUiPalette::IndustrialDark,
            "R1  Resistor",
            &fields(),
            &[("1".to_string(), "N001".to_string())],
            &ProjectSessionRegistry::new(raf_core::project::ProjectType::Electronics),
            tab,
        )
    }

    #[test]
    fn board_visibility_is_explicit_and_absent_from_component_properties() {
        let sessions = ProjectSessionRegistry::new(raf_core::project::ProjectType::Electronics);
        let mut data = fields();
        data.extend([
            ("Show reference".into(), "true".into()),
            ("Show value".into(), "true".into()),
        ]);
        let properties = build_electronics_inspector_surface_with_page(
            StudioUiPalette::IndustrialDark,
            "R1",
            &data,
            &[],
            &sessions,
            "properties",
            None,
            None,
        );
        assert!(properties
            .root
            .find("electronics.inspector.legend")
            .is_none());
        let presentation = ElectronicsInspectorPresentation {
            board_label_visibility: Some((true, false)),
            ..Default::default()
        };
        let appearance = build_electronics_inspector_surface_with_page(
            StudioUiPalette::IndustrialDark,
            "",
            &[],
            &[],
            &sessions,
            "appearance",
            None,
            Some(&presentation),
        );
        assert_eq!(
            appearance
                .root
                .find("electronics.inspector.section-title.legend")
                .unwrap()
                .text_key
                .as_deref(),
            Some("electronics.desktop.board_view")
        );
        assert!(appearance
            .root
            .find("electronics.inspector.legend.reference")
            .is_some());
        assert!(appearance
            .root
            .find("electronics.inspector.legend.value")
            .is_some());
    }

    #[test]
    fn a_trace_inspector_reports_copper_and_never_offers_a_component_footprint() {
        let fields = vec![
            ("Category".into(), "PCB trace".into()),
            ("Value".into(), "VCC".into()),
            ("Trace width".into(), "0.250 mm".into()),
            ("Trace length".into(), "18.500 mm".into()),
            ("Trace layer".into(), "F.Cu".into()),
            ("Trace vertices".into(), "4".into()),
        ];
        let surface = build_electronics_inspector_surface(
            StudioUiPalette::IndustrialDark,
            "Trace #1",
            &fields,
            &[],
            &ProjectSessionRegistry::new(raf_core::project::ProjectType::Electronics),
            InspectorTab::Properties,
        );
        assert!(surface
            .root
            .find("electronics.inspector.footprint.change")
            .is_none());
        assert!(surface
            .root
            .find("electronics.inspector.preview-open")
            .is_none());
        assert_eq!(
            surface
                .root
                .find("electronics.inspector.trace.value.1")
                .and_then(|n| n.text_value.as_deref()),
            Some("0.250 mm")
        );
        assert!(surface.root.find("electronics.inspector.trace.highlight").unwrap().event_handlers.iter().any(|e|matches!(&e.action,raf_ui::UiAction::Command{name} if name=="electronics.net.highlight:VCC")));
        assert_electronics_layout_gate(&surface, 300, 720);
    }

    #[test]
    fn every_property_label_is_a_key_and_never_a_backend_field_name() {
        let surface = surface(InspectorTab::Properties);
        let mut pending = vec![&surface.root];
        while let Some(node) = pending.pop() {
            if let Some(key) = node.text_key.as_deref() {
                for language in [raf_core::Language::English, raf_core::Language::Spanish] {
                    assert_ne!(
                        raf_core::i18n::t(key, language),
                        key,
                        "missing inspector copy: {key}"
                    );
                }
            }
            pending.extend(node.children.iter());
        }
        for id in [
            "electronics.inspector.placement.field.0.label",
            "electronics.inspector.placement.field.1.label",
            "electronics.inspector.placement.field.2.label",
            "electronics.inspector.footprint.title",
            "electronics.inspector.identity.value-label",
        ] {
            let node = surface.root.find(id).expect("label");
            assert!(node.text_key.is_some(), "{id} must use an i18n key");
            assert!(node.text_value.is_none(), "{id} must not show a field name");
        }
    }

    #[test]
    fn the_identity_label_of_a_wire_resolves_to_the_net_key() {
        assert_eq!(identity_field_key("Net"), "app.schematic_net");
        assert_eq!(identity_field_key("Value"), "app.value");
    }

    #[test]
    fn tabs_declare_the_tab_role_for_the_shared_arrow_key_order() {
        let surface = surface(InspectorTab::Properties);
        for (id, selected) in [("properties", true), ("appearance", false), ("nets", false)] {
            let tab = surface
                .root
                .find(&format!("electronics.inspector.{id}-tab"))
                .unwrap();
            assert_eq!(tab.accessibility_role, UiAccessibilityRole::Tab);
            assert_eq!(tab.accessibility_selected, Some(selected));
        }
    }

    #[test]
    fn the_value_field_exposes_a_focus_ring_and_a_textbox_role() {
        let surface = surface(InspectorTab::Properties);
        let input = surface
            .root
            .find("electronics.inspector.value")
            .expect("value input");
        assert_eq!(input.accessibility_role, UiAccessibilityRole::Textbox);
        assert!(surface.style_sheet.rules.iter().any(|rule| {
            rule.selector == UiStyleSelector::Class("electronics-inspector-input".to_string())
                && rule.state == raf_ui::UiStyleRuleState::Focused
        }));
    }

    #[test]
    fn the_value_field_describes_apply_and_cancel_gestures() {
        let surface = surface(InspectorTab::Properties);
        let input = surface
            .root
            .find("electronics.inspector.value")
            .expect("value input");
        assert_eq!(
            input.accessibility_description_key.as_deref(),
            Some("electronics.tooltip.inspector.value_commit")
        );
        assert_eq!(
            input.tooltip_key.as_deref(),
            Some("electronics.tooltip.inspector.value_commit")
        );
        let control = input.control.text_input().expect("text input control");
        assert_eq!(
            control.submit_command.as_deref(),
            Some("electronics.inspector.value.commit")
        );
    }

    #[test]
    fn the_inspector_passes_the_retained_layout_gate() {
        // The Sessions tab is composed by the shared Game inspector and is not
        // gated here; this pass owns the Electronics property body.
        assert_electronics_layout_gate(&surface(InspectorTab::Properties), 300, 760);
    }

    #[test]
    fn pcb_inspector_exposes_fabrication_export_and_its_real_status() {
        let surface = build_electronics_inspector_surface_with_fabrication_status(
            StudioUiPalette::IndustrialDark,
            "R1  Resistor",
            &fields(),
            &[("1".to_string(), "N001".to_string())],
            &ProjectSessionRegistry::new(raf_core::project::ProjectType::Electronics),
            InspectorTab::Properties,
            Some("Gerbers listos: project/fabrication/gerbers.zip"),
        );
        let export = surface
            .root
            .find("electronics.inspector.fabrication.export")
            .expect("fabrication export action");
        assert_eq!(export.accessibility_role, UiAccessibilityRole::Button);
        assert!(export.event_handlers.iter().any(|event| {
            matches!(&event.action, raf_ui::UiAction::Command { name }
                if name == "electronics.fabrication.export")
        }));
        let status = surface
            .root
            .find("electronics.inspector.fabrication.status")
            .expect("backend fabrication status");
        assert_eq!(status.accessibility_role, UiAccessibilityRole::Status);
        assert_eq!(
            status.text_value.as_deref(),
            Some("Gerbers listos: project/fabrication/gerbers.zip")
        );
    }

    #[test]
    fn selected_inspector_cpu_fixture_renders_real_footprint_geometry_and_can_be_captured() {
        use raf_render::api_graphic_basic::ui_surface::CpuUiSurfaceHost;

        // This is a small, test-only KiCad footprint. The preview is generated
        // from the parsed pad geometry, not a generic inspector illustration.
        let asset = raf_electronics::pcb::parse_kicad_footprint(
            r#"(footprint "R_0805_2012Metric"
                (pad "1" smd rect (at -1 0) (size 1 1.4) (layers "F.Cu" "F.Mask"))
                (pad "2" smd roundrect (at 1 0) (size 1 1.4)
                    (roundrect_rratio 0.2) (layers "F.Cu" "F.Mask")))"#,
        )
        .expect("valid test footprint");
        let bounds = asset.bounds_mm().expect("test footprint bounds");
        let preview_key = "electronics://inspector-qa-footprint";
        let presentation = ElectronicsInspectorPresentation {
            footprint_name: Some(asset.name.clone()),
            footprint_dimensions_mm: Some([bounds.width_mm() as f32, bounds.height_mm() as f32]),
            footprint_pad_count: Some(asset.pads.len()),
            footprint_preview_image_key: Some(preview_key.to_string()),
            pin_pad_mapping: vec![
                ("1".to_string(), "1".to_string()),
                ("2".to_string(), "2".to_string()),
            ],
            ..Default::default()
        };
        let surface = build_electronics_inspector_surface_with_presentation(
            StudioUiPalette::IndustrialDark,
            "R1  Resistor",
            &fields(),
            &[
                ("1".to_string(), "GND".to_string()),
                ("2".to_string(), "VCC".to_string()),
            ],
            &ProjectSessionRegistry::new(raf_core::project::ProjectType::Electronics),
            InspectorTab::Properties,
            None,
            Some(&presentation),
        );
        assert_electronics_layout_gate(&surface, 300, 760);
        let nets = build_electronics_inspector_surface_with_page(
            StudioUiPalette::IndustrialDark,
            "R1",
            &fields(),
            &[("1".into(), "GND".into()), ("2".into(), "VCC".into())],
            &ProjectSessionRegistry::new(raf_core::project::ProjectType::Electronics),
            "nets",
            None,
            Some(&presentation),
        );
        assert_eq!(
            nets.root
                .find("electronics.inspector.pin.0.arrow")
                .and_then(|n| n.text_value.as_deref()),
            Some(">")
        );
        assert!(nets.root.find("electronics.inspector.pin.0.pad").is_some());
        let clear = [12, 15, 21, 255];
        let mut host = CpuUiSurfaceHost::new(surface, clear);
        let (preview_size, preview_pixels) = crate::electronics_minimap::footprint_preview_rgba(
            &asset,
            StudioUiPalette::IndustrialDark,
        );
        host.images_mut()
            .insert_rgba(preview_key, preview_size, preview_pixels)
            .expect("register geometry-derived preview pixels");
        let frame = host.render([300, 760], |key| {
            raf_core::i18n::t(key, raf_core::Language::Spanish)
        });

        for (id, minimum_width) in [
            ("electronics.inspector.selected.title", 80.0),
            ("electronics.inspector.footprint.value", 80.0),
            ("electronics.inspector.footprint.dimensions.value", 55.0),
            ("electronics.inspector.footprint.preview", 100.0),
        ] {
            let node = frame
                .frame
                .layout_boxes
                .iter()
                .find(|node| node.id == id)
                .unwrap_or_else(|| panic!("visible selected inspector node {id}"));
            assert!(node.rect.width >= minimum_width, "{id}: {:?}", node.rect);
            assert!(
                node.rect.x >= 0.0 && node.rect.right() <= 300.1,
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
                dir.join("electronics-inspector-selected.png"),
                frame.pixels,
                frame.size[0],
                frame.size[1],
                image::ColorType::Rgba8,
            )
            .expect("save optional selected inspector CPU capture");
        }
    }

    #[test]
    fn legacy_inspector_builder_does_not_fabricate_footprint_preview_or_mapping() {
        let surface = surface(InspectorTab::Properties);
        assert!(surface
            .root
            .find("electronics.inspector.footprint.preview")
            .is_none());
        assert!(surface
            .root
            .find("electronics.inspector.pin.0.pad")
            .is_none());
        assert!(
            surface.root.find("electronics.inspector.pins").is_none(),
            "Connections belong to the Nets page"
        );
    }

    #[test]
    fn component_lock_carries_the_original_surface_and_identity() {
        let mut fields = fields();
        fields.push((
            "Transform target".into(),
            "pcb:00000000-0000-0000-0000-000000000001".into(),
        ));
        let surface = build_electronics_inspector_surface(
            StudioUiPalette::IndustrialDark,
            "R1",
            &fields,
            &[],
            &ProjectSessionRegistry::new(raf_core::project::ProjectType::Electronics),
            InspectorTab::Properties,
        );
        let button = surface.root.find("electronics.inspector.pin").unwrap();
        assert!(button.event_handlers.iter().any(|e|matches!(&e.action,raf_ui::UiAction::Command{name} if name=="electronics.component.lock:pcb:00000000-0000-0000-0000-000000000001:true")));
    }

    #[test]
    fn every_inspector_row_shares_the_family_density() {
        let surface = surface(InspectorTab::Properties);
        assert_eq!(
            surface
                .root
                .find("electronics.inspector.placement.field.0")
                .unwrap()
                .layout
                .basis[1],
            ELECTRONICS_ROW_HEIGHT
        );
        assert_eq!(
            surface
                .root
                .find("electronics.inspector.value")
                .unwrap()
                .layout
                .basis[1],
            ELECTRONICS_CONTROL_HEIGHT
        );
        assert_eq!(FIELD_ROW_HEIGHT, ELECTRONICS_ROW_HEIGHT);
    }

    #[test]
    fn a_locked_selection_replaces_the_field_with_a_read_only_value() {
        let mut fields = fields();
        fields.retain(|(name, _)| name != "Editable");
        let surface = build_electronics_inspector_surface(
            StudioUiPalette::IndustrialDark,
            "R1",
            &fields,
            &[],
            &ProjectSessionRegistry::new(raf_core::project::ProjectType::Electronics),
            InspectorTab::Properties,
        );
        assert!(surface.root.find("electronics.inspector.value").is_none());
        assert!(surface
            .root
            .find("electronics.inspector.identity.value")
            .is_some());
    }

    #[test]
    fn a_property_value_never_has_to_be_guessed_as_a_key() {
        let surface = surface(InspectorTab::Properties);
        let state = surface
            .root
            .find("electronics.inspector.placement.field.2.value")
            .expect("state value");
        assert_eq!(state.text_key.as_deref(), Some("app.electronics_editable"));
        assert!(state.text_value.is_none());

        let position = surface
            .root
            .find("electronics.inspector.placement.field.0.value")
            .expect("position value");
        assert_eq!(position.text_value.as_deref(), Some("12.0, 4.0"));
        assert!(position.text_key.is_none());
    }

    #[test]
    fn both_themes_keep_the_same_section_hierarchy() {
        for palette in [StudioUiPalette::IndustrialDark, StudioUiPalette::PaperLight] {
            let tokens: UiTokens = crate::panels::electronics_surface::electronics_tokens(palette);
            let surface = build_electronics_inspector_surface(
                palette,
                "R1",
                &fields(),
                &[],
                &ProjectSessionRegistry::new(raf_core::project::ProjectType::Electronics),
                InspectorTab::Properties,
            );
            // The class is the shared section recipe; the id is what the
            // document actually carries.
            let section = surface
                .root
                .find("electronics.inspector.footprint")
                .expect("section");
            assert_eq!(section.classes, vec!["electronics-inspector-section"]);
            let rule = surface
                .style_sheet
                .rules
                .iter()
                .find(|rule| {
                    rule.selector == UiStyleSelector::Class("electronics-inspector-section".into())
                })
                .expect("section rule");
            // Flat sections may expose the themed inspector background.
            let fill = rule.patch.fill.expect("section fill");
            assert!(
                fill == tokens.surface || fill == tokens.surface_alt || fill == [0, 0, 0, 0],
                "section fill {fill:?} is not a palette surface token"
            );
        }
    }
}
